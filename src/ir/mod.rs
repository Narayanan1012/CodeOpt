//! Flat three-address-code intermediate representation.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Operand {
    Var(String),
    Temp(usize),
    Const(i32),
    Label(String),
    None,
}

impl fmt::Display for Operand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Var(name) => write!(formatter, "{name}"),
            Self::Temp(index) => write!(formatter, "t{index}"),
            Self::Const(value) => write!(formatter, "{value}"),
            Self::Label(name) => write!(formatter, "{name}"),
            Self::None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpCode {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Assign,
    IfFalse,
    Goto,
    Label,
    Read,
    Print,
    Nop,
}

impl OpCode {
    pub fn from_binary_token(token: &str) -> Option<Self> {
        match token {
            "+" => Some(Self::Add),
            "-" => Some(Self::Sub),
            "*" => Some(Self::Mul),
            "/" => Some(Self::Div),
            "%" => Some(Self::Mod),
            _ => None,
        }
    }

    pub fn binary_token(self) -> Option<&'static str> {
        match self {
            Self::Add => Some("+"),
            Self::Sub => Some("-"),
            Self::Mul => Some("*"),
            Self::Div => Some("/"),
            Self::Mod => Some("%"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub id: usize,
    pub op: OpCode,
    pub arg1: Operand,
    pub arg2: Operand,
    pub dest: Operand,
    pub block_id: Option<usize>,
    pub original_line: usize,
}

impl fmt::Display for Instruction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.op {
            OpCode::Label => write!(formatter, "{}:", self.dest),
            OpCode::Goto => write!(formatter, "goto {}", self.dest),
            OpCode::IfFalse => write!(formatter, "if_false {} goto {}", self.arg1, self.dest),
            OpCode::Read => write!(formatter, "read {}", self.dest),
            OpCode::Print => write!(formatter, "print {}", self.arg1),
            OpCode::Assign => write!(formatter, "{} = {}", self.dest, self.arg1),
            OpCode::Nop => write!(formatter, "nop"),
            operation => {
                let token = operation
                    .binary_token()
                    .expect("only binary operations reach this display branch");
                write!(
                    formatter,
                    "{} = {} {token} {}",
                    self.dest, self.arg1, self.arg2
                )
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub instructions: Vec<Instruction>,
}

impl Program {
    pub fn new(instructions: Vec<Instruction>) -> Self {
        Self { instructions }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, instruction) in self.instructions.iter().enumerate() {
            if index > 0 {
                writeln!(formatter)?;
            }
            write!(formatter, "{instruction}")?;
        }
        Ok(())
    }
}
