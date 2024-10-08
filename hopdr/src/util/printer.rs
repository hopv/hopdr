use std::fmt::Display;

use crate::formula::*;
use crate::pdr::derivation::tree;
use crate::pdr::rtype;
use crate::preprocess;
use pretty::termcolor::{Color, ColorSpec};
use pretty::{BoxAllocator, BoxDoc, DocAllocator, DocBuilder};

static mut DEFAULT_WIDTH: usize = 120;
static mut COLORED: bool = true;

pub fn set_default_width(width: usize) {
    unsafe { DEFAULT_WIDTH = width - 5 }
}

pub fn get_default_width() -> usize {
    unsafe { DEFAULT_WIDTH }
}

pub fn set_colored(colored: bool) {
    unsafe { COLORED = colored }
}

pub fn colored() -> bool {
    unsafe { COLORED }
}

#[derive(Default)]
pub struct Config<'a> {
    context: Option<&'a preprocess::Context>,
    filter: bool,
}

impl<'a> Config<'a> {
    fn get_name_by_ident(&mut self, default: &str, id: &Ident) -> String {
        match self.context {
            Some(m) => m.inverse_map.get(id).map_or_else(
                || format!("{}_{}", default, id.get_id()),
                |x| {
                    format!(
                        "{}_{}",
                        if self.filter {
                            crate::util::sanitize_ident(x)
                        } else {
                            x.to_string()
                        },
                        id.get_id()
                    )
                },
            ),
            None => format!("{}_{}", default, id.get_id()),
        }
    }
    fn get_var_name_by_ident(&mut self, id: &Ident) -> String {
        self.get_name_by_ident("x", id)
    }
    fn get_pred_name_by_ident(&mut self, id: &Ident) -> String {
        self.get_name_by_ident("P", id)
    }
}

pub struct PrettyDisplay<'a, A: Pretty>(&'a A, usize, Option<&'a preprocess::Context>);

impl<'a, A: Pretty> Display for PrettyDisplay<'a, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let al = BoxAllocator;
        let mut config = Config {
            context: self.2,
            filter: false,
        };
        self.0
            .pretty::<_, ()>(&al, &mut config)
            .1
            .render_fmt(self.1, f)?;
        // because of lifetime issue, writing this way is somewhat necessary
        // FIXIME: write it beautifully
        Ok(())
    }
}

pub trait Pretty {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone;

    fn pretty_color<'b, D>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, ColorSpec>
    where
        D: DocAllocator<'b, ColorSpec>,
        D::Doc: Clone,
    {
        self.pretty(al, config)
    }

    fn pretty_display(&self) -> PrettyDisplay<Self>
    where
        Self: Sized,
    {
        self.pretty_display_with_width(get_default_width())
    }

    fn pretty_display_with_width(&self, width: usize) -> PrettyDisplay<Self>
    where
        Self: Sized,
    {
        self.pretty_display_with_width_and_context(width, None)
    }

    fn pretty_display_with_width_and_context<'a>(
        &'a self,
        width: usize,
        ctx: Option<&'a preprocess::Context>,
    ) -> PrettyDisplay<'a, Self>
    where
        Self: Sized,
    {
        PrettyDisplay(self, width, ctx)
    }

    fn pretty_display_with_context<'a>(
        &'a self,
        ctx: &'a preprocess::Context,
    ) -> PrettyDisplay<'a, Self>
    where
        Self: Sized,
    {
        self.pretty_display_with_width_and_context(get_default_width(), Some(ctx))
    }
}

pub trait PrettyColor {}

impl Pretty for str {
    fn pretty<'b, D, A>(&'b self, al: &'b D, _config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        al.text(self)
    }
}

impl Pretty for bool {
    fn pretty<'b, D, A>(&'b self, al: &'b D, _config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        al.text(if *self { "true" } else { "false" })
    }
}

macro_rules! pretty_to_string {
    ($ty:ident) => {
        impl Pretty for $ty {
            fn pretty<'b, D, A>(&'b self, al: &'b D, _config: &mut Config) -> DocBuilder<'b, D, A>
            where
                D: DocAllocator<'b, A>,
                D::Doc: Clone,
                A: Clone,
            {
                al.text(self.to_string())
            }
        }
    };
}

// impls for constants
pretty_to_string! {u64}
pretty_to_string! {usize}
pretty_to_string! {i64}
pretty_to_string! {i32}

pub fn red(c: &mut ColorSpec) -> &mut ColorSpec {
    c.set_fg(Some(Color::Red))
}

pub fn blue(c: &mut ColorSpec) -> &mut ColorSpec {
    c.set_fg(Some(Color::Blue))
}

pub fn white(c: &mut ColorSpec) -> &mut ColorSpec {
    c.set_fg(Some(Color::White))
}

pub fn title(c: &mut ColorSpec) -> &mut ColorSpec {
    c.set_fg(Some(Color::White)).set_bold(true)
}

pub fn bold(c: &mut ColorSpec) -> &mut ColorSpec {
    c.set_bold(true)
}

#[macro_export]
macro_rules! _pdebug {
    ($al:ident, $config:ident, $e:expr $(; $deco:ident)*) => {
        {
            use $crate::util::Pretty;
            #[allow(unused_mut)]
            let mut cs = pretty::termcolor::ColorSpec::new();
            $(
                $crate::util::printer::$deco(&mut cs);
            )*
            $e.pretty_color(&$al, &mut $config).annotate(cs)
        }
    };

    ($al:ident, $config:ident, $e:expr  $(; $deco:ident)* $(,$es:expr $(; $deco2:ident)*)+) => {{
        ({
            use $crate::util::Pretty;
            use pretty::DocAllocator;
            #[allow(unused_mut)]
            let mut cs = pretty::termcolor::ColorSpec::new();
            $(
                $crate::util::printer::$deco(&mut cs);
            )*
            $e.pretty_color(&$al, &mut $config).annotate(cs) + $al.softline()
        })
        +
        $crate::_pdebug! ($al, $config $(, $es $(; $deco2)*)+ ).nest(2)
    }};
}

pub struct PLog<'a>
where
    BoxDoc<'a>: pretty::DocPtr<'a, ColorSpec> + 'a,
{
    pub doc: pretty::BuildDoc<'a, BoxDoc<'a>, ColorSpec>,
}

#[macro_export]
macro_rules! _plog {
    ($al:ident, $config:ident, $($es:expr $(; $deco:ident)* ,)+) => {{
        let doc = $crate::_pdebug!($al, $config $(, $es)+ ).group().1;
        $crate::util::printer::PLog{doc}
    }};
}

#[macro_export]
macro_rules! _print_stderr {
    ($lv:expr, $($es:expr $(; $deco:ident)* ,)+) => {{
        use crate::util::printer::Config;
        use pretty::termcolor::StandardStream;
        use pretty::BoxAllocator;
        use pretty::DocAllocator;
        use pretty::termcolor::ColorChoice;
        let choice = if $crate::util::printer::colored() {
            ColorChoice::Auto
        } else {
            ColorChoice::Never
        };
        if log_enabled!($lv) {
            let al = BoxAllocator;
            let mut config = Config::default();
            crate::_pdebug!(al, config $(, $es $(; $deco)*)+ )
                .group()
                .append(al.hardline())
                .1
                .render_colored($crate::util::printer::get_default_width(), StandardStream::stderr(choice).lock())
                .unwrap();
        }
    }};
}

#[macro_export]
macro_rules! pdebug {
    ($($es:expr $(; $deco:ident)* $(,)?)+) => {{
        $crate::_print_stderr!(log::Level::Debug, $($es $(; $deco)* ,)+)
    }};
}

#[macro_export]
macro_rules! pinfo {
    ($($es:expr $(; $deco:ident)* $(,)?)+) => {{
        $crate::_print_stderr!(log::Level::Info, $($es $(; $deco)* ,)+)
    }};
}

#[macro_export]
macro_rules! pwarn {
    ($($es:expr $(; $deco:ident)* $(,)?)+) => {{
        $crate::_print_stderr!(log::Level::Warn, $($es $(; $deco)* ,)+)
    }};
}

#[test]
fn test_pdebug() {
    pdebug!(1, 2 ; red, 3);
    pdebug!(1, 2 ; red);
    pdebug!(2 ; red );
    pdebug!(1);

    pdebug!(1, 2 ; red ; bold, 3);
    pwarn!(2 ; red );
    pinfo!(2 ; red );

    pinfo!(2 ; red, "\n" );
}

impl Pretty for Ident {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        // todo: making it more human-readable name
        al.text(config.get_var_name_by_ident(self))
    }
}

impl Pretty for PredKind {
    fn pretty<'b, D, A>(&'b self, al: &'b D, _config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        al.text(self.to_str())
    }
}

impl Pretty for OpKind {
    fn pretty<'b, D, A>(&'b self, al: &'b D, _config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        al.text(self.to_str())
    }
}

impl Pretty for QuantifierKind {
    fn pretty<'b, D, A>(&'b self, al: &'b D, _config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        al.text(self.to_str())
    }
}

fn paren<'b, D, A, O>(
    al: &'b D,
    config: &mut Config,
    prec: PrecedenceKind,
    children: &'b O,
) -> DocBuilder<'b, D, A>
where
    D: DocAllocator<'b, A>,
    D::Doc: Clone,
    A: Clone,
    O: Precedence + Pretty,
{
    let child_prec = children.precedence();
    if child_prec == PrecedenceKind::Atom {
        children.pretty(al, config)
    } else if child_prec < prec {
        children.pretty(al, config).parens()
    } else {
        children.pretty(al, config)
    }
}

fn pretty_bin_op<'b, D, A, T>(
    al: &'b D,
    config: &mut Config,
    prec: PrecedenceKind,
    op_str: &'b str,
    left: &'b T,
    right: &'b T,
) -> DocBuilder<'b, D, A>
where
    D: DocAllocator<'b, A>,
    D::Doc: Clone,
    A: Clone,
    T: Precedence + Pretty,
{
    let prec_l = if prec.is_left_assoc() {
        prec
    } else {
        prec.inc()
    };
    let prec_r = if prec.is_right_assoc() {
        prec
    } else {
        prec.inc()
    };
    paren(al, config, prec_l, left) + " " + op_str + " " + paren(al, config, prec_r, right)
}

fn pretty_bin_op_soft<'b, D, A, T>(
    al: &'b D,
    config: &mut Config,
    prec: PrecedenceKind,
    op_str: &'b str,
    left: &'b T,
    right: &'b T,
) -> DocBuilder<'b, D, A>
where
    D: DocAllocator<'b, A>,
    D::Doc: Clone,
    A: Clone,
    T: Precedence + Pretty,
{
    paren(al, config, prec, left)
        + al.line()
        + (al.text(op_str) + al.line() + paren(al, config, prec, right)).hang(2)
}

fn pretty_abs<'b, D, A, T, V>(
    al: &'b D,
    config: &mut Config,
    abs_str: &'b str,
    variable: &'b V,
    content: &'b T,
) -> DocBuilder<'b, D, A>
where
    D: DocAllocator<'b, A>,
    D::Doc: Clone,
    A: Clone,
    T: Pretty,
    V: Pretty,
{
    (al.text(abs_str)
        + variable.pretty(al, config)
        + al.text(".")
        + al.line()
        + content.pretty(al, config))
    .hang(2)
}

impl Pretty for Op {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        use OpExpr::*;
        match self.kind() {
            Op(k, o1, o2) => {
                // handle (0 - x)
                // (1 + 2) + 3 -> 1 + 2 + 3
                match (*k, o1.kind()) {
                    (OpKind::Sub, OpExpr::Const(0)) => {
                        al.text("-")
                            .append(paren(al, config, PrecedenceKind::Atom, o2))
                    }
                    _ => pretty_bin_op(al, config, k.precedence(), k.to_str(), o1, o2),
                }
            }
            Var(i) => i.pretty(al, config),
            Const(c) => al.text(format!("{}", c)),
            ITE(c, x, y) => {
                let c = paren(al, config, PrecedenceKind::If, c);
                let x = paren(al, config, PrecedenceKind::If, x);
                let y = paren(al, config, PrecedenceKind::If, y);
                al.text("if")
                    .append(al.space())
                    .append(c)
                    .append(al.space())
                    .append(al.text("then"))
                    .append(al.space())
                    .append(x)
                    .append(al.space())
                    .append(al.text("else"))
                    .append(al.space())
                    .append(y)
            }
            Ptr(_, o) => o.pretty(al, config),
        }
    }
}

#[test]
fn test_pretty_op() {
    let x = Ident::fresh();
    let y = Ident::fresh();
    let o = Op::mk_mul(Op::mk_add(Op::one(), Op::mk_var(x)), Op::mk_var(y));
    assert_eq!(
        format!("{}", o.pretty_display()),
        format!("(1 + {}) * {}", x, y)
    )
}

impl Pretty for Type {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        match self.kind() {
            TypeKind::Proposition => al.text("b"),
            TypeKind::Integer => al.text("i"),
            TypeKind::Bit => al.text("bit"),
            TypeKind::Arrow(x, y) => {
                let xs = x.pretty(al, config);
                let ys = y.pretty(al, config);
                let xs = if x.order() != 0 { xs.parens() } else { xs };
                xs + " → " + ys
            }
        }
    }
}

impl Pretty for Variable {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        self.id.pretty(al, config) + ": " + self.ty.pretty(al, config)
    }
}

impl Pretty for Constraint {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        use ConstraintExpr::*;
        match self.kind() {
            True => al.text("true"),
            False => al.text("false"),
            Pred(p, ops) => {
                if ops.len() == 2 {
                    return pretty_bin_op(
                        al,
                        config,
                        self.precedence(),
                        p.to_str(),
                        &ops[0],
                        &ops[1],
                    );
                }
                p.pretty(al, config).parens().append(
                    al.intersperse(ops.iter().map(|o| o.pretty(al, config)), ",")
                        .parens(),
                )
            }
            Conj(c1, c2) => pretty_bin_op(al, config, self.precedence(), "∧", c1, c2),
            Disj(c1, c2) => pretty_bin_op(al, config, self.precedence(), "∨", c1, c2),
            Quantifier(q, x, c) => pretty_abs(al, config, q.to_str(), x, c).group(),
        }
    }
}

#[test]
fn test_constraint_printer() {
    // ∀x: i. x >= 0 ∧ (x = 0 ∨ ∀z: i. z = 0)
    let x = Ident::fresh();
    let z = Ident::fresh();
    let c1 = Constraint::mk_geq(Op::mk_var(x), Op::zero());
    let c2 = Constraint::mk_eq(Op::mk_var(x), Op::zero());
    let c3 = Constraint::mk_eq(Op::mk_var(z), Op::zero());
    let c4 = Constraint::mk_quantifier_int(QuantifierKind::Universal, z, c3);
    let c5 = Constraint::mk_conj(c1, Constraint::mk_disj(c2, c4));
    let c6 = Constraint::mk_quantifier_int(QuantifierKind::Universal, x, c5);

    let s1 = c6.pretty_display_with_width(200).to_string();
    let s2 = format!("∀{x}: i. {x} >= 0 ∧ ({x} = 0 ∨ ∀{z}: i. {z} = 0)");
    assert_eq!(s1, s2);
}

impl<C: Pretty + Precedence, T> Pretty for hes::GoalBase<C, T> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        use hes::GoalKind::*;
        match self.kind() {
            Constr(c) => c.pretty(al, config),
            Op(o) => o.pretty(al, config),
            Var(x) => x.pretty(al, config),
            App(x, y) => {
                let x = paren(al, config, self.precedence(), x);
                let y = paren(al, config, PrecedenceKind::Atom, y);
                (x + al.line() + y).hang(0).group()
            }
            Conj(x, y) => pretty_bin_op_soft(al, config, self.precedence(), "∧", x, y),
            Disj(x, y) => pretty_bin_op_soft(al, config, self.precedence(), "∨", x, y),
            Univ(x, y) => pretty_abs(al, config, "∀", x, y),
            Abs(x, y) => pretty_abs(al, config, "λ", x, y),
            ITE(c, g1, g2) => {
                let c = paren(al, config, PrecedenceKind::If, c);
                let g1 = paren(al, config, PrecedenceKind::If, g1);
                let g2 = paren(al, config, PrecedenceKind::If, g2);
                al.text("if")
                    .append(al.space())
                    .append(c)
                    .append(al.space())
                    .append(al.text("then"))
                    .append(al.space())
                    .append(g1)
                    .append(al.space())
                    .append(al.text("else"))
                    .append(al.space())
                    .append(g2)
            }
        }
        .group()
    }
}

impl<C: Pretty + Precedence> Pretty for hes::Clause<C> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        (self.head.pretty(al, config)
            + al.space()
            + "="
            + al.line()
            + self.body.pretty(al, config).nest(4))
        .group()
    }
}

impl<C: Pretty + Precedence> Pretty for hes::Problem<C> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        let toplevel = al.text("toplevel:") + al.line() + self.top.pretty(al, config);

        let docs = self
            .clauses
            .iter()
            .map(|c| al.text("- ") + c.pretty(al, config));
        let body = al.intersperse(docs, al.hardline());
        toplevel + al.hardline() + body
    }
}

fn pretty_predicate<'b, D, A, I, T>(
    al: &'b D,
    config: &mut Config,
    ident: &Ident,
    args: I,
) -> DocBuilder<'b, D, A>
where
    D: DocAllocator<'b, A>,
    D::Doc: Clone,
    A: Clone,
    I: IntoIterator<Item = &'b T>,
    T: Pretty + 'b,
{
    al.text(config.get_pred_name_by_ident(ident)).append(
        al.intersperse(args.into_iter().map(|o| o.pretty(al, config)), ",")
            .parens(),
    )
}

impl Pretty for fofml::Atom {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        use fofml::AtomKind::*;
        match self.kind() {
            True => al.text("true"),
            Constraint(c) => c.pretty(al, config),
            Predicate(p, ops) => pretty_predicate(al, config, p, ops),
            Conj(x, y) => pretty_bin_op(al, config, self.precedence(), "∧", x, y),
            Disj(x, y) => pretty_bin_op(al, config, self.precedence(), "∨", x, y),
            Quantifier(q, x, c) => pretty_abs(al, config, q.to_str(), x, c).group(),
            Not(child) => {
                let c = paren(al, config, self.precedence(), child);

                al.text("¬")
                    .append(if child.precedence() == PrecedenceKind::Atom {
                        al.nil()
                    } else {
                        al.space()
                    })
                    .append(c)
            }
        }
    }
}

impl Pretty for chc::Atom {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        pretty_predicate(al, config, &self.predicate, &self.args)
    }
}

impl<Atom: Pretty, C: Pretty> Pretty for chc::CHCHead<Atom, C> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        match self {
            chc::CHCHead::Constraint(c) => c.pretty(al, config),
            chc::CHCHead::Predicate(a) => a.pretty(al, config),
        }
    }
}

impl<Atom: Pretty, C: Pretty + Top> Pretty for chc::CHCBody<Atom, C> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        let docs = if !self.constraint.is_true() {
            Some(self.constraint.pretty(al, config))
        } else {
            None
        }
        .into_iter()
        .chain(self.predicates.iter().map(|p| p.pretty(al, config)));
        al.intersperse(docs, al.line() + "∧" + al.line())
    }
}

impl<Atom: Pretty, C: Pretty + Top> Pretty for chc::CHC<Atom, C> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        (self.body.pretty(al, config) + al.line() + "->" + al.line() + self.head.pretty(al, config))
            .hang(2)
            .group()
    }
}

impl Pretty for chc::Model {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        let docs = self.model.iter().map(|(key, (args, assign))| {
            (pretty_predicate(al, config, key, args)
                + al.line()
                + "=>"
                + al.line()
                + assign.pretty(al, config))
            .hang(2)
            .group()
        });
        al.intersperse(docs, al.hardline())
    }
}

impl Pretty for pcsp::Atom {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        use pcsp::AtomKind::*;
        match self.kind() {
            True => al.text("true"),
            Constraint(c) => c.pretty(al, config),
            Predicate(p, ops) => pretty_predicate(al, config, p, ops),
            Conj(x, y) => pretty_bin_op(al, config, self.precedence(), "∧", x, y),
            Disj(x, y) => pretty_bin_op(al, config, self.precedence(), "∨", x, y),
            Quantifier(q, x, c) => pretty_abs(al, config, q.to_str(), x, c).group(),
        }
    }
}

impl<Atom: Pretty> Pretty for pcsp::PCSP<Atom> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        (self.body.pretty(al, config) + al.line() + "->" + al.line() + self.head.pretty(al, config))
            .hang(2)
            .group()
    }
}

impl<C: Pretty> Pretty for rtype::Tau<C> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        match self.kind() {
            rtype::TauKind::Proposition(c) => al.text("*[") + c.pretty(al, config) + "]",
            rtype::TauKind::IArrow(i, t) => (i.pretty(al, config)
                + (al.text(":int") + al.line() + al.text("-> ") + t.pretty(al, config)).hang(2))
            .group(),
            rtype::TauKind::PTy(x, t) => (al.text("∀")
                + x.pretty(al, config)
                + "."
                + al.line()
                + t.pretty(al, config).hang(2))
            .group(),
            rtype::TauKind::Arrow(ts, t) => {
                let docs = ts.iter().map(|t| {
                    let tdoc = t.pretty(al, config);
                    if t.order() == 0 {
                        tdoc
                    } else {
                        tdoc.parens()
                    }
                });
                (if docs.len() == 0 {
                    al.text("T")
                } else {
                    al.intersperse(docs, "/\\")
                } + al.nil()
                    + al.line()
                    + (al.text("-> ") + t.pretty(al, config)).hang(2))
                .group()
            }
        }
    }
}
impl<T: Pretty> Pretty for rtype::TypeEnvironment<T> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        let docs = self.map.iter().map(|(id, ts)| {
            let var = id.pretty(al, config);
            let docs = ts.iter().map(|t| t.pretty(al, config));
            let t = al.intersperse(docs, al.hardline().append("/\\ "));
            var.append(al.text(" : ")).append(t.nest(4))
        });
        al.intersperse(docs, al.hardline())
    }
}

fn pretty_tree_inner<'b, D, A, T, F>(
    t: &'b tree::Tree<T>,
    al: &'b D,
    config: &mut Config,
    node_id: tree::ID,
    f: &F,
) -> DocBuilder<'b, D, A>
where
    D: DocAllocator<'b, A>,
    D::Doc: Clone,
    A: Clone,
    T: Pretty,
    F: Fn(&'b D, &mut Config, &'b T) -> DocBuilder<'b, D, A>,
{
    let cur = t.get_node_by_id(node_id);
    let cur_node = f(al, config, &cur.item);
    let mut children = t.get_children(cur).peekable();
    if children.peek().is_none() {
        cur_node
    } else {
        let children = al
            .hardline()
            .append(
                al.intersperse(
                    children
                        .into_iter()
                        .map(|child| pretty_tree_inner(t, al, config, child.id, f)),
                    al.hardline(),
                ),
            )
            .nest(2);
        cur_node.append(children)
    }
}

impl<T: Pretty> Pretty for tree::Tree<T> {
    fn pretty<'b, D, A>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, A>
    where
        D: DocAllocator<'b, A>,
        D::Doc: Clone,
        A: Clone,
    {
        pretty_tree_inner(self, al, config, self.root().id, &|al, config, x| {
            x.pretty(al, config)
        })
    }
    fn pretty_color<'b, D>(&'b self, al: &'b D, config: &mut Config) -> DocBuilder<'b, D, ColorSpec>
    where
        D: DocAllocator<'b, ColorSpec>,
        D::Doc: Clone,
    {
        pretty_tree_inner(self, al, config, self.root().id, &|al, config, x| {
            x.pretty_color(al, config)
        })
    }
}

#[test]
fn test_sub_add_string() {
    let x = Ident::fresh();
    let y = Ident::fresh();
    let o = Op::mk_sub(Op::mk_var(x), Op::mk_add(Op::mk_var(x), Op::mk_var(y)));
    let s = format!("{}", o.pretty_display());
    let xs = format!("{}", x.pretty_display());
    let ys = format!("{}", y.pretty_display());
    println!("{s}");
    println!("{}", format!("{xs} - {xs} + {ys}"));
    assert_eq!(s, format!("{xs} - ({xs} + {ys})"));

    let o2 = Op::mk_add(Op::mk_sub(Op::mk_var(x), Op::mk_var(x)), Op::mk_var(y));
    let s = format!("{}", o2.pretty_display());
    assert_eq!(s, format!("{xs} - {xs} + {ys}"));
}
