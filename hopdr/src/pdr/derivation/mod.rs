mod derive_tree;
pub mod tree;

use super::optimizer;
use super::optimizer::{variable_info, InferenceResult, Optimizer};
use super::rtype::{Tau, TauKind, TyEnv, TypeEnvironment};
use derive_tree::Derivation;

use crate::formula::chc::Model;
use crate::formula::hes::{Goal, GoalBase, GoalKind, Problem as ProblemBase};
use crate::formula::{self, Op, Type as STy};
use crate::formula::{
    chc, fofml, Constraint, Fv, Ident, Logic, Negation, Rename, Subst, Top, Variable,
};
use crate::solver;
use crate::util::Pretty;
use crate::{pdebug, pinfo, title};

use rpds::{HashTrieMap, Stack};

use std::collections::{HashMap, HashSet};
use std::fmt;

type Atom = fofml::Atom;
type Candidate = Goal<Constraint>;
pub(crate) type Ty = Tau<Atom>;
type Env = TypeEnvironment<Ty>;
type Problem = ProblemBase<Constraint>;

/// Calculate D φ while tracking the places where fixpoint variables are substituted.
/// (D is a set of fixpoint equations and φ is a candidate)
///
/// `track_idents` maps predicate in Problem to the idents of lambda abstraction exprs.
///
/// we can name each expr by using `aux.id`. for each expansion of a predicate, we memorize
/// this id.
/// example:
///  F x f = φ₁
///  ψ = F ... /\ F ...
/// then:
///  ψ = (λx1. λf. [x1/x]φ₁)ˣ ... /\ (λx2. λf. [x2/x]φ₁)ʸ ...
/// and track_idents
///  - F = [x, y]
fn subst_predicate(
    candidate: &G,
    problem: &Problem,
    track_idents: &mut HashMap<Ident, Vec<Ident>>,
) -> G {
    match candidate.kind() {
        formula::hes::GoalKind::Constr(_) | formula::hes::GoalKind::Op(_) => candidate.clone(),
        formula::hes::GoalKind::Var(x) => match problem.get_clause(x) {
            Some(clause) => {
                let body: G = clause.body.alpha_renaming().into(); // assign a fresh id by translating Candidate -> G
                track_idents
                    .entry(*x)
                    .or_insert_with(|| Vec::new())
                    .push(body.aux.id);
                match body.kind() {
                    formula::hes::GoalKind::Abs(v, g) => {
                        let id = v.id;
                        let new_id = Ident::fresh();
                        let g = g.rename(&id, &new_id);
                        G::mk_abs_t(Variable::mk(new_id, v.ty.clone()), g, body.aux.clone())
                    }
                    _ => panic!("body must be a lambda abstraction but got {}", body),
                }
            }
            None => candidate.clone(),
        },
        formula::hes::GoalKind::Abs(v, g) => {
            let g = subst_predicate(g, problem, track_idents);
            G::mk_abs_t(v.clone(), g, candidate.aux.clone())
        }
        formula::hes::GoalKind::App(x, y) => {
            let x = subst_predicate(x, problem, track_idents);
            let y = subst_predicate(y, problem, track_idents);
            G::mk_app_t(x, y, candidate.aux.clone())
        }
        formula::hes::GoalKind::Conj(x, y) => {
            let x = subst_predicate(x, problem, track_idents);
            let y = subst_predicate(y, problem, track_idents);
            G::mk_conj_t(x, y, candidate.aux.clone())
        }
        formula::hes::GoalKind::Disj(x, y) => {
            let x = subst_predicate(x, problem, track_idents);
            let y = subst_predicate(y, problem, track_idents);
            G::mk_disj_t(x, y, candidate.aux.clone())
        }
        formula::hes::GoalKind::Univ(v, g) => {
            let g = subst_predicate(g, problem, track_idents);
            G::mk_univ_t(v.clone(), g, candidate.aux.clone())
        }
        formula::hes::GoalKind::ITE(_, _, _) => todo!(),
    }
}
type Level = usize;

/// Reserved for future use, but not used in the current implementation
struct IntReduction {}

struct PredReduction {}

enum ReductionType {
    Int(IntReduction),
    Pred(PredReduction),
}

impl ReductionType {
    fn pred() -> ReductionType {
        ReductionType::Pred(PredReduction {})
    }
    fn int() -> ReductionType {
        ReductionType::Int(IntReduction {})
    }
    #[allow(dead_code)]
    fn is_int(&self) -> bool {
        matches!(self, ReductionType::Int(_))
    }
}

/// Represents a single step of beta reduction
struct ReductionInfo {
    level: Level,
    arg: G,
    arg_var: Variable,
    old_id: Ident,
    reduction_type: ReductionType,
}
impl ReductionInfo {
    fn new(
        level: Level,
        arg: G,
        arg_var: Variable,
        old_id: Ident,
        reduction_type: ReductionType,
    ) -> ReductionInfo {
        ReductionInfo {
            level,
            arg,
            arg_var,
            old_id,
            reduction_type,
        }
    }
}

/// Represents a reduction which can be composed of multiple applications
///
/// For example Reduction can represent the following reduction:
/// (\x. \y. Φ) arg1 arg2 -> [arg1/x, arg2/y] Φ.
/// In this case, `reduction_infos.len()` must be 2, `before_reduction` must be `(\x. \y. Φ) arg1 arg2`,
/// and `after_reduction` must be `[arg1/x, arg2/y] Φ`.
struct Reduction {
    #[allow(dead_code)]
    app_expr: G,
    // (λx. λy. ψ) arg1 arg2  -> ψ
    predicate: G, //λx. λy. ψ
    // args and arg_vras
    //
    // Level: this is the id of this reduction.
    // this value is memorized in the memory of each expression
    // for each reduction. That is, when we have reduction
    //    expr1 expr2 -> expr3
    // this id is memorized in expr2's memory (as the argument) and expr3's memory (as the return value)
    reduction_infos: Vec<ReductionInfo>,
    // the result of beta reduction; predicate expr -> result
    result: G,
    // predicate's free variables of type int
    fvints: HashSet<Ident>,
    #[allow(dead_code)]
    argints: HashSet<Ident>,
    // constraint of the redux where this reduction happens
    constraint: Constraint,
    // before_reduction -> after_reduction
    before_reduction: G,
    after_reduction: G,
}

impl fmt::Display for Reduction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Reduction {}]", self.reduction_infos[0].level)?;
        write!(f, "] fvints: {:?}", self.fvints)?;
        writeln!(f, " constraint: {}", self.constraint)?;
        writeln!(f, "{} ", self.predicate)?;
        // for arg in self.args.iter() {
        //     writeln!(f, "- {} ", arg.arg)?;
        // }
        write!(f, "\n ==> {}", self.result)
    }
}

impl Pretty for Reduction {
    fn pretty<'b, D, A>(
        &'b self,
        al: &'b D,
        config: &mut crate::util::printer::Config,
    ) -> pretty::DocBuilder<'b, D, A>
    where
        D: pretty::DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        al.text("[Reduction ")
            .append(self.reduction_infos[0].level.to_string())
            .append(al.text(format!(
                "] fvints: {:?} constraint: {}",
                self.fvints, self.constraint
            )))
            .append(al.hardline())
            .append(
                self.predicate
                    .pretty(al, config)
                    .append(al.hardline())
                    .append(
                        al.intersperse(
                            self.reduction_infos
                                .iter()
                                .map(|x| x.arg.pretty(al, config)),
                            " ",
                        ),
                    )
                    .append(" ==> ")
                    .append(self.result.pretty(al, config))
                    .hang(2),
            )
    }
}

impl Reduction {
    fn new(
        app_expr: G,
        predicate: G,
        result: G,
        reduction_infos: Vec<ReductionInfo>,
        fvints: HashSet<Ident>,
        argints: HashSet<Ident>,
        constraint: Constraint,
    ) -> Reduction {
        Reduction {
            app_expr,
            predicate,
            reduction_infos,
            result,
            fvints,
            argints,
            constraint,
            // dummy
            // assumed to be filled later
            before_reduction: G::mk_true(),
            after_reduction: G::mk_true(),
        }
    }
    fn level(&self) -> usize {
        self.reduction_infos[self.reduction_infos.len() - 1].level
    }
}

#[derive(Clone, Debug)]
struct TypeMemory {
    level_arg: Stack<usize>,
    id: Ident, // unique id for each sub expression of the formula
    old_ids: Stack<Ident>,
    tys: Option<Vec<Ty>>,
    // idents that are free variables at this position
    ints: Stack<Ident>,
    sty: Option<STy>,
}
impl TypeMemory {
    fn new() -> TypeMemory {
        TypeMemory {
            level_arg: Stack::new(),
            id: Ident::fresh(),
            old_ids: Stack::new(),
            tys: None,
            ints: Stack::new(),
            sty: None,
        }
    }
    fn add_arg_level(&mut self, level: usize) {
        self.level_arg = self.level_arg.push(level)
    }
    fn set_tys(&mut self, tys: Option<Vec<Ty>>) {
        self.tys = tys;
    }
}

impl Default for TypeMemory {
    fn default() -> Self {
        TypeMemory::new()
    }
}

/// internal representation of candidate terms.
///
/// Level is used for tracking when this candidate is used
/// as the argument of beta-reduction.
type G = GoalBase<Constraint, TypeMemory>;

impl From<Candidate> for G {
    fn from(c: Candidate) -> Self {
        let l = TypeMemory::new();
        match c.kind() {
            formula::hes::GoalKind::Constr(c) => G::mk_constr_t(c.clone(), l),
            formula::hes::GoalKind::Op(op) => G::mk_op_t(op.clone(), l),
            formula::hes::GoalKind::Var(id) => G::mk_var_t(*id, l),
            formula::hes::GoalKind::Abs(v, g) => G::mk_abs_t(v.clone(), g.clone().into(), l),
            formula::hes::GoalKind::App(x, y) => G::mk_app_t(x.clone().into(), y.clone().into(), l),
            formula::hes::GoalKind::Conj(x, y) => {
                G::mk_conj_t(x.clone().into(), y.clone().into(), l)
            }
            formula::hes::GoalKind::Disj(x, y) => {
                G::mk_disj_t(x.clone().into(), y.clone().into(), l)
            }
            formula::hes::GoalKind::Univ(x, g) => G::mk_univ_t(x.clone(), g.clone().into(), l),
            GoalKind::ITE(_, _, _) => todo!(),
        }
    }
}
impl From<G> for Goal<Constraint> {
    fn from(g: G) -> Self {
        match g.kind() {
            GoalKind::Constr(c) => Goal::mk_constr(c.clone()),
            GoalKind::Op(op) => Goal::mk_op(op.clone()),
            GoalKind::Var(id) => Goal::mk_var(*id),
            GoalKind::Abs(v, g) => Goal::mk_abs(v.clone(), g.clone().into()),
            GoalKind::App(x, y) => Goal::mk_app(x.clone().into(), y.clone().into()),
            GoalKind::Conj(x, y) => Goal::mk_conj(x.clone().into(), y.clone().into()),
            GoalKind::Disj(x, y) => Goal::mk_disj(x.clone().into(), y.clone().into()),
            GoalKind::Univ(x, g) => Goal::mk_univ(x.clone(), g.clone().into()),
            GoalKind::ITE(_, _, _) => todo!(),
        }
    }
}

impl PartialEq for G {
    fn eq(&self, other: &Self) -> bool {
        let g1: Goal<Constraint> = self.clone().into();
        let g2: Goal<Constraint> = other.clone().into();
        g1 == g2
    }
}

impl TypeMemory {
    fn update_id(&self) -> Self {
        let mut tm = self.clone();
        let old_id = tm.id;
        tm.id = Ident::fresh();
        tm.old_ids.push_mut(old_id);
        tm
    }
}

/// Some utility functions for goal
impl GoalBase<Constraint, TypeMemory> {
    /// Returns the pair of alpha-renamed expr and the map used for renaming.
    ///
    /// The map is a map from the id of the expr that introduces a new variable
    /// to the pair of the old id and the new id.
    pub fn alpha_renaming_with_map(&self, map: &mut HashMap<Ident, (Ident, Ident)>) -> Self {
        fn aux(
            v: &Variable,
            g: &G,
            map: &mut HashMap<Ident, (Ident, Ident)>,
            expr_id: Ident,
        ) -> (Variable, G) {
            let id = Ident::fresh();
            map.insert(expr_id, (v.id, id));
            let g = g.rename(&v.id, &id);
            (Variable::mk(id, v.ty.clone()), g)
        }
        fn go(expr: &G, map: &mut HashMap<Ident, (Ident, Ident)>) -> G {
            match expr.kind() {
                GoalKind::Constr(_) | GoalKind::Op(_) | GoalKind::Var(_) => expr.clone(),
                GoalKind::Abs(v, g) => {
                    let (v, g) = aux(v, g, map, expr.aux.id);
                    let g = go(&g, map);
                    GoalBase::mk_abs_t(v, g, expr.aux.clone())
                }
                GoalKind::Univ(v, g) => {
                    let (v, g) = aux(v, g, map, expr.aux.id);
                    let g = go(&g, map);
                    GoalBase::mk_univ_t(v, g, expr.aux.clone())
                }
                GoalKind::App(g1, g2) => {
                    let g1 = go(g1, map);
                    let g2 = go(g2, map);
                    GoalBase::mk_app_t(g1, g2, expr.aux.clone())
                }
                GoalKind::Conj(g1, g2) => {
                    let g1 = go(g1, map);
                    let g2 = go(g2, map);
                    GoalBase::mk_conj_t(g1, g2, expr.aux.clone())
                }
                GoalKind::Disj(g1, g2) => {
                    let g1 = go(g1, map);
                    let g2 = go(g2, map);
                    GoalBase::mk_disj_t(g1, g2, expr.aux.clone())
                }
                GoalKind::ITE(_, _, _) => todo!(),
            }
        }
        go(self, map)
    }

    /// Assign a fresh id to each subexpression of the given goal
    fn update_ids(&self) -> Self {
        let mut expr = match self.kind() {
            GoalKind::Constr(_) | GoalKind::Op(_) | GoalKind::Var(_) => self.clone(),
            GoalKind::App(x, y) => {
                let x = x.update_ids();
                let y = y.update_ids();
                Self::mk_app(x, y)
            }
            GoalKind::Conj(x, y) => {
                let x = x.update_ids();
                let y = y.update_ids();
                Self::mk_conj(x, y)
            }
            GoalKind::Disj(x, y) => {
                let x = x.update_ids();
                let y = y.update_ids();
                Self::mk_disj(x, y)
            }
            GoalKind::Abs(v, x) => {
                let x = x.update_ids();
                Self::mk_abs(v.clone(), x)
            }
            GoalKind::Univ(v, x) => {
                let x = x.update_ids();
                Self::mk_univ(v.clone(), x)
            }
            GoalKind::ITE(_, _, _) => todo!(),
        };
        expr.aux = self.aux.update_id();
        expr
    }
    /// Walks through subexpressions, and fills their free variables of type int
    fn calculate_free_variables(self) -> G {
        fn go(g: &G, ints: Stack<Ident>) -> G {
            let mut g = match g.kind() {
                GoalKind::Constr(_) | GoalKind::Op(_) | GoalKind::Var(_) => g.clone(),
                GoalKind::Abs(x, g2) => {
                    let x = x.clone();
                    let ints = if x.ty.is_int() {
                        ints.push(x.id)
                    } else {
                        ints.clone()
                    };
                    let g2 = go(g2, ints.clone());
                    G::mk_abs_t(x, g2, g.aux.clone())
                }
                GoalKind::App(g1, g2) => {
                    let g1 = go(g1, ints.clone());
                    let g2 = go(g2, ints.clone());
                    G::mk_app_t(g1, g2, g.aux.clone())
                }
                GoalKind::Conj(g1, g2) => {
                    let g1 = go(g1, ints.clone());
                    let g2 = go(g2, ints.clone());
                    G::mk_conj_t(g1, g2, g.aux.clone())
                }
                GoalKind::Disj(g1, g2) => {
                    let g1 = go(g1, ints.clone());
                    let g2 = go(g2, ints.clone());
                    G::mk_disj_t(g1, g2, g.aux.clone())
                }
                GoalKind::Univ(x, g2) => {
                    let x = x.clone();
                    let ints = if x.ty.is_int() {
                        ints.push(x.id)
                    } else {
                        ints.clone()
                    };
                    let g2 = go(g2, ints);
                    G::mk_univ_t(x, g2, g.aux.clone())
                }
                GoalKind::ITE(_, _, _) => todo!(),
            };
            g.aux.ints = ints.clone();
            g
        }
        go(&self, Stack::new())
    }

    /// Walks through subexpressions of the given problem, and fills the type information (TypeMemory.sty) in TypeMemory
    fn calculate_sty(self, problem: &Problem) -> G {
        fn go(g: &G, env: HashTrieMap<Ident, STy>) -> (G, STy) {
            let (mut g, sty) = match g.kind() {
                GoalKind::Constr(_) => (g.clone(), STy::mk_type_prop()),
                GoalKind::Op(_) => (g.clone(), STy::mk_type_int()),
                GoalKind::Var(x) => {
                    let g = g.clone();
                    let sty = env.get(x).unwrap().clone();
                    (g, sty)
                }
                GoalKind::Abs(x, g2) => {
                    let x = x.clone();
                    let env = env.insert(x.id, x.ty.clone());
                    let (g2, ty) = go(g2, env);
                    let g = G::mk_abs_t(x.clone(), g2, g.aux.clone());
                    (g, STy::mk_type_arrow(x.ty.clone(), ty))
                }
                GoalKind::Univ(x, g2) => {
                    let x = x.clone();
                    let env = env.insert(x.id, x.ty.clone());
                    let (g2, t) = go(g2, env);
                    let g = G::mk_univ_t(x, g2, g.aux.clone());
                    (g, t.clone())
                }
                GoalKind::App(g1, g2) => {
                    let (g1, t1) = go(g1, env.clone());
                    let (g2, t2) = go(g2, env.clone());
                    let (t, s) = t1.arrow();
                    assert_eq!(t, &t2);
                    let app = G::mk_app_t(g1, g2, g.aux.clone());
                    (app, s.clone())
                }
                GoalKind::Conj(g1, g2) => {
                    let (g1, _) = go(g1, env.clone());
                    let (g2, _) = go(g2, env.clone());
                    let g = G::mk_conj_t(g1, g2, g.aux.clone());
                    (g, STy::mk_type_prop())
                }
                GoalKind::Disj(g1, g2) => {
                    let (g1, _) = go(g1, env.clone());
                    let (g2, _) = go(g2, env.clone());
                    let g = G::mk_disj_t(g1, g2, g.aux.clone());
                    (g, STy::mk_type_prop())
                }
                GoalKind::ITE(_, _, _) => todo!(),
            };
            g.aux.sty = Some(sty.clone());
            (g, sty)
        }
        let mut env = HashTrieMap::new();
        for c in problem.clauses.iter() {
            env.insert_mut(c.head.id, c.head.ty.clone());
        }
        let (g, _) = go(&self, env);
        g
    }
    /// eta expand the goal so that all the occurrences of App(App(...)) has type *
    // assumption: aux.sty is filled
    fn eta_expand(&self) -> Self {
        fn handle_app(g: &G) -> G {
            match g.kind() {
                GoalKind::App(g1, g2) => {
                    let g1 = handle_app(g1);
                    let g2 = g2.eta_expand();
                    G::mk_app_t(g1, g2, g.aux.clone())
                }
                _ => g.eta_expand(),
            }
        }
        fn eta_expand_inner(g: G, ty: &STy) -> G {
            match ty.kind() {
                formula::TypeKind::Proposition => g,
                formula::TypeKind::Integer | formula::TypeKind::Bit => panic!("program error"),
                formula::TypeKind::Arrow(arg, rty) => {
                    let g = eta_expand_inner(g, rty);
                    let id = Ident::fresh();
                    let arg_v = Variable::mk(id, arg.clone());
                    let mut aux = TypeMemory::new();
                    aux.sty = Some(rty.clone());
                    G::mk_abs_t(arg_v, g, aux)
                }
            }
        }
        match self.kind() {
            GoalKind::Constr(_) | GoalKind::Op(_) | GoalKind::Var(_) => self.clone(),
            GoalKind::Abs(x, g) => {
                let g = g.eta_expand();
                G::mk_abs_t(x.clone(), g, self.aux.clone())
            }
            GoalKind::App(g1, g2) => {
                let g2 = g2.eta_expand();
                let g1 = handle_app(g1);
                let g = G::mk_app_t(g1, g2, self.aux.clone());
                if g.aux.sty.clone().unwrap().is_prop() {
                    g
                } else {
                    let sty = g.aux.sty.clone().unwrap();
                    eta_expand_inner(g, &sty)
                }
            }
            GoalKind::Conj(g1, g2) => {
                let g1 = g1.eta_expand();
                let g2 = g2.eta_expand();
                G::mk_conj_t(g1, g2, self.aux.clone())
            }
            GoalKind::Disj(g1, g2) => {
                let g1 = g1.eta_expand();
                let g2 = g2.eta_expand();
                G::mk_disj_t(g1, g2, self.aux.clone())
            }
            GoalKind::Univ(x, g) => {
                let g = g.eta_expand();
                G::mk_univ_t(x.clone(), g, self.aux.clone())
            }
            GoalKind::ITE(_, _, _) => todo!(),
        }
    }
}

/// Generates a reduction sequence to the normal form of the given goal.
/// Returns a tuple of the sequence and the normal form.
fn generate_reduction_sequence(goal: &G, optimizer: &mut dyn Optimizer) -> (Vec<Reduction>, G) {
    // aux function: do a reduction with multiple arguments
    fn reduction(
        optimizer: &mut dyn Optimizer,
        mut level: usize,
        idents: &HashSet<Ident>, // idents passed to optimizer when generating a shared type template
        args: Stack<G>,
        predicate: &G,
    ) -> (G, Vec<ReductionInfo>) {
        let mut predicate = predicate.clone();
        let mut reduction_infos = Vec::new();
        let mut int_reduction_pairs = Stack::new();
        for arg in args.iter() {
            let mut arg = arg.clone();
            // track the type of argument
            arg.aux.add_arg_level(level);
            let p = predicate.clone();
            let (arg_var, body) = p.abs();

            // [feature shared_ty]
            // introduce type sharing
            match arg.aux.tys {
                // if a shared type exists, we reuse it.
                Some(_) => (),
                None => {
                    let vi = variable_info(level, arg_var.clone(), idents);
                    let tys = optimizer.gen_type(&vi);
                    arg.aux.set_tys(tys);
                }
            }

            let reduction_type = if arg_var.ty.is_int() {
                let op: Op = arg.clone().into();
                let x = arg_var.id;
                int_reduction_pairs.push_mut((x, op));
                predicate = body.clone();
                ReductionType::int()
            } else {
                predicate = body.subst(&arg_var, &arg);
                ReductionType::pred()
            };

            let reduction_info = ReductionInfo::new(
                level,
                arg.clone(),
                arg_var.clone(),
                arg_var.id,
                reduction_type,
            );
            reduction_infos.push(reduction_info);
            level += 1;
        }
        predicate = predicate.update_ids();

        debug_assert!(predicate.aux.sty.clone().unwrap().is_prop());

        for (x, v) in int_reduction_pairs.iter() {
            let guard = Constraint::mk_eq(Op::mk_var(*x), v.clone());
            let mut tm = TypeMemory::new();
            tm.sty = Some(STy::mk_type_prop());
            predicate = G::mk_imply_t(guard, predicate, tm).unwrap();

            let mut tm = TypeMemory::new();
            tm.sty = Some(STy::mk_type_prop());
            predicate = G::mk_univ_t(Variable::mk(*x, STy::mk_type_int()), predicate, tm);
        }
        (predicate, reduction_infos)
    }
    // if (abs, arg1, arg2, ...) then, do the reduction
    fn generate_reduction_info(
        optimizer: &mut dyn Optimizer,
        level: usize,
        predicate: &G,
        idents: &HashSet<Ident>, // idents passed to optimizer when generating a shared type template
        args: Stack<G>,
    ) -> Option<(G, Vec<ReductionInfo>)> {
        match predicate.kind() {
            GoalKind::Abs(_x, _g) => Some(reduction(optimizer, level, idents, args, predicate)),
            _ => None,
        }
    }
    /// returns Some(_) if reduction happens in goal; otherwise None
    /// left of the return value: the reduced term
    /// right of the return value: the abstraction in the redux.
    fn go_(
        opt: &mut dyn Optimizer,
        goal: &G,
        lv: usize,
        fvints: &mut HashSet<Ident>,
        argints: &mut HashSet<Ident>,
        cnstr: &Constraint,
        args: Stack<G>,
        // third elem is true if it is just reduced in this call
    ) -> Option<(G, Reduction, bool)> {
        match goal.kind() {
            GoalKind::App(predicate, arg) => {
                // TODO: fvints & argints <- polymorphic_type config should be used to determine which to use
                match generate_reduction_info(opt, lv, predicate, fvints, args.push(arg.clone())) {
                    // App(App(...(Abs(...) arg1) .. argn)
                    Some((ret, reduction_infos)) => {
                        let reduction = Reduction::new(
                            goal.clone(),
                            predicate.clone(),
                            ret.clone(),
                            reduction_infos,
                            fvints.clone(),
                            argints.clone(),
                            cnstr.clone(),
                        );

                        return Some((ret.clone(), reduction, true));
                    }
                    None => (),
                };
                // case where App(pred, arg) cannot be reduced.
                // then, we try to reduce pred or arg.
                go_(
                    opt,
                    predicate,
                    lv,
                    fvints,
                    argints,
                    cnstr,
                    args.push(arg.clone()),
                )
                .map(|(ret, reduction, just_reduced)| {
                    if just_reduced {
                        (ret, reduction, true)
                    } else {
                        (
                            G::mk_app_t(ret, arg.clone(), goal.aux.clone()),
                            reduction,
                            just_reduced,
                        )
                    }
                })
                .or_else(|| {
                    go_(opt, arg, lv, fvints, argints, cnstr, Stack::new()).map(|(arg, pred, _)| {
                        (
                            G::mk_app_t(predicate.clone(), arg, goal.aux.clone()),
                            pred,
                            false,
                        )
                    })
                })
            }
            GoalKind::Conj(g1, g2) => go_(opt, g1, lv, fvints, argints, cnstr, Stack::new())
                .map(|(g1, p, _)| (G::mk_conj_t(g1, g2.clone(), goal.aux.clone()), p, false))
                .or_else(|| {
                    go_(opt, g2, lv, fvints, argints, cnstr, Stack::new()).map(|(g2, p, _)| {
                        (G::mk_conj_t(g1.clone(), g2, goal.aux.clone()), p, false)
                    })
                }),
            GoalKind::Disj(g1, g2) => {
                let c1: Option<Constraint> = g1.clone().into();
                match c1 {
                    Some(c1) => {
                        let constraint = Constraint::mk_conj(c1.negate().unwrap(), cnstr.clone());
                        go_(opt, g2, lv, fvints, argints, &constraint, Stack::new()).map(
                            |(g2, p, _)| {
                                (
                                    G::mk_disj_t(g1.clone(), g2.clone(), goal.aux.clone()),
                                    p,
                                    false,
                                )
                            },
                        )
                    }
                    None => {
                        let c2: Option<Constraint> = g2.clone().into();
                        let c2 = c2.unwrap_or_else(|| panic!("fatal: g1 = {}, g2 = {}", g1, g2));
                        let constraint = Constraint::mk_conj(c2.negate().unwrap(), cnstr.clone());
                        go_(opt, g1, lv, fvints, argints, &constraint, Stack::new()).map(
                            |(g1, p, _)| {
                                (
                                    G::mk_disj_t(g1.clone(), g2.clone(), goal.aux.clone()),
                                    p,
                                    false,
                                )
                            },
                        )
                    }
                }
            }
            GoalKind::Univ(x, g) => {
                let mut saved = false;
                if x.ty.is_int() && !fvints.insert(x.id) {
                    // x is type int and fvints already has x.id
                    saved = true;
                }
                let r = go_(opt, g, lv, fvints, argints, cnstr, Stack::new())
                    .map(|(g, p, _)| (G::mk_univ_t(x.clone(), g, goal.aux.clone()), p, false));
                if x.ty.is_int() && !saved {
                    fvints.remove(&x.id);
                }
                r
            }
            GoalKind::Abs(x, g) => {
                let mut saved = false;
                let mut saved_arg = false;
                if x.ty.is_int() && !fvints.insert(x.id) {
                    // x is type int and fvints already has x.id
                    saved = true;
                }
                if x.ty.is_int() && !argints.insert(x.id) {
                    // x is type int and fvints already has x.id
                    saved_arg = true;
                }

                let r = go_(opt, g, lv, fvints, argints, cnstr, Stack::new())
                    .map(|(g, p, _)| (G::mk_abs_t(x.clone(), g, goal.aux.clone()), p, false));
                if x.ty.is_int() && !saved {
                    fvints.remove(&x.id);
                }
                if x.ty.is_int() && !saved_arg {
                    argints.remove(&x.id);
                }
                r
            }
            GoalKind::Constr(_) | GoalKind::Var(_) | GoalKind::Op(_) => None,
            GoalKind::ITE(_, _, _) => todo!(),
        }
    }
    /// returns
    /// 1. Some(Candidate): substituted an app
    /// 2. None: not yet
    fn go(optimizer: &mut dyn Optimizer, goal: &G, level: &mut usize) -> Option<(G, Reduction)> {
        go_(
            optimizer,
            goal,
            *level,
            &mut HashSet::new(),
            &mut HashSet::new(),
            &Constraint::mk_true(),
            Stack::new(),
        )
        .map(|(ret, reduction, _)| {
            *level = reduction.level() + 1;
            (ret, reduction)
        })
    }
    let mut level = 0usize;
    let mut seq = Vec::new();
    let mut reduced = goal.clone();

    debug!("{}", reduced);
    while let Some((g, mut r)) = go(optimizer, &reduced, &mut level) {
        // save the formulas before and after reduction
        r.before_reduction = reduced.clone();
        r.after_reduction = g.clone();

        reduced = g.clone();
        pdebug!("->  ", reduced);

        seq.push(r);
    }
    (seq, reduced)
}

struct Context {
    normal_form: G,
    track_idents: HashMap<Ident, Vec<Ident>>,
    reduction_sequence: Vec<Reduction>,
}

impl Context {
    fn new(
        normal_form: G,
        track_idents: HashMap<Ident, Vec<Ident>>,
        reduction_sequence: Vec<Reduction>,
    ) -> Context {
        // default
        Context {
            normal_form,
            track_idents,
            reduction_sequence,
        }
    }
    fn retrieve_from_track_idents(&self, model: &chc::Model, derivation: &Derivation) -> TyEnv {
        let model = &model.model;
        let mut result_env = TypeEnvironment::new();
        for (pred_name, ids) in self.track_idents.iter() {
            for id in ids {
                for node in derivation.get_nodes_by_goal_id(id) {
                    let ty = &node.item.ty;
                    let constraint = node.item.constraint();
                    debug!("{} {}({}): {}", constraint, pred_name, id, ty);
                    let ty = ty.conjoin_constraint_to_rty(&constraint);
                    let ty = ty.assign(&model);
                    let pty = Tau::poly(ty);
                    result_env.add(*pred_name, pty);
                }
            }
        }
        result_env
    }
    /// Subject expansion with respect to the given reduction
    fn subject_expansion_wrt_reduction(&self, derivation: &mut Derivation, reduction: &Reduction) {
        title!("Reduction");
        pdebug!(reduction);
        let node_ids: Stack<_> = derivation
            .get_nodes_by_goal_id(&reduction.result.aux.id)
            .map(|n| n.id)
            .collect();
        if node_ids.iter().len() == 0 {
            pdebug!(
                "search for id=",
                reduction.result.aux.id,
                ",expr=",
                reduction.result,
                " ",
            );
            title!("no ret_tys");
            panic!("fatal");
        }

        for node_id in node_ids.iter() {
            derivation.expand_node(*node_id, reduction);

            #[cfg(debug_assertions)]
            derivation.check_subsumption_correct();
        }
    }
    fn infer_with_shared_type(
        &mut self,
        derivation: &Derivation,
    ) -> Option<(Model, Vec<chc::CHC<chc::Atom, Constraint>>, Derivation)> {
        let c1 = derive_tree::CloneConfiguration::new()
            .mode_shared(true)
            .polymorphic(false);
        let c2 = derive_tree::CloneConfiguration::new()
            .mode_shared(true)
            .polymorphic(true);

        let configurations = vec![c1, c2];
        //let configurations = vec![c2];
        for c in configurations {
            let d = derivation.clone_with_template(c);
            let clauses: Vec<_> = d.collect_chcs(false).collect();

            crate::title!("infer_with_shared_type");
            pdebug!(d);
            for c in clauses.iter() {
                pdebug!(c);
            }

            match solver::chc::default_solver().solve(&clauses) {
                solver::chc::CHCResult::Sat(m) => return Some((m, clauses, d)),
                _ => {}
            }
        }
        debug!("infer_with_shared_type: unsat");
        None
    }
    fn infer_type_with_subject_expansion(
        &self,
        derivation: Derivation,
    ) -> Option<(Model, Vec<chc::CHC<chc::Atom, Constraint>>, Derivation)> {
        let d = derivation.clone();
        title!("interpolation");
        let clauses: Vec<_> = d.collect_chcs(true).collect();
        for c in clauses.iter() {
            pdebug!(c);
        }
        // TODO: insert template types for each use of lambda abstraction

        match solver::chc::default_solver().solve(&clauses) {
            solver::chc::CHCResult::Sat(m) => Some((m, clauses, derivation)),
            solver::chc::CHCResult::Unsat => {
                // this should not happen?
                panic!("fail!")
            }
            solver::chc::CHCResult::Unknown => {
                panic!(
                    "PDR fails to infer a refinement type due to the background CHC solver's error"
                )
            }
            solver::chc::CHCResult::Timeout => panic!(
                "PDR fails to infer a refinement type due to timeout of the background CHC solver"
            ),
        }
    }
    fn infer_type(&mut self, mut derivation: Derivation) -> Option<TyEnv> {
        derivation = derivation.prepare_for_subject_expansion();

        pdebug!("derivation with templates"; title);
        pdebug!(derivation);

        #[cfg(debug_assertions)]
        derivation.check_subsumption_correct();

        pinfo!("infer_type"; title);
        for reduction in self.reduction_sequence.iter().rev() {
            let level = reduction.level();
            pinfo!("derivation ", level);
            self.subject_expansion_wrt_reduction(&mut derivation, reduction);
            pinfo!(derivation);
            debug!("checking sanity... {}", derivation.check_sanity(false));
        }

        // try to infer a type with shared type.
        let (m, clauses, derivation) = match self.infer_with_shared_type(&derivation) {
            Some(m) => m,
            None => self.infer_type_with_subject_expansion(derivation)?,
        };

        pdebug!("model from CHC solver"; title);
        pdebug!(m);
        pdebug!("clauses"; title);
        for c in clauses.iter() {
            debug!("{}", c);
        }
        let config = solver::interpolation::InterpolationConfig::new().use_chc_if_requied();
        let model = solver::interpolation::solve(&clauses, &config);
        pdebug!("interpolated"; title);
        pdebug!(model);

        // ** check if the returned model is "tractable" **
        // Here, tracktable means no constraint in model does not contain existential quantifier.
        //
        // If the shared predicate is enabled, the resulting constraints are no longer acyclic-CHCs,
        // but CHCs (can contain recursive predicates).
        // Due to the background CHC solvers, in some cases, solutions can contain existential quantifers.
        // I know that hoice can return such a solution when the the given problem is so easy that its preprocessor
        // can solve it, and that there is an option to avoid such situations (simplify-clauses: false).
        // However, it seems totally an experimental feature, and at least currently, it is not functional.
        // Therefore, if we find that the solution in model contains some existential quanfiers,
        // we just return None even though it actually exists.
        if model.is_solution_tractable() {
            // collect needed predicate
            // 5. from the model, generate a type environment
            Some(self.retrieve_from_track_idents(&model, &derivation))
        } else {
            warn!("solution from CHC is untractable");
            None
        }
    }
}

fn handle_abs(
    config: &TCConfig,
    tenv: &mut Env,
    ienv: &mut HashSet<Ident>,
    all_coefficients: &mut HashSet<Ident>,
    arg_expr: &G,
    t: &Ty,
    context: &Stack<Atom>,
) -> PossibleDerivation {
    fn handle_abs_inner(
        config: &TCConfig,
        tenv: &mut Env,
        ienv: &mut HashSet<Ident>,
        all_coefficients: &mut HashSet<Ident>,
        arg_expr: &G,
        t: &Ty,
        context: &Stack<Atom>,
    ) -> PossibleDerivation {
        let pt = match arg_expr.kind() {
            GoalKind::Abs(v, g) if v.ty.is_int() => match t.kind() {
                TauKind::IArrow(id, t) if v.ty.is_int() => {
                    let t = t.rename(id, &v.id);
                    let b = ienv.insert(v.id);
                    let pt = handle_abs_inner(config, tenv, ienv, all_coefficients, g, &t, context);
                    if b {
                        ienv.remove(&v.id);
                    }
                    pt.iarrow(context.clone(), arg_expr.clone(), &v.id)
                }
                _ => panic!("program error"),
            },
            GoalKind::Abs(v, g) => match t.kind() {
                TauKind::Arrow(ts, t) if !v.ty.is_int() => {
                    let mut tenv = tenv.clone();
                    for t in ts {
                        debug!("adding type {t} to tenv");
                        tenv.add(v.id, t.clone())
                    }
                    let pt =
                        handle_abs_inner(config, &mut tenv, ienv, all_coefficients, g, t, context);
                    pt.arrow(context.clone(), arg_expr.clone(), ts)
                }
                _ => panic!("fatal"),
            },
            _ => {
                let pt = type_check_body(config, tenv, ienv, all_coefficients, arg_expr, context);
                // skip the continuation of this inner function
                return pt.coarse_type(context.clone(), t);
            }
        };
        pdebug!("handle_abs: |- ", arg_expr, " :",  pt ; bold ; white, " ",);
        pt
    }
    match t.kind() {
        TauKind::PTy(x, t) => {
            let flag = ienv.insert(*x);
            let pt = handle_abs(config, tenv, ienv, all_coefficients, arg_expr, t, context);
            if flag {
                ienv.remove(x);
            }
            pt.pty(context.clone(), x)
        }
        _ => handle_abs_inner(config, tenv, ienv, all_coefficients, arg_expr, t, context),
    }
}

#[derive(Clone)]
struct TCConfig {
    tc_mode: TCFlag,
    // TODO
    #[allow(dead_code)]
    construct_derivation: bool,
}

#[derive(Clone)]
struct InstantiationConfig {
    derivation: Derivation,
}

#[derive(Clone)]
enum TCFlag {
    Normal,
    Shared(InstantiationConfig),
}

fn coarse_expr_for_type_sharing(
    config: &TCConfig,
    pt: PossibleDerivation,
    expr: &G,
) -> PossibleDerivation {
    // [feature shared_ty] template type sharing
    // if there is a shared type registered, coarse pt to obey the type.
    match (&config.tc_mode, &expr.aux.tys) {
        (TCFlag::Shared(_), Some(tys)) if tys.len() == 1 =>
        /*
        pt.coarse_type(cty, &tys[0])*/
        {
            unimplemented!()
        }
        (TCFlag::Normal, _) | (_, None) => pt,
        (_, _) => unimplemented!(),
    }
}

// tenv+ienv; constraint |- App(arg, ret): t
/// returns possible types for app_expr under constraint
fn handle_app(
    config: &TCConfig,
    tenv: &mut Env,
    ienv: &mut HashSet<Ident>,
    all_coefficients: &mut HashSet<Ident>,
    app_expr: &G,
    cty: &Stack<Atom>,
) -> PossibleDerivation {
    fn handle_inner(
        config: &TCConfig,
        tenv: &mut Env,
        ienv: &mut HashSet<Ident>,
        all_coefficients: &mut HashSet<Ident>,
        pred_expr: &G,
        cty: &Stack<Atom>, // context
    ) -> PossibleDerivation {
        match pred_expr.kind() {
            formula::hes::GoalKind::Var(x) => {
                handle_var(config, tenv, ienv, all_coefficients, pred_expr, cty, x)
            }
            formula::hes::GoalKind::App(predg, argg) => {
                let pred_pt = handle_app(config, tenv, ienv, all_coefficients, predg, cty);
                // Case: the argument is integer
                match argg.check_int_expr(ienv) {
                    // Case: the type of argument is int
                    Some(op) => {
                        let types = pred_pt
                            .types
                            .into_iter()
                            .map(|d| Derivation::rule_iapp(cty.clone(), pred_expr.clone(), d, &op))
                            .collect();
                        return PossibleDerivation::new(types); // early return
                    }
                    // Otherwise, we continue.
                    None => (),
                };

                // Case: the argument is not integer
                let mut result_cts = Vec::new();
                // we calculate the argument's type. we have to enumerate all the possible type of pt1.

                for pred_derivation in pred_pt.types {
                    let (arg_t, _result_t) = match pred_derivation.root_ty().kind() {
                        TauKind::Arrow(arg, result) => (arg, result),
                        TauKind::PTy(_, _) | TauKind::Proposition(_) | TauKind::IArrow(_, _) => {
                            panic!("fatal")
                        }
                    };
                    let mut arg_derivations = vec![Stack::new()];
                    // check if there exists a derivation for all types in the intersection type.
                    let arg_pts = arg_t.iter().map(|t| {
                        // check if arg_constraint |- argg: arg_t
                        debug!("arg_t: {t}");
                        handle_abs(config, tenv, ienv, all_coefficients, argg, t, &cty)
                    });
                    // Assume pred_pt = t1 /\ t2 -> t
                    // then, we have to derive arg_t: t1 and arg_t: t2
                    // for arg_t: t1, assume there are two derivations d1 and d2 (= arg_pts[0])
                    // for arg_t: t2, also d3 and d4 (= arg_pts[1])
                    // then possible derivations are [d1, d3], [d1, d4],  [d2, d3] and [d2, d4]
                    for pt in arg_pts {
                        let mut new_tmp_cts = Vec::new();
                        // di = d1
                        for di in pt.types {
                            // merge
                            for arg_derivations_so_far in arg_derivations.iter() {
                                new_tmp_cts.push(arg_derivations_so_far.push(di.clone()));
                            }
                        }
                        arg_derivations = new_tmp_cts
                    }
                    // Now that all the argument for `pred_pt` can be derived, we have candidatetype `result_t`
                    // with the derivations of `ct`s
                    for arg_derivation in arg_derivations {
                        result_cts.push(Derivation::rule_app(
                            cty.clone(),
                            pred_expr.clone(),
                            pred_derivation.clone(),
                            arg_derivation
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev(),
                        ));
                    }
                }
                PossibleDerivation::new(result_cts)
            }
            GoalKind::Constr(_)
            | GoalKind::Op(_)
            | GoalKind::Abs(_, _)
            | GoalKind::Conj(_, _)
            | GoalKind::Disj(_, _)
            | GoalKind::Univ(_, _) => panic!("fatal: {}", pred_expr),
            GoalKind::ITE(_, _, _) => todo!(),
        }
    }
    let pt = handle_inner(config, tenv, ienv, all_coefficients, app_expr, cty);

    let pt = coarse_expr_for_type_sharing(config, pt, &app_expr);

    pdebug!("handle_app: |- ", app_expr, " : ", pt ; bold ; white, " ",);
    pt
}

/// Internal function for `type_check_inner` for handling variables
fn handle_var(
    config: &TCConfig,
    tenv: &mut Env,
    ienv: &mut HashSet<Ident>,
    all_coefficients: &mut HashSet<Ident>,
    expr: &G,
    context_ty: &Stack<Atom>,
    x: &Ident,
) -> PossibleDerivation {
    match tenv.get(x) {
        Some(ts) => {
            // [feature shared_ty] if shared type mode is enabled, instantiate the polymorphic types
            // in the same way as the previous instantiation that succeeded.
            let tys = match &config.tc_mode {
                TCFlag::Normal => {
                    let mut tys = Vec::new();
                    for original_ty in ts {
                        let mut coefficients = Stack::new();
                        let (ty, instantiations) =
                            original_ty.instantiate_with_linear_template(ienv, &mut coefficients);
                        debug!("instantiate_type ienv: {:?}", ienv);
                        debug!("instantiated: {ty}");
                        coefficients.iter().for_each(|c| {
                            all_coefficients.insert(*c);
                        });

                        let cd = Derivation::rule_var(
                            context_ty.clone(),
                            expr.clone(),
                            ty,
                            original_ty.clone(),
                            coefficients,
                            instantiations,
                        );
                        tys.push(cd);
                    }
                    tys
                }
                TCFlag::Shared(d) => {
                    // TODO: actually we have to consider the case where multiple types are assigned to one expr
                    // This happens when intersection types are inferred; however, in the current setting,
                    // if shared type is enabled, there is no intersection types.
                    // So I just leave it as a future work.
                    // To tackle this issue, there are multiple methods:
                    //   1. track the current branches that we have passed so far every time intersection rules have been applied
                    //   2. track the current node of the previous derivation in d, so that corresponding node of d can easily be
                    //      retrieved
                    //   3. ?
                    let nodes: Vec<_> = d.derivation.get_nodes_by_goal_id(&expr.aux.id).collect();
                    assert!(nodes.len() == 1);
                    vec![Derivation::single_node(nodes[0].item.clone())]
                }
            };
            PossibleDerivation::new(tys)
        }
        None => PossibleDerivation::empty(),
    }
}

/// body of type checking;  tenv; ienv |- c : contex_ty
/// do some check in bottom up manner
fn type_check_body(
    config: &TCConfig,
    tenv: &mut Env,
    ienv: &mut HashSet<Ident>, // V
    all_coefs: &mut HashSet<Ident>,
    expr: &G,
    context_ty: &Stack<Atom>,
) -> PossibleDerivation {
    // for App, we delegate the procedure to `handle_app`
    // and in that procedure, it saves the types
    let result_pt = match expr.kind() {
        formula::hes::GoalKind::Constr(constraint) => {
            let constraint = constraint.clone().into();
            let t = Ty::mk_prop_ty(constraint);
            let cd = Derivation::rule_atom(context_ty.clone(), expr.clone(), t);
            PossibleDerivation::singleton(cd)
        }
        formula::hes::GoalKind::Var(x) => {
            handle_var(config, tenv, ienv, all_coefs, expr, context_ty, x)
        }
        formula::hes::GoalKind::Conj(g1, g2) => {
            let t1 = type_check_body(config, tenv, ienv, all_coefs, g1, context_ty);
            let t2 = type_check_body(config, tenv, ienv, all_coefs, g2, context_ty);
            PossibleDerivation::conjoin(context_ty.clone(), expr.clone(), t1, t2)
        }
        formula::hes::GoalKind::Disj(g1, g2) => {
            let c1: Option<Constraint> = g1.clone().into();
            let c2: Option<Constraint> = g2.clone().into();
            match (c1, c2) {
                (Some(c1), _) => {
                    let ctx = context_ty.push(c1.clone().into());
                    let t1 = type_check_body(config, tenv, ienv, all_coefs, g1, &ctx);
                    let ctx = context_ty.push(c1.negate().unwrap().into());
                    let t2 = type_check_body(config, tenv, ienv, all_coefs, g2, &ctx);
                    PossibleDerivation::disjoin(context_ty.clone(), expr.clone(), t1, t2)
                }
                (_, Some(c2)) => {
                    let ctx = context_ty.push(c2.negate().unwrap().into());
                    let t1 = type_check_body(config, tenv, ienv, all_coefs, g1, &ctx);
                    let ctx = context_ty.push(c2.into());
                    let t2 = type_check_body(config, tenv, ienv, all_coefs, g2, &ctx);
                    PossibleDerivation::disjoin(context_ty.clone(), expr.clone(), t1, t2)
                }
                (_, _) => {
                    panic!("program error")
                }
            }
        }
        formula::hes::GoalKind::Univ(x, g) => {
            let b = ienv.insert(x.id);
            let mut pt = type_check_body(config, tenv, ienv, all_coefs, &g, context_ty);
            if b {
                ienv.remove(&x.id);
            }
            // quantify all the constraint.
            pt.quantify(context_ty.clone(), expr.clone(), &x.id);
            pt
        }
        formula::hes::GoalKind::App(_, _) => {
            handle_app(config, tenv, ienv, all_coefs, expr, context_ty)
        }
        formula::hes::GoalKind::Abs(_v, _g) => {
            panic!("fatal error")
        }
        // op is always handled by App(x, op)
        formula::hes::GoalKind::Op(_) => panic!("fatal error"),
        formula::hes::GoalKind::ITE(_, _, _) => todo!(),
    };
    pdebug!("type_check_go(", expr.aux.id, ") |- ", expr, " : ", result_pt ; bold);
    result_pt
}

// we assume conjunction normal form and has the form (θ => a₁ a₂ ⋯) ∧ ⋯
/// V; Γ ⊢ c : t
/// function go constructs possible derivation trees by induction on the structure of c(ψ)
///
fn type_check(
    tenv: &mut Env,
    ienv: &mut HashSet<Ident>, // V
    c: &G,
    t: &Ty,
) -> bool {
    let mut all_coefficients = HashSet::new();
    let config = TCConfig {
        tc_mode: TCFlag::Normal,
        construct_derivation: false,
    };
    let pt = handle_abs(
        &config,
        tenv,
        ienv,
        &mut all_coefficients,
        c,
        &t,
        &Stack::new(),
    );
    match pt.check_derivation() {
        Some(d) => {
            debug_assert!(d.check_sanity(false));
            true
        }
        None => false,
    }
}

/// ε; true ; Γ ⊢ ψ : •<T>
///
/// tenv: Γ
/// candidate: ψ
/// assumption: candidate has a beta-normal form of type *.
fn type_check_top_with_derivation(psi: &G, tenv: &mut Env) -> Option<Derivation> {
    title!("type_check_top");
    debug!("tenv: {}", tenv);
    debug!("target: {}", psi);
    let mut ienv = HashSet::new();
    let mut all_coefficients = HashSet::new();
    let config = TCConfig {
        tc_mode: TCFlag::Normal,
        construct_derivation: true,
    };
    let pt = type_check_body(
        &config,
        tenv,
        &mut ienv,
        &mut all_coefficients,
        &psi,
        &Stack::new(),
    );
    let pt = pt.coarse_type(Stack::new(), &Ty::mk_prop_ty(Atom::mk_true()));

    // check if there is an actually possible derivation
    pt.check_derivation().map(|d| {
        debug_assert!(d.check_sanity(false));
        d
    })
}

/// ε; true ; Γ ⊢ ψ : •<T>
///
/// tenv: Γ
/// candidate: ψ
/// assumption: candidate has a beta-normal form of type *.
fn type_check_top_with_derivation_and_constraints(
    previous_derivation: Derivation,
    psi: &G,
    tenv: &mut Env,
) -> Derivation {
    title!("type_check_top_with_derivation_and_constraints");
    // using previous derivation,
    // constraints that are required for shared types can be generated.
    let mut ienv = HashSet::new();
    let mut all_coefficients = HashSet::new();
    let ic = InstantiationConfig {
        derivation: previous_derivation,
    };
    let config = TCConfig {
        tc_mode: TCFlag::Shared(ic),
        construct_derivation: true,
    };
    let pt = type_check_body(
        &config,
        tenv,
        &mut ienv,
        &mut all_coefficients,
        &psi,
        &Stack::new(),
    );
    let mut pt = pt.coarse_type(Stack::new(), &Ty::mk_prop_ty(Atom::mk_true()));

    // check if there is an actually possible derivation
    // since the derivation has the same shape as `previous_derivation`,
    // we do not have more than one derivation in pt.
    assert!(pt.types.len() == 1);
    let derivation = pt.types.remove(0);

    derivation
}

/// Γ ⊢ ψ : •<T>
///
/// tenv: Γ
/// candidate: ψ
/// assumption: candidate has a beta-normal form of type *.
pub fn type_check_top(candidate: &Candidate, tenv: &TyEnv) -> bool {
    let g = candidate.clone().into();
    let mut tenv = tenv.into();
    let b = type_check_top_with_derivation(&g, &mut tenv).is_some();
    b
}

/// Reduces the given candidate to the normal form
/// and appends auxiliary information to the candidate.
fn reduce_until_normal_form(
    candidate: &Candidate,
    problem: &Problem,
    optimizer: &mut dyn Optimizer,
) -> Context {
    let mut track_idents = HashMap::new();
    let candidate = candidate.clone().into(); // assign `aux` to candidate.
    let goal = subst_predicate(&candidate, problem, &mut track_idents);
    let goal = goal.alpha_renaming();
    // calculate free variables for each term
    // aux.sty is filled
    let goal = goal.calculate_sty(problem);
    // eta_expansion requires sty info
    let goal = goal.eta_expand();
    // aux.ints is filled
    let goal = goal.calculate_free_variables();
    title!("generate_reduction_sequence");
    let (reduction_sequence, normal_form) = generate_reduction_sequence(&goal, optimizer);
    Context::new(normal_form, track_idents, reduction_sequence)
}

/// Represents a set of derivations
///
/// Since type environment can contain multiple candidate types,
/// we make sure that which one is suitable by considering them parallely.
struct PossibleDerivation {
    types: Vec<Derivation>,
}

impl Pretty for PossibleDerivation {
    fn pretty<'b, D, A>(
        &'b self,
        al: &'b D,
        config: &mut crate::util::printer::Config,
    ) -> pretty::DocBuilder<'b, D, A>
    where
        D: pretty::DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        let docs = self.types.iter().map(|t| t.root_ty().pretty(al, config));
        al.intersperse(docs, al.line().append(al.text("/\\ ")))
            .hang(2)
            .group()
    }
}

/// Utilities for `PossibleDerivation`
impl PossibleDerivation {
    /// Creates a new `PossibleDerivation` with the given derivations
    fn new(types: Vec<Derivation>) -> Self {
        PossibleDerivation { types }
    }

    /// Creates an empty `PossibleDerivation`
    fn empty() -> Self {
        PossibleDerivation::new(Vec::new())
    }

    /// Creates a singleton `PossibleDerivation` from the given derivation
    fn singleton(cd: Derivation) -> Self {
        Self::new(vec![cd])
    }

    /// Generate a new `PossibleDerivation` from two `PossibleDerivation`s by
    /// introducing Conjoin rule to each pair of derivations.
    fn conjoin(context: Stack<Atom>, expr: G, pt1: Self, pt2: Self) -> Self {
        let mut ts = Vec::new();
        for d1 in pt1.types.iter() {
            for d2 in pt2.types.iter() {
                let d1 = d1.clone();
                let d2 = d2.clone();
                ts.push(Derivation::rule_conjoin(
                    context.clone(),
                    expr.clone(),
                    d1,
                    d2,
                ));
            }
        }
        PossibleDerivation::new(ts)
    }

    /// Generate a new `PossibleDerivation` from two `PossibleDerivation`s by
    /// introducing Disjoin rule to each pair of derivations.
    fn disjoin(context: Stack<Atom>, expr: G, pt1: Self, pt2: Self) -> Self {
        let mut ts = Vec::new();
        for d1 in pt1.types.iter() {
            for d2 in pt2.types.iter() {
                let d1 = d1.clone();
                let d2 = d2.clone();
                ts.push(Derivation::rule_disjoin(
                    context.clone(),
                    expr.clone(),
                    d1,
                    d2,
                ));
            }
        }
        PossibleDerivation::new(ts)
    }

    /// Introduces Univ Rule to each derivation in the `PossibleDerivation`
    fn quantify(&mut self, context: Stack<Atom>, expr: G, x: &Ident) {
        self.types = self
            .types
            .iter()
            .cloned()
            .map(|d| Derivation::rule_quantifier(context.clone(), expr.clone(), d, x))
            .collect();
    }

    /// Introduces IApp rule to each derivation in the `PossibleDerivation`
    fn iarrow(self, context: Stack<Atom>, expr: G, x: &Ident) -> Self {
        let types = self
            .types
            .into_iter()
            .map(|ct| Derivation::rule_iarrow(context.clone(), expr.clone(), ct, x))
            .collect();
        PossibleDerivation { types }
    }

    /// Introduces Arrow rule to each derivation in the `PossibleDerivation`
    fn arrow(self, context: Stack<Atom>, expr: G, ts: &Vec<Ty>) -> Self {
        let types = self
            .types
            .into_iter()
            .map(|d| Derivation::rule_arrow(context.clone(), expr.clone(), d, ts.to_vec()))
            .collect();
        PossibleDerivation { types }
    }

    /// Introduces pty rule to each derivation in the `PossibleDerivation`
    fn pty(self, context: Stack<Atom>, x: &Ident) -> Self {
        let types = self
            .types
            .into_iter()
            .map(|d| Derivation::rule_polymorphic_type(context.clone(), d, *x))
            .collect();
        PossibleDerivation { types }
    }
}

impl PossibleDerivation {
    fn coarse_type(mut self, context: Stack<Atom>, t: &Ty) -> Self {
        self.types = self
            .types
            .into_iter()
            .map(|d| Derivation::rule_subsumption(context.clone(), d, t.clone()))
            .collect();
        self
    }
    /// Checks if there is a valid derivation by solving constraints generated
    /// on subsumptions, and returns one if exists.
    fn check_derivation(self) -> Option<Derivation> {
        title!("check derivation");
        for mut ct in self.types.into_iter() {
            debug!("derivation");
            pdebug!(ct);
            let mut constraint = Constraint::mk_true();
            pdebug!("derivation constraints:");
            for c in ct.collect_constraints(false) {
                pdebug!(c);
                constraint = Constraint::mk_conj(constraint, c.clone().into());
            }
            debug!("check_derivation constraint: {constraint}");
            let fvs = constraint.fv();
            let exists: HashSet<Ident> = ct.coefficients.iter().cloned().collect();
            let vars = fvs.difference(&exists).cloned().collect();
            #[cfg(debug)]
            {
                debug!("variables used for coefficients of linear templates");
                let s = exists
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                debug!("exists: {s}");
            }

            let mut solver = solver::smt::smt_solver(solver::SMTSolverType::Auto);
            let m = solver.solve_with_model(&constraint, &vars, &exists);
            match m {
                Ok(m) => {
                    debug!("constraint was sat: {}", constraint);
                    debug!("model is {}", m);
                    pdebug!("derivation before update with model");
                    pdebug!(ct);
                    // replace all the integer coefficient
                    ct.update_with_model(&m);
                    pdebug!("derivation after update with model");
                    pdebug!(ct);

                    return Some(ct);
                }
                Err(_) => (),
            }
        }
        None
    }
}

#[derive(Clone, Copy)]
pub struct InferenceConfig {
    /// Enable inferring polymorphic types. If this is disabled, we restrict the variables
    /// avaialble in each template predicate so that type of predicates will not depend on
    /// variables introduced by universal quantifiers.
    pub infer_polymorphic_type: bool,
}
impl InferenceConfig {
    pub fn new() -> InferenceConfig {
        InferenceConfig {
            infer_polymorphic_type: true,
        }
    }

    /// Enable or disable inferring polymorphic types.
    pub fn infer_polymorphic_type(mut self, infer_polymorphic_type: bool) -> InferenceConfig {
        self.infer_polymorphic_type = infer_polymorphic_type;
        self
    }
}

/// Entry point of the type inference in HoPDR's Conflict
/// Infers a type environment Γ' s.t. Γ |- D: Γ' and Γ' |- ψ: *<T>, (here, Γ is tenv)
/// given a candidate φ (candidate) and a set of clauses D (problem).
///
pub fn search_for_type(
    candidate: &Candidate,
    problem: &Problem,
    tenv: &mut Env,
    config: InferenceConfig,
) -> Option<TyEnv> {
    crate::title!("search_for_type");
    debug!("{}", candidate);
    let infer_polymorphic_type = config.infer_polymorphic_type;
    // TODO: expand candidate once based on problem.
    const SHARED: bool = false;
    let mut optimizer = optimizer::VoidOptimizer::new();
    while optimizer.continuable() {
        let mut ctx = reduce_until_normal_form(candidate, problem, &mut optimizer);
        debug!("{}", ctx.normal_form);
        // When `type_check_top_with_derivation` fails, `normal_form` is untypeable.
        // In this case, we return None.
        let derivation = type_check_top_with_derivation(&ctx.normal_form, tenv)?;
        let derivation = if SHARED {
            type_check_top_with_derivation_and_constraints(derivation, &ctx.normal_form, tenv)
        } else {
            derivation
        };

        pdebug!("[derivation]");
        pdebug!(derivation);
        debug!("checking sanity... {}", derivation.check_sanity(false));
        match ctx.infer_type(derivation) {
            Some(x) => {
                optimizer.report_inference_result(InferenceResult::new(true));
                return Some(x);
            }
            None => (),
        }
        debug!("failed to interpolate the constraints");
        optimizer.report_inference_result(InferenceResult::new(false));
    }
    if infer_polymorphic_type {
        // must succeed in theory
        panic!("program error: constructing derivation failed")
    } else {
        None
    }
}

/// Takes a type environment Γ and a problem (D, φ), and check if Γ |- D: Γ.
pub fn check_inductive(env: &TyEnv, problem: &Problem) -> bool {
    let tenv: Env = env.into();
    for (id, ts) in env.map.iter() {
        let clause = problem.get_clause(id).unwrap();
        let mut env = tenv.clone();
        for t in ts.iter() {
            let t = t.clone().into();
            if !type_check(
                &mut env,
                &mut HashSet::new(),
                &clause.body.clone().into(),
                &t,
            ) {
                return false;
            }
        }
    }
    true
}

/// Takes a type environment Γ and a problem (D, φ), and returns a type environment Γ'
/// s.t. Γ' |- D: Γ' and Γ' ⊆ Γ.
/// In other words, it returns the inductive subset of Γ.
/// Note that Γ' |- φ: *<T> does not necessarily hold.
pub fn saturate(env: &TyEnv, problem: &Problem) -> TyEnv {
    let mut current_env = env.clone();
    loop {
        let mut new_env = TypeEnvironment::new();
        let mut saturated = true;
        for (id, ts) in current_env.map.iter() {
            let clause = problem.get_clause(id).unwrap();
            for t in ts.iter() {
                let mut env: Env = (&current_env).into();
                if type_check(
                    &mut env,
                    &mut HashSet::new(),
                    &clause.body.clone().into(),
                    &t.clone().into(),
                ) {
                    new_env.add(*id, t.clone());
                } else {
                    saturated = false;
                }
            }
        }
        current_env = new_env;
        if saturated {
            return current_env;
        }
    }
}
