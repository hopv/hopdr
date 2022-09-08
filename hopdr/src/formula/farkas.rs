use std::collections::HashMap;

/// Given a linear integer arithmetic Constraint,
/// returns
///
///
use super::{Constraint, Fv, Ident, Logic, Negation, Op, PredKind, Top};

/// transform all the predicates to another constraint that only use `<`.
fn transform_predicate(c: &Constraint) -> Constraint {
    fn inner(p: PredKind, l: &Vec<Op>) -> Constraint {
        assert_eq!(l.len(), 2);
        let x = l[0].clone();
        let y = l[1].clone();
        fn lt(x: Op, y: Op) -> Constraint {
            Constraint::mk_bin_pred(PredKind::Lt, x, y)
        }
        fn inc(x: Op) -> Op {
            Op::mk_add(x, Op::mk_const(1))
        }
        match p {
            PredKind::Eq => {
                /*
                (assert (forall ((x Int) (y Int))
                        (and
                            (implies (= x y) (and (< y (+ 1 x)) (< x (+ 1 y))))
                            (implies (and (< y (+ 1 x)) (< x (+ 1 y)))  (= x y))
                        )
                ))
                (check-sat)
                 */
                let p1 = lt(y.clone(), inc(x.clone()));
                let p2 = lt(x, inc(y));
                Constraint::mk_conj(p1, p2)
            }
            PredKind::Neq => {
                // x != y <=> x < y \/ y < x
                let p1 = lt(x.clone(), y.clone());
                let p2 = lt(y, x);
                Constraint::mk_disj(p1, p2)
            }
            PredKind::Gt => lt(y, x),
            PredKind::Geq => lt(y, inc(x)),
            PredKind::Lt => lt(x, y),
            PredKind::Leq => lt(inc(x), y),
        }
    }

    match c.kind() {
        crate::formula::ConstraintExpr::True | crate::formula::ConstraintExpr::False => c.clone(),
        crate::formula::ConstraintExpr::Pred(p, l) => inner(p, l),
        crate::formula::ConstraintExpr::Conj(x, y) => {
            let x = transform_predicate(x);
            let y = transform_predicate(y);
            Constraint::mk_conj(x, y)
        }
        crate::formula::ConstraintExpr::Disj(x, y) => {
            let x = transform_predicate(x);
            let y = transform_predicate(y);
            Constraint::mk_disj(x, y)
        }
        crate::formula::ConstraintExpr::Quantifier(q, x, c) => {
            let c = transform_predicate(c);
            Constraint::mk_quantifier(*q, x.clone(), c)
        }
    }
}

fn pred_to_vec(constr: &Constraint, m: &HashMap<Ident, usize>) -> Vec<Op> {
    match constr.kind() {
        super::ConstraintExpr::True => 
        super::ConstraintExpr::Pred(_, _) => todo!(),
        super::ConstraintExpr::False 
        | super::ConstraintExpr::Conj(_, _)
        | super::ConstraintExpr::Disj(_, _)
        | super::ConstraintExpr::Quantifier(_, _, _) => panic!("program error"),
    }
}

/// returns a constraint that does not contain universal quantifiers
/// ### Assumption:
/// - c's free variables are considered to be bound by existential quantifiers
/// - c only contains universal quantifiers
/// - c only contains linear constraints
pub fn farkas_transform(c: &Constraint) -> Constraint {
    // translates the constraint to ∧ θᵢ where θᵢ has the form ¬ (∧ eᵢ ≥ 0).
    // Note that eᵢ is a linear expression.

    // 1. prenex normal form of c
    let mut fv = c.fv();
    let (v, c2) = c.prenex_normal_form_raw(&mut fv);
    let mut univ_vars = HashMap::new();
    // now c2 is a quantifier free formula
    for (idx, (q, var)) in v.iter().enumerate() {
        assert_eq!(*q, super::QuantifierKind::Universal);
        assert!(var.ty.is_int());
        univ_vars.insert(var.id, idx);
    }

    // first replace all the predicates except for >= by constraints that only contain < (which will be negated below, so that will produce
    // <=s)
    let c2 = transform_predicate(&c2);

    // cnf = [θᵢ]
    let cnf = c2.to_cnf();

    // we transform cnf's element so that it has the form  ¬ (∧ eᵢ ≥ 0).
    let cnf = cnf
        .into_iter()
        .map(|clause| {
            let dnf = clause.to_dnf();
            // check if it is trivial or not
            let mut trivial_true = false;
            for c in dnf.iter() {
                match c.kind() {
                    super::ConstraintExpr::True => trivial_true = true,
                    _ => (),
                }
            }
            if trivial_true {
                return Constraint::mk_true();
            }

            // we transform disjunction to not conjunction by de morgan dual
            let matrix = dnf
                .into_iter()
                .map(|constr| pred_to_vec(&constr.negate().unwrap(), &univ_vars))
                .collect();

            unimplemented!()
        })
        .collect();

    unimplemented!()
}
