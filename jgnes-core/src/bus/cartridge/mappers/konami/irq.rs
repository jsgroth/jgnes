//! Code for the IRQ counter that is used in the VRC4/VRC6/VRC7 boards. Behavior is identical on
//! each board.

use crate::num::GetBit;
use bincode::{Decode, Encode};

const PRESCALER_SEQUENCE: [u8; 3] = [114, 114, 113];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum IrqMode {
    Scanline,
    Cycle,
}

#[derive(Debug, Clone, Encode, Decode)]
pub(crate) struct VrcIrqCounter {
    irq_counter: u8,
    prescaler_counter: u8,
    prescaler_seq_index: usize,
    enabled: bool,
    pending: bool,
    mode: IrqMode,
    reload_value: u8,
    enable_after_ack: bool,
}

impl VrcIrqCounter {
    pub(crate) fn new() -> Self {
        Self {
            irq_counter: 0,
            prescaler_counter: 0,
            prescaler_seq_index: 0,
            enabled: false,
            pending: false,
            mode: IrqMode::Scanline,
            reload_value: 0,
            enable_after_ack: false,
        }
    }

    pub(crate) fn set_reload_value(&mut self, value: u8) {
        self.reload_value = value;
    }

    pub(crate) fn set_reload_value_low_4_bits(&mut self, value: u8) {
        assert_eq!(value & 0xF0, 0);

        self.reload_value = (self.reload_value & 0xF0) | (value & 0x0F);
    }

    pub(crate) fn set_reload_value_high_4_bits(&mut self, value: u8) {
        assert_eq!(value & 0xF0, 0);

        self.reload_value = (self.reload_value & 0x0F) | (value << 4);
    }

    pub(crate) fn set_control(&mut self, value: u8) {
        self.pending = false;
        self.reset_prescaler();

        self.enable_after_ack = value.bit(0);
        self.enabled = value.bit(1);
        self.mode = if value.bit(2) { IrqMode::Cycle } else { IrqMode::Scanline };

        if self.enabled {
            self.irq_counter = self.reload_value;
        }
    }

    pub(crate) fn acknowledge(&mut self) {
        self.pending = false;
        self.enabled = self.enable_after_ack;
    }

    pub(crate) fn tick_cpu(&mut self) {
        if !self.enabled {
            return;
        }

        match self.mode {
            IrqMode::Scanline => {
                // Scanline mode uses a prescaler that approximates a 113.666~ divider
                self.prescaler_counter += 1;
                if self.prescaler_counter == PRESCALER_SEQUENCE[self.prescaler_seq_index] {
                    self.clock_irq();

                    self.prescaler_counter = 0;
                    self.prescaler_seq_index =
                        (self.prescaler_seq_index + 1) % PRESCALER_SEQUENCE.len();
                }
            }
            IrqMode::Cycle => {
                // Cycle mode clocks IRQ on every CPU cycle
                self.clock_irq();
            }
        }
    }

    fn clock_irq(&mut self) {
        if self.irq_counter == u8::MAX {
            self.irq_counter = self.reload_value;
            self.pending = true;
        } else {
            self.irq_counter += 1;
        }
    }

    pub(crate) fn interrupt_flag(&self) -> bool {
        self.pending
    }

    fn reset_prescaler(&mut self) {
        self.prescaler_counter = 0;
        self.prescaler_seq_index = 0;
    }
}
