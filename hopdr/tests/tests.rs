extern crate hopdr;

use hopdr::*;
use nom::error::VerboseError;

#[test]
fn type_check_1() {
    let (_, f) = parse::parse::<VerboseError<&str>>(
        "
		S n k = (n > 0 | k 0) & (n <= 0 | S (n - 1) (L n k));
		K m n = m <= n;
		L n k m = k (n + m);
		M = S 1 (K 1);
		 ",
    )
    .unwrap();

    match &f {
        parse::Problem::NuHFLZValidityChecking(vc) => {
            for fml in vc.formulas.iter() {
                println!("{}", fml);
            }
        }
    }

    let (vc, _ctx) = preprocess::hes::preprocess(f);
    for fml in vc.clauses.iter() {
        println!("{}", fml);
    }

    let mut types = Vec::new();
    {
        use formula::{Constraint, Ident, Op, PredKind, Top};
        use hopdr::pdr::*;
        use rtype::Tau;
        // S
        let n = Ident::fresh();
        let m = Ident::fresh();
        let t = Tau::mk_iarrow(
            m,
            Tau::mk_prop_ty(Constraint::mk_pred(
                PredKind::Leq,
                vec![Op::mk_var(n), Op::mk_var(m)],
            )),
        );
        let t = Tau::mk_arrow_single(t, Tau::mk_prop_ty(Constraint::mk_true()));
        let t = Tau::mk_iarrow(n, t);
        println!("{}", &t);
        types.push(t);

        // K
        let n = Ident::fresh();
        let m = Ident::fresh();
        let t = Tau::mk_iarrow(
            m,
            Tau::mk_prop_ty(Constraint::mk_pred(
                PredKind::Leq,
                vec![Op::mk_var(n), Op::mk_var(m)],
            )),
        );
        let t = Tau::mk_iarrow(n, t);
        println!("{}", &t);
        types.push(t);

        // L
        let n = Ident::fresh();
        let m = Ident::fresh();
        let p = Ident::fresh();
        let t = Tau::mk_iarrow(
            p,
            Tau::mk_prop_ty(Constraint::mk_pred(
                PredKind::Leq,
                vec![Op::mk_const(0), Op::mk_var(p)],
            )),
        );
        //let t = Tau::mk_iarrow(p, Tau::mk_prop_ty(Constraint::mk_true()));
        let s = Tau::mk_iarrow(
            m,
            Tau::mk_prop_ty(Constraint::mk_pred(
                PredKind::Leq,
                vec![Op::mk_var(n), Op::mk_var(m)],
            )),
        );
        let t = Tau::mk_arrow_single(s, t);
        let t = Tau::mk_iarrow(n, t);
        println!("{}", &t);
        types.push(t);
    }
    use pdr::rtype::*;

    let mut env = TyEnv::new();

    for (fml, ty) in vc.clauses.iter().zip(types.iter()) {
        env.add(fml.head.id, ty.clone());
    }

    let vc = vc.into();
    assert!(pdr::fml::check_inductive(&env, &vc))
}

#[test]
fn type_check_2() {
    let (_, f) = parse::parse::<VerboseError<&str>>(
        "
	    X n f = f n & X (n + 1) f;
	    Y n f = f n & X (n - 1) f;
	    E n = n != 0;
	    Z x = X x E | Y (0 - x) E;
	    M = Z 1;
	     ",
    )
    .unwrap();
    match &f {
        parse::Problem::NuHFLZValidityChecking(vc) => {
            for fml in vc.formulas.iter() {
                println!("{}", fml);
            }
        }
    }

    let (vc, _ctx) = preprocess::hes::preprocess(f);
    for fml in vc.clauses.iter() {
        println!("{}", fml);
    }

    // let mut types = Vec::new();
    // {
    //     use engine::*;
    //     use formula::{Constraint, Ident, Op, PredKind};
    //     use rtype::Tau;
    //     // X
    //     let n = Ident::fresh();
    //     let m = Ident::fresh();
    //     let t = Tau::mk_iarrow(
    //         m,
    //         Tau::mk_prop_ty(Constraint::mk_pred(
    //             PredKind::Neq,
    //             vec![Op::mk_const(0), Op::mk_var(m)],
    //         )),
    //     );
    //     let t = Tau::mk_arrow(
    //         t,
    //         Tau::mk_prop_ty(Constraint::mk_pred(
    //             PredKind::Gt,
    //             vec![Op::mk_var(n), Op::mk_const(0)],
    //         )),
    //     );
    //     let t = Tau::mk_iarrow(n, t);
    //     println!("{}", &t);
    //     types.push(t);

    //     // Y
    //     let n = Ident::fresh();
    //     let m = Ident::fresh();
    //     let t = Tau::mk_iarrow(
    //         m,
    //         Tau::mk_prop_ty(Constraint::mk_pred(
    //             PredKind::Neq,
    //             vec![Op::mk_const(0), Op::mk_var(m)],
    //         )),
    //     );
    //     let t = Tau::mk_arrow(
    //         t,
    //         Tau::mk_prop_ty(Constraint::mk_pred(
    //             PredKind::Gt,
    //             vec![Op::mk_var(n), Op::mk_const(0)],
    //         )),
    //     );
    //     let t = Tau::mk_iarrow(n, t);
    //     println!("{}", &t);
    //     types.push(t);

    //     // K
    //     let n = Ident::fresh();
    //     let m = Ident::fresh();
    //     let t = Tau::mk_iarrow(
    //         m,
    //         Tau::mk_prop_ty(Constraint::mk_pred(
    //             PredKind::Leq,
    //             vec![Op::mk_var(n), Op::mk_var(m)],
    //         )),
    //     );
    //     let t = Tau::mk_iarrow(n, t);
    //     println!("{}", &t);
    //     types.push(t);

    //     // L
    //     let n = Ident::fresh();
    //     let m = Ident::fresh();
    //     let p = Ident::fresh();
    //     let t = Tau::mk_iarrow(
    //         p,
    //         Tau::mk_prop_ty(Constraint::mk_pred(
    //             PredKind::Leq,
    //             vec![Op::mk_const(0), Op::mk_var(p)],
    //         )),
    //     );
    //     //let t = Tau::mk_iarrow(p, Tau::mk_prop_ty(Constraint::mk_true()));
    //     let s = Tau::mk_iarrow(
    //         m,
    //         Tau::mk_prop_ty(Constraint::mk_pred(
    //             PredKind::Leq,
    //             vec![Op::mk_var(n), Op::mk_var(m)],
    //         )),
    //     );
    //     let t = Tau::mk_arrow(s, t);
    //     let t = Tau::mk_iarrow(n, t);
    //     println!("{}", &t);
    //     types.push(t);
    // }

    // let mut env = engine::rtype::PosEnvironment::new();

    // for (fml, ty) in vc.clauses.iter().zip(types.iter()) {
    //     env.add(fml.head.id, ty.clone());
    // }

    // for (fml, ty) in vc.clauses.iter().zip(types.iter()) {
    //     let env = (&env).into();
    //     println!(
    //         "{}:{}\n -> {:?}",
    //         fml,
    //         ty.clone(),
    //         engine::rtype::type_check_clause(fml, ty.clone(), env)
    //     );
    // }
}
