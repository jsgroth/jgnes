use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RomMetadata {
    pub full_path: PathBuf,
    pub file_name_no_ext: String,
    pub prg_rom_len: u32,
    pub chr_rom_len: u32,
    pub mapper_name: String,
}

struct Header {
    prg_rom_len: u32,
    chr_rom_len: u32,
    mapper_number: u16,
    sub_mapper_number: u8,
}

impl Header {
    fn parse_from(file: &mut File) -> anyhow::Result<Self> {
        let mut header = [0; 16];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut header)?;

        let is_nes_2_0_header = header[7] & 0x0C == 0x08;

        let prg_rom_len_lsb = header[4];
        let chr_rom_len_lsb = header[5];

        let prg_rom_len_msb = if is_nes_2_0_header { header[9] & 0x0F } else { 0 };

        let chr_rom_len_msb = if is_nes_2_0_header { header[9] >> 4 } else { 0 };

        let mapper_number_lsb = (header[7] & 0xF0) | (header[6] >> 4);
        let mapper_number_msb = if is_nes_2_0_header { header[8] & 0x0F } else { 0 };

        let prg_rom_len =
            u32::from(u16::from_le_bytes([prg_rom_len_lsb, prg_rom_len_msb])) * 16 * 1024;
        let chr_rom_len =
            u32::from(u16::from_le_bytes([chr_rom_len_lsb, chr_rom_len_msb])) * 8 * 1024;
        let mapper_number = u16::from_le_bytes([mapper_number_lsb, mapper_number_msb]);
        let sub_mapper_number = if is_nes_2_0_header { header[8] >> 4 } else { 0 };

        Ok(Self { prg_rom_len, chr_rom_len, mapper_number, sub_mapper_number })
    }
}

pub fn get_rom_list(dir: &str) -> anyhow::Result<Vec<RomMetadata>> {
    let mut rom_list = Vec::new();

    for dir_entry in fs::read_dir(Path::new(dir))? {
        let dir_entry = dir_entry?;
        if dir_entry.path().extension().and_then(OsStr::to_str) != Some("nes") {
            continue;
        }

        let metadata = dir_entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }

        let header = {
            let mut file = File::open(&dir_entry.path())?;
            Header::parse_from(&mut file)?
        };

        let Some(file_name) = dir_entry.file_name().to_str().map(String::from) else {
            continue;
        };
        let Some(file_name_no_ext) =
            Path::new(&file_name).with_extension("").to_str().map(String::from)
        else {
            continue;
        };

        rom_list.push(RomMetadata {
            full_path: dir_entry.path(),
            file_name_no_ext,
            prg_rom_len: header.prg_rom_len,
            chr_rom_len: header.chr_rom_len,
            mapper_name: mapper_name(header.mapper_number, header.sub_mapper_number).into(),
        });
    }

    Ok(rom_list)
}

fn mapper_name(mapper_number: u16, sub_mapper_number: u8) -> &'static str {
    match mapper_number {
        0 => "NROM",
        1 => "MMC1",
        2 => "UxROM",
        3 => "CNROM",
        4 => match sub_mapper_number {
            1 => "MMC6",
            _ => "MMC3",
        },
        5 => "MMC5",
        7 => "AxROM",
        9 => "MMC2",
        10 => "MMC4",
        11 => "Color Dreams",
        16 | 153 | 159 => "Bandai FCG",
        19 => "Namco 163",
        21 | 23 | 25 => match (mapper_number, sub_mapper_number) {
            (23 | 25, 3) => "Konami VRC2",
            _ => "Konami VRC4",
        },
        22 => "Konami VRC2",
        24 | 26 => "Konami VRC6",
        34 => match sub_mapper_number {
            1 => "NINA-001",
            _ => "BNROM",
        },
        66 => "GxROM",
        69 => "Sunsoft FME-7",
        71 => "Codemasters",
        76 => "NAMCOT-3446",
        85 => "Konami VRC7",
        88 | 206 => "Namco 108",
        95 => "NAMCOT-3425",
        140 => "Jaleco JF-14",
        154 => "NAMCOT-3453",
        210 => match sub_mapper_number {
            2 => "Namco 340",
            _ => "Namco 175",
        },
        _ => "(Unknown)",
    }
}
