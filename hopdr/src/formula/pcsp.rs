

use super::{Conjunctive, Constraint, Ident, Op, Subst, Top};
use crate::util::P;

#[derive(Debug)]
pub enum AtomKind {
    True, // equivalent to Constraint(True). just for optimization purpose
    Constraint(Constraint),
    Predicate(Ident, Vec<Op>),
    Conj(Atom, Atom),
}
pub type Atom = P<AtomKind>;

impl Atom {
    pub fn mk_pred(ident: Ident, args: Vec<Op>) -> Atom {
        Atom::new(AtomKind::Predicate(ident, args))
    }
    pub fn fresh_pred(args: Vec<Ident>) -> Atom {
        let ident = Ident::fresh();
        let args = args.iter().map(|a| Op::mk_var(a.clone())).collect();
        Atom::mk_pred(ident, args)
    }
    pub fn contains_predicate(&self) -> bool {
        match self.kind() {
            AtomKind::True | AtomKind::Constraint(_) => false,
            AtomKind::Predicate(_, _) => true,
            AtomKind::Conj(c1, c2) => c1.contains_predicate() && c2.contains_predicate(),
        }
    }
    pub fn extract_pred_and_constr(&self) -> Option<(Constraint, Ident)> {
        match self.kind() {
            AtomKind::True | AtomKind::Constraint(_) => None,
            AtomKind::Predicate(i, _) => Some((Constraint::mk_false(), i.clone())),
            AtomKind::Conj(x, y) 
            | AtomKind::Conj(y, x) if x.contains_predicate() => 
                y.negate().map(|c2| 
                    x.extract_pred_and_constr().map(
                        |(c, i)| (Constraint::mk_disj(c, c2), i)
                    )
                ).flatten(),
            _ => None,
        }
    }
    pub fn negate(&self) -> Option<Constraint> {
        match self.kind() {
            AtomKind::True => Some(Constraint::mk_false()),
            AtomKind::Constraint(c) => c.clone().negate(),
            AtomKind::Conj(l, r) => {
                let l = l.negate();
                let r = r.negate();
                match (l, r) {
                    (_, None) | (None, _) => None,
                    (Some(x), Some(y)) => Some(Constraint::mk_disj(x.clone(), y.clone())),
                }
            },
            AtomKind::Predicate(_, _) => None,
        }
    }
}

impl Atom {
    // auxiliary function for generating constraint
    pub fn mk_constraint(c: Constraint) -> Atom {
        Atom::new(AtomKind::Constraint(c))
    }
}

impl From<Constraint> for Atom {
    fn from(from: Constraint) -> Atom {
        Atom::mk_constraint(from)
    }
}

impl Top for Atom {
    fn mk_true() -> Self {
        Atom::new(AtomKind::True)
    }
}

impl Conjunctive for Atom {
    fn mk_conj(x: Self, y: Self) -> Atom {
        use AtomKind::*;
        match (&*x, &*y) {
            (True, _) => y.clone(),
            (_, True) => x.clone(),
            _ => Atom::new(Conj(x.clone(), y.clone())),
        }
    }
}

impl Subst for Atom {
    fn subst(&self, x: &Ident, v: &super::Op) -> Self {
        match self.kind() {
            AtomKind::True => self.clone(),
            AtomKind::Conj(lhs, rhs) => Atom::mk_conj(lhs.subst(x, v), rhs.subst(x, v)),
            AtomKind::Constraint(c) => Atom::mk_constraint(c.subst(x, v)),
            AtomKind::Predicate(_a, ops) => {
                let ops = ops.iter().map(|op| op.subst(x, v)).collect();
                Atom::mk_pred(*x, ops)
            }
        }
    }
}

#[derive(Debug)]
pub struct PCSP<A> {
    pub body: A,
    pub head: A,
}

impl <A> PCSP<A> {
    pub fn new(body: A, head: A) -> PCSP<A> {
        PCSP { body, head }
    }
}

impl PCSP<Constraint> {
    pub fn to_constraint(&self) -> Option<Constraint> {
        Constraint::mk_arrow(self.body.clone(), self.head.clone())
    }
}