use super::alpha::alpha_renaming;
use super::safety;
use super::transform::transform;
use super::typing::typing;
use super::Context;
use crate::formula;
use crate::formula::hes;
use crate::formula::{Fv, OpKind, PredKind, Type as SimpleType};
use crate::parse;
use crate::util::Unique;

use std::collections::HashSet;
use std::fmt;

#[derive(Debug)]
pub enum ExprKind<Id, Ty> {
    Var(Id),
    Num(i64),
    True,
    False,
    Op(OpKind, Expr<Id, Ty>, Expr<Id, Ty>),
    Pred(PredKind, Expr<Id, Ty>, Expr<Id, Ty>),
    App(Expr<Id, Ty>, Expr<Id, Ty>),
    And(Expr<Id, Ty>, Expr<Id, Ty>),
    Or(Expr<Id, Ty>, Expr<Id, Ty>),
    Abs(VariableS<Id, Ty>, Expr<Id, Ty>),
    Univ(VariableS<Id, Ty>, Expr<Id, Ty>),
}
pub type Expr<Id, Ty> = Unique<ExprKind<Id, Ty>>;

impl<Ty: fmt::Display, Id: fmt::Display> fmt::Display for Expr<Id, Ty> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            ExprKind::Var(id) => write!(f, "{}", id),
            ExprKind::Op(op, e1, e2) => write!(f, "({} {} {})", e1, op, e2),
            ExprKind::Pred(pred, e1, e2) => write!(f, "({} {} {})", e1, pred, e2),
            ExprKind::App(e1, e2) => write!(f, "({} {})", e1, e2),
            ExprKind::And(e1, e2) => write!(f, "({} & {})", e1, e2),
            ExprKind::Or(e1, e2) => write!(f, "({} | {})", e1, e2),
            ExprKind::Num(x) => write!(f, "{}", x),
            ExprKind::True => write!(f, "true"),
            ExprKind::False => write!(f, "false"),
            ExprKind::Univ(id, e) => write!(f, "∀{}. {}", id, e),
            ExprKind::Abs(x, y) => write!(f, "\\{}. {}", x, y),
        }
    }
}

impl<Id, Ty> Expr<Id, Ty> {
    pub fn mk_var(x: Id) -> Expr<Id, Ty> {
        Expr::new(ExprKind::Var(x))
    }
    pub fn mk_num(x: i64) -> Expr<Id, Ty> {
        Expr::new(ExprKind::Num(x))
    }
    pub fn mk_true() -> Expr<Id, Ty> {
        Expr::new(ExprKind::True)
    }
    pub fn mk_false() -> Expr<Id, Ty> {
        Expr::new(ExprKind::False)
    }
    pub fn mk_op(op: OpKind, e1: Expr<Id, Ty>, e2: Expr<Id, Ty>) -> Expr<Id, Ty> {
        Expr::new(ExprKind::Op(op, e1, e2))
    }
    pub fn mk_pred(pred: PredKind, e1: Expr<Id, Ty>, e2: Expr<Id, Ty>) -> Expr<Id, Ty> {
        Expr::new(ExprKind::Pred(pred, e1, e2))
    }
    pub fn mk_app(e1: Expr<Id, Ty>, e2: Expr<Id, Ty>) -> Expr<Id, Ty> {
        Expr::new(ExprKind::App(e1, e2))
    }
    pub fn mk_and(e1: Expr<Id, Ty>, e2: Expr<Id, Ty>) -> Expr<Id, Ty> {
        Expr::new(ExprKind::And(e1, e2))
    }
    pub fn mk_or(e1: Expr<Id, Ty>, e2: Expr<Id, Ty>) -> Expr<Id, Ty> {
        Expr::new(ExprKind::Or(e1, e2))
    }
    pub fn mk_univ(v: VariableS<Id, Ty>, e: Expr<Id, Ty>) -> Expr<Id, Ty> {
        Expr::new(ExprKind::Univ(v, e))
    }
    pub fn mk_abs(v: VariableS<Id, Ty>, e: Expr<Id, Ty>) -> Expr<Id, Ty> {
        Expr::new(ExprKind::Abs(v, e))
    }
}

#[derive(Clone, Debug)]
pub struct VariableS<Id, Ty> {
    pub id: Id,
    pub ty: Ty,
}
impl<Id: fmt::Display, Ty: fmt::Display> fmt::Display for VariableS<Id, Ty> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.id, self.ty)
    }
}

impl From<VariableS<formula::Ident, SimpleType>> for formula::Variable {
    fn from(v: VariableS<formula::Ident, SimpleType>) -> formula::Variable {
        formula::Variable::mk(v.id, v.ty)
    }
}

impl<Id, Ty> VariableS<Id, Ty> {
    pub fn new(id: Id, ty: Ty) -> VariableS<Id, Ty> {
        VariableS { id, ty }
    }
}

#[derive(Debug)]
pub struct Clause<Id, Ty> {
    pub id: VariableS<Id, Ty>,
    pub args: Vec<Id>,
    pub expr: Expr<Id, Ty>,
}

#[derive(Debug)]
pub struct ValidityChecking<Id, Ty> {
    pub clauses: Vec<Clause<Id, Ty>>,
    pub toplevel: Expr<Id, Ty>,
}

impl<Id: fmt::Display, Ty: fmt::Display> fmt::Display for Clause<Id, Ty> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)?;
        for arg in self.args.iter() {
            write!(f, " {}", arg)?;
        }
        write!(f, " = {}", self.expr)
    }
}

fn quantify_toplevel(mut expr: parse::Expr, clauses: &Vec<parse::Clause>) -> parse::Expr {
    let fvs = expr.fv();
    let preds: HashSet<String> = clauses.iter().map(|x| x.id.clone()).collect();

    for fv in fvs.iter() {
        if !preds.contains(fv) {
            expr = parse::Expr::mk_univ(fv.clone(), expr);
        }
    }
    expr
}

pub fn preprocess<'a>(vc: parse::Problem) -> (hes::Problem<formula::Constraint>, Context) {
    match vc {
        parse::Problem::NuHFLZValidityChecking(vc) => {
            let toplevel = quantify_toplevel(vc.toplevel, &vc.formulas);
            let problem = typing(vc.formulas, toplevel);
            let (problem, ctx) = alpha_renaming(problem);
            let problem = transform(problem);
            debug!("[problem]\n{}\n", problem);
            let problem = safety::transform(problem);
            debug!("[safety::transform]\n{}\n", problem);
            (problem, ctx)
        }
    }
}
