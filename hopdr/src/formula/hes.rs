use crate::formula::ty::Type;
use crate::formula::{
    Bot, Constraint, FirstOrderLogic, Fv, Ident, Logic, Op, Precedence, PrecedenceKind, Rename,
    TeXPrinter, Top, Variable,
};
use crate::pdr::rtype::Refinement;
use crate::util::{Pretty, P};
use std::collections::HashSet;

use std::fmt;

use super::{fofml, Negation, Subst, TeXFormat};

#[derive(Debug)]
pub enum ConstKind {
    Int(i64),
}

pub type Const = P<ConstKind>;

impl fmt::Display for Const {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ConstKind::*;
        match self.kind() {
            Int(i) => write!(f, "{}", i),
        }
    }
}

impl Const {
    pub fn mk_int(v: i64) -> Const {
        Const::new(ConstKind::Int(v))
    }
    pub fn is_int(&self) -> bool {
        use ConstKind::*;
        match self.kind() {
            Int(_) => true,
        }
    }
    pub fn int(&self) -> i64 {
        use ConstKind::*;
        match self.kind() {
            Int(x) => *x,
        }
    }
}

#[derive(Debug)]
pub enum GoalKind<C, T> {
    Constr(C),
    Op(Op),
    Var(Ident),
    Abs(Variable, GoalBase<C, T>),
    App(GoalBase<C, T>, GoalBase<C, T>),
    Conj(GoalBase<C, T>, GoalBase<C, T>),
    Disj(GoalBase<C, T>, GoalBase<C, T>),
    Univ(Variable, GoalBase<C, T>),
    ITE(C, GoalBase<C, T>, GoalBase<C, T>),
}

#[derive(Debug)]
pub struct GoalBase<C, T> {
    ptr: P<GoalKind<C, T>>,
    pub aux: T,
}

impl<C, T> GoalBase<C, T> {
    pub fn kind(&self) -> &GoalKind<C, T> {
        &*self.ptr
    }
}

impl<C, T> GoalBase<C, T> {
    pub fn new_t(v: GoalKind<C, T>, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(v),
            aux,
        }
    }
}

impl<C> GoalBase<C, ()> {
    pub fn new(v: GoalKind<C, ()>) -> GoalBase<C, ()> {
        GoalBase::new_t(v, ())
    }
}

impl<C, T: Clone> Clone for GoalBase<C, T> {
    fn clone(&self) -> GoalBase<C, T> {
        GoalBase {
            ptr: self.ptr.clone(),
            aux: self.aux.clone(),
        }
    }
}

pub type Goal<C> = GoalBase<C, ()>;

impl<C: Pretty + Precedence, T> fmt::Display for GoalBase<C, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_display())
    }
}

impl<C: TeXFormat, T> TeXFormat for GoalBase<C, T> {
    fn tex_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self.kind() {
            GoalKind::Constr(c) => c.tex_fmt(f),
            GoalKind::Op(o) => o.tex_fmt(f),
            GoalKind::Var(v) => v.tex_fmt(f),
            GoalKind::Abs(x, g) => {
                write!(f, "(\\lambda {}. {})", TeXPrinter(x), TeXPrinter(g))
            }
            GoalKind::App(x, y) => {
                write!(f, "({}\\ {})", TeXPrinter(x), TeXPrinter(y))
            }
            GoalKind::Conj(x, y) => {
                write!(f, "({}\\land {})", TeXPrinter(x), TeXPrinter(y))
            }
            GoalKind::Disj(x, y) => {
                write!(f, "({}\\lor {})", TeXPrinter(x), TeXPrinter(y))
            }
            GoalKind::Univ(x, g) => {
                write!(f, "(\\forall {}. {})", TeXPrinter(x), TeXPrinter(g))
            }
            GoalKind::ITE(x, g1, g2) => {
                write!(
                    f,
                    "(if {} then {} else {})",
                    TeXPrinter(x),
                    TeXPrinter(g1),
                    TeXPrinter(g2)
                )
            }
        }
    }
}
impl<C: Top, T: Default> Top for GoalBase<C, T> {
    fn mk_true() -> Self {
        GoalBase::mk_constr(C::mk_true())
    }

    fn is_true(&self) -> bool {
        matches!(self.kind(), GoalKind::Constr(c) if c.is_true())
    }
}
impl<C: Bot, T: Default> Bot for GoalBase<C, T> {
    fn mk_false() -> Self {
        GoalBase::mk_constr(C::mk_false())
    }

    fn is_false(&self) -> bool {
        matches!(self.kind(), GoalKind::Constr(c) if c.is_false())
    }
}
impl<C> From<C> for GoalBase<C, ()> {
    fn from(c: C) -> Self {
        GoalBase::mk_constr(c)
    }
}

impl<T: Clone> From<GoalBase<Constraint, T>> for Constraint {
    // even though g has type *, and it can be beta-reduced to a constraint,
    // we cannot convert g to the constraint.
    // This is a naive way of translating Goal to Constraint.
    fn from(g: GoalBase<Constraint, T>) -> Self {
        match g.kind() {
            GoalKind::Constr(c) => c.clone(),
            GoalKind::Conj(g1, g2) => {
                let c1 = g1.clone().into();
                let c2 = g2.clone().into();
                Constraint::mk_conj(c1, c2)
            }
            GoalKind::Disj(g1, g2) => {
                let c1 = g1.clone().into();
                let c2 = g2.clone().into();
                Constraint::mk_disj(c1, c2)
            }
            GoalKind::Univ(x, g) => {
                let c = g.clone().into();
                Constraint::mk_quantifier_int(super::QuantifierKind::Universal, x.id, c)
            }
            GoalKind::Op(_) | GoalKind::Var(_) | GoalKind::Abs(_, _) | GoalKind::App(_, _) => {
                panic!("program error: {} cannot be translated to Constraint", g)
            }
            GoalKind::ITE(c1, g1, g2) => {
                let c1 = c1.clone();
                let c2 = g1.clone().into();
                let c3 = g2.clone().into();
                Constraint::mk_ite(c1, c2, c3)
            }
        }
    }
}
impl<C, T> From<GoalBase<C, T>> for Op {
    fn from(g: GoalBase<C, T>) -> Self {
        match g.kind() {
            GoalKind::Op(o) => o.clone(),
            GoalKind::Var(x) => Op::mk_var(*x),
            GoalKind::Constr(_)
            | GoalKind::Abs(_, _)
            | GoalKind::App(_, _)
            | GoalKind::Conj(_, _)
            | GoalKind::Disj(_, _)
            | GoalKind::Univ(_, _)
            | GoalKind::ITE(_, _, _) => panic!("program error"),
        }
    }
}

impl<C, T: Default> GoalBase<C, T> {
    pub fn mk_constr(x: C) -> GoalBase<C, T> {
        GoalBase::mk_constr_t(x, T::default())
    }
    pub fn mk_app(lhs: GoalBase<C, T>, rhs: GoalBase<C, T>) -> GoalBase<C, T> {
        GoalBase::mk_app_t(lhs, rhs, T::default())
    }
    pub fn mk_var(ident: Ident) -> GoalBase<C, T> {
        GoalBase::mk_var_t(ident, T::default())
    }
    pub fn mk_univ(x: Variable, g: GoalBase<C, T>) -> GoalBase<C, T> {
        GoalBase::mk_univ_t(x, g, T::default())
    }
    pub fn mk_abs(x: Variable, g: GoalBase<C, T>) -> GoalBase<C, T> {
        GoalBase::mk_abs_t(x, g, T::default())
    }
    pub fn mk_op(op: Op) -> GoalBase<C, T> {
        GoalBase::mk_op_t(op, T::default())
    }
    pub fn is_op(&self) -> bool {
        matches!(self.kind(), GoalKind::Op(_))
    }
    pub fn mk_ite(c: C, g1: GoalBase<C, T>, g2: GoalBase<C, T>) -> GoalBase<C, T> {
        GoalBase::mk_ite_t(c, g1, g2, T::default())
    }
    pub fn is_ite(&self) -> bool {
        matches!(self.kind(), GoalKind::ITE(_, _, _))
    }
    pub fn is_constr(&self) -> bool {
        match self.kind() {
            GoalKind::Constr(_) => true,
            GoalKind::Op(_) => panic!("program error"),
            GoalKind::Var(_) | GoalKind::Abs(_, _) | GoalKind::App(_, _) => false,
            GoalKind::Conj(g1, g2) | GoalKind::Disj(g1, g2) | GoalKind::ITE(_, g1, g2) => {
                g1.is_constr() && g2.is_constr()
            }
            GoalKind::Univ(_, g) => g.is_constr(),
        }
    }
}

impl<C: Bot + Top, T: Clone + Default> Logic for GoalBase<C, T> {
    fn mk_conj(lhs: GoalBase<C, T>, rhs: GoalBase<C, T>) -> GoalBase<C, T> {
        GoalBase::mk_conj_t(lhs, rhs, T::default())
    }

    fn mk_disj(lhs: GoalBase<C, T>, rhs: GoalBase<C, T>) -> GoalBase<C, T> {
        GoalBase::mk_disj_t(lhs, rhs, T::default())
    }

    fn is_conj(&self) -> Option<(&Self, &Self)> {
        match self.kind() {
            GoalKind::Conj(g1, g2) => Some((g1, g2)),
            _ => None,
        }
    }

    fn is_disj(&self) -> Option<(&Self, &Self)> {
        match self.kind() {
            GoalKind::Disj(g1, g2) => Some((g1, g2)),
            _ => None,
        }
    }
}
impl<C, T> GoalBase<C, T> {
    pub fn mk_constr_t(x: C, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::Constr(x)),
            aux,
        }
    }
    pub fn mk_app_t(lhs: GoalBase<C, T>, rhs: GoalBase<C, T>, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::App(lhs, rhs)),
            aux,
        }
    }
    pub fn mk_var_t(ident: Ident, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::Var(ident)),
            aux,
        }
    }
    pub fn mk_univ_t(x: Variable, g: GoalBase<C, T>, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::Univ(x, g)),
            aux,
        }
    }
    pub fn mk_abs_t(x: Variable, g: GoalBase<C, T>, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::Abs(x, g)),
            aux,
        }
    }
    pub fn mk_op_t(op: Op, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::Op(op)),
            aux,
        }
    }
    pub fn mk_conj_t(lhs: GoalBase<C, T>, rhs: GoalBase<C, T>, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::Conj(lhs, rhs)),
            aux,
        }
    }
    pub fn mk_disj_t(lhs: GoalBase<C, T>, rhs: GoalBase<C, T>, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::Disj(lhs, rhs)),
            aux,
        }
    }
    pub fn mk_ite_t(c: C, g1: GoalBase<C, T>, g2: GoalBase<C, T>, aux: T) -> GoalBase<C, T> {
        GoalBase {
            ptr: P::new(GoalKind::ITE(c, g1, g2)),
            aux,
        }
    }
    pub fn is_var(&self) -> bool {
        matches!(self.kind(), GoalKind::Var(_))
    }
    pub fn is_univ(&self) -> bool {
        matches!(self.kind(), GoalKind::Univ(_, _))
    }
    pub fn conj<'a>(&'a self) -> (&'a Self, &'a Self) {
        match self.kind() {
            GoalKind::Conj(g1, g2) => (g1, g2),
            _ => panic!("the given expr is not conj"),
        }
    }
    pub fn disj<'a>(&'a self) -> (&'a Self, &'a Self) {
        match self.kind() {
            GoalKind::Disj(g1, g2) => (g1, g2),
            _ => panic!("the given expr is not disj"),
        }
    }
    /// Returns Left value of a tuple of Constraint and Goal if the given formula is disjunction g1 \/ g2,
    /// and one of them is a constraint. Otherwise, returns Right value of a tuple of Goal and Goal.
    pub fn disj_constr<'a>(&'a self) -> either::Either<(&'a C, &'a Self), (&'a Self, &'a Self)> {
        let (g1, g2) = match self.kind() {
            GoalKind::Disj(g1, g2) => (g1, g2),
            _ => panic!("the given expr is not disj"),
        };
        match (g1.kind(), g2.kind()) {
            (GoalKind::Constr(c), _) => either::Left((c, g2)),
            (_, GoalKind::Constr(c)) => either::Left((c, g1)),
            _ => either::Right((g1, g2)),
        }
    }
    pub fn univ<'a>(&'a self) -> (&'a Variable, &'a Self) {
        match self.kind() {
            GoalKind::Univ(x, g) => (x, g),
            _ => panic!("the given expr is not univ"),
        }
    }
    pub fn abs<'a>(&'a self) -> (&'a Variable, &'a Self) {
        match self.kind() {
            GoalKind::Abs(x, g) => (x, g),
            _ => panic!("the given expr is not abs"),
        }
    }
    pub fn app<'a>(&'a self) -> (&'a Self, &'a Self) {
        match self.kind() {
            GoalKind::App(x, y) => (x, y),
            _ => panic!("the given expr is not app"),
        }
    }
    pub fn var<'a>(&'a self) -> &'a Ident {
        match self.kind() {
            GoalKind::Var(x) => x,
            _ => panic!("the given expr is not var"),
        }
    }
    pub fn atom<'a>(&'a self) -> &'a C {
        match self.kind() {
            GoalKind::Constr(c) => c,
            _ => panic!("the given expr is not atom"),
        }
    }
    pub fn unwrap_quantifiers<'a>(&'a self) -> (Vec<&'a Variable>, &'a Self) {
        match self.kind() {
            GoalKind::Univ(v, g) => {
                let (mut vec, g) = g.unwrap_quantifiers();
                vec.push(v);
                (vec, g)
            }
            _ => (Vec::new(), self),
        }
    }
    /// internal function used by vec_conj_disj_inner
    fn vec_conj_disj_inner<'a, F>(&'a self, v: &mut Vec<&'a Self>, f: &F)
    where
        F: Fn(&Self) -> either::Either<&Self, (&Self, &Self)>,
    {
        match f(self) {
            either::Either::Left(g) => v.push(g),
            either::Either::Right((g1, g2)) => {
                g1.vec_conj_disj_inner(v, f);
                g2.vec_conj_disj_inner(v, f);
            }
        }
    }
    pub fn vec_of_conjs(&self) -> Vec<&Self> {
        let mut r = Vec::new();
        self.vec_conj_disj_inner(&mut r, &|g| match g.kind() {
            GoalKind::Constr(_)
            | GoalKind::Op(_)
            | GoalKind::Var(_)
            | GoalKind::Abs(_, _)
            | GoalKind::App(_, _)
            | GoalKind::Disj(_, _)
            | GoalKind::Univ(_, _)
            | GoalKind::ITE(_, _, _) => either::Left(g),
            GoalKind::Conj(g1, g2) => either::Right((g1, g2)),
        });
        r
    }
    pub fn vec_of_disjs(&self) -> Vec<&Self> {
        let mut r = Vec::new();
        self.vec_conj_disj_inner(&mut r, &|g| match g.kind() {
            GoalKind::Constr(_)
            | GoalKind::Op(_)
            | GoalKind::Var(_)
            | GoalKind::Abs(_, _)
            | GoalKind::App(_, _)
            | GoalKind::Conj(_, _)
            | GoalKind::Univ(_, _)
            | GoalKind::ITE(_, _, _) => either::Left(g),
            GoalKind::Disj(g1, g2) => either::Right((g1, g2)),
        });
        r
    }
}
impl<C> Goal<C> {
    pub fn wrap_quantifiers<'a>(self, vs: Vec<&'a Variable>) -> Self {
        vs.into_iter()
            .rev()
            .fold(self, |g, v| GoalBase::mk_univ(v.clone(), g))
    }
}
impl<C, T> GoalBase<C, T> {
    pub fn is_conj(&self) -> bool {
        matches!(self.kind(), GoalKind::Conj(_, _))
    }
    pub fn is_disj(&self) -> bool {
        matches!(self.kind(), GoalKind::Disj(_, _))
    }
}

impl<C: super::FormulaSize, T> super::FormulaSize for GoalBase<C, T> {
    fn formula_size(&self) -> usize {
        match self.kind() {
            GoalKind::Constr(c) => c.formula_size(),
            GoalKind::Op(o) => o.formula_size(),
            GoalKind::Var(_) => 1,
            GoalKind::Abs(_, g) => 1 + g.formula_size(),
            GoalKind::App(g1, g2) => g1.formula_size() + g2.formula_size(),
            GoalKind::Conj(g1, g2) => g1.formula_size() + g2.formula_size(),
            GoalKind::Disj(g1, g2) => g1.formula_size() + g2.formula_size(),
            GoalKind::Univ(_, g) => 1 + g.formula_size(),
            GoalKind::ITE(_, g1, g2) => 1 + g1.formula_size() + g2.formula_size(),
        }
    }
}
impl<C: Bot + Top + Logic> Goal<C> {
    pub fn mk_ho_disj(fmls: &[Goal<C>], mut sty: Type) -> Goal<C> {
        let mut vs = Vec::new();
        loop {
            sty = match sty.kind() {
                super::TypeKind::Proposition => break,
                super::TypeKind::Arrow(t, s) => {
                    vs.push(Variable::mk(Ident::fresh(), t.clone()));
                    s.clone()
                }
                super::TypeKind::Integer | super::TypeKind::Bit => panic!("program error"),
            };
        }
        let mut x = Goal::mk_false();
        for f in fmls {
            let mut fml = f.clone();
            for v in vs.iter() {
                fml = Goal::mk_app(fml, Goal::mk_var(v.id));
            }
            x = Goal::mk_disj(x, fml);
        }
        for v in vs.iter().rev() {
            x = Goal::mk_abs(v.clone(), x);
        }
        x
    }
    /// If self is "trivially" true, then it returns true.
    /// Otherwise, it's unknown.
    pub fn is_true(&self) -> bool {
        match self.kind() {
            GoalKind::Constr(c) => c.is_true(),
            GoalKind::Conj(c1, c2) => c1.is_true() && c2.is_true(),
            GoalKind::Disj(c1, c2) => c1.is_true() || c2.is_true(),
            GoalKind::Univ(_, c) => c.is_true(),
            _ => false,
        }
    }

    /// If self is "trivially" false, then it returns true.
    /// Otherwise, it's unknown.
    pub fn is_false(&self) -> bool {
        match self.kind() {
            GoalKind::Constr(c) => c.is_false(),
            GoalKind::Conj(c1, c2) => c1.is_false() || c2.is_false(),
            GoalKind::Disj(c1, c2) => c1.is_false() && c2.is_false(),
            GoalKind::Univ(_, c) => c.is_false(),
            _ => false,
        }
    }

    /// This returns a formula that is equivalent to the conjunction of `lhs` and `rhs`.
    pub fn mk_conj_opt(lhs: Goal<C>, rhs: Goal<C>) -> Goal<C> {
        if lhs.is_true() {
            rhs
        } else if rhs.is_true() {
            lhs
        } else if lhs.is_false() || rhs.is_false() {
            Goal::mk_false()
        } else {
            Goal::mk_conj(lhs, rhs)
        }
    }

    /// This returns a formula that is equivalent to the disjunction of `lhs` and `rhs`.
    pub fn mk_disj_opt(lhs: Goal<C>, rhs: Goal<C>) -> Goal<C> {
        if lhs.is_false() {
            rhs
        } else if rhs.is_false() {
            lhs
        } else if lhs.is_true() || rhs.is_true() {
            Goal::mk_true()
        } else {
            Goal::mk_disj(lhs, rhs)
        }
    }

    /// This returns a formula that is equivalent to the universal quantification of `x` in `g`.
    pub fn mk_univ_opt(x: Variable, g: Goal<C>) -> Goal<C> {
        if g.is_true() {
            GoalBase::mk_true()
        } else if g.is_false() {
            GoalBase::mk_false()
        } else {
            GoalBase::mk_univ(x, g)
        }
    }
    pub fn mk_ite_opt(c: C, g1: Self, g2: Self) -> Self {
        if c.is_true() {
            g1
        } else if c.is_false() {
            g2
        } else if g1.is_false() && g2.is_false() || g1.is_true() && g2.is_true() {
            g1
        } else {
            GoalBase::mk_ite(c, g1, g2)
        }
    }
}

impl<C: Negation, T: Default> GoalBase<C, T> {
    pub fn mk_imply_t(lhs: C, rhs: GoalBase<C, T>, aux: T) -> Option<GoalBase<C, T>> {
        lhs.negate()
            .map(|lhs| Self::mk_disj_t(Self::mk_constr_t(lhs, T::default()), rhs, aux))
    }
}

impl<C: Fv<Id = Ident>, T> Fv for GoalBase<C, T> {
    type Id = Ident;

    fn fv_with_vec(&self, fvs: &mut HashSet<Self::Id>) {
        match self.kind() {
            GoalKind::Var(x) => {
                fvs.insert(*x);
            }
            GoalKind::Univ(x, g) | GoalKind::Abs(x, g) => {
                g.fv_with_vec(fvs);
                fvs.remove(&x.id);
            }
            GoalKind::App(g1, g2) | GoalKind::Conj(g1, g2) | GoalKind::Disj(g1, g2) => {
                g1.fv_with_vec(fvs);
                g2.fv_with_vec(fvs);
            }
            GoalKind::Constr(c) => c.fv_with_vec(fvs),
            GoalKind::Op(op) => op.fv_with_vec(fvs),
            GoalKind::ITE(c, g1, g2) => {
                c.fv_with_vec(fvs);
                g1.fv_with_vec(fvs);
                g2.fv_with_vec(fvs);
            }
        }
    }
}
impl<C: Rename, T: Clone> Rename for GoalBase<C, T> {
    fn rename(&self, x: &Ident, y: &Ident) -> Self {
        match self.kind() {
            GoalKind::Constr(c) => GoalBase::mk_constr_t(c.rename(x, y), self.aux.clone()),
            GoalKind::Op(op) => GoalBase::mk_op_t(op.rename(x, y), self.aux.clone()),
            GoalKind::Var(id) => {
                GoalBase::mk_var_t(if id == x { *y } else { *id }, self.aux.clone())
            }
            GoalKind::Abs(id, g) => {
                let g = if &id.id != x {
                    g.rename(x, y)
                } else {
                    g.clone()
                };
                GoalBase::mk_abs_t(id.clone(), g, self.aux.clone())
            }
            GoalKind::Univ(id, g) => {
                let g = if &id.id != x {
                    g.rename(x, y)
                } else {
                    g.clone()
                };
                GoalBase::mk_univ_t(id.clone(), g, self.aux.clone())
            }
            GoalKind::App(g1, g2) => {
                GoalBase::mk_app_t(g1.rename(x, y), g2.rename(x, y), self.aux.clone())
            }
            GoalKind::Conj(g1, g2) => {
                GoalBase::mk_conj_t(g1.rename(x, y), g2.rename(x, y), self.aux.clone())
            }
            GoalKind::Disj(g1, g2) => {
                GoalBase::mk_disj_t(g1.rename(x, y), g2.rename(x, y), self.aux.clone())
            }
            GoalKind::ITE(c, g1, g2) => GoalBase::mk_ite_t(
                c.rename(x, y),
                g1.rename(x, y),
                g2.rename(x, y),
                self.aux.clone(),
            ),
        }
    }
}

impl<
        C: Subst<Item = Op, Id = Ident> + Rename + Fv<Id = Ident> + Precedence + Pretty,
        T: Clone + Default,
    > GoalBase<C, T>
{
    pub fn isubst(&self, x: &Ident, v: &Op) -> GoalBase<C, T> {
        match self.kind() {
            GoalKind::Constr(c) => Self::mk_constr_t(c.subst(x, v), self.aux.clone()),
            GoalKind::Op(o) => Self::mk_op_t(o.subst(x, v), self.aux.clone()),
            GoalKind::Var(y) if x == y => Self::mk_op(v.clone()),
            GoalKind::Var(_) => self.clone(),
            GoalKind::Abs(y, _) if x == &y.id => self.clone(),
            GoalKind::Abs(y, g) => {
                let g = g.isubst(x, v);
                Self::mk_abs_t(y.clone(), g, self.aux.clone())
            }
            GoalKind::App(g1, g2) => {
                let g1 = g1.isubst(x, v);
                let g2 = g2.isubst(x, v);
                Self::mk_app_t(g1, g2, self.aux.clone())
            }
            GoalKind::Conj(g1, g2) => {
                let g1 = g1.isubst(x, v);
                let g2 = g2.isubst(x, v);
                Self::mk_conj_t(g1, g2, self.aux.clone())
            }
            GoalKind::Disj(g1, g2) => {
                let g1 = g1.isubst(x, v);
                let g2 = g2.isubst(x, v);
                Self::mk_disj_t(g1, g2, self.aux.clone())
            }
            GoalKind::Univ(y, _) if &y.id == x => self.clone(),
            GoalKind::Univ(y, g) => {
                let g = g.isubst(x, v);
                Self::mk_univ_t(y.clone(), g, self.aux.clone())
            }
            GoalKind::ITE(c, g1, g2) => {
                let c = c.subst(x, v);
                let g1 = g1.isubst(x, v);
                let g2 = g2.isubst(x, v);
                Self::mk_ite_t(c, g1, g2, self.aux.clone())
            }
        }
    }
}

impl<C: Subst<Item = Op, Id = Ident> + Rename + Fv<Id = Ident> + Precedence + Pretty, T: Clone>
    Subst for GoalBase<C, T>
{
    type Item = GoalBase<C, T>;
    type Id = Variable;
    // we assume formula has already been alpha-renamed
    // TODO: where? We will not assume alpha-renamed
    fn subst(&self, x: &Variable, v: &GoalBase<C, T>) -> Self {
        fn subst_inner<
            C: Subst<Item = Op, Id = Ident> + Rename + Pretty + Precedence + Fv<Id = Ident>,
            T: Clone,
        >(
            target: &GoalBase<C, T>,
            x: &Variable,
            v: &GoalBase<C, T>,
            fv: &HashSet<Ident>,
        ) -> GoalBase<C, T> {
            // tmp debug
            //debug!("subst_inner: [{}/{}]{}", v, x, target);
            match target.kind() {
                GoalKind::Var(y) => {
                    if x.id == *y {
                        v.alpha_renaming()
                        //v.clone()
                    } else {
                        target.clone()
                    }
                }
                GoalKind::Constr(c) if x.ty.is_int() => {
                    // when x has type int, v can be reduced to Op
                    let op = v.clone().into();
                    let c = c.subst(&x.id, &op);
                    GoalBase::mk_constr_t(c, target.aux.clone())
                }
                GoalKind::Op(o) if x.ty.is_int() => {
                    let op = v.clone().into();
                    debug!("op subst {:?}", o);
                    let o = o.subst(&x.id, &op);
                    debug!("op subst {:?}", o);
                    GoalBase::mk_op_t(o, target.aux.clone())
                }
                GoalKind::Constr(_) | GoalKind::Op(_) => target.clone(),
                GoalKind::App(g1, g2) => {
                    let g1 = subst_inner(g1, x, v, fv);
                    let g2 = subst_inner(g2, x, v, fv);
                    GoalBase::mk_app_t(g1, g2, target.aux.clone())
                }
                GoalKind::Conj(g1, g2) => {
                    let g1 = subst_inner(g1, x, v, fv);
                    let g2 = subst_inner(g2, x, v, fv);
                    GoalBase::mk_conj_t(g1, g2, target.aux.clone())
                }
                GoalKind::Disj(g1, g2) => {
                    let g1 = subst_inner(g1, x, v, fv);
                    let g2 = subst_inner(g2, x, v, fv);
                    GoalBase::mk_disj_t(g1, g2, target.aux.clone())
                }
                GoalKind::Abs(y, g) => {
                    if y.id == x.id {
                        target.clone()
                    } else if fv.contains(&y.id) {
                        let y2_ident = Ident::fresh();
                        let y2 = Variable::mk(y2_ident, y.ty.clone());
                        let g = g.rename(&y.id, &y2_ident);
                        GoalBase::mk_abs_t(y2, subst_inner(&g, x, v, fv), target.aux.clone())
                    } else {
                        GoalBase::mk_abs_t(y.clone(), subst_inner(g, x, v, fv), target.aux.clone())
                    }
                }
                GoalKind::Univ(y, g) => {
                    if y.id == x.id {
                        target.clone()
                    } else if fv.contains(&y.id) {
                        let y2_ident = Ident::fresh();
                        let y2 = Variable::mk(y2_ident, y.ty.clone());
                        let g = g.rename(&y.id, &y2_ident);
                        GoalBase::mk_univ_t(y2, subst_inner(&g, x, v, fv), target.aux.clone())
                    } else {
                        GoalBase::mk_univ_t(y.clone(), subst_inner(g, x, v, fv), target.aux.clone())
                    }
                }
                GoalKind::ITE(c, g1, g2) => {
                    let g1 = subst_inner(g1, x, v, fv);
                    let g2 = subst_inner(g2, x, v, fv);
                    GoalBase::mk_ite_t(c.clone(), g1, g2, target.aux.clone())
                }
            }
        }
        let fv = v.clone().fv();
        // debug
        crate::title!("fvs:");
        for f in fv.iter() {
            debug!("- {}", f)
        }
        crate::title!("subst");
        debug!("subst: [{}/{}]", v, x);
        debug!("{}", self);
        let r = subst_inner(self, x, v, &fv);
        debug!("{}", r);
        debug!("substed");
        r
    }
}

impl<C: Rename + Fv<Id = Ident> + Clone, T: Clone> GoalBase<C, T> {
    pub fn alpha_renaming(&self) -> GoalBase<C, T> {
        fn aux<C: Rename, T: Clone>(
            v: &Variable,
            g: &GoalBase<C, T>,
        ) -> (Variable, GoalBase<C, T>) {
            let id = Ident::fresh();
            let g = g.rename(&v.id, &id);
            (Variable::mk(id, v.ty.clone()), g)
        }
        match self.kind() {
            GoalKind::Constr(_) | GoalKind::Op(_) | GoalKind::Var(_) => self.clone(),
            GoalKind::Abs(v, g) => {
                let (v, g) = aux(v, g);
                let g = g.alpha_renaming();
                GoalBase::mk_abs_t(v, g, self.aux.clone())
            }
            GoalKind::Univ(v, g) => {
                let (v, g) = aux(v, g);
                let g = g.alpha_renaming();
                GoalBase::mk_univ_t(v, g, self.aux.clone())
            }
            GoalKind::App(g1, g2) => {
                let g1 = g1.alpha_renaming();
                let g2 = g2.alpha_renaming();
                GoalBase::mk_app_t(g1, g2, self.aux.clone())
            }
            GoalKind::Conj(g1, g2) => {
                let g1 = g1.alpha_renaming();
                let g2 = g2.alpha_renaming();
                GoalBase::mk_conj_t(g1, g2, self.aux.clone())
            }
            GoalKind::Disj(g1, g2) => {
                let g1 = g1.alpha_renaming();
                let g2 = g2.alpha_renaming();
                GoalBase::mk_disj_t(g1, g2, self.aux.clone())
            }
            GoalKind::ITE(c, g1, g2) => {
                let g1 = g1.alpha_renaming();
                let g2 = g2.alpha_renaming();
                GoalBase::mk_ite_t(c.clone(), g1, g2, self.aux.clone())
            }
        }
    }
}

impl<C: Refinement> PartialEq for GoalKind<C, ()> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (GoalKind::Constr(c), GoalKind::Constr(c2)) => c == c2,
            (GoalKind::Op(o), GoalKind::Op(o2)) => o == o2,
            (GoalKind::Var(x), GoalKind::Var(y)) => x == y,
            (GoalKind::Abs(v, g), GoalKind::Abs(v2, g2))
            | (GoalKind::Univ(v, g), GoalKind::Univ(v2, g2)) => v == v2 && g == g2,
            (GoalKind::App(x1, y1), GoalKind::App(x2, y2))
            | (GoalKind::Conj(x1, y1), GoalKind::Conj(x2, y2))
            | (GoalKind::Disj(x1, y1), GoalKind::Disj(x2, y2)) => x1 == x2 && y1 == y2,
            (_, _) => false,
        }
    }
}

impl<C: Refinement> PartialEq for Goal<C> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T, C: Precedence> Precedence for GoalBase<C, T> {
    fn precedence(&self) -> PrecedenceKind {
        match self.kind() {
            GoalKind::Constr(c) => c.precedence(),
            GoalKind::Op(o) => o.precedence(),
            GoalKind::Var(_) => PrecedenceKind::Atom,
            GoalKind::Univ(_, _) | GoalKind::Abs(_, _) => PrecedenceKind::Abs,
            GoalKind::App(_, _) => PrecedenceKind::App,
            GoalKind::Conj(_, _) => PrecedenceKind::Conj,
            GoalKind::Disj(_, _) => PrecedenceKind::Disj,
            GoalKind::ITE(_, _, _) => PrecedenceKind::If,
        }
    }
}

// TODO: fix not to use Refinement
impl<C: Refinement> Goal<C> {
    fn reduce_inner(&self) -> Goal<C> {
        match self.kind() {
            GoalKind::App(g, arg) => {
                // g must be have form \x. phi
                let g = g.reduce_inner();
                let arg = arg.reduce_inner();
                match g.kind() {
                    GoalKind::Abs(x, g) => g.subst(x, &arg),
                    _ => GoalBase::mk_app(g, arg),
                }
            }
            GoalKind::Conj(g1, g2) => {
                let g1 = g1.reduce_inner();
                let g2 = g2.reduce_inner();
                GoalBase::mk_conj(g1, g2)
            }
            GoalKind::Disj(g1, g2) => {
                let g1 = g1.reduce_inner();
                let g2 = g2.reduce_inner();
                GoalBase::mk_disj(g1, g2)
            }
            GoalKind::Univ(x, g) => {
                let g = g.reduce_inner();
                GoalBase::mk_univ(x.clone(), g)
            }
            GoalKind::Abs(x, g) => {
                let g = g.reduce_inner();
                GoalBase::mk_abs(x.clone(), g)
            }
            GoalKind::Constr(_) | GoalKind::Var(_) | GoalKind::Op(_) => self.clone(),
            GoalKind::ITE(c, g1, g2) => {
                let g1 = g1.reduce_inner();
                let g2 = g2.reduce_inner();
                GoalBase::mk_ite(c.clone(), g1, g2)
            }
        }
    }
    // since there is no recursion and g is strongly typed, this procedure
    // always terminates
    pub fn reduce_goal(&self) -> Goal<C> {
        // first reduces the formula to a formula of type *
        // then traslates it to a fofml::Atom constraint.
        // debug!("to be reduced: {}", &self);
        crate::title!("reduce_goal");
        debug!("reduce {}", self);
        let mut g_old = self.clone();
        loop {
            let g = g_old.reduce_inner();
            if g == g_old {
                // debug
                // debug!("reduced: {}", &g);
                return g;
            }
            debug!("⇝ {}", g);
            g_old = g;
        }
    }
    // until it reaches the beta normal form
    pub fn reduce(&self) -> C {
        self.reduce_goal().into()
    }
    pub fn quantify(&self, vs: &[Variable]) -> Goal<C> {
        let fv = self.fv();
        let mut result = self.clone();
        for v in vs.iter().rev() {
            if fv.contains(&v.id) {
                result = Goal::mk_univ(v.clone(), result);
            }
        }
        result
    }
    pub fn to_cnf(&self) -> Vec<Goal<C>> {
        let (vs, pnf) = self.prenex_normal_form_raw(&mut HashSet::new());
        let cnf = pnf.to_cnf_inner();
        cnf.into_iter().map(|g| g.quantify(&vs)).collect()
    }
}
impl Goal<Constraint> {
    pub fn count_quantifier(&self) -> usize {
        match self.kind() {
            GoalKind::Constr(c) => c.count_quantifier(),
            GoalKind::Op(_) | GoalKind::Var(_) => 0,
            GoalKind::Abs(_, g) => g.count_quantifier(),
            GoalKind::Univ(_, g) => 1 + g.count_quantifier(),
            GoalKind::App(g1, g2) | GoalKind::Conj(g1, g2) | GoalKind::Disj(g1, g2) => {
                g1.count_quantifier() + g2.count_quantifier()
            }
            GoalKind::ITE(c1, g1, g2) => {
                c1.count_quantifier() + g1.count_quantifier() + g2.count_quantifier()
            }
        }
    }
    pub fn simplify(&self) -> Self {
        match self.kind() {
            GoalKind::Constr(c) => Goal::mk_constr(c.simplify()),
            GoalKind::Op(o) => Goal::mk_op(o.simplify()),
            GoalKind::Var(_) => self.clone(),
            GoalKind::Abs(v, g) => {
                let g = g.simplify();
                Goal::mk_abs(v.clone(), g)
            }
            GoalKind::App(g1, g2) => {
                let g1 = g1.simplify();
                let g2 = g2.simplify();
                match g1.kind() {
                    GoalKind::Abs(v, g) => g.subst(v, &g2).simplify(),
                    _ => Goal::mk_app(g1, g2),
                }
            }
            GoalKind::Conj(g1, g2) => {
                let g1 = g1.simplify();
                let g2 = g2.simplify();
                Goal::mk_conj_opt(g1, g2)
            }
            GoalKind::Disj(g1, g2) => {
                let g1 = g1.simplify();
                let g2 = g2.simplify();
                Goal::mk_disj_opt(g1, g2)
            }
            GoalKind::Univ(v, g) => {
                let g = g.simplify();
                Goal::mk_univ_opt(v.clone(), g)
            }
            GoalKind::ITE(c, g1, g2) => {
                let c = c.simplify();
                let g1 = g1.simplify();
                let g2 = g2.simplify();
                Goal::mk_ite_opt(c, g1, g2)
            }
        }
    }
}
impl<C: Refinement, T: Clone + Default> GoalBase<C, T> {
    pub fn prenex_normal_form_raw(
        &self,
        env: &mut HashSet<Ident>,
    ) -> (Vec<Variable>, GoalBase<C, T>) {
        match self.kind() {
            GoalKind::Op(_)
            | GoalKind::Var(_)
            | GoalKind::Abs(_, _)
            | GoalKind::App(_, _)
            | GoalKind::Constr(_) => (Vec::new(), self.clone()),
            GoalKind::Conj(a1, a2) => {
                let (mut v1, a1) = a1.prenex_normal_form_raw(env);
                let (mut v2, a2) = a2.prenex_normal_form_raw(env);
                v1.append(&mut v2);
                (v1, GoalBase::mk_conj_t(a1, a2, self.aux.clone()))
            }
            GoalKind::Disj(a1, a2) => {
                let (mut v1, a1) = a1.prenex_normal_form_raw(env);
                let (mut v2, a2) = a2.prenex_normal_form_raw(env);
                v1.append(&mut v2);
                (v1, GoalBase::mk_disj_t(a1, a2, self.aux.clone()))
            }
            GoalKind::Univ(x, a) => {
                let (x, a) = if env.contains(&x.id) {
                    // if env already contains the ident to be bound,
                    // we rename it to a fresh one.
                    let x2_ident = Ident::fresh();
                    let x2 = Variable::mk(x2_ident, x.ty.clone());
                    let a = a.rename(&x.id, &x2.id);
                    (x2, a)
                } else {
                    (x.clone(), a.clone())
                };
                env.insert(x.id);
                let (mut v, a) = a.prenex_normal_form_raw(env);
                debug_assert!(!v.iter().any(|y| { x.id == y.id }));
                env.remove(&x.id);
                v.push(x);
                (v, a)
            }
            GoalKind::ITE(c, g1, g2) => {
                let c = c.clone();
                let (mut v1, g1) = g1.prenex_normal_form_raw(env);
                let (mut v2, g2) = g2.prenex_normal_form_raw(env);
                v1.append(&mut v2);
                (v1, GoalBase::mk_ite_t(c, g1, g2, self.aux.clone()))
            }
        }
    }

    // assumption: prenex normal form without universal quantifiers
    pub fn to_cnf_inner(&self) -> Vec<GoalBase<C, T>> {
        fn cross_or<T: Clone + Default, C: Refinement>(
            v1: &[GoalBase<C, T>],
            v2: &[GoalBase<C, T>],
        ) -> Vec<GoalBase<C, T>> {
            let mut v = Vec::new();
            for x in v1 {
                for y in v2 {
                    v.push(GoalBase::mk_disj_t(x.clone(), y.clone(), T::default()));
                }
            }
            v
        }
        match self.kind() {
            GoalKind::Conj(x, y) => {
                let mut v1 = x.to_cnf_inner();
                let mut v2 = y.to_cnf_inner();
                v1.append(&mut v2);
                v1
            }
            GoalKind::Disj(x, y) => {
                let v1 = x.to_cnf_inner();
                let v2 = y.to_cnf_inner();
                cross_or(&v1, &v2)
            }
            // TODO: implement cnf for ite
            // after ite is introduced (which happens in preprocess::find_ite), not sure which is better:
            // 1. unfold ite to disj & conj, then apply the above rule
            // 2. see ite as one component
            GoalKind::ITE(_, _, _) => unimplemented!(),
            GoalKind::Constr(_)
            | GoalKind::Op(_)
            | GoalKind::Var(_)
            | GoalKind::Abs(_, _)
            | GoalKind::App(_, _)
            | GoalKind::Univ(_, _) => vec![self.clone()],
        }
    }
}

impl<C, T> GoalBase<C, T> {
    pub fn order(&self) -> usize {
        match self.kind() {
            // if order(Var(_)) > 0, then \x. ... has bigger order than that.
            GoalKind::Constr(_) | GoalKind::Op(_) | GoalKind::Var(_) => 0,
            GoalKind::Abs(x, y) => std::cmp::max(x.order() + 1, y.order()),
            GoalKind::App(x, y)
            | GoalKind::Conj(x, y)
            | GoalKind::Disj(x, y)
            | GoalKind::ITE(_, x, y) => std::cmp::max(x.order(), y.order()),
            GoalKind::Univ(_, y) => y.order(),
        }
    }
    // returns ident of Abs(ident, x). If self is not Abs(_), abs_var panics.
    pub fn abs_var(&self) -> &Variable {
        match self.kind() {
            GoalKind::Abs(x, _) => x,
            _ => panic!("abs_var assumes that self.kind() is Abs(_, _)."),
        }
    }
    pub fn check_int_expr(&self, ienv: &HashSet<Ident>) -> Option<Op> {
        match self.kind() {
            GoalKind::Op(o) => Some(o.clone()),
            GoalKind::Var(x) if ienv.contains(x) => Some(Op::mk_var(*x)),
            GoalKind::Var(_)
            | GoalKind::Constr(_)
            | GoalKind::Abs(_, _)
            | GoalKind::App(_, _)
            | GoalKind::Conj(_, _)
            | GoalKind::Disj(_, _)
            | GoalKind::Univ(_, _)
            | GoalKind::ITE(_, _, _) => None,
        }
    }
    pub fn disjunctive_degree(&self) -> usize {
        match self.kind() {
            GoalKind::Constr(_) | GoalKind::Op(_) => 0,
            GoalKind::Var(_) => 1,
            GoalKind::Abs(_, g) | GoalKind::Univ(_, g) => std::cmp::max(1, g.disjunctive_degree()),
            GoalKind::App(g1, g2) | GoalKind::Conj(g1, g2) | GoalKind::ITE(_, g1, g2) => {
                std::cmp::max(
                    1,
                    std::cmp::max(g1.disjunctive_degree(), g2.disjunctive_degree()),
                )
            }
            GoalKind::Disj(g1, g2) => g1.disjunctive_degree() + g2.disjunctive_degree(),
        }
    }
}

#[test]
fn test_disjunctive_degree() {
    let x = Ident::fresh();
    let f = Ident::fresh();
    let g: Goal<Constraint> = Goal::mk_app(Goal::mk_var(f), Goal::mk_var(x));
    let g2 = Goal::mk_conj(g.clone(), g.clone());
    let g3 = Goal::mk_disj(g.clone(), g.clone());
    let g4 = Goal::mk_disj(g.clone(), g2.clone());
    let g5 = Goal::mk_disj(g3.clone(), g3.clone());
    let table = vec![(g, 1), (g2, 1), (g3, 2), (g4, 2), (g5, 4)];
    for (g, expected) in table.iter() {
        assert_eq!(g.disjunctive_degree(), *expected);
    }
}

impl<C: Refinement, T: Clone> Into<Option<C>> for GoalBase<C, T> {
    fn into(self) -> Option<C> {
        match self.kind() {
            GoalKind::Constr(c) => Some(c.clone()),
            GoalKind::Abs(_, _) | GoalKind::App(_, _) | GoalKind::Op(_) | GoalKind::Var(_) => None,
            GoalKind::Conj(g1, g2) => Into::<Option<C>>::into(g1.clone())
                .and_then(|c1| Into::<Option<C>>::into(g2.clone()).map(|c2| C::mk_conj(c1, c2))),
            GoalKind::Disj(g1, g2) => Into::<Option<C>>::into(g1.clone())
                .and_then(|c1| Into::<Option<C>>::into(g2.clone()).map(|c2| C::mk_disj(c1, c2))),
            GoalKind::Univ(v, g) if v.ty.is_int() => Into::<Option<C>>::into(g.clone())
                .map(|c| C::mk_quantifier_int(super::QuantifierKind::Universal, v.id, c)),
            GoalKind::Univ(_, _) => None,
            GoalKind::ITE(c, g1, g2) => {
                let c = c.clone();
                Into::<Option<C>>::into(g1.clone()).and_then(|c1| {
                    Into::<Option<C>>::into(g2.clone()).map(|c2| C::mk_ite(c, c1, c2))
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClauseBase<C, Aux> {
    pub body: GoalBase<C, Aux>,
    pub head: Variable,
    // pub fixpoint: Fixpoint
}

pub type Clause<C> = ClauseBase<C, ()>;

impl From<Clause<Constraint>> for Clause<fofml::Atom> {
    fn from(c: Clause<Constraint>) -> Self {
        Clause {
            body: c.body.into(),
            head: c.head,
        }
    }
}

impl PartialEq for Clause<Constraint> {
    fn eq(&self, other: &Self) -> bool {
        self.body == other.body && self.head == other.head
    }
}

impl<C: Fv<Id = Ident>> Fv for Clause<C> {
    type Id = Ident;

    fn fv_with_vec(&self, fvs: &mut std::collections::HashSet<Self::Id>) {
        self.body.fv_with_vec(fvs);
    }
}

impl<C: Pretty + Precedence> fmt::Display for Clause<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_display())
    }
}

impl<C: TeXFormat> TeXFormat for Clause<C> {
    fn tex_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{} ", TeXPrinter(&self.head))?;
        write!(f, "= {}", TeXPrinter(&self.body))
    }
}

impl<C> Clause<C> {
    pub fn new(body: Goal<C>, head: Variable) -> Clause<C> {
        Clause { body, head }
    }
    pub fn new_top_clause(body: Goal<C>) -> Clause<C> {
        let head = Variable::fresh_prop();
        Clause { body, head }
    }
    pub fn order(&self) -> usize {
        self.body.order()
    }
    pub fn disjunctive_degree(&self) -> usize {
        self.body.disjunctive_degree()
    }
}

#[derive(Debug, Clone)]
pub struct ProblemBase<C, Aux> {
    pub clauses: Vec<ClauseBase<C, Aux>>,
    pub top: GoalBase<C, Aux>,
}

pub type Problem<C> = ProblemBase<C, ()>;

impl From<Problem<Constraint>> for Problem<fofml::Atom> {
    fn from(p: Problem<Constraint>) -> Self {
        let clauses = p.clauses.into_iter().map(|x| x.into()).collect();
        let top = p.top.into();
        Problem { clauses, top }
    }
}

impl<C: Pretty + Precedence> fmt::Display for Problem<C> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.pretty_display())
    }
}

impl PartialEq for Problem<Constraint> {
    fn eq(&self, other: &Self) -> bool {
        self.clauses == other.clauses && self.top == other.top
    }
}

impl<C: TeXFormat> TeXFormat for Problem<C> {
    fn tex_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        writeln!(f, r"\begin{{align*}}")?;
        writeln!(f, r"\pdrtop := {}\\", TeXPrinter(&self.top))?;
        for c in self.clauses.iter() {
            writeln!(f, r"{} \\", TeXPrinter(c))?;
        }
        writeln!(f, r"\end{{align*}}")
    }
}

impl<C: Rename + Bot + Top + Clone> Problem<C> {
    // [ψ₁/F₁, ψ₂/F₂ … ]ψ
    pub fn eval(&self, target: &Goal<C>) -> Goal<C> {
        match target.kind() {
            GoalKind::Var(y) => match self.get_clause(y) {
                Some(c) => c.body.clone(),
                None => target.clone(),
            },
            GoalKind::Constr(_) | GoalKind::Op(_) => target.clone(),
            GoalKind::App(g1, g2) => {
                let g1 = self.eval(g1);
                let g2 = self.eval(g2);
                Goal::mk_app(g1, g2)
            }
            GoalKind::Conj(g1, g2) => {
                let g1 = self.eval(g1);
                let g2 = self.eval(g2);
                Goal::mk_conj(g1, g2)
            }
            GoalKind::Disj(g1, g2) => {
                let g1 = self.eval(g1);
                let g2 = self.eval(g2);
                Goal::mk_disj(g1, g2)
            }
            GoalKind::Abs(y, g) => match self.get_clause(&y.id) {
                Some(_) => {
                    let y2_ident = Ident::fresh();
                    let y2 = Variable::mk(y2_ident, y.ty.clone());
                    let g = g.rename(&y.id, &y2_ident);
                    Goal::mk_abs(y2, self.eval(&g))
                }
                None => Goal::mk_abs(y.clone(), self.eval(g)),
            },
            GoalKind::Univ(y, g) => {
                if self.get_clause(&y.id).is_some() {
                    let y2_ident = Ident::fresh();
                    let y2 = Variable::mk(y2_ident, y.ty.clone());
                    let g = g.rename(&y.id, &y2_ident);
                    GoalBase::mk_univ(y2, self.eval(&g))
                } else {
                    Goal::mk_univ(y.clone(), self.eval(g))
                }
            }
            GoalKind::ITE(c, g1, g2) => {
                let g1 = self.eval(g1);
                let g2 = self.eval(g2);
                Goal::mk_ite(c.clone(), g1, g2)
            }
        }
    }
}

impl<C> Problem<C> {
    pub fn order(&self) -> usize {
        let mut ord = 0;
        for c in self.clauses.iter() {
            ord = std::cmp::max(ord, c.order())
        }
        ord
    }

    pub fn get_clause<'a>(&'a self, id: &Ident) -> Option<&'a Clause<C>> {
        self.clauses.iter().find(|&c| c.head.id == *id)
    }
}
