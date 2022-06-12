pub mod location;

pub(crate) mod branch_info;
pub(crate) mod value_info;

mod phi;
mod terminator;

mod binary;
mod unary;

mod call;
mod concat;

mod load;
mod r#move;
mod store;

use std::fmt;

pub use phi::Phi;
pub use terminator::{ConditionalJump, Return, Terminator, UnconditionalJump};

pub use binary::{Binary, BinaryOp};
pub use unary::{Unary, UnaryOp};

pub use call::Call;
pub use concat::Concat;

pub use load::{LoadConstant, LoadGlobal, LoadIndex};
pub use r#move::Move;
pub use store::{StoreGlobal, StoreIndex};

use super::value::ValueId;
use enum_as_inner::EnumAsInner;
use enum_dispatch::enum_dispatch;
use value_info::ValueInfo;

/// A struct that represents an instruction in the IR that is not a terminator or phi.
#[enum_dispatch(ValueInfo)]
#[derive(Debug, Clone, EnumAsInner)]
pub enum Inner {
    Binary(Binary),
    Unary(Unary),
    LoadConstant(LoadConstant),
    LoadGlobal(LoadGlobal),
    LoadIndex(LoadIndex),
    Move(Move),
    StoreGlobal(StoreGlobal),
    StoreIndex(StoreIndex),
    Concat(Concat),
    Call(Call),
}

impl fmt::Display for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            Self::Binary(v) => write!(f, "{}", v),
            Self::Unary(v) => write!(f, "{}", v),
            Self::LoadConstant(v) => write!(f, "{}", v),
            Self::LoadGlobal(v) => write!(f, "{}", v),
            Self::LoadIndex(v) => write!(f, "{}", v),
            Self::Move(v) => write!(f, "{}", v),
            Self::StoreGlobal(v) => write!(f, "{}", v),
            Self::StoreIndex(v) => write!(f, "{}", v),
            Self::Concat(v) => write!(f, "{}", v),
            Self::Call(v) => write!(f, "{}", v),
        }
    }
}

pub enum InstructionRef<'a> {
    Phi(&'a Phi),
    Inner(&'a Inner),
    Terminator(&'a Terminator),
}

pub enum InstructionRefMut<'a> {
    Phi(&'a mut Phi),
    Inner(&'a mut Inner),
    Terminator(&'a mut Terminator),
}
