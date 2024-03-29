//! One of the APU's square wave generator channels, which generates a square wave with a
//! configurable duty cycle and period. These channels also have sweep units which can automatically
//! increase or decrease the period over time.
//!
//! Channel output values are between 0 and 15 (inclusive).
//!
//! This code is also used in the MMC5 mapper's expansion audio implementation.

use crate::apu::units::{Envelope, LengthCounter, LengthCounterChannel, PhaseTimer};
use crate::num::GetBit;
use bincode::{Decode, Encode};

type PulsePhaseTimer = PhaseTimer<8, 2, 11, true>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum DutyCycle {
    OneEighth,
    OneFourth,
    OneHalf,
    ThreeFourths,
}

impl DutyCycle {
    fn from_vol(vol_value: u8) -> Self {
        match vol_value & 0xC0 {
            0x00 => Self::OneEighth,
            0x40 => Self::OneFourth,
            0x80 => Self::OneHalf,
            0xC0 => Self::ThreeFourths,
            _ => unreachable!("{vol_value} & 0xC0 was not 0x00/0x40/0x80/0xC0"),
        }
    }

    fn waveform(self) -> [u8; 8] {
        match self {
            Self::OneEighth => [0, 1, 0, 0, 0, 0, 0, 0],
            Self::OneFourth => [0, 1, 1, 0, 0, 0, 0, 0],
            Self::OneHalf => [0, 1, 1, 1, 1, 0, 0, 0],
            Self::ThreeFourths => [1, 0, 0, 1, 1, 1, 1, 1],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum SweepNegateBehavior {
    OnesComplement,
    TwosComplement,
}

impl SweepNegateBehavior {
    fn negate(self, value: u16) -> u16 {
        match self {
            Self::OnesComplement => !value,
            Self::TwosComplement => (!value).wrapping_add(1),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
struct PulseSweep {
    enabled: bool,
    divider: u8,
    divider_period: u8,
    negate_flag: bool,
    negate_behavior: SweepNegateBehavior,
    shift: u8,
    reload_flag: bool,
    target_period: u16,
}

impl PulseSweep {
    fn new(negate_behavior: SweepNegateBehavior) -> Self {
        Self {
            enabled: false,
            divider: 0,
            divider_period: 0,
            negate_flag: false,
            negate_behavior,
            shift: 0,
            reload_flag: false,
            target_period: 0,
        }
    }

    fn process_sweep_update(&mut self, sweep_value: u8, timer_period: u16) {
        self.reload_flag = true;

        self.enabled = sweep_value.bit(7);
        self.divider_period = (sweep_value >> 4) & 0x07;
        self.negate_flag = sweep_value.bit(3);
        self.shift = sweep_value & 0x07;

        self.target_period = self.compute_target_period(timer_period);
    }

    fn process_timer_period_update(&mut self, timer_period: u16) {
        self.target_period = self.compute_target_period(timer_period);
    }

    fn compute_target_period(&self, timer_period: u16) -> u16 {
        if self.shift == 0 && self.negate_flag {
            // Always return 0 when negate is set and shift is 0.
            // Not doing this will cause channel 1 to incorrectly silence during some games because
            // of its weird one's complement behavior.
            return 0;
        }

        let delta = timer_period >> self.shift;
        let signed_delta =
            if self.negate_flag { self.negate_behavior.negate(delta) } else { delta };

        timer_period.wrapping_add(signed_delta)
    }

    fn is_channel_muted(&self, timer_period: u16) -> bool {
        timer_period < 8 || self.target_period > 0x07FF
    }

    fn clock(&mut self, timer_period: &mut u16) {
        if self.divider == 0
            && self.enabled
            && self.shift > 0
            && !self.is_channel_muted(*timer_period)
        {
            *timer_period = self.target_period;
            self.target_period = self.compute_target_period(self.target_period);
        }

        if self.divider == 0 || self.reload_flag {
            self.divider = self.divider_period;
            self.reload_flag = false;
        } else {
            self.divider -= 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum SweepStatus {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct PulseChannel {
    timer: PulsePhaseTimer,
    duty_cycle: DutyCycle,
    length_counter: LengthCounter,
    envelope: Envelope,
    sweep: PulseSweep,
    sweep_status: SweepStatus,
}

impl PulseChannel {
    pub fn new_channel_1(sweep_status: SweepStatus) -> Self {
        Self {
            timer: PulsePhaseTimer::new(),
            duty_cycle: DutyCycle::OneEighth,
            length_counter: LengthCounter::new(LengthCounterChannel::Pulse1),
            envelope: Envelope::new(),
            sweep: PulseSweep::new(SweepNegateBehavior::OnesComplement),
            sweep_status,
        }
    }

    pub fn new_channel_2(sweep_status: SweepStatus) -> Self {
        Self {
            timer: PulsePhaseTimer::new(),
            duty_cycle: DutyCycle::OneEighth,
            length_counter: LengthCounter::new(LengthCounterChannel::Pulse2),
            envelope: Envelope::new(),
            sweep: PulseSweep::new(SweepNegateBehavior::TwosComplement),
            sweep_status,
        }
    }

    pub fn process_vol_update(&mut self, vol_value: u8) {
        self.duty_cycle = DutyCycle::from_vol(vol_value);
        self.length_counter.process_vol_update(vol_value);
        self.envelope.process_vol_update(vol_value);
    }

    pub fn process_sweep_update(&mut self, sweep_value: u8) {
        self.sweep.process_sweep_update(sweep_value, self.timer.divider_period);
    }

    pub fn process_lo_update(&mut self, lo_value: u8) {
        self.timer.process_lo_update(lo_value);
        self.sweep.process_timer_period_update(self.timer.divider_period);
    }

    pub fn process_hi_update(&mut self, hi_value: u8) {
        self.timer.process_hi_update(hi_value);
        self.sweep.process_timer_period_update(self.timer.divider_period);

        self.length_counter.process_hi_update(hi_value);
        self.envelope.process_hi_update();
    }

    pub fn process_snd_chn_update(&mut self, snd_chn_value: u8) {
        self.length_counter.process_snd_chn_update(snd_chn_value);
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }

    pub fn clock_half_frame(&mut self) {
        self.length_counter.clock();

        if self.sweep_status == SweepStatus::Enabled {
            self.sweep.clock(&mut self.timer.divider_period);
        }
    }

    pub fn tick_cpu(&mut self) {
        self.timer.tick(true);
    }

    pub fn sample(&self) -> u8 {
        if self.length_counter.counter == 0
            || (self.sweep_status == SweepStatus::Enabled
                && self.sweep.is_channel_muted(self.timer.divider_period))
        {
            return 0;
        }

        let wave_step = self.duty_cycle.waveform()[self.timer.phase as usize];
        wave_step * self.envelope.volume()
    }

    pub fn length_counter(&self) -> u8 {
        self.length_counter.counter
    }
}
