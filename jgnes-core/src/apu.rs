mod dmc;
mod noise;
pub mod pulse;
mod triangle;
pub mod units;

use crate::apu::dmc::DeltaModulationChannel;
use crate::apu::noise::NoiseChannel;
use crate::apu::pulse::{PulseChannel, SweepStatus};
use crate::apu::triangle::TriangleChannel;
use crate::bus::{CpuBus, IoRegister, IrqSource};
use crate::num::GetBit;
use bincode::{Decode, Encode};
use once_cell::sync::Lazy;
use std::iter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum FrameCounterMode {
    FourStep,
    FiveStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum FrameCounterResetState {
    Joy2Updated,
    PendingReset,
    JustReset,
    None,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct FrameCounter {
    cpu_ticks: u16,
    mode: FrameCounterMode,
    interrupt_inhibit_flag: bool,
    reset_state: FrameCounterResetState,
}

impl FrameCounter {
    pub fn new() -> Self {
        Self {
            cpu_ticks: 0,
            mode: FrameCounterMode::FourStep,
            interrupt_inhibit_flag: false,
            reset_state: FrameCounterResetState::None,
        }
    }

    fn process_joy2_update(&mut self, joy2_value: u8) {
        self.mode = if joy2_value.bit(7) {
            FrameCounterMode::FiveStep
        } else {
            FrameCounterMode::FourStep
        };
        self.interrupt_inhibit_flag = joy2_value.bit(6);

        self.reset_state = FrameCounterResetState::Joy2Updated;
    }

    pub fn tick(&mut self) {
        if self.reset_state == FrameCounterResetState::JustReset {
            self.reset_state = FrameCounterResetState::None;
        }

        if (self.cpu_ticks == 29830 && self.mode == FrameCounterMode::FourStep)
            || self.cpu_ticks == 37282
        {
            self.cpu_ticks = 1;
        } else {
            self.cpu_ticks += 1;
        }

        if !self.cpu_ticks.bit(0) {
            match self.reset_state {
                FrameCounterResetState::Joy2Updated => {
                    self.reset_state = FrameCounterResetState::PendingReset;
                }
                FrameCounterResetState::PendingReset => {
                    self.cpu_ticks = 0;
                    self.reset_state = FrameCounterResetState::JustReset;
                }
                _ => {}
            }
        }
    }

    pub fn generate_quarter_frame_clock(&self) -> bool {
        (self.cpu_ticks == 7456
            || self.cpu_ticks == 14912
            || self.cpu_ticks == 22370
            || (self.cpu_ticks == 29828 && self.mode == FrameCounterMode::FourStep)
            || self.cpu_ticks == 37280)
            || (self.reset_state == FrameCounterResetState::JustReset
                && self.mode == FrameCounterMode::FiveStep)
    }

    pub fn generate_half_frame_clock(&self) -> bool {
        (self.cpu_ticks == 14912
            || (self.cpu_ticks == 29828 && self.mode == FrameCounterMode::FourStep)
            || self.cpu_ticks == 37280)
            || (self.reset_state == FrameCounterResetState::JustReset
                && self.mode == FrameCounterMode::FiveStep)
    }

    fn should_set_interrupt_flag(&self) -> bool {
        !self.interrupt_inhibit_flag
            && self.mode == FrameCounterMode::FourStep
            && (29827..29830).contains(&self.cpu_ticks)
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct ApuState {
    pulse_channel_1: PulseChannel,
    pulse_channel_2: PulseChannel,
    triangle_channel: TriangleChannel,
    noise_channel: NoiseChannel,
    dmc: DeltaModulationChannel,
    frame_counter: FrameCounter,
    frame_counter_interrupt_flag: bool,
    hpf_capacitor: f64,
}

// Formulas from https://www.nesdev.org/wiki/APU_Mixer
static PULSE_AUDIO_LOOKUP_TABLE: Lazy<[[f64; 16]; 16]> = Lazy::new(|| {
    let mut lookup_table = [[0.0; 16]; 16];

    for (pulse1_sample, row) in lookup_table.iter_mut().enumerate() {
        for (pulse2_sample, value) in row.iter_mut().enumerate() {
            if pulse1_sample > 0 || pulse2_sample > 0 {
                *value = 95.88 / (8128.0 / (pulse1_sample + pulse2_sample) as f64 + 100.0);
            }
        }
    }

    lookup_table
});

static TND_AUDIO_LOOKUP_TABLE: Lazy<[[[f64; 128]; 16]; 16]> = Lazy::new(|| {
    let mut lookup_table = [[[0.0; 128]; 16]; 16];

    for (triangle_sample, triangle_row) in lookup_table.iter_mut().enumerate() {
        for (noise_sample, noise_row) in triangle_row.iter_mut().enumerate() {
            for (dmc_sample, value) in noise_row.iter_mut().enumerate() {
                if triangle_sample > 0 || noise_sample > 0 || dmc_sample > 0 {
                    *value = 159.79
                        / (1.0
                            / (triangle_sample as f64 / 8227.0
                                + noise_sample as f64 / 12241.0
                                + dmc_sample as f64 / 22638.0)
                            + 100.0);
                }
            }
        }
    }

    lookup_table
});

impl ApuState {
    pub fn new() -> Self {
        Self {
            pulse_channel_1: PulseChannel::new_channel_1(SweepStatus::Enabled),
            pulse_channel_2: PulseChannel::new_channel_2(SweepStatus::Enabled),
            triangle_channel: TriangleChannel::new(),
            noise_channel: NoiseChannel::new(),
            dmc: DeltaModulationChannel::new(),
            frame_counter: FrameCounter::new(),
            frame_counter_interrupt_flag: false,
            hpf_capacitor: 0.0,
        }
    }

    pub fn is_active_cycle(&self) -> bool {
        self.frame_counter.cpu_ticks.bit(0)
    }

    fn process_register_updates(
        &mut self,
        iter: impl Iterator<Item = (IoRegister, u8)>,
        bus: &mut CpuBus<'_>,
    ) {
        for (register, value) in iter {
            match register {
                IoRegister::SQ1_VOL => {
                    self.pulse_channel_1.process_vol_update(value);
                }
                IoRegister::SQ1_SWEEP => {
                    self.pulse_channel_1.process_sweep_update(value);
                }
                IoRegister::SQ1_LO => {
                    self.pulse_channel_1.process_lo_update(value);
                }
                IoRegister::SQ1_HI => {
                    self.pulse_channel_1.process_hi_update(value);
                }
                IoRegister::SQ2_VOL => {
                    self.pulse_channel_2.process_vol_update(value);
                }
                IoRegister::SQ2_SWEEP => {
                    self.pulse_channel_2.process_sweep_update(value);
                }
                IoRegister::SQ2_LO => {
                    self.pulse_channel_2.process_lo_update(value);
                }
                IoRegister::SQ2_HI => {
                    self.pulse_channel_2.process_hi_update(value);
                }
                IoRegister::TRI_LINEAR => {
                    self.triangle_channel.process_tri_linear_update(value);
                }
                IoRegister::TRI_LO => {
                    self.triangle_channel.process_lo_update(value);
                }
                IoRegister::TRI_HI => {
                    self.triangle_channel.process_hi_update(value);
                }
                IoRegister::NOISE_VOL => {
                    self.noise_channel.process_vol_update(value);
                }
                IoRegister::NOISE_LO => {
                    self.noise_channel.process_lo_update(value);
                }
                IoRegister::NOISE_HI => {
                    self.noise_channel.process_hi_update(value);
                }
                IoRegister::DMC_FREQ => {
                    self.dmc.process_dmc_freq_update(value);
                }
                IoRegister::DMC_RAW => {
                    self.dmc.process_dmc_raw_update(value);
                }
                IoRegister::DMC_START => {
                    self.dmc.process_dmc_start_update(value);
                }
                IoRegister::DMC_LEN => {
                    self.dmc.process_dmc_len_update(value);
                }
                IoRegister::SND_CHN => {
                    self.pulse_channel_1.process_snd_chn_update(value);
                    self.pulse_channel_2.process_snd_chn_update(value);
                    self.triangle_channel.process_snd_chn_update(value);
                    self.noise_channel.process_snd_chn_update(value);
                    self.dmc.process_snd_chn_update(value, bus);
                }
                IoRegister::JOY2 => {
                    self.frame_counter.process_joy2_update(value);
                }
                _ => {}
            }
        }
    }

    fn tick_cpu(&mut self, bus: &mut CpuBus<'_>) {
        self.pulse_channel_1.tick_cpu();
        self.pulse_channel_2.tick_cpu();
        self.triangle_channel.tick_cpu();
        self.noise_channel.tick_cpu();
        self.dmc.tick_cpu(bus);
        self.frame_counter.tick();

        if self.frame_counter.generate_quarter_frame_clock() {
            self.pulse_channel_1.clock_quarter_frame();
            self.pulse_channel_2.clock_quarter_frame();
            self.triangle_channel.clock_quarter_frame();
            self.noise_channel.clock_quarter_frame();
        }

        if self.frame_counter.generate_half_frame_clock() {
            self.pulse_channel_1.clock_half_frame();
            self.pulse_channel_2.clock_half_frame();
            self.triangle_channel.clock_half_frame();
            self.noise_channel.clock_half_frame();
        }

        if self.frame_counter.should_set_interrupt_flag() {
            self.frame_counter_interrupt_flag = true;
        } else if self.frame_counter.interrupt_inhibit_flag {
            self.frame_counter_interrupt_flag = false;
        }

        bus.interrupt_lines().set_irq_low_pull(
            IrqSource::ApuFrameCounter,
            self.frame_counter_interrupt_flag,
        );

        bus.interrupt_lines()
            .set_irq_low_pull(IrqSource::ApuDmc, self.dmc.interrupt_flag());
    }

    fn get_apu_status(&self) -> u8 {
        (u8::from(self.dmc.interrupt_flag()) << 7)
            | (u8::from(self.frame_counter_interrupt_flag) << 6)
            | (u8::from(self.dmc.sample_bytes_remaining() > 0) << 4)
            | (u8::from(self.noise_channel.length_counter() > 0) << 3)
            | (u8::from(self.triangle_channel.length_counter() > 0) << 2)
            | (u8::from(self.pulse_channel_2.length_counter() > 0) << 1)
            | u8::from(self.pulse_channel_1.length_counter() > 0)
    }

    fn mix_samples(&self) -> f64 {
        let pulse1_sample = self.pulse_channel_1.sample();
        let pulse2_sample = self.pulse_channel_2.sample();
        let triangle_sample = self.triangle_channel.sample();
        let noise_sample = self.noise_channel.sample();
        let dmc_sample = self.dmc.sample();

        let pulse_mix = mix_pulse_samples(pulse1_sample, pulse2_sample);
        let tnd_mix = TND_AUDIO_LOOKUP_TABLE[triangle_sample as usize][noise_sample as usize]
            [dmc_sample as usize];

        pulse_mix + tnd_mix
    }

    pub fn high_pass_filter(&mut self, sample: f64) -> f64 {
        let filtered_sample = sample - self.hpf_capacitor;

        // TODO figure out something better to do than copy-pasting what I did for the Game Boy
        self.hpf_capacitor = sample - 0.9999015765 * filtered_sample;

        filtered_sample
    }

    pub fn sample(&self) -> f64 {
        self.mix_samples()
    }
}

pub fn mix_pulse_samples(pulse1_sample: u8, pulse2_sample: u8) -> f64 {
    assert!((0..=15).contains(&pulse1_sample) && (0..=15).contains(&pulse2_sample));

    PULSE_AUDIO_LOOKUP_TABLE[pulse1_sample as usize][pulse2_sample as usize]
}

pub fn tick(state: &mut ApuState, bus: &mut CpuBus<'_>) {
    log::trace!("APU: Frame counter state: {:?}", state.frame_counter);
    log::trace!("APU: Pulse 1 state: {:?}", state.pulse_channel_1);
    log::trace!("APU: Pulse 2 state: {:?}", state.pulse_channel_2);
    log::trace!("APU: DMC state: {:?}", state.dmc);

    if bus.get_io_registers_mut().get_and_clear_snd_chn_read() {
        state.frame_counter_interrupt_flag = false;
    }

    let dirty_registers: Vec<_> = bus.get_io_registers_mut().drain_dirty_registers().collect();
    state.process_register_updates(dirty_registers.into_iter(), bus);

    state.tick_cpu(bus);

    bus.get_io_registers_mut()
        .set_apu_status(state.get_apu_status());
    log::trace!("APU: Status set to {:02X}", state.get_apu_status());
}

pub fn reset(state: &mut ApuState, bus: &mut CpuBus<'_>) {
    // Silence the APU by simulating a SND_CHN=$00 write
    state.process_register_updates(iter::once((IoRegister::SND_CHN, 0x00)), bus);

    state.frame_counter.reset_state = FrameCounterResetState::Joy2Updated;
    state.frame_counter_interrupt_flag = false;

    state.triangle_channel.reset();
    state.dmc.reset();
}
