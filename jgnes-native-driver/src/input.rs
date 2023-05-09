use crate::{AxisDirection, HatDirection, InputConfig, InputConfigBase, JoystickInput};
use jgnes_core::JoypadState;
use sdl2::event::Event;
use sdl2::joystick::{HatState, Joystick};
use sdl2::keyboard::Keycode;
use sdl2::JoystickSubsystem;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use tinyvec::ArrayVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Up,
    Left,
    Right,
    Down,
    A,
    B,
    Start,
    Select,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Player {
    Player1,
    Player2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Input {
    Keyboard(Keycode),
    Joystick(JoystickInput),
}

pub(crate) struct SdlInputHandler<'a> {
    raw_p1_joypad_state: JoypadState,
    p1_joypad_state: Rc<RefCell<JoypadState>>,
    raw_p2_joypad_state: JoypadState,
    p2_joypad_state: Rc<RefCell<JoypadState>>,
    keyboard_input_mapping: HashMap<Keycode, Vec<(Player, Button)>>,
    joystick_input_mapping: HashMap<JoystickInput, Vec<(Player, Button)>>,
    axis_deadzone: u16,
    allow_opposite_directions: bool,
    joystick_subsystem: &'a JoystickSubsystem,
    joysticks: HashMap<u32, Joystick>,
    instance_id_to_device_id: HashMap<u32, u32>,
}

impl<'a> SdlInputHandler<'a> {
    pub(crate) fn new(
        joystick_subsystem: &'a JoystickSubsystem,
        input_config: &InputConfig,
        p1_joypad_state: Rc<RefCell<JoypadState>>,
        p2_joypad_state: Rc<RefCell<JoypadState>>,
    ) -> Self {
        let mut keyboard_input_mapping = HashMap::new();
        populate_map(
            &mut keyboard_input_mapping,
            &input_config.p1.keyboard.to_keycode_config(),
            Player::Player1,
        );
        populate_map(
            &mut keyboard_input_mapping,
            &input_config.p2.keyboard.to_keycode_config(),
            Player::Player2,
        );

        let mut joystick_input_mapping = HashMap::new();
        populate_map(
            &mut joystick_input_mapping,
            &input_config.p1.joystick,
            Player::Player1,
        );
        populate_map(
            &mut joystick_input_mapping,
            &input_config.p2.joystick,
            Player::Player2,
        );

        Self {
            raw_p1_joypad_state: JoypadState::new(),
            p1_joypad_state,
            raw_p2_joypad_state: JoypadState::new(),
            p2_joypad_state,
            keyboard_input_mapping,
            joystick_input_mapping,
            axis_deadzone: input_config.axis_deadzone,
            allow_opposite_directions: input_config.allow_opposite_directions,
            joystick_subsystem,
            joysticks: HashMap::new(),
            instance_id_to_device_id: HashMap::new(),
        }
    }

    pub(crate) fn handle_event(&mut self, event: &Event) -> Result<(), anyhow::Error> {
        match *event {
            Event::KeyDown {
                keycode: Some(keycode),
                ..
            } => {
                self.update_joypad_state(Input::Keyboard(keycode), true);
            }
            Event::KeyUp {
                keycode: Some(keycode),
                ..
            } => {
                self.update_joypad_state(Input::Keyboard(keycode), false);
            }
            Event::JoyDeviceAdded {
                which: device_id, ..
            } => {
                let joystick = self.joystick_subsystem.open(device_id)?;
                let instance_id = joystick.instance_id();
                log::info!(
                    "Opened joystick device id {device_id} with instance id {instance_id}: {} ({})",
                    joystick.name(),
                    joystick.guid()
                );
                self.joysticks.insert(device_id, joystick);
                self.instance_id_to_device_id.insert(instance_id, device_id);
            }
            Event::JoyDeviceRemoved {
                which: instance_id, ..
            } => {
                if let Some(device_id) = self.instance_id_to_device_id.remove(&instance_id) {
                    if let Some(removed) = self.joysticks.remove(&device_id) {
                        log::info!(
                            "Joystick {device_id} removed (instance id {instance_id}): {}",
                            removed.name()
                        );
                    }
                }
            }
            Event::JoyButtonDown {
                which: instance_id,
                button_idx,
                ..
            } => {
                if let Some(&device_id) = self.instance_id_to_device_id.get(&instance_id) {
                    let input = JoystickInput::Button {
                        device_id,
                        button_idx,
                    };
                    self.update_joypad_state(Input::Joystick(input), true);
                }
            }
            Event::JoyButtonUp {
                which: instance_id,
                button_idx,
                ..
            } => {
                if let Some(&device_id) = self.instance_id_to_device_id.get(&instance_id) {
                    let input = JoystickInput::Button {
                        device_id,
                        button_idx,
                    };
                    self.update_joypad_state(Input::Joystick(input), false);
                }
            }
            Event::JoyAxisMotion {
                which: instance_id,
                axis_idx,
                value,
                ..
            } => {
                if let Some(&device_id) = self.instance_id_to_device_id.get(&instance_id) {
                    let positive = JoystickInput::Axis {
                        device_id,
                        axis_idx,
                        direction: AxisDirection::Positive,
                    };
                    let negative = JoystickInput::Axis {
                        device_id,
                        axis_idx,
                        direction: AxisDirection::Negative,
                    };
                    if value.saturating_abs() >= self.axis_deadzone as i16 {
                        if value > 0 {
                            self.update_joypad_state(Input::Joystick(positive), true);
                            self.update_joypad_state(Input::Joystick(negative), false);
                        } else {
                            self.update_joypad_state(Input::Joystick(positive), false);
                            self.update_joypad_state(Input::Joystick(negative), true);
                        }
                    } else {
                        self.update_joypad_state(Input::Joystick(positive), false);
                        self.update_joypad_state(Input::Joystick(negative), false);
                    }
                }
            }
            Event::JoyHatMotion {
                which: instance_id,
                hat_idx,
                state,
                ..
            } => {
                if let Some(&device_id) = self.instance_id_to_device_id.get(&instance_id) {
                    for direction in HatDirection::ALL {
                        let input = JoystickInput::Hat {
                            device_id,
                            hat_idx,
                            direction,
                        };
                        self.update_joypad_state(Input::Joystick(input), false);
                    }
                    for direction in hat_directions_for(state) {
                        let input = JoystickInput::Hat {
                            device_id,
                            hat_idx,
                            direction,
                        };
                        self.update_joypad_state(Input::Joystick(input), true);
                    }
                }
            }
            _ => {}
        }

        *self.p1_joypad_state.borrow_mut() =
            sanitize_joypad_state(self.raw_p1_joypad_state, self.allow_opposite_directions);
        *self.p2_joypad_state.borrow_mut() =
            sanitize_joypad_state(self.raw_p2_joypad_state, self.allow_opposite_directions);

        Ok(())
    }

    fn update_joypad_state(&mut self, input: Input, value: bool) {
        let input_mapping = match input {
            Input::Keyboard(keycode) => self.keyboard_input_mapping.get(&keycode),
            Input::Joystick(joystick_input) => self.joystick_input_mapping.get(&joystick_input),
        };

        for (player, button) in input_mapping.unwrap_or(&Vec::new()).iter().copied() {
            let joypad_state = match player {
                Player::Player1 => &mut self.raw_p1_joypad_state,
                Player::Player2 => &mut self.raw_p2_joypad_state,
            };

            let field = match button {
                Button::Up => &mut joypad_state.up,
                Button::Left => &mut joypad_state.left,
                Button::Right => &mut joypad_state.right,
                Button::Down => &mut joypad_state.down,
                Button::A => &mut joypad_state.a,
                Button::B => &mut joypad_state.b,
                Button::Start => &mut joypad_state.start,
                Button::Select => &mut joypad_state.select,
            };

            *field = value;
        }
    }
}

fn populate_map<K>(
    map: &mut HashMap<K, Vec<(Player, Button)>>,
    config: &InputConfigBase<K>,
    player: Player,
) where
    K: Eq + Hash + Copy,
{
    macro_rules! populate_map {
        ($($field:expr => $button:expr),+$(,)?) => {
            {
                $(
                    if let Some(field) = $field {
                        add_to_map(map, field, player, $button);
                    }
                )*
            }
        }
    }

    populate_map!(
        config.up => Button::Up,
        config.left => Button::Left,
        config.right => Button::Right,
        config.down => Button::Down,
        config.a => Button::A,
        config.b => Button::B,
        config.start => Button::Start,
        config.select => Button::Select,
    );
}

fn add_to_map<K>(
    map: &mut HashMap<K, Vec<(Player, Button)>>,
    key: K,
    player: Player,
    button: Button,
) where
    K: Eq + Hash,
{
    map.entry(key)
        .and_modify(|buttons| buttons.push((player, button)))
        .or_insert(vec![(player, button)]);
}

fn hat_directions_for(state: HatState) -> ArrayVec<[HatDirection; 2]> {
    match state {
        HatState::Up => [HatDirection::Up].into_iter().collect(),
        HatState::LeftUp => [HatDirection::Left, HatDirection::Up].into_iter().collect(),
        HatState::Left => [HatDirection::Left].into_iter().collect(),
        HatState::LeftDown => [HatDirection::Left, HatDirection::Down]
            .into_iter()
            .collect(),
        HatState::Down => [HatDirection::Down].into_iter().collect(),
        HatState::RightDown => [HatDirection::Right, HatDirection::Down]
            .into_iter()
            .collect(),
        HatState::Right => [HatDirection::Right].into_iter().collect(),
        HatState::RightUp => [HatDirection::Right, HatDirection::Up]
            .into_iter()
            .collect(),
        HatState::Centered => [].into_iter().collect(),
    }
}

#[must_use]
fn sanitize_joypad_state(
    joypad_state: JoypadState,
    allow_opposite_directions: bool,
) -> JoypadState {
    if allow_opposite_directions {
        return joypad_state;
    }

    let mut sanitized = joypad_state;

    if sanitized.up && sanitized.down {
        // Arbitrarily give Up priority
        sanitized.down = false;
    }

    if sanitized.left && sanitized.right {
        // Arbitrarily give Left priority
        sanitized.right = false;
    }

    sanitized
}
