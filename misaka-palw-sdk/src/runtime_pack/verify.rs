//! **`palw-class pack verify`**: is this pack true, and does this artifact follow from it?
//!
//! Every claim of the manifest that can be checked from what is at hand is checked, and each check
//! reports `PASS`, `FAIL` or `SKIPPED` (not checkable here, and why): the manifest and its sidecars
//! by hash; the public source files by hash; the adapter and the quant descriptors by hash against this
//! build's and the pack's own files; the frontend (the spec's digest, the features, the scope) by reading
//! the model again; the artifact — rebuilt from the source with the pack's profile and statistics, or the
//! one supplied — by file digest, inventory root, graph root and tokenizer; the declared classes by class
//! id; the conformance vectors on every executor; and the program against the Hugging Face reference
//! within the pack's tolerance. A pack is VERIFIED only when nothing failed and nothing was skipped.

use super::conformance::{self, ConformanceJob, ImplSet};
use super::hfref::{self, HfReference};
use super::manifest::*;
use crate::tir_manifest::PalwTirManifestV1;
use misaka_palw_tir_lower::adapter;
use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest, convert_model};
use misaka_palw_tir_lower::detmath::MathMode;
use misaka_palw_tir_lower::quantfmt::QuantFormat;
use std::path::{Path, PathBuf};

/// Distinct scratch names for verifications running side by side in one process.
static UNIQUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct VerifyReport {
    pub pack_digest: String,
    pub checks: Vec<Check>,
}

impl VerifyReport {
    /// Nothing failed.
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.status != Status::Fail)
    }
    /// Nothing failed and nothing was left unchecked.
    pub fn verified(&self) -> bool {
        self.checks.iter().all(|c| c.status == Status::Pass)
    }
    pub fn render(&self) -> String {
        let mut o = format!("pack {}\n", self.pack_digest);
        for c in &self.checks {
            let s = match c.status {
                Status::Pass => "PASS   ",
                Status::Fail => "FAIL   ",
                Status::Skipped => "SKIPPED",
            };
            o.push_str(&format!("  {s} {:<18} {}\n", c.name, c.detail));
        }
        o.push_str(if self.verified() {
            "VERIFIED\n"
        } else if self.ok() {
            "NOT FULLY VERIFIED (nothing failed; some checks were skipped)\n"
        } else {
            "FAILED\n"
        });
        o
    }
}

#[derive(Clone, Debug)]
pub struct VerifyOpts {
    pub pack_dir: PathBuf,
    /// The public source: enables the source hash check, the frontend check and the rebuild.
    pub model: Option<PathBuf>,
    /// An artifact to check instead of (or without) a rebuild.
    pub artifact: Option<PathBuf>,
    /// Rebuild the artifact from `model` with the pack's profile.
    pub rebuild: bool,
    pub impls: ImplSet,
    /// Check the exact declared identities (needs the artifact); no fresh layout search.
    pub declared: bool,
    /// **Streamed conformance** (RFC-0002 Part II §II.9 L2): the reference reads each tensor when asked, the typed backend runs over the
    /// mapped file, and the Hugging Face fit runs on the mapped backend — no executor holds the artifact whole. `None`: automatically,
    /// for an artifact over [`conformance::STREAM_ABOVE_BYTES_V1`].
    pub streamed: Option<bool>,
}

impl VerifyOpts {
    pub fn new(pack_dir: impl Into<PathBuf>) -> Self {
        VerifyOpts {
            pack_dir: pack_dir.into(),
            model: None,
            artifact: None,
            rebuild: false,
            impls: ImplSet::default(),
            declared: true,
            streamed: None,
        }
    }
}

struct Acc(Vec<Check>);
impl Acc {
    fn add(&mut self, name: &str, status: Status, detail: impl Into<String>) {
        self.0.push(Check { name: name.into(), status, detail: detail.into() });
    }
    fn pass(&mut self, name: &str, detail: impl Into<String>) {
        self.add(name, Status::Pass, detail)
    }
    fn fail(&mut self, name: &str, detail: impl Into<String>) {
        self.add(name, Status::Fail, detail)
    }
    fn skip(&mut self, name: &str, detail: impl Into<String>) {
        self.add(name, Status::Skipped, detail)
    }
}

fn cmp(acc: &mut Acc, name: &str, what: &str, have: &str, want: &str) -> bool {
    if have == want {
        true
    } else {
        acc.fail(name, format!("{what}: the pack says {}, this is {}", short(want), short(have)));
        false
    }
}

fn short(s: &str) -> String {
    if s.len() > 24 { format!("{}…", &s[..24]) } else { s.to_string() }
}

/// **Verify a pack.**
pub fn verify(opts: &VerifyOpts, log: &dyn Fn(String)) -> Result<VerifyReport, String> {
    let dir = &opts.pack_dir;
    let text = std::fs::read_to_string(dir.join(PACK_FILE)).map_err(|e| format!("{}: {e}", dir.join(PACK_FILE).display()))?;
    let mut acc = Acc(Vec::new());
    let pack = match RuntimePackV1::parse(&text) {
        Ok(p) => p,
        Err(e) => {
            acc.fail("manifest", e);
            return Ok(VerifyReport { pack_digest: String::new(), checks: acc.0 });
        }
    };
    let digest = pack.digest();
    acc.pass("manifest", format!("{} ({}), schema {}", pack.name, &digest[..16], pack.schema));

    // Sidecars by hash.
    let mut bad = Vec::new();
    for f in &pack.files {
        match std::fs::read(dir.join(&f.path)) {
            Ok(b) if b.len() as u64 == f.bytes && blake2b256_hex(&b) == f.blake2b256 => {}
            Ok(_) => bad.push(format!("{} differs", f.path)),
            Err(_) => bad.push(format!("{} is missing", f.path)),
        }
    }
    if bad.is_empty() {
        acc.pass("sidecars", format!("{} file(s) match their hashes", pack.files.len()));
    } else {
        acc.fail("sidecars", bad.join("; "));
    }

    // The adapter and the descriptors, against this build and the pack's own files.
    adapter_check(&mut acc, &pack, dir);
    let supplied = descriptor_check(&mut acc, &pack, dir);

    // The converter.
    let this_platform = misaka_palw_tir_lower::detmath::platform();
    let math_note = match (pack.converter.math.mode.as_str(), pack.converter.math.platform.as_deref()) {
        ("libm-v1", _) => "math libm-v1: the same bytes on every platform".to_string(),
        (_, Some(p)) if p == this_platform => format!("math std on {p}: this platform rebuilds it"),
        (_, p) => format!("math std built on {}: only that platform rebuilds it (this is {this_platform})", p.unwrap_or("?")),
    };
    let rebuildable = pack.converter.math.mode == "libm-v1" || pack.converter.math.platform.as_deref() == Some(this_platform.as_str());
    let same_build = pack.converter.crate_version == env!("CARGO_PKG_VERSION") && pack.converter.lowering == LOWERING_VERSION_V1;
    acc.pass(
        "converter",
        format!(
            "built by {} {} ({}); this build {} ({}) — {}; {}",
            pack.converter.name,
            pack.converter.crate_version,
            pack.converter.lowering,
            env!("CARGO_PKG_VERSION"),
            LOWERING_VERSION_V1,
            if same_build {
                "the same converter"
            } else {
                "another converter: the artifact's roots, not the version, are what must agree"
            },
            math_note
        ),
    );

    // The source and the frontend.
    let mut artifact: Option<PathBuf> = opts.artifact.clone();
    let mut cleanup: Option<PathBuf> = None;
    match &opts.model {
        None => {
            acc.skip("source", "no --model: the source files are not hashed");
            acc.skip("frontend", "no --model: the frontend is not run");
        }
        Some(model) => {
            source_check(&mut acc, &pack, model);
            frontend_check(&mut acc, &pack, model, dir, &supplied);
        }
    }

    // The artifact: rebuilt, or the one supplied.
    if opts.rebuild {
        match (&opts.model, rebuildable) {
            (None, _) => acc.skip("rebuild", "--rebuild needs --model (the public source)"),
            (Some(_), false) => acc.skip("rebuild", format!("not attempted: {math_note}")),
            (Some(model), true) => {
                let out_dir = std::env::temp_dir().join(format!(
                    "palw-pack-verify-{}-{}",
                    std::process::id(),
                    UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
                let out = out_dir.join("rebuild.palwtir");
                let mut req = ConvertRequest::new(model, &out);
                req.stats_in = Some(dir.join(&pack.profile.calibration.stats_file));
                // A recurrent artifact records the longest calibration sequence (`calibrated_context`); the pack carries the
                // sequences beside the statistics, so the rebuild records the same value the build did.
                req.calibrated_context = pack
                    .profile
                    .calibration
                    .sequences_file
                    .as_ref()
                    .and_then(|f| std::fs::read(dir.join(f)).ok())
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .and_then(|v| v["sequences"].as_array().and_then(|a| a.iter().filter_map(|q| q.as_array().map(Vec::len)).max()))
                    .filter(|n| *n > 0);
                req.policy = misaka_palw_tir_lower::quant::QuantPolicy {
                    headroom16: pack.profile.policy.headroom16,
                    headroom32: pack.profile.policy.headroom32,
                    headroom_resid: pack.profile.policy.headroom_resid,
                };
                req.max_window = pack.profile.max_window;
                req.context = pack.profile.context;
                req.math = MathMode::parse(&pack.converter.math.mode).ok_or("the pack's math mode")?;
                req.quant_formats = supplied.clone();
                req.adapter = pack.frontend.adapter.file.as_ref().map(|f| dir.join(f));
                req.chunk_store = Some(out_dir.join("chunks"));
                match convert_model(&req, log) {
                    Ok(_) => {
                        artifact = Some(out.clone());
                        cleanup = Some(out_dir);
                    }
                    Err(e) => {
                        acc.fail("rebuild", format!("the conversion failed: {e}"));
                        let _ = std::fs::remove_dir_all(out_dir);
                    }
                }
            }
        }
    }
    let Some(art) = artifact else {
        for n in ["artifact", "declared", "conformance", "hf-reference"] {
            acc.skip(n, "no artifact: pass --artifact <file>, or --model <dir> --rebuild");
        }
        return Ok(VerifyReport { pack_digest: digest, checks: acc.0 });
    };

    artifact_check(&mut acc, &pack, &art);
    if opts.declared && !pack.declared.is_empty() {
        declared_check(&mut acc, &pack, &art);
    } else if !pack.declared.is_empty() {
        acc.skip("declared", "not asked");
    }
    let art_bytes = std::fs::metadata(&art).map(|m| m.len()).unwrap_or(0);
    let streamed = opts.streamed.unwrap_or(art_bytes > conformance::STREAM_ABOVE_BYTES_V1);
    if streamed {
        // Neither the reference nor the typed backend holds the artifact whole: the reference reads one tensor at a time, the typed backend
        // runs over the mapping, and the fit to the Hugging Face reference is measured on the mapped backend.
        conformance_check_streamed(&mut acc, &pack, &art, opts.impls, log);
        hf_check_streamed(&mut acc, &pack, &art, dir);
    } else {
        let loaded = match conformance::LoadedArtifact::open(&art) {
            Ok(l) => Some(l),
            Err(e) => {
                acc.fail("conformance", format!("the artifact does not load: {e}"));
                None
            }
        };
        if let Some(loaded) = &loaded {
            conformance_check(&mut acc, &pack, loaded, opts.impls);
            hf_check(&mut acc, &pack, loaded, dir);
        }
    }
    if let Some(d) = cleanup {
        let _ = std::fs::remove_dir_all(d);
    }
    Ok(VerifyReport { pack_digest: digest, checks: acc.0 })
}

fn adapter_check(acc: &mut Acc, pack: &RuntimePackV1, dir: &Path) {
    let check_pin = |acc: &mut Acc, what: &str, p: &AdapterPin| match p.kind.as_str() {
        "none" => acc.pass(what, "none (Level A)"),
        "built-in" => match (p.id.as_deref().and_then(adapter::builtin::by_id), &p.hash) {
            (Some(a), Some(h)) if a.hash == *h => acc.pass(what, format!("built-in `{}` {}", a.id, &a.hash[..16])),
            (Some(a), Some(h)) => acc.fail(
                what,
                format!(
                    "built-in `{}`: the pack pins {}, this build's is {} (the adapter changed since the pack was written)",
                    a.id,
                    short(h),
                    short(&a.hash)
                ),
            ),
            _ => acc.fail(what, "this build has no such built-in adapter"),
        },
        "user-file" => match (&p.file, &p.hash) {
            (Some(f), Some(h)) => match std::fs::read_to_string(dir.join(f))
                .map_err(|e| e.to_string())
                .and_then(|t| adapter::parse(&t, adapter::Origin::User).map_err(|e| e.to_string()))
            {
                Ok(a) if a.hash == *h => acc.pass(what, format!("supplied `{}` {}", a.id, &a.hash[..16])),
                Ok(a) => acc.fail(what, format!("the supplied file hashes to {}, the pack pins {}", short(&a.hash), short(h))),
                Err(e) => acc.fail(what, format!("the supplied adapter does not parse: {e}")),
            },
            _ => acc.fail(what, "a user adapter without a file and a hash"),
        },
        other => acc.fail(what, format!("adapter kind `{other}`")),
    };
    check_pin(acc, "adapter", &pack.frontend.adapter);
    if let Some(t) = &pack.frontend.template {
        check_pin(acc, "template", t);
    }
}

/// The supplied descriptor files (verified), for the rebuild.
fn descriptor_check(acc: &mut Acc, pack: &RuntimePackV1, dir: &Path) -> Vec<PathBuf> {
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let (mut ok, mut bad, mut files) = (0, Vec::new(), Vec::new());
    for d in &pack.quant.descriptors {
        match d.source.as_str() {
            "built-in" => match reg.named(&d.name) {
                Some(f) if f.digest_hex() == d.digest => ok += 1,
                Some(f) => bad.push(format!("{}: pinned {}, this build's is {}", d.name, short(&d.digest), short(&f.digest_hex()))),
                None => bad.push(format!("{}: not built in here", d.name)),
            },
            "file" => match d.file.as_ref().map(|f| dir.join(f)) {
                Some(p) => match std::fs::read_to_string(&p)
                    .map_err(|e| e.to_string())
                    .and_then(|t| QuantFormat::from_json(&t).map_err(|e| e.to_string()))
                {
                    Ok(f) if f.digest_hex() == d.digest => {
                        ok += 1;
                        files.push(p);
                    }
                    Ok(f) => {
                        bad.push(format!("{}: the file's digest is {}, pinned {}", d.name, short(&f.digest_hex()), short(&d.digest)))
                    }
                    Err(e) => bad.push(format!("{}: {e}", d.name)),
                },
                None => bad.push(format!("{}: a file descriptor without a file", d.name)),
            },
            other => bad.push(format!("{}: source `{other}`", d.name)),
        }
    }
    if bad.is_empty() {
        acc.pass("descriptors", if ok == 0 { "none (a float checkpoint)".to_string() } else { format!("{ok} pinned, all equal") });
    } else {
        acc.fail("descriptors", bad.join("; "));
    }
    files
}

fn source_check(acc: &mut Acc, pack: &RuntimePackV1, model: &Path) {
    let sdir = super::build::source_dir(model);
    let mut bad = Vec::new();
    for f in &pack.model.files {
        match super::build::sha256_file(&sdir.join(&f.path)) {
            Ok((n, sha)) if n == f.bytes && sha == f.sha256 => {}
            Ok((n, sha)) => {
                bad.push(format!("{}: {} bytes {} (pinned {} bytes {})", f.path, n, short(&sha), f.bytes, short(&f.sha256)))
            }
            Err(e) => bad.push(e),
        }
    }
    // A weight file the pack does not list would change the artifact.
    if let Ok(now) = super::build::source_files(model) {
        for n in now {
            if !pack.model.files.iter().any(|f| f.path == n) {
                bad.push(format!("{n} is in the source but not in the pack"));
            }
        }
    }
    if bad.is_empty() {
        acc.pass("source", format!("{} file(s) match their SHA-256", pack.model.files.len()));
    } else {
        acc.fail("source", bad.join("; "));
    }
}

/// Read the model again through the frontend: the spec, the features and the scope must be the pack's.
fn frontend_check(acc: &mut Acc, pack: &RuntimePackV1, model: &Path, dir: &Path, supplied: &[PathBuf]) -> Option<()> {
    let reg = match misaka_palw_tir_lower::quantfmt::QuantRegistry::with_files(supplied) {
        Ok(r) => r,
        Err(e) => {
            acc.fail("frontend", format!("the supplied descriptors do not load: {e}"));
            return None;
        }
    };
    let read = match pack.frontend.adapter.file.as_ref().map(|f| std::fs::read_to_string(dir.join(f))) {
        Some(Ok(t)) => {
            misaka_palw_tir_lower::hf_schema::ReadOptions { adapter: misaka_palw_tir_lower::hf_schema::AdapterChoice::Text(t) }
        }
        Some(Err(e)) => {
            acc.fail("frontend", format!("the supplied adapter: {e}"));
            return None;
        }
        None => misaka_palw_tir_lower::hf_schema::ReadOptions::default(),
    };
    let opts = misaka_palw_tir_lower::lower::LowerOpts { max_window: pack.profile.max_window, ..Default::default() };
    if let Some(m) = MathMode::parse(&pack.converter.math.mode) {
        misaka_palw_tir_lower::detmath::set_mode(m);
    }
    let o = match misaka_palw_tir_lower::fidelity::open_model_full(model, &opts, &reg, &read) {
        Ok(o) => o,
        Err(e) => {
            acc.fail("frontend", format!("the model is not read: {e}"));
            return None;
        }
    };
    let f = &o.frontend;
    let mut ok = cmp(acc, "frontend", "spec digest", &f.spec_digest, &pack.frontend.spec_digest);
    ok &= cmp(acc, "frontend", "level", &f.level.to_string(), &pack.frontend.level);
    ok &= cmp(
        acc,
        "frontend",
        "configuration digest",
        &blake2b256_hex(adapter::canonical_json(&f.config).as_bytes()),
        &pack.model.config_digest,
    );
    let uses: Vec<serde_json::Value> =
        o.prepared.spec.features().iter().map(|u| serde_json::to_value(u).unwrap_or_default()).collect();
    if uses != pack.features.used {
        acc.fail("frontend", "the features the spec uses differ from the pack's");
        ok = false;
    }
    let idx = if o.gguf {
        misaka_palw_tir_lower::hf_schema::TensorIndex::from_source(o.source.as_ref())
    } else {
        misaka_palw_tir_lower::hf_schema::TensorIndex::from_checkpoint_path(model).unwrap_or_default()
    };
    let scope = misaka_palw_tir_lower::model::scope_of(
        &f.config,
        Some(&o.prepared.spec),
        Some(&idx),
        &misaka_palw_tir_lower::model::sibling_files(model),
    );
    if serde_json::to_value(&scope).unwrap_or_default() != pack.features.scope {
        acc.fail("frontend", "the feature scope differs from the pack's");
        ok = false;
    }
    if ok {
        acc.pass(
            "frontend",
            format!("level {}, spec {}, {} feature uses, scope: {}", f.level, &f.spec_digest[..16], uses.len(), scope.headline()),
        );
    }
    if misaka_palw_tir_lower::model::registry_digest() != pack.features.registry_digest {
        acc.skip(
            "feature-registry",
            "this build's feature vocabulary differs from the pack's (feature ids are versioned: the uses above still agree)",
        );
    }
    Some(())
}

fn artifact_check(acc: &mut Acc, pack: &RuntimePackV1, art: &Path) {
    let m = match PalwTirManifestV1::derive_streamed(art) {
        Ok(m) => m,
        Err(e) => {
            acc.fail("artifact", e);
            return;
        }
    };
    let r = &pack.result;
    let mut ok = true;
    // The file may be the pack's artifact, or that artifact with a layout declared (the class a registration carries: its
    // container differs by the layout and the logits scheme, so its file digest and graph root are the declared class's, which the
    // pack pins in `declared`). The inventory root and the tokenizer are the same in both.
    let digest = hex(&m.artifact_digest);
    if let Some(d) = pack.declared.iter().find(|d| d.file_digest == digest && digest != r.artifact_digest) {
        ok &= cmp(acc, "artifact", "inventory root", &m.inventory_root.to_string(), &r.inventory_root);
        ok &= cmp(acc, "artifact", "tokenizer id", &hex(&m.tokenizer_id), &r.tokenizer_id);
        ok &= cmp(acc, "artifact", "class id", &m.class_id.map(|h| h.to_string()).unwrap_or_default(), &d.class_id);
        ok &= cmp(acc, "artifact", "layout digest", &m.layout_digest.map(|h| h.to_string()).unwrap_or_default(), &d.layout_digest);
        if ok {
            acc.pass(
                "artifact",
                format!(
                    "the artifact declared for {} (class {}): file {} · inventory root {}",
                    d.network,
                    &d.class_id[..16],
                    &digest[..16],
                    &r.inventory_root[..16]
                ),
            );
        }
        return;
    }
    ok &= cmp(acc, "artifact", "file digest", &digest, &r.artifact_digest);
    ok &= cmp(acc, "artifact", "inventory root", &m.inventory_root.to_string(), &r.inventory_root);
    ok &= cmp(acc, "artifact", "graph root", &m.graph_ir_root.to_string(), &r.graph_ir_root);
    ok &= cmp(acc, "artifact", "tokenizer id", &hex(&m.tokenizer_id), &r.tokenizer_id);
    if ok {
        acc.pass(
            "artifact",
            format!(
                "file {} · inventory root {} · graph root {}",
                &r.artifact_digest[..16],
                &r.inventory_root[..16],
                &r.graph_ir_root[..16]
            ),
        );
    }
}

fn declared_check(acc: &mut Acc, pack: &RuntimePackV1, art: &Path) {
    let artifact = match misaka_palw_tir_exec::node::TirArtifactV1::open(art) {
        Ok(a) => a,
        Err(e) => {
            acc.fail("declared", e);
            return;
        }
    };
    let root = match pack.result.inventory_root.parse::<kaspa_hashes::Hash64>() {
        Ok(r) => r,
        Err(e) => {
            acc.fail("declared", format!("inventory root: {e:?}"));
            return;
        }
    };
    for d in &pack.declared {
        if d.exact_layout.is_none() {
            acc.skip(
                "declared",
                format!("{}: legacy pack has no exact layout; bind the declared artifact into a new pack", d.network),
            );
            continue;
        }
        match d.class_from_program(&artifact.container().program, kaspa_hashes::Hash64::from_bytes(artifact.header().tokenizer_id)) {
            Ok(class) => {
                let ok = cmp(acc, "declared", "class id", &class.class_id(&root).to_string(), &d.class_id)
                    & cmp(acc, "declared", "layout digest", &class.layout_digest().to_string(), &d.layout_digest);
                if ok {
                    acc.pass(
                        "declared",
                        format!(
                            "{}: exact class {} (context {}, interval {}); identity verification, not live admission",
                            d.network,
                            &d.class_id[..16],
                            d.max_context,
                            d.checkpoint_interval
                        ),
                    );
                }
            }
            Err(e) => acc.fail("declared", e),
        }
    }
}

fn conformance_check(acc: &mut Acc, pack: &RuntimePackV1, a: &conformance::LoadedArtifact, impls: ImplSet) {
    let jobs: Vec<ConformanceJob> = pack
        .conformance
        .vectors
        .iter()
        .map(|v| ConformanceJob { label: v.label.clone(), prompt: v.prompt.clone(), decode: v.decode })
        .collect();
    match conformance::run(a, &jobs, impls, &|_| {}) {
        Err(e) => acc.fail("conformance", e),
        Ok(got) => {
            let bad: Vec<String> = got
                .iter()
                .zip(&pack.conformance.vectors)
                .filter(|(g, w)| g != w)
                .map(|(g, w)| {
                    if g.tokens != w.tokens {
                        format!("{}: decoded {:?}, the pack says {:?}", g.label, g.tokens, w.tokens)
                    } else if g.logits_digest != w.logits_digest {
                        format!("{}: the logits differ", g.label)
                    } else {
                        format!("{}: the commit points differ", g.label)
                    }
                })
                .collect();
            if bad.is_empty() {
                let names: Vec<String> = impls.records().into_iter().map(|r| r.name).collect();
                acc.pass(
                    "conformance",
                    format!(
                        "{} vector(s), {} positions, equal on {}",
                        got.len(),
                        got.iter().map(|v| v.positions).sum::<usize>(),
                        names.join(", ")
                    ),
                );
            } else {
                acc.fail("conformance", bad.join("; "));
            }
        }
    }
}

/// [`conformance_check`] streamed (L2): the same vectors, the same digests, the artifact never held whole.
fn conformance_check_streamed(acc: &mut Acc, pack: &RuntimePackV1, art: &Path, impls: ImplSet, log: &dyn Fn(String)) {
    let jobs: Vec<ConformanceJob> = pack
        .conformance
        .vectors
        .iter()
        .map(|v| ConformanceJob { label: v.label.clone(), prompt: v.prompt.clone(), decode: v.decode })
        .collect();
    log(format!(
        "streamed conformance: {} vectors, {} positions; checking every requested executor",
        jobs.len(),
        jobs.iter().map(|j| j.prompt.len() + j.decode).sum::<usize>()
    ));
    let position = |ji: usize, pos: usize| {
        log(format!(
            "conformance vector {}/{} ({}): position {}/{} computed; final conformance-vector comparison pending",
            ji + 1,
            jobs.len(),
            jobs[ji].label,
            pos + 1,
            jobs[ji].prompt.len() + jobs[ji].decode
        ));
    };
    match conformance::run_streamed_with_progress(art, &jobs, impls, &|_| {}, &position) {
        Err(e) => acc.fail("conformance", e),
        Ok((got, note)) => {
            let bad: Vec<String> = got
                .iter()
                .zip(&pack.conformance.vectors)
                .filter(|(g, w)| g != w)
                .map(|(g, w)| {
                    if g.tokens != w.tokens {
                        format!("{}: decoded {:?}, the pack says {:?}", g.label, g.tokens, w.tokens)
                    } else if g.logits_digest != w.logits_digest {
                        format!("{}: the logits differ", g.label)
                    } else {
                        format!("{}: the commit points differ", g.label)
                    }
                })
                .collect();
            if !bad.is_empty() {
                acc.fail("conformance", bad.join("; "));
            } else if let Some(why) = &note.ref2_skipped {
                // Nothing failed, but a check that was asked was not made: SKIPPED, with the reason, never PASS.
                acc.skip(
                    "conformance",
                    format!(
                        "streamed: {} vector(s), {} positions, equal on {}; the independent implementation did not run — {why}",
                        got.len(),
                        got.iter().map(|v| v.positions).sum::<usize>(),
                        note.ran.join(", ")
                    ),
                );
            } else {
                acc.pass(
                    "conformance",
                    format!(
                        "streamed: {} vector(s), {} positions, equal on {} (largest decoded parameter: reference {:.1} MiB, independent {:.1} MiB; not process RSS)",
                        got.len(),
                        got.iter().map(|v| v.positions).sum::<usize>(),
                        note.ran.join(", "),
                        note.reference_peak_tensor_bytes as f64 / (1 << 20) as f64,
                        note.independent_peak_tensor_bytes as f64 / (1 << 20) as f64
                    ),
                );
            }
        }
    }
}

/// [`hf_check`] streamed: the fit measured on the typed backend over the mapped artifact.
fn hf_check_streamed(acc: &mut Acc, pack: &RuntimePackV1, art: &Path, dir: &Path) {
    let Some(h) = &pack.hf_reference else {
        acc.skip("hf-reference", "the pack carries no Hugging Face reference");
        return;
    };
    let hf = match HfReference::load(&dir.join(&h.file)) {
        Ok(x) => x,
        Err(e) => {
            acc.fail("hf-reference", e);
            return;
        }
    };
    let artifact = match misaka_palw_tir_exec::node::TirArtifactV1::open(art) {
        Ok(a) => a,
        Err(e) => {
            acc.fail("hf-reference", format!("the artifact does not open: {e}"));
            return;
        }
    };
    match hfref::measure_streamed(&artifact, pack.logits.scale, &hf) {
        Err(e) => acc.fail("hf-reference", e),
        Ok(fit) => {
            let bad = hfref::check(&fit, &pack.logits.tolerance);
            if bad.is_empty() {
                acc.pass(
                    "hf-reference",
                    format!(
                        "streamed: slope {:.4}, corr {:.5}, top-1 {:.3}, KL {:.5} over {} positions",
                        fit.slope,
                        fit.corr,
                        fit.top1,
                        fit.kl_mean,
                        hf.positions()
                    ),
                );
            } else {
                acc.fail("hf-reference", bad.join("; "));
            }
        }
    }
}

fn hf_check(acc: &mut Acc, pack: &RuntimePackV1, a: &conformance::LoadedArtifact, dir: &Path) {
    let Some(h) = &pack.hf_reference else {
        acc.skip("hf-reference", "the pack carries no Hugging Face reference");
        return;
    };
    let hf = match HfReference::load(&dir.join(&h.file)) {
        Ok(x) => x,
        Err(e) => {
            acc.fail("hf-reference", e);
            return;
        }
    };
    match hfref::measure(a, pack.logits.scale, &hf) {
        Err(e) => acc.fail("hf-reference", e),
        Ok(fit) => {
            let bad = hfref::check(&fit, &pack.logits.tolerance);
            let units =
                if pack.logits.convention == "q24-natural-v1" { "q24 natural-log logits" } else { "legacy greedy-only: tool scale" };
            if bad.is_empty() {
                acc.pass("hf-reference", format!("{units}: slope {:.4}, corr {:.5}, top-1 {:.3}, KL {:.5} over {} positions (pack recorded top-1 {:.3}, KL {:.5})", fit.slope, fit.corr, fit.top1, fit.kl_mean, hf.positions(), h.measured.top1, h.measured.kl_mean));
            } else {
                acc.fail("hf-reference", bad.join("; "));
            }
        }
    }
}

/// Used by tests and tools: the calibration sequences a pack carries, when it does.
pub fn pack_calibration(dir: &Path, pack: &RuntimePackV1) -> Option<CalibInput> {
    pack.profile.calibration.sequences_file.as_ref().and_then(|f| CalibInput::from_file(&dir.join(f)).ok())
}
