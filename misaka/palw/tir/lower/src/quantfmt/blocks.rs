//! **The `blocks` interpreter**: a tensor stored as rows of fixed-size blocks (ggml's quantised
//! types), decoded to its stored integers — or floats — by evaluating a descriptor's expressions over
//! batches of blocks (see [`super::desc`] for the schema and [`super::expr`] for the language).
//!
//! Evaluation is per batch of rows: the lanes of the element pass are `rows × columns` (every
//! weight of the batch), the lanes of the group pass `rows × groups`. Each batch is independent, so
//! batches run in parallel and the result is the same in any order and at any thread count.

use super::desc::{CodeRange, DecodeDesc, FieldDesc, LayoutDesc, QuantFormatDesc, SizeDesc, TableDesc, unhex};
use super::expr::{Col, DslError, Env, Mask, Name, Node, R, Scope, Table, compile, eval};
use crate::prequant::QWeight;
use rayon::prelude::*;

mod streamed;
pub use streamed::BlockStreamPlan;

/// Variable slots of the expressions (both passes; see [`BlocksFormat::decode`]).
const VARS: &[&str] = &["e", "j", "blk", "row", "i", "g"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F16,
    Bf16,
    F32,
    F64,
}

impl Ty {
    fn parse(s: &str) -> R<Ty> {
        Ok(match s {
            "u8" => Ty::U8,
            "i8" => Ty::I8,
            "u16" => Ty::U16,
            "i16" => Ty::I16,
            "u32" => Ty::U32,
            "i32" => Ty::I32,
            "u64" => Ty::U64,
            "i64" => Ty::I64,
            "f16" => Ty::F16,
            "bf16" => Ty::Bf16,
            "f32" => Ty::F32,
            "f64" => Ty::F64,
            other => return Err(DslError(format!("a field of type `{other}` (u8 i8 u16 i16 u32 i32 u64 i64 f16 bf16 f32 f64)"))),
        })
    }
    fn size(self) -> usize {
        match self {
            Ty::U8 | Ty::I8 => 1,
            Ty::U16 | Ty::I16 | Ty::F16 | Ty::Bf16 => 2,
            Ty::U32 | Ty::I32 | Ty::F32 => 4,
            Ty::U64 | Ty::I64 | Ty::F64 => 8,
        }
    }
    fn is_float(self) -> bool {
        matches!(self, Ty::F16 | Ty::Bf16 | Ty::F32 | Ty::F64)
    }
}

#[derive(Clone, Debug)]
struct Field {
    name: String,
    at: usize,
    ty: Ty,
    /// `None`: a scalar.
    count: Option<usize>,
}

/// What a descriptor decodes to.
#[derive(Clone, Debug)]
enum Target {
    Integers { q: Node, scale: Node, zero: Option<Node>, min: Option<Node>, code: CodeRange },
    Floats { value: Node },
}

/// A compiled `blocks` format.
#[derive(Clone, Debug)]
pub struct BlocksFormat {
    stream_id: [u8; 32],
    pub name: String,
    pub elems: usize,
    pub bytes: usize,
    fields: Vec<Field>,
    tables: Vec<Table>,
    pub group: usize,
    target: Target,
}

struct BScope<'a> {
    fields: &'a [Field],
    tables: &'a [Table],
}

impl Scope for BScope<'_> {
    fn resolve(&self, name: &str) -> Option<Name> {
        if let Some(i) = VARS.iter().position(|v| *v == name) {
            return Some(Name::Var(i));
        }
        if let Some(i) = self.fields.iter().position(|f| f.name == name) {
            return Some(Name::Read { slot: i, arity: usize::from(self.fields[i].count.is_some()) });
        }
        self.tables.iter().position(|t| t.name == name).map(Name::Table)
    }
}

pub(super) fn table_of(name: &str, t: &TableDesc) -> R<Table> {
    if !matches!(t.bits, 8 | 16 | 32 | 64) {
        return Err(DslError(format!("table `{name}`: {} bits per entry (8, 16, 32, 64)", t.bits)));
    }
    let bytes = unhex(&t.hex)?;
    let w = (t.bits / 8) as usize;
    if bytes.is_empty() || bytes.len() % w != 0 {
        return Err(DslError(format!("table `{name}`: {} bytes is not a whole number of {}-bit entries", bytes.len(), t.bits)));
    }
    let mut values = Vec::with_capacity(bytes.len() / w);
    for c in bytes.chunks_exact(w) {
        let mut b = [0u8; 8];
        b[..w].copy_from_slice(c);
        let u = u64::from_le_bytes(b);
        values.push(if t.signed {
            let sh = 64 - t.bits;
            ((u << sh) as i64) >> sh
        } else {
            i64::try_from(u).map_err(|_| DslError(format!("table `{name}`: an unsigned 64-bit entry beyond i64 (declare it signed)")))?
        });
    }
    Ok(Table { name: name.to_string(), values })
}

impl BlocksFormat {
    pub fn compile(d: &QuantFormatDesc) -> R<BlocksFormat> {
        let LayoutDesc::Blocks { elems, bytes, fields } = &d.layout else {
            return Err(DslError("not a blocks layout".into()));
        };
        if *elems == 0 || *bytes == 0 || *elems > 4096 || *bytes > 65536 {
            return Err(DslError(format!("a block of {elems} elements in {bytes} bytes")));
        }
        let mut fs = Vec::new();
        for FieldDesc { name, at, ty, count } in fields {
            let ty = Ty::parse(ty)?;
            let n = count.unwrap_or(1);
            if n == 0 || n.checked_mul(ty.size()).and_then(|n| at.checked_add(n)).is_none_or(|end| end > *bytes) {
                return Err(DslError(format!("field `{name}` ({n} × {} bytes at {at}) leaves the {bytes}-byte block", ty.size())));
            }
            if VARS.contains(&name.as_str()) || fs.iter().any(|f: &Field| f.name == *name) {
                return Err(DslError(format!("field name `{name}` is taken")));
            }
            fs.push(Field { name: name.clone(), at: *at, ty, count: *count });
        }
        let mut tables = Vec::new();
        for (name, t) in &d.tables {
            if VARS.contains(&name.as_str()) || fs.iter().any(|f| f.name == *name) {
                return Err(DslError(format!("table name `{name}` is taken")));
            }
            tables.push(table_of(name, t)?);
        }
        let DecodeDesc { target, group, q, scale, zero, min, value, code, offset_term, order } = &d.decode;
        if offset_term.is_some() || order.is_some() {
            return Err(DslError("decode.offset_term and decode.order belong to tensors layouts (a blocks format's follow from its codes and its `min`)".into()));
        }
        let group_size = match group.as_ref().map(|g| &g.size) {
            Some(SizeDesc::Fixed(n)) => *n,
            Some(SizeDesc::Expr(_)) => return Err(DslError("a blocks format's decode.group.size is a number".into())),
            // A float format has no scale groups; one group per block keeps the lanes uniform.
            None if target == "floats" => *elems,
            None => return Err(DslError("a blocks format declares decode.group.size".into())),
        };
        if group.as_ref().is_some_and(|g| g.index.is_some()) {
            return Err(DslError("a blocks format's groups are contiguous (no group.index)".into()));
        }
        if group_size == 0 || elems % group_size != 0 {
            return Err(DslError(format!("groups of {group_size} do not tile a block of {elems}")));
        }
        let scope = BScope { fields: &fs, tables: &tables };
        let comp = |what: &str, s: &Option<String>| -> R<Option<Node>> {
            s.as_ref().map(|s| compile(s, &scope).map_err(|e| DslError(format!("decode.{what}: {e}")))).transpose()
        };
        let target = match target.as_str() {
            "integers" => {
                if value.is_some() {
                    return Err(DslError("decode.value belongs to target `floats`".into()));
                }
                let code = code
                    .as_ref()
                    .ok_or_else(|| DslError("target `integers` declares decode.code {min, max}".into()))?
                    .literal()?
                    .check()?;
                Target::Integers {
                    q: comp("q", q)?.ok_or_else(|| DslError("target `integers` declares decode.q".into()))?,
                    scale: comp("scale", scale)?.ok_or_else(|| DslError("target `integers` declares decode.scale".into()))?,
                    zero: comp("zero", zero)?,
                    min: comp("min", min)?,
                    code,
                }
            }
            "floats" => {
                if q.is_some() || scale.is_some() || zero.is_some() || min.is_some() || code.is_some() {
                    return Err(DslError("target `floats` declares decode.value only".into()));
                }
                Target::Floats { value: comp("value", value)?.ok_or_else(|| DslError("target `floats` declares decode.value".into()))? }
            }
            other => return Err(DslError(format!("decode.target `{other}` (integers or floats)"))),
        };
        Ok(BlocksFormat { stream_id: d.digest(), name: d.name.clone(), elems: *elems, bytes: *bytes, fields: fs, tables, group: group_size, target })
    }

    /// Whether the format decodes to stored integers (else floats).
    pub fn is_integers(&self) -> bool {
        matches!(self.target, Target::Integers { .. })
    }

    pub fn has_min(&self) -> bool {
        matches!(&self.target, Target::Integers { min: Some(_), .. })
    }

    /// Whether the lowering must carry a per-group offset term for this format: a float offset
    /// (`min`), or codes whose distance from the zero point does not fit `i8` (a data-dependent zero
    /// is taken to lie in the code range).
    pub fn offset_term(&self) -> bool {
        match &self.target {
            Target::Integers { min, code, zero, .. } => {
                if min.is_some() {
                    return true;
                }
                let (zlo, zhi) = match zero {
                    None => (0, 0),
                    Some(Node::Int(z)) => (*z, *z),
                    Some(_) => (code.min, code.max),
                };
                code.min - zhi < -128 || code.max - zlo > 127
            }
            Target::Floats { .. } => false,
        }
    }

    pub fn code_range(&self) -> Option<CodeRange> {
        match &self.target {
            Target::Integers { code, .. } => Some(*code),
            Target::Floats { .. } => None,
        }
    }

    /// The range of the zero point, when it is a constant (`None`: data-dependent).
    pub fn constant_zero(&self) -> Option<i64> {
        match &self.target {
            Target::Integers { zero: None, .. } => Some(0),
            Target::Integers { zero: Some(Node::Int(z)), .. } => Some(*z),
            _ => None,
        }
    }

    /// Bytes of `rows` rows of `inp` columns.
    pub fn row_bytes(&self, inp: usize) -> R<usize> {
        if !inp.is_multiple_of(self.elems) {
            return Err(DslError(format!("rows of {inp} are not whole {}-element {} blocks", self.elems, self.name)));
        }
        (inp / self.elems).checked_mul(self.bytes).ok_or_else(|| DslError("block row byte count overflow".into()))
    }

    /// Decode `rows` rows of `inp` columns (`raw` is exactly their bytes) to stored integers.
    pub fn decode_integers(&self, raw: &[u8], rows: usize, inp: usize) -> R<QWeight> {
        let Target::Integers { q, scale, zero, min, code } = &self.target else {
            return Err(DslError(format!("{} decodes to floats, not stored integers", self.name)));
        };
        let rb = self.row_bytes(inp)?;
        if raw.len() != rows * rb {
            return Err(DslError(format!("{} bytes for {rows} rows of {inp} {} (needs {})", raw.len(), self.name, rows * rb)));
        }
        let (gs, ng) = (self.group, inp / self.group);
        let step = (65536 / inp.max(1)).max(1);
        let batches: Vec<(usize, usize)> = (0..rows.div_ceil(step)).map(|b| (b * step, ((b + 1) * step).min(rows))).collect();
        type Part = (Vec<i16>, Vec<f64>, Vec<i16>, Option<Vec<f64>>);
        let parts: Vec<R<Part>> = batches
            .par_iter()
            .map(|&(r0, r1)| {
                let nr = r1 - r0;
                // Elements.
                let env = self.env(raw, r0, nr, inp, false);
                let qs = eval(q, &env, Mask(None)).map_err(|e| DslError(format!("{}: decode.q: {e}", self.name)))?.into_ints(env.n)?;
                let mut q16 = Vec::with_capacity(qs.len());
                for v in qs {
                    if v < code.min || v > code.max {
                        return Err(DslError(format!("{}: a code {v} outside the declared {}..={}", self.name, code.min, code.max)));
                    }
                    q16.push(v as i16);
                }
                // Groups.
                let genv = self.env(raw, r0, nr, inp, true);
                let scales = eval(scale, &genv, Mask(None)).map_err(|e| DslError(format!("{}: decode.scale: {e}", self.name)))?.into_floats(genv.n);
                let zeros: Vec<i16> = match zero {
                    None => vec![0; genv.n],
                    Some(z) => eval(z, &genv, Mask(None))
                        .map_err(|e| DslError(format!("{}: decode.zero: {e}", self.name)))?
                        .into_ints(genv.n)?
                        .into_iter()
                        .map(|v| i16::try_from(v).map_err(|_| DslError(format!("{}: a zero point {v} outside i16", self.name))))
                        .collect::<R<_>>()?,
                };
                let mins = match min {
                    None => None,
                    Some(m) => Some(eval(m, &genv, Mask(None)).map_err(|e| DslError(format!("{}: decode.min: {e}", self.name)))?.into_floats(genv.n)),
                };
                debug_assert_eq!(genv.n, nr * ng);
                Ok((q16, scales, zeros, mins))
            })
            .collect();
        let mut q16 = Vec::with_capacity(rows * inp);
        let mut sc = Vec::with_capacity(rows * ng);
        let mut zr = Vec::with_capacity(rows * ng);
        let mut mn = min.as_ref().map(|_| Vec::with_capacity(rows * ng));
        for p in parts {
            let (a, b, c, d) = p?;
            q16.extend(a);
            sc.extend(b);
            zr.extend(c);
            if let (Some(dst), Some(src)) = (mn.as_mut(), d) {
                dst.extend(src);
            }
        }
        let range = code.max - code.min + 1;
        let bits = (64 - ((range - 1).max(1) as u64).leading_zeros()) as u8;
        Ok(QWeight {
            out: rows,
            inp,
            group: gs,
            q: q16,
            scale: sc,
            zero: zr,
            min: mn,
            gidx: (0..inp).map(|i| (i / gs) as u32).collect(),
            bits,
            signed: code.min < 0,
            label: self.name.clone(),
        })
    }

    /// Decode `rows` rows of `inp` columns to `f32` (`W = scale·(q − zero) − min` rounded once, or
    /// the descriptor's `value`).
    pub fn decode_floats(&self, raw: &[u8], rows: usize, inp: usize) -> R<Vec<f32>> {
        match &self.target {
            Target::Integers { .. } => {
                let w = self.decode_integers(raw, rows, inp)?;
                let mut out = Vec::with_capacity(rows * inp);
                for o in 0..rows {
                    for i in 0..inp {
                        out.push(w.value(o, i) as f32);
                    }
                }
                Ok(out)
            }
            Target::Floats { value } => {
                let rb = self.row_bytes(inp)?;
                if raw.len() != rows * rb {
                    return Err(DslError(format!("{} bytes for {rows} rows of {inp} {} (needs {})", raw.len(), self.name, rows * rb)));
                }
                let step = (65536 / inp.max(1)).max(1);
                let parts: Vec<R<Vec<f32>>> = (0..rows.div_ceil(step))
                    .into_par_iter()
                    .map(|b| {
                        let (r0, r1) = (b * step, ((b + 1) * step).min(rows));
                        let env = self.env(raw, r0, r1 - r0, inp, false);
                        let v = eval(value, &env, Mask(None)).map_err(|e| DslError(format!("{}: decode.value: {e}", self.name)))?;
                        Ok(v.into_floats(env.n).into_iter().map(|x| x as f32).collect())
                    })
                    .collect();
                let mut out = Vec::with_capacity(rows * inp);
                for p in parts {
                    out.extend(p?);
                }
                Ok(out)
            }
        }
    }

    /// The lanes of rows `r0..r0 + nr`: every element (`groups = false`) or every group.
    fn env<'a>(&'a self, raw: &'a [u8], r0: usize, nr: usize, inp: usize, groups: bool) -> BEnv<'a> {
        let (bpr, gs) = (inp / self.elems, self.group);
        let (per_row, unit) = if groups { (inp / gs, gs) } else { (inp, 1) };
        let n = nr * per_row;
        let (mut e, mut j, mut blk, mut row, mut i, mut g, mut base) =
            (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for r in 0..nr {
            for k in 0..per_row {
                let col = k * unit; // the first column of the lane's element or group
                let b = col / self.elems;
                let ee = col % self.elems;
                e.push(ee as i64);
                j.push((ee / gs) as i64);
                blk.push(b as i64);
                row.push((r0 + r) as i64);
                i.push(col as i64);
                g.push((col / gs) as i64);
                base.push((((r0 + r) * bpr + b) * self.bytes) as i64);
            }
        }
        BEnv { raw, n, vars: vec![Col::I(e), Col::I(j), Col::I(blk), Col::I(row), Col::I(i), Col::I(g)], base, fmt: self }
    }
}

struct BEnv<'a> {
    raw: &'a [u8],
    n: usize,
    vars: Vec<Col>,
    base: Vec<i64>,
    fmt: &'a BlocksFormat,
}

impl Env for BEnv<'_> {
    fn lanes(&self) -> usize {
        self.n
    }
    fn var(&self, slot: usize) -> &Col {
        &self.vars[slot]
    }
    fn table(&self, t: usize) -> &Table {
        &self.fmt.tables[t]
    }
    fn read(&self, slot: usize, index: &[Col], mask: Mask<'_>) -> R<Col> {
        self.read_with(slot, index, mask, &|range| self.raw.get(range).map(std::borrow::Cow::Borrowed)
            .ok_or_else(|| DslError("field reads past the data".into())))
    }
}
impl<'a> BEnv<'a> {
    fn read_with(&self, slot: usize, index: &[Col], mask: Mask<'_>, read: &dyn Fn(std::ops::Range<usize>) -> R<std::borrow::Cow<'a,[u8]>>) -> R<Col> {
        let f = &self.fmt.fields[slot];
        let size = f.ty.size();
        let count = f.count.unwrap_or(1) as i64;
        let mut ints: Vec<i64> = Vec::new();
        let mut floats: Vec<f64> = Vec::new();
        for k in 0..self.n {
            if !mask.on(k) {
                if f.ty.is_float() { floats.push(0.0) } else { ints.push(0) }
                continue;
            }
            let idx = if index.is_empty() { 0 } else { index[0].int_at(k)? };
            if idx < 0 || idx >= count {
                return Err(DslError(format!("field `{}` has {count} element(s), indexed at {idx}", f.name)));
            }
            let off = (self.base[k] as usize).checked_add(f.at).and_then(|at| (idx as usize).checked_mul(size).and_then(|i| at.checked_add(i)))
                .ok_or_else(|| DslError("block field offset overflows".into()))?;
            let end = off.checked_add(size).ok_or_else(|| DslError("block field offset overflows".into()))?;
            let bytes = read(off..end)?;
            if bytes.len()!=size { return Err(DslError("FRONTEND_BINDING: short block field range".into())); }
            let b = bytes.as_ref();
            match f.ty {
                Ty::U8 => ints.push(b[0] as i64),
                Ty::I8 => ints.push(b[0] as i8 as i64),
                Ty::U16 => ints.push(u16::from_le_bytes([b[0], b[1]]) as i64),
                Ty::I16 => ints.push(i16::from_le_bytes([b[0], b[1]]) as i64),
                Ty::U32 => ints.push(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
                Ty::I32 => ints.push(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
                Ty::U64 => {
                    let v = u64::from_le_bytes(b.try_into().expect("8 bytes"));
                    ints.push(i64::try_from(v).map_err(|_| DslError(format!("field `{}`: a u64 beyond i64 (declare it i64)", f.name)))?);
                }
                Ty::I64 => ints.push(i64::from_le_bytes(b.try_into().expect("8 bytes"))),
                Ty::F16 => floats.push(super::expr::f16_bits_to_f64(u16::from_le_bytes([b[0], b[1]]))?),
                Ty::Bf16 => floats.push(super::expr::bf16_bits_to_f64(u16::from_le_bytes([b[0], b[1]]))?),
                Ty::F32 => {
                    let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    if !v.is_finite() {
                        return Err(DslError(format!("field `{}`: a binary32 NaN or infinity", f.name)));
                    }
                    floats.push(v as f64)
                }
                Ty::F64 => {
                    let v = f64::from_le_bytes(b.try_into().expect("8 bytes"));
                    if !v.is_finite() {
                        return Err(DslError(format!("field `{}`: a binary64 NaN or infinity", f.name)));
                    }
                    floats.push(v)
                }
            }
        }
        Ok(if f.ty.is_float() { Col::F(floats) } else { Col::I(ints) })
    }
}
