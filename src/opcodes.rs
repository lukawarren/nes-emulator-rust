#[derive(PartialEq, Debug)]
pub enum AddressingMode
{
    Implied,
    Accumulator,
    Immediate,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Relative,
    Indirect,
    IndirectX,
    IndirectY,
}

pub enum Operation
{
    // Binary operations
    ADC,
    SBC,
    AND,
    EOR,
    ORA,

    // Shifting and rotating
    ASL,
    LSR,
    ROL,
    ROR,

    // Incrementing and decrementing
    INC,
    DEC,
    INX,
    INY,
    DEX,
    DEY,

    // Loading and storing
    LDA,
    LDX,
    LDY,
    STA,
    STX,
    STY,

    // Setting and clearing flags
    SEC,
    SED,
    SEI,
    CLC,
    CLD,
    CLI,
    CLV,

    // Comparing
    CMP,
    CPX,
    CPY,

    // Returning and jumping
    JMP,
    JSR,
    RTI,
    RTS,

    // Branching
    BCC,
    BCS,
    BEQ,
    BMI,
    BNE,
    BPL,
    BVC,
    BVS,

    // Pushes and pops
    PHA,
    PHP,
    PLA,
    PLP,

    // Transfers
    TAX,
    TAY,
    TSX,
    TXA,
    TYA,
    TXS,

    // Other stuff
    BRK,
    BIT,
    NOP,

    // Placeholder for unknown
    XXX,

    // Unofficial opcodes
    LAX,
    SAX,
    IGN,
    SKB,
    DCP,
    ISC,
    RLA,
    RRA,
    SLO,
    SRE,
    ALR,
    ANC,
    ARR,
    AXS
}

pub fn operation_requires_fetched_argument(operation: &Operation) -> bool
{
    match operation
    {
        Operation::ADC => true,
        Operation::SBC => true,
        Operation::AND => true,
        Operation::EOR => true,
        Operation::ORA => true,
        Operation::ASL => true,
        Operation::LSR => true,
        Operation::ROL => true,
        Operation::ROR => true,
        Operation::INC => true,
        Operation::DEC => true,
        Operation::LDA => true,
        Operation::LDX => true,
        Operation::LDY => true,
        Operation::CMP => true,
        Operation::CPX => true,
        Operation::CPY => true,
        Operation::BIT => true,
        Operation::LAX => true,
        Operation::SAX => true,
        Operation::DCP => true,
        Operation::ISC => true,
        Operation::RLA => true,
        Operation::RRA => true,
        Operation::SLO => true,
        Operation::SRE => true,
        Operation::SKB => true,
        Operation::IGN => true,
        Operation::ALR => true,
        Operation::ANC => true,
        Operation::ARR => true,
        Operation::AXS => true,

        _ => false
    }
}

pub struct Instruction(pub &'static str, pub Operation, pub AddressingMode, pub u8);

pub static INSTRUCTIONS: [Instruction; 256] =
[
    Instruction("BRK", Operation::BRK, AddressingMode::Immediate, 7),
    Instruction("ORA", Operation::ORA, AddressingMode::IndirectX, 6),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("SLO", Operation::SLO, AddressingMode::IndirectX, 8),       // 0x03 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPage, 3),        // 0x04 - unofficial
    Instruction("ORA", Operation::ORA, AddressingMode::ZeroPage, 3),
    Instruction("ASL", Operation::ASL, AddressingMode::ZeroPage, 5),
    Instruction("SLO", Operation::SLO, AddressingMode::ZeroPage, 5),        // 0x07 - unofficial
    Instruction("PHP", Operation::PHP, AddressingMode::Implied, 3),
    Instruction("ORA", Operation::ORA, AddressingMode::Immediate, 2),
    Instruction("ASL", Operation::ASL, AddressingMode::Accumulator, 2),
    Instruction("ANC", Operation::ANC, AddressingMode::Immediate, 2),       // 0x0b - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::Absolute, 4),        // 0x0c - unofficial
    Instruction("ORA", Operation::ORA, AddressingMode::Absolute, 4),
    Instruction("ASL", Operation::ASL, AddressingMode::Absolute, 6),
    Instruction("SLO", Operation::SLO, AddressingMode::Absolute, 6),        // 0x0f - unofficial

    Instruction("BPL", Operation::BPL, AddressingMode::Relative, 2),
    Instruction("ORA", Operation::ORA, AddressingMode::IndirectY, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("SLO", Operation::SLO, AddressingMode::IndirectY, 8),       // 0x13 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPageX, 4),       // 0x14 - unofficial
    Instruction("ORA", Operation::ORA, AddressingMode::ZeroPageX, 4),
    Instruction("ASL", Operation::ASL, AddressingMode::ZeroPageX, 6),
    Instruction("SLO", Operation::SLO, AddressingMode::ZeroPageX, 6),       // 0x17 - unofficial
    Instruction("CLC", Operation::CLC, AddressingMode::Implied, 2),
    Instruction("ORA", Operation::ORA, AddressingMode::AbsoluteY, 4),
    Instruction("NOP", Operation::NOP, AddressingMode::Implied, 2),         // 0x1a - unofficial
    Instruction("SLO", Operation::SLO, AddressingMode::AbsoluteX, 7),       // 0x1b - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::AbsoluteX, 4),       // 0x1c - unofficial
    Instruction("ORA", Operation::ORA, AddressingMode::AbsoluteX, 4),
    Instruction("ASL", Operation::ASL, AddressingMode::AbsoluteX, 7),
    Instruction("SLO", Operation::SLO, AddressingMode::AbsoluteX, 7),       // 0x1f - unofficial

    Instruction("JSR", Operation::JSR, AddressingMode::Absolute, 6),
    Instruction("AND", Operation::AND, AddressingMode::IndirectX, 6),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("RLA", Operation::RLA, AddressingMode::IndirectX, 8),       // 0x23 - unofficial
    Instruction("BIT", Operation::BIT, AddressingMode::ZeroPage, 3),
    Instruction("AND", Operation::AND, AddressingMode::ZeroPage, 3),
    Instruction("ROL", Operation::ROL, AddressingMode::ZeroPage, 5),
    Instruction("RLA", Operation::RLA, AddressingMode::ZeroPage, 5),        // 0x27 - unofficial
    Instruction("PLP", Operation::PLP, AddressingMode::Implied, 4),
    Instruction("AND", Operation::AND, AddressingMode::Immediate, 2),
    Instruction("ROL", Operation::ROL, AddressingMode::Accumulator, 2),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("BIT", Operation::BIT, AddressingMode::Absolute, 4),
    Instruction("AND", Operation::AND, AddressingMode::Absolute, 4),
    Instruction("ROL", Operation::ROL, AddressingMode::Absolute, 6),
    Instruction("RLA", Operation::RLA, AddressingMode::Absolute, 6),        // 0x2f - unofficial

    Instruction("BMI", Operation::BMI, AddressingMode::Relative, 2),
    Instruction("AND", Operation::AND, AddressingMode::IndirectY, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("RLA", Operation::RLA, AddressingMode::IndirectY, 8),       // 0x33 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPageX, 4),       // 0x34 - unofficial
    Instruction("AND", Operation::AND, AddressingMode::ZeroPageX, 4),
    Instruction("ROL", Operation::ROL, AddressingMode::ZeroPageX, 6),
    Instruction("RLA", Operation::RLA, AddressingMode::ZeroPageX, 6),       // 0x37 - unofficial
    Instruction("SEC", Operation::SEC, AddressingMode::Implied, 2),
    Instruction("AND", Operation::AND, AddressingMode::AbsoluteY, 4),
    Instruction("NOP", Operation::NOP, AddressingMode::Implied, 2),         // 0x3a - unofficial
    Instruction("RLA", Operation::RLA, AddressingMode::AbsoluteY, 7),       // 0x3b - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::AbsoluteX, 4),       // 0x3c - unofficial
    Instruction("AND", Operation::AND, AddressingMode::AbsoluteX, 4),
    Instruction("ROL", Operation::ROL, AddressingMode::AbsoluteX, 7),
    Instruction("RLA", Operation::RLA, AddressingMode::AbsoluteX, 7),       // 0x3f - unofficial

    Instruction("RTI", Operation::RTI, AddressingMode::Implied, 6),
    Instruction("EOR", Operation::EOR, AddressingMode::IndirectX, 6),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("SRE", Operation::SRE, AddressingMode::IndirectX, 8),       // 0x43 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPage, 3),        // 0x44 - unofficial
    Instruction("EOR", Operation::EOR, AddressingMode::ZeroPage, 3),
    Instruction("LSR", Operation::LSR, AddressingMode::ZeroPage, 5),
    Instruction("SRE", Operation::SRE, AddressingMode::ZeroPage, 5),        // 0x47 - unofficial
    Instruction("PHA", Operation::PHA, AddressingMode::Implied, 3),
    Instruction("EOR", Operation::EOR, AddressingMode::Immediate, 2),
    Instruction("LSR", Operation::LSR, AddressingMode::Accumulator, 2),
    Instruction("ALR", Operation::ALR, AddressingMode::Immediate, 2),       // 0x4b - unofficial
    Instruction("JMP", Operation::JMP, AddressingMode::Absolute, 3),
    Instruction("EOR", Operation::EOR, AddressingMode::Absolute, 4),
    Instruction("LSR", Operation::LSR, AddressingMode::Absolute, 6),
    Instruction("SRE", Operation::SRE, AddressingMode::Absolute, 6),        // 0x4f - unofficial

    Instruction("BVC", Operation::BVC, AddressingMode::Relative, 2),
    Instruction("EOR", Operation::EOR, AddressingMode::IndirectY, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("SRE", Operation::SRE, AddressingMode::IndirectY, 8),       // 0x53 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPageX, 4),       // 0x54 - unofficial
    Instruction("EOR", Operation::EOR, AddressingMode::ZeroPageX, 4),
    Instruction("LSR", Operation::LSR, AddressingMode::ZeroPageX, 6),
    Instruction("SRE", Operation::SRE, AddressingMode::ZeroPageX, 6),       // 0x57 - unofficial
    Instruction("CLI", Operation::CLI, AddressingMode::Implied, 2),
    Instruction("EOR", Operation::EOR, AddressingMode::AbsoluteY, 4),
    Instruction("NOP", Operation::NOP, AddressingMode::Implied, 2),         // 0x5a - unofficial
    Instruction("SRE", Operation::SRE, AddressingMode::AbsoluteY, 7),       // 0x5b - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::AbsoluteX, 4),       // 0x5c - unofficial
    Instruction("EOR", Operation::EOR, AddressingMode::AbsoluteX, 4),
    Instruction("LSR", Operation::LSR, AddressingMode::AbsoluteX, 7),
    Instruction("SRE", Operation::SRE, AddressingMode::AbsoluteX, 7),       // 0x5f - unofficial

    Instruction("RTS", Operation::RTS, AddressingMode::Implied, 6),
    Instruction("ADC", Operation::ADC, AddressingMode::IndirectX, 6),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("RRA", Operation::RRA, AddressingMode::IndirectX, 8),       // 0x63 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPage, 3),        // 0x64 - unofficial
    Instruction("ADC", Operation::ADC, AddressingMode::ZeroPage, 3),
    Instruction("ROR", Operation::ROR, AddressingMode::ZeroPage, 5),
    Instruction("RRA", Operation::RRA, AddressingMode::ZeroPage, 5),        // 0x67 - unofficial
    Instruction("PLA", Operation::PLA, AddressingMode::Implied, 4),
    Instruction("ADC", Operation::ADC, AddressingMode::Immediate, 2),
    Instruction("ROR", Operation::ROR, AddressingMode::Accumulator, 2),
    Instruction("ARR", Operation::ARR, AddressingMode::Immediate, 2),       // 0x6b - unofficial
    Instruction("JMP", Operation::JMP, AddressingMode::Indirect, 5),
    Instruction("ADC", Operation::ADC, AddressingMode::Absolute, 4),
    Instruction("ROR", Operation::ROR, AddressingMode::Absolute, 6),
    Instruction("RRA", Operation::RRA, AddressingMode::Absolute, 6),        // 0x6f - unofficial

    Instruction("BVS", Operation::BVS, AddressingMode::Relative, 2),
    Instruction("ADC", Operation::ADC, AddressingMode::IndirectY, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("RRA", Operation::RRA, AddressingMode::IndirectY, 8),       // 0x73 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPageX, 4),       // 0x74 - unofficial
    Instruction("ADC", Operation::ADC, AddressingMode::ZeroPageX, 4),
    Instruction("ROR", Operation::ROR, AddressingMode::ZeroPageX, 6),
    Instruction("RRA", Operation::RRA, AddressingMode::ZeroPageX, 6),       // 0x77 - unofficial
    Instruction("SEI", Operation::SEI, AddressingMode::Implied, 2),
    Instruction("ADC", Operation::ADC, AddressingMode::AbsoluteY, 4),
    Instruction("NOP", Operation::NOP, AddressingMode::Implied, 2),         // 0x7a - unofficial
    Instruction("RRA", Operation::RRA, AddressingMode::AbsoluteY, 7),       // 0x7b - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::AbsoluteX, 4),       // 0x7c - unofficial
    Instruction("ADC", Operation::ADC, AddressingMode::AbsoluteX, 4),
    Instruction("ROR", Operation::ROR, AddressingMode::AbsoluteX, 7),
    Instruction("RRA", Operation::RRA, AddressingMode::AbsoluteX, 7),       // 0x7f - unofficial

    Instruction("SKB", Operation::SKB, AddressingMode::Immediate, 2),       // 0x80 - unofficial
    Instruction("STA", Operation::STA, AddressingMode::IndirectX, 6),
    Instruction("SKB", Operation::SKB, AddressingMode::Immediate, 2),       // 0x82 - unofficial
    Instruction("SAX", Operation::SAX, AddressingMode::IndirectX, 6),       // 0x83 - unofficial
    Instruction("STY", Operation::STY, AddressingMode::ZeroPage, 3),
    Instruction("STA", Operation::STA, AddressingMode::ZeroPage, 3),
    Instruction("STX", Operation::STX, AddressingMode::ZeroPage, 3),
    Instruction("SAX", Operation::SAX, AddressingMode::ZeroPage, 3),        // 0x87 - unofficial
    Instruction("DEY", Operation::DEY, AddressingMode::Implied, 2),
    Instruction("SKB", Operation::SKB, AddressingMode::Immediate, 2),       // 0x89 - unofficial
    Instruction("TXA", Operation::TXA, AddressingMode::Implied, 2),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("STY", Operation::STY, AddressingMode::Absolute, 4),
    Instruction("STA", Operation::STA, AddressingMode::Absolute, 4),
    Instruction("STX", Operation::STX, AddressingMode::Absolute, 4),
    Instruction("SAX", Operation::SAX, AddressingMode::Absolute, 4),        // 0x8f - unofficial

    Instruction("BCC", Operation::BCC, AddressingMode::Relative, 2),
    Instruction("STA", Operation::STA, AddressingMode::IndirectY, 6),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 6),
    Instruction("STY", Operation::STY, AddressingMode::ZeroPageX, 4),
    Instruction("STA", Operation::STA, AddressingMode::ZeroPageX, 4),
    Instruction("STX", Operation::STX, AddressingMode::ZeroPageY, 4),
    Instruction("SAX", Operation::SAX, AddressingMode::ZeroPageY, 4),       // 0x97 - unofficial
    Instruction("TYA", Operation::TYA, AddressingMode::Implied, 2),
    Instruction("STA", Operation::STA, AddressingMode::AbsoluteY, 5),
    Instruction("TXS", Operation::TXS, AddressingMode::Implied, 2),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 5),
    Instruction("STA", Operation::STA, AddressingMode::AbsoluteX, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 5),

    Instruction("LDY", Operation::LDY, AddressingMode::Immediate, 2),
    Instruction("LDA", Operation::LDA, AddressingMode::IndirectX, 6),
    Instruction("LDX", Operation::LDX, AddressingMode::Immediate, 2),
    Instruction("LAX", Operation::LAX, AddressingMode::IndirectX, 6),       // 0xa3 - unofficial
    Instruction("LDY", Operation::LDY, AddressingMode::ZeroPage, 3),
    Instruction("LDA", Operation::LDA, AddressingMode::ZeroPage, 3),
    Instruction("LDX", Operation::LDX, AddressingMode::ZeroPage, 3),
    Instruction("LAX", Operation::LAX, AddressingMode::ZeroPage, 3),        // oxa7 - unofficial
    Instruction("TAY", Operation::TAY, AddressingMode::Implied, 2),
    Instruction("LDA", Operation::LDA, AddressingMode::Immediate, 2),
    Instruction("TAX", Operation::TAX, AddressingMode::Implied, 2),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("LDY", Operation::LDY, AddressingMode::Absolute, 4),
    Instruction("LDA", Operation::LDA, AddressingMode::Absolute, 4),
    Instruction("LDX", Operation::LDX, AddressingMode::Absolute, 4),
    Instruction("LAX", Operation::LAX, AddressingMode::Absolute, 4),        // 0xaf - unofficial

    Instruction("BCS", Operation::BCS, AddressingMode::Relative, 2),
    Instruction("LDA", Operation::LDA, AddressingMode::IndirectY, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("LAX", Operation::LAX, AddressingMode::IndirectY, 5),       // 0xb3 - unofficial
    Instruction("LDY", Operation::LDY, AddressingMode::ZeroPageX, 4),
    Instruction("LDA", Operation::LDA, AddressingMode::ZeroPageX, 4),
    Instruction("LDX", Operation::LDX, AddressingMode::ZeroPageY, 4),
    Instruction("LAX", Operation::LAX, AddressingMode::ZeroPageY, 4),       // 0xb7 - unofficial
    Instruction("CLV", Operation::CLV, AddressingMode::Implied, 2),
    Instruction("LDA", Operation::LDA, AddressingMode::AbsoluteY, 4),
    Instruction("TSX", Operation::TSX, AddressingMode::Implied, 2),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 4),
    Instruction("LDY", Operation::LDY, AddressingMode::AbsoluteX, 4),
    Instruction("LDA", Operation::LDA, AddressingMode::AbsoluteX, 4),
    Instruction("LDX", Operation::LDX, AddressingMode::AbsoluteY, 4),
    Instruction("LAX", Operation::LAX, AddressingMode::AbsoluteY, 4),       // 0xbf - unofficial

    Instruction("CPY", Operation::CPY, AddressingMode::Immediate, 2),
    Instruction("CMP", Operation::CMP, AddressingMode::IndirectX, 6),
    Instruction("SKB", Operation::SKB, AddressingMode::Immediate, 2),       // 0xc2 - unofficial
    Instruction("DCP", Operation::DCP, AddressingMode::IndirectX, 8),       // 0xc3 - unofficial
    Instruction("CPY", Operation::CPY, AddressingMode::ZeroPage, 3),
    Instruction("CMP", Operation::CMP, AddressingMode::ZeroPage, 3),
    Instruction("DEC", Operation::DEC, AddressingMode::ZeroPage, 5),
    Instruction("DCP", Operation::DCP, AddressingMode::ZeroPage, 5),        // 0xc7 - unofficial
    Instruction("INY", Operation::INY, AddressingMode::Implied, 2),
    Instruction("CMP", Operation::CMP, AddressingMode::Immediate, 2),
    Instruction("DEX", Operation::DEX, AddressingMode::Implied, 2),
    Instruction("AXS", Operation::AXS, AddressingMode::Immediate, 2),       // 0xcb - unofficial
    Instruction("CPY", Operation::CPY, AddressingMode::Absolute, 4),
    Instruction("CMP", Operation::CMP, AddressingMode::Absolute, 4),
    Instruction("DEC", Operation::DEC, AddressingMode::Absolute, 6),
    Instruction("DCP", Operation::DCP, AddressingMode::Absolute, 6),        // 0xcf - unofficial

    Instruction("BNE", Operation::BNE, AddressingMode::Relative, 2),
    Instruction("CMP", Operation::CMP, AddressingMode::IndirectY, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("DCP", Operation::DCP, AddressingMode::IndirectY, 8),       // 0xd3 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPageX, 4),       // 0xd4 - unofficial
    Instruction("CMP", Operation::CMP, AddressingMode::ZeroPageX, 4),
    Instruction("DEC", Operation::DEC, AddressingMode::ZeroPageX, 6),
    Instruction("DCP", Operation::DCP, AddressingMode::ZeroPageX, 6),       // 0xd7 - unofficial
    Instruction("CLD", Operation::CLD, AddressingMode::Implied, 2),
    Instruction("CMP", Operation::CMP, AddressingMode::AbsoluteY, 4),
    Instruction("NOP", Operation::NOP, AddressingMode::Implied, 2),         // 0xda - unofficial
    Instruction("DCP", Operation::DCP, AddressingMode::AbsoluteY, 7),       // 0xdb - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::AbsoluteX, 4),       // 0xdc - unofficial
    Instruction("CMP", Operation::CMP, AddressingMode::AbsoluteX, 4),
    Instruction("DEC", Operation::DEC, AddressingMode::AbsoluteX, 7),
    Instruction("DCP", Operation::DCP, AddressingMode::AbsoluteX, 7),       // 0xdf - unofficial

    Instruction("CPX", Operation::CPX, AddressingMode::Immediate, 2),
    Instruction("SBC", Operation::SBC, AddressingMode::IndirectX, 6),
    Instruction("SKB", Operation::SKB, AddressingMode::Immediate, 2),       // 0xe2 - unofficial
    Instruction("ISC", Operation::ISC, AddressingMode::IndirectX, 8),       // 0xe3 - unofficial
    Instruction("CPX", Operation::CPX, AddressingMode::ZeroPage, 3),
    Instruction("SBC", Operation::SBC, AddressingMode::ZeroPage, 3),
    Instruction("INC", Operation::INC, AddressingMode::ZeroPage, 5),
    Instruction("ISC", Operation::ISC, AddressingMode::ZeroPage, 5),        // 0xe7 - unofficial
    Instruction("INX", Operation::INX, AddressingMode::Implied, 2),
    Instruction("SBC", Operation::SBC, AddressingMode::Immediate, 2),
    Instruction("NOP", Operation::NOP, AddressingMode::Implied, 2),
    Instruction("SBC", Operation::SBC, AddressingMode::Immediate, 2),       // 0xeb - unofficial
    Instruction("CPX", Operation::CPX, AddressingMode::Absolute, 4),
    Instruction("SBC", Operation::SBC, AddressingMode::Absolute, 4),
    Instruction("INC", Operation::INC, AddressingMode::Absolute, 6),
    Instruction("ISC", Operation::ISC, AddressingMode::Absolute, 6),        // 0xef - unofficial

    Instruction("BEQ", Operation::BEQ, AddressingMode::Relative, 2),
    Instruction("SBC", Operation::SBC, AddressingMode::IndirectY, 5),
    Instruction("???", Operation::XXX, AddressingMode::Implied, 2),
    Instruction("ISC", Operation::ISC, AddressingMode::IndirectY, 8),       // 0xf3 - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::ZeroPageX, 4),       // 0xf4 - unofficial
    Instruction("SBC", Operation::SBC, AddressingMode::ZeroPageX, 4),
    Instruction("INC", Operation::INC, AddressingMode::ZeroPageX, 6),
    Instruction("ISC", Operation::ISC, AddressingMode::ZeroPageX, 6),       // 0xf7 - unofficial
    Instruction("SED", Operation::SED, AddressingMode::Implied, 2),
    Instruction("SBC", Operation::SBC, AddressingMode::AbsoluteY, 4),
    Instruction("NOP", Operation::NOP, AddressingMode::Implied, 2),         // 0xfa - unofficial
    Instruction("ISC", Operation::ISC, AddressingMode::AbsoluteY, 7),       // 0xfb - unofficial
    Instruction("IGN", Operation::IGN, AddressingMode::AbsoluteX, 4),       // 0xfc - unofficial
    Instruction("SBC", Operation::SBC, AddressingMode::AbsoluteX, 4),
    Instruction("INC", Operation::INC, AddressingMode::AbsoluteX, 7),
    Instruction("ISC", Operation::ISC, AddressingMode::AbsoluteX, 7)        // 0xff - unofficial
];
