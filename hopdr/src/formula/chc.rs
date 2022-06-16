use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;

use std::vec;

use crate::pdr::rtype::Refinement;

use super::pcsp;
use super::Bot;
use super::Negation;
use super::Subst;
use super::{Constraint, Fv, Ident, Logic, Op, Rename, Top, DerefPtr};

#[derive(Debug, Clone)]
pub struct Atom {
    pub predicate: Ident,
    pub args: Vec<Op>,
}

pub trait TConstraint:
    Clone
    + Top
    + Bot
    + Negation
    + Logic
    + Subst<Id = Ident, Item = Op>
    + Fv<Id = Ident>
    + PartialEq
    + Rename
    + fmt::Display
{
}
impl<T> TConstraint for T where
    T: Clone
        + Top
        + Bot
        + Negation
        + Logic
        + Subst<Id = Ident, Item = Op>
        + Fv<Id = Ident>
        + PartialEq
        + Rename
        + fmt::Display
{
}

#[derive(Debug, Clone)]
pub enum CHCHead<A, C> {
    Constraint(C),
    Predicate(A),
}

#[derive(Debug, Clone)]
pub struct CHCBody<A, C> {
    pub predicates: Vec<A>,
    pub constraint: C,
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.predicate)?;
        if !self.args.is_empty() {
            write!(f, "{}", self.args[0])?;
            if !self.args.len() > 1 {
                for arg in self.args[1..].iter() {
                    write!(f, ",{}", arg)?;
                }
            }
        }
        write!(f, ")")
    }
}

impl<A: fmt::Display, C: fmt::Display> fmt::Display for CHCHead<A, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CHCHead::Constraint(c) => write!(f, "{}", c),
            CHCHead::Predicate(a) => {
                write!(f, "{}", a)
            }
        }
    }
}
impl Atom {
    pub fn new(predicate: Ident, args: Vec<Op>) -> Atom {
        Atom { predicate, args }
    }
}

impl<A, C: TConstraint> CHCHead<A, C> {
    pub fn mk_true() -> CHCHead<A, C> {
        CHCHead::Constraint(C::mk_true())
    }
    pub fn mk_constraint(c: C) -> CHCHead<A, C> {
        CHCHead::Constraint(c)
    }
}
impl<C> CHCHead<Atom, C> {
    pub fn mk_predicate(predicate: Ident, args: Vec<Op>) -> CHCHead<Atom, C> {
        CHCHead::Predicate(Atom { predicate, args })
    }
}
impl Rename for Atom {
    fn rename(&self, x: &Ident, y: &Ident) -> Self {
        let args = self.args.iter().map(|o| o.rename(x, y)).collect();
        Atom {
            args,
            predicate: self.predicate,
        }
    }
}
impl<A: Rename, C: Rename> Rename for CHCHead<A, C> {
    fn rename(&self, x: &Ident, y: &Ident) -> Self {
        match self {
            CHCHead::Constraint(c) => CHCHead::Constraint(c.rename(x, y)),
            CHCHead::Predicate(a) => CHCHead::Predicate(a.rename(x, y)),
        }
    }
}
impl<A: Rename, C: Rename> Rename for CHCBody<A, C> {
    fn rename(&self, x: &Ident, y: &Ident) -> Self {
        let constraint = self.constraint.rename(x, y);
        let predicates = self.predicates.iter().map(|a| a.rename(x, y)).collect();
        CHCBody {
            constraint,
            predicates,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CHC<A, C> {
    pub body: CHCBody<A, C>,
    pub head: CHCHead<A, C>,
}

impl<A: Refinement, C: Refinement> Rename for CHC<A, C> {
    fn rename(&self, x: &Ident, y: &Ident) -> Self {
        let body = self.body.rename(x, y);
        let head = self.head.rename(x, y);
        CHC { head, body }
    }
}

impl<A: Rename + Fv, C: Rename + Fv> CHCBody<A, C> {}

impl<A: Rename + Fv<Id = Ident> + Clone, C: Rename + Fv<Id = Ident> + Clone> CHC<A, C> {
    pub fn fresh_variables(&self) -> CHC<A, C> {
        let mut fvs = self.body.fv();
        self.head.fv_with_vec(&mut fvs);

        let mut head = self.head.clone();
        let mut body = self.body.clone();
        let fvs = fvs.into_iter().map(|x| (x, Ident::fresh()));
        for (old, new) in fvs {
            head = head.rename(&old, &new);
            body = body.rename(&old, &new);
        }
        CHC { body, head }
    }
}

impl Fv for Atom {
    type Id = Ident;

    fn fv_with_vec(&self, fvs: &mut HashSet<Self::Id>) {
        for a in self.args.iter() {
            a.fv_with_vec(fvs);
        }
    }
}

impl<A: Fv<Id = Ident>, C: Fv<Id = Ident>> Fv for CHCHead<A, C> {
    type Id = Ident;

    fn fv_with_vec(&self, fvs: &mut HashSet<Self::Id>) {
        match &self {
            CHCHead::Constraint(c) => c.fv_with_vec(fvs),
            CHCHead::Predicate(a) => a.fv_with_vec(fvs),
        }
    }
}

impl<A: Fv<Id = Ident>, C: Fv<Id = Ident>> Fv for CHCBody<A, C> {
    type Id = Ident;

    fn fv_with_vec(&self, fvs: &mut HashSet<Self::Id>) {
        for b in self.predicates.iter() {
            b.fv_with_vec(fvs);
        }
        self.constraint.fv_with_vec(fvs);
    }
}

impl<A: Fv<Id = Ident>, C: Fv<Id = Ident>> Fv for CHC<A, C> {
    type Id = Ident;

    fn fv_with_vec(&self, fvs: &mut HashSet<Self::Id>) {
        self.body.fv_with_vec(fvs);
        self.head.fv_with_vec(fvs);
    }
}
fn to_pnf(a: &pcsp::Atom) -> pcsp::Atom {
    use pcsp::Atom;
    use pcsp::AtomKind;
    match a.kind() {
        AtomKind::True | AtomKind::Constraint(_) | AtomKind::Predicate(_, _) => a.clone(),
        AtomKind::Conj(a1, a2) => {
            let a1 = to_pnf(a1);
            let a2 = to_pnf(a2);
            Atom::mk_conj(a1, a2)
        }
        AtomKind::Disj(a1, a2) => {
            let a1 = to_pnf(a1);
            let a2 = to_pnf(a2);
            Atom::mk_disj(a1, a2)
        }
        AtomKind::Quantifier(_, _, a) => to_pnf(a),
    }
}

pub(crate) fn body_iter(body: pcsp::Atom) -> impl Iterator<Item = CHCBody<Atom, Constraint>> {
    fn translate(atom: pcsp::Atom, predicates: &mut Vec<Atom>, constraint: &mut Constraint) {
        match atom.kind() {
            pcsp::AtomKind::True => (),
            pcsp::AtomKind::Constraint(c) => {
                *constraint = Constraint::mk_conj(constraint.clone(), c.clone())
            }
            pcsp::AtomKind::Predicate(predicate, args) => predicates.push(Atom {
                predicate: *predicate,
                args: args.clone(),
            }),
            pcsp::AtomKind::Conj(_, _)
            | pcsp::AtomKind::Disj(_, _)
            | pcsp::AtomKind::Quantifier(_, _, _) => panic!("program error"),
        }
    }

    // 1. to_pnf
    let body = to_pnf(&body);
    // 2. dnf
    body.to_dnf().into_iter().map(|body| {
        let atoms = body.to_cnf();
        let mut constraint = Constraint::mk_true();
        let mut predicates = Vec::new();
        for atom in atoms {
            translate(atom, &mut predicates, &mut constraint);
        }
        CHCBody {
            predicates,
            constraint,
        }
    })
}

pub fn generate_chcs(
    pairs: impl Iterator<Item = (pcsp::Atom, CHCHead<Atom, Constraint>)>,
) -> Vec<CHC<Atom, Constraint>> {
    let mut chcs = Vec::new();
    for (body, head) in pairs {
        for body in body_iter(body) {
            chcs.push(CHC {
                body,
                head: head.clone(),
            })
        }
    }
    chcs
}

impl From<CHCBody<Atom, Constraint>> for pcsp::Atom {
    fn from(body: CHCBody<Atom, Constraint>) -> Self {
        let mut a = pcsp::Atom::mk_true();
        for b in body.predicates {
            let b = pcsp::Atom::mk_pred(b.predicate, b.args);
            a = pcsp::Atom::mk_conj(a, b);
        }
        pcsp::Atom::mk_conj(pcsp::Atom::mk_constraint(body.constraint), a)
    }
}
impl From<CHCBody<Atom, pcsp::Atom>> for pcsp::Atom {
    fn from(body: CHCBody<Atom, pcsp::Atom>) -> Self {
        let mut a = pcsp::Atom::mk_true();
        for b in body.predicates {
            let b = pcsp::Atom::mk_pred(b.predicate, b.args);
            a = pcsp::Atom::mk_conj(a, b);
        }
        pcsp::Atom::mk_conj(body.constraint, a)
    }
}

impl<A: fmt::Display, C: fmt::Display + Top> fmt::Display for CHCBody<A, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        if !self.constraint.is_true() {
            first = false;
            write!(f, "{}", self.constraint)?;
        }
        for b in &self.predicates {
            if !first {
                write!(f, "/\\ ")?;
            } else {
                first = false;
            }
            write!(f, "{}", b)?;
        }
        Ok(())
    }
}
impl<A: fmt::Display, C: fmt::Display + Top> fmt::Display for CHC<A, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", self.body, self.head)
    }
}

impl<A> From<CHCHead<A, Constraint>> for CHCHead<A, pcsp::Atom> {
    fn from(h: CHCHead<A, Constraint>) -> Self {
        match h {
            CHCHead::Constraint(c) => CHCHead::Constraint(c.into()),
            CHCHead::Predicate(p) => CHCHead::Predicate(p),
        }
    }
}

impl<A> From<CHCBody<A, Constraint>> for CHCBody<A, pcsp::Atom> {
    fn from(h: CHCBody<A, Constraint>) -> Self {
        let constraint = h.constraint.into();
        CHCBody {
            constraint,
            predicates: h.predicates,
        }
    }
}

impl<A> From<CHC<A, Constraint>> for CHC<A, pcsp::Atom> {
    fn from(c: CHC<A, Constraint>) -> Self {
        let body = c.body.into();
        let head = c.head.into();
        CHC { body, head }
    }
}

impl<A> From<CHCHead<A, pcsp::Atom>> for CHCHead<A, Constraint> {
    fn from(h: CHCHead<A, pcsp::Atom>) -> Self {
        match h {
            CHCHead::Constraint(c) => CHCHead::Constraint(c.to_constraint().unwrap()),
            CHCHead::Predicate(p) => CHCHead::Predicate(p),
        }
    }
}

impl<A> From<CHCBody<A, pcsp::Atom>> for CHCBody<A, Constraint> {
    fn from(h: CHCBody<A, pcsp::Atom>) -> Self {
        let constraint = h.constraint.to_constraint().unwrap();
        CHCBody {
            constraint,
            predicates: h.predicates,
        }
    }
}

impl<A> From<CHC<A, pcsp::Atom>> for CHC<A, Constraint> {
    fn from(c: CHC<A, pcsp::Atom>) -> Self {
        let body = c.body.into();
        let head = c.head.into();
        CHC { body, head }
    }
}

impl<C: TConstraint> CHCBody<Atom, C> {
    fn collect_predicates(&self, predicates: &mut HashMap<Ident, usize>) {
        for a in self.predicates.iter() {
            match predicates.insert(a.predicate, a.args.len()) {
                Some(n) => debug_assert!(n == a.args.len()),
                None => (),
            }
        }
    }
}
impl<C: TConstraint> CHC<Atom, C> {
    pub fn collect_predicates(&self, predicates: &mut HashMap<Ident, usize>) {
        match &self.head {
            CHCHead::Constraint(_) => (),
            CHCHead::Predicate(a) => match predicates.insert(a.predicate, a.args.len()) {
                Some(n) => debug_assert!(n == a.args.len()),
                None => (),
            },
        }
        self.body.collect_predicates(predicates);
    }
}

impl Atom {
    fn replace_with_model(&self, model: &Model) -> Constraint {
        let m = model.model.get(&self.predicate).unwrap();
        assert_eq!(m.0.len(), self.args.len());
        m.1.subst_multi(
            m.0.iter()
                .zip(self.args.iter())
                .map(|(x, y)| (x.clone(), y.clone())),
        )
    }
}

impl CHCHead<Atom, Constraint> {
    fn replace_with_model(&self, model: &Model) -> Constraint {
        match self {
            CHCHead::Constraint(c) => c.clone(),
            CHCHead::Predicate(a) => a.replace_with_model(model),
        }
    }
}

impl CHCBody<Atom, Constraint> {
    fn replace_with_model(&self, model: &Model) -> Constraint {
        let mut c = self.constraint.clone();
        for a in self.predicates.iter() {
            c = Constraint::mk_conj(c, a.replace_with_model(model));
        }
        c
    }
}

impl CHC<Atom, Constraint> {
    pub fn replace_with_model(&self, model: &Model) -> Constraint {
        let head = self.head.replace_with_model(model);
        let body = self.body.replace_with_model(model);
        Constraint::mk_implies(body, head)
    }
}

#[cfg(test)]
/// ### clause
/// P(x + 1, y) /\ Q(y) /\ x < 0 => P(x, y)
/// ### model
/// - P(x, y) = x < y
/// - Q(y)    = 5 < y
/// ### variables
/// [x, y, p, q]
pub fn gen_clause_pqp() -> (CHC<Atom, Constraint>, Model, Vec<Ident>) {
    let p = Ident::fresh();
    let q = Ident::fresh();
    let x = Ident::fresh();
    let y = Ident::fresh();
    let x_p_1 = Op::mk_add(Op::mk_var(x), Op::mk_const(1));
    let head = CHCHead::Predicate(Atom {
        predicate: p,
        args: vec![Op::mk_var(x), Op::mk_var(y)],
    });
    let c1 = Atom {
        predicate: p,
        args: vec![x_p_1, Op::mk_var(y)],
    };
    let c2 = Atom {
        predicate: q,
        args: vec![Op::mk_var(y)],
    };
    let constr = Constraint::mk_lt(Op::mk_var(x), Op::mk_const(0));
    let body = CHCBody {
        constraint: constr,
        predicates: vec![c1, c2],
    };

    let p_c = Constraint::mk_lt(Op::mk_var(x), Op::mk_var(y));
    let q_c = Constraint::mk_lt(Op::mk_const(5), Op::mk_var(y));
    let mut model = Model::new();
    model.model.insert(p, (vec![x, y], p_c));
    model.model.insert(q, (vec![x], q_c));
    (CHC { head, body }, model, vec![x, y, p, q])
}

#[test]
fn test_replace_with_model() {
    let (chc, model, vars) = gen_clause_pqp();
    let result = chc.replace_with_model(&model);
    println!("result: {}", result);
    let x = vars[0];
    let y = vars[1];

    // x + 1 < y /\ 5 < y /\ x < 0 => x < y
    let c1 = Constraint::mk_lt(Op::mk_add(Op::mk_var(x), Op::mk_const(1)), Op::mk_var(y));
    let c2 = Constraint::mk_lt(Op::mk_const(5), Op::mk_var(y));
    let c3 = Constraint::mk_lt(Op::mk_var(x), Op::mk_const(0));
    let head = Constraint::mk_lt(Op::mk_var(x), Op::mk_var(y));
    let body = Constraint::mk_conj(c1, Constraint::mk_conj(c2, c3));
    let answer = Constraint::mk_implies(body, head);
    println!("answer: {}", answer);

    // check if result <=> answer using SMT solver
    use crate::solver::smt;
    let mut solver = smt::default_solver();
    match solver.check_equivalent(&result, &answer) {
        crate::solver::SolverResult::Sat => (),
        _ => panic!("error"),
    }
}

fn cross_and<A: Clone, C: TConstraint>(
    left: Vec<Vec<CHCHead<A, C>>>,
    mut right: Vec<Vec<CHCHead<A, C>>>,
) -> Vec<Vec<CHCHead<A, C>>> {
    let mut ret = Vec::new();
    for x in left.iter() {
        for y in right.iter_mut() {
            let mut v = x.clone();
            v.append(y);
            ret.push(v);
        }
    }
    ret
}

pub fn to_dnf(atom: &pcsp::Atom) -> Vec<Vec<CHCHead<Atom, Constraint>>> {
    use pcsp::AtomKind;
    match atom.kind() {
        AtomKind::True => vec![vec![CHCHead::mk_true()]],
        AtomKind::Constraint(c) => vec![vec![CHCHead::mk_constraint(c.clone())]],
        AtomKind::Predicate(p, l) => vec![vec![CHCHead::mk_predicate(*p, l.clone())]],
        AtomKind::Conj(x, y) => {
            let left = to_dnf(x);
            let right = to_dnf(y);
            cross_and(left, right)
        }
        AtomKind::Disj(x, y) => {
            let mut left = to_dnf(x);
            let mut right = to_dnf(y);
            left.append(&mut right);
            left
        }
        AtomKind::Quantifier(_, _, _) => unimplemented!(),
    }
}

pub struct Model {
    pub model: HashMap<Ident, (Vec<Ident>, Constraint)>,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, (args, assign)) in self.model.iter() {
            write!(f, "{}(", key)?;
            let mut first = true;
            for arg in args.iter() {
                if first {
                    first = false
                } else {
                    write!(f, ", ")?;
                }
                write!(f, "{}", arg)?;
            }
            write!(f, ") => {}\n", assign)?;
        }
        Ok(())
    }
}

impl Model {
    pub fn new() -> Model {
        Model {
            model: HashMap::new(),
        }
    }
    pub fn merge(&mut self, model: Model) {
        for (k, v) in model.model.into_iter() {
            self.model.insert(k, v);
        }
    }
}
