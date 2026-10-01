//! **The expression language of quant-format descriptors** (`misaka.palw.quant-format.v1`).
//!
//! A format is DATA: a descriptor says where a block's fields are, which tables it indexes, and, as
//! expressions, how a code, a group's scale, zero point and offset follow from them. This module is
//! the whole of what an expression can say — C-like integer and `f64` arithmetic, ternaries, indexed
//! reads of the block's bytes or of a checkpoint's tensors, table lookups, and a closed set of pure
//! functions (bit fields, sign extension, power of two, the small floating-point formats' decoders).
//! Nothing here depends on a format; adding a format adds a file, not code.
//!
//! **Total and deterministic.** There are no loops and no recursion, so every expression ends; every
//! integer operation is checked (an overflow, a division by zero, a shift out of range is an error,
//! never a wrapped value), every read is bounds-checked, and every floating-point result must be
//! finite. The floating point is IEEE `f64` with `+ − × ÷` only (correctly rounded, the same on every
//! host); there is no transcendental function, so a decoded value does not depend on a platform's
//! libm. A scale an expression builds is exact whenever its operands are (a binary16 times a small
//! integer is).
//!
//! **Lanes.** An expression is evaluated for many elements at once: its variables (`e`, `blk`, `i`,
//! …) are columns of `i64`, every node a loop over the lanes, so the interpretive overhead of a
//! node is paid per batch, not per weight. A ternary evaluates each arm only for the lanes that take
//! it (a mask), so an out-of-range read in the arm not taken is never an error.
//!
//! Grammar, lowest precedence first: `c ? a : b`, `||`, `&&`, `|`, `^`, `&`, `== !=`,
//! `< <= > >=`, `<< >>`, `+ -`, `* / %`, unary `- ~ !`, `name[i, j]`, `f(a, b)`, literals
//! (`12`, `0xFF`, `0.25`, `1e-3`). Integers are `i64`; a result is a float if either operand is.
//! `/` and `%` on two integers truncate toward zero, as C does.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DslError(pub String);

impl std::fmt::Display for DslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type R<T> = std::result::Result<T, DslError>;

fn err<T>(m: impl Into<String>) -> R<T> {
    Err(DslError(m.into()))
}

// ───────────────────────────── columns ─────────────────────────────

/// One value per lane, or one value for all lanes.
#[derive(Clone, Debug, PartialEq)]
pub enum Col {
    I(Vec<i64>),
    F(Vec<f64>),
    CI(i64),
    CF(f64),
}

impl Col {
    fn is_float(&self) -> bool {
        matches!(self, Col::F(_) | Col::CF(_))
    }
    /// The integer of lane `k`.
    pub fn int_at(&self, k: usize) -> R<i64> {
        match self {
            Col::I(v) => Ok(v[k]),
            Col::CI(x) => Ok(*x),
            _ => err("a float where an integer is needed"),
        }
    }
    pub fn float_at(&self, k: usize) -> f64 {
        match self {
            Col::I(v) => v[k] as f64,
            Col::CI(x) => *x as f64,
            Col::F(v) => v[k],
            Col::CF(x) => *x,
        }
    }
    /// The column as `n` integers (an integer column; a float is refused).
    pub fn into_ints(self, n: usize) -> R<Vec<i64>> {
        match self {
            Col::I(v) => Ok(v),
            Col::CI(x) => Ok(vec![x; n]),
            _ => err("a float where an integer is needed"),
        }
    }
    /// The column as `n` floats (an integer widens).
    pub fn into_floats(self, n: usize) -> Vec<f64> {
        match self {
            Col::F(v) => v,
            Col::CF(x) => vec![x; n],
            Col::I(v) => v.into_iter().map(|x| x as f64).collect(),
            Col::CI(x) => vec![x as f64; n],
        }
    }
}

/// Which lanes are live (`None`: all).
#[derive(Clone, Copy)]
pub struct Mask<'a>(pub Option<&'a [bool]>);

impl Mask<'_> {
    #[inline]
    pub fn on(&self, k: usize) -> bool {
        self.0.is_none_or(|m| m[k])
    }
}

// ───────────────────────────── syntax ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Int(i64),
    Float(f64),
    Id(String),
    Op(&'static str),
}

const OPS: &[&str] = &["<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "&", "|", "^", "~", "!", "<", ">", "?", ":", "(", ")", "[", "]", ","];

fn lex(src: &str) -> R<Vec<Tok>> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            if c == '0' && matches!(b.get(i + 1), Some(b'x' | b'X')) {
                i += 2;
                while i < b.len() && (b[i] as char).is_ascii_hexdigit() {
                    i += 1;
                }
                let v = u64::from_str_radix(&src[start + 2..i], 16).map_err(|e| DslError(format!("`{}`: {e}", &src[start..i])))?;
                out.push(Tok::Int(i64::try_from(v).map_err(|_| DslError(format!("`{}` does not fit i64", &src[start..i])))?));
            } else {
                let mut float = false;
                while i < b.len() {
                    let d = b[i] as char;
                    if d.is_ascii_digit() {
                        i += 1;
                    } else if d == '.' {
                        float = true;
                        i += 1;
                    } else if (d == 'e' || d == 'E') && b.get(i + 1).is_some_and(|n| n.is_ascii_digit() || *n == b'-' || *n == b'+') {
                        float = true;
                        i += 2;
                    } else {
                        break;
                    }
                }
                let t = &src[start..i];
                if float {
                    let v: f64 = t.parse().map_err(|_| DslError(format!("`{t}` is not a number")))?;
                    out.push(Tok::Float(v));
                } else {
                    out.push(Tok::Int(t.parse().map_err(|_| DslError(format!("`{t}` does not fit i64")))?));
                }
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Id(src[start..i].to_string()));
        } else {
            match OPS.iter().find(|o| src[i..].starts_with(**o)) {
                Some(o) => {
                    out.push(Tok::Op(o));
                    i += o.len();
                }
                None => return err(format!("unexpected `{c}` in `{src}`")),
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Un {
    Neg,
    Not,
    LNot,
}

/// The closed set of functions an expression may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Float,
    Int,
    Abs,
    Min,
    Max,
    Floor,
    Sext,
    Bits,
    Pow2,
    F16,
    Bf16,
    F32,
    /// Round a value to the nearest binary32, bfloat16, binary16 (ties to even): the dtype a library
    /// computes a dequantised weight in. A value outside the format is an error.
    Rnd32,
    RndBf16,
    RndF16,
    /// `rnd(x, kind)`: round to binary32 (kind 0), bfloat16 (1) or binary16 (2).
    Rnd,
    E8m0,
    Fp8E4m3,
    Fp8E5m2,
}

fn func_of(name: &str) -> Option<(Func, usize)> {
    Some(match name {
        "float" => (Func::Float, 1),
        "int" => (Func::Int, 1),
        "abs" => (Func::Abs, 1),
        "min" => (Func::Min, 2),
        "max" => (Func::Max, 2),
        "floor" => (Func::Floor, 1),
        "sext" => (Func::Sext, 2),
        "bits" => (Func::Bits, 3),
        "pow2" => (Func::Pow2, 1),
        "f16" => (Func::F16, 1),
        "bf16" => (Func::Bf16, 1),
        "f32" => (Func::F32, 1),
        "rnd32" => (Func::Rnd32, 1),
        "rndbf16" => (Func::RndBf16, 1),
        "rndf16" => (Func::RndF16, 1),
        "rnd" => (Func::Rnd, 2),
        "e8m0" => (Func::E8m0, 1),
        "fp8e4m3" => (Func::Fp8E4m3, 1),
        "fp8e5m2" => (Func::Fp8E5m2, 1),
        _ => return None,
    })
}

/// What a name means in the scope an expression is compiled in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Name {
    /// A lane variable (`e`, `blk`, `i`, …): slot index.
    Var(usize),
    /// A value read from the data under evaluation (a block's field, a checkpoint tensor): index
    /// into the scope's reader list, `arity` index arguments (0: a scalar field).
    Read { slot: usize, arity: usize },
    /// A constant table: index.
    Table(usize),
    /// A parameter of the format (`bits`, `group_size`): its value.
    Param(i64),
}

pub trait Scope {
    fn resolve(&self, name: &str) -> Option<Name>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Int(i64),
    Float(f64),
    Var(usize),
    Read(usize, Vec<Node>),
    Table(usize, Box<Node>),
    Un(Un, Box<Node>),
    Bin(Bin, Box<Node>, Box<Node>),
    Cond(Box<Node>, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
}

struct Parser<'s> {
    t: Vec<Tok>,
    p: usize,
    scope: &'s dyn Scope,
    src: &'s str,
    depth: usize,
}

const MAX_DEPTH: usize = 64;

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.p)
    }
    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.p += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, op: &str) -> R<()> {
        if self.eat(op) { Ok(()) } else { err(format!("expected `{op}` in `{}`", self.src)) }
    }
    fn expr(&mut self) -> R<Node> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return err(format!("`{}` is nested deeper than {MAX_DEPTH}", self.src));
        }
        let c = self.bin_level(0)?;
        let r = if self.eat("?") {
            let a = self.expr()?;
            self.expect(":")?;
            let b = self.expr()?;
            Node::Cond(Box::new(c), Box::new(a), Box::new(b))
        } else {
            c
        };
        self.depth -= 1;
        Ok(r)
    }
    /// Binary operator levels, lowest first.
    fn bin_level(&mut self, level: usize) -> R<Node> {
        const LEVELS: &[&[(&str, Option<Bin>)]] = &[
            &[("||", None)],
            &[("&&", None)],
            &[("|", Some(Bin::Or))],
            &[("^", Some(Bin::Xor))],
            &[("&", Some(Bin::And))],
            &[("==", Some(Bin::Eq)), ("!=", Some(Bin::Ne))],
            &[("<=", Some(Bin::Le)), (">=", Some(Bin::Ge)), ("<", Some(Bin::Lt)), (">", Some(Bin::Gt))],
            &[("<<", Some(Bin::Shl)), (">>", Some(Bin::Shr))],
            &[("+", Some(Bin::Add)), ("-", Some(Bin::Sub))],
            &[("*", Some(Bin::Mul)), ("/", Some(Bin::Div)), ("%", Some(Bin::Rem))],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.bin_level(level + 1)?;
        'outer: loop {
            for (op, bin) in LEVELS[level] {
                if self.eat(op) {
                    let rhs = self.bin_level(level + 1)?;
                    lhs = match bin {
                        Some(b) => Node::Bin(*b, Box::new(lhs), Box::new(rhs)),
                        // `a && b` is `a ? (b != 0) : 0`, `a || b` is `a ? 1 : (b != 0)`: the arm not
                        // taken is not evaluated.
                        None if *op == "&&" => Node::Cond(
                            Box::new(lhs),
                            Box::new(Node::Bin(Bin::Ne, Box::new(rhs), Box::new(Node::Int(0)))),
                            Box::new(Node::Int(0)),
                        ),
                        None => Node::Cond(
                            Box::new(lhs),
                            Box::new(Node::Int(1)),
                            Box::new(Node::Bin(Bin::Ne, Box::new(rhs), Box::new(Node::Int(0)))),
                        ),
                    };
                    continue 'outer;
                }
            }
            return Ok(lhs);
        }
    }
    fn unary(&mut self) -> R<Node> {
        if self.eat("-") {
            return Ok(Node::Un(Un::Neg, Box::new(self.unary()?)));
        }
        if self.eat("~") {
            return Ok(Node::Un(Un::Not, Box::new(self.unary()?)));
        }
        if self.eat("!") {
            return Ok(Node::Un(Un::LNot, Box::new(self.unary()?)));
        }
        self.primary()
    }
    fn args(&mut self, close: &str) -> R<Vec<Node>> {
        let mut v = Vec::new();
        if self.eat(close) {
            return Ok(v);
        }
        loop {
            v.push(self.expr()?);
            if self.eat(",") {
                continue;
            }
            self.expect(close)?;
            return Ok(v);
        }
    }
    fn primary(&mut self) -> R<Node> {
        match self.t.get(self.p).cloned() {
            Some(Tok::Int(v)) => {
                self.p += 1;
                Ok(Node::Int(v))
            }
            Some(Tok::Float(v)) => {
                self.p += 1;
                Ok(Node::Float(v))
            }
            Some(Tok::Op("(")) => {
                self.p += 1;
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Id(name)) => {
                self.p += 1;
                if self.eat("(") {
                    let (f, arity) = func_of(&name).ok_or_else(|| DslError(format!("unknown function `{name}` in `{}`", self.src)))?;
                    let a = self.args(")")?;
                    if a.len() != arity {
                        return err(format!("`{name}` takes {arity} argument(s), got {} in `{}`", a.len(), self.src));
                    }
                    return Ok(Node::Call(f, a));
                }
                match self.scope.resolve(&name) {
                    Some(Name::Var(s)) => Ok(Node::Var(s)),
                    Some(Name::Param(v)) => Ok(Node::Int(v)),
                    Some(Name::Read { slot, arity }) => {
                        let idx = if self.eat("[") { self.args("]")? } else { Vec::new() };
                        if idx.len() != arity {
                            return err(format!("`{name}` takes {arity} index(es), got {} in `{}`", idx.len(), self.src));
                        }
                        Ok(Node::Read(slot, idx))
                    }
                    Some(Name::Table(t)) => {
                        self.expect("[")?;
                        let mut a = self.args("]")?;
                        if a.len() != 1 {
                            return err(format!("table `{name}` takes one index in `{}`", self.src));
                        }
                        Ok(Node::Table(t, Box::new(a.remove(0))))
                    }
                    None => err(format!("unknown name `{name}` in `{}`", self.src)),
                }
            }
            other => err(format!("unexpected {other:?} in `{}`", self.src)),
        }
    }
}

/// Compile `src` in `scope`: parse, resolve every name, fold constants.
pub fn compile(src: &str, scope: &dyn Scope) -> R<Node> {
    let t = lex(src)?;
    let mut p = Parser { t, p: 0, scope, src, depth: 0 };
    let n = p.expr()?;
    if p.p != p.t.len() {
        return err(format!("trailing input in `{src}`"));
    }
    fold(n)
}

/// Fold constant sub-expressions (a `pow2(5)`, a `2 * 8`), so a descriptor's arithmetic on literals
/// costs nothing per lane.
fn fold(n: Node) -> R<Node> {
    Ok(match n {
        Node::Un(op, a) => {
            let a = fold(*a)?;
            if let Some(v) = konst(&a) {
                return Ok(from_col(un(op, &v, 1, Mask(None))?));
            }
            Node::Un(op, Box::new(a))
        }
        Node::Bin(op, a, b) => {
            let (a, b) = (fold(*a)?, fold(*b)?);
            if let (Some(x), Some(y)) = (konst(&a), konst(&b)) {
                return Ok(from_col(bin(op, &x, &y, 1, Mask(None))?));
            }
            Node::Bin(op, Box::new(a), Box::new(b))
        }
        Node::Cond(c, a, b) => {
            let c = fold(*c)?;
            // A constant condition takes its arm now; the other is never compiled into the program.
            match c {
                Node::Int(v) => return fold(if v != 0 { *a } else { *b }),
                c => Node::Cond(Box::new(c), Box::new(fold(*a)?), Box::new(fold(*b)?)),
            }
        }
        Node::Call(f, args) => {
            let args = args.into_iter().map(fold).collect::<R<Vec<_>>>()?;
            if args.iter().all(|a| konst(a).is_some()) {
                let cols: Vec<Col> = args.iter().map(|a| konst(a).expect("const")).collect();
                return Ok(from_col(call(f, &cols, 1, Mask(None))?));
            }
            Node::Call(f, args)
        }
        Node::Read(s, idx) => Node::Read(s, idx.into_iter().map(fold).collect::<R<_>>()?),
        Node::Table(t, i) => Node::Table(t, Box::new(fold(*i)?)),
        other => other,
    })
}

fn konst(n: &Node) -> Option<Col> {
    match n {
        Node::Int(v) => Some(Col::CI(*v)),
        Node::Float(v) => Some(Col::CF(*v)),
        _ => None,
    }
}

fn from_col(c: Col) -> Node {
    match c {
        Col::CI(v) => Node::Int(v),
        Col::CF(v) => Node::Float(v),
        _ => unreachable!("a folded constant is a constant"),
    }
}

// ───────────────────────────── evaluation ─────────────────────────────

/// The data an expression reads: the lanes' variables, readers (a block's bytes, a tensor), tables.
pub trait Env {
    /// The number of lanes.
    fn lanes(&self) -> usize;
    /// Variable `slot` as a column.
    fn var(&self, slot: usize) -> &Col;
    /// Read `slot` at the index columns (one per index argument), for the live lanes.
    fn read(&self, slot: usize, index: &[Col], mask: Mask<'_>) -> R<Col>;
    /// Table `t` at `index`'s values (already checked in range by the caller).
    fn table(&self, t: usize) -> &Table;
}

/// A constant table of integers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub values: Vec<i64>,
}

pub fn eval(n: &Node, env: &dyn Env, mask: Mask<'_>) -> R<Col> {
    let lanes = env.lanes();
    Ok(match n {
        Node::Int(v) => Col::CI(*v),
        Node::Float(v) => Col::CF(*v),
        Node::Var(s) => env.var(*s).clone(),
        Node::Read(slot, idx) => {
            let cols = idx.iter().map(|i| eval(i, env, mask)).collect::<R<Vec<_>>>()?;
            env.read(*slot, &cols, mask)?
        }
        Node::Table(t, i) => {
            let tab = env.table(*t);
            let ix = eval(i, env, mask)?;
            match ix {
                Col::CI(k) => Col::CI(*usize::try_from(k).ok().and_then(|k| tab.values.get(k)).ok_or_else(|| DslError(format!("table `{}` has no entry {k}", tab.name)))?),
                Col::I(v) => {
                    let mut out = Vec::with_capacity(lanes);
                    for (k, i) in v.iter().enumerate() {
                        if mask.on(k) {
                            out.push(*usize::try_from(*i).ok().and_then(|i| tab.values.get(i)).ok_or_else(|| {
                                DslError(format!("table `{}` has {} entries, indexed at {i}", tab.name, tab.values.len()))
                            })?);
                        } else {
                            out.push(0);
                        }
                    }
                    Col::I(out)
                }
                _ => return err(format!("table `{}` indexed by a float", tab.name)),
            }
        }
        Node::Un(op, a) => un(*op, &eval(a, env, mask)?, lanes, mask)?,
        Node::Bin(op, a, b) => bin(*op, &eval(a, env, mask)?, &eval(b, env, mask)?, lanes, mask)?,
        Node::Call(f, args) => call(*f, &args.iter().map(|a| eval(a, env, mask)).collect::<R<Vec<_>>>()?, lanes, mask)?,
        Node::Cond(c, a, b) => {
            let cc = eval(c, env, mask)?;
            if let Col::CI(v) = cc {
                return eval(if v != 0 { a } else { b }, env, mask);
            }
            let cv = match cc {
                Col::I(v) => v,
                _ => return err("a condition must be an integer"),
            };
            let live = |k: usize| mask.on(k);
            let ma: Vec<bool> = (0..lanes).map(|k| live(k) && cv[k] != 0).collect();
            let mb: Vec<bool> = (0..lanes).map(|k| live(k) && cv[k] == 0).collect();
            let (x, y) = (eval(a, env, Mask(Some(&ma)))?, eval(b, env, Mask(Some(&mb)))?);
            if x.is_float() || y.is_float() {
                let (xf, yf) = (x.into_floats(lanes), y.into_floats(lanes));
                Col::F((0..lanes).map(|k| if ma[k] { xf[k] } else { yf[k] }).collect())
            } else {
                let (xi, yi) = (x.into_ints(lanes)?, y.into_ints(lanes)?);
                Col::I((0..lanes).map(|k| if ma[k] { xi[k] } else { yi[k] }).collect())
            }
        }
    })
}

fn un(op: Un, a: &Col, n: usize, mask: Mask<'_>) -> R<Col> {
    Ok(match (op, a) {
        (Un::Neg, Col::CI(x)) => Col::CI(x.checked_neg().ok_or_else(|| DslError("negation overflows".into()))?),
        (Un::Neg, Col::CF(x)) => Col::CF(-x),
        (Un::Neg, Col::I(v)) => {
            let mut o = Vec::with_capacity(n);
            for (k, x) in v.iter().enumerate() {
                o.push(if mask.on(k) { x.checked_neg().ok_or_else(|| DslError("negation overflows".into()))? } else { 0 });
            }
            Col::I(o)
        }
        (Un::Neg, Col::F(v)) => Col::F(v.iter().map(|x| -x).collect()),
        (Un::Not, Col::CI(x)) => Col::CI(!x),
        (Un::Not, Col::I(v)) => Col::I(v.iter().map(|x| !x).collect()),
        (Un::LNot, Col::CI(x)) => Col::CI((*x == 0) as i64),
        (Un::LNot, Col::I(v)) => Col::I(v.iter().map(|x| (*x == 0) as i64).collect()),
        _ => return err("`~` and `!` take integers"),
    })
}

fn bin_int(op: Bin, x: i64, y: i64) -> R<i64> {
    let ovf = |what: &str| DslError(format!("integer {what} overflows or is undefined ({x} and {y})"));
    Ok(match op {
        Bin::Add => x.checked_add(y).ok_or_else(|| ovf("addition"))?,
        Bin::Sub => x.checked_sub(y).ok_or_else(|| ovf("subtraction"))?,
        Bin::Mul => x.checked_mul(y).ok_or_else(|| ovf("multiplication"))?,
        Bin::Div => x.checked_div(y).ok_or_else(|| ovf("division"))?,
        Bin::Rem => x.checked_rem(y).ok_or_else(|| ovf("remainder"))?,
        Bin::Shl => {
            if !(0..=62).contains(&y) {
                return err(format!("a left shift by {y}"));
            }
            x.checked_mul(1i64 << y).ok_or_else(|| ovf("left shift"))?
        }
        Bin::Shr => {
            if !(0..=63).contains(&y) {
                return err(format!("a right shift by {y}"));
            }
            x >> y
        }
        Bin::And => x & y,
        Bin::Or => x | y,
        Bin::Xor => x ^ y,
        Bin::Eq => (x == y) as i64,
        Bin::Ne => (x != y) as i64,
        Bin::Lt => (x < y) as i64,
        Bin::Le => (x <= y) as i64,
        Bin::Gt => (x > y) as i64,
        Bin::Ge => (x >= y) as i64,
    })
}

/// Floating-point `op`; bit operations and shifts are integer-only. The result of arithmetic must be
/// finite.
fn bin_float(op: Bin, x: f64, y: f64) -> R<Result<f64, i64>> {
    let fin = |v: f64| if v.is_finite() { Ok(Ok(v)) } else { err("a floating-point result is not finite") };
    match op {
        Bin::Add => fin(x + y),
        Bin::Sub => fin(x - y),
        Bin::Mul => fin(x * y),
        Bin::Div => {
            if y == 0.0 {
                return err("a floating-point division by zero");
            }
            fin(x / y)
        }
        Bin::Eq => Ok(Err((x == y) as i64)),
        Bin::Ne => Ok(Err((x != y) as i64)),
        Bin::Lt => Ok(Err((x < y) as i64)),
        Bin::Le => Ok(Err((x <= y) as i64)),
        Bin::Gt => Ok(Err((x > y) as i64)),
        Bin::Ge => Ok(Err((x >= y) as i64)),
        _ => err("`%`, `<<`, `>>`, `&`, `|` and `^` take integers"),
    }
}

fn bin(op: Bin, a: &Col, b: &Col, n: usize, mask: Mask<'_>) -> R<Col> {
    // Integers.
    if !a.is_float() && !b.is_float() {
        return Ok(match (a, b) {
            (Col::CI(x), Col::CI(y)) => Col::CI(bin_int(op, *x, *y)?),
            _ => {
                let mut out = Vec::with_capacity(n);
                for k in 0..n {
                    out.push(if mask.on(k) { bin_int(op, a.int_at(k)?, b.int_at(k)?)? } else { 0 });
                }
                Col::I(out)
            }
        });
    }
    // Floats (an integer operand widens).
    let is_cmp = matches!(op, Bin::Eq | Bin::Ne | Bin::Lt | Bin::Le | Bin::Gt | Bin::Ge);
    if let (Col::CI(_) | Col::CF(_), Col::CI(_) | Col::CF(_)) = (a, b) {
        return Ok(match bin_float(op, a.float_at(0), b.float_at(0))? {
            Ok(v) => Col::CF(v),
            Err(c) => Col::CI(c),
        });
    }
    if is_cmp {
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            out.push(if mask.on(k) { bin_float(op, a.float_at(k), b.float_at(k))?.unwrap_or_else(|c| c as f64) as i64 } else { 0 });
        }
        return Ok(Col::I(out));
    }
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        out.push(if mask.on(k) { bin_float(op, a.float_at(k), b.float_at(k))?.map_err(|_| DslError("internal: a comparison".into()))? } else { 0.0 });
    }
    Ok(Col::F(out))
}

// The small floating-point formats, decoded exactly to f64.

/// A binary16 bit pattern as `f64`, exactly; NaN and infinity are errors.
pub fn f16_bits_to_f64(h: u16) -> R<f64> {
    let (sign, exp, frac) = ((h >> 15) & 1, ((h >> 10) & 0x1f) as i32, (h & 0x3ff) as f64);
    let m = match exp {
        0 => frac * 2f64.powi(-24),
        31 => return err("a binary16 NaN or infinity"),
        e => (1.0 + frac / 1024.0) * 2f64.powi(e - 15),
    };
    Ok(if sign == 1 { -m } else { m })
}

/// A bfloat16 bit pattern as `f64`, exactly; NaN and infinity are errors.
pub fn bf16_bits_to_f64(h: u16) -> R<f64> {
    let v = f32::from_bits((h as u32) << 16);
    if !v.is_finite() {
        return err("a bfloat16 NaN or infinity");
    }
    Ok(v as f64)
}

/// OCP FP8 E4M3 (`float8_e4m3fn`): bias 7, no infinity, NaN at `S.1111.111`.
pub fn fp8_e4m3_bits_to_f64(b: u8) -> R<f64> {
    let (sign, exp, man) = ((b >> 7) & 1, ((b >> 3) & 0xf) as i32, (b & 7) as f64);
    if exp == 15 && man == 7.0 {
        return err("an FP8 E4M3 NaN");
    }
    let m = if exp == 0 { man / 8.0 * 2f64.powi(-6) } else { (1.0 + man / 8.0) * 2f64.powi(exp - 7) };
    Ok(if sign == 1 { -m } else { m })
}

/// OCP FP8 E5M2: bias 15, infinity and NaN at exponent 31.
pub fn fp8_e5m2_bits_to_f64(b: u8) -> R<f64> {
    let (sign, exp, man) = ((b >> 7) & 1, ((b >> 2) & 0x1f) as i32, (b & 3) as f64);
    if exp == 31 {
        return err("an FP8 E5M2 NaN or infinity");
    }
    let m = if exp == 0 { man / 4.0 * 2f64.powi(-14) } else { (1.0 + man / 4.0) * 2f64.powi(exp - 15) };
    Ok(if sign == 1 { -m } else { m })
}

/// The nearest binary32 (ties to even), as `f64`.
pub fn round_to_f32(x: f64) -> R<f64> {
    let r = x as f32;
    if r.is_finite() { Ok(r as f64) } else { err(format!("{x:e} is outside binary32")) }
}

/// The nearest bfloat16 (ties to even) of the nearest binary32, as `f64` — what `tensor.to(torch.bfloat16)`
/// of a float32 tensor gives.
pub fn round_to_bf16(x: f64) -> R<f64> {
    let f = round_to_f32(x)? as f32;
    let b = f.to_bits();
    let r = f32::from_bits(b.wrapping_add(0x7FFF + ((b >> 16) & 1)) & 0xFFFF_0000);
    if r.is_finite() { Ok(r as f64) } else { err(format!("{x:e} is outside bfloat16")) }
}

/// The binary16 bits of a binary32, nearest, ties to even; `None` outside binary16's range.
pub fn f32_to_f16_bits(f: f32) -> Option<u16> {
    let b = f.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xFF) as i32;
    let man = b & 0x7F_FFFF;
    if exp == 0xFF {
        return None;
    }
    let e = exp - 127 + 15;
    if e >= 31 {
        return None;
    }
    if e <= 0 {
        // Subnormal in binary16 (or zero).
        if e < -10 {
            return Some(sign);
        }
        let m = man | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = 1u32 << (shift - 1);
        let mut r = m >> shift;
        let rem = m & ((1u32 << shift) - 1);
        if rem > half || (rem == half && r & 1 == 1) {
            r += 1;
        }
        return Some(sign | r as u16);
    }
    let mut r = ((e as u32) << 10) | (man >> 13);
    let rem = man & 0x1FFF;
    if rem > 0x1000 || (rem == 0x1000 && r & 1 == 1) {
        r += 1;
    }
    if (r >> 10) >= 31 {
        return None;
    }
    Some(sign | r as u16)
}

/// The nearest binary16 (ties to even) of the nearest binary32, as `f64`.
pub fn round_to_f16(x: f64) -> R<f64> {
    let f = round_to_f32(x)? as f32;
    match f32_to_f16_bits(f) {
        Some(h) => f16_bits_to_f64(h),
        None => err(format!("{x:e} is outside binary16")),
    }
}

fn e8m0_to_f64(b: u8) -> R<f64> {
    if b == 255 {
        return err("an E8M0 NaN");
    }
    Ok(2f64.powi(b as i32 - 127))
}

fn pow2(n: i64) -> R<f64> {
    if !(-1074..=1023).contains(&n) {
        return err(format!("pow2({n}) is outside f64"));
    }
    Ok(if n >= -1022 { f64::from_bits(((n + 1023) as u64) << 52) } else { f64::from_bits(1u64 << (n + 1074)) })
}

fn call(f: Func, args: &[Col], n: usize, mask: Mask<'_>) -> R<Col> {
    // Per-lane scalar functions of one integer argument.
    let unary_int = |conv: &dyn Fn(i64) -> R<f64>| -> R<Col> {
        match &args[0] {
            Col::CI(x) => Ok(Col::CF(conv(*x)?)),
            Col::I(v) => {
                let mut out = Vec::with_capacity(n);
                for (k, x) in v.iter().enumerate() {
                    out.push(if mask.on(k) { conv(*x)? } else { 0.0 });
                }
                Ok(Col::F(out))
            }
            _ => err("this function takes an integer"),
        }
    };
    let bits_of = |x: i64, w: u32| -> R<u64> {
        if x < 0 || (w < 64 && (x as u64) >> w != 0) {
            return err(format!("{x} is not a {w}-bit pattern"));
        }
        Ok(x as u64)
    };
    match f {
        Func::F16 => unary_int(&|x| f16_bits_to_f64(bits_of(x, 16)? as u16)),
        Func::Bf16 => unary_int(&|x| bf16_bits_to_f64(bits_of(x, 16)? as u16)),
        Func::F32 => unary_int(&|x| {
            let v = f32::from_bits(bits_of(x, 32)? as u32);
            if v.is_finite() { Ok(v as f64) } else { err("a binary32 NaN or infinity") }
        }),
        Func::E8m0 => unary_int(&|x| e8m0_to_f64(bits_of(x, 8)? as u8)),
        Func::Rnd => {
            let kind = match &args[1] {
                Col::CI(k) => *k,
                _ => return err("rnd(x, kind): the kind is a constant"),
            };
            let round: fn(f64) -> R<f64> = match kind {
                0 => round_to_f32,
                1 => round_to_bf16,
                2 => round_to_f16,
                other => return err(format!("rnd kind {other} (0: binary32, 1: bfloat16, 2: binary16)")),
            };
            let one = |x: f64| round(x);
            match &args[0] {
                Col::CI(x) => Ok(Col::CF(one(*x as f64)?)),
                Col::CF(x) => Ok(Col::CF(one(*x)?)),
                Col::I(v) => {
                    let mut out = Vec::with_capacity(n);
                    for (k, x) in v.iter().enumerate() {
                        out.push(if mask.on(k) { one(*x as f64)? } else { 0.0 });
                    }
                    Ok(Col::F(out))
                }
                Col::F(v) => {
                    let mut out = Vec::with_capacity(n);
                    for (k, x) in v.iter().enumerate() {
                        out.push(if mask.on(k) { one(*x)? } else { 0.0 });
                    }
                    Ok(Col::F(out))
                }
            }
        }
        Func::Rnd32 | Func::RndBf16 | Func::RndF16 => {
            let round: fn(f64) -> R<f64> = match f {
                Func::Rnd32 => round_to_f32,
                Func::RndBf16 => round_to_bf16,
                _ => round_to_f16,
            };
            match &args[0] {
                Col::CI(x) => Ok(Col::CF(round(*x as f64)?)),
                Col::CF(x) => Ok(Col::CF(round(*x)?)),
                Col::I(v) => {
                    let mut out = Vec::with_capacity(n);
                    for (k, x) in v.iter().enumerate() {
                        out.push(if mask.on(k) { round(*x as f64)? } else { 0.0 });
                    }
                    Ok(Col::F(out))
                }
                Col::F(v) => {
                    let mut out = Vec::with_capacity(n);
                    for (k, x) in v.iter().enumerate() {
                        out.push(if mask.on(k) { round(*x)? } else { 0.0 });
                    }
                    Ok(Col::F(out))
                }
            }
        }
        Func::Fp8E4m3 => unary_int(&|x| fp8_e4m3_bits_to_f64(bits_of(x, 8)? as u8)),
        Func::Fp8E5m2 => unary_int(&|x| fp8_e5m2_bits_to_f64(bits_of(x, 8)? as u8)),
        Func::Pow2 => unary_int(&pow2),
        Func::Float => Ok(match &args[0] {
            Col::CI(x) => Col::CF(*x as f64),
            Col::I(v) => Col::F(v.iter().map(|x| *x as f64).collect()),
            other => other.clone(),
        }),
        Func::Int => {
            let conv = |x: f64| -> R<i64> {
                if x.is_finite() && x.abs() < 9.0e18 { Ok(x.trunc() as i64) } else { err("int() of a value outside i64") }
            };
            Ok(match &args[0] {
                Col::CF(x) => Col::CI(conv(*x)?),
                Col::F(v) => {
                    let mut out = Vec::with_capacity(n);
                    for (k, x) in v.iter().enumerate() {
                        out.push(if mask.on(k) { conv(*x)? } else { 0 });
                    }
                    Col::I(out)
                }
                other => other.clone(),
            })
        }
        Func::Floor => Ok(match &args[0] {
            Col::CF(x) => Col::CF(x.floor()),
            Col::F(v) => Col::F(v.iter().map(|x| x.floor()).collect()),
            other => other.clone(),
        }),
        Func::Abs => Ok(match &args[0] {
            Col::CI(x) => Col::CI(x.checked_abs().ok_or_else(|| DslError("abs overflows".into()))?),
            Col::CF(x) => Col::CF(x.abs()),
            Col::I(v) => {
                let mut out = Vec::with_capacity(n);
                for (k, x) in v.iter().enumerate() {
                    out.push(if mask.on(k) { x.checked_abs().ok_or_else(|| DslError("abs overflows".into()))? } else { 0 });
                }
                Col::I(out)
            }
            Col::F(v) => Col::F(v.iter().map(|x| x.abs()).collect()),
        }),
        Func::Min | Func::Max => {
            let pick = |a: f64, b: f64| if f == Func::Min { a.min(b) } else { a.max(b) };
            if args[0].is_float() || args[1].is_float() {
                let (a, b) = (&args[0], &args[1]);
                Ok(Col::F((0..n).map(|k| pick(a.float_at(k), b.float_at(k))).collect()))
            } else {
                let mut out = Vec::with_capacity(n);
                for k in 0..n {
                    out.push(if mask.on(k) {
                        let (a, b) = (args[0].int_at(k)?, args[1].int_at(k)?);
                        if f == Func::Min { a.min(b) } else { a.max(b) }
                    } else {
                        0
                    });
                }
                Ok(if n == 1 && matches!((&args[0], &args[1]), (Col::CI(_), Col::CI(_))) { Col::CI(out[0]) } else { Col::I(out) })
            }
        }
        Func::Sext => {
            let w = match &args[1] {
                Col::CI(w) if (1..=63).contains(w) => *w as u32,
                _ => return err("sext(x, bits): bits must be a constant in 1..=63"),
            };
            let s = |x: i64| -> R<i64> {
                if x < 0 || (x >> w) != 0 {
                    return err(format!("sext: {x} is not a {w}-bit pattern"));
                }
                Ok((x << (64 - w)) >> (64 - w))
            };
            match &args[0] {
                Col::CI(x) => Ok(Col::CI(s(*x)?)),
                Col::I(v) => {
                    let mut out = Vec::with_capacity(n);
                    for (k, x) in v.iter().enumerate() {
                        out.push(if mask.on(k) { s(*x)? } else { 0 });
                    }
                    Ok(Col::I(out))
                }
                _ => err("sext takes an integer"),
            }
        }
        Func::Bits => {
            let (lo, w) = match (&args[1], &args[2]) {
                (Col::CI(lo), Col::CI(w)) if (0..=62).contains(lo) && (1..=62).contains(w) => (*lo, *w),
                _ => return err("bits(x, lo, width): lo and width must be constants (lo 0..=62, width 1..=62)"),
            };
            let m = (1i64 << w) - 1;
            match &args[0] {
                Col::CI(x) => Ok(Col::CI((x >> lo) & m)),
                Col::I(v) => Ok(Col::I(v.iter().map(|x| (x >> lo) & m).collect())),
                _ => err("bits takes an integer"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestScope;
    impl Scope for TestScope {
        fn resolve(&self, name: &str) -> Option<Name> {
            match name {
                "e" => Some(Name::Var(0)),
                "buf" => Some(Name::Read { slot: 0, arity: 1 }),
                "tab" => Some(Name::Table(0)),
                "bits_" => Some(Name::Param(4)),
                _ => None,
            }
        }
    }

    struct TestEnv {
        vars: Vec<Col>,
        buf: Vec<u8>,
        tabs: Vec<Table>,
        n: usize,
    }
    impl Env for TestEnv {
        fn lanes(&self) -> usize {
            self.n
        }
        fn var(&self, slot: usize) -> &Col {
            &self.vars[slot]
        }
        fn read(&self, _slot: usize, index: &[Col], mask: Mask<'_>) -> R<Col> {
            let mut out = Vec::with_capacity(self.n);
            for k in 0..self.n {
                out.push(if mask.on(k) {
                    let i = index[0].int_at(k)?;
                    *usize::try_from(i).ok().and_then(|i| self.buf.get(i)).ok_or_else(|| DslError(format!("read at {i}")))? as i64
                } else {
                    0
                });
            }
            Ok(Col::I(out))
        }
        fn table(&self, t: usize) -> &Table {
            &self.tabs[t]
        }
    }

    fn run(src: &str, n: usize) -> R<Col> {
        let node = compile(src, &TestScope)?;
        let env = TestEnv {
            vars: vec![Col::I((0..n as i64).collect())],
            buf: (0..16u8).map(|b| b * 3).collect(),
            tabs: vec![Table { name: "tab".into(), values: vec![10, 20, 30, 40] }],
            n,
        };
        eval(&node, &env, Mask(None))
    }

    fn ints(c: Col) -> Vec<i64> {
        match c {
            Col::I(v) => v,
            Col::CI(x) => vec![x],
            other => panic!("not integers: {other:?}"),
        }
    }

    #[test]
    fn arithmetic_precedence_and_c_division() {
        assert_eq!(ints(run("2 + 3 * 4 - 1", 1).unwrap()), vec![13]);
        assert_eq!(ints(run("-7 / 2", 1).unwrap()), vec![-3], "truncation toward zero, as C");
        assert_eq!(ints(run("-7 % 3", 1).unwrap()), vec![-1]);
        assert_eq!(ints(run("1 << 4 | 3 & 2 ^ 1", 1).unwrap()), vec![(1 << 4) | (3 & 2) ^ 1]);
        assert_eq!(ints(run("e * 2 + bits_", 4).unwrap()), vec![4, 6, 8, 10]);
        assert_eq!(ints(run("(e >> 1) & 1", 4).unwrap()), vec![0, 0, 1, 1]);
    }

    #[test]
    fn ternary_takes_each_arm_only_for_its_lanes() {
        // `buf[e + 100]` is out of range for every lane: the arm is never taken.
        assert_eq!(ints(run("e < 8 ? buf[e] : buf[e - 8]", 16).unwrap())[..4], [0, 3, 6, 9]);
        assert_eq!(ints(run("e < 100 ? e : buf[e + 100]", 4).unwrap()), vec![0, 1, 2, 3]);
        assert!(run("e < 100 ? buf[e + 100] : 0", 4).is_err(), "taken, and out of range");
        // Short-circuit: the right side of `&&` is not evaluated where the left is false.
        assert_eq!(ints(run("(e < 2) && (buf[e + 100] > 0)", 4).unwrap_or(Col::CI(-1))), vec![-1]);
        assert_eq!(ints(run("(e >= 100) && (buf[e + 100] > 0)", 4).unwrap()), vec![0, 0, 0, 0]);
        assert_eq!(ints(run("(e < 100) || (buf[e + 100] > 0)", 4).unwrap()), vec![1, 1, 1, 1]);
    }

    #[test]
    fn integer_errors_are_errors_never_wraps() {
        for bad in ["1 / 0", "1 % 0", "1 << 63", "1 << -1", "9223372036854775807 + 1", "tab[7]", "tab[-1]", "buf[99]"] {
            assert!(run(bad, 1).is_err(), "{bad}");
        }
        assert!(compile("1 +", &TestScope).is_err());
        assert!(compile("nope + 1", &TestScope).is_err());
        assert!(compile("f16(1, 2)", &TestScope).is_err());
        assert!(compile("buf + 1", &TestScope).is_err(), "buf takes an index");
        assert_eq!(ints(run("tab[e]", 4).unwrap()), vec![10, 20, 30, 40]);
    }

    #[test]
    fn small_float_formats_decode_exactly() {
        let f = |src: &str| match run(src, 1).unwrap() {
            Col::CF(v) => v,
            Col::F(v) => v[0],
            other => panic!("{other:?}"),
        };
        assert_eq!(f("f16(0x3c00)"), 1.0);
        assert_eq!(f("f16(0xc000)"), -2.0);
        assert_eq!(f("f16(0x7bff)"), 65504.0);
        assert_eq!(f("f16(1)"), 2f64.powi(-24));
        assert_eq!(f("bf16(0x3fc0)"), 1.5);
        assert_eq!(f("e8m0(127)"), 1.0);
        // Rounding to the dtype a library computes in: ties to even, subnormals, overflow is an error.
        assert_eq!(f("rnd32(0.1)"), 0.1f32 as f64);
        assert_eq!(f("rndbf16(1.00390625)"), 1.0, "1 + 2^-8 is the tie between 1 and 1 + 2^-7: to even");
        assert_eq!(f("rndbf16(1.01171875)"), 1.015625, "1 + 3 * 2^-8 ties up to the even 1 + 2^-6");
        assert_eq!(f("rndf16(1.00048828125)"), 1.0, "1 + 2^-11 ties to even");
        assert_eq!(f("rndf16(1.00146484375)"), 1.001953125, "1 + 3 * 2^-11 ties up to the even 1 + 2^-9");
        assert_eq!(f("rndf16(0.000000059604644775390625)"), 2f64.powi(-24), "the smallest binary16 subnormal");
        assert_eq!(f("rndf16(0.00000002980232238769531)"), 0.0, "half of it ties to the even zero");
        assert_eq!(f("rndf16(65504)"), 65504.0);
        assert!(run("rndf16(65520)", 1).is_err() && run("rnd32(1e300)", 1).is_err() && run("rndbf16(1e39)", 1).is_err());
        // `rnd(x, kind)` is the same three roundings with the dtype a constant a descriptor can take from the checkpoint (0: f32, 1: bf16, 2: f16).
        assert_eq!(f("rnd(0.1, 0)"), 0.1f32 as f64);
        assert_eq!(f("rnd(1.00390625, 1)"), 1.0);
        assert_eq!(f("rnd(1.01171875, 1)"), 1.015625);
        assert_eq!(f("rnd(1.00048828125, 2)"), 1.0);
        assert_eq!(f("rnd(-0.0, 1)").to_bits(), (-0.0f64).to_bits(), "a negative zero stays one");
        assert!(run("rnd(1.0, 3)", 1).is_err(), "no such kind");
        assert!(run("rnd(1.0, e)", 1).is_err(), "the kind is a constant, not a lane");
        assert!(run("rnd(65520, 2)", 1).is_err());
        assert_eq!(f("e8m0(0)"), 2f64.powi(-127));
        assert_eq!(f("fp8e4m3(0x38)"), 1.0);
        assert_eq!(f("fp8e4m3(0x7e)"), 448.0);
        assert_eq!(f("fp8e4m3(0x01)"), 2f64.powi(-9));
        assert_eq!(f("fp8e5m2(0x3c)"), 1.0);
        assert_eq!(f("pow2(-3) * 8"), 1.0);
        assert!(run("e8m0(255)", 1).is_err() && run("fp8e4m3(0x7f)", 1).is_err() && run("f16(0x7c00)", 1).is_err());
        assert_eq!(ints(run("sext(0xF, 4)", 1).unwrap()), vec![-1]);
        assert_eq!(ints(run("sext(7, 4)", 1).unwrap()), vec![7]);
        assert_eq!(ints(run("bits(0xABCD, 4, 8)", 1).unwrap()), vec![0xBC]);
        assert_eq!(f("float(7) / 2"), 3.5);
        assert_eq!(ints(run("int(3.9) + int(-3.9)", 1).unwrap()), vec![0]);
    }

    #[test]
    fn floats_must_stay_finite() {
        assert!(run("1.0 / 0.0", 1).is_err());
        assert!(run("1e308 * 10.0", 1).is_err());
        assert_eq!(ints(run("0.5 < 1", 1).unwrap()), vec![1]);
    }

    #[test]
    fn constants_fold_and_depth_is_bounded() {
        let n = compile("pow2(3) * 2 + 1", &TestScope).unwrap();
        assert_eq!(n, Node::Float(17.0));
        let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
        assert!(compile(&deep, &TestScope).is_err());
    }
}
