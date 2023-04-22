use crate::bus::CpuBus;
use crate::cpu::{CpuRegisters, StatusFlags};
use tinyvec::ArrayVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressingMode {
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndirectX,
    IndirectY,
    Implied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuRegister {
    A,
    X,
    Y,
    S,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushableRegister {
    A,
    P,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadInstruction {
    // ADC
    AddWithCarry(AddressingMode),
    // AND
    And(AddressingMode),
    // BIT
    BitTest(AddressingMode),
    // CMP / CPX / CPY
    Compare(CpuRegister, AddressingMode),
    // EOR
    ExclusiveOr(AddressingMode),
    // LDA / LDX / LDY
    LoadRegister(CpuRegister, AddressingMode),
    // ORA
    InclusiveOr(AddressingMode),
    // SBC
    SubtractWithCarry(AddressingMode),
}

impl ReadInstruction {
    fn execute(self, value: u8, registers: &mut CpuRegisters) {
        match self {
            Self::AddWithCarry(..) => {
                registers.accumulator = add(registers.accumulator, value, &mut registers.status);
            }
            Self::And(..) => {
                registers.accumulator = and(registers.accumulator, value, &mut registers.status);
            }
            Self::BitTest(..) => {
                bit_test(registers.accumulator, value, &mut registers.status);
            }
            Self::Compare(register, ..) => {
                let register_value = read_register(registers, register);
                compare(register_value, value, &mut registers.status);
            }
            Self::ExclusiveOr(..) => {
                registers.accumulator = xor(registers.accumulator, value, &mut registers.status);
            }
            Self::LoadRegister(register, ..) => {
                write_register(registers, register, value);
                registers
                    .status
                    .set_negative(value & 0x80 != 0)
                    .set_zero(value == 0);
            }
            Self::InclusiveOr(..) => {
                registers.accumulator = or(registers.accumulator, value, &mut registers.status);
            }
            Self::SubtractWithCarry(..) => {
                registers.accumulator =
                    subtract(registers.accumulator, value, &mut registers.status);
            }
        }
    }

    fn addressing_mode(self) -> AddressingMode {
        match self {
            Self::AddWithCarry(addressing_mode)
            | Self::And(addressing_mode)
            | Self::BitTest(addressing_mode)
            | Self::Compare(_, addressing_mode)
            | Self::ExclusiveOr(addressing_mode)
            | Self::LoadRegister(_, addressing_mode)
            | Self::InclusiveOr(addressing_mode)
            | Self::SubtractWithCarry(addressing_mode) => addressing_mode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifyInstruction {
    // ASL
    ShiftLeft(AddressingMode),
    // DEC
    DecrementMemory(AddressingMode),
    // INC
    IncrementMemory(AddressingMode),
    // LSR
    LogicalShiftRight(AddressingMode),
    // ROL
    RotateLeft(AddressingMode),
    // ROR
    RotateRight(AddressingMode),
}

impl ModifyInstruction {
    fn addressing_mode(self) -> AddressingMode {
        match self {
            Self::ShiftLeft(addressing_mode)
            | Self::DecrementMemory(addressing_mode)
            | Self::IncrementMemory(addressing_mode)
            | Self::LogicalShiftRight(addressing_mode)
            | Self::RotateLeft(addressing_mode)
            | Self::RotateRight(addressing_mode) => addressing_mode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistersInstruction {
    // CLC
    ClearCarryFlag,
    // CLD
    ClearDecimalFlag,
    // CLI
    ClearInterruptDisable,
    // CLV
    ClearOverflowFlag,
    // DEX / DEY
    DecrementRegister(CpuRegister),
    // INX / INY
    IncrementRegister(CpuRegister),
    // NOP
    NoOp,
    // SEC
    SetCarryFlag,
    // SED
    SetDecimalFlag,
    // SEI
    SetInterruptDisable,
    // TAX / TAY / TSX / TXA / TXS / TYA
    TransferBetweenRegisters { to: CpuRegister, from: CpuRegister },
}

impl RegistersInstruction {
    fn execute(self, registers: &mut CpuRegisters) {
        match self {
            Self::ClearCarryFlag => {
                registers.status.carry = false;
            }
            Self::ClearDecimalFlag => {
                registers.status.decimal = false;
            }
            Self::ClearInterruptDisable => {
                registers.status.interrupt_disable = false;
            }
            Self::ClearOverflowFlag => {
                registers.status.overflow = false;
            }
            Self::DecrementRegister(register) => {
                let value = read_register(registers, register).wrapping_sub(1);
                write_register(registers, register, value);
                registers
                    .status
                    .set_negative(value & 0x80 != 0)
                    .set_zero(value == 0);
            }
            Self::IncrementRegister(register) => {
                let value = read_register(registers, register).wrapping_add(1);
                write_register(registers, register, value);
                registers
                    .status
                    .set_negative(value & 0x80 != 0)
                    .set_zero(value == 0);
            }
            Self::NoOp => {}
            Self::SetCarryFlag => {
                registers.status.carry = true;
            }
            Self::SetDecimalFlag => {
                registers.status.decimal = true;
            }
            Self::SetInterruptDisable => {
                registers.status.interrupt_disable = true;
            }
            Self::TransferBetweenRegisters { to, from } => {
                let value = read_register(registers, from);
                write_register(registers, to, value);
                if to != CpuRegister::S {
                    registers
                        .status
                        .set_negative(value & 0x80 != 0)
                        .set_zero(value == 0);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchCondition {
    // BCC
    CarryClear,
    // BCS
    CarrySet,
    // BEQ
    Equal,
    // BMI
    Minus,
    // BNE
    NotEqual,
    // BPL
    Positive,
    // BVC
    OverflowClear,
    // BVS
    OverflowSet,
}

type OpsVec = ArrayVec<[CycleOp; 7]>;

#[derive(Debug, Clone)]
struct InstructionState {
    instruction: Instruction,
    ops: OpsVec,
    op_index: u8,
    operand_first_byte: u8,
    operand_second_byte: u8,
    target_first_byte: u8,
    target_second_byte: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Index {
    X,
    Y,
}

impl Index {
    fn get(self, registers: &CpuRegisters) -> u8 {
        match self {
            Self::X => registers.x,
            Self::Y => registers.y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CycleOp {
    FetchOperand1,
    FetchOperand2,
    ZeroPageIndexAddress,
    FetchZeroPage1,
    FetchZeroPage2,
    FetchZeroPageIndexed1,
    FetchZeroPageIndexed2,
    AbsoluteIndexedFixHighByte(Index),
    IndirectIndexedFixHighByte,
    ExecuteRegistersOnly,
    ExecuteImmediateRead,
    ExecuteZeroPageRead,
    ExecuteZeroPageStore,
    ExecuteZeroPageIndexedRead(Index),
    ExecuteZeroPageIndexedStore(Index),
    ExecuteAbsoluteRead,
    ExecuteAbsoluteStore,
    ExecuteAbsoluteIndexedRead(Index),
    ExecuteAbsoluteIndexedReadDelayed(Index),
    ExecuteAbsoluteIndexedStore(Index),
    ExecuteIndexedIndirectRead,
    ExecuteIndexedIndirectStore,
    ExecuteIndirectIndexedRead,
    ExecuteIndirectIndexedReadDelayed,
    ExecuteIndirectIndexedStore,
}

// Needed for ArrayVec
impl Default for CycleOp {
    fn default() -> Self {
        Self::FetchOperand1
    }
}

impl CycleOp {
    fn execute(
        self,
        mut state: InstructionState,
        registers: &mut CpuRegisters,
        bus: &mut CpuBus<'_>,
    ) -> InstructionState {
        match self {
            Self::FetchOperand1 => {
                state.operand_first_byte = bus.read_address(registers.pc);
                registers.pc += 1;
            }
            Self::FetchOperand2 => {
                state.operand_second_byte = bus.read_address(registers.pc);
                registers.pc += 1;
            }
            Self::ZeroPageIndexAddress => {
                // Spurious read
                bus.read_address(u16::from(state.operand_first_byte));
            }
            Self::FetchZeroPage1 => {
                state.target_first_byte = bus.read_address(u16::from(state.operand_first_byte));
            }
            Self::FetchZeroPage2 => {
                state.target_second_byte =
                    bus.read_address(u16::from(state.operand_first_byte.wrapping_add(1)));
            }
            Self::FetchZeroPageIndexed1 => {
                let address = u16::from(state.operand_first_byte.wrapping_add(registers.x));
                state.target_first_byte = bus.read_address(address);
            }
            Self::FetchZeroPageIndexed2 => {
                let address = u16::from(
                    state
                        .operand_first_byte
                        .wrapping_add(registers.x)
                        .wrapping_add(1),
                );
                state.target_second_byte = bus.read_address(address);
            }
            Self::AbsoluteIndexedFixHighByte(index) => {
                let index = index.get(registers);
                let indexed_low_byte = state.operand_first_byte.wrapping_add(index);
                bus.read_address(u16::from_le_bytes([
                    indexed_low_byte,
                    state.operand_second_byte,
                ]));
            }
            Self::IndirectIndexedFixHighByte => {
                let indexed_low_byte = state.target_first_byte.wrapping_add(registers.y);
                bus.read_address(u16::from_le_bytes([
                    indexed_low_byte,
                    state.target_second_byte,
                ]));
            }
            Self::ExecuteRegistersOnly => {
                // Spurious bus read
                bus.read_address(registers.pc);

                state.instruction.as_registers_only().execute(registers);
            }
            Self::ExecuteImmediateRead => {
                state
                    .instruction
                    .as_read()
                    .execute(state.operand_first_byte, registers);
            }
            Self::ExecuteZeroPageRead => {
                let value = bus.read_address(u16::from(state.operand_first_byte));
                state.instruction.as_read().execute(value, registers);
            }
            Self::ExecuteZeroPageStore => {
                let value = read_register(registers, state.instruction.as_store());
                let address = u16::from(state.operand_first_byte);

                bus.write_address(address, value);
            }
            Self::ExecuteZeroPageIndexedRead(index) => {
                let index = index.get(registers);
                let indexed_address = u16::from(state.operand_first_byte.wrapping_add(index));
                let value = bus.read_address(indexed_address);

                state.instruction.as_read().execute(value, registers);
            }
            Self::ExecuteZeroPageIndexedStore(index) => {
                let index = index.get(registers);
                let indexed_address = u16::from(state.operand_first_byte.wrapping_add(index));
                let value = read_register(registers, state.instruction.as_store());

                bus.write_address(indexed_address, value);
            }
            Self::ExecuteAbsoluteRead => {
                let address =
                    u16::from_le_bytes([state.operand_first_byte, state.operand_second_byte]);
                let value = bus.read_address(address);

                state.instruction.as_read().execute(value, registers);
            }
            Self::ExecuteAbsoluteStore => {
                let address =
                    u16::from_le_bytes([state.operand_first_byte, state.operand_second_byte]);
                let value = read_register(registers, state.instruction.as_store());

                bus.write_address(address, value);
            }
            Self::ExecuteAbsoluteIndexedRead(index) => {
                let index = index.get(registers);
                let (indexed_low_byte, overflowed) =
                    state.operand_first_byte.overflowing_add(index);

                let address = u16::from_le_bytes([indexed_low_byte, state.operand_second_byte]);
                let value = bus.read_address(address);

                if !overflowed {
                    state.instruction.as_read().execute(value, registers);

                    // Skip next (last) cycle
                    state.op_index += 1;
                }
            }
            Self::ExecuteAbsoluteIndexedReadDelayed(index) => {
                let address =
                    u16::from_le_bytes([state.operand_first_byte, state.operand_second_byte]);
                let index = index.get(registers);
                let indexed_address = address.wrapping_add(u16::from(index));
                let value = bus.read_address(indexed_address);

                state.instruction.as_read().execute(value, registers);
            }
            Self::ExecuteAbsoluteIndexedStore(index) => {
                let address =
                    u16::from_le_bytes([state.operand_first_byte, state.operand_second_byte]);
                let index = index.get(registers);
                let indexed_address = address.wrapping_add(u16::from(index));
                let value = read_register(registers, state.instruction.as_store());

                bus.write_address(indexed_address, value);
            }
            Self::ExecuteIndexedIndirectRead => {
                let effective_address =
                    u16::from_le_bytes([state.target_first_byte, state.target_second_byte]);
                let value = bus.read_address(effective_address);

                state.instruction.as_read().execute(value, registers);
            }
            Self::ExecuteIndexedIndirectStore => {
                let effective_address =
                    u16::from_le_bytes([state.target_first_byte, state.target_second_byte]);
                let value = read_register(registers, state.instruction.as_store());

                bus.write_address(effective_address, value);
            }
            Self::ExecuteIndirectIndexedRead => {
                let (indexed_low_byte, overflowed) =
                    state.target_first_byte.overflowing_add(registers.y);
                let address = u16::from_le_bytes([indexed_low_byte, state.target_second_byte]);
                let value = bus.read_address(address);

                if !overflowed {
                    state.instruction.as_read().execute(value, registers);

                    // Skip next (last) cycle
                    state.op_index += 1;
                }
            }
            Self::ExecuteIndirectIndexedReadDelayed => {
                let indexed_address =
                    u16::from_le_bytes([state.target_first_byte, state.target_second_byte])
                        .wrapping_add(u16::from(registers.y));
                let value = bus.read_address(indexed_address);

                state.instruction.as_read().execute(value, registers);
            }
            Self::ExecuteIndirectIndexedStore => {
                let indexed_address =
                    u16::from_le_bytes([state.target_first_byte, state.target_second_byte])
                        .wrapping_add(u16::from(registers.y));
                let value = read_register(registers, state.instruction.as_store());

                bus.write_address(indexed_address, value);
            }
        }

        state.op_index += 1;
        state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    Read(ReadInstruction),
    ReadModifyWrite(ModifyInstruction),
    RegistersOnly(RegistersInstruction),
    Branch(BranchCondition),
    // STA / STX / STY
    StoreRegister(CpuRegister, AddressingMode),
    // BRK
    ForceInterrupt,
    // JMP
    Jump(AddressingMode),
    // JSR
    JumpToSubroutine,
    // PHA / PHP
    PushStack(PushableRegister),
    // PLA / PLP
    PullStack(PushableRegister),
    // RTI
    ReturnFromInterrupt,
    // RTS
    ReturnFromSubroutine,
}

impl Instruction {
    fn as_read(self) -> ReadInstruction {
        match self {
            Self::Read(instruction) => instruction,
            _ => panic!("instruction is not a read instruction: {self:?}"),
        }
    }

    fn as_store(self) -> CpuRegister {
        match self {
            Self::StoreRegister(register, ..) => register,
            _ => panic!("instruction is not StoreRegister: {self:?}"),
        }
    }

    fn as_modify(self) -> ModifyInstruction {
        match self {
            Self::ReadModifyWrite(instruction) => instruction,
            _ => panic!("instruction is not a read-modify-write instruction: {self:?}"),
        }
    }

    fn as_registers_only(self) -> RegistersInstruction {
        match self {
            Self::RegistersOnly(instruction) => instruction,
            _ => panic!("instruction is not a registers-only instruction: {self:?}"),
        }
    }

    fn get_cycle_ops(self) -> OpsVec {
        match self {
            Self::Read(instruction) => match instruction.addressing_mode() {
                AddressingMode::Immediate => {
                    [CycleOp::FetchOperand1, CycleOp::ExecuteImmediateRead]
                        .into_iter()
                        .collect()
                }
                AddressingMode::ZeroPage => [CycleOp::FetchOperand1, CycleOp::ExecuteZeroPageRead]
                    .into_iter()
                    .collect(),
                AddressingMode::ZeroPageX => [
                    CycleOp::FetchOperand1,
                    CycleOp::ZeroPageIndexAddress,
                    CycleOp::ExecuteZeroPageIndexedRead(Index::X),
                ]
                .into_iter()
                .collect(),
                AddressingMode::ZeroPageY => [
                    CycleOp::FetchOperand1,
                    CycleOp::ZeroPageIndexAddress,
                    CycleOp::ExecuteZeroPageIndexedRead(Index::Y),
                ]
                .into_iter()
                .collect(),
                AddressingMode::Absolute => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchOperand2,
                    CycleOp::ExecuteAbsoluteRead,
                ]
                .into_iter()
                .collect(),
                AddressingMode::AbsoluteX => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchOperand2,
                    CycleOp::ExecuteAbsoluteIndexedRead(Index::X),
                    CycleOp::ExecuteAbsoluteIndexedReadDelayed(Index::X),
                ]
                .into_iter()
                .collect(),
                AddressingMode::AbsoluteY => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchOperand2,
                    CycleOp::ExecuteAbsoluteIndexedRead(Index::Y),
                    CycleOp::ExecuteAbsoluteIndexedReadDelayed(Index::Y),
                ]
                .into_iter()
                .collect(),
                AddressingMode::IndirectX => [
                    CycleOp::FetchOperand1,
                    CycleOp::ZeroPageIndexAddress,
                    CycleOp::FetchZeroPageIndexed1,
                    CycleOp::FetchZeroPageIndexed2,
                    CycleOp::ExecuteIndexedIndirectRead,
                ]
                .into_iter()
                .collect(),
                AddressingMode::IndirectY => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchZeroPage1,
                    CycleOp::FetchZeroPage2,
                    CycleOp::ExecuteIndirectIndexedRead,
                    CycleOp::ExecuteIndirectIndexedReadDelayed,
                ]
                .into_iter()
                .collect(),
                _ => panic!("unsupported addressing mode for a read instruction: {self:?}"),
            },
            Self::StoreRegister(_, addressing_mode) => match addressing_mode {
                AddressingMode::ZeroPage => [CycleOp::FetchOperand1, CycleOp::ExecuteZeroPageStore]
                    .into_iter()
                    .collect(),
                AddressingMode::ZeroPageX => [
                    CycleOp::FetchOperand1,
                    CycleOp::ZeroPageIndexAddress,
                    CycleOp::ExecuteZeroPageIndexedStore(Index::X),
                ]
                .into_iter()
                .collect(),
                AddressingMode::ZeroPageY => [
                    CycleOp::FetchOperand1,
                    CycleOp::ZeroPageIndexAddress,
                    CycleOp::ExecuteZeroPageIndexedStore(Index::Y),
                ]
                .into_iter()
                .collect(),
                AddressingMode::Absolute => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchOperand2,
                    CycleOp::ExecuteAbsoluteStore,
                ]
                .into_iter()
                .collect(),
                AddressingMode::AbsoluteX => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchOperand2,
                    CycleOp::AbsoluteIndexedFixHighByte(Index::X),
                    CycleOp::ExecuteAbsoluteIndexedStore(Index::X),
                ]
                .into_iter()
                .collect(),
                AddressingMode::AbsoluteY => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchOperand2,
                    CycleOp::AbsoluteIndexedFixHighByte(Index::Y),
                    CycleOp::ExecuteAbsoluteIndexedStore(Index::Y),
                ]
                .into_iter()
                .collect(),
                AddressingMode::IndirectX => [
                    CycleOp::FetchOperand1,
                    CycleOp::ZeroPageIndexAddress,
                    CycleOp::FetchZeroPageIndexed1,
                    CycleOp::FetchZeroPageIndexed2,
                    CycleOp::ExecuteIndexedIndirectStore,
                ]
                .into_iter()
                .collect(),
                AddressingMode::IndirectY => [
                    CycleOp::FetchOperand1,
                    CycleOp::FetchZeroPage1,
                    CycleOp::FetchZeroPage2,
                    CycleOp::IndirectIndexedFixHighByte,
                    CycleOp::ExecuteIndirectIndexedStore,
                ]
                .into_iter()
                .collect(),
                _ => panic!("unsupported addressing mode for StoreRegister: {self:?}"),
            },
            _ => todo!("non-read instructions"),
        }
    }

    fn from_opcode(opcode: u8) -> Option<Self> {
        match opcode {
            0x00 => Some(Self::ForceInterrupt),
            0x01 => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::IndirectX,
            ))),
            0x05 => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::ZeroPage,
            ))),
            0x06 => Some(Self::ReadModifyWrite(ModifyInstruction::ShiftLeft(
                AddressingMode::ZeroPage,
            ))),
            0x08 => Some(Self::PushStack(PushableRegister::P)),
            0x09 => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::Immediate,
            ))),
            0x0A => Some(Self::ReadModifyWrite(ModifyInstruction::ShiftLeft(
                AddressingMode::Accumulator,
            ))),
            0x0D => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::Absolute,
            ))),
            0x0E => Some(Self::ReadModifyWrite(ModifyInstruction::ShiftLeft(
                AddressingMode::Absolute,
            ))),
            0x10 => Some(Self::Branch(BranchCondition::Positive)),
            0x11 => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::IndirectY,
            ))),
            0x15 => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::ZeroPageX,
            ))),
            0x16 => Some(Self::ReadModifyWrite(ModifyInstruction::ShiftLeft(
                AddressingMode::ZeroPageX,
            ))),
            0x18 => Some(Self::RegistersOnly(RegistersInstruction::ClearCarryFlag)),
            0x19 => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::AbsoluteY,
            ))),
            0x1D => Some(Self::Read(ReadInstruction::InclusiveOr(
                AddressingMode::AbsoluteX,
            ))),
            0x1E => Some(Self::ReadModifyWrite(ModifyInstruction::ShiftLeft(
                AddressingMode::AbsoluteX,
            ))),
            0x20 => Some(Self::JumpToSubroutine),
            0x21 => Some(Self::Read(ReadInstruction::And(AddressingMode::IndirectX))),
            0x24 => Some(Self::Read(ReadInstruction::BitTest(
                AddressingMode::ZeroPage,
            ))),
            0x25 => Some(Self::Read(ReadInstruction::And(AddressingMode::ZeroPage))),
            0x26 => Some(Self::ReadModifyWrite(ModifyInstruction::RotateLeft(
                AddressingMode::ZeroPage,
            ))),
            0x28 => Some(Self::PullStack(PushableRegister::P)),
            0x29 => Some(Self::Read(ReadInstruction::And(AddressingMode::Immediate))),
            0x2A => Some(Self::ReadModifyWrite(ModifyInstruction::RotateLeft(
                AddressingMode::Accumulator,
            ))),
            0x2C => Some(Self::Read(ReadInstruction::BitTest(
                AddressingMode::Absolute,
            ))),
            0x2D => Some(Self::Read(ReadInstruction::And(AddressingMode::Absolute))),
            0x2E => Some(Self::ReadModifyWrite(ModifyInstruction::RotateLeft(
                AddressingMode::Absolute,
            ))),
            0x30 => Some(Self::Branch(BranchCondition::Minus)),
            0x31 => Some(Self::Read(ReadInstruction::And(AddressingMode::IndirectY))),
            0x35 => Some(Self::Read(ReadInstruction::And(AddressingMode::ZeroPageX))),
            0x36 => Some(Self::ReadModifyWrite(ModifyInstruction::RotateLeft(
                AddressingMode::ZeroPageX,
            ))),
            0x38 => Some(Self::RegistersOnly(RegistersInstruction::SetCarryFlag)),
            0x39 => Some(Self::Read(ReadInstruction::And(AddressingMode::AbsoluteY))),
            0x3D => Some(Self::Read(ReadInstruction::And(AddressingMode::AbsoluteX))),
            0x3E => Some(Self::ReadModifyWrite(ModifyInstruction::RotateLeft(
                AddressingMode::AbsoluteX,
            ))),
            0x40 => Some(Self::ReturnFromInterrupt),
            0x41 => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::IndirectX,
            ))),
            0x45 => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::ZeroPage,
            ))),
            0x46 => Some(Self::ReadModifyWrite(ModifyInstruction::LogicalShiftRight(
                AddressingMode::ZeroPage,
            ))),
            0x48 => Some(Self::PushStack(PushableRegister::A)),
            0x49 => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::Immediate,
            ))),
            0x4A => Some(Self::ReadModifyWrite(ModifyInstruction::LogicalShiftRight(
                AddressingMode::Accumulator,
            ))),
            0x4C => Some(Self::Jump(AddressingMode::Absolute)),
            0x4D => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::Absolute,
            ))),
            0x4E => Some(Self::ReadModifyWrite(ModifyInstruction::LogicalShiftRight(
                AddressingMode::Absolute,
            ))),
            0x50 => Some(Self::Branch(BranchCondition::OverflowClear)),
            0x51 => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::IndirectY,
            ))),
            0x55 => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::ZeroPageX,
            ))),
            0x56 => Some(Self::ReadModifyWrite(ModifyInstruction::LogicalShiftRight(
                AddressingMode::ZeroPageX,
            ))),
            0x58 => Some(Self::RegistersOnly(
                RegistersInstruction::ClearInterruptDisable,
            )),
            0x59 => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::AbsoluteY,
            ))),
            0x5D => Some(Self::Read(ReadInstruction::ExclusiveOr(
                AddressingMode::AbsoluteX,
            ))),
            0x5E => Some(Self::ReadModifyWrite(ModifyInstruction::LogicalShiftRight(
                AddressingMode::AbsoluteX,
            ))),
            0x60 => Some(Self::ReturnFromSubroutine),
            0x61 => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::IndirectX,
            ))),
            0x65 => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::ZeroPage,
            ))),
            0x66 => Some(Self::ReadModifyWrite(ModifyInstruction::RotateRight(
                AddressingMode::ZeroPage,
            ))),
            0x68 => Some(Self::PullStack(PushableRegister::A)),
            0x69 => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::Immediate,
            ))),
            0x6A => Some(Self::ReadModifyWrite(ModifyInstruction::RotateRight(
                AddressingMode::Accumulator,
            ))),
            0x6C => Some(Self::Jump(AddressingMode::Indirect)),
            0x6D => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::Absolute,
            ))),
            0x6E => Some(Self::ReadModifyWrite(ModifyInstruction::RotateRight(
                AddressingMode::Absolute,
            ))),
            0x70 => Some(Self::Branch(BranchCondition::OverflowSet)),
            0x71 => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::IndirectY,
            ))),
            0x75 => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::ZeroPageX,
            ))),
            0x76 => Some(Self::ReadModifyWrite(ModifyInstruction::RotateRight(
                AddressingMode::ZeroPageX,
            ))),
            0x78 => Some(Self::RegistersOnly(
                RegistersInstruction::SetInterruptDisable,
            )),
            0x79 => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::AbsoluteY,
            ))),
            0x7D => Some(Self::Read(ReadInstruction::AddWithCarry(
                AddressingMode::AbsoluteX,
            ))),
            0x7E => Some(Self::ReadModifyWrite(ModifyInstruction::RotateRight(
                AddressingMode::AbsoluteX,
            ))),
            0x81 => Some(Self::StoreRegister(
                CpuRegister::A,
                AddressingMode::IndirectX,
            )),
            0x84 => Some(Self::StoreRegister(
                CpuRegister::Y,
                AddressingMode::ZeroPage,
            )),
            0x85 => Some(Self::StoreRegister(
                CpuRegister::A,
                AddressingMode::ZeroPage,
            )),
            0x86 => Some(Self::StoreRegister(
                CpuRegister::X,
                AddressingMode::ZeroPage,
            )),
            0x88 => Some(Self::RegistersOnly(
                RegistersInstruction::DecrementRegister(CpuRegister::Y),
            )),
            0x8A => Some(Self::RegistersOnly(
                RegistersInstruction::TransferBetweenRegisters {
                    to: CpuRegister::A,
                    from: CpuRegister::X,
                },
            )),
            0x8C => Some(Self::StoreRegister(
                CpuRegister::Y,
                AddressingMode::Absolute,
            )),
            0x8D => Some(Self::StoreRegister(
                CpuRegister::A,
                AddressingMode::Absolute,
            )),
            0x8E => Some(Self::StoreRegister(
                CpuRegister::X,
                AddressingMode::Absolute,
            )),
            0x90 => Some(Self::Branch(BranchCondition::CarryClear)),
            0x91 => Some(Self::StoreRegister(
                CpuRegister::A,
                AddressingMode::IndirectY,
            )),
            0x94 => Some(Self::StoreRegister(
                CpuRegister::Y,
                AddressingMode::ZeroPageX,
            )),
            0x95 => Some(Self::StoreRegister(
                CpuRegister::A,
                AddressingMode::ZeroPageX,
            )),
            0x96 => Some(Self::StoreRegister(
                CpuRegister::X,
                AddressingMode::ZeroPageY,
            )),
            0x98 => Some(Self::RegistersOnly(
                RegistersInstruction::TransferBetweenRegisters {
                    to: CpuRegister::A,
                    from: CpuRegister::Y,
                },
            )),
            0x99 => Some(Self::StoreRegister(
                CpuRegister::A,
                AddressingMode::AbsoluteY,
            )),
            0x9A => Some(Self::RegistersOnly(
                RegistersInstruction::TransferBetweenRegisters {
                    to: CpuRegister::S,
                    from: CpuRegister::X,
                },
            )),
            0x9D => Some(Self::StoreRegister(
                CpuRegister::A,
                AddressingMode::AbsoluteX,
            )),
            0xA0 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::Y,
                AddressingMode::Immediate,
            ))),
            0xA1 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::IndirectX,
            ))),
            0xA2 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::X,
                AddressingMode::Immediate,
            ))),
            0xA4 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::Y,
                AddressingMode::ZeroPage,
            ))),
            0xA5 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::ZeroPage,
            ))),
            0xA6 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::X,
                AddressingMode::ZeroPage,
            ))),
            0xA8 => Some(Self::RegistersOnly(
                RegistersInstruction::TransferBetweenRegisters {
                    to: CpuRegister::Y,
                    from: CpuRegister::A,
                },
            )),
            0xA9 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::Immediate,
            ))),
            0xAA => Some(Self::RegistersOnly(
                RegistersInstruction::TransferBetweenRegisters {
                    to: CpuRegister::X,
                    from: CpuRegister::A,
                },
            )),
            0xAC => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::Y,
                AddressingMode::Absolute,
            ))),
            0xAD => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::Absolute,
            ))),
            0xAE => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::X,
                AddressingMode::Absolute,
            ))),
            0xB0 => Some(Self::Branch(BranchCondition::CarrySet)),
            0xB1 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::IndirectY,
            ))),
            0xB4 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::Y,
                AddressingMode::ZeroPageX,
            ))),
            0xB5 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::ZeroPageX,
            ))),
            0xB6 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::X,
                AddressingMode::ZeroPageY,
            ))),
            0xB8 => Some(Self::RegistersOnly(RegistersInstruction::ClearOverflowFlag)),
            0xB9 => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::AbsoluteY,
            ))),
            0xBA => Some(Self::RegistersOnly(
                RegistersInstruction::TransferBetweenRegisters {
                    to: CpuRegister::X,
                    from: CpuRegister::S,
                },
            )),
            0xBC => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::Y,
                AddressingMode::AbsoluteX,
            ))),
            0xBD => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::A,
                AddressingMode::AbsoluteX,
            ))),
            0xBE => Some(Self::Read(ReadInstruction::LoadRegister(
                CpuRegister::X,
                AddressingMode::AbsoluteY,
            ))),
            0xC0 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::Y,
                AddressingMode::Immediate,
            ))),
            0xC1 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::IndirectX,
            ))),
            0xC4 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::Y,
                AddressingMode::ZeroPage,
            ))),
            0xC5 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::ZeroPage,
            ))),
            0xC6 => Some(Self::ReadModifyWrite(ModifyInstruction::DecrementMemory(
                AddressingMode::ZeroPage,
            ))),
            0xC8 => Some(Self::RegistersOnly(
                RegistersInstruction::IncrementRegister(CpuRegister::Y),
            )),
            0xC9 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::Immediate,
            ))),
            0xCA => Some(Self::RegistersOnly(
                RegistersInstruction::DecrementRegister(CpuRegister::X),
            )),
            0xCC => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::Y,
                AddressingMode::Absolute,
            ))),
            0xCD => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::Absolute,
            ))),
            0xCE => Some(Self::ReadModifyWrite(ModifyInstruction::DecrementMemory(
                AddressingMode::Absolute,
            ))),
            0xD0 => Some(Self::Branch(BranchCondition::NotEqual)),
            0xD1 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::IndirectY,
            ))),
            0xD5 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::ZeroPageX,
            ))),
            0xD6 => Some(Self::ReadModifyWrite(ModifyInstruction::DecrementMemory(
                AddressingMode::ZeroPageX,
            ))),
            0xD8 => Some(Self::RegistersOnly(RegistersInstruction::ClearDecimalFlag)),
            0xD9 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::AbsoluteY,
            ))),
            0xDD => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::A,
                AddressingMode::AbsoluteX,
            ))),
            0xDE => Some(Self::ReadModifyWrite(ModifyInstruction::DecrementMemory(
                AddressingMode::AbsoluteX,
            ))),
            0xE0 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::X,
                AddressingMode::Immediate,
            ))),
            0xE1 => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::IndirectX,
            ))),
            0xE4 => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::X,
                AddressingMode::ZeroPage,
            ))),
            0xE5 => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::ZeroPage,
            ))),
            0xE6 => Some(Self::ReadModifyWrite(ModifyInstruction::IncrementMemory(
                AddressingMode::ZeroPage,
            ))),
            0xE8 => Some(Self::RegistersOnly(
                RegistersInstruction::IncrementRegister(CpuRegister::X),
            )),
            0xE9 => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::Immediate,
            ))),
            0xEA => Some(Self::RegistersOnly(RegistersInstruction::NoOp)),
            0xEC => Some(Self::Read(ReadInstruction::Compare(
                CpuRegister::X,
                AddressingMode::Absolute,
            ))),
            0xED => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::Absolute,
            ))),
            0xEE => Some(Self::ReadModifyWrite(ModifyInstruction::IncrementMemory(
                AddressingMode::Absolute,
            ))),
            0xF0 => Some(Self::Branch(BranchCondition::Equal)),
            0xF1 => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::IndirectY,
            ))),
            0xF5 => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::ZeroPageX,
            ))),
            0xF6 => Some(Self::ReadModifyWrite(ModifyInstruction::IncrementMemory(
                AddressingMode::ZeroPageX,
            ))),
            0xF8 => Some(Self::RegistersOnly(RegistersInstruction::SetDecimalFlag)),
            0xF9 => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::AbsoluteY,
            ))),
            0xFD => Some(Self::Read(ReadInstruction::SubtractWithCarry(
                AddressingMode::AbsoluteX,
            ))),
            0xFE => Some(Self::ReadModifyWrite(ModifyInstruction::IncrementMemory(
                AddressingMode::AbsoluteX,
            ))),
            _ => {
                // Unused or unofficial opcode
                None
            }
        }
    }
}

fn read_register(registers: &CpuRegisters, register: CpuRegister) -> u8 {
    match register {
        CpuRegister::A => registers.accumulator,
        CpuRegister::X => registers.x,
        CpuRegister::Y => registers.y,
        CpuRegister::S => registers.sp,
    }
}

fn write_register(registers: &mut CpuRegisters, register: CpuRegister, value: u8) {
    let field = match register {
        CpuRegister::A => &mut registers.accumulator,
        CpuRegister::X => &mut registers.x,
        CpuRegister::Y => &mut registers.y,
        CpuRegister::S => &mut registers.sp,
    };
    *field = value;
}

fn add(accumulator: u8, value: u8, flags: &mut StatusFlags) -> u8 {
    let existing_carry = flags.carry;

    let (result, new_carry) = match accumulator.overflowing_add(value) {
        (sum, true) => (sum + u8::from(existing_carry), true),
        (sum, false) => sum.overflowing_add(u8::from(existing_carry)),
    };

    let (_, overflow) = match (accumulator as i8).overflowing_add(value as i8) {
        (sum, true) => (sum, true),
        (sum, false) => sum.overflowing_add(i8::from(existing_carry)),
    };

    flags
        .set_negative(result & 0x80 != 0)
        .set_overflow(overflow)
        .set_zero(result == 0)
        .set_carry(new_carry);

    result
}

fn subtract(accumulator: u8, value: u8, flags: &mut StatusFlags) -> u8 {
    // Carry flag is inverted in subtraction
    let existing_carry = u8::from(!flags.carry);

    let (result, borrowed) = match accumulator.overflowing_sub(value) {
        (difference, true) => (difference - existing_carry, true),
        (difference, false) => difference.overflowing_sub(existing_carry),
    };

    let (_, overflow) = match (accumulator as i8).overflowing_sub(value as i8) {
        (difference, true) => (difference, true),
        (difference, false) => difference.overflowing_sub(existing_carry as i8),
    };

    flags
        .set_negative(result & 0x80 != 0)
        .set_overflow(overflow)
        .set_zero(result == 0)
        .set_carry(!borrowed);

    result
}

fn and(accumulator: u8, value: u8, flags: &mut StatusFlags) -> u8 {
    let result = accumulator & value;
    flags.set_negative(result & 0x80 != 0).set_zero(result == 0);
    result
}

fn or(accumulator: u8, value: u8, flags: &mut StatusFlags) -> u8 {
    let result = accumulator | value;
    flags.set_negative(result & 0x80 != 0).set_zero(result == 0);
    result
}

fn xor(accumulator: u8, value: u8, flags: &mut StatusFlags) -> u8 {
    let result = accumulator ^ value;
    flags.set_negative(result & 0x80 != 0).set_zero(result == 0);
    result
}

fn compare(register: u8, value: u8, flags: &mut StatusFlags) {
    flags
        .set_negative(register.wrapping_sub(value) & 0x80 != 0)
        .set_zero(register == value)
        .set_carry(register >= value);
}

fn bit_test(accumulator: u8, value: u8, flags: &mut StatusFlags) {
    flags
        .set_negative(value & 0x80 != 0)
        .set_overflow(value & 0x40 != 0)
        .set_zero(accumulator & value == 0);
}
