//! **One place a node turns "the chain says class X" into "run it"** (ADR-0053) — now the SDK's
//! door inside kaspad.
//!
//! The dispatch itself lives in `misaka_palw_sdk`: the SDK holds one lineage list (the dense
//! container and the Qwen3.6 mmap tier today), and resolving a chain-named `(class_id,
//! artifact_root)` walks it — each lineage serves its class, refuses it by name, or passes. This
//! module keeps the node-side shape both services construct per duty, and nothing else: a new
//! lineage lands in the SDK and this file does not move, which is the property the old
//! three-armed dispatch could not have.
//!
//! `resolve` still refuses rather than substitutes — the floor is DERIVED, so a node with nothing
//! installed can always serve it, and a converted class this node lacks the artifact for is an
//! error and never a fallback to some class it does have.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_core::{info, warn};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::{PalwClassSdk, PalwLoadedArtifactV1};

/// What a node holds that lets it act for some class: the SDK (which classes exist, how they
/// load, pair, and execute) plus this node's loaded holdings. Rebuilt per duty and per pooled
/// payload; the holdings are `Arc`-backed inside, so the rebuild is pointer clones, not gigabytes
/// (audit M2-14). That sharing is between REBUILDS of one service's registry; between the two
/// services that each build one from the same `--palw-class-artifact` list it is
/// [`load_class_holdings_v1`], because each constructor loading the list for itself was two
/// mappings and two root passes over the same 33 GiB file (testnet-11 Relaunch 5c).
pub struct PalwBackendRegistry {
    sdk: PalwClassSdk,
    holdings: Vec<PalwLoadedArtifactV1>,
}

impl PalwBackendRegistry {
    pub fn new(court: PalwCourtParamsV2, holdings: Vec<PalwLoadedArtifactV1>, network_id: Vec<u8>) -> Self {
        Self { sdk: PalwClassSdk::builtin_v1(court, network_id), holdings }
    }

    /// **ADR-0067: a registry whose chain-registered arm is armed.** The operator's deliberate
    /// flag (`--palw-chain-classes`) is the ONLY caller — the fence's SDK half refuses without
    /// this, and this constructor is the greppable node half.
    pub fn new_with_chain_classes(court: PalwCourtParamsV2, holdings: Vec<PalwLoadedArtifactV1>, network_id: Vec<u8>) -> Self {
        Self { sdk: PalwClassSdk::builtin_v1(court, network_id).with_chain_classes_v1(), holdings }
    }

    /// The SDK this registry dispatches through — the panel's registration builder asks it for
    /// candidates and admission preflight, against the same holdings `resolve` serves.
    pub fn sdk(&self) -> &PalwClassSdk {
        &self.sdk
    }

    pub fn holdings(&self) -> &[PalwLoadedArtifactV1] {
        &self.holdings
    }

    /// **Resolve the class the chain named into something that can run it.**
    ///
    /// `class_id` and `artifact_root` come off the class record, so they are the chain's answer.
    /// A node that cannot serve that class says so — it does not fall back to one it can, because
    /// producing or judging under a class the chain did not name is worse than not participating.
    pub fn resolve(&self, class_id: Hash64, artifact_root: Hash64) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        self.sdk.resolve(class_id, artifact_root, &self.holdings)
    }

    /// **Resolve through the tables, then — armed — through the chain's own registration**
    /// (ADR-0067 Decisions 1–2). `fetch` is the caller's session read
    /// (`palw_registered_class_carriage_v1`): it runs only when every table has passed, and its
    /// `None` keeps the table refusal, because "the chain never registered it" must not read
    /// better than "this build cannot serve it".
    pub fn resolve_or_chain<F>(
        &self,
        class_id: Hash64,
        artifact_root: Hash64,
        fetch: F,
    ) -> Result<Box<dyn PalwExecutionBackendV1>, String>
    where
        F: FnOnce(
            Hash64,
        )
            -> Option<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)>,
    {
        match self.sdk.resolve(class_id, artifact_root, &self.holdings) {
            Ok(backend) => Ok(backend),
            Err(table_refusal) => {
                // **ADR-0067 SA-2: a class this node already failed to serve is not compiled
                // again.** The chain arm fetches a stranger's declaration and compiles it; a
                // hostile registration that costs a second to refuse costs that second on EVERY
                // duty until it is remembered. The mark is node-local serviceability and nothing
                // else — see `unservable_chain_classes`.
                if let Some(why) = remembered_unservable(class_id, artifact_root) {
                    return Err(why);
                }
                match fetch(class_id) {
                    Some((profile, canonical)) => {
                        match self.sdk.resolve_chain_registered(class_id, artifact_root, &self.holdings, &profile, &canonical) {
                            Ok(backend) => Ok(backend),
                            Err(why) => {
                                remember_unservable(class_id, artifact_root, &why);
                                Err(why)
                            }
                        }
                    }
                    None => Err(table_refusal),
                }
            }
        }
    }
}

/// **ADR-0067 SA-2: the classes this node has tried to serve from chain data and could not.**
///
/// Keyed by `(class_id, artifact_root)` — the two facts the chain states — and holding the refusal
/// text, so the second caller gets the same sentence the first did.
///
/// **What this is NOT, said where the map is:** it is not a consensus fact and it can never reject
/// a block. Nothing in `consensus/` links against `misaka-palw-sdk` at all
/// (`class_resolution_is_not_reachable_from_the_block_processing_path` asserts it from the
/// manifests), so a profile that refuses to compile stops this node from PRODUCING or JUDGING for
/// that class and stops nothing else. That is the whole of SA-2: resolution is lazy, off the block
/// path, and fails closed to "cannot serve" — because the alternative is a stranger's registration
/// stalling every validator's pipeline once.
///
/// The mark is dropped when the holdings it was computed against are (SA-3): a refusal that said
/// "this node holds no artifact whose digest is the registered root" is a statement about the
/// node's files, and it stops being true the moment the operator supplies them.
fn unservable_chain_classes() -> &'static Mutex<HashMap<(Hash64, Hash64), String>> {
    static UNSERVABLE: OnceLock<Mutex<HashMap<(Hash64, Hash64), String>>> = OnceLock::new();
    UNSERVABLE.get_or_init(Default::default)
}

fn remembered_unservable(class_id: Hash64, artifact_root: Hash64) -> Option<String> {
    unservable_chain_classes().lock().unwrap_or_else(|p| p.into_inner()).get(&(class_id, artifact_root)).cloned()
}

fn remember_unservable(class_id: Hash64, artifact_root: Hash64, why: &str) {
    warn!("[palw] class {class_id} is unservable on this node and will not be recompiled until its artifacts change: {why}");
    unservable_chain_classes().lock().unwrap_or_else(|p| p.into_inner()).insert((class_id, artifact_root), why.to_string());
}

/// **ADR-0067 SA-3: dropping a held artifact drops every verdict derived from it.**
///
/// Two verdicts hang off a holding, and both are derivations from BYTES: the artifact's root (the
/// positive one — "these bytes are that class's weights") and any "unservable" mark (the negative
/// one). ADR-0079 Decision 9's rule is that artifact identity never comes from metadata, and the
/// holdings map is keyed by `(path, len, mtime)` — metadata, and metadata a re-mint can be made to
/// reproduce. That key is sound only while the mapping it guards is alive, because the mapping is
/// what pins the bytes the root was computed over. The moment a holding is released, its key stops
/// standing for anything, so the verdict goes with it and re-entry re-reads the file.
///
/// Returns how many holdings were released, so a caller can log a number rather than a hope.
pub fn evict_held_artifacts_v1(paths: &[PathBuf]) -> usize {
    let mut held = held_artifacts().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut released = 0usize;
    for path in paths {
        // The key is recomputed from the file as it is NOW, and a file whose metadata moved is a
        // file whose old key is unreachable anyway — so eviction also sweeps by path, or a
        // re-minted file would leave its predecessor's holding (and its root) resident forever.
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let before = held.len();
        held.retain(|key, _| key.path != canonical);
        released += before - held.len();
    }
    // The negative verdicts go with them. A class marked unservable because this node held no
    // matching artifact must be re-judged once the holdings change, or supplying the file would
    // never take effect — the remembered refusal would outlive the fact it described.
    unservable_chain_classes().lock().unwrap_or_else(|p| p.into_inner()).clear();
    released
}

/// [`evict_held_artifacts_v1`] for every holding this process has — the shape an operator-driven
/// cache flush takes, and what the tests use to prove re-entry re-reads bytes.
pub fn evict_all_held_artifacts_v1() -> usize {
    let mut held = held_artifacts().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let released = held.len();
    held.clear();
    unservable_chain_classes().lock().unwrap_or_else(|p| p.into_inner()).clear();
    released
}

/// **What identifies an artifact FILE to the process-wide holdings below**: the path the operator
/// named, resolved (two spellings of one file are one file), and the size and modification time
/// it had when it was mapped. A re-mint dropped in under the same name has a different size or
/// mtime, so it is mapped afresh and re-hashed rather than served from the previous file's root —
/// the root is derived from bytes, and a key blind to which bytes would let a stale derivation
/// stand in for a fresh one, which is a declared root wearing a derived one's clothes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct HeldArtifactKey {
    path: PathBuf,
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl HeldArtifactKey {
    /// `None` when the file cannot be stat'ed: the load itself then refuses by name, and a
    /// refusal is never held.
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        Some(Self { path, len: meta.len(), modified: meta.modified().ok() })
    }
}

/// The holdings this process has loaded, by file identity — the one place a mapping lives.
fn held_artifacts() -> &'static Mutex<HashMap<HeldArtifactKey, PalwLoadedArtifactV1>> {
    static HELD: OnceLock<Mutex<HashMap<HeldArtifactKey, PalwLoadedArtifactV1>>> = OnceLock::new();
    HELD.get_or_init(Default::default)
}

/// **Load a duty's `--palw-class-artifact` list, mapping and hashing each file at most once per
/// process.**
///
/// The producer and the panel build their registries from the same list, and each service's
/// constructor loaded it for itself. On testnet-11 Relaunch 5c (2026-09-02) that was two `mmap`s
/// of one 33 GiB file and two full root passes over it — `[palw-producer] mapped …` at 01:49:44,
/// `[palw-panel] mapped …` at 01:57:23 on the same host — with a 24 GiB machine paying the
/// second pass's page-cache churn right after the first. The `Arc` inside a holding shares it
/// between rebuilds of ONE duty's registry, which is all the claim on [`PalwBackendRegistry`]
/// ever covered; between the two constructors nothing was shared, and that is where the second
/// mapping came from.
///
/// So the process holds each artifact once, here, and every duty that names that file gets the
/// same holding back: pointer clones of one mapping, the root computed exactly once. The
/// operator's byte bound still applies per duty, in their order, through the SDK's own loop — a
/// file the bound would skip is skipped whether or not another duty holds it, because the bound
/// says what this duty declares it can serve, not what the process has mapped.
///
/// Every outcome is logged under `role` as the two constructors logged it before: the lineage's
/// own summary for a file this call mapped (`mapped Qwen3.6 artifact …` — once per file per
/// process, so that line still counts artifacts), a line naming the file for one another duty
/// already holds, and a warning for each path not held and why. A file about to be mapped is
/// announced first, because a cold root pass over 33 GiB is minutes of otherwise silent startup.
pub fn load_class_holdings_v1(role: &str, sdk: &PalwClassSdk, paths: &[PathBuf], bound_bytes: u64) -> Vec<PalwLoadedArtifactV1> {
    // Held across the whole load on purpose: the guarantee is "once", so a second duty asking
    // for a file the first is still hashing waits for that holding rather than starting its own
    // pass. Nothing under the lock calls back in. A load that panicked (the root pass on a file
    // that became unreadable) poisons the lock without corrupting the map, so a later duty takes
    // the map as it stands rather than losing every holding to one bad file.
    let mut held = held_artifacts().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (holdings, skipped) = sdk.load_artifacts_bounded_with_v1(paths, bound_bytes, |path| {
        held_or_load_locked(&mut held, role, path, |p| sdk.load_artifact(p))
    });
    for (path, why) in &skipped {
        warn!("[{role}] class artifact {} is not held: {why}", path.display());
    }
    holdings
}

/// One path's half of [`load_class_holdings_v1`], under a lock the caller already holds.
///
/// Factored out so the cache's ONE rule — a holding answers for a file only while this process
/// still holds the mapping that pins its bytes (ADR-0067 SA-3) — has one implementation and can be
/// tested for what it does after an eviction, which is the half that matters and the half a
/// process-wide static otherwise hides.
fn held_or_load_locked<F>(
    held: &mut HashMap<HeldArtifactKey, PalwLoadedArtifactV1>,
    role: &str,
    path: &Path,
    load: F,
) -> Result<PalwLoadedArtifactV1, String>
where
    F: FnOnce(&Path) -> Result<PalwLoadedArtifactV1, String>,
{
    let key = HeldArtifactKey::of(path);
    if let Some(holding) = key.as_ref().and_then(|key| held.get(key)) {
        info!(
            "[{role}] class artifact {} is already held by this process ({}): sharing that holding, \
             not mapping it a second time",
            path.display(),
            holding.lineage_id
        );
        return Ok(holding.clone());
    }
    if let Some(key) = &key {
        info!(
            "[{role}] loading class artifact {} ({:.2} GiB); a mapped class derives its root in one pass over the file, \
             which is minutes on a cold disk",
            path.display(),
            key.len as f64 / (1u64 << 30) as f64
        );
    }
    let holding = load(path)?;
    info!("[{role}] {}", holding.summary);
    if let Some(key) = key {
        held.insert(key, holding.clone());
    }
    Ok(holding)
}

/// The hybrid class's chain id — **re-exported, not re-derived**.
///
/// This used to project the geometry here, which its own doc called out as the hazard it was: "a
/// second spelling of the class id would be a second thing to drift". It then drifted. The
/// registration moved to the `graph-v3` declaration (ADR-0069: v1 names a GDN node no backend can
/// serve) and this spelling stayed on v1, so the node was asserting that the chain registers a
/// class the chain no longer registers. One spelling now, in the module that owns the geometry.
pub use kaspa_consensus_core::palw_qwen36_profile::qwen36_class_id_v3 as qwen36_class_id_v1;

#[cfg(test)]
mod tests {
    use super::*;

    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court")
    }

    fn registry() -> PalwBackendRegistry {
        PalwBackendRegistry::new(court(), Vec::new(), b"misaka-palw-rc".to_vec())
    }

    /// **The PUBLIC network's own class set, resolved against a node's holdings.**
    ///
    /// Not a fixture: this reads `Params::from(testnet-11)` — the ruleset a real node boots with —
    /// walks the classes its genesis registers, and asks the registry for each. The floor must
    /// resolve on a node holding nothing (it is derived); Qwen3.6 must be refused BY ROOT with the
    /// message that names the flag, because a node without the weights still validates the chain
    /// and simply cannot produce for that class.
    ///
    /// This is the test that would have caught a two-class ruleset whose second class no node
    /// could ever name — the class id the params register and the class id the registry dispatches
    /// on are derived in different modules, and nothing else compares them.
    #[test]
    fn the_public_networks_classes_resolve_the_way_a_node_would_ask() {
        use kaspa_consensus_core::config::params::palw_rc_qwen36_is_registered;
        let params: kaspa_consensus_core::config::params::Params =
            kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11).into();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 ships a ConsensusV2 bundle");
        };
        let classes: Vec<(Hash64, Hash64)> = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => {
                    Some((*class_id, *artifact_root))
                }
                _ => None,
            })
            .collect();
        let expected = 1
            + usize::from(palw_rc_qwen36_is_registered())
            + usize::from(kaspa_consensus_core::config::params::palw_rc_qwen25_a16_is_registered());
        assert_eq!(classes.len(), expected, "the shipped network registers exactly the classes its pins describe");

        let bare = PalwBackendRegistry::new(bundle.court, Vec::new(), params.net.to_string().into_bytes());
        let (floor_id, floor_root) = classes[0];
        assert_eq!(floor_id, bundle.base_class_id, "the floor is registered first");
        let floor = bare.resolve(floor_id, floor_root).expect("the derived floor resolves on a node holding nothing");
        assert_eq!(floor.model_id(), "PALW-BASE-0/rc");

        // Every non-floor class must be one this build can NAME (its id derives from a pinned
        // geometry here) and REFUSE BY ROOT on a node holding nothing. The floor is index 0; the
        // rest are checked by membership rather than by position, because the registration list's
        // order is the genesis gate's business and not this test's.
        let known: Vec<(Hash64, Hash64)> = [
            palw_rc_qwen36_is_registered()
                .then(|| (qwen36_class_id_v1(), kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT)),
            kaspa_consensus_core::config::params::palw_rc_qwen25_a16_is_registered().then(|| {
                (
                    kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_class_id_v2(),
                    kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT,
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        for (id, root) in &known {
            let (_, registered_root) = classes
                .iter()
                .find(|(c, _)| c == id)
                .unwrap_or_else(|| panic!("the network registers a class this build dispatches on: {id}"));
            assert_eq!(registered_root, root, "the network's artifact root for {id} is the pinned one");
            let err = match bare.resolve(*id, *root) {
                Err(e) => e,
                Ok(b) => panic!("a node holding no weights resolved {id} to {}", b.model_id()),
            };
            assert!(err.contains("--palw-class-artifact"), "the refusal names the flag that fixes it: {err}");
        }
        assert_eq!(known.len() + 1, classes.len(), "every registered class is one this build can name");

        if let Some(&(qwen_id, qwen_root)) = classes.iter().find(|(c, _)| *c == qwen36_class_id_v1()) {
            assert_eq!(qwen_id, qwen36_class_id_v1(), "the registered second class is the one this build dispatches on");
            assert_eq!(
                qwen_root,
                kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT,
                "the network's artifact root is the pinned one"
            );
            // And an artifact whose COMPUTED root is not the chain's is refused too — the file's
            // name is never the answer, and neither is a declared root: the holding derives its
            // root from the fixture's own bytes, and that root is not the registered class's.
            let alien = PalwBackendRegistry::new(
                bundle.court,
                vec![misaka_palw_sdk::lineages::qwen36::holding_from_artifact(
                    std::sync::Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(1, 8)),
                    None,
                )],
                params.net.to_string().into_bytes(),
            );
            assert!(alien.resolve(qwen_id, qwen_root).is_err(), "a file with the wrong root is not this class");
        }
    }

    /// **The floor resolves on a node with nothing installed.** It is derived, so it needs no
    /// artifact and no worker — the property that keeps a plain Linux node the liveness anchor,
    /// and the property the withdrawn family could not have (its seats had to hold particular
    /// hardware before one claim could license).
    #[test]
    fn the_floor_resolves_with_no_files_at_all() {
        let entry = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court(), "PALW-BASE-0/rc").expect("the floor");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        let backend = registry().resolve(entry.class_id(), root).expect("the floor resolves");
        assert_eq!(backend.model_id(), "PALW-BASE-0/rc");
    }

    /// **A class this node does not hold is an error, not a substitution.** The old dispatch could
    /// answer "wrong family" here; what is left is the honest question — does this node have the
    /// artifact the chain's `(class_id, artifact_root)` names — and the honest refusal.
    #[test]
    fn a_class_this_node_does_not_hold_is_refused_by_name() {
        let err = match registry().resolve(Hash64::from_u64_word(0x99), Hash64::from_u64_word(0xA1)) {
            Err(e) => e,
            Ok(b) => panic!("a node with no artifacts resolved an unknown class to {}", b.model_id()),
        };
        assert!(err.contains("cannot serve the registered class"), "{err}");
    }

    /// A `.palwq36` on disk, written from the dev fixture the way the base0 round-trip test writes
    /// one. `layers` changes the file's size, which is what a replaced artifact looks like to the
    /// holdings' key.
    fn write_qwen36_fixture(path: &Path, layers: usize) {
        use misaka_palw_base0::qwen36::{Qwen36Writer, qwen36_dev_fixture};
        let owned = qwen36_dev_fixture(layers, 8);
        let plan: Vec<(String, usize)> =
            owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
        let mut writer =
            Qwen36Writer::create(path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("the file is created");
        for (name, _) in &plan {
            writer.push(name, owned.tensor(name).expect("present")).expect("the tensor is appended");
        }
        writer.finish().expect("the plan is filled");
    }

    fn temp_artifact(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("misaka-holdings-{name}-{}.palwq36", std::process::id()))
    }

    /// **The holdings map and the unservable map are PROCESS-wide, so a test that clears one runs
    /// alone.** Eviction is the only operation whose effect is not confined to its own paths
    /// (ADR-0067 SA-3: changing the holdings re-opens every "unservable" question), which is
    /// exactly why it cannot share a process with a test asserting that two loads returned one
    /// mapping. Every test that evicts, and every test an eviction could invalidate, takes this.
    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: Mutex<()> = Mutex::new(());
        SERIAL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn sdk() -> PalwClassSdk {
        PalwClassSdk::builtin_v1(court(), b"misaka-palw-rc".to_vec())
    }

    fn qwen36_parts(holding: &PalwLoadedArtifactV1) -> (Hash64, std::sync::Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>) {
        misaka_palw_sdk::lineages::qwen36::parts_of(holding).expect("a Qwen3.6 holding")
    }

    /// **Two duties naming one file hold one mapping.** The producer's and the panel's
    /// constructors each ask for the operator's list; the second answer is the first's holding —
    /// the same `Arc`, the same mapping, the root computed once — which is the whole of the fix
    /// for the testnet-11 double mapping.
    #[test]
    fn two_duties_naming_one_artifact_share_one_mapping() {
        let _serial = exclusive();
        let path = temp_artifact("shared");
        write_qwen36_fixture(&path, 1);
        let producer = load_class_holdings_v1("test-producer", &sdk(), std::slice::from_ref(&path), 0);
        let panel = load_class_holdings_v1("test-panel", &sdk(), std::slice::from_ref(&path), 0);
        assert_eq!((producer.len(), panel.len()), (1, 1), "both duties hold the file");
        assert!(
            std::sync::Arc::ptr_eq(&producer[0].payload(), &panel[0].payload()),
            "the panel holds the producer's holding, not a second one"
        );
        let (root_a, map_a) = qwen36_parts(&producer[0]);
        let (root_b, map_b) = qwen36_parts(&panel[0]);
        assert_eq!(root_a, root_b, "one root, computed once");
        assert!(std::sync::Arc::ptr_eq(&map_a, &map_b), "one `Qwen36ArtifactV1`, one mmap");
        std::fs::remove_file(&path).ok();
    }

    /// **A file replaced under the same name is mapped and hashed afresh.** The key is the file's
    /// identity (size, mtime), not the path string: a re-mint dropped in over the old artifact has
    /// a different root, and serving the previous mapping's root for it would be a declared root
    /// wearing a derived one's clothes.
    #[test]
    fn a_replaced_artifact_is_mapped_and_hashed_afresh() {
        let path = temp_artifact("replaced");
        write_qwen36_fixture(&path, 1);
        let first = load_class_holdings_v1("test", &sdk(), std::slice::from_ref(&path), 0);
        // Replaced by rename, the way an operator drops in a new artifact: the old mapping stays
        // valid on its own inode, and the path now names a file of another size.
        let staged = temp_artifact("replaced-staged");
        write_qwen36_fixture(&staged, 2);
        std::fs::rename(&staged, &path).expect("renamed over the old artifact");
        let second = load_class_holdings_v1("test", &sdk(), std::slice::from_ref(&path), 0);
        assert_eq!((first.len(), second.len()), (1, 1));
        assert!(!std::sync::Arc::ptr_eq(&first[0].payload(), &second[0].payload()), "a different file is a different holding");
        assert_ne!(qwen36_parts(&first[0]).0, qwen36_parts(&second[0]).0, "the new file's root is derived from the new file");
        std::fs::remove_file(&path).ok();
    }

    /// **The operator's byte bound is still this duty's, held file or not.** A path the bound
    /// would skip is skipped by name even when another duty already holds it: the bound says what
    /// this duty declares it can serve, and sharing a mapping does not widen that.
    #[test]
    fn the_bound_still_skips_a_file_the_process_already_holds() {
        let path = temp_artifact("bounded");
        write_qwen36_fixture(&path, 1);
        let size = std::fs::metadata(&path).expect("the fixture exists").len();
        assert_eq!(load_class_holdings_v1("test", &sdk(), std::slice::from_ref(&path), 0).len(), 1, "unbounded: held");
        let bounded = load_class_holdings_v1("test", &sdk(), std::slice::from_ref(&path), size - 1);
        assert!(bounded.is_empty(), "the bound skips the file whether or not the process already holds it");
        std::fs::remove_file(&path).ok();
    }

    /// **ADR-0067 SA-3: eviction retracts the verdict, and re-entry re-reads the bytes.**
    ///
    /// The holdings map is keyed by `(path, len, mtime)`, and ADR-0079 Decision 9 says artifact
    /// identity may not come from metadata. It does not have to: while the mapping is alive it is
    /// what pins the bytes the root was computed over, so the key is a handle to a live derivation
    /// rather than a substitute for one. The rule that keeps that true is this one — releasing the
    /// holding releases the verdict, and the next caller does the byte work again. A cache that
    /// answered from a remembered root after its mapping was gone would be exactly the metadata
    /// identity D9 refuses.
    ///
    /// Driven through `held_or_load_locked` with a counting loader, because "the bytes were read
    /// again" is the observable and a returned holding is not.
    #[test]
    fn evicting_a_holding_retracts_its_verdict_so_re_entry_reads_the_bytes_again() {
        let _serial = exclusive();
        let path = temp_artifact("evicted");
        write_qwen36_fixture(&path, 1);
        let reads = std::cell::Cell::new(0usize);
        let counting = |p: &Path| -> Result<PalwLoadedArtifactV1, String> {
            reads.set(reads.get() + 1);
            Ok(PalwLoadedArtifactV1::from_parts(
                "test-lineage",
                Some(p.to_path_buf()),
                "a fixture holding".to_string(),
                std::sync::Arc::new(reads.get()),
            ))
        };
        {
            let mut held = held_artifacts().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            held_or_load_locked(&mut held, "test", &path, counting).expect("first load");
            assert_eq!(reads.get(), 1, "the first call must read the file");
            held_or_load_locked(&mut held, "test", &path, counting).expect("cached");
            assert_eq!(reads.get(), 1, "a live mapping answers without a second pass — that is the cache");
        }
        // The verdict is dropped with the mapping…
        assert!(evict_held_artifacts_v1(std::slice::from_ref(&path)) >= 1, "the holding was released");
        {
            let mut held = held_artifacts().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            held_or_load_locked(&mut held, "test", &path, counting).expect("re-entry");
            assert_eq!(reads.get(), 2, "…and re-entry derives from bytes, never from what the cache remembered");
        }
        evict_held_artifacts_v1(std::slice::from_ref(&path));
        std::fs::remove_file(&path).ok();
    }

    /// **ADR-0067 SA-2: an unservable chain class is remembered, and forgotten when the holdings
    /// that made it unservable change.**
    ///
    /// The mark exists so a hostile registration costs one compile per node rather than one per
    /// duty. It must also be exactly as short-lived as the fact it records: most refusals say
    /// "this node holds no artifact whose digest is the registered root", which stops being true
    /// the moment the operator supplies the file. A remembered refusal that outlived its cause
    /// would make supplying the artifact do nothing.
    #[test]
    fn an_unservable_chain_class_is_remembered_until_the_holdings_change() {
        let _serial = exclusive();
        evict_all_held_artifacts_v1();
        let class = Hash64::from_u64_word(0x0067_5A02);
        let root = Hash64::from_u64_word(0x0067_5A03);
        assert!(remembered_unservable(class, root).is_none(), "nothing is unservable before anything was tried");
        remember_unservable(class, root, "the profile names a kernel this build does not carry");
        assert_eq!(
            remembered_unservable(class, root).as_deref(),
            Some("the profile names a kernel this build does not carry"),
            "the second caller gets the first caller's sentence, without recompiling a stranger's graph"
        );
        // A different pairing is a different question and is not answered by this mark.
        assert!(remembered_unservable(class, Hash64::from_u64_word(0xFEED)).is_none());
        evict_all_held_artifacts_v1();
        assert!(remembered_unservable(class, root).is_none(), "changing the holdings re-opens the question");
    }

    /// **ADR-0067 SA-2's load-bearing half, asserted from the manifests: class resolution is not
    /// reachable from block processing at all.**
    ///
    /// The amendment asks that resolving a chain class — fetching bytes, verifying a digest,
    /// compiling a stranger's profile — never run on the block-processing path, so that a hostile
    /// registration cannot stall every validator's pipeline. The strongest form of that is not a
    /// discipline about where a call is made; it is that the consensus crates cannot make the call.
    /// They do not depend on `misaka-palw-sdk`, and this asserts it where a future edit would have
    /// to notice: adding the dependency fails here, before anyone writes the call site.
    #[test]
    fn class_resolution_is_not_reachable_from_the_block_processing_path() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        for manifest in ["consensus/Cargo.toml", "consensus/core/Cargo.toml", "consensus/pow/Cargo.toml"] {
            let path = root.join(manifest);
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                !text.contains("misaka-palw-sdk"),
                "{manifest} depends on the class SDK — resolving a chain-registered class would then be reachable \
                 from block processing, and one hostile profile could stall every validator (ADR-0067 SA-2)"
            );
        }
    }
}
