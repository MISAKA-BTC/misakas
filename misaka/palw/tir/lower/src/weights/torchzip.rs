//! **A PyTorch checkpoint (`pytorch_model.bin`, `adapter_model.bin`, `.pt`, `.pth`) read without running anything.**
//!
//! `torch.save` writes a ZIP archive of uncompressed members: `<prefix>/data.pkl` (a pickle of the state dict whose tensors are
//! `torch._utils._rebuild_tensor_v2(storage, offset, size, stride, …)` over persistent storage ids), and one raw member
//! `<prefix>/data/<key>` per storage. A pickle is a program for a stack machine, and `torch.load` runs it: a hostile file executes
//! code on load. **This reader never runs it and never imports anything.** It is
//!
//! * a ZIP reader (end-of-central-directory, ZIP64, the central directory, the local headers of the members it uses), with
//!   explicit bounds on every count and length it believes;
//! * a *symbolic* pickle interpreter: the opcodes torch writes, a stack of inert values, and **an explicit allowlist of globals**
//!   — `collections.OrderedDict`, `torch._utils._rebuild_tensor_v2` (and `_rebuild_tensor`, `_rebuild_parameter`), and the storage
//!   types `torch.{Float,Double,Half,BFloat16,Long,Int,Short,Char,Byte,Bool}Storage`. `REDUCE` is evaluated by this module for exactly
//!   those callables and refused for everything else; `GLOBAL` of anything else is refused when it is read, before it can be called;
//!   `BUILD` accepts only the state dict's `_metadata` (a mapping of strings, dropped); every other opcode that constructs an object
//!   (`INST`, `OBJ`, `NEWOBJ`, `EXT*`, `PERSID`, buffers, sets) is refused;
//! * bounded: opcodes, stack, memo, container nesting, bytes allocated, string length, tensors, rank and every length prefix (a prefix
//!   longer than the bytes left is refused before anything is allocated). A container becomes immutable the moment it is a member of
//!   another, so a pickle cannot build a cycle or a nesting deeper than the bound.
//!
//! Anything it refuses is a [`LowerError`] whose message starts with `FORMAT_UNSUPPORTED`: the preflight reports it as that code, and
//! no file is half-read. Only contiguous tensors (row-major strides, any storage offset; tied weights share a storage) are served;
//! a strided view is refused by name. Tensor bytes are read by `pread` at the offsets this module computes; nothing else is read.

use super::read_exact_at;
use crate::error::{LowerError, Result};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

/// The prefix of every refusal: the census code the preflight reports it under.
pub const FORMAT_UNSUPPORTED: &str = "FORMAT_UNSUPPORTED";

fn refuse(msg: impl std::fmt::Display) -> LowerError {
    LowerError::not_lowerable(format!("{FORMAT_UNSUPPORTED}: PyTorch checkpoint: {msg}"))
}

// ───────────────────────────────────────── bounds ─────────────────────────────────────────

/// Hard bounds on everything the reader believes.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The pickle's size (`data.pkl`).
    pub max_pickle_bytes: u64,
    /// The central directory's size and the number of members.
    pub max_directory_bytes: u64,
    pub max_members: usize,
    /// Opcodes executed.
    pub max_ops: u64,
    /// Values on the stack.
    pub max_stack: usize,
    /// Memo entries (and the highest memo index).
    pub max_memo: usize,
    /// Container nesting.
    pub max_depth: u32,
    /// Bytes the values allocate (strings, containers, nodes).
    pub max_alloc_bytes: u64,
    /// One string.
    pub max_string: usize,
    pub max_tensors: usize,
    pub max_rank: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_pickle_bytes: 256 << 20,
            max_directory_bytes: 256 << 20,
            max_members: 1 << 22,
            max_ops: 1 << 27,
            max_stack: 1 << 22,
            max_memo: 1 << 22,
            max_depth: 32,
            max_alloc_bytes: 512 << 20,
            max_string: 1 << 20,
            max_tensors: 1 << 20,
            max_rank: 8,
        }
    }
}

// ───────────────────────────────────────── the result ─────────────────────────────────────────

/// One tensor of the checkpoint: where its data is, in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TorchEntry {
    /// A safetensors dtype name (`F32`, `F16`, `BF16`, `F64`, `I64`, `I32`, `I16`, `I8`, `U8`, `BOOL`).
    pub dtype: &'static str,
    pub shape: Vec<usize>,
    /// The absolute offset of the tensor's first byte in the file.
    pub begin: u64,
    pub bytes: u64,
}

/// What a checkpoint's headers say, read without its tensor data.
#[derive(Clone, Debug)]
pub struct TorchHeader {
    pub entries: BTreeMap<String, TorchEntry>,
    pub file_len: u64,
    /// Bytes read from the file (the directory, the pickle, the local headers): what a preflight reports as read.
    pub bytes_read: u64,
    /// One past the last byte any tensor needs.
    pub data_end: u64,
    /// BLAKE2b-256 (hex) of `data.pkl`: the pickle's identity.
    pub pickle_digest: String,
}

// ───────────────────────────────────────── the zip ─────────────────────────────────────────

fn le16(b: &[u8], at: usize) -> u64 {
    u16::from_le_bytes([b[at], b[at + 1]]) as u64
}
fn le32(b: &[u8], at: usize) -> u64 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as u64
}
fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3], b[at + 4], b[at + 5], b[at + 6], b[at + 7]])
}

#[derive(Clone, Debug)]
struct Member {
    name: String,
    method: u64,
    flags: u64,
    size: u64,
    compressed: u64,
    local: u64,
}

struct Zip<'a> {
    file: &'a std::fs::File,
    len: u64,
    read: u64,
    members: BTreeMap<String, Member>,
}

impl<'a> Zip<'a> {
    fn pread(&mut self, at: u64, n: u64) -> Result<Vec<u8>> {
        if at.checked_add(n).is_none_or(|e| e > self.len) {
            return Err(refuse(format!("a read of {n} bytes at {at} runs past the end of the file ({} bytes)", self.len)));
        }
        let mut buf = vec![0u8; n as usize];
        read_exact_at(self.file, &mut buf, at).map_err(|e| LowerError::Io(e.to_string()))?;
        self.read += n;
        Ok(buf)
    }

    fn open(file: &'a std::fs::File, lim: &Limits) -> Result<Zip<'a>> {
        let len = file.metadata().map_err(|e| LowerError::Io(e.to_string()))?.len();
        let mut z = Zip { file, len, read: 0, members: BTreeMap::new() };
        if len < 22 {
            return Err(refuse("not a ZIP archive (a file of fewer than 22 bytes)"));
        }
        // The magic: a torch zip starts with a local file header; the legacy (pre-1.6) format is a pickle of a magic number.
        let head = z.pread(0, 4)?;
        if head != b"PK\x03\x04" {
            return Err(refuse(if head[0] == 0x80 {
                "the legacy (pre-1.6) serialization — a sequence of pickles, not a zip archive — is not read; re-save it with torch.save, or use safetensors"
            } else {
                "not a ZIP archive (no local file header at the start)"
            }));
        }
        // End of central directory: the last 22 + comment bytes.
        let tail_len = len.min(22 + 65535);
        let tail = z.pread(len - tail_len, tail_len)?;
        let mut eocd = None;
        for i in (0..=tail.len() - 22).rev() {
            if &tail[i..i + 4] == b"PK\x05\x06" && i + 22 + le16(&tail, i + 20) as usize == tail.len() {
                eocd = Some(i);
                break;
            }
        }
        let i = eocd.ok_or_else(|| refuse("no end-of-central-directory record"))?;
        let (mut entries, mut cd_size, mut cd_off) = (le16(&tail, i + 10), le32(&tail, i + 12), le32(&tail, i + 16));
        if le16(&tail, i + 4) != 0 || le16(&tail, i + 6) != 0 {
            return Err(refuse("a multi-disk archive"));
        }
        if entries == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
            // ZIP64: the locator just before the record, then the record it names.
            if i < 20 || &tail[i - 20..i - 16] != b"PK\x06\x07" {
                return Err(refuse("a ZIP64 archive without its locator"));
            }
            let off64 = le64(&tail, i - 20 + 8);
            let r = z.pread(off64, 56)?;
            if &r[0..4] != b"PK\x06\x06" {
                return Err(refuse("a ZIP64 locator that names no ZIP64 end record"));
            }
            entries = le64(&r, 32);
            cd_size = le64(&r, 40);
            cd_off = le64(&r, 48);
        }
        if cd_size > lim.max_directory_bytes || entries as usize > lim.max_members || entries > cd_size / 46 + 1 {
            return Err(refuse(format!("a central directory of {entries} members in {cd_size} bytes is past the bounds")));
        }
        if cd_off.checked_add(cd_size).is_none_or(|e| e > len) {
            return Err(refuse("the central directory runs past the end of the file"));
        }
        let cd = z.pread(cd_off, cd_size)?;
        let mut p = 0usize;
        for _ in 0..entries {
            if p + 46 > cd.len() || &cd[p..p + 4] != b"PK\x01\x02" {
                return Err(refuse("a damaged central directory"));
            }
            let (flags, method) = (le16(&cd, p + 8), le16(&cd, p + 10));
            let (mut comp, mut size, mut local) = (le32(&cd, p + 20), le32(&cd, p + 24), le32(&cd, p + 42));
            let (nl, el, cl) = (le16(&cd, p + 28) as usize, le16(&cd, p + 30) as usize, le16(&cd, p + 32) as usize);
            let end = p + 46 + nl + el + cl;
            if end > cd.len() {
                return Err(refuse("a central directory entry runs past the directory"));
            }
            let name = String::from_utf8(cd[p + 46..p + 46 + nl].to_vec()).map_err(|_| refuse("a member name that is not UTF-8"))?;
            // The ZIP64 extra field carries, in order, the fields whose 32-bit value is 0xFFFFFFFF.
            let mut x = p + 46 + nl;
            let xe = x + el;
            while x + 4 <= xe {
                let (id, sz) = (le16(&cd, x), le16(&cd, x + 2) as usize);
                if x + 4 + sz > xe {
                    return Err(refuse("a damaged extra field"));
                }
                if id == 1 {
                    let mut f = x + 4;
                    for v in [&mut size, &mut comp, &mut local] {
                        if *v == 0xFFFF_FFFF {
                            if f + 8 > x + 4 + sz {
                                return Err(refuse("a ZIP64 extra field too short for its entry"));
                            }
                            *v = le64(&cd, f);
                            f += 8;
                        }
                    }
                }
                x += 4 + sz;
            }
            if z.members.insert(name.clone(), Member { name, method, flags, size, compressed: comp, local }).is_some() {
                return Err(refuse("two members of one name"));
            }
            p = end;
        }
        Ok(z)
    }

    /// The absolute offset of a stored member's data, after checking its local header against the directory.
    fn data_start(&mut self, m: &Member) -> Result<u64> {
        if m.method != 0 {
            return Err(refuse(format!("member `{}` is compressed (method {}): a torch checkpoint stores its members", m.name, m.method)));
        }
        if m.flags & 1 != 0 {
            return Err(refuse(format!("member `{}` is encrypted", m.name)));
        }
        if m.size != m.compressed {
            return Err(refuse(format!("member `{}` is stored with two sizes", m.name)));
        }
        let h = self.pread(m.local, 30)?;
        if &h[0..4] != b"PK\x03\x04" {
            return Err(refuse(format!("member `{}` has no local header at the offset the directory gives", m.name)));
        }
        let (nl, el) = (le16(&h, 26), le16(&h, 28));
        if nl != m.name.len() as u64 {
            return Err(refuse(format!("member `{}`: the local header and the directory disagree on the name", m.name)));
        }
        let name = self.pread(m.local + 30, nl)?;
        if name != m.name.as_bytes() {
            return Err(refuse(format!("member `{}`: the local header and the directory disagree on the name", m.name)));
        }
        let start = m.local + 30 + nl + el;
        if start.checked_add(m.size).is_none_or(|e| e > self.len) {
            return Err(refuse(format!("member `{}` runs past the end of the file", m.name)));
        }
        Ok(start)
    }
}

// ───────────────────────────────────────── values ─────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glob {
    OrderedDict,
    RebuildTensorV2,
    RebuildTensorV1,
    RebuildParameter,
    Storage(&'static str, usize),
}

fn glob_of(module: &str, name: &str) -> Option<Glob> {
    Some(match (module, name) {
        ("collections", "OrderedDict") => Glob::OrderedDict,
        ("torch._utils", "_rebuild_tensor_v2") => Glob::RebuildTensorV2,
        ("torch._utils", "_rebuild_tensor") => Glob::RebuildTensorV1,
        ("torch._utils", "_rebuild_parameter") => Glob::RebuildParameter,
        ("torch", "FloatStorage") => Glob::Storage("F32", 4),
        ("torch", "DoubleStorage") => Glob::Storage("F64", 8),
        ("torch", "HalfStorage") => Glob::Storage("F16", 2),
        ("torch", "BFloat16Storage") => Glob::Storage("BF16", 2),
        ("torch", "LongStorage") => Glob::Storage("I64", 8),
        ("torch", "IntStorage") => Glob::Storage("I32", 4),
        ("torch", "ShortStorage") => Glob::Storage("I16", 2),
        ("torch", "CharStorage") => Glob::Storage("I8", 1),
        ("torch", "ByteStorage") => Glob::Storage("U8", 1),
        ("torch", "BoolStorage") => Glob::Storage("BOOL", 1),
        _ => return None,
    })
}

#[derive(Clone, Debug)]
struct StorageRef {
    dtype: &'static str,
    elem: usize,
    key: String,
    numel: u64,
}

#[derive(Clone, Debug)]
struct TensorRef {
    storage: Rc<StorageRef>,
    offset: u64,
    shape: Vec<u64>,
    stride: Vec<u64>,
}

#[allow(dead_code)] // payloads kept for the allowlist's checks and for debugging
enum Kind {
    None,
    Bool(bool),
    Int(i64),
    Float,
    Str(String),
    Bytes,
    Tuple(Vec<Val>),
    List(Vec<Val>),
    Dict(Vec<(Val, Val)>),
    Global(Glob),
    Storage(Rc<StorageRef>),
    Tensor(Rc<TensorRef>),
}

/// A value of the symbolic machine. `sealed`: it is a member of another value and no longer changes; `depth`: its container nesting.
struct Node {
    kind: RefCell<Kind>,
    depth: Cell<u32>,
    sealed: Cell<bool>,
}
type Val = Rc<Node>;

enum Item {
    Mark,
    V(Val),
}

struct Machine<'a> {
    data: &'a [u8],
    at: usize,
    lim: Limits,
    ops: u64,
    stack: Vec<Item>,
    memo: Vec<Option<Val>>,
    memo_count: usize,
    alloc: u64,
    storages: BTreeMap<String, Rc<StorageRef>>,
    tensors: usize,
}

fn describe(k: &Kind) -> &'static str {
    match k {
        Kind::None => "None",
        Kind::Bool(_) => "a bool",
        Kind::Int(_) => "an int",
        Kind::Float => "a float",
        Kind::Str(_) => "a string",
        Kind::Bytes => "bytes",
        Kind::Tuple(_) => "a tuple",
        Kind::List(_) => "a list",
        Kind::Dict(_) => "a dict",
        Kind::Global(_) => "a global",
        Kind::Storage(_) => "a storage",
        Kind::Tensor(_) => "a tensor",
    }
}

impl<'a> Machine<'a> {
    fn new(data: &'a [u8], lim: Limits) -> Machine<'a> {
        Machine { data, at: 0, lim, ops: 0, stack: Vec::new(), memo: Vec::new(), memo_count: 0, alloc: 0, storages: BTreeMap::new(), tensors: 0 }
    }

    fn charge(&mut self, bytes: u64) -> Result<()> {
        self.alloc = self.alloc.saturating_add(bytes);
        if self.alloc > self.lim.max_alloc_bytes {
            return Err(refuse(format!("the pickle allocates more than {} bytes of values", self.lim.max_alloc_bytes)));
        }
        Ok(())
    }

    fn node(&mut self, kind: Kind, depth: u32) -> Result<Val> {
        self.charge(64)?;
        Ok(Rc::new(Node { kind: RefCell::new(kind), depth: Cell::new(depth), sealed: Cell::new(false) }))
    }

    fn push(&mut self, it: Item) -> Result<()> {
        if self.stack.len() >= self.lim.max_stack {
            return Err(refuse(format!("the pickle's stack is deeper than {}", self.lim.max_stack)));
        }
        self.stack.push(it);
        Ok(())
    }

    fn push_val(&mut self, kind: Kind) -> Result<()> {
        let v = self.node(kind, 0)?;
        self.push(Item::V(v))
    }

    fn pop(&mut self) -> Result<Val> {
        match self.stack.pop() {
            Some(Item::V(v)) => Ok(v),
            Some(Item::Mark) => Err(refuse("an opcode reached a mark where it needs a value")),
            None => Err(refuse("an opcode on an empty stack")),
        }
    }

    /// Everything above the topmost mark, in order; the mark is removed.
    fn pop_mark(&mut self) -> Result<Vec<Val>> {
        let mut out = Vec::new();
        loop {
            match self.stack.pop() {
                Some(Item::V(v)) => out.push(v),
                Some(Item::Mark) => break,
                None => return Err(refuse("an opcode that needs a mark found none")),
            }
        }
        out.reverse();
        Ok(out)
    }

    // ---- reading the stream ----
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.data.len() - self.at {
            return Err(refuse(format!("the pickle is cut short or a length prefix ({n}) is longer than the bytes left ({})", self.data.len() - self.at)));
        }
        let s = &self.data[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
    }
    fn line(&mut self) -> Result<&'a str> {
        let rest = &self.data[self.at..];
        let n = rest.iter().take(4097).position(|b| *b == b'\n').ok_or_else(|| refuse("a text opcode without its newline"))?;
        let s = std::str::from_utf8(&rest[..n]).map_err(|_| refuse("a text opcode that is not UTF-8"))?;
        self.at += n + 1;
        Ok(s)
    }
    fn string_of(&mut self, n: usize) -> Result<String> {
        if n > self.lim.max_string {
            return Err(refuse(format!("a string of {n} bytes is past the bound {}", self.lim.max_string)));
        }
        let b = self.take(n)?;
        self.charge(n as u64)?;
        String::from_utf8(b.to_vec()).map_err(|_| refuse("a string that is not UTF-8"))
    }

    // ---- values ----
    fn seal(&self, v: &Val) {
        v.sealed.set(true);
    }

    fn container_depth(&self, items: &[Val]) -> Result<u32> {
        let d = items.iter().map(|v| v.depth.get() + 1).max().unwrap_or(1);
        if d > self.lim.max_depth {
            return Err(refuse(format!("containers nested deeper than {}", self.lim.max_depth)));
        }
        Ok(d)
    }

    fn memo_put(&mut self, idx: usize) -> Result<()> {
        if idx >= self.lim.max_memo {
            return Err(refuse(format!("memo index {idx} is past the bound {}", self.lim.max_memo)));
        }
        let top = match self.stack.last() {
            Some(Item::V(v)) => v.clone(),
            _ => return Err(refuse("a memo put with no value on the stack")),
        };
        if self.memo.len() <= idx {
            self.charge(8 * (idx + 1 - self.memo.len()) as u64)?;
            self.memo.resize(idx + 1, None);
        }
        if self.memo[idx].is_none() {
            self.memo_count += 1;
            if self.memo_count > self.lim.max_memo {
                return Err(refuse("the memo is full"));
            }
        }
        self.memo[idx] = Some(top);
        Ok(())
    }

    fn memo_get(&mut self, idx: usize) -> Result<()> {
        let v = self.memo.get(idx).and_then(|v| v.clone()).ok_or_else(|| refuse(format!("a memo get of index {idx}, which was never put")))?;
        self.push(Item::V(v))
    }

    fn int_of(v: &Val) -> Result<i64> {
        match &*v.kind.borrow() {
            Kind::Int(i) => Ok(*i),
            other => Err(refuse(format!("an int was expected, found {}", describe(other)))),
        }
    }
    fn ints_of(v: &Val, max_rank: usize) -> Result<Vec<u64>> {
        match &*v.kind.borrow() {
            Kind::Tuple(t) | Kind::List(t) => {
                if t.len() > max_rank {
                    return Err(refuse(format!("a tensor of rank {} (the bound is {max_rank})", t.len())));
                }
                t.iter()
                    .map(|x| {
                        let i = Self::int_of(x)?;
                        u64::try_from(i).map_err(|_| refuse(format!("a negative extent or stride {i}")))
                    })
                    .collect()
            }
            other => Err(refuse(format!("a tuple of ints was expected, found {}", describe(other)))),
        }
    }

    fn global(&mut self, module: &str, name: &str) -> Result<()> {
        match glob_of(module, name) {
            Some(g) => self.push_val(Kind::Global(g)),
            None => Err(refuse(format!("the pickle names `{module}.{name}`, which is not on the allowlist — it would be called when the file is loaded, and is not"))),
        }
    }

    fn reduce(&mut self) -> Result<()> {
        let args = self.pop()?;
        let callee = self.pop()?;
        let g = match &*callee.kind.borrow() {
            Kind::Global(g) => *g,
            other => return Err(refuse(format!("REDUCE on {}, which is not an allowlisted callable", describe(other)))),
        };
        let items: Vec<Val> = match &*args.kind.borrow() {
            Kind::Tuple(t) => t.clone(),
            other => return Err(refuse(format!("REDUCE with {} as its arguments", describe(other)))),
        };
        match g {
            Glob::OrderedDict => {
                if !items.is_empty() {
                    return Err(refuse("collections.OrderedDict called with arguments"));
                }
                let d = self.node(Kind::Dict(Vec::new()), 0)?;
                self.push(Item::V(d))
            }
            Glob::RebuildParameter => {
                // (data, requires_grad, backward_hooks): the parameter is its data.
                if items.len() != 3 {
                    return Err(refuse("torch._utils._rebuild_parameter with a form it does not take"));
                }
                if !matches!(&*items[0].kind.borrow(), Kind::Tensor(_)) {
                    return Err(refuse("torch._utils._rebuild_parameter over something that is not a tensor"));
                }
                self.empty_hooks(&items[2])?;
                self.push(Item::V(items[0].clone()))
            }
            Glob::RebuildTensorV2 | Glob::RebuildTensorV1 => {
                let n = items.len();
                let ok = if g == Glob::RebuildTensorV2 { n == 6 || n == 7 } else { n == 4 };
                if !ok {
                    return Err(refuse(format!("torch._utils._rebuild_tensor with {n} arguments")));
                }
                let storage = match &*items[0].kind.borrow() {
                    Kind::Storage(s) => s.clone(),
                    other => return Err(refuse(format!("a tensor over {}, not a persistent storage", describe(other)))),
                };
                let offset = u64::try_from(Self::int_of(&items[1])?).map_err(|_| refuse("a negative storage offset"))?;
                let shape = Self::ints_of(&items[2], self.lim.max_rank)?;
                let stride = Self::ints_of(&items[3], self.lim.max_rank)?;
                if shape.len() != stride.len() {
                    return Err(refuse("a tensor whose size and stride differ in rank"));
                }
                if g == Glob::RebuildTensorV2 {
                    // requires_grad, backward_hooks[, metadata]: no hooks, no callables.
                    if !matches!(&*items[4].kind.borrow(), Kind::Bool(_)) {
                        return Err(refuse("a tensor whose requires_grad is not a bool"));
                    }
                    self.empty_hooks(&items[5])?;
                    if n == 7 && !matches!(&*items[6].kind.borrow(), Kind::None | Kind::Dict(_)) {
                        return Err(refuse("a tensor whose metadata is neither None nor a dict"));
                    }
                }
                self.tensors += 1;
                if self.tensors > self.lim.max_tensors {
                    return Err(refuse(format!("more than {} tensors", self.lim.max_tensors)));
                }
                for it in &items {
                    self.seal(it);
                }
                let t = self.node(Kind::Tensor(Rc::new(TensorRef { storage, offset, shape, stride })), 0)?;
                self.push(Item::V(t))
            }
            Glob::Storage(..) => Err(refuse("REDUCE on a storage type")),
        }
    }

    fn empty_hooks(&self, v: &Val) -> Result<()> {
        match &*v.kind.borrow() {
            Kind::Dict(d) if d.is_empty() => Ok(()),
            _ => Err(refuse("backward hooks are not empty — a hook is a callable, and none is run")),
        }
    }

    fn persistent_load(&mut self) -> Result<()> {
        let pid = self.pop()?;
        let t: Vec<Val> = match &*pid.kind.borrow() {
            Kind::Tuple(t) => t.clone(),
            other => return Err(refuse(format!("a persistent id that is {}, not a tuple", describe(other)))),
        };
        if t.len() != 5 {
            return Err(refuse("a persistent id that is not (storage, type, key, location, numel)"));
        }
        match &*t[0].kind.borrow() {
            Kind::Str(s) if s == "storage" => {}
            _ => return Err(refuse("a persistent id of a kind other than `storage`")),
        }
        let (dtype, elem) = match &*t[1].kind.borrow() {
            Kind::Global(Glob::Storage(d, e)) => (*d, *e),
            _ => return Err(refuse("a persistent storage whose type is not an allowlisted storage class")),
        };
        let key = match &*t[2].kind.borrow() {
            Kind::Str(s) => s.clone(),
            _ => return Err(refuse("a persistent storage whose key is not a string")),
        };
        if !matches!(&*t[3].kind.borrow(), Kind::Str(_) | Kind::None) {
            return Err(refuse("a persistent storage whose location is not a string"));
        }
        let numel = u64::try_from(Self::int_of(&t[4])?).map_err(|_| refuse("a negative storage size"))?;
        let s = match self.storages.get(&key) {
            Some(s) => {
                if s.dtype != dtype || s.numel != numel {
                    return Err(refuse(format!("storage `{key}` is declared twice with different types or sizes")));
                }
                s.clone()
            }
            None => {
                let s = Rc::new(StorageRef { dtype, elem, key: key.clone(), numel });
                self.storages.insert(key, s.clone());
                s
            }
        };
        let v = self.node(Kind::Storage(s), 0)?;
        self.push(Item::V(v))
    }

    fn build(&mut self) -> Result<()> {
        // The state dict's `_metadata` (a mapping of module names to {version}) is applied to the OrderedDict below it: dropped.
        let state = self.pop()?;
        let target_ok = match self.stack.last() {
            Some(Item::V(v)) => matches!(&*v.kind.borrow(), Kind::Dict(_)),
            _ => false,
        };
        let state_ok = match &*state.kind.borrow() {
            Kind::Dict(d) => d.iter().all(|(k, _)| matches!(&*k.kind.borrow(), Kind::Str(s) if s == "_metadata")),
            _ => false,
        };
        if !(target_ok && state_ok) {
            return Err(refuse("BUILD of anything but a state dict's `_metadata` — it would set attributes or call __setstate__"));
        }
        Ok(())
    }

    fn dict_insert(&mut self, d: &Val, items: Vec<Val>) -> Result<()> {
        if d.sealed.get() {
            return Err(refuse("a dict that is a member of another value is changed"));
        }
        if items.len() % 2 != 0 {
            return Err(refuse("SETITEMS with an odd number of values"));
        }
        if items.iter().any(|v| Rc::ptr_eq(v, d)) {
            return Err(refuse("a dict that is made a member of itself"));
        }
        let mut pairs = Vec::with_capacity(items.len() / 2);
        let mut it = items.into_iter();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            if !matches!(&*k.kind.borrow(), Kind::Str(_) | Kind::Int(_)) {
                return Err(refuse("a dict key that is not a string or an int"));
            }
            self.seal(&k);
            self.seal(&v);
            pairs.push((k, v));
        }
        self.charge(32 * pairs.len() as u64)?;
        let depth = pairs.iter().map(|(_, v)| v.depth.get() + 1).max().unwrap_or(1);
        if depth > self.lim.max_depth {
            return Err(refuse(format!("containers nested deeper than {}", self.lim.max_depth)));
        }
        match &mut *d.kind.borrow_mut() {
            Kind::Dict(existing) => existing.extend(pairs),
            other => return Err(refuse(format!("SETITEM on {}", describe(other)))),
        }
        d.depth.set(d.depth.get().max(depth));
        Ok(())
    }

    fn list_extend(&mut self, l: &Val, items: Vec<Val>) -> Result<()> {
        if l.sealed.get() {
            return Err(refuse("a list that is a member of another value is changed"));
        }
        if items.iter().any(|v| Rc::ptr_eq(v, l)) {
            return Err(refuse("a list that is made a member of itself"));
        }
        for v in &items {
            self.seal(v);
        }
        self.charge(8 * items.len() as u64)?;
        let depth = self.container_depth(&items)?;
        match &mut *l.kind.borrow_mut() {
            Kind::List(existing) => existing.extend(items),
            other => return Err(refuse(format!("APPEND on {}", describe(other)))),
        }
        l.depth.set(l.depth.get().max(depth));
        Ok(())
    }

    fn make_tuple(&mut self, items: Vec<Val>) -> Result<()> {
        for v in &items {
            self.seal(v);
        }
        self.charge(8 * items.len() as u64)?;
        let depth = self.container_depth(&items)?;
        let t = self.node(Kind::Tuple(items), depth)?;
        self.push(Item::V(t))
    }

    fn top_val(&self) -> Result<Val> {
        match self.stack.last() {
            Some(Item::V(v)) => Ok(v.clone()),
            _ => Err(refuse("an opcode that needs a value on the stack")),
        }
    }

    /// Run the pickle to its STOP; the value it returns.
    fn run(&mut self) -> Result<Val> {
        loop {
            self.ops += 1;
            if self.ops > self.lim.max_ops {
                return Err(refuse(format!("the pickle runs more than {} opcodes", self.lim.max_ops)));
            }
            let op = self.u8()?;
            match op {
                // protocol, frames
                0x80 => {
                    let v = self.u8()?;
                    if v > 5 {
                        return Err(refuse(format!("pickle protocol {v}")));
                    }
                }
                0x95 => {
                    self.take(8)?; // the frame's length: advisory
                }
                b'.' => {
                    let v = self.pop()?;
                    if !self.stack.is_empty() {
                        return Err(refuse("values left on the stack at STOP"));
                    }
                    return Ok(v);
                }
                // globals
                b'c' => {
                    let m = self.line()?.to_string();
                    let n = self.line()?.to_string();
                    self.global(&m, &n)?;
                }
                0x93 => {
                    let n = self.pop()?;
                    let m = self.pop()?;
                    let (m, n) = match (&*m.kind.borrow(), &*n.kind.borrow()) {
                        (Kind::Str(m), Kind::Str(n)) => (m.clone(), n.clone()),
                        _ => return Err(refuse("STACK_GLOBAL over values that are not strings")),
                    };
                    self.global(&m, &n)?;
                }
                // marks and stack
                b'(' => self.push(Item::Mark)?,
                b'0' => {
                    self.pop()?;
                }
                b'1' => {
                    self.pop_mark()?;
                }
                b'2' => {
                    let v = self.top_val()?;
                    self.push(Item::V(v))?;
                }
                // atoms
                b'N' => self.push_val(Kind::None)?,
                0x88 => self.push_val(Kind::Bool(true))?,
                0x89 => self.push_val(Kind::Bool(false))?,
                b'K' => {
                    let v = self.u8()? as i64;
                    self.push_val(Kind::Int(v))?;
                }
                b'M' => {
                    let v = self.u16()? as i64;
                    self.push_val(Kind::Int(v))?;
                }
                b'J' => {
                    let v = self.u32()? as i32 as i64;
                    self.push_val(Kind::Int(v))?;
                }
                0x8a => {
                    let n = self.u8()? as usize;
                    if n > 8 {
                        return Err(refuse("an integer wider than 64 bits"));
                    }
                    let b = self.take(n)?;
                    let mut v: i64 = 0;
                    for (i, byte) in b.iter().enumerate() {
                        v |= (*byte as i64) << (8 * i);
                    }
                    if n > 0 && n < 8 && b[n - 1] & 0x80 != 0 {
                        v |= -1i64 << (8 * n);
                    }
                    self.push_val(Kind::Int(v))?;
                }
                0x8b => return Err(refuse("LONG4: an integer of an unbounded width")),
                // The protocol-0 text forms (I, L, F, S, V, P, p, g) are not what torch writes (protocol 2 and up): refused.
                b'I' | b'L' | b'F' | b'S' | b'V' | b'p' | b'g' => {
                    return Err(refuse("a protocol-0 text opcode — torch.save writes protocol 2 or later"));
                }
                b'G' => {
                    self.take(8)?;
                    self.push_val(Kind::Float)?;
                }
                // strings and bytes
                b'X' => {
                    let n = self.u32()? as usize;
                    let s = self.string_of(n)?;
                    self.push_val(Kind::Str(s))?;
                }
                0x8c => {
                    let n = self.u8()? as usize;
                    let s = self.string_of(n)?;
                    self.push_val(Kind::Str(s))?;
                }
                0x8d => {
                    let n = self.u64()?;
                    let s = self.string_of(usize::try_from(n).map_err(|_| refuse("a string length past the address space"))?)?;
                    self.push_val(Kind::Str(s))?;
                }
                b'T' | b'U' | b'B' | b'C' | 0x8e => {
                    // Byte strings are never read: only their length, taken from the stream, is checked and charged.
                    let n = match op {
                        b'T' | b'B' => self.u32()? as usize,
                        b'U' | b'C' => self.u8()? as usize,
                        _ => usize::try_from(self.u64()?).map_err(|_| refuse("a bytes length past the address space"))?,
                    };
                    self.take(n)?;
                    self.charge(n as u64)?;
                    self.push_val(Kind::Bytes)?;
                }
                // containers
                b')' => self.push_val(Kind::Tuple(Vec::new()))?,
                0x85 | 0x86 | 0x87 => {
                    let n = (op - 0x84) as usize;
                    let mut items = Vec::with_capacity(n);
                    for _ in 0..n {
                        items.push(self.pop()?);
                    }
                    items.reverse();
                    self.make_tuple(items)?;
                }
                b't' => {
                    let items = self.pop_mark()?;
                    self.make_tuple(items)?;
                }
                b']' => self.push_val(Kind::List(Vec::new()))?,
                b'l' => {
                    let items = self.pop_mark()?;
                    let l = self.node(Kind::List(Vec::new()), 0)?;
                    self.list_extend(&l, items)?;
                    self.push(Item::V(l))?;
                }
                b'a' => {
                    let v = self.pop()?;
                    let l = self.top_val()?;
                    self.list_extend(&l, vec![v])?;
                }
                b'e' => {
                    let items = self.pop_mark()?;
                    let l = self.top_val()?;
                    self.list_extend(&l, items)?;
                }
                b'}' => self.push_val(Kind::Dict(Vec::new()))?,
                b'd' => {
                    let items = self.pop_mark()?;
                    let d = self.node(Kind::Dict(Vec::new()), 0)?;
                    self.dict_insert(&d, items)?;
                    self.push(Item::V(d))?;
                }
                b's' => {
                    let v = self.pop()?;
                    let k = self.pop()?;
                    let d = self.top_val()?;
                    self.dict_insert(&d, vec![k, v])?;
                }
                b'u' => {
                    let items = self.pop_mark()?;
                    let d = self.top_val()?;
                    self.dict_insert(&d, items)?;
                }
                // memo
                b'q' => {
                    let i = self.u8()? as usize;
                    self.memo_put(i)?;
                }
                b'r' => {
                    let i = self.u32()? as usize;
                    self.memo_put(i)?;
                }
                0x94 => {
                    // Python's pickler memoizes under `len(memo)`: the number of entries put so far.
                    let i = self.memo_count;
                    self.memo_put(i)?;
                }
                b'h' => {
                    let i = self.u8()? as usize;
                    self.memo_get(i)?;
                }
                b'j' => {
                    let i = self.u32()? as usize;
                    self.memo_get(i)?;
                }
                // calls
                b'R' => self.reduce()?,
                b'b' => self.build()?,
                b'Q' => self.persistent_load()?,
                // Everything else constructs objects or reads outside the file: refused.
                b'i' | b'o' | 0x81 | 0x92 => return Err(refuse("an opcode that instantiates a class (INST/OBJ/NEWOBJ) — refused, never run")),
                b'P' => return Err(refuse("PERSID (a text persistent id)")),
                0x82..=0x84 => return Err(refuse("an extension-registry opcode (EXT1/EXT2/EXT4)")),
                0x8f..=0x91 => return Err(refuse("a set or frozenset")),
                0x96..=0x98 => return Err(refuse("an out-of-band buffer opcode")),
                other => return Err(refuse(format!("the unknown pickle opcode 0x{other:02x}"))),
            }
        }
    }
}

// ───────────────────────────────────────── the checkpoint ─────────────────────────────────────────

/// The state dict a pickle describes: name → tensor, in the pickle's order.
fn state_dict_of(top: &Val, lim: &Limits) -> Result<Vec<(String, Rc<TensorRef>)>> {
    let k = top.kind.borrow();
    let d = match &*k {
        Kind::Dict(d) => d,
        other => return Err(refuse(format!("the pickle's object is {}, not a state dict (a mapping of names to tensors)", describe(other)))),
    };
    if d.len() > lim.max_tensors {
        return Err(refuse(format!("a state dict of {} entries", d.len())));
    }
    let mut out = Vec::with_capacity(d.len());
    for (key, v) in d {
        let name = match &*key.kind.borrow() {
            Kind::Str(s) => s.clone(),
            _ => return Err(refuse("a state dict key that is not a string")),
        };
        match &*v.kind.borrow() {
            Kind::Tensor(t) => out.push((name, t.clone())),
            Kind::Dict(_) => {
                return Err(refuse(format!("`{name}` holds a mapping: this is a training checkpoint (a wrapper around the weights), not a flat state dict")));
            }
            other => return Err(refuse(format!("`{name}` is {}, not a tensor", describe(other)))),
        }
    }
    Ok(out)
}

/// Row-major strides of `shape` (dims of size 1 may carry any stride).
fn contiguous(shape: &[u64], stride: &[u64]) -> bool {
    let mut expect = 1u64;
    for (d, s) in shape.iter().zip(stride).rev() {
        if *d != 1 && *s != expect {
            return false;
        }
        expect = expect.saturating_mul(*d);
    }
    true
}

/// Read the headers of the checkpoint at `path`: its tensors' names, dtypes, shapes and where their bytes are. **Nothing is run and no
/// tensor data is read.**
pub fn read_header(path: &Path) -> Result<TorchHeader> {
    read_header_with(path, &Limits::default())
}

pub fn read_header_with(path: &Path, lim: &Limits) -> Result<TorchHeader> {
    let file = std::fs::File::open(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?;
    let mut z = Zip::open(&file, lim)?;
    // The pickle: the one member named `<prefix>/data.pkl`.
    let pkls: Vec<Member> = z.members.values().filter(|m| m.name == "data.pkl" || m.name.ends_with("/data.pkl")).cloned().collect();
    let pkl = match pkls.as_slice() {
        [one] => one.clone(),
        [] => return Err(refuse("no data.pkl member — not a torch.save archive")),
        _ => return Err(refuse("more than one data.pkl member")),
    };
    let prefix = pkl.name[..pkl.name.len() - "data.pkl".len()].to_string();
    if pkl.size > lim.max_pickle_bytes {
        return Err(refuse(format!("data.pkl is {} bytes (the bound is {})", pkl.size, lim.max_pickle_bytes)));
    }
    if let Some(bo) = z.members.get(&format!("{prefix}byteorder")).cloned() {
        let at = z.data_start(&bo)?;
        let b = z.pread(at, bo.size.min(16))?;
        if b != b"little" {
            return Err(refuse("a checkpoint that is not little-endian"));
        }
    }
    let start = z.data_start(&pkl)?;
    let bytes = z.pread(start, pkl.size)?;
    let mut m = Machine::new(&bytes, *lim);
    let top = m.run()?;
    let dict = state_dict_of(&top, lim)?;
    let mut entries = BTreeMap::new();
    let mut data_end = 0u64;
    let mut starts: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for (name, t) in dict {
        let s = &t.storage;
        // The storage's member.
        let (member_start, member_size) = match starts.get(&s.key) {
            Some(x) => *x,
            None => {
                let mname = format!("{prefix}data/{}", s.key);
                let mem = z.members.get(&mname).cloned().ok_or_else(|| refuse(format!("tensor `{name}`: no member `{mname}` holds its storage")))?;
                let st = z.data_start(&mem)?;
                starts.insert(s.key.clone(), (st, mem.size));
                (st, mem.size)
            }
        };
        if s.numel.checked_mul(s.elem as u64) != Some(member_size) {
            return Err(refuse(format!("tensor `{name}`: storage `{}` declares {} {} elements but its member holds {member_size} bytes", s.key, s.numel, s.dtype)));
        }
        if !contiguous(&t.shape, &t.stride) {
            return Err(refuse(format!(
                "tensor `{name}` is a strided view (shape {:?}, stride {:?}), not a contiguous tensor — re-save it with .contiguous()",
                t.shape, t.stride
            )));
        }
        let numel = t.shape.iter().try_fold(1u64, |a, d| a.checked_mul(*d)).filter(|n| *n <= 1 << 40).ok_or_else(|| refuse(format!("tensor `{name}`: an element count past 2^40")))?;
        if numel > 0 && t.offset.checked_add(numel).is_none_or(|e| e > s.numel) {
            return Err(refuse(format!("tensor `{name}`: {numel} elements at storage offset {} run past its storage of {}", t.offset, s.numel)));
        }
        let begin = member_start + t.offset * s.elem as u64;
        let nbytes = numel * s.elem as u64;
        data_end = data_end.max(begin + nbytes);
        let shape: Vec<usize> = t.shape.iter().map(|d| *d as usize).collect();
        if entries.insert(name.clone(), TorchEntry { dtype: s.dtype, shape, begin, bytes: nbytes }).is_some() {
            return Err(refuse(format!("tensor `{name}` appears twice")));
        }
    }
    let digest = {
        let h = blake2b_simd::Params::new().hash_length(32).hash(&bytes);
        h.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    Ok(TorchHeader { entries, file_len: z.len, bytes_read: z.read.min(z.len), data_end, pickle_digest: digest })
}

/// Whether `path` names a PyTorch checkpoint by its extension (`.bin`, `.pt`, `.pth`).
pub fn is_torch_path(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("bin" | "pt" | "pth"))
}

/// Run a pickle program (no archive) under `lim` and return the tensors it describes as `(name, storage key, dtype, offset, shape)`.
/// For the tests of the interpreter's refusals; a checkpoint is read by [`read_header`].
pub fn interpret_pickle(bytes: &[u8], lim: &Limits) -> Result<Vec<(String, String, &'static str, u64, Vec<u64>)>> {
    let mut m = Machine::new(bytes, *lim);
    let top = m.run()?;
    Ok(state_dict_of(&top, lim)?
        .into_iter()
        .map(|(n, t)| (n, t.storage.key.clone(), t.storage.dtype, t.offset, t.shape.clone()))
        .collect())
}
