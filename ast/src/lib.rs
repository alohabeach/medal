use derive_more::{Deref, DerefMut, From};
use enum_as_inner::EnumAsInner;
use itertools::Itertools;
use std::{
    fmt,
    ops::{Deref, DerefMut},
};

pub mod assign;
pub mod call;
pub mod global;
pub mod r#if;
pub mod index;
pub mod literal;
pub mod local;
pub mod name_gen;

use assign::*;
use call::*;
use global::*;
use index::*;
use literal::*;
use local::*;
use r#if::*;

#[derive(Debug, From, Clone)]
pub enum RValue<'a> {
    Local(Local<'a>),
    Global(Global<'a>),
    Call(Call<'a>),
    Literal(Literal<'a>),
    Index(Index<'a>),
}

impl fmt::Display for RValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RValue::Local(local) => write!(f, "{}", local),
            RValue::Global(global) => write!(f, "{}", global),
            RValue::Literal(literal) => write!(f, "{}", literal),
            RValue::Call(call) => write!(f, "{}", call),
            RValue::Index(index) => write!(f, "{}", index),
        }
    }
}

#[derive(Debug, From, Clone, EnumAsInner)]
pub enum LValue<'a> {
    Local(Local<'a>),
    Global(Global<'a>),
}

impl fmt::Display for LValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LValue::Local(local) => write!(f, "{}", local),
            LValue::Global(global) => write!(f, "{}", global),
        }
    }
}

#[derive(Debug, From, Clone)]
pub enum Statement<'a> {
    Call(Call<'a>),
    Assign(Assign<'a>),
    If(If<'a>),
}

impl fmt::Display for Statement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Statement::Call(call) => write!(f, "{}", call),
            Statement::Assign(assign) => write!(f, "{}", assign),
            Statement::If(if_) => write!(f, "{}", if_),
        }
    }
}

#[derive(Debug, Clone, Default, Deref, DerefMut)]
pub struct Block<'a>(pub Vec<Statement<'a>>);

impl<'a> Block<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_vec(statements: Vec<Statement<'a>>) -> Self {
        Self(statements)
    }
}

impl fmt::Display for Block<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0.iter().map(|node| node.to_string()).join("\n")
        )
    }
}
