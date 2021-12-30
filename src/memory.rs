use super::ppu::Ppu;
use std::fs::File;
use std::io::Read;
use std::ops::BitAnd;
use bitflags::bitflags;

pub struct Memory
{
    pub ram: [u8; 2048],
    pub pgr_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub internal_controller: [u8; 2], // What is readable by the CPU; has to be written to update
    pub controller: [u8; 2], // The actual state, as set by the emulator
    pub rom_header: RomHeader,

    // DMA
    pub dma_page: u8,
    pub dma_address: u8,
    pub dma_data: u8,
    pub dma_happening: bool,
    pub dma_waiting_for_sync: bool,
}

bitflags!
{
    #[derive(Default)]
    struct FlagsSix: u8
    {
        const MIRRORING = 0b1; // 0 = horizontal, 1 = vertical
        const CONTAINS_PERSISTENT_MEMORY = 0b10;
        const HAS_TRAINER = 0b100;
        const IGNORE_MIRRORING_CONTROL = 0b1000;
        const MAPPER_NUMBER_LOWER_NIBBLE = 0b11110000;
    }

    #[derive(Default)]
    struct FlagsSeven: u8
    {
        const MAPPER_NUMBER_HIGHER_NIBBLE = 0b11110000;
    }

    #[derive(Default)]
    struct FlagsEight: u8 {}

    #[derive(Default)]
    struct FlagsNine: u8 {}

    #[derive(Default)]
    struct FlagsTen: u8 {}
}

#[allow(dead_code)]
pub struct RomHeader
{
    header_string: [u8; 4], // Reads "NES" - terminated by MS-DOS EOF
    pgr_size: usize,
    chr_size: usize,
    flags_six: FlagsSix,
    flags_seven: FlagsSeven,
    flags_eight: FlagsEight,
    flags_nine: FlagsNine,
    flags_ten: FlagsTen
}

impl RomHeader
{
    fn from_bytes(bytes: &[u8; 16]) -> Self
    {
        RomHeader
        {
            header_string: [
                bytes[0], bytes[1], bytes[2], bytes[3]
            ],
            pgr_size: bytes[4] as usize * 16384,
            chr_size: bytes[5] as usize * 8192,
            flags_six: FlagsSix::from_bits(bytes[6]).unwrap(),
            flags_seven: FlagsSeven::from_bits(bytes[7]).unwrap(),
            flags_eight: FlagsEight::from_bits(bytes[8]).unwrap(),
            flags_nine: FlagsNine::from_bits(bytes[9]).unwrap(),
            flags_ten: FlagsTen::from_bits(bytes[10]).unwrap()
        }
    }

    fn get_mapper_number(&self) -> u8
    {
        return ((self.flags_seven.bits & FlagsSeven::MAPPER_NUMBER_HIGHER_NIBBLE.bits) << 4) |
            (self.flags_six.bits & FlagsSix::MAPPER_NUMBER_LOWER_NIBBLE.bits);
    }

    pub fn has_vertical_mirroring(&self) -> bool
    {
        self.flags_six.contains(FlagsSix::MIRRORING)
    }

    fn has_trainer(&self) -> bool
    {
        return !self.flags_six.bitand(FlagsSix::HAS_TRAINER).is_empty();
    }
}

impl Memory
{
    pub fn default() -> Self
    {
        // Open ROM and get size
        let rom_filename = "./mario.nes";
        let mut rom_file = File::open(&rom_filename).expect("Could not find ROM file");
        let rom_size = std::fs::metadata(&rom_filename).expect("Could not read ROM metadata").len() as usize;

        // Fill into buffer
        let mut rom_data = vec![0; rom_size];
        rom_file.read(&mut rom_data).expect("Could not find enough space to read ROM into buffer");

        /*
            ROM will be in "iNES" format (aka ".nes" files), whereupon the structure will be as so:
            - First 16 bytes: header
            - The trainer, if present, at 512 bytes (or 0)
            - PRG ROM data (aligned to sizes of 16k)
            - CHR ROM data (aligned to sizes of 8k)
         */

        // Get header
        let header = RomHeader::from_bytes(&rom_data[0..16].try_into().unwrap());

        // Determine mapper type
        if header.get_mapper_number() != 0 {
            panic!("Attempted to load ROM with unrecognised mapper type {}", header.get_mapper_number());
        }

        // Retrieve PGR ROM
        let pgr_offset = 16 + if header.has_trainer() { 512 } else { 0 } as usize;
        let pgr_rom = &rom_data[pgr_offset..pgr_offset + header.pgr_size as usize];

        // Retrieve CHR ROM
        let chr_offset = pgr_offset + header.pgr_size;
        let chr_rom = &rom_data[chr_offset..chr_offset + header.chr_size as usize];

        Memory
        {
            ram: [0; 2048],
            pgr_rom: pgr_rom.to_vec(),
            chr_rom: chr_rom.to_vec(),
            controller: [0; 2],
            internal_controller: [0; 2],
            rom_header: header,
            dma_page: 0,
            dma_address: 0,
            dma_data: 0,
            dma_happening: false,
            dma_waiting_for_sync: true,
        }
    }

    // For debugging purposes, reading must have no affect on internal registers like the PPU address

    pub fn read_byte(&mut self, ppu: &mut Ppu, address: u16, debugger: bool) -> u8
    {
        /*
            0x0000-0x07ff - 2kb internal RAM
            0x0800-0x1fff - Mirrors of above
            0x2000-0x2007 - PPU registers
            0x2008-0x3fff - Mirrors of above
            0x4000-0x4017 - APU and I/O registers
            0x4018-0x401f - More APU and I/O stuff
            0x4020-0xffff - Actual cartridge ROM (subject to mappers)
        */

        if address <= 0x1fff {
            return self.ram[(address & 0x7ff) as usize];
        }

        if address >= 0x2000 && address <= 0x2007 {
            return ppu.read_byte_from_cpu(self, address, debugger);
        }

        if address == 0x4016 || address == 0x4017
        {
            // Read from correct controller then shift bits down
            let id = (address & 1) as usize;
            let value = (self.internal_controller[id] & 0x80) > 0;
            self.internal_controller[id] <<= 1;
            return if value { 1 } else { 0 }
        }

        if address >= 0x4000 && address <= 0x401f { return 0 }

        // Assume ROM with mapper type 0 - "NROM"
        else if address >= 0x4020
        {
            // First 16 KB of ROM
            if address >= 0x8000 && address <= 0xbfff { return self.pgr_rom[address as usize - 0x8000]; }

            // Last 16 KB of ROM... or the first 16 KB mirrored (depending on size)
            if address >= 0xc000 && self.rom_header.pgr_size == 0x4000 { return self.pgr_rom[address as usize - 0xc000]; }
            if address >= 0xc000 && self.rom_header.pgr_size == 0x8000 { return self.pgr_rom[address as usize - 0x8000]; }
            
			// All other addresses are invalid, but may be called by the debugger, so as a "quick fix":
			if debugger { return 0 }
        }

        panic!("Could not map memory read for address {:#06x}", address);
    }

    pub fn read_word(&mut self, ppu: &mut Ppu, address: u16, debugger: bool) -> u16
    {
        let high = self.read_byte(ppu, address.wrapping_add(1), debugger) as u16;
        let low = self.read_byte(ppu, address, debugger) as u16;
        (high << 8) | low
    }

    // In indirect addressing modes, we read words from memory, but the nature of the read
    // is such that the address is 8 bit, so adding 1 should cause an overflow. As such,
    // taking a u16 as a function argument causes problems, so to avoid human error by
    // calling this function in other places, it therefore has a very specific name!

    pub fn read_word_from_first_page(&mut self, ppu: &mut Ppu, address: u8, debugger: bool) -> u16
    {
        let high = self.read_byte(ppu, address.wrapping_add(1) as u16, debugger) as u16;
        let low = self.read_byte(ppu, address as u16, debugger) as u16;
        (high << 8) | low
    }

    pub fn write_byte(&mut self, ppu: &mut Ppu, address: u16, value: u8)
    {
        /*
            0x0000-0x07ff - 2kb internal RAM
            0x0800-0x1fff - Mirrors of above
            0x2000-0x2007 - PPU registers
            0x2008-0x3fff - Mirrors of above
            0x4000-0x4017 - APU and I/O registers
            0x4018-0x401f - More APU and I/O stuff
            0x4020-0xffff - Actual cartridge ROM (subject to mappers)
        */

        if address <= 0x7ff
        {
            self.ram[address as usize] = value;
            return
        }

        if address >= 0x2000 && address <= 0x2007
        {
            ppu.write_byte_from_cpu(self, address, value);
            return
        }

        if address == 0x4014
        {
            // Begin DMA by navigating to page; TODO: fix dma_address; doesn't always start at 0
            self.dma_page = value;
            self.dma_address = 0;
            self.dma_happening = true;
        }

        if address == 0x4016 || address == 0x4017
        {
            let id = (address & 1) as usize;
            self.internal_controller[id] = self.controller[id];
        }

        if address >= 0x4000 && address <= 0x401f { return }

        // Assume ROM with mapper type 0 - "NROM"
        if address >= 0x4020
        {
            // First 16 KB of ROM
            if address >= 0x8000 && address <= 0xbfff { self.pgr_rom[address as usize - 0x8000] = value; return }

            // Last 16 KB of ROM... or the first 16 KB mirrored (depending on size)
            if address >= 0xc000 && self.rom_header.pgr_size == 0x4000 { self.pgr_rom[address as usize - 0xc000] = value; return }
            if address >= 0xc000 && self.rom_header.pgr_size == 0x8000 { self.pgr_rom[address as usize - 0x8000] = value; return }
        }

        panic!("Could not map memory write for address {:#06x}", address);
    }

    pub fn pages_differ(&self, first_address: u16, second_address: u16) -> bool
    {
        let first_page = first_address & 0xff00;
        let second_page = second_address & 0xff00;
        first_page != second_page
    }

    // The PPU may wish to read from or write to the cartridge in order to affect CHR ROM, but of course
    // this is subject to a cartridge's individual mapper, hence it lives here, in memory code

    pub fn read_byte_from_ppu(&self, address: u16) -> (bool, u8)
    {
        // Address is relative to cartridge anyway because we're being called from the PPU
        if address <= 0x1fff { return (true, self.chr_rom[address as usize]) }
        (false, 0)
    }

    pub fn write_byte_from_ppu(&mut self, address: u16, value: u8) -> bool
    {
        // Address is relative to cartridge anyway because we're being called from the PPU
        if address <= 0x1fff { self.chr_rom[address as usize] = value; return true }
        false
    }
}