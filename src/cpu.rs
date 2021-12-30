use super::memory::Memory;
use super::ppu::Ppu;
use super::opcodes::INSTRUCTIONS;
use super::opcodes::AddressingMode;
use super::opcodes::Operation;
use super::opcodes::Instruction;
use super::opcodes::operation_requires_fetched_argument;
use bitflags::bitflags;

bitflags!
{
    #[derive(Default)]
    pub struct ProcessorState: u8
    {
        const CARRY                 = 0b1;
        const ZERO                  = 0b10;
        const DISABLE_INTERRUPTS    = 0b100;
        const DECIMAL               = 0b1000;
        const B_FLAG                = 0b10000; // B flag lower bit
        const U_FLAG                = 0b100000; // B flag upper bit
        const OVERFLOW              = 0b1000000;
        const NEGATIVE              = 0b10000000;
    }
}

pub struct Cpu
{
    pub pc: u16,               // Program counter
    pub sp: u8,                // Stack pointer
    pub a: u8,                 // Accumulator
    pub x: u8,                 // Index register X
    pub y: u8,                 // Index register Y
    pub flags: ProcessorState, // Processor status (flags)
    pub cycles: u32
}

pub struct Operand
{
    pub data: u16,
    additional_cycle: bool
}

impl Cpu // TODO: use read_x!() and write_x!() macros to clean up arguments
{
    pub fn from_memory(ppu: &mut Ppu, memory: &mut Memory) -> Self
    {
        // Flags start at 0x34 - IRQs disabled
        let mut flags = ProcessorState::default();
        flags.set(ProcessorState::DISABLE_INTERRUPTS, true);
        flags.set(ProcessorState::B_FLAG, true);
        flags.set(ProcessorState::U_FLAG, true);
        assert_eq!(flags.bits, 0x34);

        Cpu
        {
            pc: memory.read_word(ppu, 0xfffc, false), // Program counter depends on reset vector (see memory mapping)
            flags,
            sp: 0xfd,
            a: 0,
            x: 0,
            y: 0,
            cycles: 7
        }
    }

    // Non-maskable interrupts cannot be masked (by definition of course), and store the program
    // counter on the stack, as well as the status register. At the end of the interrupt, it is
    // the "RTI" instruction that will therefore return us from the interrupt. I don't know what
    // the NES calls it, but what I'd call the "interrupt vector" is stored at 0xfffa.

    pub fn on_non_maskable_interrupt(&mut self, ppu: &mut Ppu, memory: &mut Memory)
    {
        // Push program counter
        self.push(ppu, memory, (self.pc >> 8) as u8); // higher byte
        self.push(ppu, memory, (self.pc >> 0) as u8); // lower byte

        // Set the "B flag" to 01
        self.flags.set(ProcessorState::B_FLAG, false);
        self.flags.set(ProcessorState::U_FLAG, true);

        // Disable interrupts now it's dealt with
        self.flags.set(ProcessorState::DISABLE_INTERRUPTS, true);

        // Push modified flags
        self.push(ppu, memory, self.flags.bits);

        // Read "interrupt vector" (or whatever it's called) from 0xfffa
        self.pc = memory.read_word(ppu, 0xfffa, false);
        self.cycles = 8;
    }


    fn read_byte_for_operand(&mut self, ppu: &mut Ppu, memory: &mut Memory, debugger: bool) -> u8
    {
        // Read from program counter than advance it (even in debug mode)
        let data = memory.read_byte(ppu, self.pc, debugger);
        self.pc += 1;
        data
    }

    fn read_word_for_operand(&mut self, ppu: &mut Ppu, memory: &mut Memory, debugger: bool) -> u16
    {
        // As above, but combine into word
        let low = self.read_byte_for_operand(ppu, memory, debugger) as u16;
        let high = self.read_byte_for_operand(ppu, memory, debugger) as u16;
        (high << 8) | low
    }

    // The 6502 has a number of different "addressing modes", so that each opcode may have multiple versions
    // depending on which mode is being used. For example, one could "LDA $1234", "LDA ($12, X)", or simply
    // just "LDA #$10". It is convenient if the resultant *address* is loaded in a generalised way so that
    // one single opcode can support all its addressing modes. This must not be done with the actual data
    // underlying this address (if any) however, because that may incur an additional read on an instruction
    // (such as "STA") where this is not necessary. Some read operations can alter the internal status of the
    // NES (eg. reading from 0x2007 will change the PPU address), so it is crucial we only "fetch_args" when
    // the instruction actually calls for it. To this end, "fetch_operand" will fetch the *address* of data
    // (or the raw data itself where this is not applicable), and "fetch_args" will fetch the corresponding
    // "argument" at that address (if the above data was indeed a valid address), but only when explicitly
    // called for!

    pub fn fetch_operand(&mut self, ppu: &mut Ppu, memory: &mut Memory, addressing_mode: &AddressingMode, debugger: bool) -> Operand
    {
        match addressing_mode
        {
            AddressingMode::Implied => Operand { data: 0, additional_cycle: false },

            AddressingMode::Accumulator => Operand { data: self.a as u16, additional_cycle: false },

            // Fetches from the next byte after the opcode
            AddressingMode::Immediate => {
                Operand { data: self.read_byte_for_operand(ppu, memory, debugger) as u16, additional_cycle: false }
            },

            // Fetches the following 16-bit address
            AddressingMode::Absolute => {
                let address = self.read_word_for_operand(ppu, memory, debugger);
                Operand { data: address, additional_cycle: false }
            }

            // As above, but either X or Y is added to the address
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => {
                let register = if addressing_mode == &AddressingMode::AbsoluteX { self.x } else { self.y };
                let base_address = self.read_word_for_operand(ppu, memory, debugger);
                let address = base_address.wrapping_add(register as u16);

                // If a page boundary has been crossed, an additional clock cycle is required
                Operand { data: address, additional_cycle: memory.pages_differ(base_address, address) }
            }

            // Fetches byte in first page from following address
            AddressingMode::ZeroPage => {
                let address = self.read_byte_for_operand(ppu, memory, debugger);
                Operand { data: address as u16, additional_cycle: false }
            }

            // As above, but with either X or Y used as an offset
            AddressingMode::ZeroPageX | AddressingMode::ZeroPageY => {
                let register = if addressing_mode == &AddressingMode::ZeroPageX { self.x } else { self.y };
                let address = self.read_byte_for_operand(ppu, memory, debugger).wrapping_add(register);
                Operand { data: address as u16, additional_cycle: false }
            }

            // Fetches from address from -128 to +127 bytes from opcode - used only in branching;
            // relative in terms of relative to the program counter *after* the offset has been fetched
            AddressingMode::Relative => {
                let opcode_offset = self.read_byte_for_operand(ppu, memory, debugger) as i8;
                let opcode_address = self.pc;
                let address = opcode_address.wrapping_add(opcode_offset as u16);
                Operand { data: address as u16, additional_cycle: false }
            }

            // The following 16-bit address points to a new 16-bit address, which is the actual one
            // used. A bug in the hardware is present however: if the first 16-bit address has a
            // low byte of 0xff, then when reading the "actual address", we of course need to
            // cross a page. Yet in real hardware, this doesn't happen - instead we must wrap
            // back round to the same page.
            AddressingMode::Indirect => {
                let original_address = self.read_word_for_operand(ppu, memory, debugger);
                let actual_address: u16;

                // Emulate bug
                let lower_byte = memory.read_byte(ppu, original_address, debugger) as u16;
                if original_address & 0xff == 0xff { actual_address = ((memory.read_byte(ppu, original_address & 0xff00, debugger) as u16) << 8) | lower_byte; }
                else { actual_address = ((memory.read_byte(ppu, original_address + 1, debugger) as u16) << 8) | lower_byte; }

                Operand { data: actual_address, additional_cycle: false }
            }

            AddressingMode::IndirectX => {
                // The following 8-bit address is added to register X, and this is then used to
                // find an address in the first page, which contains the actual address, spanning 16 bits.
                let address = self.read_byte_for_operand(ppu, memory, debugger).wrapping_add(self.x);
                let value = memory.read_word_from_first_page(ppu, address, debugger);
                Operand { data: value, additional_cycle: false }
            }

            AddressingMode::IndirectY => {
                // Like above, but with the offset being from register Y, and only added after the
                // sought-after 16-bit address afterwards.
                let address = self.read_byte_for_operand(ppu, memory, debugger);
                let value = memory.read_word_from_first_page(ppu, address, debugger);

                // Where this offset causes a change in page, an additional cycle is needed.
                let page_crossed = memory.pages_differ(value, value.wrapping_add(self.y as u16));
                Operand { data: value.wrapping_add(self.y as u16), additional_cycle: page_crossed }
            }
        }
    }

    fn fetch_args(&mut self, ppu: &mut Ppu, memory: &mut Memory, addressing_mode: &AddressingMode, operand_data: u16) -> u8
    {
        match addressing_mode
        {
            // First, the addressing modes where this doesn't count...
            AddressingMode::Implied => ( 0 ),
            AddressingMode::Accumulator | AddressingMode::Immediate => { operand_data as u8 }

            // and then the rest...
            _ => { memory.read_byte(ppu, operand_data, false) }
        }
    }

    pub fn execute(&mut self, ppu: &mut Ppu, memory: &mut Memory)
    {
        // Fetch opcode
        let opcode = memory.read_byte(ppu, self.pc, false);

        // Decode opcode into more abstract form (because there may be multiple forms of an opcode for each addressing mode)
        let Instruction(name, operation, addressing_mode, cycles) = &INSTRUCTIONS[opcode as usize];
        self.pc += 1;

        // Fetch operand, advancing the program counter too if need be
        let operand = self.fetch_operand(ppu, memory, addressing_mode, false);

        // Fetch argument, but only if the operation calls for it (see long paragraph attached to "fetch_operand")
        let argument = if operation_requires_fetched_argument(operation) { self.fetch_args(ppu, memory, addressing_mode, operand.data) } else { 0 };

        // Execute opcode
        let has_extra_cycles = match operation
        {
            // ----------------------- Binary operations -----------------------

            Operation::ADC => {

                // Adds the current accumulator value, the operand value and the carry flag, whilst also
                // supporting signed overflow and negative numbers. The carry flag is kept so that multiple
                // numbers can be added together in sequence.

                let value = self.a as u16 + argument as u16 + (self.flags.bits & ProcessorState::CARRY.bits) as u16;

                self.set_carry_flag(value > 255);
                self.set_zero_flag(value as u8);
                self.set_overflow_flag(((!(self.a as u16 ^ argument as u16) & (self.a as u16 ^ value)) & 0x80) != 0);
                self.set_negative_flag(value as u8);

                self.a = value as u8;
                true
            }

            Operation::SBC => {

                let value = argument as u16 ^ 0x00ff;
                let temp = self.a as u16 + value + (self.flags.bits & ProcessorState::CARRY.bits) as u16;

                // Because of above logic, the flags can be treated as above, as if addition just occurred
                self.set_carry_flag(temp & 0xff00 != 0);
                self.set_zero_flag(temp as u8);
                self.set_overflow_flag(((temp ^ self.a as u16) & (temp ^ value) & 0x80) != 0);
                self.set_negative_flag(temp as u8);

                self.a = temp as u8;
                true
            }

            Operation::AND => { self.a &= argument as u8; self.set_zero_flag(self.a); self.set_negative_flag(self.a); true }
            Operation::EOR => { self.a ^= argument as u8; self.set_zero_flag(self.a); self.set_negative_flag(self.a); true }
            Operation::ORA => { self.a |= argument as u8; self.set_zero_flag(self.a); self.set_negative_flag(self.a); true }


            // ----------------------- Shifting and rotating -----------------------

            Operation::ASL => {
                let result = argument.wrapping_shl(1);
                self.set_zero_flag(result);
                self.set_negative_flag(result);
                self.set_carry_flag(argument & 0x80 != 0);

                // Result is written either back to byte (in addressing modes absolute, absolute x,
                // zero page, and zero page x), or is stored in the accumulator
                if addressing_mode == &AddressingMode::Accumulator { self.a = result; }
                else { memory.write_byte(ppu, operand.data, result); }

                false
            }

            Operation::LSR => {
                let result = argument.wrapping_shr(1);

                self.set_zero_flag(result);
                self.flags.set(ProcessorState::NEGATIVE, false);
                self.set_carry_flag((argument & 1) != 0);

                // See above
                if addressing_mode == &AddressingMode::Accumulator { self.a = result; }
                else { memory.write_byte(ppu, operand.data, result); }

                false
            }

            Operation::ROL => {
                // Rotate the bits of the specified byte one bit to the left, but then set the
                // new bit number zero to be the value of the carry flag
                let result = argument.wrapping_shl(1) | (if self.flags.contains(ProcessorState::CARRY) { 1 } else { 0 });

                self.set_zero_flag(result);
                self.set_negative_flag(result);
                self.set_carry_flag(argument & 0x80 != 0);

                // As above
                if addressing_mode == &AddressingMode::Accumulator { self.a = result; }
                else { memory.write_byte(ppu, operand.data, result); }

                false
            }

            Operation::ROR => {
                // Similar to above but we shift right and the carry flag becomes the left-most bit instead
                let result = argument.wrapping_shr(1) | (if self.flags.contains(ProcessorState::CARRY) { 0x80 } else { 0 });

                self.set_zero_flag(result);
                self.flags.set(ProcessorState::NEGATIVE, self.flags.contains(ProcessorState::CARRY));
                self.set_carry_flag((argument & 0b1) != 0);

                // As above
                if addressing_mode == &AddressingMode::Accumulator { self.a = result; }
                else { memory.write_byte(ppu, operand.data, result); }

                false
            }


            // ----------------------- Incrementing and decrementing -----------------------

            Operation::INC => { let result = argument.wrapping_add(1); self.set_zero_flag(result); self.set_negative_flag(result); memory.write_byte(ppu, operand.data, result); false }
            Operation::DEC => { let result = argument.wrapping_sub(1); self.set_zero_flag(result); self.set_negative_flag(result); memory.write_byte(ppu, operand.data, result); false }

            Operation::INX => { let result = self.x.wrapping_add(1);   self.set_zero_flag(result); self.set_negative_flag(result); self.x = result; false }
            Operation::INY => { let result = self.y.wrapping_add(1);   self.set_zero_flag(result); self.set_negative_flag(result); self.y = result; false }

            Operation::DEX => { let result = self.x.wrapping_sub(1);   self.set_zero_flag(result); self.set_negative_flag(result); self.x = result; false }
            Operation::DEY => { let result = self.y.wrapping_sub(1);   self.set_zero_flag(result); self.set_negative_flag(result); self.y = result; false }


            // ----------------------- Loading and storing -----------------------

            Operation::LDA => { self.a = argument as u8; self.set_negative_flag(self.a); self.set_zero_flag(self.a); true },
            Operation::LDX => { self.x = argument as u8; self.set_negative_flag(self.x); self.set_zero_flag(self.x); true },
            Operation::LDY => { self.y = argument as u8; self.set_negative_flag(self.y); self.set_zero_flag(self.y); true },

            Operation::STA => { memory.write_byte(ppu, operand.data, self.a); false }
            Operation::STX => { memory.write_byte(ppu, operand.data, self.x); false }
            Operation::STY => { memory.write_byte(ppu, operand.data, self.y); false }


            // ----------------------- Setting and clearing flags -----------------------

            Operation::SEC => { self.flags.set(ProcessorState::CARRY,              true);  false },
            Operation::SED => { self.flags.set(ProcessorState::DECIMAL,            true);  false },
            Operation::SEI => { self.flags.set(ProcessorState::DISABLE_INTERRUPTS, true);  false },

            Operation::CLC => { self.flags.set(ProcessorState::CARRY,              false); false },
            Operation::CLD => { self.flags.set(ProcessorState::DECIMAL,            false); false },
            Operation::CLI => { self.flags.set(ProcessorState::DISABLE_INTERRUPTS, false); false },
            Operation::CLV => { self.flags.set(ProcessorState::OVERFLOW,           false); false },


            // ----------------------- Comparing -----------------------

            Operation::CMP => { self.compare(self.a, argument) }
            Operation::CPX => { self.compare(self.x, argument) }
            Operation::CPY => { self.compare(self.y, argument) }


            // ----------------------- Returning and jumping -----------------------

            Operation::JMP =>
            {
                self.pc = operand.data;
                false
            }

            Operation::JSR => {
                // Push onto the stack the *current* program counter, because it's actually "RTS"
                // that has the burden of adding one to skip past this instruction when returning
                self.pc -= 1;
                self.push(ppu, memory, (self.pc >> 8) as u8);
                self.push(ppu, memory, (self.pc & 0xff) as u8);

                // Jump to subroutine
                self.pc = operand.data;
                false
            }

            Operation::RTI => {
                // Pops the topmost byte from the stack and uses it to update the processor status, then pops
                // the next two bytes from the stack so as to update the program counter
                self.flags.bits = self.pop(ppu, memory);
                self.pc = self.pop(ppu, memory) as u16 | ((self.pop(ppu, memory) as u16) << 8);
                false
            }

            Operation::RTS => {
                // Pop the top two bytes off the stack so as to update the program counter, then add one
                // to get past the pushed "JSR" opcode (see above)
                self.pc = self.pop(ppu, memory) as u16 | ((self.pop(ppu, memory) as u16) << 8);
                self.pc += 1;
                false
            }


            // ----------------------- Branching -----------------------

            Operation::BCC => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::CARRY)    == false) }
            Operation::BCS => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::CARRY)    == true ) }
            Operation::BEQ => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::ZERO)     == true ) }
            Operation::BMI => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::NEGATIVE) == true ) }
            Operation::BNE => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::ZERO)     == false) }
            Operation::BPL => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::NEGATIVE) == false) }
            Operation::BVC => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::OVERFLOW) == false) }
            Operation::BVS => { self.branch(memory, operand.data, self.flags.contains(ProcessorState::OVERFLOW) == true ) }


            // ----------------------- Pushes and pops -----------------------

            Operation::PHA => { self.push(ppu, memory, self.a); false }

            Operation::PHP => {
                // The "B" flag must be set in the pushed flags, but not in our actual flags
                self.push(ppu, memory, self.flags.bits | ProcessorState::B_FLAG.bits | ProcessorState::U_FLAG.bits);
                false
            }

            Operation::PLA => {
                self.a = self.pop(ppu, memory);
                self.set_zero_flag(self.a);
                self.set_negative_flag(self.a);
                false
            }

            Operation::PLP => { self.flags.bits = self.pop(ppu, memory); false }


            // ----------------------- Transfers -----------------------

            Operation::TAX => { self.x = self.transfer_from_accumulator(); false }
            Operation::TAY => { self.y = self.transfer_from_accumulator(); false }

            Operation::TSX => { self.x = self.transfer_to_register(self.sp); false }
            Operation::TXA => { self.a = self.transfer_to_register(self.x);  false }
            Operation::TYA => { self.a = self.transfer_to_register(self.y);  false }

            Operation::TXS => { self.sp = self.x; false },


            // ----------------------- Other stuff -----------------------

            Operation::BIT => {
                // Perform an AND operation between the accumulator value and the operand (without saving the result),
                // then set the zero flag accordingly (based on this result), and set the overflow flag equal to bit
                // number 6 of the original operand, and the negative flag to bit number 7!
                let result = self.a & argument;
                self.set_zero_flag(result);
                self.set_overflow_flag((argument & (1<<6)) != 0);
                self.set_negative_flag(argument);
                false
            }

            Operation::NOP => { false }


            // ----------------------- Unofficial opcodes -----------------------

            Operation::LAX => {
                // Performs an LDA, and then a TAX, saving two cycles and a single byte. Funnily enough, does not support
                // immediate addressing, due to line noise on the data bus, which means even the bugs have bugs!
                self.set_zero_flag(argument);
                self.set_negative_flag(argument);
                self.a = argument;
                self.x = argument;
                true
            },

            Operation::SAX => {
                // Stores the AND of A and X, affecting no flags
                memory.write_byte(ppu, operand.data, self.a & self.x);
                false
            }

            Operation::IGN => {
                // Essentially a NOP that spans two bytes
                true
            }

            Operation::SKB => {
                // Just a fancy NOP
                false
            }

            Operation::DCP => {
                // Equivalent to a DEC followed by a CMP, except that it supports more address modes
                let dec_value = argument.wrapping_sub(1);
                memory.write_byte(ppu, operand.data, dec_value);

                let cmp_value = self.a.wrapping_sub(dec_value);
                self.set_carry_flag(self.a >= dec_value);
                self.set_zero_flag(cmp_value);
                self.set_negative_flag(cmp_value);

                false
            }

            Operation::ISC => {
                // Equivalent to a INC followed by an SBC, but again supporting more address modes
                let inc_value = argument.wrapping_add(1);
                memory.write_byte(ppu, operand.data, inc_value);

                let (sbc_value_one, sbc_carry_one) = self.a.overflowing_sub(inc_value);
                let (sbc_value_two, sbc_carry_two) = sbc_value_one.overflowing_sub(if self.flags.contains(ProcessorState::CARRY) { 0 } else { 1 });

                self.set_carry_flag(!(sbc_carry_one || sbc_carry_two ));
                self.set_zero_flag(sbc_value_two);
                self.set_negative_flag(sbc_value_two);
                self.set_overflow_flag((((self.a ^ inc_value) & 0x80) == 0x80) && (((self.a ^ sbc_value_two) & 0x80) == 0x80));

                self.a = sbc_value_two;
                false
            }

            Operation::RLA => {
                // Equivalent to an ROL followed by an AND, but again supporting more address modes
                let rol_value = argument.wrapping_shl(1) | (if self.flags.contains(ProcessorState::CARRY) { 1 } else { 0 });
                self.set_carry_flag(argument & 0x80 != 0);
                memory.write_byte(ppu, operand.data, rol_value);

                let and_value = self.a & rol_value;
                self.set_zero_flag(and_value);
                self.set_negative_flag(and_value);
                self.a = and_value;

                false
            }

            Operation::RRA => {
                // Equivalent to an ROR followed by an ADC, but again supporting more address modes
                let ror_value = argument.wrapping_shr(1) | (if self.flags.contains(ProcessorState::CARRY) { 0x80 } else { 0x00 });
                self.set_carry_flag((argument & 1) == 1);
                memory.write_byte(ppu, operand.data, ror_value);

                let adc_value = self.a as u16 + ror_value as u16 + (if self.flags.contains(ProcessorState::CARRY) { 1 } else { 0 });

                self.set_carry_flag(adc_value > 255);
                self.set_zero_flag(adc_value as u8);
                self.set_overflow_flag(((self.a as u16 ^ adc_value) & (ror_value as u16 ^ adc_value) & 0x80) == 0x80);
                self.set_negative_flag(adc_value as u8);

                self.a = adc_value as u8;
                false
            }

            Operation::SLO => {

                // Equivalent to an ASL followed by an ORA, but again supporting more address modes
                let asl_value = argument.wrapping_shl(1);
                self.set_carry_flag(argument & 0x80 != 0);
                memory.write_byte(ppu, operand.data, asl_value);

                let ora_value = self.a | asl_value;
                self.set_zero_flag(ora_value);
                self.set_negative_flag(ora_value);
                self.a = ora_value;

                false
            }

            Operation::SRE => {

                // Equivalent to an LSR followed by an EOR, but again supporting more address modes
                let lsr_value = argument.wrapping_shr(1);
                self.set_carry_flag((argument & 1) == 1);
                memory.write_byte(ppu, operand.data, lsr_value);

                let eor_value = self.a ^ lsr_value;
                self.set_zero_flag(eor_value);
                self.set_negative_flag(eor_value);
                self.a = eor_value;

                false
            }

            Operation::BRK => {
                println!("\n\nDone!\n");
                println!("0x2: {:#02x}", memory.read_byte(ppu, 0x2, false));
                println!("0x3: {:#02x}", memory.read_byte(ppu, 0x3, false));
                println!();
                panic!();
            }

            _ => panic!("Could not decode opcode {} - {:#04x}", name, opcode as u8)
        };

        // Some opcodes take longer depending on the addressing mode, and some don't, but it's almost always
        // one cycle extra, so for the majority of opcodes we can say when the generic operation (LDA, AND, etc)
        // has the potential to require extra time, *and* the addressing mode is known to require extra time too,
        // then that particular opcode will indeed be deserving of an extra clock cycle. Fortunately this works.
        if operand.additional_cycle && has_extra_cycles { self.cycles += 1 }

        // Of course we should also take into account the regular old number of cycles too
        self.cycles += *cycles as u32;
    }

    // Below are helper functions for the above opcodes, just to make things tidier and more compact

    pub fn compare(&mut self, register: u8, argument: u8) -> bool
    {
        let (result, _) = register.overflowing_sub(argument);
        self.set_carry_flag(register >= argument);
        self.set_zero_flag(register.wrapping_sub(argument));
        self.set_negative_flag(result);
        true
    }

    pub fn branch(&mut self, memory: &mut Memory, location: u16, condition: bool) -> bool
    {
        if condition
        {
            // Branching to the same page adds one cycle, whilst a different page incurs two extra cycles
            if memory.pages_differ(self.pc, location) { self.cycles += 2 } else { self.cycles += 1 }
            self.pc = location;
        }

        false
    }

    pub fn transfer_from_accumulator(&mut self) -> u8
    {
        self.set_zero_flag(self.a);
        self.set_negative_flag(self.a);
        self.a
    }

    pub fn transfer_to_register(&mut self, register: u8) -> u8
    {
        self.set_zero_flag(register);
        self.set_negative_flag(register);
        register
    }

    pub fn set_carry_flag(&mut self, value: bool)
    {
        self.flags.set(ProcessorState::CARRY, value);
    }

    pub fn set_zero_flag(&mut self, value: u8)
    {
        self.flags.set(ProcessorState::ZERO, value == 0);
    }

    pub fn set_overflow_flag(&mut self, value: bool)
    {
        self.flags.set(ProcessorState::OVERFLOW, value);
    }

    pub fn set_negative_flag(&mut self, value: u8)
    {
        self.flags.set(ProcessorState::NEGATIVE, (value & 0b10000000) != 0);
    }

    pub fn push(&mut self, ppu: &mut Ppu, memory: &mut Memory, value: u8)
    {
        // Stack pointer is just the low byte of the actual stack, which resides from 0x100-0x1ff
        memory.write_byte(ppu, 0x100 + self.sp as u16, value);
        self.sp -= 1;
    }

    pub fn pop(&mut self, ppu: &mut Ppu, memory: &mut Memory) -> u8
    {
        self.sp += 1;
        memory.read_byte(ppu, 0x100 + self.sp as u16, false) // See above for "0x100 + self.sp"
    }
}
