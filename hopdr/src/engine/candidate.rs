use std::collections::HashSet;

use super::rtype;
use super::rtype::Environment;
use crate::formula::chc;
use crate::formula::fofml;
use crate::formula::hes;
use crate::formula::hes::{Goal, GoalKind};
use crate::formula::pcsp;
use crate::formula::{
    Bot, Conjunctive, Constraint, Fv, IntegerEnvironment, QuantifierKind, Rename,
};

#[derive(Debug)]
pub struct Negative {}
pub type Sty = rtype::Tau<Negative, Constraint>;
pub type NegEnvironment = rtype::TypeEnvironment<Sty>;

fn consistent(s: &Sty, t: &rtype::Tau<rtype::Positive, pcsp::Atom>) -> fofml::Atom {
    use fofml::Atom;
    use rtype::TauKind::*;
    match (s.kind(), t.kind()) {
        (Proposition(x), Proposition(y)) => {
            Atom::mk_not(Atom::mk_conj(x.clone().into(), y.clone().into()))
        }
        (IArrow(x, s), IArrow(y, t)) => {
            let t = t.rename(y, x);
            let c = consistent(s, &t);
            Atom::mk_univq(*x, c)
        }
        (Arrow(s1, s2), Arrow(t1, t2)) => {
            let c1 = Atom::mk_not(consistent(s1, t1));
            let c2 = consistent(s2, t2);
            Atom::mk_disj(c1, c2)
        }
        _ => panic!("program error"),
    }
}

fn types_top(env: &rtype::PosEnvironment, g: &hes::Goal, rty: Constraint) -> fofml::Atom {
    let mut env = Environment::from_type_environment(env.into());
    let c2 = rtype::type_check_goal(g, &mut env).unwrap();
    let p = pcsp::PCSP::new(rty.into(), c2);
    p.into()
}

fn types(
    env: &rtype::PosEnvironment,
    cl: &hes::Clause,
    rty: rtype::Tau<rtype::Positive, pcsp::Atom>,
) -> fofml::Atom {
    let mut env = Environment::from_type_environment(env.into());
    let t = env.add_arg_types(&cl.args, rty);
    let c = match t.kind() {
        rtype::TauKind::Proposition(c) => c,
        _ => panic!("program error"),
    };
    let c2 = rtype::type_check_goal(&cl.body, &mut env).unwrap();
    let p = pcsp::PCSP::new(c.clone(), c2);
    p.into()
}

impl Sty {
    // returns Ok(positive type) when this candidate is refutable
    // otherwise, Err(NegEnvironmet) where NegEnvironment is a negative type environment Δ
    // such that Δ |- clause: self
    pub fn is_refutable_top(
        &self,
        g: &hes::Goal,
        env: &rtype::PosEnvironment,
    ) -> Result<rtype::Ty, NegEnvironment> {
        let cnstr = match self.kind() {
            rtype::TauKind::Proposition(c) => c.clone(),
            rtype::TauKind::IArrow(_, _) | rtype::TauKind::Arrow(_, _) => panic!("program error"),
        };

        let mut new_idents = HashSet::new();
        let ty = self.clone_with_template(IntegerEnvironment::new(), &mut new_idents);
        let fml = consistent(self, &ty);
        let fml2 = types_top(env, g, cnstr);
        let fml = fofml::Atom::mk_conj(fml, fml2);
        match fml.check_satisfiability() {
            Some(model) => Ok(ty.assign(&model)),
            None => unimplemented!(),
        }
    }
    pub fn is_refutable(
        &self,
        clause: &hes::Clause,
        env: &rtype::PosEnvironment,
    ) -> Result<rtype::Ty, NegEnvironment> {
        let mut new_idents = HashSet::new();
        let ty = self.clone_with_template(IntegerEnvironment::new(), &mut new_idents);
        let fml = consistent(self, &ty);
        let fml2 = types(env, clause, ty.clone());
        let fml = fofml::Atom::mk_conj(fml, fml2);
        match fml.check_satisfiability() {
            Some(model) => Ok(ty.assign(&model)),
            None => {
                let mut new_idents = HashSet::new();
                let _template_env: rtype::TypeEnvironment<rtype::Tau<Negative, pcsp::Atom>> =
                    env.clone_with_template(&mut new_idents);
                // generate_constraint
                // check_sat
                unimplemented!()
            }
        }
    }
}

pub fn type_check_goal<'a>(
    goal: &Goal,
    tenv: &mut Environment<rtype::Tau<Negative, pcsp::Atom>>,
) -> Result<pcsp::Atom, rtype::Error> {
    debug!("type_check_goal start: {}", goal);
    let f = type_check_goal;
    use rtype::{type_check_atom, TauKind};
    use GoalKind::*;
    let r = match goal.kind() {
        Atom(atom) => {
            let ts = type_check_atom(atom, tenv)?;
            for (t, constraints) in ts.iter() {
                debug!("- type: {}", t);
                for c in constraints.iter() {
                    debug!("-- constraint: {}", c)
                }
            }

            // TODO: here calculate greatest type
            let mut ret_constr = pcsp::Atom::mk_false();
            for (t, constraints) in ts {
                let mut targets = HashSet::new();
                t.fv_with_vec(&mut targets);
                match t.kind() {
                    TauKind::Proposition(_) => {
                        let c = chc::resolve_target(constraints, &targets).unwrap();
                        debug!("final constraint: {}", c);
                        ret_constr = pcsp::Atom::mk_disj(ret_constr, c.clone());
                    }
                    _ => panic!("program error. The result type of atom must be prop."),
                }
            }
            ret_constr
        }
        Constr(c) => c.clone().negate().unwrap().into(),
        Conj(x, y) => pcsp::Atom::mk_disj(f(x, tenv)?, f(y, tenv)?),
        Disj(x, y) => pcsp::Atom::mk_conj(f(x, tenv)?, f(y, tenv)?),
        Univ(v, x) => {
            if v.ty.is_int() {
                pcsp::Atom::mk_quantifier(QuantifierKind::Existential, v.id, f(x, tenv)?)
            } else {
                unimplemented!()
            }
        }
    };
    debug!("type_check_goal: {} has type {} ", goal, r);
    Ok(r)
}
