//! **A sharded checkpoint whose shards arrive one at a time** (genesis 8, 2026-10-15: shard in →
//! chunk out → upload → delete).
//!
//! A conversion reads a checkpoint occurrence by occurrence (`pre`, layer 0, layer 1, …, `post`), and
//! Hugging Face shards are cut in that order, so at any time it reads one or two shards. This source
//! knows every shard's HEADER from the start — a header-only stub (`<shard>.header`: the 8-byte length
//! and the JSON header, what `scripts/genesis/genesis_header_fixtures.py` fetches by HTTP range) — so
//! names, shapes, dtypes and byte lengths answer without data. A shard's DATA is fetched when a read
//! first needs it, by running the fetch program (the genesis pipeline: `hf download`, then the
//! publisher's LFS SHA-256 checked), and the least recently used shard is deleted when more than
//! `keep` are resident. A shard read again after its eviction is fetched again: slower, never wrong.
//!
//! * **The real header must be the stub.** A fetched shard's own header (length, JSON bytes) must be
//!   byte-identical to the stub the program was lowered from; otherwise it is refused (a shard that
//!   changed between the plan and the read is a different checkpoint).
//! * **What it deletes.** Only shards it fetched itself (a shard already on disk when the source
//!   opened is the caller's and is kept).
//! * **Untrusted data.** A shard is parsed as safetensors (bounded header, checked offsets); nothing
//!   in it is executed. The fetch program is the caller's, named on the command line.

use super::{Entry, SafetensorsFile, Tensor, TensorMeta, TensorSource, parse_header};
use crate::error::{LowerError, Result};
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The suffix of a shard's header-only stub.
pub const SHARD_STUB_SUFFIX_V1: &str = ".header";
/// A stub larger than this is refused (the local reader's header cap plus the length).
const MAX_STUB: u64 = 100 * 1024 * 1024 + 8;

/// How shards are obtained.
#[derive(Clone, Debug)]
pub struct LazyShardSpecV1 {
    /// The fetch program and its arguments. It is run with `PALW_SHARD_NAME` (the shard's file name),
    /// `PALW_SHARD_PATH` (where it must leave the file) and `PALW_SHARD_BYTES` (its length) in the
    /// environment, and must exit 0 with the file in place.
    pub fetch: Vec<String>,
    /// The most shards resident at once (at least 1; 2 lets a layer straddle a cut without refetching).
    pub keep: usize,
}

struct LazyShard {
    name: String,
    path: PathBuf,
    file_len: u64,
    stub: Vec<u8>,
    entries: BTreeMap<String, Entry>,
}

struct Resident {
    shard: usize,
    file: SafetensorsFile,
    fetched_here: bool,
    last_use: u64,
}

/// The sharded checkpoint read lazily (see the module docs).
pub struct LazyShardCheckpoint {
    spec: LazyShardSpecV1,
    shards: Vec<LazyShard>,
    index: BTreeMap<String, usize>,
    state: Mutex<(Vec<Resident>, u64, u64)>,
}

fn io(p: &Path, e: impl std::fmt::Display) -> LowerError {
    LowerError::Io(format!("{}: {e}", p.display()))
}

impl LazyShardCheckpoint {
    /// Open `dir` (holding `model.safetensors.index.json`, or one `model.safetensors[.header]`), with
    /// every shard's stub `<shard>.header` beside it (or the shard itself). `sizes` gives each
    /// shard's real length (the registry's); a stub alone does not say how long its file is.
    pub fn open(dir: &Path, sizes: &BTreeMap<String, u64>, spec: LazyShardSpecV1) -> Result<Self> {
        if spec.fetch.is_empty() {
            return Err(LowerError::bad("a lazy checkpoint needs a fetch program"));
        }
        let idx = dir.join("model.safetensors.index.json");
        let names: Vec<String> = if idx.exists() {
            let v: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&idx).map_err(|e| io(&idx, e))?).map_err(|e| LowerError::weights(format!("index: {e}")))?;
            let wm = v.get("weight_map").and_then(|w| w.as_object()).ok_or_else(|| LowerError::weights("index has no weight_map"))?;
            let mut s: Vec<String> = wm.values().filter_map(|x| x.as_str().map(str::to_string)).collect();
            s.sort();
            s.dedup();
            s
        } else {
            vec!["model.safetensors".to_string()]
        };
        let mut shards = Vec::new();
        let mut index = BTreeMap::new();
        for name in names {
            if name.contains('/') || name.contains('\\') || name.starts_with('.') {
                return Err(LowerError::weights(format!("shard name `{name}`")));
            }
            let path = dir.join(&name);
            let stub_path = dir.join(format!("{name}{SHARD_STUB_SUFFIX_V1}"));
            let file_len = *sizes.get(&name).ok_or_else(|| LowerError::weights(format!("no length given for shard `{name}`")))?;
            let stub = if stub_path.exists() {
                let len = std::fs::metadata(&stub_path).map_err(|e| io(&stub_path, e))?.len();
                if len > MAX_STUB {
                    return Err(LowerError::weights(format!("{}: a stub of {len} bytes", stub_path.display())));
                }
                std::fs::read(&stub_path).map_err(|e| io(&stub_path, e))?
            } else if path.exists() {
                read_header_bytes(&path)?
            } else {
                return Err(LowerError::weights(format!("shard `{name}`: neither the file nor its stub `{}`", stub_path.display())));
            };
            let (entries, _) = parse_header(&stub, file_len)?;
            let stub = stub_header_prefix(&stub)?;
            for n in entries.keys() {
                if index.insert(n.clone(), shards.len()).is_some() {
                    return Err(LowerError::weights(format!("tensor `{n}` appears in two shards")));
                }
            }
            shards.push(LazyShard { name, path, file_len, stub, entries });
        }
        let keep = spec.keep.max(1);
        Ok(Self { spec: LazyShardSpecV1 { keep, ..spec }, shards, index, state: Mutex::new((Vec::new(), 0, 0)) })
    }

    /// How many shard fetches the source has run (instrumentation: a second pass refetches).
    pub fn fetches(&self) -> u64 {
        self.state.lock().map(|s| s.2).unwrap_or(0)
    }

    fn shard_of(&self, name: &str) -> Result<usize> {
        self.index.get(name).copied().ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))
    }

    /// The shard's file, fetched if need be, evicting the least recently used beyond `keep`.
    fn resident(&self, i: usize) -> Result<SafetensorsFile> {
        let mut st = self.state.lock().map_err(|_| LowerError::eval("lazy checkpoint: a poisoned lock"))?;
        st.1 += 1;
        let tick = st.1;
        if let Some(r) = st.0.iter_mut().find(|r| r.shard == i) {
            r.last_use = tick;
            return Ok(r.file.clone());
        }
        // Make room first: the disk holds at most `keep` shards this source fetched.
        while st.0.len() >= self.spec.keep {
            let (k, _) = st.0.iter().enumerate().min_by_key(|(_, r)| r.last_use).expect("non-empty");
            let gone = st.0.remove(k);
            if gone.fetched_here {
                let _ = std::fs::remove_file(&gone.file.path);
            }
        }
        let s = &self.shards[i];
        let fetched_here = if s.path.exists() {
            false
        } else {
            let (prog, args) = self.spec.fetch.split_first().expect("checked in open");
            let status = std::process::Command::new(prog)
                .args(args)
                .env("PALW_SHARD_NAME", &s.name)
                .env("PALW_SHARD_PATH", &s.path)
                .env("PALW_SHARD_BYTES", s.file_len.to_string())
                .status()
                .map_err(|e| LowerError::Io(format!("{prog}: {e}")))?;
            if !status.success() {
                return Err(LowerError::Io(format!("{prog} exited {status} fetching shard `{}`", s.name)));
            }
            st.2 += 1;
            true
        };
        let len = std::fs::metadata(&s.path).map_err(|e| io(&s.path, e))?.len();
        if len != s.file_len {
            return Err(LowerError::weights(format!("shard `{}`: {len} bytes, the registry says {}", s.name, s.file_len)));
        }
        let real = stub_header_prefix(&read_header_bytes(&s.path)?)?;
        if real != s.stub {
            return Err(LowerError::weights(format!("shard `{}`: its header is not the stub the program was lowered from", s.name)));
        }
        let file = SafetensorsFile::open(&s.path)?;
        st.0.push(Resident { shard: i, file: file.clone(), fetched_here, last_use: tick });
        Ok(file)
    }
}

/// A file's 8-byte length and JSON header.
fn read_header_bytes(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| io(path, e))?;
    let mut head = [0u8; 8];
    f.read_exact(&mut head).map_err(|e| io(path, e))?;
    let n = u64::from_le_bytes(head);
    if n + 8 > MAX_STUB {
        return Err(LowerError::weights(format!("{}: implausible header length {n}", path.display())));
    }
    let mut buf = vec![0u8; 8 + n as usize];
    buf[..8].copy_from_slice(&head);
    f.read_exact(&mut buf[8..]).map_err(|e| io(path, e))?;
    Ok(buf)
}

/// The length and header of a stub or a file's head (anything after the header is ignored).
fn stub_header_prefix(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 8 {
        return Err(LowerError::weights("a stub shorter than its length field"));
    }
    let n = u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes"));
    let end = 8usize.checked_add(n as usize).filter(|e| *e <= bytes.len()).ok_or_else(|| LowerError::weights("a stub cut inside its header"))?;
    Ok(bytes[..end].to_vec())
}

impl TensorSource for LazyShardCheckpoint {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.index.get(name).map(|i| self.shards[*i].entries[name].shape.clone())
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        let i = self.shard_of(name)?;
        self.resident(i)?.read(name)
    }
    fn names(&self) -> Vec<String> {
        self.index.keys().cloned().collect()
    }
    fn load_i32(&self, name: &str) -> Result<(Vec<usize>, Vec<i32>)> {
        let i = self.shard_of(name)?;
        self.resident(i)?.read_i32(name)
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        self.index.get(name).map(|i| {
            let e = &self.shards[*i].entries[name];
            TensorMeta { dtype: e.dtype.clone(), shape: e.shape.clone(), bytes: e.end - e.begin }
        })
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let i = self.shard_of(name)?;
        self.resident(i)?.read_range(name, range)
    }
    fn load_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        let i = self.shard_of(name)?;
        self.resident(i)?.read_rows(name, rows)
    }
    fn serves_row_ranges(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shard(tensors: &[(&str, Vec<f32>)]) -> Vec<u8> {
        let mut header = serde_json::Map::new();
        let mut data = Vec::new();
        for (n, v) in tensors {
            let b = data.len();
            for x in v {
                data.extend_from_slice(&x.to_le_bytes());
            }
            header.insert((*n).into(), serde_json::json!({"dtype": "F32", "shape": [v.len()], "data_offsets": [b, data.len()]}));
        }
        let h = serde_json::Value::Object(header).to_string().into_bytes();
        let mut out = (h.len() as u64).to_le_bytes().to_vec();
        out.extend_from_slice(&h);
        out.extend_from_slice(&data);
        out
    }

    /// Two shards, stubs only on disk; a "fetch" that copies from a store directory. Reads fetch on
    /// demand, `keep = 1` evicts, a changed shard is refused.
    #[test]
    fn shards_arrive_on_demand_and_leave_when_evicted() {
        let root = std::env::temp_dir().join(format!("lazy-shards-{}", std::process::id()));
        let (dir, store) = (root.join("ck"), root.join("store"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&store).unwrap();
        let a = shard(&[("a.w", vec![1.0, 2.0]), ("a.b", vec![3.0])]);
        let b = shard(&[("b.w", vec![4.0, 5.0, 6.0])]);
        let mut sizes = BTreeMap::new();
        for (n, bytes) in [("m-1.safetensors", &a), ("m-2.safetensors", &b)] {
            std::fs::write(store.join(n), bytes).unwrap();
            let hl = 8 + u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
            std::fs::write(dir.join(format!("{n}.header")), &bytes[..hl]).unwrap();
            sizes.insert(n.to_string(), bytes.len() as u64);
        }
        let wm = serde_json::json!({"weight_map": {"a.w": "m-1.safetensors", "a.b": "m-1.safetensors", "b.w": "m-2.safetensors"}});
        std::fs::write(dir.join("model.safetensors.index.json"), wm.to_string()).unwrap();
        let script = root.join("fetch.sh");
        std::fs::write(&script, format!("#!/bin/sh\ncp \"{}/$PALW_SHARD_NAME\" \"$PALW_SHARD_PATH\"\n", store.display())).unwrap();
        let spec = LazyShardSpecV1 { fetch: vec!["/bin/sh".into(), script.display().to_string()], keep: 1 };
        let ck = LazyShardCheckpoint::open(&dir, &sizes, spec).unwrap();
        assert_eq!(ck.shape("b.w"), Some(vec![3]), "shapes from the stubs, no fetch");
        assert_eq!(ck.fetches(), 0);
        assert_eq!(ck.load("a.w").unwrap().data, vec![1.0, 2.0]);
        assert!(dir.join("m-1.safetensors").exists());
        assert_eq!(ck.load("b.w").unwrap().data, vec![4.0, 5.0, 6.0]);
        assert!(!dir.join("m-1.safetensors").exists(), "keep = 1 evicted the first shard");
        assert_eq!(ck.load("a.b").unwrap().data, vec![3.0], "an evicted shard is fetched again");
        assert_eq!(ck.fetches(), 3);
        // A shard whose header changed after the plan is refused.
        std::fs::write(store.join("m-2.safetensors"), shard(&[("b.w", vec![4.0, 5.0, 7.0, 8.0])])).unwrap();
        let _ = std::fs::remove_file(dir.join("m-2.safetensors"));
        assert!(ck.load("b.w").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
