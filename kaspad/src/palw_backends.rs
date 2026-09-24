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
use kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1;
use kaspa_core::{error, info, warn};
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
    pub fn new(
        court: PalwCourtParamsV2,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        holdings: Vec<PalwLoadedArtifactV1>,
        network_id: Vec<u8>,
    ) -> Self {
        Self { sdk: PalwClassSdk::builtin_v1(court, prompt_ids_form, network_id), holdings }
    }

    /// **ADR-0067: a registry whose chain-registered arm is armed.** The operator's deliberate
    /// flag (`--palw-chain-classes`) is the ONLY caller — the fence's SDK half refuses without
    /// this, and this constructor is the greppable node half.
    pub fn new_with_chain_classes(
        court: PalwCourtParamsV2,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        holdings: Vec<PalwLoadedArtifactV1>,
        network_id: Vec<u8>,
    ) -> Self {
        Self { sdk: PalwClassSdk::builtin_v1(court, prompt_ids_form, network_id).with_chain_classes_v1(), holdings }
    }

    /// **Every backend this registry resolves runs `rules`** (ADR-0152 v3.1 J-5) — the node passes
    /// `palw_attempt_rules_of_params_v1(&params)`, `CoreV1` on a network that arms
    /// `palw_offence_attribution`.
    pub fn with_attempt_rules_v1(mut self, rules: kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1) -> Self {
        self.sdk = self.sdk.with_attempt_rules_v1(rules);
        self
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

    /// **The bytes a replay of THIS class will actually hold** (H-4 of the 2026-09-18 audit,
    /// ADR-0112 for the mmap tier).
    ///
    /// The resolver is the only thing that knows which holding answers a (class, root), so each
    /// holding is offered on its own and the one that resolves is the one whose pages the replay
    /// will touch. `None` when no holding serves the class — the caller then has no per-class
    /// figure and falls back to its conservative one.
    ///
    /// A Qwen3.6 mmap holding under a residency budget pages the always-set plus routed experts,
    /// not the 34 GiB file. Pricing the replay on `metadata.len()` made every 23 GiB testnet-11
    /// seat defer QWEN36 forever (`class needs 34.49 GiB` against a ~13 GiB budget) even after the
    /// process had already pinned ~6 GiB. Dense holdings still report the file: they decode it.
    pub fn holding_bytes_for_v1(&self, class_id: Hash64, artifact_root: Hash64) -> Option<u64> {
        self.holdings
            .iter()
            .find(|holding| self.sdk.resolve(class_id, artifact_root, std::slice::from_ref(holding)).is_ok())
            .and_then(holding_replay_bytes_v1)
    }

    /// **Bytes a replay of this class still has to take from MemAvailable** — a full seat's need,
    /// through [`Self::role_memory_need_v1`]. Kept for the readers that want one number.
    pub fn incremental_replay_bytes_for_v1(&self, class_id: Hash64, artifact_root: Hash64) -> Option<u64> {
        self.role_memory_need_v1(class_id, artifact_root, PalwResourceRoleV1::FullSeat).map(|n| n.total_bytes())
    }

    /// **What `role` of this class needs on this node, decomposed** (ADR-0151 follow-up, items 1
    /// and 2): the holding's incremental bytes plus the role's resource profile — the ONE
    /// composition the producer's gate, the panel's pre-check and the court's replay read, so a
    /// role is never again priced by the artifact's file size.
    ///
    /// A Qwen3.6 holding with ADR-0112 residency has already pinned its budget in this process.
    /// Charging `budget_bytes` again against `MemAvailable` made every 23 GiB seat that had just
    /// mapped QWEN36 defer it (`class needs 6.50 GiB` against a ~3 GiB leftover budget) — the
    /// always-set was already in RSS. A page-cache or dense holding still reports the file, which
    /// is what a replay will fault in.
    ///
    /// **Memoized, because this is the panel's PER-TICK pre-check and a resolve constructs a
    /// backend.** Every tick, before deciding whether a readiness proof is affordable, the panel
    /// asked this, and this resolved the class against each holding — compiling the class's plan
    /// and hashing the artifact each time. The figure is a function of (which holdings, class,
    /// root, role) and none of those change for the life of this registry, so it is computed once.
    /// A partial seat's figure is a function of the claim as well and is not memoized here — the
    /// caller that holds the claim asks [`Self::role_memory_need_for_backend_v1`].
    pub fn role_memory_need_v1(&self, class_id: Hash64, artifact_root: Hash64, role: PalwResourceRoleV1) -> Option<PalwRoleMemoryNeedV1> {
        let key = (
            self.holdings.iter().map(|h| h.path.clone().unwrap_or_default()).collect::<Vec<_>>(),
            class_id,
            artifact_root,
            role,
            false,
        );
        let memoized = !matches!(role, PalwResourceRoleV1::PartialSeat { .. });
        if memoized
            && let Ok(memo) = replay_bytes_memo_v1().lock()
            && let Some(hit) = memo.get(&key)
        {
            return hit.clone();
        }
        let figure = self.holdings.iter().find_map(|holding| {
            let backend = self.sdk.resolve(class_id, artifact_root, std::slice::from_ref(holding)).ok()?;
            let holding_bytes = incremental_replay_bytes_v1(holding)?;
            Some(Self::compose_need_v1(backend.as_ref(), holding_bytes, None, role))
        });
        if memoized && let Ok(mut memo) = replay_bytes_memo_v1().lock() {
            memo.insert(key, figure.clone());
        }
        figure
    }

    /// [`Self::role_memory_need_v1`] for a caller that already holds the resolved backend and the
    /// job — the producer at its gate, a seat at its claim — so no second resolve is paid.
    pub fn role_memory_need_for_backend_v1(
        &self,
        backend: &dyn PalwExecutionBackendV1,
        class_id: Hash64,
        artifact_root: Hash64,
        job: Option<&kaspa_consensus_core::palw_v2::PalwJobContextV2>,
        role: PalwResourceRoleV1,
    ) -> PalwRoleMemoryNeedV1 {
        let holding_bytes = self
            .holdings
            .iter()
            .find(|holding| self.sdk.resolve(class_id, artifact_root, std::slice::from_ref(holding)).is_ok())
            .and_then(incremental_replay_bytes_v1)
            .unwrap_or(0);
        Self::compose_need_v1(backend, holding_bytes, job, role)
    }

    /// **The holding that serves `(class_id, artifact_root)`, through [`Self::resolve_or_chain`]'s
    /// door** (the route-matrix re-audit's #5), with the backend it resolved to: each holding offered
    /// on its own to the tables, then — where `fetch` answers — to the chain's own registration.
    /// `None` when no holding serves the class by either door.
    ///
    /// The panel's readiness proofs and status resolve a chain-registered class through that door,
    /// while the memory figures beside them asked the tables alone: for such a class they found no
    /// holding, priced the replay at the widest holding with no derived, KV or runtime bytes, and a
    /// seat proved readiness for a class it could not replay (drawable, then silent or OOM when
    /// drawn) — or was refused a proof because it held a large unrelated artifact.
    fn serving_holding_or_chain_v1<F>(
        &self,
        class_id: Hash64,
        artifact_root: Hash64,
        fetch: F,
    ) -> Option<(&PalwLoadedArtifactV1, Box<dyn PalwExecutionBackendV1>)>
    where
        F: FnOnce(
            Hash64,
        )
            -> Option<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)>,
    {
        if let Some(found) = self
            .holdings
            .iter()
            .find_map(|holding| self.sdk.resolve(class_id, artifact_root, std::slice::from_ref(holding)).ok().map(|b| (holding, b)))
        {
            return Some(found);
        }
        // ADR-0067 SA-2: a class this node already failed to serve from chain data is not compiled
        // again per holding here either.
        if remembered_unservable(class_id, artifact_root, &self.holdings).is_some() {
            return None;
        }
        let (profile, canonical) = fetch(class_id)?;
        self.holdings.iter().find_map(|holding| {
            self.sdk
                .resolve_chain_registered(class_id, artifact_root, std::slice::from_ref(holding), &profile, &canonical)
                .ok()
                .map(|backend| (holding, backend))
        })
    }

    /// [`Self::role_memory_need_v1`] through [`Self::resolve_or_chain`]'s door — the tables, then the
    /// chain's registration where `fetch` answers (the route-matrix re-audit's #5). The table figure
    /// is the one [`Self::role_memory_need_v1`] gives, memo and all; a chain figure is memoized once
    /// found, and a miss is not (the registration may simply not have been read yet).
    pub fn role_memory_need_or_chain_v1<F>(
        &self,
        class_id: Hash64,
        artifact_root: Hash64,
        role: PalwResourceRoleV1,
        fetch: F,
    ) -> Option<PalwRoleMemoryNeedV1>
    where
        F: FnOnce(
            Hash64,
        )
            -> Option<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)>,
    {
        if let Some(need) = self.role_memory_need_v1(class_id, artifact_root, role) {
            return Some(need);
        }
        let key = (
            self.holdings.iter().map(|h| h.path.clone().unwrap_or_default()).collect::<Vec<_>>(),
            class_id,
            artifact_root,
            role,
            true,
        );
        let memoized = !matches!(role, PalwResourceRoleV1::PartialSeat { .. });
        if memoized
            && let Ok(memo) = replay_bytes_memo_v1().lock()
            && let Some(Some(hit)) = memo.get(&key)
        {
            return Some(hit.clone());
        }
        let (holding, backend) = self.serving_holding_or_chain_v1(class_id, artifact_root, fetch)?;
        let need = Self::compose_need_v1(backend.as_ref(), incremental_replay_bytes_v1(holding)?, None, role);
        if memoized && let Ok(mut memo) = replay_bytes_memo_v1().lock() {
            memo.insert(key, Some(need.clone()));
        }
        Some(need)
    }

    /// [`Self::role_memory_need_for_backend_v1`] whose holding is found through
    /// [`Self::resolve_or_chain`]'s door (the route-matrix re-audit's #5): a caller that resolved a
    /// chain-registered class got a backend the tables do not know, and the table-only holding lookup
    /// priced that replay's artifact at zero bytes.
    pub fn role_memory_need_for_backend_or_chain_v1<F>(
        &self,
        backend: &dyn PalwExecutionBackendV1,
        class_id: Hash64,
        artifact_root: Hash64,
        job: Option<&kaspa_consensus_core::palw_v2::PalwJobContextV2>,
        role: PalwResourceRoleV1,
        fetch: F,
    ) -> PalwRoleMemoryNeedV1
    where
        F: FnOnce(
            Hash64,
        )
            -> Option<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)>,
    {
        let holding_bytes = self
            .serving_holding_or_chain_v1(class_id, artifact_root, fetch)
            .and_then(|(holding, _)| incremental_replay_bytes_v1(holding))
            .unwrap_or(0);
        Self::compose_need_v1(backend, holding_bytes, job, role)
    }

    /// **What laying `capture` out WHOLE costs this node** (DoS audit 2026-09-24, #4) — the need a
    /// panel seat reserves before its capture sampler opens a single leaf.
    ///
    /// The sampler's prover re-executes a folded capture into dense tiles (`dense_capture_from_
    /// fold_v1`, `DenseTiles`) and holds them while it draws. The full-seat figure every other
    /// replay reserves prices the class's own sink, which for a held class is the FOLD — a few MiB
    /// where the sampler builds 62 GiB. So the figure here is the full-seat need of the capture's
    /// OWN job (its binding's context, never the class's canonical job: a free-prompt job is the
    /// user's) with the capture term replaced by the dense capture of the capture's own leaf count
    /// ([`palw_whole_capture_need_v1`]).
    ///
    /// `Err` is a refusal by name, BEFORE anything is priced: a capture that does not decode, or
    /// one past this node's materialization cap (`base0_materialize_cap_v1` at the ladder the
    /// node's backends are built at, the same function every family's `materialize_cap()` reads) —
    /// which goes to the streamed routes and is never reserved for, let alone laid out.
    ///
    /// The holding is found through [`Self::resolve_or_chain`]'s door (`fetch`, the route-matrix
    /// re-audit's #5), as every other panel memory figure is: a chain-registered class priced off
    /// the tables alone would reserve its capture with a zero-byte artifact.
    pub fn whole_capture_memory_need_v1<F>(
        &self,
        backend: &dyn PalwExecutionBackendV1,
        class_id: Hash64,
        artifact_root: Hash64,
        capture: &[u8],
        class_ladder: u64,
        fetch: F,
    ) -> Result<PalwRoleMemoryNeedV1, String>
    where
        F: FnOnce(
            Hash64,
        )
            -> Option<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)>,
    {
        let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(capture)
            .map_err(|_| "the capture does not decode".to_string())?;
        let binding = retention.binding();
        palw_whole_capture_admits_v1(
            self.sdk.court().max_step_leaf_count(),
            class_ladder,
            &binding.shape_profile,
            binding.step_leaf_count,
        )?;
        let need = self.role_memory_need_for_backend_or_chain_v1(
            backend,
            class_id,
            artifact_root,
            Some(&binding.job_context),
            PalwResourceRoleV1::FullSeat,
            fetch,
        );
        Ok(palw_whole_capture_need_v1(need, &binding.shape_profile, binding.step_leaf_count))
    }

    fn compose_need_v1(
        backend: &dyn PalwExecutionBackendV1,
        holding_bytes: u64,
        job: Option<&kaspa_consensus_core::palw_v2::PalwJobContextV2>,
        role: PalwResourceRoleV1,
    ) -> PalwRoleMemoryNeedV1 {
        PalwRoleMemoryNeedV1 {
            role,
            holding_bytes,
            derived_bytes: backend.artifact_derived_resident_bytes_v1(),
            runtime: backend.runtime_profile_v1(),
            profile: backend.resource_profile_v1(job, role),
        }
    }
}

/// **Write the ledger into the node's runtime state** (ADR-0151 follow-up, item 6) — the host
/// pool's snapshot, as `getPalwNodeStatus` reports it. Called by the panel's per-tick publish and by
/// the producer at its gate, so the figures a hold names are the figures a program can read.
pub fn publish_memory_ledger_v1(runtime: &mut kaspa_p2p_flows::flow_context::PalwNodeRuntimeV1) {
    let snapshot = crate::palw_memory_ledger::host_ledger_v1().snapshot();
    runtime.memory_share_bytes = snapshot.share_bytes.unwrap_or(0);
    runtime.memory_headroom_bytes = snapshot.live_bytes.unwrap_or(0);
    runtime.memory_reserved_bytes = snapshot.reserved_bytes;
    runtime.memory_available_bytes = snapshot.available_bytes.unwrap_or(0);
    runtime.memory_bounded = snapshot.available_bytes.is_some();
    runtime.memory_holders = snapshot
        .rows
        .iter()
        .map(|row| format!("{} of class {} job {} ({:.2} GiB)", row.key.role, row.key.class_id, row.key.job, gib(row.bytes)))
        .collect::<Vec<_>>()
        .join("; ");
}

impl PalwBackendRegistry {

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
                if let Some(why) = remembered_unservable(class_id, artifact_root, &self.holdings) {
                    return Err(why);
                }
                match fetch(class_id) {
                    Some((profile, canonical)) => {
                        match self.sdk.resolve_chain_registered(class_id, artifact_root, &self.holdings, &profile, &canonical) {
                            Ok(backend) => Ok(backend),
                            Err(why) => {
                                remember_unservable(class_id, artifact_root, &why, &self.holdings);
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
/// **The mark is answerable to the holdings it was computed against, and it is answerable AT THE
/// READ** (SA-3). A refusal that said "this node holds no artifact whose digest is the registered
/// root" is a statement about the artifacts this registry dispatches against, so a cached copy of
/// it must not be able to outlive them. What "them" means — and why it is the loaded holdings and
/// not the files on disk — is [`holdings_identity_v1`].
///
/// The first form of this map delegated that to an evictor, and **the evictor had no production
/// caller**: `evict_held_artifacts_v1` and `evict_all_held_artifacts_v1` are `pub` and are reached
/// only from this file's tests, so in a running node the rule the doc promised did not exist. It
/// is enforced here instead, where it cannot be forgotten: each entry records the holdings identity
/// it was derived from, and a read whose holdings do not match DROPS the entry and recomputes. No
/// caller has to remember anything, and the eviction path still clears the map for the same reason
/// it always did — one rule, two doors.
///
/// Two consequences worth stating because they are the whole of the guarantee:
///
/// * A registry whose holdings differ — a different `--palw-class-artifact` list, or a list
///   reloaded into different objects — cannot be served another registry's verdict. That was safe
///   by accident before (`daemon.rs` builds the producer's and the panel's configs from the same
///   two args, so their holdings could not diverge); it is safe by construction now, and a future
///   caller that builds a registry over a different list does not have to know this map exists.
/// * What the mark cannot do, and no longer pretends to: notice a file rewritten under a live
///   holding. `load_class_holdings_v1` runs once per service construction, so the running service
///   keeps dispatching against the mapping it loaded at startup — re-deriving would return the
///   identical sentence, which is why retraction keys on the holdings and not on their files
///   (see [`holdings_identity_v1`]). Supplying an artifact to a running node is a restart, or an
///   eviction; it was never a `touch`.
fn unservable_chain_classes() -> &'static Mutex<HashMap<(Hash64, Hash64), UnservableMarkV1>> {
    static UNSERVABLE: OnceLock<Mutex<HashMap<(Hash64, Hash64), UnservableMarkV1>>> = OnceLock::new();
    UNSERVABLE.get_or_init(Default::default)
}

/// A remembered refusal and the holdings it is a statement about.
#[derive(Clone, Debug)]
struct UnservableMarkV1 {
    why: String,
    /// [`holdings_identity_v1`] as of the moment the refusal was computed.
    against: HoldingsIdentityV1,
}

/// One entry per holding: the lineage that loaded it, the path the operator named for it, the
/// lineage's own summary line, and the address of the loaded object itself.
type HoldingsIdentityV1 = Vec<(&'static str, Option<PathBuf>, String, usize)>;

/// **What a set of holdings IS, for the purpose of deciding whether a verdict about them still
/// stands — and it is a question about MEMORY, with no filesystem in it.**
///
/// A refusal is derived from `resolve_chain_registered`'s inputs, and the only one of those that
/// can change under a running node is `&self.holdings`: the loaded objects a registry dispatches
/// against. So the identity is those objects — per holding, the lineage that produced it, the path
/// it was named by, the summary the lineage wrote (which is where a container records the root it
/// DERIVED from the bytes), and the address of the loaded payload, which is the mapping the answer
/// was actually computed from. Different holdings, different verdict; identical holdings, the same
/// verdict as many times as it is asked for.
///
/// **Two round-3 defects are closed by keying it here rather than on file metadata.**
///
/// * The first form re-stat'ed every configured artifact — `fs::metadata` plus `fs::canonicalize`,
///   one pair per holding — and did it with the process-wide [`unservable_chain_classes`] guard
///   held. One `--palw-class-artifact` on a mount that can block in `stat` would then park the
///   producer's registry and the panel's registry (one static, shared) behind a syscall that does
///   not return. This node has watched that shape wedge a public node for 46 minutes while systemd
///   still called it active; it does not belong under a global lock, and it does not belong on
///   this path at all.
/// * The first form also retracted the mark on any `len`/`mtime` movement, so an rsync in place, a
///   backup, or a `touch` dropped every remembered refusal in the process — and each subsequent
///   duty re-paid the compile SA-2 exists to prevent, only to re-derive the identical refusal,
///   because `load_class_holdings_v1` runs once per service construction and the MAPPING had not
///   moved. Retraction now keys on the thing that would actually change the answer.
///
/// **What this costs, said plainly:** replacing an artifact file at a configured path no longer
/// retracts anything. It never usefully did — the running service still dispatches against the
/// mapping it loaded at startup, so the re-derived verdict was the same sentence — and the honest
/// version of "supply the artifact and the refusal goes away" is and always was a restart, or an
/// eviction (which drops the holdings and clears this map in one move).
///
/// The payload address cannot go stale under us: a holding is released only by
/// [`evict_held_artifacts_v1`] / [`evict_all_held_artifacts_v1`], and both clear this map in the
/// same call — so no mark can outlive the allocation its address names.
fn holdings_identity_v1(holdings: &[PalwLoadedArtifactV1]) -> HoldingsIdentityV1 {
    holdings
        .iter()
        .map(|h| {
            let payload = std::sync::Arc::as_ptr(&h.payload()) as *const () as usize;
            (h.lineage_id, h.path.clone(), h.summary.clone(), payload)
        })
        .collect()
}

fn remembered_unservable(class_id: Hash64, artifact_root: Hash64, holdings: &[PalwLoadedArtifactV1]) -> Option<String> {
    // Read the mark under the lock and compare outside it. Nothing between these two guards
    // blocks, and nothing under either one calls back into this module.
    let mark = {
        let marks = unservable_chain_classes().lock().unwrap_or_else(|p| p.into_inner());
        marks.get(&(class_id, artifact_root))?.clone()
    };
    if mark.against == holdings_identity_v1(holdings) {
        return Some(mark.why);
    }
    // The holdings this verdict was about are not the holdings in hand. Drop it rather than serve
    // it: re-deriving costs one compile, and answering from it costs correctness. A racing caller
    // that inserted a fresh mark for this key in between loses it and pays that one compile again
    // — the conservative direction, and the only one available without holding the lock across the
    // comparison.
    warn!("[palw] the remembered refusal for class {class_id} was computed against different holdings; re-deriving it");
    unservable_chain_classes().lock().unwrap_or_else(|p| p.into_inner()).remove(&(class_id, artifact_root));
    None
}

fn remember_unservable(class_id: Hash64, artifact_root: Hash64, why: &str, holdings: &[PalwLoadedArtifactV1]) {
    warn!("[palw] class {class_id} is unservable on this node and will not be recompiled while it holds these artifacts: {why}");
    // Built before the lock is taken, for the same reason the read compares outside it.
    let against = holdings_identity_v1(holdings);
    unservable_chain_classes()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert((class_id, artifact_root), UnservableMarkV1 { why: why.to_string(), against });
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
    // The negative verdicts go with them. This is the SECOND door, not the only one: a mark is
    // already checked against its holdings at every read (see `unservable_chain_classes`), so an
    // operator flush does not have to be the thing that keeps the map honest — which is just as
    // well, since nothing in a running node calls this.
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
        // The one place this module touches the filesystem for a holding, so it is the one place
        // the round-3 lock probe has to sit. See `fs_probe`.
        #[cfg(test)]
        fs_probe::record();
        let meta = std::fs::metadata(path).ok()?;
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        Some(Self { path, len: meta.len(), modified: meta.modified().ok() })
    }
}

/// **The instrument for "no syscall runs under the global mark lock" (round-3 defect I-1).**
///
/// Counts every stat this thread performs for a holding, and how many of them happened while
/// SOMEBODY held [`unservable_chain_classes`]. `try_lock` reporting `WouldBlock` is that
/// "somebody" — a `std::sync::Mutex` is not reentrant, so a caller that stats while holding its
/// own guard sees exactly this, which is the failure the probe exists to catch. A POISONED lock
/// is not a held lock and is not counted.
///
/// Thread-local on purpose: these tests share a process with tests that map artifacts, and a
/// process-wide counter would be measuring them too.
#[cfg(test)]
mod fs_probe {
    use std::cell::Cell;

    thread_local! {
        static STATS: Cell<usize> = const { Cell::new(0) };
        static UNDER_LOCK: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn record() {
        STATS.with(|c| c.set(c.get() + 1));
        if matches!(super::unservable_chain_classes().try_lock(), Err(std::sync::TryLockError::WouldBlock)) {
            UNDER_LOCK.with(|c| c.set(c.get() + 1));
        }
    }

    pub(super) fn reset() {
        STATS.with(|c| c.set(0));
        UNDER_LOCK.with(|c| c.set(0));
    }

    pub(super) fn stats() -> usize {
        STATS.with(|c| c.get())
    }

    pub(super) fn under_lock() -> usize {
        UNDER_LOCK.with(|c| c.get())
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
/// **Re-derive every root in every sidecar, and refuse to start if one disagrees**
/// (`--palw-verify-class-manifest`).
///
/// A manifest is a cache for a value that must not be typed by a human, and a cache nobody ever
/// checks is how a wrong value survives a rebuild. Twice now, a flat artifact digest was pinned where
/// the operand-inventory root belonged and a network's dense tier produced zero blocks — the second
/// time over a byte-identical artifact, with every seat reporting the mismatch every thirty seconds
/// and nobody able to see WHICH value the node derived.
///
/// So this returns the sentence rather than logging it, and its caller refuses to start: a node that
/// serves a sidecar it did not write should find out at startup, while an operator is watching, and
/// not at the moment a claim's openings fail to verify.
///
/// Costs one streamed walk per class per artifact — 135 s at a 2,097,152 context, measured — which is
/// why it is a flag and not the default. Absent sidecars are not failures: a node with none derives
/// every root it is asked for, which is correct and slower.
/// **This process's `--ram-scale`**, armed once by the daemon so the memory decomposition can name
/// the caches' declared budget without every caller carrying the figure.
static ARMED_RAM_SCALE: std::sync::OnceLock<f64> = std::sync::OnceLock::new();

pub fn arm_ram_scale_v1(scale: f64) {
    let _ = ARMED_RAM_SCALE.set(scale);
}

fn armed_ram_scale_v1() -> f64 {
    *ARMED_RAM_SCALE.get().unwrap_or(&1.0)
}

/// The armed `--ram-scale`, for a caller outside this module that prints a memory phase.
pub fn armed_ram_scale_pub_v1() -> f64 {
    armed_ram_scale_v1()
}

/// **Whether `--palw-verify-class-manifest` was given**, set once by the daemon.
///
/// A process-global rather than a parameter threaded through two service constructors: holdings are
/// materialized in exactly one place ([`load_class_holdings_v1`], which caches process-wide), so the
/// check belongs there, and both services must be held to the same answer. A flag that only one of
/// them read would be a node that verifies its panel's artifacts and not its producer's.
static VERIFY_CLASS_MANIFESTS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Called once from the daemon, before any service is constructed.
pub fn arm_class_manifest_verification_v1(on: bool) {
    let _ = VERIFY_CLASS_MANIFESTS.set(on);
}

fn class_manifest_verification_armed_v1() -> bool {
    *VERIFY_CLASS_MANIFESTS.get().unwrap_or(&false)
}

/// **What a role of a class needs on this node** — the holding's incremental bytes beside the
/// role's resource profile, kept apart so a log line can say which is which (the confusion the
/// 3.17-vs-16 GiB estimate was made of). `profile` is `None` for a family that cannot derive one,
/// and `total_bytes` then falls back to the scratch estimate the node used before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwRoleMemoryNeedV1 {
    pub role: PalwResourceRoleV1,
    pub holding_bytes: u64,
    /// Tables the backend derived at load and holds for the artifact's life (the dense tier's
    /// rotary table: 1.07 GiB at 2M). Resident already, so reported and not reserved.
    pub derived_bytes: u64,
    pub runtime: Option<kaspa_consensus_core::palw_resource_profile_v1::PalwRuntimeProfileV1>,
    pub profile: Option<kaspa_consensus_core::palw_resource_profile_v1::PalwResourceProfileV1>,
}

impl PalwRoleMemoryNeedV1 {
    /// The figure a gate compares and the ledger reserves.
    pub fn total_bytes(&self) -> u64 {
        let working_set = self.profile.map(|p| p.working_set_bytes()).unwrap_or(PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1);
        self.holding_bytes.saturating_add(working_set)
    }

    /// The artifact's resident bytes: its file (or its pinned residency) plus the derived tables.
    pub fn artifact_resident_bytes(&self) -> u64 {
        self.holding_bytes.saturating_add(self.derived_bytes)
    }

    /// The decomposition, for a hold line or a telemetry field.
    pub fn describe(&self) -> String {
        match self.profile {
            Some(p) => format!(
                "{:.2} GiB as {} (artifact {:.2} GiB + K/V {:.2} GiB at {} rows{} + attention scratch {:.2} GiB + trace scratch {:.2} GiB \
                 + capture {:.2} GiB ({:?}) + checkpoint leg {:.2} GiB{}) under {}",
                gib(self.total_bytes()),
                self.role.name(),
                gib(self.holding_bytes),
                gib(p.kv_resident_bytes),
                p.end_rows,
                if p.opening_bytes > 0 { format!(" + opening {:.2} GiB", gib(p.opening_bytes)) } else { String::new() },
                gib(p.attention_scratch_bytes),
                gib(p.trace_scratch_bytes),
                gib(p.capture_retained_bytes),
                p.capture,
                gib(p.checkpoint_leg_bytes),
                if p.retained_checkpoint_bytes > 0 {
                    format!(" + retained checkpoints {:.2} GiB", gib(p.retained_checkpoint_bytes))
                } else {
                    String::new()
                },
                self.runtime.map(|r| r.name()).unwrap_or("no runtime profile"),
            ),
            None => format!(
                "{:.2} GiB as {} (artifact {:.2} GiB + the {:.2} GiB scratch estimate; this family derives no resource profile)",
                gib(self.total_bytes()),
                self.role.name(),
                gib(self.holding_bytes),
                gib(PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1)
            ),
        }
    }
}

/// **The whole-capture refusal, asked before a reservation is priced** (DoS audit 2026-09-24, #4):
/// `base0_whole_capture_refusal_v1` at the cap `base0_materialize_cap_v1` derives from the
/// network's ladder and the capture's own profile — the one number every family's
/// `materialize_cap()` answers — so a held capture past it is refused by name here and never
/// reaches the ledger as a 62 GiB request that reads like a busy host.
pub fn palw_whole_capture_admits_v1(
    network_ladder: u64,
    class_ladder: u64,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    step_leaf_count: u64,
) -> Result<(), String> {
    let cap = misaka_palw_base0::fp_interval::base0_materialize_cap_v1(network_ladder, Some(profile));
    match misaka_palw_base0::fp_interval::base0_whole_capture_refusal_v1(step_leaf_count, cap, class_ladder) {
        None => Ok(()),
        Some(why) => Err(why),
    }
}

/// **A full-seat need, re-priced for a capture laid out whole** (DoS audit 2026-09-24, #4): the
/// capture term becomes `DenseTiles` at `profile`'s widest tile over `leaves` — the vector and the
/// tiles the whole-capture prover holds — and every other term (the K/V history, the scratch, the
/// holding) is the full seat's, because the re-execution that builds the tiles IS a full-seat run.
///
/// A family that derives no resource profile is priced fail-closed: the dense capture plus the
/// scratch estimate the node used before profiles existed, rather than the estimate alone — the
/// estimate is half a GiB and the capture is not.
pub fn palw_whole_capture_need_v1(
    mut need: PalwRoleMemoryNeedV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    leaves: u64,
) -> PalwRoleMemoryNeedV1 {
    use kaspa_consensus_core::palw_resource_profile_v1::{
        PalwCaptureRetentionV1, PalwResourceProfileV1, PalwRuntimeProfileV1, palw_dense_capture_bytes_v1, palw_profile_max_tile_len_v1,
    };
    let tile_len = palw_profile_max_tile_len_v1(profile);
    let capture = PalwCaptureRetentionV1::DenseTiles { tile_len };
    let capture_retained_bytes = palw_dense_capture_bytes_v1(leaves, tile_len);
    need.role = PalwResourceRoleV1::FullSeat;
    need.profile = Some(match need.profile {
        Some(p) => PalwResourceProfileV1 { capture, capture_retained_bytes, leaves, ..p },
        None => PalwResourceProfileV1 {
            runtime: need.runtime.unwrap_or(PalwRuntimeProfileV1::A16KvI32),
            role: PalwResourceRoleV1::FullSeat,
            attention_layers: 0,
            kv_dim: 0,
            heads: 0,
            resume_rows: 0,
            end_rows: 0,
            kv_resident_bytes: 0,
            checkpoint_bytes: 0,
            opening_bytes: 0,
            retained_checkpoint_bytes: 0,
            attention_scratch_bytes: 0,
            trace_scratch_bytes: PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1,
            capture,
            capture_retained_bytes,
            checkpoint_leg_bytes: 0,
            leaves,
            recurrence_layers: 0,
            gdn_state_bytes: 0,
        },
    });
    need
}

/// The per-(holdings, class, root, role, door) need figures — see `role_memory_need_v1`. The door
/// flag keeps a table-only miss (`false`) from answering for the chain arm (`true`).
type NeedMemoKey = (Vec<PathBuf>, Hash64, Hash64, PalwResourceRoleV1, bool);
fn replay_bytes_memo_v1() -> &'static std::sync::Mutex<std::collections::HashMap<NeedMemoKey, Option<PalwRoleMemoryNeedV1>>> {
    static MEMO: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<NeedMemoKey, Option<PalwRoleMemoryNeedV1>>>> =
        std::sync::OnceLock::new();
    MEMO.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub fn verify_class_manifests_v1(sdk: &PalwClassSdk, holdings: &[PalwLoadedArtifactV1]) -> Result<usize, String> {
    let mut checked = 0usize;
    // Artifacts with no sidecar. Collected rather than skipped: see the refusal at the end.
    let mut without: Vec<String> = Vec::new();
    for holding in holdings {
        let name = holding.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| holding.lineage_id.to_string());
        match misaka_palw_sdk::class_manifest::manifest_beside(holding) {
            misaka_palw_sdk::class_manifest::PalwManifestLookupV1::Absent => without.push(name.clone()),
            misaka_palw_sdk::class_manifest::PalwManifestLookupV1::Disagrees(why) => {
                return Err(format!("{name}: {why}"));
            }
            misaka_palw_sdk::class_manifest::PalwManifestLookupV1::Agrees(m) => {
                m.verify_against_the_artifact(sdk, holding).map_err(|e| format!("{name}: {e}"))?;
                checked += m.rows.len();
            }
        }
    }
    // **A verification that verified nothing is a refusal, not a pass.**
    //
    // This returned `Ok(0)` for a node whose artifacts have no sidecar, and the caller logged
    // "0 registered root(s) re-derived … all agreeing" — measured on the acceptance run, where the
    // host simply had no `.palwmanifest` on it. An operator who ASKS to be told whether their roots
    // agree and is told "0, all agreeing" has learned nothing and been reassured, which is the
    // failure this whole flag exists to end, wearing a different costume.
    if !without.is_empty() {
        return Err(format!(
            "--palw-verify-class-manifest was given and {} of {} class artifact(s) have no `.palwmanifest` beside them, so \
             there was nothing to verify: {}. Write one with `palw-class manifest <artifact>`, or drop the flag and accept \
             that every root is derived at resolve time",
            without.len(),
            holdings.len(),
            without.join(", ")
        ));
    }
    if checked == 0 {
        return Err("--palw-verify-class-manifest was given and this node holds no class artifacts, so there was nothing to \
                    verify. Drop the flag, or give the node the artifacts whose roots you meant to check"
            .to_string());
    }
    Ok(checked)
}

/// **Can this node ever produce for its `--palw-producer-class`?** (the 2026-09-23 route-matrix
/// audit's #1.) `Some(why)` only for a configuration that can NEVER produce, whatever the chain does:
///
/// * a model class and no class artifact held at all;
/// * a GENESIS class that EVERY held artifact's sidecar lists, each under a root other than the one
///   the class registered — the defect that left testnet-12's dense tier at zero blocks for a whole
///   deployment while every producer said only "holding".
///
/// `None` otherwise (the route-matrix re-audit's #4 and #6: only definitive verdicts). A sidecar is
/// written from THIS build's catalog, so one that carries no row for the class says nothing about a
/// class the chain registered permissionlessly (served through `--palw-chain-classes`), nor about a
/// row a newer build added — the SDK's own reader (`inventory_root_from_sidecar`) reads a missing
/// row as "derive", and so does this. A held artifact with no sidecar is judged at resolve time, as
/// before, because deciding its pairing would walk its inventory at startup. And one holding that
/// lists the class under the wrong root does not condemn another that may carry the right
/// conversion — adding the correct file beside the old one is the natural remedy, and the table
/// resolve picks whichever holding matches. The floor needs no artifact and is never refused here.
/// A post-genesis class's root is on the chain, not in the params, so its mismatch is a resolve-time
/// finding.
pub fn producer_class_unproducible_v1(
    class_id: &Hash64,
    holdings: &[PalwLoadedArtifactV1],
    params: &kaspa_consensus_core::config::params::Params,
) -> Option<String> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return None;
    };
    if *class_id == bundle.base_class_id {
        return None;
    }
    let registered_root = bundle.genesis_objects.iter().find_map(|object| match object {
        kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id: id, artifact_root, .. }
            if id == class_id =>
        {
            Some(*artifact_root)
        }
        _ => None,
    });
    let sidecar_rows: Vec<Option<(String, Hash64)>> = holdings
        .iter()
        .map(|holding| match misaka_palw_sdk::class_manifest::manifest_beside(holding) {
            misaka_palw_sdk::class_manifest::PalwManifestLookupV1::Agrees(manifest) => manifest
                .rows
                .iter()
                .find(|row| row.class_id.into_hash64() == *class_id)
                .map(|row| (row.model_id.clone(), row.inventory_root.into_hash64())),
            _ => None,
        })
        .collect();
    producer_class_verdict_v1(class_id, registered_root, &sidecar_rows)
}

/// [`producer_class_unproducible_v1`]'s verdict over what each held artifact's sidecar lists for the
/// class — `Some((model id, root))` for a sidecar that agrees with its file and carries the class's
/// row, `None` for everything that decides nothing (no sidecar, one that disagrees with its file, or
/// one without the row). One entry per holding; none at all is "nothing held".
fn producer_class_verdict_v1(
    class_id: &Hash64,
    registered_root: Option<Hash64>,
    sidecar_rows: &[Option<(String, Hash64)>],
) -> Option<String> {
    if sidecar_rows.is_empty() {
        return Some(format!(
            "class {class_id} is a model class and this node holds no class artifact (--palw-class-artifact names none, or \
             none loaded): there is nothing to run its inference on"
        ));
    }
    // A chain-registered class: its root is the chain's, read at resolve time.
    let root = registered_root?;
    let mut listed: Vec<String> = Vec::new();
    for row in sidecar_rows {
        match row {
            // One holding this node cannot judge, or one that carries the registered root: it may
            // produce, so nothing here is definitive.
            None => return None,
            Some((_, held)) if *held == root => return None,
            Some((model_id, held)) => listed.push(format!("{model_id} at {held}")),
        }
    }
    Some(format!(
        "class {class_id} is registered at genesis with root {root}, and every artifact this node holds for it has another \
         root (their sidecars, checked against the files: {}) — every claim would be refused as the wrong artifact, so this \
         node could never produce for it",
        listed.join(", ")
    ))
}

pub fn load_class_holdings_v1(
    role: &str,
    sdk: &PalwClassSdk,
    paths: &[PathBuf],
    bound_bytes: u64,
    residency: misaka_palw_sdk::PalwWeightResidencyV1,
) -> Vec<PalwLoadedArtifactV1> {
    // The scale this node was configured with, for the decomposition below. Read from the process's
    // own armed value rather than threaded through two service constructors, for the same reason the
    // manifest check is: both services must report the same arithmetic.
    let ram_scale = armed_ram_scale_v1();
    // Held across the whole load on purpose: the guarantee is "once", so a second duty asking
    // for a file the first is still hashing waits for that holding rather than starting its own
    // pass. Nothing under the lock calls back in. A load that panicked (the root pass on a file
    // that became unreadable) poisons the lock without corrupting the map, so a later duty takes
    // the map as it stands rather than losing every holding to one bad file.
    let mut held = held_artifacts().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // A default measured against the host is spent across the files this call maps: two classes
    // do not each take a fifth of the same spare bytes (ADR-0112 Decision 2, amended).
    let mut policy = residency;
    let (holdings, skipped) = sdk.load_artifacts_bounded_with_v1(paths, bound_bytes, |path| {
        held_or_load_locked(&mut held, role, path, |p| {
            let loaded = sdk.load_artifact_with(p, policy);
            if let (Ok(holding), misaka_palw_sdk::PalwWeightResidencyV1::FifthWithin(spare)) = (&loaded, policy)
                && let Some(stats) = misaka_palw_sdk::lineages::qwen36::residency_stats_of(holding)
            {
                policy = misaka_palw_sdk::PalwWeightResidencyV1::FifthWithin(spare.saturating_sub(stats.budget_bytes));
            }
            loaded
        })
    });
    for (path, why) in &skipped {
        warn!("[{role}] class artifact {} is not held: {why}", path.display());
    }
    // **Fail closed before anything is served.** The sentence goes to stdout as well as the log
    // because a node that refuses to start must say why where the operator is looking, and a startup
    // refusal that only appears in a log file is how an operator concludes the binary is broken.
    if class_manifest_verification_armed_v1() {
        match verify_class_manifests_v1(sdk, &holdings) {
            Ok(rows) => info!(
                "[{role}] --palw-verify-class-manifest: {rows} registered root(s) re-derived from the artifacts beside their \
                 manifests, all agreeing"
            ),
            Err(why) => {
                let sentence = format!(
                    "--palw-verify-class-manifest: {why}\n\nThis node will not start. A class manifest is how a registered \
                     inventory root stops being a value a human types, and one that disagrees with the artifact beside it would \
                     have this node answer \"yes, I can serve that class\" about weights that produce a different root — whose \
                     claims are slashable. Regenerate it with `palw-class manifest <artifact>`, or drop the sidecar to derive \
                     every root instead."
                );
                println!("{sentence}");
                error!("[{role}] {sentence}");
                std::process::exit(1);
            }
        }
    }
    // **The sidecar, said out loud at load** — once per artifact, where the operator is already
    // reading, rather than on a resolve that retries every thirty seconds.
    //
    // A `.palwmanifest` is what keeps an inventory root out of human hands, so whether one is present
    // and whether it agrees are facts about this node's ability to serve a registered class. Absent
    // is not an error: the node derives, streamed. Disagreeing is not an error either — it still
    // derives — but it is a line the operator must see, because a sidecar that describes a different
    // file will mislead whoever reads it next, and nothing on the resolve path fixes a stale file.
    for holding in &holdings {
        let name = holding.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| holding.lineage_id.to_string());
        match misaka_palw_sdk::class_manifest::manifest_beside(holding) {
            misaka_palw_sdk::class_manifest::PalwManifestLookupV1::Agrees(m) => info!(
                "[{role}] class manifest for {name}: {} class(es) with their inventory roots, digest {} — a registered root is                  read rather than walked (`palw-class manifest --check` re-derives them)",
                m.rows.len(),
                m.artifact_digest
            ),
            misaka_palw_sdk::class_manifest::PalwManifestLookupV1::Absent => info!(
                "[{role}] no class manifest beside {name}: this node will DERIVE each registered root it is asked about                  (streamed, but 135 s at a 2M context). Write one with `palw-class manifest {name}`"
            ),
            misaka_palw_sdk::class_manifest::PalwManifestLookupV1::Disagrees(why) => warn!(
                "[{role}] the class manifest beside {name} is NOT USED: {why}. This node derives every root instead, so it is                  correct and slow; the file on disk stays wrong until it is regenerated"
            ),
        }
    }
    // ADR-0112 Decision 8: the budget's arithmetic, printed where the operator reads the log —
    // and a warning when the host cannot hold what the budget pins, because a budget the kernel
    // reclaims is the page cache with extra steps.
    for holding in &holdings {
        let name = holding.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| holding.lineage_id.to_string());
        if let Some(declined) = misaka_palw_sdk::lineages::qwen36::residency_declined_of(holding) {
            warn!(
                "[{role}] residency for {name}: the page cache, as before ADR-0112 — this host could spare {:.2} GiB for it \
                 (what it has available less the node's own {:.0} GiB), under the class's floor of {:.2} GiB (a fifth would \
                 be {:.2} GiB); every draw will read through page faults. Free the host, or state --palw-class-resident-bytes \
                 (at least the floor) to hold it anyway",
                gib(declined.spare_bytes),
                gib(PALW_CLASS_NODE_RESERVE_BYTES_V1),
                gib(declined.floor_bytes),
                gib(declined.fifth_bytes)
            );
            continue;
        }
        let Some(stats) = misaka_palw_sdk::lineages::qwen36::residency_stats_of(holding) else { continue };
        let available = host_available_bytes_v1();
        info!(
            "[{role}] residency for {name}: budget {:.2} GiB = {:.2} GiB pinned (the always-set) + {:.2} GiB for routed experts \
             (about {:.1} tokens of them); the host reports {} available (ADR-0112)",
            gib(stats.budget_bytes),
            gib(stats.pinned_bytes),
            gib(stats.expert_budget_bytes()),
            stats.expert_budget_bytes() as f64 / stats.token_expert_bytes.max(1) as f64,
            available.map_or("no available memory on this platform".to_string(), |a| format!("{:.2} GiB", gib(a)))
        );
        if let Some(available) = available
            && stats.budget_bytes > available
        {
            warn!(
                "[{role}] the residency budget ({:.2} GiB) is more than this host has available ({:.2} GiB): the kernel will \
                 reclaim what the budget pins and every draw will page — lower --palw-class-resident-bytes, or free the host",
                gib(stats.budget_bytes),
                gib(available)
            );
        }
    }
    if !holdings.is_empty() {
        info!("[{role}] memory after loading {} class artifact(s): {}", holdings.len(), process_memory_line_v1());
        // The decomposed form beside it, because "after loading" is the phase the fleet died in and the
        // one-line form does not separate the caches from the rest of the anonymous memory.
        log_memory_phase_v1(role, "after the class artifacts loaded", ram_scale);
    }
    holdings
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

/// **What a node holds for itself, reserved before a default budget is taken** (ADR-0112
/// Decision 2, amended 2026-09-11). The fleet's nodes held 9–14 GB of anonymous memory each before
/// any class budget (ADR-0112 §1), and at startup — when the default is measured — they hold
/// almost none of it yet; the reserve is that working set, rounded up.
pub const PALW_CLASS_NODE_RESERVE_BYTES_V1: u64 = 16 << 30;

/// **The residency policy from the operator's flag** (ADR-0112 Decision 2): `0` is no loader, the
/// page cache; a number is the number; nothing said is the default, measured against this host
/// ([`palw_class_default_residency_v1`]).
pub fn palw_class_residency_v1(resident_bytes: Option<u64>) -> misaka_palw_sdk::PalwWeightResidencyV1 {
    palw_class_residency_within_share_v1(resident_bytes, None)
}

/// **The residency policy, measured against this process's SHARE of the host rather than the host**
/// (ADR-0151 follow-up item 4).
///
/// `share` is `--palw-host-memory-budget / --palw-host-node-count`, or `None` when the operator
/// declared no budget — in which case this is exactly [`palw_class_residency_v1`]'s old behaviour and
/// every node on the host still sizes itself as though it were alone. That default is kept, rather
/// than guessed at, because a budget is a fact only the operator has: a node cannot count its
/// siblings without racing them, and the loser of that race is the process the kernel kills.
///
/// A STATED `--palw-class-resident-bytes` is still the number, share or no share. An operator who
/// names a figure has made the division themselves, and silently shrinking it would make the flag
/// mean something other than what it says.
pub fn palw_class_residency_within_share_v1(
    resident_bytes: Option<u64>,
    share: Option<u64>,
) -> misaka_palw_sdk::PalwWeightResidencyV1 {
    palw_class_residency_beside_the_attempt_v1(resident_bytes, share, 0)
}

/// **The residency policy with the ATTEMPT carved out of the share first** (ADR-0151 follow-up,
/// item 3's other half).
///
/// On ibm, 2026-09-23: a 7 GiB share, and `residency … budget 6.65 GiB = 1.86 GiB pinned + 4.79 GiB
/// for routed experts` — anonymous, i.e. the residency took the whole share and the attempt that
/// followed had nothing, whatever the gate said. `attempt_reserve` is what this node's producer
/// class needs for one attempt (`palw_producer_attempt_reserve_v1`: the class's resource profile,
/// folded), and it comes off the share BEFORE the fifth is measured against it, so the weights are
/// budgeted from what the attempt leaves. Zero for a node that produces nothing.
pub fn palw_class_residency_beside_the_attempt_v1(
    resident_bytes: Option<u64>,
    share: Option<u64>,
    attempt_reserve: u64,
) -> misaka_palw_sdk::PalwWeightResidencyV1 {
    use misaka_palw_sdk::PalwWeightResidencyV1 as Residency;
    match resident_bytes {
        None => match share {
            // The share IS the headroom: it already excludes the other nodes, so the host-wide
            // reserve is not subtracted a second time — that reserve exists to keep a node's own
            // non-weight memory out of the weights' budget, and a share divides the same host. The
            // attempt's reserve IS subtracted: it is memory this process will allocate beside the
            // weights, and a residency that ignored it would be sized for a node that never produces.
            Some(share) => palw_class_default_residency_v1(Some(
                share.saturating_sub(attempt_reserve).saturating_add(PALW_CLASS_NODE_RESERVE_BYTES_V1),
            )),
            None => palw_class_default_residency_v1(host_available_bytes_v1()),
        },
        Some(0) => Residency::PageCache,
        Some(bytes) => Residency::Bytes(bytes),
    }
}

/// **What one attempt of this node's producer class needs, from the class table alone** — before
/// any artifact is loaded, so the residency can be carved around it. The class's profile and
/// canonical job come from the genesis table (`canonical_classes_v1`), the capture is the lane's
/// (a held class folds), the representation is the family's shipped one, and the leaf count is
/// priced at the class's own ladder. Zero for no producer class, an unknown one, or a class the
/// derivation cannot price — a producer that then holds is the gate's business, not this carve's.
pub fn palw_producer_attempt_reserve_v1(court: &PalwCourtParamsV2, producer_class: Option<Hash64>) -> u64 {
    use kaspa_consensus_core::palw_resource_profile_v1::{
        PalwCaptureRetentionV1, PalwResourceRoleV1, PalwRuntimeLimitsV1, PalwRuntimeProfileV1, palw_attempt_capture_folds_v1,
        palw_profile_max_tile_len_v1, palw_resource_profile_v1,
    };
    let Some(class_id) = producer_class else { return 0 };
    let classes = misaka_palw_base0::classes::canonical_classes_v1(court);
    let Some(entry) = classes.iter().find(|c| c.profile.shape_profile_id() == class_id) else { return 0 };
    let profile = &entry.profile;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, entry.canonical_job.0, entry.canonical_job.1);
    let ladder = kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(court.max_step_leaf_count(), profile);
    let Ok(leaf_count) = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, &job, ladder) else { return 0 };
    let hybrid = kaspa_consensus_core::palw_resource_profile_v1::palw_recurrence_layers_v1(profile) > 0;
    let runtime = if hybrid { PalwRuntimeProfileV1::Q36KvI32 } else { misaka_palw_base0::engine_a16::KV_STORAGE_SHIPPED_V1 };
    let capture = if palw_attempt_capture_folds_v1(profile) {
        PalwCaptureRetentionV1::Fold {
            retain_level: misaka_palw_base0::fp_capture::palw_base0_sparse_retain_level_for_class_v1(profile, ladder),
        }
    } else {
        PalwCaptureRetentionV1::DenseTiles { tile_len: palw_profile_max_tile_len_v1(profile) }
    };
    // The same run-length rule each backend's own derivation applies: the dense engine walks the
    // prefill `A16_PREFILL_RUN_POSITIONS` at a time; the hybrid engine walks a per-position class one
    // position at a time and a per-call class in one pass over the prefill.
    let run_positions = if !hybrid {
        misaka_palw_base0::qwen25_a16_backend::A16_PREFILL_RUN_POSITIONS as u32
    } else if kaspa_consensus_core::palw_state_chunk_map::palw_map_addresses_history_tiles_v1(profile) {
        1
    } else {
        job.declared_prefill_tokens.max(1)
    };
    let limits = PalwRuntimeLimitsV1 { threads: std::thread::available_parallelism().map_or(1, |n| n.get() as u32), prefill_run_positions: run_positions };
    palw_resource_profile_v1(profile, &job, leaf_count, runtime, PalwResourceRoleV1::Producer, limits, capture)
        .map(|p| p.working_set_bytes())
        .unwrap_or(0)
}

/// **The floor a share must clear to run a node at all**, beyond any class it holds: the consensus
/// stores, RocksDB's caches and the node's own working set at the smallest `--ram-scale` the daemon
/// accepts (0.1).
///
/// Measured rather than chosen: on the t12 fleet a seat that held the 2M artifact and served the panel
/// sat at 2.55–3.92 GiB with `--ram-scale=0.25`, of which 1.06 GiB was anonymous and 0.81–2.69 GiB was
/// the artifact's file-backed pages shared with its siblings. 2 GiB is the anonymous half with room,
/// and it is a FLOOR — a share that clears it is not thereby comfortable.
pub const PALW_HOST_SHARE_FLOOR_BYTES_V1: u64 = 2 << 30;

/// **Refuse a share that cannot run a node** (ADR-0151 follow-up item 4).
///
/// The point of a declared budget is that over-commitment becomes a statement the operator can be
/// held to instead of a race the kernel settles. That only works if an impossible division is refused:
/// `--palw-host-memory-budget=24GiB --palw-host-node-count=20` is not a cautious operator, it is a
/// host that will thrash, and the honest moment to say so is before anything is served.
///
/// `Ok` for no declared budget — that is the old per-process behaviour, which this does not tighten
/// behind the operator's back.
pub fn check_host_share_v1(budget: Option<u64>, node_count: u32, share: Option<u64>) -> Result<(), String> {
    let (Some(budget), Some(share)) = (budget, share) else { return Ok(()) };
    if share >= PALW_HOST_SHARE_FLOOR_BYTES_V1 {
        return Ok(());
    }
    Err(format!(
        "--palw-host-memory-budget {:.2} GiB divided between {node_count} node(s) leaves {:.2} GiB for this one, under the \
         {:.2} GiB a node needs for its consensus stores and caches before it holds any class weights at all. Raise the \
         budget, lower --palw-host-node-count, or run fewer nodes on this host — a division this thin does not make the \
         memory go further, it makes every node page.",
        gib(budget),
        gib(share),
        gib(PALW_HOST_SHARE_FLOOR_BYTES_V1)
    ))
}

/// **The host-budget arithmetic, printed once at startup.**
///
/// The failure this exists to make impossible was invisible: four seats each logged a residency budget
/// that was individually reasonable, and nothing anywhere said that the four of them summed past the
/// host. A line that names the budget, the count, the share and the derived scale is the difference
/// between an operator who can see the over-commitment and one who finds out from `dmesg`.
pub fn report_host_memory_budget_v1(budget: Option<u64>, node_count: u32, share: Option<u64>, ram_scale: f64) {
    match (budget, share) {
        // A share stated outright, with or without a budget beside it.
        (_, Some(share)) if budget.is_none_or(|b| b / node_count.max(1) as u64 != share) => info!(
            "[palw-host] memory share for this node stated outright: {:.2} GiB (--palw-host-memory-share); --ram-scale \
             {ram_scale:.3} and the class residency budget follow it, not the host's free memory. The host reports {} \
             available right now; the shares on this host must sum to what it has, and the reservation ledger's live \
             bound is the backstop when they do not",
            gib(share),
            host_available_bytes_v1().map_or("no figure on this platform".to_string(), |a| format!("{:.2} GiB", gib(a)))
        ),
        (Some(budget), Some(share)) => info!(
            "[palw-host] memory budget {:.2} GiB across {node_count} node(s) on this host = {:.2} GiB for this one;              --ram-scale {ram_scale:.3} and the class residency budget both follow that share, not the host's free memory.              The host reports {} available right now",
            gib(budget),
            gib(share),
            host_available_bytes_v1().map_or("no figure on this platform".to_string(), |a| format!("{:.2} GiB", gib(a)))
        ),
        _ => info!(
            "[palw-host] no --palw-host-memory-budget: this node sizes its caches and its class residency against the WHOLE              host's free memory, which is correct for one node and over-commits by a factor of N for N of them on one host.              Declare a budget and --palw-host-node-count when this host runs more than one"
        ),
    }
}



/// **The default, given what the host has available** (ADR-0112 Decision 2, amended 2026-09-11):
/// a fifth of the weights within what is available less the node's own reserve — taken whole, in
/// part, or, below the class's floor, not at all (the page cache, as before ADR-0112, and a
/// warning that says so). Where the platform reports no available memory (macOS), the fifth.
///
/// Measured on the fleet the day the ADR landed: every host 23 GB, `ibm` running two nodes with
/// 6 of 7 GB of swap in use, `.113` with all 15 GB of its swap in use and three pool slots at
/// their 6 GiB cgroup limit. A fixed fifth is 6.65 GiB of anonymous memory — which the kernel
/// cannot reclaim the way it reclaims the page cache's pages — on hosts that had none to give.
pub fn palw_class_default_residency_v1(available: Option<u64>) -> misaka_palw_sdk::PalwWeightResidencyV1 {
    use misaka_palw_sdk::PalwWeightResidencyV1 as Residency;
    match available {
        Some(available) => Residency::FifthWithin(available.saturating_sub(PALW_CLASS_NODE_RESERVE_BYTES_V1)),
        None => Residency::FifthOfTheWeights,
    }
}

/// **What this process can take**: the host's `MemAvailable`, or its memory cgroup's headroom
/// where that is smaller — a pool slot in a 6 GiB cgroup on a host with 8 GB available has 6 GiB
/// less what it holds, not 8. `None` off Linux.
pub fn host_available_bytes_v1() -> Option<u64> {
    let host = mem_available_bytes_v1()?;
    Some(cgroup_headroom_bytes_v1().map_or(host, |cgroup| cgroup.min(host)))
}

fn cgroup_headroom_bytes_v1() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let own = std::fs::read_to_string("/proc/self/cgroup").ok()?;
        cgroup_headroom_from_v1(&own, |path| std::fs::read_to_string(path).ok())
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// The smallest `limit − usage` over a process's memory cgroup and every ancestor that sets a
/// limit, from `/proc/self/cgroup`'s text and a reader for the cgroup filesystem. cgroup v2's
/// unified `0::<path>` reads `memory.max` (`max` is no limit) and `memory.current`; failing that,
/// v1's `memory` controller reads `memory.limit_in_bytes` (its no-limit is a page-rounded
/// `i64::MAX`) and `memory.usage_in_bytes`. `None` when nothing on the way up sets a limit.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn cgroup_headroom_from_v1(proc_self_cgroup: &str, read: impl Fn(&Path) -> Option<String>) -> Option<u64> {
    let number = |path: PathBuf| read(&path).and_then(|text| text.trim().parse::<u64>().ok());
    if let Some(rel) = proc_self_cgroup.lines().find_map(|line| line.strip_prefix("0::")) {
        let root = Path::new("/sys/fs/cgroup");
        let mut dir = root.join(rel.trim().trim_start_matches('/'));
        let mut least: Option<u64> = None;
        while dir.starts_with(root) && dir != root {
            if let (Some(max), Some(current)) = (number(dir.join("memory.max")), number(dir.join("memory.current"))) {
                let headroom = max.saturating_sub(current);
                least = Some(least.map_or(headroom, |l| l.min(headroom)));
            }
            if !dir.pop() {
                break;
            }
        }
        if least.is_some() {
            return least;
        }
    }
    let rel = proc_self_cgroup.lines().find_map(|line| {
        let mut parts = line.splitn(3, ':');
        let (_, controllers, path) = (parts.next()?, parts.next()?, parts.next()?);
        controllers.split(',').any(|c| c == "memory").then_some(path)
    })?;
    let dir = Path::new("/sys/fs/cgroup/memory").join(rel.trim().trim_start_matches('/'));
    let (limit, usage) = (number(dir.join("memory.limit_in_bytes"))?, number(dir.join("memory.usage_in_bytes"))?);
    (limit < 1 << 62).then(|| limit.saturating_sub(usage))
}

/// `MemAvailable` of this host in bytes, where the kernel says it (Linux); `None` elsewhere.
/// **What this process holds, as the kernel accounts it** (`/proc/self/smaps_rollup`, Linux):
/// proportional set size split into anonymous and file-backed pages, and what is swapped out.
/// The number that tells a mapped artifact from a copied one is `pss_file` against `pss_anon`:
/// seven seats sharing one mapped file show it once in `pss_file` each (divided by the sharers)
/// and nothing in `pss_anon`; seven copies show 1.8 GiB of `pss_anon` each. `None` off Linux.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwProcessMemoryV1 {
    pub rss_bytes: u64,
    pub pss_bytes: u64,
    pub pss_anon_bytes: u64,
    pub pss_file_bytes: u64,
    pub swap_bytes: u64,
}

pub fn process_memory_v1() -> Option<PalwProcessMemoryV1> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/self/smaps_rollup").ok()?;
        let field = |name: &str| -> u64 {
            text.lines()
                .find_map(|line| {
                    let rest = line.strip_prefix(name)?.trim();
                    rest.strip_suffix("kB")?.trim().parse::<u64>().ok().map(|kb| kb.saturating_mul(1024))
                })
                .unwrap_or(0)
        };
        Some(PalwProcessMemoryV1 {
            rss_bytes: field("Rss:"),
            pss_bytes: field("Pss:"),
            pss_anon_bytes: field("Pss_Anon:"),
            pss_file_bytes: field("Pss_File:"),
            swap_bytes: field("Swap:"),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// One line of [`process_memory_v1`] for a log, or the reason there is none.
pub fn process_memory_line_v1() -> String {
    match process_memory_v1() {
        Some(m) => format!(
            "PSS {:.2} GiB = {:.2} GiB anonymous + {:.2} GiB file-backed (shared with every process that maps the same \
             files); swapped out {:.2} GiB",
            gib(m.pss_bytes),
            gib(m.pss_anon_bytes),
            gib(m.pss_file_bytes),
            gib(m.swap_bytes)
        ),
        None => "process memory accounting is Linux-only here".to_string(),
    }
}

/// **A producer's memory, decomposed and named, at a named phase** (ADR-0151 follow-up item 5).
///
/// The reason this exists: a testnet-12 seat reached 11.5 GiB of anonymous memory and was
/// OOM-killed, and the only figure anyone had was that 11.5. `RSS` alone names no culprit — it does
/// not say whether the growth was the artifact, the consensus caches, a class's KV/context, an
/// operand inventory, RocksDB, or allocator fragmentation — so the diagnosis took a bespoke probe and
/// a rebuild. Every one of those is separable, and most of them are already known to the process.
///
/// What each column is, and how far to trust it:
///
/// * `rss` / `pss` / `anon` / `file` / `swap` — measured, from `smaps_rollup`. `pss` divides
///   shared pages by their sharers, which is the honest figure on a host running four seats over ONE
///   mapped artifact: four processes each reporting the artifact's 2.68 GiB in `rss` would triple-count
///   the host's memory, and `file` in `pss` does not.
/// * `caches` — DECLARED, [`kaspa_consensus::consensus::storage::declared_cache_budget_bytes_v1`] at
///   this node's `--ram-scale`. A bound, not an occupancy, and allocator overhead sits on top of it.
/// * `unaccounted` — `anon` less `caches`. The residue is where a class's KV/context, an operand
///   inventory walk and fragmentation live, and naming it as a residue is the point: a number with a
///   name gets investigated, and a subtraction the reader has to perform does not.
///
/// A phase string rather than a timestamp, because the question is always "between which two points
/// did it grow".
pub fn log_memory_phase_v1(role: &str, phase: &str, ram_scale: f64) {
    let caches = kaspa_consensus::consensus::storage::declared_cache_budget_bytes_v1(ram_scale);
    match process_memory_v1() {
        Some(m) => {
            let unaccounted = m.pss_anon_bytes.saturating_sub(caches);
            info!(
                "[{role}] memory at {phase}: rss {:.2} / pss {:.2} GiB = anon {:.2} + file {:.2} (shared per sharer);                  swap {:.2}; consensus caches declared {:.2} at --ram-scale {ram_scale:.3}; unaccounted anon {:.2} GiB                  (a class's KV/context, an operand inventory walk, allocator fragmentation)",
                gib(m.rss_bytes),
                gib(m.pss_bytes),
                gib(m.pss_anon_bytes),
                gib(m.pss_file_bytes),
                gib(m.swap_bytes),
                gib(caches),
                gib(unaccounted)
            );
        }
        None => info!(
            "[{role}] memory at {phase}: per-process accounting is Linux-only here; consensus caches declared {:.2} GiB at              --ram-scale {ram_scale:.3}",
            gib(caches)
        ),
    }
}

/// **The host memory a replay must leave alone**, and the fraction of the rest it may take.
/// Node-local policy, never consensus: a seat that starts a replay its host cannot hold does not
/// finish it, and a host driven into swap finishes nothing (measured 2026-09-17: seven seats on a
/// 12 GiB host, 8 GiB of swap filled in minutes, the OOM killer took a node). Swap is a crash
/// guard, not capacity, so only `MemAvailable` counts.
pub const PALW_REPLAY_HOST_RESERVE_BYTES_V1: u64 = 1 << 30;
pub const PALW_REPLAY_BUDGET_PERMILLE_V1: u64 = 700;
/// What one dense replay adds on top of the artifact's own pages until it is measured: caches,
/// retained logits rows and the trace. Half a GiB is the round number above the measured floor
/// (0.9 GiB was the worst node-wide growth under seven concurrent claims, most of it the copied
/// artifact this build no longer makes).
pub const PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1: u64 = 512 << 20;

/// **What a replay of this holding will hold on this host.**
///
/// A Qwen3.6 mapping with ADR-0112 residency reports the budget it already pinned. Every other
/// holding — dense decode, or a mapping the page cache decides — reports the file, which is what
/// the kernel will page in.
pub fn holding_replay_bytes_v1(holding: &PalwLoadedArtifactV1) -> Option<u64> {
    if let Some(stats) = misaka_palw_sdk::lineages::qwen36::residency_stats_of(holding) {
        return Some(stats.budget_bytes);
    }
    holding.path.as_ref().and_then(|path| std::fs::metadata(path).ok()).map(|meta| meta.len())
}

/// **What a replay of this holding still has to take from MemAvailable.** A resident Qwen3.6
/// mapping has already pinned its budget; charging it again against leftover MemAvailable is the
/// 6.50-vs-3 GiB deferral on a host that just mapped the class. Page-cache and dense holdings
/// still report the file.
pub fn incremental_replay_bytes_v1(holding: &PalwLoadedArtifactV1) -> Option<u64> {
    if misaka_palw_sdk::lineages::qwen36::residency_stats_of(holding).is_some() {
        return Some(0);
    }
    holding_replay_bytes_v1(holding)
}

/// Whether a replay that needs `need_bytes` (the class's artifact plus its scratch) fits this
/// host's budget now: `need ≤ 70 % × (MemAvailable − reserve)`. `Ok` where the platform cannot say
/// (the node then behaves as before this policy); `Err` names the numbers.
pub fn replay_memory_budget_v1(need_bytes: u64) -> Result<(), String> {
    let Some(available) = host_available_bytes_v1() else { return Ok(()) };
    let budget = host_headroom_of_v1(available);
    if need_bytes <= budget {
        return Ok(());
    }
    Err(format!(
        "the host's replay budget is {:.2} GiB (70 % of {:.2} GiB available past a {:.2} GiB reserve; swap is not capacity) \
         and this class needs {:.2} GiB",
        gib(budget),
        gib(available),
        gib(PALW_REPLAY_HOST_RESERVE_BYTES_V1),
        gib(need_bytes)
    ))
}

/// The host's headroom for one more duty: `70 % × (available − reserve)` — the policy above, as
/// the number the reservation ledger's live bound reads (`palw_memory_ledger`).
fn host_headroom_of_v1(available: u64) -> u64 {
    available.saturating_sub(PALW_REPLAY_HOST_RESERVE_BYTES_V1).saturating_mul(PALW_REPLAY_BUDGET_PERMILLE_V1) / 1_000
}

/// The host's headroom now, in both readings the ledger chooses between (`palw_memory_ledger`'s
/// doc), or `None` where the platform cannot say.
pub fn host_headroom_v1() -> Option<crate::palw_memory_ledger::PalwHostHeadroomV1> {
    let available = host_available_bytes_v1()?;
    Some(crate::palw_memory_ledger::PalwHostHeadroomV1 {
        past_reserve: available.saturating_sub(PALW_REPLAY_HOST_RESERVE_BYTES_V1),
        haircut: host_headroom_of_v1(available),
    })
}

/// **Whether the reservation ledger could cover `need_bytes` now** — the dry run every
/// pre-check runs instead of reading `MemAvailable` on its own: a figure the ledger has already
/// promised to another duty is not available, whatever the kernel says (ADR-0151 follow-up,
/// item 3). `Err` is the ledger's own sentence, naming what is held and by whom.
pub fn ledger_admits_v1(need_bytes: u64) -> Result<(), String> {
    crate::palw_memory_ledger::host_ledger_v1().can_reserve(need_bytes).map_err(|refusal| refusal.to_string())
}

/// A line logged at most once a minute per key — a deferral that repeats every tick is one fact.
pub fn note_throttled_v1(key: &str, line: impl FnOnce() -> String) {
    static LAST: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>> =
        std::sync::OnceLock::new();
    let map = LAST.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut map = map.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = std::time::Instant::now();
    if map.get(key).is_some_and(|at| now.duration_since(*at) < std::time::Duration::from_secs(60)) {
        return;
    }
    map.insert(key.to_string(), now);
    warn!("{}", line());
}

fn mem_available_bytes_v1() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/meminfo").ok()?.lines().find_map(|line| {
            let rest = line.strip_prefix("MemAvailable:")?.trim();
            rest.strip_suffix("kB")?.trim().parse::<u64>().ok().map(|kb| kb.saturating_mul(1024))
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Bytes this process has caused to be read from the storage layer since it started
/// (`/proc/self/io`'s `read_bytes`, Linux) — page faults included, which is what made it the
/// number that explained the fleet's draws. `None` off Linux, and the line then says so rather
/// than printing a zero.
pub fn process_storage_read_bytes_v1() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/io")
            .ok()?
            .lines()
            .find_map(|line| line.strip_prefix("read_bytes:")?.trim().parse::<u64>().ok())
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// What one draw starts from: the process's storage counter, and each mapped holding's residency
/// numbers (ADR-0112 Decision 8).
pub struct PalwStorageSnapshotV1 {
    process_read_bytes: Option<u64>,
    holdings: Vec<(String, misaka_palw_base0::qwen36::Qwen36ResidencyStatsV1)>,
}

pub fn storage_snapshot_v1(holdings: &[PalwLoadedArtifactV1]) -> PalwStorageSnapshotV1 {
    PalwStorageSnapshotV1 {
        process_read_bytes: process_storage_read_bytes_v1(),
        holdings: holdings
            .iter()
            .filter_map(|h| {
                let stats = misaka_palw_sdk::lineages::qwen36::residency_stats_of(h)?;
                let name = h.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| h.lineage_id.to_string());
                Some((name, stats))
            })
            .collect(),
    }
}

/// **The draw's storage line** (ADR-0112 Decision 8): what the process read from storage during
/// the draw, and what the class loader read of it — with the loader's hit rate and what it holds.
/// The number the fleet lacked: 12.8 GiB through 3 million page faults a draw was found with a
/// sampler, not a log.
pub fn log_draw_storage_v1(role: &str, before: &PalwStorageSnapshotV1, holdings: &[PalwLoadedArtifactV1]) -> Option<u64> {
    let after = storage_snapshot_v1(holdings);
    let mib = |bytes: u64| bytes as f64 / (1u64 << 20) as f64;
    // ADR-0132: the number the telemetry keeps, in whole MiB, where the platform counts it.
    let process_mib = match (before.process_read_bytes, after.process_read_bytes) {
        (Some(a), Some(b)) => Some(b.saturating_sub(a) >> 20),
        _ => None,
    };
    let process = match (before.process_read_bytes, after.process_read_bytes) {
        (Some(a), Some(b)) => format!("{:.1} MiB", mib(b.saturating_sub(a))),
        _ => "an amount this platform does not count".to_string(),
    };
    if after.holdings.is_empty() {
        info!("[{role}] this draw read {process} from storage (no mapped class holds a residency: the page cache decides)");
        return process_mib;
    }
    for (name, now) in &after.holdings {
        let then = before.holdings.iter().find(|(n, _)| n == name).map(|(_, s)| *s).unwrap_or_default();
        let lookups = now.hits.saturating_sub(then.hits) + now.misses.saturating_sub(then.misses);
        info!(
            "[{role}] this draw read {process} from storage; the loader for {name} read {:.1} MiB in {} misses of {lookups} expert \
             lookups ({:.1} % hits), evicted {}, and holds {:.2} of {:.2} GiB of routed experts beside {:.2} GiB pinned",
            mib(now.bytes_read.saturating_sub(then.bytes_read)),
            now.misses.saturating_sub(then.misses),
            100.0 * now.hits.saturating_sub(then.hits) as f64 / lookups.max(1) as f64,
            now.evictions.saturating_sub(then.evictions),
            gib(now.resident_expert_bytes),
            gib(now.expert_budget_bytes()),
            gib(now.pinned_bytes)
        );
    }
    process_mib
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

    /// **The startup refusal's two cheap verdicts, on the network it was written for** (the route-matrix
    /// audit's #1): testnet-12's floor needs no artifact and is never refused; its 8k dense row with no
    /// artifact held is refused by name. (The sidecar verdicts need a real artifact on disk and are
    /// exercised where one exists: the drill host.)
    #[test]
    fn a_model_producer_class_with_nothing_to_run_it_on_is_refused_and_the_floor_never_is() {
        let t12 = kaspa_consensus_core::config::params::Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(
            kaspa_consensus_core::network::NetworkType::Testnet,
            12,
        ));
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
            panic!("testnet-12 is ConsensusV2")
        };
        assert_eq!(producer_class_unproducible_v1(&bundle.base_class_id, &[], &t12), None, "the floor needs no artifact");
        let dense_8k = kaspa_consensus_core::config::class_manifest_const_v1::class_id_of_class(
            kaspa_consensus_core::config::class_manifest_const_v1::QWEN25_A16_8K_MANIFEST_V1,
            1,
        );
        let why = producer_class_unproducible_v1(&dense_8k, &[], &t12).expect("a model class with no artifact cannot produce");
        assert!(why.contains("holds no class artifact"), "{why}");
        // A network with no ConsensusV2 bundle has nothing to judge against.
        let legacy = kaspa_consensus_core::config::params::Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(
            kaspa_consensus_core::network::NetworkType::Testnet,
            10,
        ));
        assert_eq!(producer_class_unproducible_v1(&dense_8k, &[], &legacy), None);
    }

    /// **Only definitive verdicts** (the route-matrix re-audit's #4 and #6). A sidecar that carries no
    /// row for the class decides nothing — a chain-registered class never appears in one, and an older
    /// build's sidecar lacks a newer row — and one holding with the wrong root does not condemn
    /// another that carries the right one. Refused: nothing held, or a genesis class every holding
    /// lists under another root.
    #[test]
    fn the_startup_refusal_gives_only_definitive_verdicts() {
        let class = Hash64::from_u64_word(0xC1A5);
        let (genesis_root, wrong_root, other_wrong) =
            (Hash64::from_u64_word(0x600D), Hash64::from_u64_word(0xBAD), Hash64::from_u64_word(0xBAD2));
        let row = |root: Hash64| Some(("qwen25-graph-v7".to_string(), root));
        // Nothing held: definitive for a genesis class and a chain-registered one alike.
        assert!(producer_class_verdict_v1(&class, Some(genesis_root), &[]).is_some());
        assert!(producer_class_verdict_v1(&class, None, &[]).is_some());
        // #4: a sidecar with no row for the class is unknown, not "pairs with nothing".
        assert_eq!(producer_class_verdict_v1(&class, None, &[None]), None, "a chain-registered class is in no sidecar");
        assert_eq!(producer_class_verdict_v1(&class, Some(genesis_root), &[None]), None, "an older build's sidecar lacks the row");
        // A chain-registered class's root is the chain's: no sidecar root is compared against it.
        assert_eq!(producer_class_verdict_v1(&class, None, &[row(wrong_root)]), None);
        // The testnet-12 incident: the one artifact held lists the genesis class under another root.
        let why = producer_class_verdict_v1(&class, Some(genesis_root), &[row(wrong_root)]).expect("every holding is wrong");
        assert!(why.contains(&genesis_root.to_string()) && why.contains(&wrong_root.to_string()), "{why}");
        // #6: the correct conversion added beside the wrong one — whichever order — is not refused.
        assert_eq!(producer_class_verdict_v1(&class, Some(genesis_root), &[row(wrong_root), row(genesis_root)]), None);
        assert_eq!(producer_class_verdict_v1(&class, Some(genesis_root), &[row(genesis_root), row(wrong_root)]), None);
        // …nor beside a file with no sidecar, which the table resolve may still match.
        assert_eq!(producer_class_verdict_v1(&class, Some(genesis_root), &[row(wrong_root), None]), None);
        // Every holding wrong: refused, naming each.
        let why = producer_class_verdict_v1(&class, Some(genesis_root), &[row(wrong_root), row(other_wrong)]).expect("all wrong");
        assert!(why.contains(&wrong_root.to_string()) && why.contains(&other_wrong.to_string()), "{why}");
        assert!(!why.contains('\n'), "one line, so the log line carrying it is one line: {why}");
    }

    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court")
    }

    fn registry() -> PalwBackendRegistry {
        PalwBackendRegistry::new(
            court(),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            Vec::new(),
            b"misaka-palw-rc".to_vec(),
        )
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

        let bare = PalwBackendRegistry::new(
            bundle.court,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            Vec::new(),
            params.net.to_string().into_bytes(),
        );
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
            // **The dense slot is ADR-0082's graph-v5 512 row**, not the graph-v2 n_ctx-16 one it
            // replaced (5f genesis card §2). Named through the same projection the genesis
            // registration derives its class id from, so this table cannot come to describe a
            // class the network does not register — which is precisely what this test caught the
            // day the slot changed.
            kaspa_consensus_core::config::params::palw_rc_qwen25_a16_is_registered().then(|| {
                (
                    kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_profile_v1()
                        .expect("the graph-v5 dense row projects")
                        .shape_profile_id(),
                    kaspa_consensus_core::config::params::PALW_RC_GENESIS_QWEN25_A16_GRAPH_V5_ARTIFACT_ROOT,
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
                kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
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
        write_qwen36_fixture_with(path, layers, 8)
    }

    /// The same, with the routed expert count stated: ADR-0112's default budget is a fifth of
    /// the weights, and at the fixture's width only the class's own 256 experts a layer put a
    /// fifth above the floor.
    fn write_qwen36_fixture_with(path: &Path, layers: usize, experts: usize) {
        use misaka_palw_base0::qwen36::{Qwen36Writer, qwen36_dev_fixture};
        let owned = qwen36_dev_fixture(layers, experts);
        let plan: Vec<(String, usize)> =
            owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
        let mut writer =
            Qwen36Writer::create(path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("the file is created");
        for (name, _) in &plan {
            writer.push(name, &owned.tensor(name).expect("present")).expect("the tensor is appended");
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
        PalwClassSdk::builtin_v1(
            court(),
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            b"misaka-palw-rc".to_vec(),
        )
    }

    fn qwen36_parts(holding: &PalwLoadedArtifactV1) -> (Hash64, std::sync::Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>) {
        misaka_palw_sdk::lineages::qwen36::parts_of(holding).expect("a Qwen3.6 holding")
    }

    /// **ADR-0112 on the node's own door**: a holding loaded under a budget carries its
    /// residency — the summary names the budget, the stats say what is pinned — one loaded with
    /// the page cache carries none, and one whose default the host could not spare says why. The
    /// flag's spellings map as Decision 2 (amended 2026-09-11) says: `0` the page cache, a number
    /// the number, nothing a fifth within what is available less the node's reserve.
    #[test]
    fn a_budgeted_holding_reports_its_residency() {
        use misaka_palw_sdk::PalwWeightResidencyV1 as Residency;
        let _guard = exclusive();
        let path = temp_artifact("budgeted");
        write_qwen36_fixture_with(&path, 2, 256);
        assert_eq!(palw_class_residency_v1(Some(0)), Residency::PageCache);
        assert_eq!(palw_class_residency_v1(Some(7 << 30)), Residency::Bytes(7 << 30));
        assert_eq!(palw_class_default_residency_v1(None), Residency::FifthOfTheWeights, "no reading: the fifth");
        assert_eq!(
            palw_class_default_residency_v1(Some(40 << 30)),
            Residency::FifthWithin(24 << 30),
            "40 GiB available, 16 of them the node's own"
        );
        assert_eq!(palw_class_default_residency_v1(Some(8 << 30)), Residency::FifthWithin(0), "a host in swap spares nothing");
        // **The attempt is carved out of the share before the weights are budgeted.** ibm, 2026-09-23:
        // a 7 GiB share let the residency take 6.65 GiB and the attempt that followed was killed. With
        // the producer's attempt reserved first the weights get what the attempt leaves; a node that
        // produces nothing reserves nothing and reads as before.
        assert_eq!(palw_class_residency_within_share_v1(None, Some(7 << 30)), Residency::FifthWithin(7 << 30));
        assert_eq!(palw_class_residency_beside_the_attempt_v1(None, Some(7 << 30), 0), Residency::FifthWithin(7 << 30));
        assert_eq!(
            palw_class_residency_beside_the_attempt_v1(None, Some(7 << 30), 1 << 30),
            Residency::FifthWithin(6 << 30),
            "a 1 GiB attempt leaves 6 GiB for the weights"
        );
        assert_eq!(palw_class_residency_beside_the_attempt_v1(None, Some(7 << 30), 9 << 30), Residency::FifthWithin(0), "an attempt past the share leaves none");
        assert_eq!(palw_class_residency_beside_the_attempt_v1(Some(5 << 30), Some(7 << 30), 9 << 30), Residency::Bytes(5 << 30), "a stated figure is still the figure");
        assert_eq!(palw_producer_attempt_reserve_v1(&PalwCourtParamsV2::new(1 << 26, 4, 2).unwrap(), None), 0, "no producer class, no reserve");

        let declined = load_class_holdings_v1("test-declined", &sdk(), std::slice::from_ref(&path), 0, Residency::FifthWithin(0));
        assert_eq!(declined.len(), 1, "a default the host cannot spare still holds the class");
        assert!(misaka_palw_sdk::lineages::qwen36::residency_stats_of(&declined[0]).is_none(), "the page cache decides");
        let why = misaka_palw_sdk::lineages::qwen36::residency_declined_of(&declined[0]).expect("and says why");
        assert_eq!(why.spare_bytes, 0);
        assert!(declined[0].summary.contains("under the class's floor"), "{}", declined[0].summary);
        evict_held_artifacts_v1(std::slice::from_ref(&path));

        let roomy = load_class_holdings_v1("test-roomy", &sdk(), std::slice::from_ref(&path), 0, Residency::FifthWithin(u64::MAX));
        let fifth = misaka_palw_sdk::lineages::qwen36::residency_stats_of(&roomy[0]).expect("room for a fifth").budget_bytes;
        evict_held_artifacts_v1(std::slice::from_ref(&path));

        let budgeted = load_class_holdings_v1("test-budget", &sdk(), std::slice::from_ref(&path), 0, Residency::FifthOfTheWeights);
        assert_eq!(budgeted.len(), 1, "held");
        let stats = misaka_palw_sdk::lineages::qwen36::residency_stats_of(&budgeted[0]).expect("a budgeted holding has stats");
        assert!(stats.pinned_bytes > 0 && stats.budget_bytes > stats.pinned_bytes, "{stats:?}");
        assert_eq!(stats.budget_bytes, fifth, "room for a fifth is the fifth the ADR first wrote");
        assert!(budgeted[0].summary.contains("resident within"), "{}", budgeted[0].summary);
        // The holding is shared by file identity, so a second duty asking under ANOTHER policy
        // gets the holding the first made: one mapping, one residency, per file per process.
        let again = load_class_holdings_v1("test-budget-2", &sdk(), std::slice::from_ref(&path), 0, Residency::PageCache);
        assert!(
            misaka_palw_sdk::lineages::qwen36::residency_stats_of(&again[0]).is_some(),
            "the first duty's residency is the process's"
        );
        evict_held_artifacts_v1(std::slice::from_ref(&path));

        let paged = load_class_holdings_v1("test-paged", &sdk(), std::slice::from_ref(&path), 0, Residency::PageCache);
        assert!(misaka_palw_sdk::lineages::qwen36::residency_stats_of(&paged[0]).is_none(), "the page cache decides: no stats");
        assert!(paged[0].summary.contains("page cache"), "{}", paged[0].summary);
        evict_held_artifacts_v1(std::slice::from_ref(&path));
        std::fs::remove_file(&path).ok();
    }

    /// A budgeted Qwen3.6 holding prices a replay on the residency it pinned, not on the file.
    /// A page-cache holding still reports the file — that mapping will fault the whole artifact.
    #[test]
    fn a_qwen36_replay_is_priced_on_residency_not_the_file() {
        use misaka_palw_sdk::PalwWeightResidencyV1 as Residency;
        let _guard = exclusive();
        let path = temp_artifact("replay-bytes");
        write_qwen36_fixture_with(&path, 2, 256);
        let file = std::fs::metadata(&path).expect("the fixture is on disk").len();

        let budgeted =
            load_class_holdings_v1("test-replay-residency", &sdk(), std::slice::from_ref(&path), 0, Residency::FifthOfTheWeights);
        let stats = misaka_palw_sdk::lineages::qwen36::residency_stats_of(&budgeted[0]).expect("budgeted");
        assert_eq!(holding_replay_bytes_v1(&budgeted[0]), Some(stats.budget_bytes));
        assert!(stats.budget_bytes < file, "residency is the pages held, not the mapping's file");
        evict_held_artifacts_v1(std::slice::from_ref(&path));

        let paged = load_class_holdings_v1("test-replay-paged", &sdk(), std::slice::from_ref(&path), 0, Residency::PageCache);
        assert_eq!(holding_replay_bytes_v1(&paged[0]), Some(file), "page cache: the file is what will fault in");
        evict_held_artifacts_v1(std::slice::from_ref(&path));
        std::fs::remove_file(&path).ok();
    }

    /// Once a Qwen3.6 holding is resident, a replay must not re-charge the budget against
    /// MemAvailable — that leftover is ~3 GiB on a 23 GiB host that just pinned 6 GiB, and the
    /// panel would defer forever. The incremental figure is zero; scratch is added by the caller.
    #[test]
    fn a_resident_qwen36_replay_does_not_recharge_the_budget() {
        use misaka_palw_sdk::PalwWeightResidencyV1 as Residency;
        let _guard = exclusive();
        let path = temp_artifact("replay-incremental");
        write_qwen36_fixture_with(&path, 2, 256);
        let file = std::fs::metadata(&path).expect("the fixture is on disk").len();
        let budgeted =
            load_class_holdings_v1("test-replay-incremental", &sdk(), std::slice::from_ref(&path), 0, Residency::FifthOfTheWeights);
        assert_eq!(incremental_replay_bytes_v1(&budgeted[0]), Some(0), "already pinned: MemAvailable does not pay twice");
        evict_held_artifacts_v1(std::slice::from_ref(&path));

        let paged =
            load_class_holdings_v1("test-replay-incremental-paged", &sdk(), std::slice::from_ref(&path), 0, Residency::PageCache);
        assert_eq!(incremental_replay_bytes_v1(&paged[0]), Some(file), "page cache still faults the file");
        evict_held_artifacts_v1(std::slice::from_ref(&path));
        std::fs::remove_file(&path).ok();
    }

    /// **A cgroup's headroom is the least over the process's cgroup and its ancestors**, v2 first
    /// and v1 where v2 sets nothing — the reading that tells a pool slot in a 6 GiB cgroup from
    /// the host it runs on.
    #[test]
    fn a_cgroup_limit_bounds_what_the_default_may_take() {
        let files = |pairs: &'static [(&'static str, &'static str)]| {
            move |path: &Path| pairs.iter().find(|(p, _)| Path::new(p) == path).map(|(_, v)| v.to_string())
        };
        let slot = files(&[
            ("/sys/fs/cgroup/system.slice/misaka-pool-slot@06.service/memory.max", "6442450944\n"),
            ("/sys/fs/cgroup/system.slice/misaka-pool-slot@06.service/memory.current", "6323744768\n"),
            ("/sys/fs/cgroup/system.slice/memory.max", "max\n"),
            ("/sys/fs/cgroup/system.slice/memory.current", "20000000000\n"),
        ]);
        assert_eq!(
            cgroup_headroom_from_v1("0::/system.slice/misaka-pool-slot@06.service\n", slot),
            Some(6_442_450_944 - 6_323_744_768),
            "the slot's own limit, not the slice's none"
        );
        let nested = files(&[
            ("/sys/fs/cgroup/a/b/memory.max", "max"),
            ("/sys/fs/cgroup/a/b/memory.current", "100"),
            ("/sys/fs/cgroup/a/memory.max", "1000"),
            ("/sys/fs/cgroup/a/memory.current", "900"),
        ]);
        assert_eq!(cgroup_headroom_from_v1("0::/a/b", nested), Some(100), "an ancestor's limit binds its children");
        assert_eq!(cgroup_headroom_from_v1("0::/", files(&[])), None, "the root sets nothing");
        let v1 = files(&[
            ("/sys/fs/cgroup/memory/user.slice/memory.limit_in_bytes", "4294967296"),
            ("/sys/fs/cgroup/memory/user.slice/memory.usage_in_bytes", "1073741824"),
        ]);
        assert_eq!(cgroup_headroom_from_v1("12:memory:/user.slice\n0::/user.slice", v1), Some(3 << 30), "v1 where v2 sets nothing");
        let unlimited = files(&[
            ("/sys/fs/cgroup/memory/memory.limit_in_bytes", "9223372036854771712"),
            ("/sys/fs/cgroup/memory/memory.usage_in_bytes", "1"),
        ]);
        assert_eq!(cgroup_headroom_from_v1("5:cpu,memory:/", unlimited), None, "v1's no-limit is no limit");
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
        let producer = load_class_holdings_v1(
            "test-producer",
            &sdk(),
            std::slice::from_ref(&path),
            0,
            misaka_palw_sdk::PalwWeightResidencyV1::PageCache,
        );
        let panel = load_class_holdings_v1(
            "test-panel",
            &sdk(),
            std::slice::from_ref(&path),
            0,
            misaka_palw_sdk::PalwWeightResidencyV1::PageCache,
        );
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
        let first =
            load_class_holdings_v1("test", &sdk(), std::slice::from_ref(&path), 0, misaka_palw_sdk::PalwWeightResidencyV1::PageCache);
        // Replaced by rename, the way an operator drops in a new artifact: the old mapping stays
        // valid on its own inode, and the path now names a file of another size.
        let staged = temp_artifact("replaced-staged");
        write_qwen36_fixture(&staged, 2);
        std::fs::rename(&staged, &path).expect("renamed over the old artifact");
        let second =
            load_class_holdings_v1("test", &sdk(), std::slice::from_ref(&path), 0, misaka_palw_sdk::PalwWeightResidencyV1::PageCache);
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
        assert_eq!(
            load_class_holdings_v1("test", &sdk(), std::slice::from_ref(&path), 0, misaka_palw_sdk::PalwWeightResidencyV1::PageCache)
                .len(),
            1,
            "unbounded: held"
        );
        let bounded = load_class_holdings_v1(
            "test",
            &sdk(),
            std::slice::from_ref(&path),
            size - 1,
            misaka_palw_sdk::PalwWeightResidencyV1::PageCache,
        );
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

    /// **ADR-0067 SA-2: an unservable chain class is remembered, and the mark cannot outlive the
    /// holdings it is a statement about.**
    ///
    /// The mark exists so a hostile registration costs one compile per node rather than one per
    /// duty. It must also be exactly as short-lived as the fact it records: most refusals say
    /// "this node holds no artifact whose digest is the registered root", which stops being true
    /// the moment the operator supplies the file. A remembered refusal that outlived its cause
    /// would make supplying the artifact do nothing.
    ///
    /// **The first form of this test proved the wrong thing and this is the correction.** It
    /// called `evict_all_held_artifacts_v1` to show the mark being dropped — and that function's
    /// only callers were this test and its sibling, so what it demonstrated was a rule with no
    /// production path. The rule is enforced at the READ now, so the assertions below are about
    /// asking with DIFFERENT HOLDINGS, which is what a running node actually does.
    #[test]
    fn a_remembered_refusal_answers_only_for_the_holdings_it_was_computed_against() {
        let _serial = exclusive();
        evict_all_held_artifacts_v1();
        let class = Hash64::from_u64_word(0x0067_5A02);
        let root = Hash64::from_u64_word(0x0067_5A03);
        let holding = |lineage: &'static str, summary: &str| {
            PalwLoadedArtifactV1::from_parts(lineage, None, summary.to_string(), std::sync::Arc::new(0usize))
        };
        let held = vec![holding("test-lineage", "artifact root aaaa")];
        let other = vec![holding("test-lineage", "artifact root bbbb")];

        assert!(remembered_unservable(class, root, &held).is_none(), "nothing is unservable before anything was tried");
        remember_unservable(class, root, "the profile names a kernel this build does not carry", &held);
        assert_eq!(
            remembered_unservable(class, root, &held).as_deref(),
            Some("the profile names a kernel this build does not carry"),
            "the second caller gets the first caller's sentence, without recompiling a stranger's graph"
        );
        // A different pairing is a different question and is not answered by this mark.
        assert!(remembered_unservable(class, Hash64::from_u64_word(0xFEED), &held).is_none());

        // **The rule, with no evictor involved.** Ask the same question holding something else and
        // the verdict does not answer — it is dropped and re-derived. This is what makes supplying
        // the artifact take effect, and what stops one registry's holdings from deciding another's.
        assert!(
            remembered_unservable(class, root, &other).is_none(),
            "a verdict about one set of holdings must not be served to a caller holding another set"
        );
        assert!(
            remembered_unservable(class, root, &held).is_none(),
            "…and the stale entry is dropped rather than left for the next matching caller"
        );

        // The eviction door still works, for the same reason it always did.
        remember_unservable(class, root, "still unservable", &held);
        evict_all_held_artifacts_v1();
        assert!(remembered_unservable(class, root, &held).is_none(), "an explicit flush re-opens the question too");
    }

    /// **Round-3 defect I-2: a file rewritten under a live holding does NOT retract the verdict,
    /// because it cannot change the verdict — and the mark exists to stop exactly that
    /// recomputation.**
    ///
    /// This test asserted the opposite shape one round ago: [`holdings_identity_v1`] re-stat'ed
    /// each configured path, so any `len`/`mtime` movement dropped every remembered refusal in the
    /// process. That reads like operator responsiveness and is not: `load_class_holdings_v1` runs
    /// once per service construction, so a running producer keeps dispatching against the mapping
    /// it loaded at startup, and the "retracted" verdict is re-derived — one compile of a
    /// stranger's graph — to the identical sentence. An rsync in place, a backup that rewrites
    /// mtime, or a `touch` therefore re-paid the compile SA-2 exists to prevent, on every duty,
    /// for no possible change of answer.
    ///
    /// What retracts it is a change to the holdings the registry actually dispatches against: a
    /// different list, a reloaded mapping, or an eviction. Both halves are asserted here.
    #[test]
    fn a_file_rewritten_under_a_live_holding_does_not_retract_the_verdict() {
        let _serial = exclusive();
        evict_all_held_artifacts_v1();
        let class = Hash64::from_u64_word(0x0067_5A04);
        let root = Hash64::from_u64_word(0x0067_5A05);
        let path = temp_artifact("unservable-mark");
        std::fs::write(&path, b"the first bytes").expect("fixture writes");
        let held = vec![PalwLoadedArtifactV1::from_parts(
            "test-lineage",
            Some(path.clone()),
            "held".to_string(),
            std::sync::Arc::new(0usize),
        )];

        remember_unservable(class, root, "this node holds no artifact whose digest is the registered root", &held);
        assert!(remembered_unservable(class, root, &held).is_some(), "the mark stands while the holding does");

        // Different bytes, a different length, a new mtime — and the same in-memory holding, so
        // the same answer, so the same mark.
        std::fs::write(&path, b"the second bytes, a re-mint, a different length").expect("fixture rewrites");
        assert!(
            remembered_unservable(class, root, &held).is_some(),
            "the running service still dispatches against the mapping it loaded, so re-deriving could only \
             reproduce this refusal — paying for that is what the mark is for"
        );

        // A RELOADED holding is a different holding, and the question is open again. This is what
        // an operator supplying the artifact actually does: restart the service, or evict.
        let reloaded = vec![PalwLoadedArtifactV1::from_parts(
            "test-lineage",
            Some(path.clone()),
            "held".to_string(),
            std::sync::Arc::new(0usize),
        )];
        assert!(
            remembered_unservable(class, root, &reloaded).is_none(),
            "a verdict about one loaded artifact must not answer for another one"
        );
        std::fs::remove_file(&path).ok();
        evict_all_held_artifacts_v1();
    }

    /// **Round-3 defect I-1: the mark path performs no filesystem work at all, and therefore none
    /// of it under the process-wide mark lock.**
    ///
    /// The first form of the read-the-mark rule re-stat'ed every configured artifact — `metadata`
    /// plus `canonicalize`, one pair per holding — with the global `unservable_chain_classes`
    /// guard held. A blocking syscall under a process-wide lock is the shape that wedged a public
    /// node for 46 minutes while systemd still called it active: one `--palw-class-artifact` on a
    /// stalled NFS mount, and the producer's registry and the panel's registry (they share this
    /// one static) both queue behind a stat that will not return.
    ///
    /// The rule the mark actually needs is answered from the holdings this process serves FROM,
    /// which is memory, so the fix is not "stat outside the lock" but "do not stat": see
    /// [`holdings_identity_v1`]. Both halves are asserted here, because only the second one stays
    /// true if someone reintroduces a filesystem read on this path.
    #[test]
    fn the_mark_path_does_no_filesystem_work_under_the_global_lock() {
        let _serial = exclusive();
        evict_all_held_artifacts_v1();
        let path = temp_artifact("mark-lock-syscalls");
        std::fs::write(&path, b"a held artifact").expect("fixture writes");
        let holding = |summary: &str| {
            PalwLoadedArtifactV1::from_parts("test-lineage", Some(path.clone()), summary.to_string(), std::sync::Arc::new(0usize))
        };
        let held = vec![holding("held, root aaaa")];
        let other = vec![holding("held, root bbbb")];
        let class = Hash64::from_u64_word(0x0067_5A08);
        let root = Hash64::from_u64_word(0x0067_5A09);

        // **The probe is not vacuous**: a stat this thread really performs is counted, and counted
        // as "not under the lock", because nothing holds the lock right here.
        fs_probe::reset();
        assert!(HeldArtifactKey::of(&path).is_some(), "the fixture file is there to be stat'ed");
        assert_eq!(fs_probe::stats(), 1, "the probe counts a stat this thread performs — otherwise it proves nothing");
        assert_eq!(fs_probe::under_lock(), 0, "…and nothing holds the mark lock on this line");

        for (what, expected) in [("write", true), ("read-hit", true), ("read-miss", false), ("read-stale", false)] {
            fs_probe::reset();
            let answered = match what {
                "write" => {
                    remember_unservable(class, root, "the profile names a kernel this build does not carry", &held);
                    true
                }
                "read-hit" => remembered_unservable(class, root, &held).is_some(),
                "read-miss" => remembered_unservable(class, Hash64::from_u64_word(0xFEED), &held).is_some(),
                _ => remembered_unservable(class, root, &other).is_some(),
            };
            assert_eq!(answered, expected, "{what}: the mark answers the way SA-3 says it does");
            assert_eq!(
                fs_probe::under_lock(),
                0,
                "{what}: a filesystem syscall ran while the process-wide mark lock was held — that is the wedge shape, \
                 and every duty in this process queues behind it"
            );
            assert_eq!(
                fs_probe::stats(),
                0,
                "{what}: the mark path touched the filesystem at all; the verdict is about the holdings this process \
                 serves from, and those are in memory"
            );
        }
        std::fs::remove_file(&path).ok();
        evict_all_held_artifacts_v1();
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

    /// The t12 genesis's held rows, as the shipped card registers them: `(profile, canonical job)`.
    fn t12_held_rows() -> Vec<(kaspa_consensus_core::palw_step::PalwShapeProfileV3, kaspa_consensus_core::palw_v2::PalwJobContextV2)> {
        use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
        let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("t12 is a ConsensusV2 network") };
        bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. }
                    if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) =>
                {
                    Some((c.profile.clone(), c.canonical.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// **DoS audit 2026-09-24, #4: a held capture past the materialization cap is refused BY NAME
    /// before it is priced.** At testnet-12's own `2^40` network ladder — the ladder its backends
    /// are built at — every held genesis row's canonical capture (105.5M and 27.0G leaves) is past
    /// the host's `2^26`, and the pre-check the capture arm runs says so in the words the family's
    /// prover uses; a capture at the cap is admitted and goes on to be priced. Before the fix the
    /// cap was the network's ladder and this answered `Ok` for every count the class walks.
    #[test]
    fn a_held_capture_past_the_hosts_cap_is_refused_before_it_is_priced() {
        use kaspa_consensus_core::palw_resource_profile_v1::PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1;
        use kaspa_consensus_core::palw_state_chunk_map::{PALW_HELD_STEP_LADDER_V1, palw_class_step_ladder_v1};
        let rows = t12_held_rows();
        assert!(!rows.is_empty(), "the t12 card registers held rows");
        for (profile, job) in rows {
            let ladder = palw_class_step_ladder_v1(PALW_HELD_STEP_LADDER_V1, &profile);
            let leaves =
                kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &job, ladder).expect("the canonical job");
            assert!(leaves > PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1, "{leaves}");
            let why = palw_whole_capture_admits_v1(PALW_HELD_STEP_LADDER_V1, ladder, &profile, leaves).expect_err("past the cap");
            assert!(why.contains("materialization cap") && why.contains("streamed routes"), "{why}");
            assert_eq!(
                palw_whole_capture_admits_v1(PALW_HELD_STEP_LADDER_V1, ladder, &profile, PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1),
                Ok(())
            );
        }
    }

    /// **DoS audit 2026-09-24, #4: the capture sampler's reservation is priced by the DENSE
    /// capture, and the ledger refuses it where the full seat's fold figure would have fitted.**
    ///
    /// On a held t12 row the full seat's own sink is the fold — a few MiB — so a sampler that took
    /// the ordinary full-seat reservation would still have laid out tens of GiB unaccounted. The
    /// re-priced need carries the dense capture of the capture's own leaf count (and keeps every
    /// other term the full seat's); against a 24 GiB share (this Mac) a capture at the cap
    /// (`2^26` leaves, 39.5 GiB at tile 128) is refused, naming the role and the need, while the
    /// fold figure for the same job is granted; a small capture is granted and its bytes return on
    /// drop. A family that derives no profile is priced fail-closed: the dense capture is still in
    /// the figure.
    #[test]
    fn a_whole_capture_reservation_is_priced_dense_and_refused_where_it_does_not_fit() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        use kaspa_consensus_core::palw_resource_profile_v1::{
            PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1, PalwCaptureRetentionV1, PalwRuntimeLimitsV1, PalwRuntimeProfileV1,
            palw_dense_capture_bytes_v1, palw_profile_max_tile_len_v1, palw_resource_profile_v1,
        };
        const GIB: u64 = 1 << 30;
        let (profile, job) = t12_held_rows().into_iter().next().expect("a held row");
        let tile_len = palw_profile_max_tile_len_v1(&profile);
        let limits = PalwRuntimeLimitsV1 { threads: 8, prefill_run_positions: 64 };
        let fold = |leaves: u64| PalwRoleMemoryNeedV1 {
            role: PalwResourceRoleV1::FullSeat,
            holding_bytes: 0,
            derived_bytes: 0,
            runtime: Some(PalwRuntimeProfileV1::A16KvI32),
            profile: palw_resource_profile_v1(
                &profile,
                &job,
                leaves,
                PalwRuntimeProfileV1::A16KvI32,
                PalwResourceRoleV1::FullSeat,
                limits,
                PalwCaptureRetentionV1::Fold { retain_level: 12 },
            ),
        };
        let at_cap = PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1;
        let folded = fold(at_cap);
        let dense = palw_whole_capture_need_v1(folded.clone(), &profile, at_cap);
        let p = dense.profile.expect("priced");
        assert_eq!(p.capture, PalwCaptureRetentionV1::DenseTiles { tile_len });
        assert_eq!(p.capture_retained_bytes, palw_dense_capture_bytes_v1(at_cap, tile_len));
        assert_eq!(
            dense.total_bytes() - folded.total_bytes(),
            p.capture_retained_bytes - folded.profile.expect("fold").capture_retained_bytes,
            "only the capture term moved"
        );

        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(24 * GIB), || None);
        let key = PalwMemoryReservationKeyV1 { role: "full-seat capture", class_id: profile.shape_profile_id(), job: job.job_id };
        assert!(ledger.can_reserve(folded.total_bytes()).is_ok(), "the fold figure fits: {}", folded.describe());
        let refusal = ledger.reserve(key.clone(), dense.total_bytes()).expect_err("the dense capture does not");
        assert_eq!(refusal.need_bytes, dense.total_bytes());
        assert_eq!(refusal.key.as_ref().map(|k| k.role), Some("full-seat capture"), "the refusal names the role: {refusal}");
        assert_eq!(ledger.reserved_bytes(), 0, "a refusal reserves nothing");

        let small = palw_whole_capture_need_v1(fold(1 << 20), &profile, 1 << 20);
        let granted = ledger.reserve(key, small.total_bytes()).expect("a small capture fits");
        assert_eq!(ledger.reserved_bytes(), small.total_bytes());
        drop(granted);
        assert_eq!(ledger.reserved_bytes(), 0, "the samples' bytes return with the guard");

        let unprofiled = PalwRoleMemoryNeedV1 {
            role: PalwResourceRoleV1::FullSeat,
            holding_bytes: 0,
            derived_bytes: 0,
            runtime: None,
            profile: None,
        };
        let priced = palw_whole_capture_need_v1(unprofiled, &profile, at_cap);
        assert_eq!(priced.total_bytes(), palw_dense_capture_bytes_v1(at_cap, tile_len) + PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1);
    }
}
