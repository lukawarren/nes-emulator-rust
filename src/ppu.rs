use bitflags::bitflags;
use super::memory::Memory;
use super::palette_table::Colour;
use super::palette_table::PALETTE_TABLE;

pub const SCREEN_WIDTH: usize = 256;
pub const SCREEN_HEIGHT: usize = 240;
pub const PATTERN_TABLE_SIZE: usize = 128;
pub const CYCLES_PER_FRAME: usize = (341 / 3) * (262+1);

#[derive(Copy, Clone)]
pub struct Ppu
{
    // Registers
    ppu_control: PpuControl,
    ppu_mask: PpuMask,
    ppu_status: PpuStatus,
    ppu_address: u16,
    table_ram_address: u16,

    // Scrolling
    fine_x: u8,

    // Memory access
    address_latch: bool,
    data_buffer: u8,

    // Timing
    scanline: i16,
    cycles: i16,

    // Memory
    name_tables: [[u8; 1024]; 2],
    palette: [u8; 32],

    // "In-progress" rendering
    next_background_tile_id: u8,
    next_background_tile_attribute: u8,
    next_background_tile_lsb: u8,
    next_background_tile_msb: u8,
    shifter_pattern_low: u16,
    shifter_pattern_high: u16,
    shifter_attribute_low: u16,
    shifter_attribute_high: u16,

    // Sprites; OAM can be written to during DMA, hence the "pub"
    pub object_attribute_memory: [u8; 256],
    oam_address: u8,

    // "In-progress" sprite rendering
    current_scanline_sprites: [ObjectAttribute; 8],
    current_scanline_sprites_count: u8,
    sprite_shifter_pattern_low: [u8; 8],
    sprite_shifter_pattern_high: [u8; 8],
    sprite_zero_in_scanline: bool, // For collision
    sprite_zero_being_rendered: bool, // For collision

    // Input and output
    pub output: [u8; SCREEN_WIDTH*SCREEN_HEIGHT*3],
    pub due_non_maskable_interrupt: bool,
}

bitflags!
{
    #[derive(Default)]
    struct PpuControl: u8
    {
        const NAMETABLE_ADDR1        = 0b00000001;
        const NAMETABLE_ADDR2        = 0b00000010;
        const VRAM_ADDR_INCREMENT    = 0b00000100;
        const SPRITE_PATTERN_ADDR    = 0b00001000;
        const BACKROUND_PATTERN_ADDR = 0b00010000;
        const SPRITE_SIZE            = 0b00100000;
        const MASTER_SLAVE_SELECT    = 0b01000000;
        const GENERATE_NMI           = 0b10000000;
    }

    #[derive(Default)]
    struct PpuMask: u8
    {
        const GREYSCALE                          = 0b00000001;
        const SHOW_BACKGROUND_IN_LEFTMOST_PIXELS = 0b00000010;
        const SHOW_SPRITES_IN_LEFTMOST_PIXELS    = 0b00000100;
        const SHOW_BACKGROUND                    = 0b00001000;
        const SHOW_SPRITES                       = 0b00010000;
        const EMPHASISE_RED                      = 0b00100000; // TODO: emulate
        const EMPHASISE_GREEN                    = 0b01000000; // TODO: emulate
        const EMPHASISE_BLUE                     = 0b10000000; // TODO: emulate
    }

    #[derive(Default)]
    struct PpuStatus: u8
    {
        const SPRITE_OVERFLOW = 0b100000;
        const SPRITE_ZERO_HIT = 0b1000000;
        const V_BLANK         = 0b10000000;
    }
}

impl PpuControl
{
    fn get_sprite_size(&self) -> u8
    {
        if self.contains(PpuControl::SPRITE_SIZE) { return 16 }
        8
    }
}

impl PpuMask
{
    fn rendering_enabled(&self) -> bool
    {
        self.contains(PpuMask::SHOW_BACKGROUND) || self.contains(PpuMask::SHOW_SPRITES)
    }
}

// Addresses can be best conceptualised using "Loopy's scroll docs" -
// see https://wiki.nesdev.org/w/index.php/PPU_scrolling#PPU_internal_registers.
// Representing the different bits in this way makes life easier when working
// out scrolling, and cleans up the code a bit.

#[derive(Default)]
struct LoopyRegister
{
    coarse_x: u8,       // 5 bits; the Nth tile column
    coarse_y: u8,       // 5 bits; the Nth tile row
    name_table_x: u8,   // 1 bit; the name table's column |==> Whilst there are only two name tables, mirroring
    name_table_y: u8,   // 1 bit; the name table's row    |==> makes it so that there are essentially four!
    fine_y: u8,         // 3 bits; the Nth row within the tile
}

// Rust doesn't really have struct unions as in C, so converting to and from a "LoopyRegister"
// requires a bit of bitwise

impl LoopyRegister
{
    fn bits(&self) -> u16
    {
        let coarse_x = ((self.coarse_x & 0b11111) as u16) << 0;
        let coarse_y = ((self.coarse_y & 0b11111) as u16) << 5;
        let name_table_x = ((self.name_table_x & 1) as u16) << 10;
        let name_table_y = ((self.name_table_y & 1) as u16) << 11;
        let fine_y = ((self.fine_y & 0b111) as u16) << 12;

        coarse_x | coarse_y | name_table_x | name_table_y | fine_y
    }

    fn set(&mut self, value: u16)
    {
        self.coarse_x = (value & 0b11111) as u8;
        self.coarse_y = ((value & 0b1111100000) >> 5) as u8;
        self.name_table_x = ((value & 0b10000000000) >> 10) as u8;
        self.name_table_y = ((value & 0b100000000000) >> 11) as u8;
        self.fine_y = ((value & 0b111000000000000) >> 12) as u8;
    }

    fn from(value: u16) -> Self
    {
        let mut register = LoopyRegister::default();
        register.set(value);
        register
    }
}

// Each sprite rendered by the PPU has a corresponding "object attribute" in "object attribute memory"
// (or "OAM" for short). All it stores is the position of the sprite, its corresponding graphical tile
// and a few flags. Here the struct is stored as it is in memory:

#[derive(Default, Copy, Clone)]
pub struct ObjectAttribute
{
    y: u8,
    id: u8,
    attributes: u8,
    x: u8
}

impl ObjectAttribute
{
    pub fn from(bytes: [u8; 4]) -> Self
    {
        ObjectAttribute { y: bytes[0], id: bytes[1], attributes: bytes[2], x: bytes[3] }
    }

    // Sprites can be flipped on both axes
    fn is_flipped_horizontally(&self) -> bool
    {
        self.attributes & 0x40 != 0
    }
    fn is_flipped_vertically(&self) -> bool { self.attributes & 0x80 != 0 }

    // Returns the pattern table used if a sprite is 8x16
    fn get_double_height_pattern_table(&self) -> u8
    {
        self.id & 0x1
    }

    // The first four palettes are for background tiles, leaving the last four for sprites
    fn get_palette(&self) -> u8 { (self.attributes & 3) + 4 }

    // Sprites with priority will take precedence when they overlap with background tiles
    fn has_priority(&self) -> bool
    {
        self.attributes & 0x20 == 0
    }
}

impl Ppu
{
    pub fn default() -> Self
    {
        Ppu
        {
            // Registers
            ppu_control: PpuControl::default(),
            ppu_mask: PpuMask::default(),
            ppu_status: PpuStatus::default(),
            table_ram_address: 0,
            ppu_address: 0,

            // Scrolling
            fine_x: 0,

            // Memory access
            address_latch: false,
            data_buffer: 0,

            // Timing
            scanline: 0,
            cycles: 0,

            // Memory
            name_tables: [[0; 1024]; 2],
            palette: [0; 32],

            // "In-progress" rendering
            next_background_tile_id: 0,
            next_background_tile_attribute: 0,
            next_background_tile_lsb: 0,
            next_background_tile_msb: 0,
            shifter_pattern_low: 0,
            shifter_pattern_high: 0,
            shifter_attribute_low: 0,
            shifter_attribute_high: 0,

            // Sprites
            object_attribute_memory: [0; 256],
            oam_address: 0,

            // "In-progress" sprite rendering
            current_scanline_sprites: [ObjectAttribute::default(); 8],
            current_scanline_sprites_count: 0,
            sprite_shifter_pattern_low: [0; 8],
            sprite_shifter_pattern_high: [0; 8],
            sprite_zero_in_scanline: false,
            sprite_zero_being_rendered: false,

            // Input and output
            output: [0; SCREEN_WIDTH*SCREEN_HEIGHT*3],
            due_non_maskable_interrupt: false,
        }
    }

    // "debugger" prevents debug code modifying the PPU address
    pub fn read_byte_from_cpu(&mut self, memory: &mut Memory, address: u16, debugger: bool) -> u8
    {
        if address == 0x2000 { return 0 } // PPU control; not readable
        if address == 0x2001 { return 0 } // PPU mask; not readable

        // PPU status
        if address == 0x2002
        {
            // Reading this register also resets the v-blank status and the address latch,
            // but this must be done *after* the data has been returned!
            let old_status = self.ppu_status.bits;

            self.ppu_status.set(PpuStatus::V_BLANK, false);
            self.address_latch = false;

            return old_status
        }

        if address == 0x2003 { return 0 } // OAM address; not readable

        // OAM data
        if address == 0x2004 {
            return self.object_attribute_memory[self.oam_address as usize]
        }

        if address == 0x2005 { return 0 } // Scroll registers; not readable
        if address == 0x2006 { return 0 } // PPU address; not readable

        // PPU data
        if address == 0x2007
        {
            // Reading is actually delayed by one cycle, with the result being stored in a
            // buffer within the PPU...
            let mut data = self.data_buffer;
            self.data_buffer = self.read_byte_from_ppu(memory, self.ppu_address);

            // ...unless it's palette memory, in which case there is no delay (but the buffer
            // is still updated), so set it immediately to the contents of the buffer
            if self.ppu_address >= 0x3f00 { data = self.data_buffer; }

            // Reading also increments "ppu_address", respecting the "ppu_control" register too
            if self.ppu_control.contains(PpuControl::VRAM_ADDR_INCREMENT) && debugger { self.ppu_address += 32; }
            else if !debugger { self.ppu_address += 1; }
            return data
        }

        panic!("Could not map external PPU read for address {:#06x}", address);
    }

    pub fn write_byte_from_cpu(&mut self, memory: &mut Memory, address: u16, value: u8)
    {
        // PPU control
        if address == 0x2000
        {
            // Set control bits
            self.ppu_control.bits = value;

            // Update name tables as a result
            let mut loopy = LoopyRegister::from(self.table_ram_address);
            loopy.name_table_x = if self.ppu_control.contains(PpuControl::NAMETABLE_ADDR1) { 1 } else { 0 };
            loopy.name_table_y = if self.ppu_control.contains(PpuControl::NAMETABLE_ADDR2) { 1 } else { 0 };
            self.table_ram_address = loopy.bits();

            return
        }

        // PPU mask
        if address == 0x2001 { self.ppu_mask.bits = value; return }

        // OAM address
        if address == 0x2003 { self.oam_address = value; return }

        // OAM data
        if address == 0x2004 { self.object_attribute_memory[self.oam_address as usize] = value; }

        // Scrolling
        if address == 0x2005
        {
            // Write to X or Y depending on the address latch
            if self.address_latch == false
            {
                self.fine_x = value & 0x7;

                // Set "table_ram_address" coarse_x
                let mut loopy = LoopyRegister::from(self.table_ram_address);
                loopy.coarse_x = value >> 3;
                self.table_ram_address = loopy.bits();
            }

            else
            {
                // Similar to above
                let mut loopy = LoopyRegister::from(self.table_ram_address);
                loopy.fine_y = value & 0x7;
                loopy.coarse_y = value >> 3;
                self.table_ram_address = loopy.bits();
            }

            self.address_latch = !self.address_latch;
            return
        }

        // PPU address
        if address == 0x2006
        {
            // Upper byte first, then lower byte, again depending on address latch as above,
            // only updating once both values have been written
            if self.address_latch == false
            {
                self.table_ram_address = ((value & 0x3f) as u16) << 8 | (self.table_ram_address & 0xff);
            }
            else
            {
                // Only when a whole address has been written is the internal ppu address updated
                self.table_ram_address = (self.table_ram_address & 0xff00) | value as u16;
                self.ppu_address = self.table_ram_address;
            }

            self.address_latch = !self.address_latch;
            return
        }

        // PPU data
        if address == 0x2007
        {
            // Similar to with reading, but with no buffer
            self.write_byte_from_ppu(memory, self.ppu_address, value);
            if self.ppu_control.contains(PpuControl::VRAM_ADDR_INCREMENT) { self.ppu_address += 32; }
            else { self.ppu_address += 1; }
            return
        }

        panic!("Could not map external PPU write for address {:#06x}", address);
    }

    pub fn read_byte_from_ppu(&mut self, memory: &mut Memory, mut address: u16) -> u8
    {
        /*
            0x0000-0x199f - pattern tables (CHR ROM)
            0x2000-0x3eff - name tables (VRAM)
            0x3f00-0x3fff - palettes
            0x4000-0xffff - mirrors of 0x0000 - 0x3fff
         */

        address &= 0x3fff;

        // Check cartridge first
        let (cartridge_read, value) = memory.read_byte_from_ppu(address);
        if cartridge_read { return value }

        // Name tables with mirroring
        if address >= 0x2000 && address <= 0x3eff
        {
            let name_table_address = (address & 0xfff) as usize;

            if memory.rom_header.has_vertical_mirroring()
            {
                if                                name_table_address <= 0x3ff { return self.name_tables[0][name_table_address & 0x3ff] }
                if name_table_address >= 0x400 && name_table_address <= 0x7ff { return self.name_tables[1][name_table_address & 0x3ff] }
                if name_table_address >= 0x800 && name_table_address <= 0xbff { return self.name_tables[0][name_table_address & 0x3ff] }
                if name_table_address >= 0xc00 && name_table_address <= 0xfff { return self.name_tables[1][name_table_address & 0x3ff] }
            }
            else
            {
                if                                name_table_address <= 0x3ff { return self.name_tables[0][name_table_address & 0x3ff] }
                if name_table_address >= 0x400 && name_table_address <= 0x7ff { return self.name_tables[0][name_table_address & 0x3ff] }
                if name_table_address >= 0x800 && name_table_address <= 0xbff { return self.name_tables[1][name_table_address & 0x3ff] }
                if name_table_address >= 0xc00 && name_table_address <= 0xfff { return self.name_tables[1][name_table_address & 0x3ff] }
            }
        }

        // Palettes
        if address >= 0x3f00 && address <= 0x3fff
        {
            let mut palette_address = (address & 0x1f) as usize;
            if palette_address == 0x10 { palette_address = 0x0; }
            if palette_address == 0x14 { palette_address = 0x4; }
            if palette_address == 0x18 { palette_address = 0x8; }
            if palette_address == 0x1c { palette_address = 0xc; }

            // Apply greyscale if need be
            let colour_mask;
            if self.ppu_mask.contains(PpuMask::GREYSCALE) { colour_mask = 0x30 }
            else { colour_mask = 0x3f };

            return self.palette[palette_address] & colour_mask;
        }

        panic!("Could not map internal PPU read for address {:#06x}", address);
    }

    pub fn write_byte_from_ppu(&mut self, memory: &mut Memory, mut address: u16, value: u8)
    {
        /*
            0x0000-0x199f - pattern tables (CHR ROM)
            0x2000-0x3eff - name tables (VRAM)
            0x3f00-0x3fff - palettes
            0x4000-0xffff - mirrors of 0x0000 - 0x3fff
         */

        address &= 0x3fff;

        // Check cartridge first;
        if memory.write_byte_from_ppu(address, value) { return }

        // Name tables with mirroring
        if address >= 0x2000 && address <= 0x3eff
        {
            let name_table_address = (address & 0xfff) as usize;

            if memory.rom_header.has_vertical_mirroring()
            {
                if                                name_table_address <= 0x3ff { self.name_tables[0][name_table_address & 0x3ff] = value; }
                if name_table_address >= 0x400 && name_table_address <= 0x7ff { self.name_tables[1][name_table_address & 0x3ff] = value; }
                if name_table_address >= 0x800 && name_table_address <= 0xbff { self.name_tables[0][name_table_address & 0x3ff] = value; }
                if name_table_address >= 0xc00 && name_table_address <= 0xfff { self.name_tables[1][name_table_address & 0x3ff] = value; }
            }
            else
            {
                if                                name_table_address <= 0x3ff { self.name_tables[0][name_table_address & 0x3ff] = value; }
                if name_table_address >= 0x400 && name_table_address <= 0x7ff { self.name_tables[0][name_table_address & 0x3ff] = value; }
                if name_table_address >= 0x800 && name_table_address <= 0xbff { self.name_tables[1][name_table_address & 0x3ff] = value; }
                if name_table_address >= 0xc00 && name_table_address <= 0xfff { self.name_tables[1][name_table_address & 0x3ff] = value; }
            }

            return
        }

        // Palettes
        if address >= 0x3f00 && address <= 0x3fff
        {
            let mut palette_address = (address & 0x1f) as usize;
            if palette_address == 0x10 { palette_address = 0x0; }
            if palette_address == 0x14 { palette_address = 0x4; }
            if palette_address == 0x18 { palette_address = 0x8; }
            if palette_address == 0x1c { palette_address = 0xc; }
            self.palette[palette_address] = value;
            return
        }

        panic!("Could not map internal PPU write for address {:#06x}", address);
    }

    pub fn execute(&mut self, memory: &mut Memory)
    {
        // Deal with visible scanlines (and -1)
        if self.scanline >= -1 && self.scanline < 240
        {
            // Odd frame cycle skip
            if self.scanline == 0 && self.cycles == 0 { self.cycles = 1; }

            // On the *second* tick of line -1 (that is to say when "cycles" equals 1), the
            // v-blank flag is reset. This is pretty much when a new frame starts, so reset
            // the sprite variables too.
            if self.scanline == -1 && self.cycles == 1
            {
                self.ppu_status.set(PpuStatus::V_BLANK, false);
                self.ppu_status.set(PpuStatus::SPRITE_OVERFLOW, false);
                self.ppu_status.set(PpuStatus::SPRITE_ZERO_HIT, false);

                for i in 0..8
                {
                    self.sprite_shifter_pattern_low[i] = 0;
                    self.sprite_shifter_pattern_high[i] = 0;
                }
            }

            // Fetch next background tile, then deal with sprites
            self.process_background_tiles(memory);
            self.process_sprites(memory);
        }

        // Nothing is done on scanline 240, and then afterwards it's V-blank time
        if self.scanline >= 241 && self.scanline < 261
        {
            if self.scanline == 241 && self.cycles == 1
            {
                // "Vertical blanking lines" - a.k.a. v-blank! On the *second* tick of line 241,
                // we update the v-blank flag and call the NMI too
                self.ppu_status.set(PpuStatus::V_BLANK, true);

                if self.ppu_control.contains(PpuControl::GENERATE_NMI) {
                    self.due_non_maskable_interrupt = true;
                }

            }
        }

        // Get pixel and palette for background (if any), then pixel, palette and priority for sprite (if any)
        let (tile_pixel, tile_palette) = self.get_background_tile_to_draw();
        let (sprite_pixel, sprite_palette, sprite_priority) = self.get_sprite_to_draw();

        // Combine pixels based on how ordering should work; checks for sprite zero hit too
        let (final_pixel, final_palette) = self.get_final_pixel(tile_pixel, tile_palette, sprite_pixel, sprite_palette, sprite_priority);

        // Lookup pixel in palette and work out X and Y based on progress of PPU along screen
        let Colour(red, green, blue) = self.get_colour_from_palette(memory, final_palette, final_pixel);
        let screen_x = (self.cycles - 1) as usize;
        let screen_y = self.scanline as usize;

        // If within visible bounds, plot pixel
        if screen_x < SCREEN_WIDTH && screen_y < SCREEN_HEIGHT
        {
            self.output[(screen_y * SCREEN_WIDTH + screen_x) * 3 + 0] = red;
            self.output[(screen_y * SCREEN_WIDTH + screen_x) * 3 + 1] = green;
            self.output[(screen_y * SCREEN_WIDTH + screen_x) * 3 + 2] = blue;
        }

        // Advance cycles
        self.cycles += 1;

        // Every 341 cycles, the scanline advances
        if self.cycles >= 341
        {
            self.cycles = 0;
            self.scanline += 1;

            // Every 261 scanlines, we go back to the top (which is actually at -1)
            if self.scanline >= 261 {
                self.scanline = -1;
            }
        }
    }

    fn process_background_tiles(&mut self, memory: &mut Memory)
    {
        // Main "fetching stage" for PPU background tiles - split across 8 cycles
        if (self.cycles >= 2 && self.cycles < 258) || (self.cycles >= 321 && self.cycles < 338)
        {
            self.advance_background_shifters();
            let loopy = LoopyRegister::from(self.ppu_address);

            match (self.cycles-1) % 8
            {
                // Fetch background tile
                0 => {
                    self.prime_background_shifters();
                    self.next_background_tile_id = self.read_byte_from_ppu(memory, 0x2000 | (self.ppu_address & 0x0fff));
                }

                // Fetch attribute
                2 => {
                    self.next_background_tile_attribute = self.read_byte_from_ppu(memory,
                      0x23c0 | ((loopy.name_table_y as u16) << 11)
                          | ((loopy.name_table_x as u16) << 10)
                          | ((loopy.coarse_y as u16 / 4) << 3)
                          | (loopy.coarse_x as u16 / 4));

                    if (loopy.coarse_y & 2) != 0 { self.next_background_tile_attribute >>= 4; }
                    if (loopy.coarse_x & 2) != 0 { self.next_background_tile_attribute >>= 2; }
                    self.next_background_tile_attribute &= 3;
                }

                // Fetch pixel from lower plane
                4 => {
                    let background_bit = if self.ppu_control.contains(PpuControl::BACKROUND_PATTERN_ADDR) { 1 } else { 0 };
                    self.next_background_tile_lsb = self.read_byte_from_ppu(memory,
                            (background_bit << 12) +
                            ((self.next_background_tile_id as u16) << 4) +
                            loopy.fine_y as u16);
                }

                // Fetch pixel from higher plane
                6 => {
                    let background_bit = if self.ppu_control.contains(PpuControl::BACKROUND_PATTERN_ADDR) { 1 } else { 0 };
                    self.next_background_tile_msb = self.read_byte_from_ppu(memory,
                            (background_bit << 12) +
                            ((self.next_background_tile_id as u16) << 4) +
                            loopy.fine_y as u16 + 8);
                }

                // Scroll along to next tile
                7 => {
                    self.increment_scroll_x();
                }

                _ => {}
            }
        }

        // Go to next row...
        if self.cycles == 256 {
            self.increment_scroll_y();
        }

        // ...then reset our column (X position) accordingly
        if self.cycles == 257
        {
            self.prime_background_shifters();
            self.update_address_x();
        }

        // The end of the scanline sees a read of the next tile ID, even though we don't need it
        if self.cycles == 338 || self.cycles == 340 {
            self.next_background_tile_id = self.read_byte_from_ppu(memory, 0x2000 | (self.ppu_address & 0xfff));
        }

        // V-blank has ended; begin again
        if self.scanline == -1 && self.cycles >= 280 && self.cycles < 305 {
            self.update_address_y();
        }
    }

    fn process_sprites(&mut self, memory: &mut Memory)
    {
        // Above, tiles are fetched more-or-less consistently with how the PPU operated; that is to say, when the PPU
        // fetched things at certain times, I do too (more or less). Sprites follow the same pattern, but it is not as
        // crucial to emulate this for many games, so for now sprites are fetched and drawn when it's convenient to do so.

        if self.cycles == 257 && self.scanline >= 0
        {
            // Clear the current scanline of data, but set all the Y coordinates to 255, as that'll make it go off screen,
            // where it won't be rendered
            for i in 0..self.current_scanline_sprites.len()
            {
                self.current_scanline_sprites[i] = ObjectAttribute::default();
                self.current_scanline_sprites[i].y = 255;
            }

            // We start of, on our search for sprites in our current scanline, with 0 sprites (of course)
            self.current_scanline_sprites_count = 0;
            self.sprite_zero_in_scanline = false;

            // Now go through OAM memory, and look for the first 8 sprites; the "divide by 4" is because each attribute entry is 4 bytes
            for i in 0..(self.object_attribute_memory.len()/4)
            {
                // Convert bytes in memory to nice struct format
                let entry = ObjectAttribute::from
                    ([
                        self.object_attribute_memory[i*4+0],
                        self.object_attribute_memory[i*4+1],
                        self.object_attribute_memory[i*4+2],
                        self.object_attribute_memory[i*4+3]
                    ]);

                // Work out if scanline intersects sprite based on its height (similar to AABB collision)
                let y_difference: i16 = self.scanline as i16 - entry.y as i16;
                if y_difference >= 0 && y_difference < self.ppu_control.get_sprite_size() as i16
                {
                    // If there's "room on the broom" in the current scanline, add sprite
                    if self.current_scanline_sprites_count != 8
                    {
                        // If it's sprite zero in the scanline, update collision variable
                        if i == 0 { self.sprite_zero_in_scanline = true; }

                        self.current_scanline_sprites[self.current_scanline_sprites_count as usize] = entry;
                        self.current_scanline_sprites_count += 1;
                    }

                    else { self.ppu_status.set(PpuStatus::SPRITE_OVERFLOW, true); }
                }
            }

            // Now we can also figure out if we had too many sprites on this scanline
            self.ppu_status.set(PpuStatus::SPRITE_OVERFLOW, self.current_scanline_sprites_count > 8);
        }

        // Once we know what sprites are coming up, let's prime then into shifters, just like background tiles
        if self.cycles == 340
        {
            for i in 0..self.current_scanline_sprites_count
            {
                let mut sprite_pattern_bits_low: u8;
                let mut sprite_pattern_bits_high: u8;
                let sprite_pattern_address_low: u16;
                let sprite_pattern_address_high: u16;
                let sprite = self.current_scanline_sprites[i as usize];

                // Fetch the pattern bytes from memory, applying vertical mirroring if need be. This can be done simply
                // by subtracting seven from the address. Think about it: each row in an 8x8 sprite (or "half sprite" if
                // we're talking double height sprites) is a byte, and to get the pattern bytes, we just sample at some
                // row within that sprite. Flipping the "row address" on its head will therefore flip the image too!

                if self.ppu_control.get_sprite_size() == 8
                {
                    // The control bit affects which pattern table the sprite is fetched from
                    let pattern_table = if self.ppu_control.contains(PpuControl::SPRITE_PATTERN_ADDR) { 1 } else { 0 };

                    // Apply vertical flipping if need be
                    if !sprite.is_flipped_vertically()
                    {
                        sprite_pattern_address_low =
                            ((pattern_table as u16) << 12) |                            // Pattern table
                            ((sprite.id as u16) << 4) |                                 // Cell
                            (self.scanline - sprite.y as i16) as u16;                   // Row
                    }
                    else
                    {
                        sprite_pattern_address_low =
                            ((pattern_table as u16) << 12) |                            // Pattern table
                            ((sprite.id as u16) << 4) |                                 // Cell
                            (7 - (self.scanline - sprite.y as i16)) as u16;             // Row
                    }
                }

                else
                {
                    // Sprites that're effectively "two sprites tall" have their pattern table set by their id
                    let pattern_table = sprite.get_double_height_pattern_table();

                    if !sprite.is_flipped_vertically()
                    {
                        // Top half
                        if self.scanline - (sprite.y as i16) < 8
                        {
                            sprite_pattern_address_low =
                                ((pattern_table as u16) << 12) |                        // Pattern table
                                (((sprite.id & 0xfe) as u16) << 4) |                    // Cell
                                ((self.scanline - sprite.y as i16) & 7) as u16;         // Row
                        }

                        // Bottom half
                        else
                        {
                            sprite_pattern_address_low =
                                ((pattern_table as u16) << 12) |                        // Pattern table
                                (((sprite.id & 0xfe) as u16 + 1) << 4) |                // Cell
                                ((self.scanline - sprite.y as i16) & 7) as u16;         // Row
                        }
                    }
                    else
                    {
                        // Top half
                        if self.scanline - (sprite.y as i16) < 8
                        {
                            sprite_pattern_address_low =
                                ((pattern_table as u16) << 12) |                        // Pattern table
                                (((sprite.id & 0xfe) as u16) << 4) |                    // Cell
                                (7 - (self.scanline - sprite.y as i16) & 7) as u16;     // Row
                        }

                        // Bottom half
                        else
                        {
                            sprite_pattern_address_low =
                                ((pattern_table as u16) << 12) |                        // Pattern table
                                (((sprite.id & 0xfe) as u16 + 1) << 4) |                // Cell
                                (7 - (self.scanline - sprite.y as i16) & 7) as u16;     // Row
                        }
                    }
                }

                // For the high address we can simply just skip ahead
                sprite_pattern_address_high = sprite_pattern_address_low + 8;

                // To get the pattern bits, it's just a case of reading from the addresses
                sprite_pattern_bits_low = self.read_byte_from_ppu(memory, sprite_pattern_address_low);
                sprite_pattern_bits_high = self.read_byte_from_ppu(memory, sprite_pattern_address_high);

                // Now we've got the pattern bytes, all flipped vertically if need be (which changes the address),
                // we're at liberty to flip stuff horizontally if need be (which changes the actual underlying
                // byte *value*)

                if sprite.is_flipped_horizontally()
                {
                    sprite_pattern_bits_low = self.flip_byte(sprite_pattern_bits_low);
                    sprite_pattern_bits_high = self.flip_byte(sprite_pattern_bits_high);
                }

                // Load onto shift registers for drawing
                self.sprite_shifter_pattern_low[i as usize] = sprite_pattern_bits_low;
                self.sprite_shifter_pattern_high[i as usize] = sprite_pattern_bits_high;
            }
        }
    }

    fn get_background_tile_to_draw(&mut self) -> (u8, u8)
    {
        let mut pixel = 0;
        let mut palette = 0;

        if self.ppu_mask.contains(PpuMask::SHOW_BACKGROUND)
        {
            let scrolling_mask = 0x8000 >> self.fine_x;

            let first_plane_pixel = if (self.shifter_pattern_low & scrolling_mask) > 0 { 1 } else { 0 };
            let second_plane_pixel = if (self.shifter_pattern_high & scrolling_mask) > 0 { 1 } else { 0 };
            pixel = (second_plane_pixel << 1) | first_plane_pixel;

            let first_plane_palette = if (self.shifter_attribute_low & scrolling_mask) > 0 { 1 } else { 0 };
            let second_plane_palette = if (self.shifter_attribute_high & scrolling_mask) > 0 { 1 } else { 0 };
            palette = (second_plane_palette << 1) | first_plane_palette;
        }

        (pixel, palette)
    }

    fn get_sprite_to_draw(&mut self) -> (u8, u8, bool)
    {
        let mut pixel = 0;
        let mut palette = 0;
        let mut priority = false;

        if self.ppu_mask.contains(PpuMask::SHOW_SPRITES)
        {
            // Work through each sprite, which as a consequence of the above fetching is already in the order
            // it should be memory-wise (as per how the z-ordering works). If sprite zero is found, we know
            // it's going to be rendered therefore (assuming it's not transparent!)

            for i in 0..self.current_scanline_sprites_count as usize
            {
                let sprite = self.current_scanline_sprites[i];

                // If the scanline touches this sprite (see "advance_background_shifters")
                if sprite.x == 0
                {
                    // Fetch from both planes by using shifters and combine
                    let first_plane_pixel = if (self.sprite_shifter_pattern_low[i] & 0x80) > 0 { 1 } else { 0 };
                    let second_plane_pixel = if (self.sprite_shifter_pattern_high[i] & 0x80) > 0 { 1 } else { 0 };
                    pixel = (second_plane_pixel << 1) | first_plane_pixel;

                    // Fetch palette and priority
                    palette = sprite.get_palette();
                    priority = sprite.has_priority();

                    // Values of zero for a pixel corresponds to transparency, and sprites are already sorted by priority, so
                    // the first non-zero pixel value we see is the one to stick with
                    if pixel != 0
                    {
                        if i == 0 { self.sprite_zero_being_rendered = true; }
                        break;
                    }
                }
            }
        }

        (pixel, palette, priority)
    }

    fn get_final_pixel(&mut self, tile_pixel: u8, tile_palette: u8, sprite_pixel: u8, sprite_palette: u8, sprite_priority: bool) -> (u8, u8)
    {
        // Work out if to chose the background tile or the sprite - if it stays zero, that'll just end up as the background colour
        let mut rendered_pixel = 0;
        let mut rendered_palette = 0;

        // Transparent background, solid sprite
        if tile_pixel == 0 && sprite_pixel > 0
        {
            rendered_pixel = sprite_pixel;
            rendered_palette = sprite_palette;
        }

        // Solid background, transparent sprite
        else if tile_pixel > 0 && sprite_pixel == 0
        {
            rendered_pixel = tile_pixel;
            rendered_palette = tile_palette;
        }

        // Both are solid - respect sprite priority boolean
        else if tile_pixel > 0 && sprite_pixel > 0
        {
            rendered_pixel = if sprite_priority { sprite_pixel } else { tile_pixel };
            rendered_palette = if sprite_priority { sprite_palette } else { tile_palette };

            // Sprite zero and background may overlap, so update collision
            if self.sprite_zero_in_scanline && self.sprite_zero_being_rendered
                && self.ppu_mask.contains(PpuMask::SHOW_BACKGROUND) && self.ppu_mask.contains(PpuMask::SHOW_SPRITES)
            {
                // If we're not drawing sprites or the background in the very left of the screen,
                // the window for collision is smaller - TODO: visibly respect this

                if !self.ppu_mask.contains(PpuMask::SHOW_BACKGROUND_IN_LEFTMOST_PIXELS) || !self.ppu_mask.contains(PpuMask::SHOW_SPRITES_IN_LEFTMOST_PIXELS)
                {
                    if self.cycles >= 9 && self.cycles < 258 { self.ppu_status.set(PpuStatus::SPRITE_ZERO_HIT, true); }
                }
                else
                {
                    if self.cycles >= 1 && self.cycles < 258 { self.ppu_status.set(PpuStatus::SPRITE_ZERO_HIT, true); }
                }

            }
        }

        (rendered_pixel, rendered_palette)
    }

    fn get_colour_from_palette(&mut self, memory: &mut Memory, palette: u8, pixel: u8) -> Colour
    {
        // Get nth palette - each is 4 bytes large
        let palette_address = palette as u16 * 4 + 0x3f00;

        // Lookup pixel in memory
        let colour = self.read_byte_from_ppu(memory, palette_address + pixel as u16);

        // Convert with lookup table - 0x3f to stop potential array bounds overflows
        PALETTE_TABLE[(colour & 0x3f) as usize]
    }

    fn increment_scroll_x(&mut self)
    {
        // Make sure rendering is enabled
        if self.ppu_mask.rendering_enabled() == false { return }

        // For easier logic, convert address to Loopy register
        let mut loopy = LoopyRegister::from(self.ppu_address);

        // One name table is 32x30 titles (each being 8x8 pixels). If we are going to encounter
        // the neighbouring name table whilst scrolling, we need to loop back round
        if loopy.coarse_x == 31
        {
            loopy.coarse_x = 0;
            loopy.name_table_x = !loopy.name_table_x;
        }
        else
        {
            loopy.coarse_x += 1;
        }

        self.ppu_address = loopy.bits();
    }

    fn increment_scroll_y(&mut self)
    {
        // Make sure rendering is enabled
        if self.ppu_mask.rendering_enabled() == false { return }

        // For easier logic, convert address to Loopy register
        let mut loopy = LoopyRegister::from(self.ppu_address);

        // If we're just scrolling within one 8x8 tile and staying there, it's happy days
        if loopy.fine_y < 7 { loopy.fine_y += 1; }

        else
        {
            // Otherwise we need to worry about wrapping to a new tile ("coarse_y")
            loopy.fine_y = 0;

            // Check if we've crossed a name table
            if loopy.coarse_y == 29
            {
                loopy.coarse_y = 0;
                loopy.name_table_y = !loopy.name_table_y;
            }

            // Go back to the top if we're at the bottom
            else if loopy.coarse_y == 31
            {
                loopy.coarse_y = 0;
            }

            // Otherwise just increase normally
            else { loopy.coarse_y += 1; }
        }

        self.ppu_address = loopy.bits();
    }

    fn update_address_x(&mut self)
    {
        if self.ppu_mask.rendering_enabled() == false { return }

        let mut loopy_ppu_address = LoopyRegister::from(self.ppu_address);
        let loopy_table_ram_address = LoopyRegister::from(self.table_ram_address);

        loopy_ppu_address.name_table_x = loopy_table_ram_address.name_table_x;
        loopy_ppu_address.coarse_x = loopy_table_ram_address.coarse_x;

        self.ppu_address = loopy_ppu_address.bits();
    }

    fn update_address_y(&mut self)
    {
        if self.ppu_mask.rendering_enabled() == false { return }

        let mut loopy_ppu_address = LoopyRegister::from(self.ppu_address);
        let loopy_table_ram_address = LoopyRegister::from(self.table_ram_address);

        loopy_ppu_address.fine_y = loopy_table_ram_address.fine_y;
        loopy_ppu_address.name_table_y = loopy_table_ram_address.name_table_y;
        loopy_ppu_address.coarse_y = loopy_table_ram_address.coarse_y;

        self.ppu_address = loopy_ppu_address.bits();
    }

    // Fill background tile shifters with the data they'll need
    fn prime_background_shifters(&mut self)
    {
        self.shifter_pattern_low = (self.shifter_pattern_low & 0xff00) | self.next_background_tile_lsb as u16;
        self.shifter_pattern_high = (self.shifter_pattern_high & 0xff00) | self.next_background_tile_msb as u16;

        self.shifter_attribute_low = (self.shifter_attribute_low & 0xff00) | (if (self.next_background_tile_attribute & 0b01) != 0 { 0xff } else { 0 });
        self.shifter_attribute_high = (self.shifter_attribute_high & 0xff00) | (if (self.next_background_tile_attribute & 0b10) != 0 { 0xff } else { 0 });
    }

    // Advances shifters for both sprites and backgrounds
    fn advance_background_shifters(&mut self)
    {
        if self.ppu_mask.contains(PpuMask::SHOW_BACKGROUND)
        {
            self.shifter_pattern_low <<= 1;
            self.shifter_pattern_high <<= 1;
            self.shifter_attribute_low <<= 1;
            self.shifter_attribute_high <<= 1;
        }

        if self.ppu_mask.contains(PpuMask::SHOW_SPRITES) && self.cycles >= 1 && self.cycles < 258
        {
            for i in 0..self.current_scanline_sprites_count as usize
            {
                // The easiest way to move the data of each sprite along to us is by decrementing
                // the X coordinate, and then beginning to shift once we hit the sprite
                if self.current_scanline_sprites[i].x > 0 { self.current_scanline_sprites[i].x -= 1; }
                else
                {
                    self.sprite_shifter_pattern_low[i] <<= 1;
                    self.sprite_shifter_pattern_high[i] <<= 1;
                }
            }
        }
    }

    // Flips the bits around in a byte (like a mirror) - used when mirroring sprites horizontally.
    // https://stackoverflow.com/questions/2602823
    fn flip_byte(&self, mut value: u8) -> u8
    {
        value = (value & 0xf0) >> 4 | (value & 0x0f) << 4;
        value = (value & 0xcc) >> 2 | (value & 0x33) << 2;
        value = (value & 0xaa) >> 1 | (value & 0x55) << 1;
        value
    }

    // Debugging code
    pub fn get_pattern_table(&mut self, memory: &mut Memory, pattern_table: u8, palette: u8) -> [u8; PATTERN_TABLE_SIZE*PATTERN_TABLE_SIZE*3]
    {
        let mut output = [0; PATTERN_TABLE_SIZE*PATTERN_TABLE_SIZE*3];

        for tile_y in 0..16
        {
            for tile_x in 0..16
            {
                // Convert to 1D offset (to the nearest tile)
                let tile_address_offset = tile_y * 256 + tile_x * 16;

                for row in 0..8
                {
                    // Fetch row byte from both planes (where each bit is one pixel in its corresponding column)
                    let mut tile_lower_plane = self.read_byte_from_ppu(memory, pattern_table as u16 * 0x1000 + tile_address_offset + row);
                    let mut tile_higher_plane = self.read_byte_from_ppu(memory, pattern_table as u16 * 0x1000 + tile_address_offset + row + 8);

                    for col in 0..8
                    {
                        // Combine least significant bits into single pixel value, then shift along;
                        // writing therefore goes from right to left (hence the "7-col" at the end)
                        let pixel = (tile_lower_plane & 1) << 1 | (tile_higher_plane & 1);
                        tile_lower_plane >>= 1;
                        tile_higher_plane >>= 1;

                        // Write into array after converting colour with palette
                        let x = tile_x * 8 + (7 - col);
                        let y = tile_y * 8 + row;
                        let Colour(red, green, blue) = self.get_colour_from_palette(memory, palette, pixel);
                        output[(y as usize * PATTERN_TABLE_SIZE + x as usize) * 3 + 0] = red;
                        output[(y as usize * PATTERN_TABLE_SIZE + x as usize) * 3 + 1] = green;
                        output[(y as usize * PATTERN_TABLE_SIZE + x as usize) * 3 + 2] = blue;
                    }
                }
            }
        }

        output
    }
}