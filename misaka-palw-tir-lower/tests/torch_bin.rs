//! **PyTorch checkpoints (`pytorch_model.bin`) are read and never run** (`weights::torchzip`).
//!
//! `torch.load` executes the pickle inside the archive; this reader interprets it symbolically. Three kinds of test:
//!
//! * files `torch.save` wrote (tests/fixtures/torch, `tools/gen_torch_bin_fixtures.py`): every dtype, a tied weight (one storage, two
//!   names), a storage-offset view, a `state_dict()` with its `_metadata`, protocol 4, and a Hugging Face checkpoint re-saved as
//!   `.bin` (one file and two shards) that reads **tensor for tensor** like its safetensors original;
//! * the refusals of well-formed files: the legacy format, a strided view, a training checkpoint;
//! * **hostile and malformed pickles and archives**, built byte by byte here: an unknown global, REDUCE on a non-allowlisted callable,
//!   `STACK_GLOBAL` of `os.system`, huge length prefixes, deep nesting, a cycle, a memo and a stack bomb, an opcode bomb, an allocation
//!   bomb, instantiating opcodes, a lying zip. Every one is a `FORMAT_UNSUPPORTED` error that names its cause, none allocates what a
//!   prefix claims, and none runs anything (a pickle that would, if run, write a marker file: the marker never appears).

use misaka_palw_tir_lower::weights::torchzip::{self, Limits, interpret_pickle, read_header};
use misaka_palw_tir_lower::weights::{Checkpoint, SafetensorsFile, TensorSource};
use std::path::{Path, PathBuf};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("torch-bin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn refused(r: Result<impl std::fmt::Debug, impl std::fmt::Display>, why: &str) {
    match r {
        Ok(v) => panic!("expected a refusal containing `{why}`, got {v:?}"),
        Err(e) => {
            let e = e.to_string();
            assert!(e.contains("FORMAT_UNSUPPORTED"), "not a FORMAT_UNSUPPORTED refusal: {e}");
            assert!(e.contains(why), "`{why}` not in: {e}");
        }
    }
}

// ───────────────────────────────── files torch.save wrote ─────────────────────────────────

#[test]
fn a_flat_state_dict_reads_every_dtype_a_tied_weight_and_a_storage_offset() {
    let h = read_header(&fixture("torch/flat.bin")).expect("header");
    let want: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture("torch/flat.json")).unwrap()).unwrap();
    assert_eq!(h.entries.len(), want.as_object().unwrap().len());
    let f = SafetensorsFile::open(&fixture("torch/flat.bin")).expect("open as a checkpoint");
    for (name, w) in want.as_object().unwrap() {
        let e = &h.entries[name];
        assert_eq!(e.dtype, w["dtype"].as_str().unwrap(), "{name}");
        let shape: Vec<usize> = w["shape"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as usize).collect();
        assert_eq!(e.shape, shape, "{name}");
        // The values: widened by the same reader safetensors uses.
        let got = f.read(name).expect("read");
        let vals: Vec<f64> = w["values"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(got.shape, shape);
        assert_eq!(got.data.len(), vals.len());
        for (a, b) in got.data.iter().zip(&vals) {
            // f32/bf16/f16 widen exactly to the f32 the python side printed (as f64).
            assert!((*a as f64 - b).abs() <= 1e-6 * b.abs().max(1.0), "{name}: {a} vs {b}");
        }
    }
    // Tied weights share a storage; the view is the same bytes from the second row.
    assert_eq!(h.entries["tied"].begin, h.entries["embed.weight"].begin);
    assert_eq!(h.entries["view"].begin, h.entries["embed.weight"].begin + 4 * 4, "row 1 of a [3,4] f32");
    assert_eq!(h.entries["scalar"].shape, Vec::<usize>::new());
    assert!(h.bytes_read > 0 && h.bytes_read < h.file_len + 1);
}

#[test]
fn the_header_is_read_without_the_tensor_data() {
    // Overwrite every tensor's bytes: the header, which is the pickle and the directory, reads identically.
    let src = fixture("torch/flat.bin");
    let h = read_header(&src).unwrap();
    let mut bytes = std::fs::read(&src).unwrap();
    for e in h.entries.values() {
        for b in &mut bytes[e.begin as usize..(e.begin + e.bytes) as usize] {
            *b = 0xFF;
        }
    }
    let dir = scratch("nodata");
    let p = dir.join("zeroed.bin");
    std::fs::write(&p, &bytes).unwrap();
    let h2 = read_header(&p).unwrap();
    assert_eq!(h.entries, h2.entries);
    assert_eq!(h.pickle_digest, h2.pickle_digest);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_module_state_dict_with_its_metadata_and_protocol_4_both_read() {
    let want: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture("torch/state_dict.json")).unwrap()).unwrap();
    for file in ["torch/state_dict.bin", "torch/proto4.bin"] {
        let f = SafetensorsFile::open(&fixture(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(f.entries.len(), 4, "{file}");
        for (name, w) in want.as_object().unwrap() {
            let t = f.read(name).unwrap();
            let vals: Vec<f64> = w["values"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            assert_eq!(t.data.len(), vals.len(), "{file} {name}");
            for (a, b) in t.data.iter().zip(&vals) {
                assert!((*a as f64 - b).abs() <= 1e-6 * b.abs().max(1.0), "{file} {name}: {a} vs {b}");
            }
        }
    }
}

#[test]
fn well_formed_files_that_are_not_flat_contiguous_zip_checkpoints_are_refused_by_name() {
    refused(read_header(&fixture("torch/legacy.bin")), "legacy");
    refused(read_header(&fixture("torch/strided.bin")), "strided view");
    refused(read_header(&fixture("torch/nested.bin")), "training checkpoint");
    // …and they are the same refusal through the checkpoint opener.
    refused(SafetensorsFile::open(&fixture("torch/strided.bin")).map(|_| ()), "strided view");
}

fn assert_same_checkpoints(bin_dir: &str) {
    let bin = Checkpoint::open(&fixture(bin_dir)).unwrap_or_else(|e| panic!("{bin_dir}: {e}"));
    let st = Checkpoint::open(&fixture("hf/llama")).expect("safetensors original");
    let (mut a, mut b) = (bin.names(), st.names());
    a.sort();
    b.sort();
    assert_eq!(a, b, "{bin_dir}: the same tensor names");
    for n in &a {
        assert_eq!(bin.shape(n), st.shape(n), "{bin_dir} {n}");
        let (x, y) = (bin.load(n).unwrap(), st.load(n).unwrap());
        assert_eq!(x.data, y.data, "{bin_dir}: `{n}` is not bit-identical to its safetensors original");
    }
}

#[test]
fn a_huggingface_checkpoint_saved_as_pytorch_model_bin_reads_tensor_for_tensor_like_its_safetensors_original() {
    assert_same_checkpoints("hf-bin/llama");
    // Two shards and their index: the same checkpoint.
    assert_same_checkpoints("hf-bin/llama-sharded");
}

// ───────────────────────────────── a pickle assembler ─────────────────────────────────

#[derive(Default)]
struct Pk(Vec<u8>);

impl Pk {
    fn new() -> Pk {
        Pk(vec![0x80, 2])
    }
    fn raw(mut self, b: &[u8]) -> Pk {
        self.0.extend_from_slice(b);
        self
    }
    fn global(self, m: &str, n: &str) -> Pk {
        self.raw(b"c").raw(m.as_bytes()).raw(b"\n").raw(n.as_bytes()).raw(b"\n")
    }
    fn s(self, s: &str) -> Pk {
        let l = (s.len() as u32).to_le_bytes();
        self.raw(b"X").raw(&l).raw(s.as_bytes())
    }
    fn int(self, i: i64) -> Pk {
        if (0..256).contains(&i) { self.raw(&[b'K', i as u8]) } else { self.raw(&[b'J']).raw(&(i as i32).to_le_bytes()) }
    }
    fn mark(self) -> Pk {
        self.raw(b"(")
    }
    fn tuple_n(self, n: u8) -> Pk {
        match n {
            0 => self.raw(b")"),
            1 => self.raw(&[0x85]),
            2 => self.raw(&[0x86]),
            _ => self.raw(&[0x87]),
        }
    }
    fn ints(mut self, v: &[i64]) -> Pk {
        match v.len() {
            0 => self.raw(b")"),
            1..=3 => {
                for i in v {
                    self = self.int(*i);
                }
                self.tuple_n(v.len() as u8)
            }
            _ => {
                self = self.mark();
                for i in v {
                    self = self.int(*i);
                }
                self.raw(b"t")
            }
        }
    }
    fn ordered_dict(self) -> Pk {
        self.global("collections", "OrderedDict").raw(b")R")
    }
    fn storage(self, ty: &str, key: &str, numel: i64) -> Pk {
        self.mark().s("storage").global("torch", ty).s(key).s("cpu").int(numel).raw(b"tQ")
    }
    /// `torch._utils._rebuild_tensor_v2(storage, offset, size, stride, requires_grad, OrderedDict())`.
    fn tensor(self, ty: &str, key: &str, numel: i64, off: i64, size: &[i64], stride: &[i64]) -> Pk {
        self.global("torch._utils", "_rebuild_tensor_v2")
            .mark()
            .storage(ty, key, numel)
            .int(off)
            .ints(size)
            .ints(stride)
            .raw(&[0x89])
            .ordered_dict()
            .raw(b"tR")
    }
    /// An OrderedDict of one tensor, completed.
    fn one(self, name: &str, tensor: Pk) -> Pk {
        self.ordered_dict().mark().s(name).raw(&tensor.0).raw(b"u.")
    }
    fn done(self) -> Vec<u8> {
        self.0
    }
}

fn ok_limits() -> Limits {
    Limits::default()
}

#[test]
fn the_assembler_writes_a_pickle_the_interpreter_reads() {
    let p = Pk::new().one("w", Pk::default().tensor("FloatStorage", "0", 6, 0, &[2, 3], &[3, 1])).done();
    let t = interpret_pickle(&p, &ok_limits()).expect("read");
    assert_eq!(t, vec![("w".to_string(), "0".to_string(), "F32", 0, vec![2, 3])]);
}

// ───────────────────────────────── hostile pickles ─────────────────────────────────

fn rejects(p: Vec<u8>, why: &str) {
    refused(interpret_pickle(&p, &ok_limits()), why);
}

#[test]
fn a_global_off_the_allowlist_is_refused_when_read_so_it_can_never_be_called() {
    for (m, n) in [
        ("os", "system"),
        ("posix", "system"),
        ("builtins", "eval"),
        ("__builtin__", "exec"),
        ("subprocess", "check_output"),
        ("torch", "load"),
        ("torch", "UntypedStorage"),
        ("torch._utils", "_rebuild_qtensor"),
        ("numpy.core.multiarray", "_reconstruct"),
        ("collections", "defaultdict"),
        ("_codecs", "encode"),
        ("pickle", "loads"),
    ] {
        rejects(Pk::new().global(m, n).raw(b".").done(), &format!("`{m}.{n}`"));
    }
    // The same through STACK_GLOBAL (protocol 4), where the names are values on the stack.
    rejects(Pk::new().s("os").s("system").raw(&[0x93, b'.']).done(), "`os.system`");
}

#[test]
fn reduce_on_anything_but_an_allowlisted_callable_is_refused() {
    rejects(Pk::new().raw(b"N)R.").done(), "REDUCE on None");
    rejects(Pk::new().raw(b"]").raw(b")R.").done(), "REDUCE on a list");
    // An allowlisted global called with arguments it does not take.
    rejects(Pk::new().global("collections", "OrderedDict").mark().int(1).raw(b"tR.").done(), "called with arguments");
    rejects(Pk::new().global("torch._utils", "_rebuild_tensor_v2").raw(b")R.").done(), "0 arguments");
    rejects(Pk::new().global("torch", "FloatStorage").raw(b")R.").done(), "REDUCE on a storage type");
    // REDUCE whose arguments are not a tuple.
    rejects(Pk::new().global("collections", "OrderedDict").raw(b"NR.").done(), "arguments");
}

#[test]
fn the_instantiating_extension_and_buffer_opcodes_are_refused() {
    for (op, why) in [
        (&b"cos\nsystem\n)\x81."[..], "not on the allowlist"), // NEWOBJ after a refused global
        (&b"N)\x81."[..], "instantiates"),                    // NEWOBJ
        (&b"(N(No."[..], "instantiates"),                     // OBJ
        (&b"(S'x'\ni"[..], "protocol-0"),                     // INST via a text string first
        (&b"\x82\x01."[..], "extension-registry"),
        (&b"Pabc\n."[..], "PERSID"),
        (&b"\x8f."[..], "set"),
        (&b"\x97."[..], "buffer"),
        (&b"\xff."[..], "unknown pickle opcode"),
        (&b"I12\n."[..], "protocol-0"),
    ] {
        rejects(Pk::new().raw(op).done(), why);
    }
    rejects(Pk::new().raw(b"N\x8b\x01\x00\x00\x00\x01.").done(), "unbounded width");
}

#[test]
fn a_length_prefix_longer_than_the_bytes_left_is_refused_before_anything_is_allocated() {
    // BINUNICODE with 4 GiB − 1; SHORT_BINUNICODE with 255 and no bytes; BINUNICODE8 with 2^62; BINBYTES / BINBYTES8 likewise.
    rejects(Pk::new().raw(b"X\xff\xff\xff\xff").done(), "past the bound");
    rejects(Pk::new().raw(&[0x8c, 0xff]).done(), "longer than the bytes left");
    rejects(Pk::new().raw(&[0x8d]).raw(&(1u64 << 62).to_le_bytes()).done(), "past the bound");
    rejects(Pk::new().raw(b"B\xff\xff\xff\xff").done(), "longer than the bytes left");
    rejects(Pk::new().raw(&[0x8e]).raw(&(1u64 << 62).to_le_bytes()).done(), "longer than the bytes left");
    rejects(Pk::new().raw(&[0x8e]).raw(&u64::MAX.to_le_bytes()).done(), "longer than the bytes left");
    // A frame, a tuple count or a text line cannot make it read past the stream either.
    rejects(Pk::new().raw(b"cos").done(), "newline");
    rejects(Pk::new().raw(&[0x95, 1, 2]).done(), "cut short");
}

#[test]
fn nesting_the_stack_the_memo_and_the_opcodes_are_bounded() {
    // Containers nested 100 deep (the bound is 32).
    let mut deep = Pk::new().raw(b")");
    for _ in 0..100 {
        deep = deep.tuple_n(1);
    }
    rejects(deep.raw(b".").done(), "nested deeper");
    // A memo index of 2^32 − 1, and a get of an index never put.
    rejects(Pk::new().raw(b"Nr\xff\xff\xff\xff.").done(), "memo index");
    rejects(Pk::new().raw(b"h\x07.").done(), "never put");
    // A stack bomb and an opcode bomb, under small limits.
    let small = Limits { max_stack: 1000, max_ops: 5000, ..Limits::default() };
    let pushes = Pk::new().raw(&vec![b'N'; 2000]).raw(b".").done();
    refused(interpret_pickle(&pushes, &small), "stack");
    let spin = Pk::new().raw(&b"N0".repeat(4000)).raw(b".").done();
    refused(interpret_pickle(&spin, &small), "opcodes");
    // An allocation bomb: many strings under a small allocation bound.
    let mut p = Pk::new();
    for _ in 0..400 {
        p = p.s(&"a".repeat(1000)).raw(b"0");
    }
    let tiny = Limits { max_alloc_bytes: 100_000, ..Limits::default() };
    refused(interpret_pickle(&p.raw(b".").done(), &tiny), "allocates more");
    // A string over the per-string bound.
    let big = Limits { max_string: 16, ..Limits::default() };
    refused(interpret_pickle(&Pk::new().s(&"x".repeat(17)).raw(b".").done(), &big), "past the bound");
    // Truncated mid-stream.
    refused(interpret_pickle(&Pk::new().ordered_dict().raw(b"(X\x05\x00").done(), &ok_limits()), "cut short");
}

#[test]
fn a_value_cannot_be_made_a_member_of_itself_so_no_cycle_and_no_unbounded_depth_is_built() {
    // A list appended to itself.
    rejects(Pk::new().raw(b"]q\x00h\x00a.").done(), "member of itself");
    // B.append(A) seals A; A.append(B) is then a change to a member of another value.
    rejects(Pk::new().raw(b"]q\x00]q\x01h\x00aa.").done(), "member of another");
    // A dict made a member of itself.
    rejects(Pk::new().raw(b"}q\x00(X\x01\x00\x00\x00kh\x00u.").done(), "member of itself");
}

#[test]
fn a_tensor_with_a_lying_size_stride_hook_or_storage_is_refused() {
    let t = |ty: &str, size: &[i64], stride: &[i64]| Pk::new().one("w", Pk::default().tensor(ty, "0", 6, 0, size, stride)).done();
    rejects(t("FloatStorage", &[-2, 3], &[3, 1]), "negative");
    rejects(t("FloatStorage", &[2, 3], &[3, -1]), "negative");
    rejects(t("FloatStorage", &[1, 1, 1, 1, 1, 1, 1, 1, 1], &[1; 9]), "rank 9");
    rejects(t("FloatStorage", &[2, 3], &[3]), "differ in rank");
    // A hook is a callable: a non-empty backward_hooks dict is refused (the fifth argument is a bool, the sixth the hooks).
    let hooked = Pk::new()
        .ordered_dict()
        .mark()
        .s("w")
        .global("torch._utils", "_rebuild_tensor_v2")
        .mark()
        .storage("FloatStorage", "0", 6)
        .int(0)
        .ints(&[2, 3])
        .ints(&[3, 1])
        .raw(&[0x89])
        .raw(b"}X\x01\x00\x00\x00hK\x01s") // {'h': 1}
        .raw(b"tRu.")
        .done();
    rejects(hooked, "backward hooks");
    // requires_grad that is not a bool.
    let bad_rg = Pk::new()
        .ordered_dict()
        .mark()
        .s("w")
        .global("torch._utils", "_rebuild_tensor_v2")
        .mark()
        .storage("FloatStorage", "0", 6)
        .int(0)
        .ints(&[2, 3])
        .ints(&[3, 1])
        .int(1)
        .ordered_dict()
        .raw(b"tRu.")
        .done();
    rejects(bad_rg, "requires_grad");
    // A tensor over something that is not a persistent storage; a persistent id of another kind or shape.
    rejects(Pk::new().global("torch._utils", "_rebuild_tensor_v2").mark().int(5).int(0).ints(&[1]).ints(&[1]).raw(&[0x89]).ordered_dict().raw(b"tR.").done(), "not a persistent storage");
    rejects(Pk::new().mark().s("module").int(1).raw(b"tQ.").done(), "not (storage");
    rejects(Pk::new().mark().s("other").global("torch", "FloatStorage").s("0").s("cpu").int(1).raw(b"tQ.").done(), "other than `storage`");
    // BUILD of anything but `_metadata`.
    rejects(Pk::new().ordered_dict().raw(b"}").s("x").int(1).raw(b"sb.").done(), "BUILD");
    // The top-level object must be a flat dict of tensors.
    rejects(Pk::new().raw(b"].").done(), "not a state dict");
    rejects(Pk::new().ordered_dict().mark().s("w").int(1).raw(b"u.").done(), "not a tensor");
    // A storage declared twice with different sizes.
    let twice = Pk::new()
        .ordered_dict()
        .mark()
        .s("a")
        .raw(&Pk::default().tensor("FloatStorage", "0", 6, 0, &[6], &[1]).0)
        .s("b")
        .raw(&Pk::default().tensor("FloatStorage", "0", 8, 0, &[8], &[1]).0)
        .raw(b"u.")
        .done();
    rejects(twice, "declared twice");
}

// ───────────────────────────────── hostile archives ─────────────────────────────────

/// A ZIP archive of stored members (CRCs are not read by the reader, and are zero here). `method` and `flags` go into every entry.
fn zip(members: &[(&str, Vec<u8>)], method: u16, flags: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dir = Vec::new();
    for (name, data) in members {
        let off = out.len() as u32;
        out.extend(b"PK\x03\x04");
        out.extend([20u8, 0]);
        out.extend(flags.to_le_bytes());
        out.extend(method.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend([0u8; 4]);
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0u8, 0]);
        out.extend(name.as_bytes());
        out.extend(data);
        dir.extend(b"PK\x01\x02");
        dir.extend([20u8, 0, 20, 0]);
        dir.extend(flags.to_le_bytes());
        dir.extend(method.to_le_bytes());
        dir.extend([0u8; 4]);
        dir.extend([0u8; 4]);
        dir.extend((data.len() as u32).to_le_bytes());
        dir.extend((data.len() as u32).to_le_bytes());
        dir.extend((name.len() as u16).to_le_bytes());
        dir.extend([0u8; 2 + 2 + 2 + 2 + 4]);
        dir.extend(off.to_le_bytes());
        dir.extend(name.as_bytes());
    }
    let cd_off = out.len() as u32;
    out.extend(&dir);
    out.extend(b"PK\x05\x06");
    out.extend([0u8; 4]);
    out.extend((members.len() as u16).to_le_bytes());
    out.extend((members.len() as u16).to_le_bytes());
    out.extend((dir.len() as u32).to_le_bytes());
    out.extend(cd_off.to_le_bytes());
    out.extend([0u8; 2]);
    out
}

fn write(tag: &str, bytes: &[u8]) -> PathBuf {
    let d = scratch(tag);
    let p = d.join("m.bin");
    std::fs::write(&p, bytes).unwrap();
    p
}

fn tiny_checkpoint(pickle: Vec<u8>, storage: Vec<u8>) -> Vec<(&'static str, Vec<u8>)> {
    vec![("a/data.pkl", pickle), ("a/data/0", storage)]
}

#[test]
fn the_archive_the_assembler_and_zip_writer_make_reads_and_each_lie_about_it_is_refused() {
    let pk = Pk::new().one("w", Pk::default().tensor("FloatStorage", "0", 6, 0, &[2, 3], &[3, 1])).done();
    let data: Vec<u8> = (0..6).flat_map(|i| (i as f32).to_le_bytes()).collect();
    let good = zip(&tiny_checkpoint(pk.clone(), data.clone()), 0, 0);
    let p = write("good", &good);
    let h = read_header(&p).expect("a good archive reads");
    assert_eq!(h.entries["w"].shape, vec![2, 3]);
    let f = SafetensorsFile::open(&p).unwrap();
    assert_eq!(f.read("w").unwrap().data, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);

    // Not an archive; too short; a damaged directory; a compressed member; an encrypted member.
    refused(read_header(&write("short", b"PK")), "not a ZIP");
    refused(read_header(&write("txt", &b"hello world, this is not an archive at all".repeat(2))), "not a ZIP");
    let mut cut = good.clone();
    cut.truncate(good.len() - 30);
    refused(read_header(&write("cut", &cut)), "end-of-central-directory");
    refused(read_header(&write("deflate", &zip(&tiny_checkpoint(pk.clone(), data.clone()), 8, 0))), "compressed");
    refused(read_header(&write("crypt", &zip(&tiny_checkpoint(pk.clone(), data.clone()), 0, 1))), "encrypted");
    // No pickle; two pickles.
    refused(read_header(&write("nopkl", &zip(&[("a/other", vec![1])], 0, 0))), "no data.pkl");
    refused(read_header(&write("twopkl", &zip(&[("a/data.pkl", pk.clone()), ("b/data.pkl", pk.clone())], 0, 0))), "more than one data.pkl");
    // The pickle names a storage the archive lacks; the storage is the wrong size for its declaration; the tensor outruns it.
    refused(read_header(&write("nostorage", &zip(&[("a/data.pkl", pk.clone())], 0, 0))), "no member");
    refused(read_header(&write("size", &zip(&tiny_checkpoint(pk.clone(), data[..20].to_vec()), 0, 0))), "declares");
    let outrun = Pk::new().one("w", Pk::default().tensor("FloatStorage", "0", 6, 3, &[2, 3], &[3, 1])).done();
    refused(read_header(&write("outrun", &zip(&tiny_checkpoint(outrun, data.clone()), 0, 0))), "run past its storage");
    // Big-endian: refused.
    let be = zip(&[("a/data.pkl", pk.clone()), ("a/data/0", data.clone()), ("a/byteorder", b"big".to_vec())], 0, 0);
    refused(read_header(&write("big", &be)), "little-endian");
    // A central directory whose entry count is a lie.
    let mut lie = good.clone();
    let n = lie.len();
    lie[n - 10] = 0xFF; // the directory's size
    lie[n - 9] = 0xFF;
    refused(read_header(&write("lie", &lie)), "central directory");
    // An entry count that claims ZIP64 without the locator that would give the real one.
    let mut z64 = good.clone();
    z64[n - 12] = 0xFF;
    z64[n - 11] = 0xFF;
    refused(read_header(&write("z64", &z64)), "ZIP64");
    // The local header and the directory disagree on the name.
    let mut swap = good.clone();
    let at = swap.windows(8).position(|w| w == b"a/data.p").unwrap();
    swap[at + 2] = b'X';
    refused(read_header(&write("swap", &swap)), "disagree");
    // A stored member of an archive built from an assembler pickle that WOULD run code: the marker is never written.
    let dir = scratch("marker");
    let marker = dir.join("pwned");
    let evil = Pk::new()
        .global("os", "system")
        .mark()
        .s(&format!("touch {}", marker.display()))
        .raw(b"tR.")
        .done();
    let p = dir.join("evil.bin");
    std::fs::write(&p, zip(&tiny_checkpoint(evil, data), 0, 0)).unwrap();
    refused(read_header(&p), "`os.system`");
    refused(Checkpoint::open(&p).map(|_| ()), "`os.system`");
    assert!(!marker.exists(), "the pickle ran");
}

#[test]
fn a_zip64_archive_reads_the_same() {
    // The same checkpoint with its central directory entry and end record in ZIP64 form (sizes and offset in the extra field).
    let pk = Pk::new().one("w", Pk::default().tensor("FloatStorage", "0", 6, 0, &[2, 3], &[3, 1])).done();
    let data: Vec<u8> = (0..6).flat_map(|i| (i as f32 * 2.0).to_le_bytes()).collect();
    let mut out = Vec::new();
    let mut dir = Vec::new();
    for (name, d) in [("a/data.pkl", pk), ("a/data/0", data)] {
        let off = out.len() as u64;
        out.extend(b"PK\x03\x04");
        out.extend([45u8, 0, 0, 0, 0, 0]);
        out.extend([0u8; 8]);
        out.extend((d.len() as u32).to_le_bytes());
        out.extend((d.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0u8, 0]);
        out.extend(name.as_bytes());
        out.extend(&d);
        dir.extend(b"PK\x01\x02");
        dir.extend([45u8, 0, 45, 0, 0, 0, 0, 0]);
        dir.extend([0u8; 4]);
        dir.extend([0u8; 4]);
        dir.extend(0xFFFF_FFFFu32.to_le_bytes()); // compressed: in the extra field
        dir.extend(0xFFFF_FFFFu32.to_le_bytes()); // uncompressed
        dir.extend((name.len() as u16).to_le_bytes());
        dir.extend(28u16.to_le_bytes()); // extra: id, size, 3 × u64
        dir.extend([0u8; 2 + 2 + 2 + 4]);
        dir.extend(0xFFFF_FFFFu32.to_le_bytes()); // local header offset
        dir.extend(name.as_bytes());
        dir.extend(1u16.to_le_bytes());
        dir.extend(24u16.to_le_bytes());
        dir.extend((d.len() as u64).to_le_bytes());
        dir.extend((d.len() as u64).to_le_bytes());
        dir.extend(off.to_le_bytes());
    }
    let cd_off = out.len() as u64;
    out.extend(&dir);
    let eocd64 = out.len() as u64;
    out.extend(b"PK\x06\x06");
    out.extend(44u64.to_le_bytes());
    out.extend([45u8, 0, 45, 0]);
    out.extend([0u8; 8]);
    out.extend(2u64.to_le_bytes());
    out.extend(2u64.to_le_bytes());
    out.extend((dir.len() as u64).to_le_bytes());
    out.extend(cd_off.to_le_bytes());
    out.extend(b"PK\x06\x07");
    out.extend([0u8; 4]);
    out.extend(eocd64.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend(b"PK\x05\x06");
    out.extend([0u8; 4]);
    out.extend(0xFFFFu16.to_le_bytes());
    out.extend(0xFFFFu16.to_le_bytes());
    out.extend(0xFFFF_FFFFu32.to_le_bytes());
    out.extend(0xFFFF_FFFFu32.to_le_bytes());
    out.extend([0u8; 2]);
    let p = write("zip64", &out);
    let f = SafetensorsFile::open(&p).expect("zip64");
    assert_eq!(f.read("w").unwrap().data, vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
}

#[test]
fn the_extension_decides_which_reader_opens_a_file() {
    assert!(torchzip::is_torch_path(Path::new("pytorch_model.bin")));
    assert!(torchzip::is_torch_path(Path::new("adapter_model.bin")));
    assert!(torchzip::is_torch_path(Path::new("x.pt")) && torchzip::is_torch_path(Path::new("x.pth")));
    assert!(!torchzip::is_torch_path(Path::new("model.safetensors")) && !torchzip::is_torch_path(Path::new("m.gguf")));
}

/// Whatever the bytes, the reader returns — a value or an error — and never panics, hangs or allocates what a prefix claims: 4,000
/// deterministic mutants of the pickle (byte flips, truncations, insertions) and 1,500 of the whole archive.
#[test]
fn mutated_pickles_and_archives_never_panic() {
    let src = std::fs::read(fixture("torch/flat.bin")).unwrap();
    let h = read_header(&fixture("torch/flat.bin")).unwrap();
    // The pickle is the first member: its bytes follow the first local header (30 + the name 10 + the extra field).
    // torch's writer leaves the local header's sizes at 0 (a data descriptor follows); the pickle begins after the header, and the
    // interpreter stops at its own STOP, so the bytes after it are only more material to mutate.
    let start = src.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
    let nl = u16::from_le_bytes([src[start + 26], src[start + 27]]) as usize;
    let el = u16::from_le_bytes([src[start + 28], src[start + 29]]) as usize;
    let from = start + 30 + nl + el;
    let pkl = src[from..from + 700.min(src.len() - from)].to_vec();
    assert_eq!(pkl[0], 0x80);
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let lim = Limits { max_ops: 100_000, max_stack: 10_000, max_memo: 10_000, ..Limits::default() };
    let mut accepted = 0;
    for _ in 0..4000 {
        let mut m = pkl.clone();
        match next() % 4 {
            0 => {
                let i = (next() as usize) % m.len();
                m[i] = next() as u8;
            }
            1 => m.truncate((next() as usize) % m.len()),
            2 => {
                let i = (next() as usize) % m.len();
                m.insert(i, next() as u8);
            }
            _ => {
                for _ in 0..4 {
                    let i = (next() as usize) % m.len();
                    m[i] ^= 1 << (next() % 8);
                }
            }
        }
        if interpret_pickle(&m, &lim).is_ok() {
            accepted += 1;
        }
    }
    assert!(accepted < 4000, "the mutants include refusals");
    let dir = scratch("fuzz");
    for k in 0..1500 {
        let mut m = src.clone();
        for _ in 0..(1 + next() % 3) {
            let i = (next() as usize) % m.len();
            m[i] = next() as u8;
        }
        if k % 7 == 0 {
            m.truncate(m.len() - (next() as usize) % 64);
        }
        let p = dir.join("m.bin");
        std::fs::write(&p, &m).unwrap();
        let _ = read_header(&p);
    }
    assert!(h.entries.len() > 3);
    let _ = std::fs::remove_dir_all(dir);
}
