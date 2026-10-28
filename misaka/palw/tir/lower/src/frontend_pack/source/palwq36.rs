//! Explicit PALWQ361 source view. This is acquisition, never an architecture decoder.
//! Every code, grouped exponent, rotary entry and seventeen-byte quantization row is retained.
//! Split m/s/z planes are exact integer views; a public TIR program must implement their math.
use super::*;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::Mutex,
};
const MAX_FILE: u64 = 512 << 30;
const MAX_HEADER: u64 = 2 << 30;
const MAX_ENTRIES: u64 = 200_000;
const MAX_RANGE: u64 = 64 << 20;
const MAX_NAME: u64 = 1024;
#[derive(Clone)]
struct Entry {
    at: u64,
    rows: u64,
    width: u64,
    stride: u64,
    lane: u64,
    dtype: &'static str,
}
pub(super) struct PalwQ36 {
    path: PathBuf,
    file: Mutex<File>,
    length: u64,
    modified: Option<std::time::SystemTime>,
    entries: BTreeMap<String, Entry>,
    pub(super) config: Value,
}
fn fail(s: impl std::fmt::Display) -> LowerError {
    bad(format!("FRONTEND_PALWQ361: {s}"))
}
struct Reader {
    file: File,
    pos: u64,
    len: u64,
}
impl Reader {
    fn take(&mut self, n: u64) -> Result<Vec<u8>> {
        let end = self.pos.checked_add(n).ok_or_else(|| fail("header overflow"))?;
        if n > MAX_RANGE || end > self.len || end > MAX_HEADER {
            return Err(fail("header/range limit"));
        }
        let mut b = vec![0; usize::try_from(n).map_err(|_| fail("host size"))?];
        self.file.read_exact(&mut b).map_err(fail)?;
        self.pos = end;
        Ok(b)
    }
    fn skip(&mut self, n: u64) -> Result<u64> {
        let at = self.pos;
        let end = at.checked_add(n).ok_or_else(|| fail("offset overflow"))?;
        if end > self.len || end > MAX_HEADER {
            return Err(fail("header extent"));
        }
        self.file.seek(SeekFrom::Start(end)).map_err(fail)?;
        self.pos = end;
        Ok(at)
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| fail("u64"))?))
    }
    fn name(&mut self) -> Result<String> {
        let n = self.u64()?;
        if n == 0 || n > MAX_NAME {
            return Err(fail("name length"));
        }
        let b = self.take(n)?;
        let s = String::from_utf8(b).map_err(|_| fail("name UTF8"))?;
        if s.contains('\0') {
            return Err(fail("NUL name"));
        }
        Ok(s)
    }
}
impl PalwQ36 {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(fail)?;
        let meta = file.metadata().map_err(fail)?;
        if !meta.is_file() || meta.len() > MAX_FILE {
            return Err(fail("file limit"));
        }
        let mut r = Reader { file, pos: 0, len: meta.len() };
        if r.take(8)?.as_slice() != b"PALWQ361" {
            return Err(fail("magic/version"));
        }
        let layers = r.u64()?;
        if layers == 0 || layers > 1024 {
            return Err(fail("layer count"));
        }
        let kinds = r.take(layers)?;
        if kinds.iter().any(|k| *k > 1) {
            return Err(fail("layer kind"));
        }
        let fields = [
            "d_model",
            "n_heads",
            "n_kv_heads",
            "head_dim",
            "rotary_dim",
            "linear_k_heads",
            "linear_v_heads",
            "linear_head_dim",
            "conv_kernel",
            "n_experts",
            "experts_per_token",
            "moe_dim",
            "shared_dim",
            "vocab",
            "max_position",
        ];
        let mut shape = serde_json::Map::new();
        shape.insert("layer_types".into(), Value::Array(kinds.into_iter().map(Value::from).collect()));
        for name in fields {
            let n = r.u64()?;
            if n > 1 << 30 {
                return Err(fail("shape field bound"));
            }
            shape.insert(name.into(), Value::from(n));
        }
        let eps = i64::from_le_bytes(r.take(8)?.try_into().map_err(|_| fail("eps"))?);
        shape.insert("eps_q".into(), Value::from(eps));
        let router = r.take(1)?[0];
        if router > 62 {
            return Err(fail("router widening"));
        }
        shape.insert("router_up_bits".into(), Value::from(router));
        let dh = r.u64()?;
        let maxpos = r.u64()?;
        if dh == 0 || dh % 2 != 0 || dh > 1 << 20 || maxpos == 0 || maxpos > 1 << 30 {
            return Err(fail("rotary geometry"));
        }
        let rope_count = dh.checked_div(2).and_then(|x| x.checked_mul(maxpos)).ok_or_else(|| fail("rotary overflow"))?;
        let mut entries = BTreeMap::new();
        for name in ["palwq361.rope.cos", "palwq361.rope.sin"] {
            let count = r.u64()?;
            if count != rope_count {
                return Err(fail("rotary count"));
            }
            let at = r.skip(count.checked_mul(4).ok_or_else(|| fail("rotary bytes"))?)?;
            entries.insert(name.into(), Entry { at, rows: count, width: 4, stride: 4, lane: 0, dtype: "I32" });
        }
        let np = r.u64()?;
        if np > MAX_ENTRIES {
            return Err(fail("parameter count"));
        }
        let mut source_names = std::collections::BTreeSet::new();
        for _ in 0..np {
            let name = r.name()?;
            if !source_names.insert(name.clone()) {
                return Err(fail("duplicate parameter"));
            }
            let bytes = r.u64()?;
            if bytes == 0 || bytes % 17 != 0 {
                return Err(fail("quantization row must have seventeen bytes"));
            }
            let at = r.skip(bytes)?;
            let rows = bytes / 17;
            for (suffix, lane, width, dtype) in [("m", 0, 8, "I64"), ("s", 8, 1, "I8"), ("z", 9, 8, "I64")] {
                let key = format!("palwq361.parameter.{name}.{suffix}");
                if entries.insert(key, Entry { at, rows, width, stride: 17, lane, dtype }).is_some() {
                    return Err(fail("duplicate plane"));
                }
            }
        }
        let nt = r.u64()?;
        if nt == 0 || nt > MAX_ENTRIES {
            return Err(fail("tensor count"));
        }
        let mut raw = Vec::with_capacity(nt as usize);
        let mut expected = 0u64;
        for _ in 0..nt {
            let name = r.name()?;
            if !source_names.insert(name.clone()) || entries.contains_key(&name) {
                return Err(fail("duplicate tensor/parameter name"));
            }
            let offset = r.u64()?;
            let len = r.u64()?;
            if offset != expected || len == 0 {
                return Err(fail("tensor directory must be contiguous/nonempty"));
            }
            expected = offset.checked_add(len).ok_or_else(|| fail("tensor extent overflow"))?;
            raw.push((name, offset, len));
        }
        let data = r.u64()?;
        let aligned = r.pos.checked_add(16383).map(|v| v / 16384 * 16384).ok_or_else(|| fail("alignment overflow"))?;
        if data != aligned || data.checked_add(expected) != Some(r.len) {
            return Err(fail("data extent/alignment/trailing bytes"));
        }
        if r.take(data - r.pos)?.iter().any(|b| *b != 0) {
            return Err(fail("nonzero header padding"));
        }
        for (name, offset, len) in raw {
            entries.insert(name, Entry { at: data + offset, rows: len, width: 1, stride: 1, lane: 0, dtype: "I8" });
        }
        let config = serde_json::json!({"palwq361.version":1,"palwq361.shape":shape,"palwq361.rope":{"d_head":dh,"max_position":maxpos},"palwq361.parameter_view":"exact-m-i64/s-i8/z-i64-v1"});
        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(r.file),
            length: meta.len(),
            modified: meta.modified().ok(),
            entries,
            config,
        })
    }
    pub(super) fn files(&self) -> Vec<PathBuf> {
        vec![self.path.clone()]
    }
}
impl TensorSource for PalwQ36 {
    fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.metadata(name).map(|m| m.shape)
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        let e = self.entries.get(name)?;
        Some(TensorMeta { dtype: e.dtype.into(), shape: vec![usize::try_from(e.rows).ok()?], bytes: e.rows.checked_mul(e.width)? })
    }
    fn load(&self, _: &str) -> Result<Tensor> {
        Err(fail("integer-only source: implicit float decoding is forbidden"))
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let e = self.entries.get(name).ok_or_else(|| fail("unknown tensor"))?;
        let total = e.rows.checked_mul(e.width).ok_or_else(|| fail("view extent"))?;
        if range.start > range.end || range.end > total || range.end - range.start > MAX_RANGE {
            return Err(fail("read range"));
        }
        let n = range.end - range.start;
        let mut out = vec![0; usize::try_from(n).map_err(|_| fail("host size"))?];
        if n == 0 {
            return Ok(out);
        }
        let mut file = self.file.lock().map_err(|_| fail("file lock"))?;
        if e.width == e.stride {
            file.seek(SeekFrom::Start(e.at + range.start)).map_err(fail)?;
            file.read_exact(&mut out).map_err(fail)?;
        } else {
            // Strided views never materialize the full parameter header. Each bounded batch
            // preserves all bits, including zero-point fields used as q36 coefficients.
            let first = range.start / e.width;
            let last = (range.end - 1) / e.width;
            let mut at = first;
            file.seek(SeekFrom::Start(e.at + first * 17)).map_err(fail)?;
            while at <= last {
                let rows = (last - at + 1).min(16_384);
                let mut batch = vec![0u8; rows as usize * 17];
                file.read_exact(&mut batch).map_err(fail)?;
                for (j, row) in batch.chunks_exact(17).enumerate() {
                    let i = at + j as u64;
                    if row[8] > 62 {
                        return Err(fail("quantization shift exceeds62"));
                    }
                    let begin = range.start.max(i * e.width);
                    let end = range.end.min((i + 1) * e.width);
                    let dst = (begin - range.start) as usize;
                    let src = (e.lane + begin - i * e.width) as usize;
                    out[dst..dst + (end - begin) as usize].copy_from_slice(&row[src..src + (end - begin) as usize]);
                }
                at += rows;
            }
        }
        Ok(out)
    }
    fn validate_snapshot(&self) -> Result<()> {
        let m = std::fs::metadata(&self.path).map_err(fail)?;
        if m.len() != self.length || m.modified().ok() != self.modified {
            return Err(fail("source changed"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(b: &mut Vec<u8>, n: u64) {
        b.extend_from_slice(&n.to_le_bytes());
    }
    fn name(b: &mut Vec<u8>, s: &str) {
        put(b, s.len() as u64);
        b.extend_from_slice(s.as_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut b = b"PALWQ361".to_vec();
        put(&mut b, 1);
        b.push(1);
        for n in [2, 1, 1, 2, 2, 0, 0, 0, 0, 1, 1, 2, 0, 4, 1] {
            put(&mut b, n);
        }
        b.extend_from_slice(&1i64.to_le_bytes());
        b.push(0);
        put(&mut b, 2);
        put(&mut b, 1);
        for v in [123i32, -456] {
            put(&mut b, 1);
            b.extend_from_slice(&v.to_le_bytes());
        }
        put(&mut b, 1);
        name(&mut b, "output.a16");
        put(&mut b, 34);
        for (m, s, z) in [(-123i64, 2u8, i64::MIN), (456, 62, i64::MAX)] {
            b.extend_from_slice(&m.to_le_bytes());
            b.push(s);
            b.extend_from_slice(&z.to_le_bytes());
        }
        put(&mut b, 2);
        name(&mut b, "output.weight");
        put(&mut b, 0);
        put(&mut b, 4);
        name(&mut b, "output.weight.exp");
        put(&mut b, 4);
        put(&mut b, 2);
        let start = (b.len() as u64 + 8).div_ceil(16384) * 16384;
        put(&mut b, start);
        b.resize(start as usize, 0);
        b.extend_from_slice(&[0, 127, 128, 255, 3, 4]);
        b
    }
    fn with_source<T>(raw: &[u8], f: impl FnOnce(&Path) -> T) -> T {
        let dir = std::env::temp_dir().join(format!(
            "palwq36-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("source.palwq36");
        std::fs::write(&path, raw).unwrap();
        let v = f(&path);
        std::fs::remove_dir_all(dir).unwrap();
        v
    }
    #[test]
    fn exact_codes_group_exponents_rope_and_all_parameter_bits_are_retained() {
        with_source(&fixture(), |p| {
            let s = PalwQ36::open(p).unwrap();
            assert_eq!(s.names().len(), 7);
            assert_eq!(s.read_slice("output.weight", 0..4).unwrap(), vec![0, 127, 128, 255]);
            assert_eq!(s.read_slice("output.weight.exp", 0..2).unwrap(), vec![3, 4]);
            assert_eq!(s.read_slice("palwq361.rope.sin", 0..4).unwrap(), (-456i32).to_le_bytes());
            assert_eq!(s.read_slice("palwq361.parameter.output.a16.s", 0..2).unwrap(), vec![2, 62]);
            let bytes = s.read_slice("palwq361.parameter.output.a16.z", 0..16).unwrap();
            assert_eq!(&bytes[..8], &i64::MIN.to_le_bytes());
            assert_eq!(&bytes[8..], &i64::MAX.to_le_bytes());
            let m = s.read_slice("palwq361.parameter.output.a16.m", 3..12).unwrap();
            let all = [(-123i64).to_le_bytes(), 456i64.to_le_bytes()].concat();
            assert_eq!(m, &all[3..12]);
            assert!(s.load("output.weight").is_err());
            assert!(s.read_slice("output.weight", 0..5).is_err());
        });
    }
    #[test]
    fn unknown_version_huge_count_and_trailing_payload_refuse_before_allocation() {
        let mut magic = fixture();
        magic[7] = b'2';
        with_source(&magic, |p| assert!(PalwQ36::open(p).is_err()));
        let mut count = b"PALWQ361".to_vec();
        put(&mut count, u64::MAX);
        with_source(&count, |p| assert!(PalwQ36::open(p).is_err()));
        let mut trailing = fixture();
        trailing.push(0);
        with_source(&trailing, |p| assert!(PalwQ36::open(p).is_err()));
    }
}
