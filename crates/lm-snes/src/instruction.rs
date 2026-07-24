#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchCondition {
    Always,
    CarryClear,
    CarrySet,
    Equal,
    Minus,
    NotEqual,
    Plus,
    OverflowClear,
    OverflowSet,
}

impl BranchCondition {
    pub(crate) const fn opcode(self) -> u8 {
        match self {
            Self::Always => 0x80,
            Self::CarryClear => 0x90,
            Self::CarrySet => 0xb0,
            Self::Equal => 0xf0,
            Self::Minus => 0x30,
            Self::NotEqual => 0xd0,
            Self::Plus => 0x10,
            Self::OverflowClear => 0x50,
            Self::OverflowSet => 0x70,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterWidth {
    Eight,
    Sixteen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexWidth {
    Eight,
    Sixteen,
}
