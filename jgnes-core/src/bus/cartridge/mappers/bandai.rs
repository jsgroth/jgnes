mod eeprom;

use crate::bus;
use crate::bus::cartridge::mappers::{BankSizeKb, ChrType, NametableMirroring, PpuMapResult};
use crate::bus::cartridge::MapperImpl;
use crate::num::GetBit;
use bincode::{Decode, Encode};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum MemoryVariant {
    None,
    RAM,
    X24C01,
    X24C02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum Variant {
    Fcg,
    Lz93D50(MemoryVariant),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
enum IrqCounterUpdate {
    LowByte,
    HighByte,
}

#[derive(Debug, Clone, Encode, Decode)]
struct IrqCounter {
    variant: Variant,
    counter: u16,
    latch: u16,
    enabled: bool,
}

impl IrqCounter {
    fn new(variant: Variant) -> Self {
        Self {
            variant,
            counter: 0,
            latch: 0,
            enabled: false,
        }
    }

    fn handle_control_write(&mut self, value: u8) {
        self.enabled = value.bit(0);

        if matches!(self.variant, Variant::Lz93D50(_) | Variant::Unknown) {
            self.counter = self.latch;
        }
    }

    fn update_counter(&mut self, update: IrqCounterUpdate, value: u8) {
        let field_to_update = match self.variant {
            Variant::Fcg => &mut self.counter,
            Variant::Lz93D50(_) | Variant::Unknown => &mut self.latch,
        };

        *field_to_update = match update {
            IrqCounterUpdate::LowByte => (*field_to_update & 0xFF00) | u16::from(value),
            IrqCounterUpdate::HighByte => (*field_to_update & 0x00FF) | (u16::from(value) << 8),
        };
    }

    fn tick_cpu(&mut self) {
        if self.enabled {
            self.counter = self.counter.saturating_sub(1);
        }
    }

    fn interrupt_flag(&self) -> bool {
        self.enabled && self.counter == 0
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub(crate) struct BandaiFcg {
    variant: Variant,
    chr_type: ChrType,
    prg_bank: u8,
    chr_banks: [u8; 8],
    nametable_mirroring: NametableMirroring,
    ram_enabled: bool,
    irq: IrqCounter,
}

impl BandaiFcg {
    pub(crate) fn new(
        mapper_number: u16,
        sub_mapper_number: u8,
        chr_type: ChrType,
        prg_ram_len: u32,
    ) -> Self {
        let variant = match (mapper_number, sub_mapper_number) {
            (16, 4) => Variant::Fcg,
            (16, 5) => {
                let memory_variant = if prg_ram_len > 0 {
                    MemoryVariant::X24C02
                } else {
                    MemoryVariant::None
                };
                Variant::Lz93D50(memory_variant)
            }
            (14, _) => Variant::Unknown,
            (153, _) => Variant::Lz93D50(MemoryVariant::RAM),
            (159, _) => Variant::Lz93D50(MemoryVariant::X24C01),
            _ => panic!("unsupported Bandai mapper number: {mapper_number}"),
        };

        log::info!("Bandai FCG variant: {variant:?}");

        Self {
            variant,
            chr_type,
            prg_bank: 0,
            chr_banks: [0; 8],
            nametable_mirroring: NametableMirroring::Vertical,
            ram_enabled: false,
            irq: IrqCounter::new(variant),
        }
    }
}

impl MapperImpl<BandaiFcg> {
    pub(crate) fn read_cpu_address(&mut self, address: u16) -> u8 {
        match address {
            0x0000..=0x401F => panic!("invalid CPU map address: {address:04X}"),
            0x4020..=0x5FFF => bus::cpu_open_bus(address),
            0x6000..=0x7FFF => match self.data.variant {
                Variant::Fcg => bus::cpu_open_bus(address),
                Variant::Lz93D50(MemoryVariant::RAM) => {
                    if self.data.ram_enabled {
                        self.cartridge.get_prg_ram((address & 0x1FFF).into())
                    } else {
                        bus::cpu_open_bus(address)
                    }
                }
                Variant::Lz93D50(_) | Variant::Unknown => todo!(),
            },
            0x8000..=0xBFFF => {
                let prg_rom_addr =
                    BankSizeKb::Sixteen.to_absolute_address(self.data.prg_bank, address);
                self.cartridge.get_prg_rom(prg_rom_addr)
            }
            0xC000..=0xFFFF => {
                let prg_rom_addr = BankSizeKb::Sixteen
                    .to_absolute_address_last_bank(self.cartridge.prg_rom.len() as u32, address);
                self.cartridge.get_prg_rom(prg_rom_addr)
            }
        }
    }

    pub(crate) fn write_cpu_address(&mut self, address: u16, value: u8) {
        log::trace!("Wrote address={address:04X}, value={value:02X}");
        match (self.data.variant, address) {
            (_, 0x0000..=0x401F) => panic!("invalid CPU map address: {address:04X}"),
            (Variant::Fcg | Variant::Unknown, 0x6000..=0x7FFF)
            | (Variant::Lz93D50(_) | Variant::Unknown, 0x8000..=0xFFFF) => {
                match (self.data.variant, address & 0x000F) {
                    (Variant::Lz93D50(MemoryVariant::RAM), 0x0000..=0x0003) => todo!(),
                    (
                        Variant::Fcg
                        | Variant::Lz93D50(
                            MemoryVariant::None | MemoryVariant::X24C02 | MemoryVariant::X24C01,
                        )
                        | Variant::Unknown,
                        0x0000..=0x0007,
                    ) => {
                        let chr_bank_index = address & 0x0007;
                        self.data.chr_banks[chr_bank_index as usize] = value;
                    }
                    (_, 0x0008) => {
                        self.data.prg_bank = value & 0x0F;
                    }
                    (_, 0x0009) => {
                        self.data.nametable_mirroring = match value & 0x03 {
                            0x00 => NametableMirroring::Vertical,
                            0x01 => NametableMirroring::Horizontal,
                            0x02 => NametableMirroring::SingleScreenBank0,
                            0x03 => NametableMirroring::SingleScreenBank1,
                            _ => unreachable!("value & 0x03 should be 0x00/0x01/0x02/0x03"),
                        };
                    }
                    (_, 0x000A) => {
                        self.data.irq.handle_control_write(value);
                    }
                    (_, 0x000B) => {
                        self.data
                            .irq
                            .update_counter(IrqCounterUpdate::LowByte, value);
                    }
                    (_, 0x000C) => {
                        self.data
                            .irq
                            .update_counter(IrqCounterUpdate::HighByte, value);
                    }
                    (Variant::Lz93D50(MemoryVariant::X24C02 | MemoryVariant::X24C01), 0x000D) => {
                        todo!()
                    }
                    _ => {}
                }
            }
            (Variant::Lz93D50(MemoryVariant::RAM), 0x6000..=0x7FFF) => {
                if self.data.ram_enabled {
                    self.cartridge.set_prg_ram((address & 0x1FFF).into(), value);
                }
            }
            _ => {}
        }
    }

    fn map_ppu_address(&self, address: u16) -> PpuMapResult {
        match address {
            0x0000..=0x1FFF => match self.data.variant {
                Variant::Lz93D50(MemoryVariant::RAM) => PpuMapResult::ChrRAM(address.into()),
                _ => {
                    let chr_bank_index = address / 0x0400;
                    let chr_bank_number = self.data.chr_banks[chr_bank_index as usize];
                    let chr_addr = BankSizeKb::One.to_absolute_address(chr_bank_number, address);
                    self.data.chr_type.to_map_result(chr_addr)
                }
            },
            0x2000..=0x3EFF => {
                PpuMapResult::Vram(self.data.nametable_mirroring.map_to_vram(address))
            }
            0x3F00..=0xFFFF => panic!("invalid PPU map address: {address:04X}"),
        }
    }

    pub(crate) fn read_ppu_address(&self, address: u16, vram: &[u8; 2048]) -> u8 {
        self.map_ppu_address(address).read(&self.cartridge, vram)
    }

    pub(crate) fn write_ppu_address(&mut self, address: u16, value: u8, vram: &mut [u8; 2048]) {
        self.map_ppu_address(address)
            .write(value, &mut self.cartridge, vram);
    }

    pub(crate) fn tick_cpu(&mut self) {
        self.data.irq.tick_cpu();
    }

    pub(crate) fn interrupt_flag(&self) -> bool {
        self.data.irq.interrupt_flag()
    }

    pub(crate) fn name(&self) -> &'static str {
        match self.data.variant {
            Variant::Fcg => "Bandai FCG-1 / FCG-2",
            Variant::Lz93D50(_) => "Bandai LZ93D50",
            Variant::Unknown => "Bandai FCG",
        }
    }
}