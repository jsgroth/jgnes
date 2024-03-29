//! CPU emulation code.

use crate::bus;
use crate::bus::{CpuBus, PpuRegister};
use crate::cpu::instructions::InstructionState;
use crate::num::GetBit;
use bincode::{Decode, Encode};

mod instructions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum StatusReadContext {
    HardwareInterruptHandler,
    Brk,
    PushStack,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct StatusFlags {
    negative: bool,
    overflow: bool,
    decimal: bool,
    interrupt_disable: bool,
    zero: bool,
    carry: bool,
}

impl StatusFlags {
    pub fn new() -> Self {
        // I flag defaults to 1, others default to 0
        Self {
            negative: false,
            overflow: false,
            decimal: false,
            interrupt_disable: true,
            zero: false,
            carry: false,
        }
    }

    pub fn set_negative(&mut self, negative: bool) -> &mut Self {
        self.negative = negative;
        self
    }

    pub fn set_overflow(&mut self, overflow: bool) -> &mut Self {
        self.overflow = overflow;
        self
    }

    pub fn set_zero(&mut self, zero: bool) -> &mut Self {
        self.zero = zero;
        self
    }

    pub fn set_carry(&mut self, carry: bool) -> &mut Self {
        self.carry = carry;
        self
    }

    pub fn to_byte(self, read_ctx: StatusReadContext) -> u8 {
        // B flag is set during BRK and PHA/PHP, cleared during NMI & IRQ handlers
        let b_flag = match read_ctx {
            StatusReadContext::Brk | StatusReadContext::PushStack => 0x10,
            StatusReadContext::HardwareInterruptHandler => 0x00,
        };

        // Bit 5 is unused, always reads as 1
        (u8::from(self.negative) << 7)
            | (u8::from(self.overflow) << 6)
            | 0x20
            | b_flag
            | (u8::from(self.decimal) << 3)
            | (u8::from(self.interrupt_disable) << 2)
            | (u8::from(self.zero) << 1)
            | u8::from(self.carry)
    }

    pub fn from_byte(byte: u8) -> Self {
        Self {
            negative: byte.bit(7),
            overflow: byte.bit(6),
            decimal: byte.bit(3),
            interrupt_disable: byte.bit(2),
            zero: byte.bit(1),
            carry: byte.bit(0),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CpuRegisters {
    pub accumulator: u8,
    pub x: u8,
    pub y: u8,
    pub status: StatusFlags,
    pub pc: u16,
    pub sp: u8,
}

impl CpuRegisters {
    /// Create a new `CpuRegisters` value with the PC pointing to the address at the cartridge's
    /// RESET vector.
    pub fn create(bus: &mut CpuBus<'_>) -> Self {
        let pc_lsb = bus.read_address(bus::CPU_RESET_VECTOR);
        let pc_msb = bus.read_address(bus::CPU_RESET_VECTOR + 1);
        let pc = u16::from_le_bytes([pc_lsb, pc_msb]);

        Self { accumulator: 0, x: 0, y: 0, status: StatusFlags::new(), pc, sp: 0xFD }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
struct OamDmaState {
    cycles_remaining: u16,
    source_high_byte: u8,
    last_read_value: u8,
    pending_interrupt: bool,
}

#[derive(Debug, Clone, Encode, Decode)]
enum State {
    InstructionStart { pending_interrupt: bool },
    Executing(InstructionState),
    OamDmaDelay(OamDmaState),
    OamDma(OamDmaState),
}

impl State {
    const INITIAL: Self = Self::InstructionStart { pending_interrupt: false };
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CpuState {
    registers: CpuRegisters,
    state: State,
    terminated: bool,
}

impl CpuState {
    pub fn new(registers: CpuRegisters) -> Self {
        Self { registers, state: State::INITIAL, terminated: false }
    }

    #[cfg(test)]
    pub fn at_instruction_start(&self) -> bool {
        matches!(self.state, State::InstructionStart { .. })
    }
}

/// Run the CPU for 1 CPU cycle.
pub fn tick(state: &mut CpuState, bus: &mut CpuBus<'_>, is_apu_active_cycle: bool) {
    if state.terminated {
        return;
    }

    state.state = match std::mem::replace(&mut state.state, State::INITIAL) {
        State::InstructionStart { pending_interrupt } => {
            // Always read opcode, even if it won't be used
            let opcode = bus.read_address(state.registers.pc);

            if bus.is_oamdma_dirty() {
                bus.clear_oamdma_dirty();

                let source_high_byte = bus.read_oamdma_for_transfer();
                log::trace!("OAM: Initiating OAM DMA transfer from {source_high_byte:02X}");

                let oam_dma_state = OamDmaState {
                    cycles_remaining: 512,
                    source_high_byte,
                    last_read_value: 0,
                    pending_interrupt,
                };
                if is_apu_active_cycle {
                    State::OamDmaDelay(oam_dma_state)
                } else {
                    State::OamDma(oam_dma_state)
                }
            } else if pending_interrupt {
                log::trace!("INTERRUPT: Handling hardware NMI/IRQ interrupt");

                let interrupt_state = InstructionState::interrupt_service();

                State::Executing(interrupt_state)
            } else {
                state.registers.pc += 1;

                let instruction_state = InstructionState::from_opcode(opcode);

                State::Executing(instruction_state)
            }
        }
        State::Executing(mut instruction_state) => {
            instructions::execute_cycle(&mut instruction_state, &mut state.registers, bus);
            state.terminated = instruction_state.terminated;

            if !instruction_state.instruction_complete {
                State::Executing(instruction_state)
            } else {
                State::InstructionStart { pending_interrupt: instruction_state.pending_interrupt }
            }
        }
        State::OamDmaDelay(state) => State::OamDma(state),
        State::OamDma(OamDmaState {
            mut cycles_remaining,
            source_high_byte,
            mut last_read_value,
            pending_interrupt,
        }) => {
            cycles_remaining -= 1;

            if cycles_remaining % 2 == 1 {
                let source_low_byte = (0xFF - cycles_remaining / 2) as u8;
                last_read_value =
                    bus.read_address(u16::from_le_bytes([source_low_byte, source_high_byte]));
            } else {
                bus.write_address(PpuRegister::OAMDATA.to_address(), last_read_value);
            }

            if cycles_remaining > 0 {
                State::OamDma(OamDmaState {
                    cycles_remaining,
                    source_high_byte,
                    last_read_value,
                    pending_interrupt,
                })
            } else {
                State::InstructionStart { pending_interrupt }
            }
        }
    };
}

/// Reset the CPU, as if the console's reset button was pressed.
///
/// Reset does the following:
/// * Immediately update the PC to point to the RESET vector, and abandon the currently-in-progress instruction (if any)
/// * Subtract 3 from the stack pointer
/// * Disable IRQs
pub fn reset(cpu_state: &mut CpuState, bus: &mut CpuBus<'_>) {
    let reset_vector_lsb = bus.read_address(bus::CPU_RESET_VECTOR);
    let reset_vector_msb = bus.read_address(bus::CPU_RESET_VECTOR + 1);
    let reset_vector = u16::from_le_bytes([reset_vector_lsb, reset_vector_msb]);
    cpu_state.registers.pc = reset_vector;

    cpu_state.registers.sp = cpu_state.registers.sp.wrapping_sub(3);

    cpu_state.registers.status.interrupt_disable = true;

    cpu_state.state = State::INITIAL;

    cpu_state.terminated = false;
}
