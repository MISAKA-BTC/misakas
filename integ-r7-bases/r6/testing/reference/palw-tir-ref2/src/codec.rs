//! The canonical byte encoding (04b §4), written by hand from §4.1–4.3 — no derive.

use crate::error::{Class, Res, TirError, err};
use crate::program::*;
use crate::types::{DType, Dim, TensorType};

/// §4.4: the size cap of an encoded program.
pub const MAX_PROGRAM_BYTES: usize = 262_144;

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

fn enc_err(what: impl Into<String>) -> TirError {
    TirError::new(Class::Encoding, what)
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.b.len() - self.at
    }

    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        if n > self.remaining() {
            return Err(enc_err(format!("truncated: need {n} bytes at offset {}", self.at)));
        }
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }

    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }

    /// Little-endian, assembled byte by byte (§4.1).
    fn le(&mut self, n: usize) -> Res<u64> {
        let s = self.take(n)?;
        let mut v: u64 = 0;
        for (i, &byte) in s.iter().enumerate() {
            v |= (byte as u64) << (8 * i);
        }
        Ok(v)
    }

    fn u16(&mut self) -> Res<u16> {
        Ok(self.le(2)? as u16)
    }

    fn u32(&mut self) -> Res<u32> {
        Ok(self.le(4)? as u32)
    }

    fn i64(&mut self) -> Res<i64> {
        Ok(self.le(8)? as i64)
    }

    fn bool(&mut self) -> Res<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(enc_err(format!("bool byte {b:#04x}"))),
        }
    }

    fn arr64(&mut self) -> Res<[u8; 64]> {
        let s = self.take(64)?;
        let mut a = [0u8; 64];
        a.copy_from_slice(s);
        Ok(a)
    }

    /// A sequence count. Every element of every sequence of §4.2 encodes to at least one byte, so a
    /// count above the remaining bytes can never decode; refusing it early only bounds allocation.
    fn count(&mut self) -> Res<usize> {
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return Err(enc_err(format!("sequence of {n} elements with {} bytes left", self.remaining())));
        }
        Ok(n)
    }

    fn seq<T>(&mut self, mut f: impl FnMut(&mut Self) -> Res<T>) -> Res<Vec<T>> {
        let n = self.count()?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(f(self)?);
        }
        Ok(v)
    }

    fn string(&mut self) -> Res<String> {
        let n = self.u32()? as usize;
        let s = self.take(n)?;
        String::from_utf8(s.to_vec()).map_err(|_| enc_err("string is not UTF-8"))
    }

    fn dtype(&mut self) -> Res<DType> {
        let t = self.u8()?;
        DType::from_tag(t).ok_or_else(|| enc_err(format!("unknown dtype tag {t}")))
    }

    fn dim(&mut self) -> Res<Dim> {
        match self.u8()? {
            0 => Ok(Dim::Fixed(self.u32()?)),
            1 => Ok(Dim::H),
            t => Err(enc_err(format!("unknown Dim tag {t}"))),
        }
    }

    fn tensor_type(&mut self) -> Res<TensorType> {
        let dtype = self.dtype()?;
        let shape = self.seq(|r| r.dim())?;
        Ok(TensorType { dtype, shape })
    }

    fn reference(&mut self) -> Res<Ref> {
        match self.u8()? {
            0 => Ok(Ref::Node(self.u16()?)),
            1 => Ok(Ref::CarryIn(self.u8()?)),
            2 => Ok(Ref::Param(self.u16()?)),
            3 => Ok(Ref::Const(self.u16()?)),
            4 => Ok(Ref::State(self.u16()?)),
            5 => Ok(Ref::Input(self.u8()?)),
            t => Err(enc_err(format!("unknown Ref tag {t}"))),
        }
    }

    fn rounding(&mut self) -> Res<Rounding> {
        match self.u8()? {
            0 => Ok(Rounding::Floor),
            1 => Ok(Rounding::HalfUp),
            2 => Ok(Rounding::HalfAwayFromZero),
            t => Err(enc_err(format!("unknown Rounding tag {t}"))),
        }
    }

    fn cmp(&mut self) -> Res<Cmp> {
        match self.u8()? {
            0 => Ok(Cmp::Eq),
            1 => Ok(Cmp::Ne),
            2 => Ok(Cmp::Lt),
            3 => Ok(Cmp::Le),
            4 => Ok(Cmp::Gt),
            5 => Ok(Cmp::Ge),
            t => Err(enc_err(format!("unknown Cmp tag {t}"))),
        }
    }

    fn prim(&mut self) -> Res<Prim> {
        let tag = self.u8()?;
        Ok(match tag {
            0 => Prim::Reshape,
            1 => Prim::Transpose { perm: self.seq(|r| r.u8())? },
            2 => Prim::Slice { axis: self.u8()?, start: self.u32()? },
            3 => Prim::Concat { axis: self.u8()? },
            4 => Prim::Broadcast,
            5 => Prim::Iota { axis: self.u8()?, start: self.i64()?, step: self.i64()? },
            6 => Prim::Gather { axis: self.u8()?, batch_dims: self.u8()? },
            7 => Prim::Cast,
            8 => Prim::Add,
            9 => Prim::Sub,
            10 => Prim::Mul,
            11 => Prim::MatMul,
            12 => Prim::ReduceSum { axis: self.u8()? },
            13 => Prim::ReduceMax { axis: self.u8()? },
            14 => Prim::Div { rule: self.rounding()? },
            15 => Prim::Clamp { lo: self.i64()?, hi: self.i64()? },
            16 => Prim::Log2Floor,
            17 => Prim::IntExp,
            18 => Prim::IntRsqrt,
            19 => Prim::IntLn,
            20 => Prim::Compare { cmp: self.cmp()? },
            21 => Prim::Select,
            22 => Prim::TopK { axis: self.u8()?, k: self.u32()? },
            23 => Prim::StateWrite { state: self.u16()? },
            24 => Prim::HistAppend { state: self.u16()? },
            t => return Err(enc_err(format!("unknown Prim tag {t}"))),
        })
    }

    fn node(&mut self) -> Res<Node> {
        let prim = self.prim()?;
        let inputs = self.seq(|r| r.reference())?;
        let out = self.tensor_type()?;
        let commit = self.bool()?;
        Ok(Node { prim, inputs, out, commit })
    }

    fn block(&mut self) -> Res<Block> {
        let name = self.string()?;
        let carry_in = self.seq(|r| r.tensor_type())?;
        let nodes = self.seq(|r| r.node())?;
        let carry_out = self.seq(|r| r.u16())?;
        Ok(Block { name, carry_in, nodes, carry_out })
    }

    fn param(&mut self) -> Res<ParamDecl> {
        let name = self.string()?;
        let dtype = self.dtype()?;
        let shape = self.seq(|r| r.u32())?;
        let per_layer = self.bool()?;
        Ok(ParamDecl { name, dtype, shape, per_layer })
    }

    fn konst(&mut self) -> Res<ConstDecl> {
        let dtype = self.dtype()?;
        let shape = self.seq(|r| r.u32())?;
        let data = self.seq(|r| r.u8())?;
        Ok(ConstDecl { dtype, shape, data })
    }

    fn state(&mut self) -> Res<StateDecl> {
        let name = self.string()?;
        let kind = match self.u8()? {
            0 => StateKind::Fixed { lo: self.i64()?, hi: self.i64()? },
            1 => StateKind::Hist { window: self.u32()? },
            t => return Err(enc_err(format!("unknown StateKind tag {t}"))),
        };
        let dtype = self.dtype()?;
        let shape = self.seq(|r| r.u32())?;
        let per_layer = self.bool()?;
        Ok(StateDecl { name, kind, dtype, shape, per_layer })
    }

    fn program(&mut self) -> Res<Program> {
        let version = self.u16()?;
        let prim_set_id = self.arr64()?;
        let token_bound = self.u32()?;
        let history_bound = self.u32()?;
        let params = self.seq(|r| r.param())?;
        let consts = self.seq(|r| r.konst())?;
        let states = self.seq(|r| r.state())?;
        let blocks = self.seq(|r| r.block())?;
        let pre = self.u8()?;
        let layers = self.seq(|r| r.u8())?;
        let post = self.u8()?;
        let logits = self.u16()?;
        let logits_scheme_id = self.arr64()?;
        Ok(Program {
            version,
            prim_set_id,
            token_bound,
            history_bound,
            params,
            consts,
            states,
            blocks,
            schedule: Schedule { pre, layers, post },
            logits,
            logits_scheme_id,
        })
    }
}

/// Strict structural decoding only (§4.1–4.3): every byte consumed, every tag known, every `bool`
/// 0 or 1, every `String` UTF-8. No normal-form check.
pub fn decode_structural(bytes: &[u8]) -> Res<Program> {
    if bytes.len() > MAX_PROGRAM_BYTES {
        return err(Class::Encoding, format!("{} bytes, above {}", bytes.len(), MAX_PROGRAM_BYTES));
    }
    let mut r = Reader { b: bytes, at: 0 };
    let p = r.program()?;
    if r.remaining() != 0 {
        return err(Class::Encoding, format!("{} trailing bytes", r.remaining()));
    }
    Ok(p)
}

/// `decode_canonical` of §4.4: size cap, strict decoding, re-encoding identity, normal form (§5).
pub fn decode_canonical(bytes: &[u8]) -> Res<Program> {
    let p = decode_structural(bytes)?;
    if encode(&p) != bytes {
        return err(Class::Encoding, "re-encoding does not reproduce the bytes");
    }
    crate::normal_form::check(&p)?;
    Ok(p)
}

/// The classes of every rule a byte string breaks (§9.3): `{Encoding}` when §4.4's decoding fails
/// (normal form is then not evaluable), else the classes of every violated NF rule; `Ok` when the
/// bytes are a program in normal form.
pub fn decode_violations(bytes: &[u8]) -> Result<Program, std::collections::BTreeSet<Class>> {
    let enc = || std::collections::BTreeSet::from([Class::Encoding]);
    let p = decode_structural(bytes).map_err(|_| enc())?;
    if encode(&p) != bytes {
        return Err(enc());
    }
    let v = crate::normal_form::violations(&p);
    if v.is_empty() { Ok(p) } else { Err(v.into_iter().map(|x| x.class).collect()) }
}

struct Writer {
    v: Vec<u8>,
}

impl Writer {
    fn le(&mut self, x: u64, n: usize) {
        for i in 0..n {
            self.v.push((x >> (8 * i)) as u8);
        }
    }
    fn u8(&mut self, x: u8) {
        self.v.push(x);
    }
    fn u16(&mut self, x: u16) {
        self.le(x as u64, 2);
    }
    fn u32(&mut self, x: u32) {
        self.le(x as u64, 4);
    }
    fn i64(&mut self, x: i64) {
        self.le(x as u64, 8);
    }
    fn bool(&mut self, x: bool) {
        self.v.push(if x { 1 } else { 0 });
    }
    fn count(&mut self, n: usize) {
        self.u32(n as u32);
    }
    fn string(&mut self, s: &str) {
        self.count(s.len());
        self.v.extend_from_slice(s.as_bytes());
    }
    fn dtype(&mut self, d: DType) {
        self.u8(d.tag());
    }
    fn u32s(&mut self, xs: &[u32]) {
        self.count(xs.len());
        for &x in xs {
            self.u32(x);
        }
    }
    fn tensor_type(&mut self, t: &TensorType) {
        self.dtype(t.dtype);
        self.count(t.shape.len());
        for d in &t.shape {
            match d {
                Dim::Fixed(n) => {
                    self.u8(0);
                    self.u32(*n);
                }
                Dim::H => self.u8(1),
            }
        }
    }
    fn reference(&mut self, r: &Ref) {
        match *r {
            Ref::Node(i) => {
                self.u8(0);
                self.u16(i)
            }
            Ref::CarryIn(k) => {
                self.u8(1);
                self.u8(k)
            }
            Ref::Param(j) => {
                self.u8(2);
                self.u16(j)
            }
            Ref::Const(j) => {
                self.u8(3);
                self.u16(j)
            }
            Ref::State(j) => {
                self.u8(4);
                self.u16(j)
            }
            Ref::Input(j) => {
                self.u8(5);
                self.u8(j)
            }
        }
    }
    fn prim(&mut self, p: &Prim) {
        self.u8(p.tag());
        match p {
            Prim::Transpose { perm } => {
                self.count(perm.len());
                self.v.extend_from_slice(perm);
            }
            Prim::Slice { axis, start } => {
                self.u8(*axis);
                self.u32(*start);
            }
            Prim::Concat { axis } | Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => self.u8(*axis),
            Prim::Iota { axis, start, step } => {
                self.u8(*axis);
                self.i64(*start);
                self.i64(*step);
            }
            Prim::Gather { axis, batch_dims } => {
                self.u8(*axis);
                self.u8(*batch_dims);
            }
            Prim::Div { rule } => self.u8(match rule {
                Rounding::Floor => 0,
                Rounding::HalfUp => 1,
                Rounding::HalfAwayFromZero => 2,
            }),
            Prim::Clamp { lo, hi } => {
                self.i64(*lo);
                self.i64(*hi);
            }
            Prim::Compare { cmp } => self.u8(match cmp {
                Cmp::Eq => 0,
                Cmp::Ne => 1,
                Cmp::Lt => 2,
                Cmp::Le => 3,
                Cmp::Gt => 4,
                Cmp::Ge => 5,
            }),
            Prim::TopK { axis, k } => {
                self.u8(*axis);
                self.u32(*k);
            }
            Prim::StateWrite { state } | Prim::HistAppend { state } => self.u16(*state),
            Prim::Reshape
            | Prim::Broadcast
            | Prim::Cast
            | Prim::Add
            | Prim::Sub
            | Prim::Mul
            | Prim::MatMul
            | Prim::Log2Floor
            | Prim::IntExp
            | Prim::IntRsqrt
            | Prim::IntLn
            | Prim::Select => {}
        }
    }
}

/// The canonical encoding of a `Prim` alone (the `borsh_hex` of the primitive vectors).
pub fn encode_prim(p: &Prim) -> Vec<u8> {
    let mut w = Writer { v: Vec::new() };
    w.prim(p);
    w.v
}

/// Decode a lone `Prim` (strict, no trailing byte).
pub fn decode_prim(bytes: &[u8]) -> Res<Prim> {
    let mut r = Reader { b: bytes, at: 0 };
    let p = r.prim()?;
    if r.remaining() != 0 {
        return err(Class::Encoding, "trailing bytes after Prim");
    }
    Ok(p)
}

/// The canonical encoding of a program (§4.2).
pub fn encode(p: &Program) -> Vec<u8> {
    let mut w = Writer { v: Vec::new() };
    w.u16(p.version);
    w.v.extend_from_slice(&p.prim_set_id);
    w.u32(p.token_bound);
    w.u32(p.history_bound);
    w.count(p.params.len());
    for d in &p.params {
        w.string(&d.name);
        w.dtype(d.dtype);
        w.u32s(&d.shape);
        w.bool(d.per_layer);
    }
    w.count(p.consts.len());
    for c in &p.consts {
        w.dtype(c.dtype);
        w.u32s(&c.shape);
        w.count(c.data.len());
        w.v.extend_from_slice(&c.data);
    }
    w.count(p.states.len());
    for s in &p.states {
        w.string(&s.name);
        match s.kind {
            StateKind::Fixed { lo, hi } => {
                w.u8(0);
                w.i64(lo);
                w.i64(hi);
            }
            StateKind::Hist { window } => {
                w.u8(1);
                w.u32(window);
            }
        }
        w.dtype(s.dtype);
        w.u32s(&s.shape);
        w.bool(s.per_layer);
    }
    w.count(p.blocks.len());
    for b in &p.blocks {
        w.string(&b.name);
        w.count(b.carry_in.len());
        for t in &b.carry_in {
            w.tensor_type(t);
        }
        w.count(b.nodes.len());
        for n in &b.nodes {
            w.prim(&n.prim);
            w.count(n.inputs.len());
            for r in &n.inputs {
                w.reference(r);
            }
            w.tensor_type(&n.out);
            w.bool(n.commit);
        }
        w.count(b.carry_out.len());
        for &c in &b.carry_out {
            w.u16(c);
        }
    }
    w.u8(p.schedule.pre);
    w.count(p.schedule.layers.len());
    w.v.extend_from_slice(&p.schedule.layers);
    w.u8(p.schedule.post);
    w.u16(p.logits);
    w.v.extend_from_slice(&p.logits_scheme_id);
    w.v
}
