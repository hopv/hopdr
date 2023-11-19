mod peephole;
mod printer;
pub mod syntax;
pub mod ty;

use crate::formula::Ident;
pub use syntax::{Expr, ExprKind, Function, Program, Range};
pub use ty::{Type, TypeKind};

#[derive(Clone, Debug)]
pub struct Variable {
    pub ident: Ident,
    pub ty: Type,
}

impl Variable {
    pub fn mk(ident: Ident, ty: Type) -> Self {
        Self { ident, ty }
    }
}

pub fn optimize(p: Program) -> Program {
    let p = peephole::peephole_optimize(p);
    p
}