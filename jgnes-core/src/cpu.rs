use crate::bus;
use crate::bus::{Bus, CpuBus};
use crate::cpu::instructions::ExecutingInstruction;

mod instructions;

#[derive(Debug)]
pub struct CpuRegisters {
    pub accumulator: u8,
    pub x: u8,
    pub y: u8,
    pub status: u8,
    pub pc: u16,
    pub sp: u8,
}

impl CpuRegisters {
    pub fn new(bus: &mut CpuBus<'_>) -> Self {
        let pc_lsb = bus.read_address(bus::CPU_RESET_VECTOR);
        let pc_msb = bus.read_address(bus::CPU_RESET_VECTOR + 1);
        let pc = u16::from_le_bytes([pc_lsb, pc_msb]);

        Self {
            accumulator: 0,
            x: 0,
            y: 0,
            status: 0x34,
            pc,
            sp: 0xFD,
        }
    }

    fn status_flags(&mut self) -> StatusFlags<'_> {
        StatusFlags(&mut self.status)
    }
}

pub struct StatusFlags<'a>(&'a mut u8);

impl<'a> StatusFlags<'a> {
    fn negative(&self) -> bool {
        *self.0 & 0x80 != 0
    }

    fn set_negative(&mut self, value: bool) -> &mut Self {
        if value {
            *self.0 |= 0x80;
        } else {
            *self.0 &= !0x80;
        }
        self
    }

    fn overflow(&self) -> bool {
        *self.0 & 0x40 != 0
    }

    fn set_overflow(&mut self, value: bool) -> &mut Self {
        if value {
            *self.0 |= 0x40;
        } else {
            *self.0 &= !0x40;
        }
        self
    }

    fn set_decimal(&mut self, value: bool) -> &mut Self {
        if value {
            *self.0 |= 0x08;
        } else {
            *self.0 &= !0x08;
        }
        self
    }

    fn interrupt_disable(&self) -> bool {
        *self.0 & 0x04 != 0
    }

    fn set_interrupt_disable(&mut self, value: bool) -> &mut Self {
        if value {
            *self.0 |= 0x04;
        } else {
            *self.0 &= !0x04;
        }
        self
    }

    fn zero(&self) -> bool {
        *self.0 & 0x02 != 0
    }

    fn set_zero(&mut self, value: bool) -> &mut Self {
        if value {
            *self.0 |= 0x02;
        } else {
            *self.0 &= !0x02;
        }
        self
    }

    fn carry(&self) -> bool {
        *self.0 & 0x01 != 0
    }

    fn set_carry(&mut self, value: bool) -> &mut Self {
        if value {
            *self.0 |= 0x01;
        } else {
            *self.0 &= !0x01;
        }
        self
    }
}

enum State {
    InstructionStart,
    InstructionExecuting(ExecutingInstruction),
}

pub struct CpuState {
    registers: CpuRegisters,
    state: State,
}

impl CpuState {
    pub fn new(registers: CpuRegisters) -> Self {
        Self {
            registers,
            state: State::InstructionStart,
        }
    }
}

pub fn tick(state: &mut CpuState, bus: &mut Bus) {
    // TODO interrupts

    let new_state = match std::mem::replace(&mut state.state, State::InstructionStart) {
        State::InstructionStart => {
            let executing_instruction =
                ExecutingInstruction::fetch(&mut state.registers, &mut bus.cpu());
            println!("Fetched {executing_instruction:?}");
            State::InstructionExecuting(executing_instruction)
        }
        State::InstructionExecuting(executing_instruction) => {
            match executing_instruction.next(&mut state.registers, &mut bus.cpu()) {
                Some(executing_instruction) => State::InstructionExecuting(executing_instruction),
                None => State::InstructionStart,
            }
        }
    };
    state.state = new_state;
}
