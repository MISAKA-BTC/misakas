//! **Lane KNODE: the kernel route's Panel=1 slice roles on a real node** (G14, 2026-10-26) — node policy only: no consensus rule,
//! param, wire format or DB meaning changes. A child module of [`PalwPanelService`], run in the same tick right after the kernel
//! verifier (`palw_kernel_verifier`, which already carries the two roles a slice node needs most: the **outsider filer** — own replay
//! of every open claim from the public commitments, seal → `FileProof` through `misaka_palw_remote::pursuit` — and the **Panel=1
//! interim seat**'s passing receipt). This module adds the remaining ones, on the same court queue (funded carriers from the
//! virtual UTXO set, the PQ mempool, the production template):
//!
//! * **poster** (`--palw-kernel-post-job=<tok,tok,..>:<max_new>`): posts ONE single-prompt greedy decode job of the class its held
//!   model opens to (the slice's genesis class), once per process unless the chain already carries that exact job.
//! * **producer** (`--palw-kernel-producer`): for each open job of a class whose registered commitments a held model opens to, with no
//!   claim and no seal of this bond yet, it decodes greedily with ITS OWN model, builds the evidence, seals (`SealClaim`, salted past
//!   the salted-seal height — the salt is a deterministic ML-DSA-87 signature of this bond over the claim id, so a restart recomputes
//!   it and nobody else can) and, once the seal is folded, reveals (`CommitClaimSalted` / `CommitClaim`). It **answers every open
//!   `FileDemand` on its own claims** (`Respond` with the position's committed values, recomputed from the model — nothing persists),
//!   and with `--palw-kernel-export-dir=<dir>` also writes each response as `<claim>-<stage>-<position>.palwresp` (Borsh of
//!   `(claim, stage, position, bytes)`) for a relay.
//! * **Gate C drawn verifier** (identity-armed, no flag; a held model): for each kernel claim's `'K'` certification row (aux 136)
//!   that is drawn, undecided, inside its attest window, names this bond among its draws and holds no attestation of it, the node
//!   replays the claim from the public commitments with its own model and signs `AttestEligibilityExamV1`: `Reproduced` when the
//!   replay finds nothing (transcript = digest of what it replayed), `NotReproducible` when it refutes the claim, `Unavailable`
//!   when it cannot check it here. This is the drawn verifiers' signed approval under the honest-fraction premise (PRINCIPLES §8):
//!   no cryptographic proof of computation is made or used.
//! * **relay** (`--palw-kernel-relay-dir=<dir>`): a third party carries responses it did not compute — each `*.palwresp` file whose
//!   demand is open and unanswered is signed by THIS bond as `Respond` and queued (the bytes are authenticated by the court against
//!   the claim's commitments; the relay vouches for nothing).
//! * **drill-only injection** (`--palw-drill-kernel-inject=<what>`, repeatable; refused at start-up without `--palw-drill-genesis-salt`
//!   and the drill datadir marker, see `palw_drill`): `lie` (the producer commits a trace with one MatMul value bumped at position 1),
//!   `silent` (the producer never answers a demand), `forged-receipt` (a passing receipt for another bond's seat, signed with this
//!   node's key — the fold must drop it), `wrong-challenge` (a sealed `FileProof` accusing an honest claim's first decoded token with
//!   its TRUE logits — the court must dismiss it and charge the filer), `junk-flood` (`--palw-drill-kernel-flood=<n>` out-of-range
//!   `FileDemand`s against each open claim, once). TEST ONLY: no shipped preset can start with them.
//!
//! All roles are a clean no-op unless the kernel route's fence (`palw_probabilistic_constraints_v1` / `palw_panel_free_v1`) is in
//! force, the node has a funded bond (`--palw-fee-outpoint`) and holds a key.
use super::*;
use crate::palw_panel::court_outbox::CourtOutboxV1;
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1,
};
use misaka_palw_kernel::evidence::build_evidence_v1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeFaultV1, DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{ClaimBodyV1, KernelLedgerV1, ProsecutionV1, SaltedCommitV1, claim_seal_v1, claim_seal_v2, proof_seal_v1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1, TensorWireV1};
use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
use misaka_palw_kernel::trace::{TraceV1, WiringV1, derived_mask_v1, trace_v1};
use misaka_palw_tir::MapParams;
use misaka_palw_tir::program::TirProgramV1;

/// Court-queue rounds of this module's objects (distinct from every other lane's: `u32::MAX − 1 … − 28`, `− 64`).
pub(super) const PALW_KERNEL_PRODUCER_ROUND_V1: u32 = u32::MAX - 40;
pub(super) const PALW_KERNEL_CERT_ROUND_V1: u32 = u32::MAX - 41;
pub(super) const PALW_KERNEL_RELAY_ROUND_V1: u32 = u32::MAX - 42;
pub(super) const PALW_KERNEL_INJECT_ROUND_V1: u32 = u32::MAX - 43;

/// An object queued this many DAA ago and not yet seen folded is offered again.
const PALW_KERNEL_ROLES_RESEND_DAA_V1: u64 = 6;

/// The ML-DSA-87 context of the producer's claim-salt derivation (never a consensus context).
const PALW_KERNEL_CLAIM_SALT_CONTEXT_V1: &[u8] = b"misaka-palw/kaspad/kernel-producer/claim-salt/v1";

/// **A drill-only injection** (`--palw-drill-kernel-inject`). TEST ONLY.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwKernelInjectV1 {
    Lie,
    Silent,
    ForgedReceipt,
    WrongChallenge,
    JunkFlood,
}

impl std::str::FromStr for PalwKernelInjectV1 {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "lie" => Self::Lie,
            "silent" => Self::Silent,
            "forged-receipt" => Self::ForgedReceipt,
            "wrong-challenge" => Self::WrongChallenge,
            "junk-flood" => Self::JunkFlood,
            other => {
                return Err(format!(
                    "unknown kernel injection {other:?} (one of lie, silent, forged-receipt, wrong-challenge, junk-flood)"
                ));
            }
        })
    }
}

/// **The slice roles' configuration** (all off by default).
#[derive(Clone, Debug, Default)]
pub struct PalwKernelRolesConfigV1 {
    pub producer: bool,
    /// One job to post: (prompt, max_new_tokens).
    pub post_job: Option<(Vec<u32>, u32)>,
    pub export_dir: Option<PathBuf>,
    pub relay_dir: Option<PathBuf>,
    /// TEST ONLY: validated at start-up to need a drill salt.
    pub inject: Vec<PalwKernelInjectV1>,
    pub flood: u32,
}

impl PalwKernelRolesConfigV1 {
    pub fn injects(&self, what: PalwKernelInjectV1) -> bool {
        self.inject.contains(&what)
    }
}

/// Parse `--palw-kernel-post-job=<tok,tok,..>:<max_new>`.
pub fn palw_kernel_post_job_parse_v1(s: &str) -> Result<(Vec<u32>, u32), String> {
    let (prompt, n) = s.rsplit_once(':').ok_or("expected <tok,tok,..>:<max_new_tokens>")?;
    let prompt: Vec<u32> =
        prompt.split(',').map(|t| t.trim().parse::<u32>().map_err(|e| format!("prompt token {t:?}: {e}"))).collect::<Result<_, _>>()?;
    let n: u32 = n.trim().parse().map_err(|e| format!("max_new_tokens {n:?}: {e}"))?;
    if prompt.is_empty() || n == 0 {
        return Err("a job needs a non-empty prompt and max_new_tokens >= 1".into());
    }
    Ok((prompt, n))
}

/// The process's book: what was queued (object hash → DAA), the jobs posted, the drill injections already done.
#[derive(Default)]
pub(super) struct PalwKernelRolesBookV1 {
    queued: HashMap<Hash64, u64>,
    posted: bool,
    injected: HashSet<(Digest, u8)>,
    last_daa: Option<u64>,
}

fn object_key(object: &PalwConsensusObjectV2) -> Hash64 {
    let bytes = borsh::to_vec(object).expect("an object serializes");
    Hash64::from_bytes(*blake2b_simd::Params::new().hash_length(64).hash(&bytes).as_array())
}

// ---- pure parts (unit-tested) -------------------------------------------------------------------------------

/// The first MatMul node at position `from` — where an injected lie edits the committed trace.
pub(crate) fn palw_kernel_lie_site_v1(program: &TirProgramV1, from: u32) -> Option<(u32, u16, u16)> {
    for (s, (b, _)) in program.occurrences().iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            if matches!(node.prim, misaka_palw_tir::Prim::MatMul) {
                return Some((from, s as u16, n as u16));
            }
        }
    }
    None
}

/// The trace this producer commits for `stream` (honest, or with the drill lie applied — deterministic, so a restart recomputes it).
pub(crate) fn palw_kernel_committed_trace_v1(
    program: &TirProgramV1,
    model: &MapParams,
    stream: &[u32],
    lie: bool,
) -> Result<TraceV1, String> {
    let mut trace = trace_v1(program, model, stream).map_err(|e| format!("the model does not run the class program: {e:?}"))?;
    if lie {
        let (p, s, n) = palw_kernel_lie_site_v1(program, 1).ok_or("the program has no MatMul to lie at")?;
        let t = trace
            .values
            .get_mut(p as usize)
            .and_then(|v| v.get_mut(s as usize))
            .and_then(|v| v.get_mut(n as usize))
            .ok_or("the stream is too short to lie at position 1")?;
        let at = 1.min(t.data.len().saturating_sub(1));
        let v = t.data[at];
        t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
    }
    Ok(trace)
}

/// The stream a claim's trace runs over: the prompt and every generated token but the last.
fn claim_stream(job: &KernelJobV1, generated: &[u32]) -> Vec<u32> {
    let fed = generated.len().saturating_sub(1);
    job.prompt.iter().chain(&generated[..fed]).copied().collect()
}

/// **The producer's claim of `job`**: greedy (the job's rule) decode with its own model, the evidence, the claim id.
/// Returns `(claim id, plain reveal)`.
pub(crate) fn palw_kernel_produce_v1(
    ledger: &KernelLedgerV1,
    job: &KernelJobV1,
    producer: Digest,
    model: &MapParams,
    lie: bool,
) -> Result<(Digest, K), String> {
    let row = ledger.classes.get(&job.class_binding_id).ok_or("the job's class is not a single-program class")?;
    let (post, logits) = row.logits_at();
    let mut stream = job.prompt.clone();
    let mut generated = Vec::with_capacity(job.max_new_tokens as usize);
    for _ in 0..job.max_new_tokens {
        let t = trace_v1(&row.program, model, &stream).map_err(|e| format!("decode: {e:?}"))?;
        let tensor = t
            .values
            .get(stream.len() - 1)
            .and_then(|v| v.get(post as usize))
            .and_then(|v| v.get(logits as usize))
            .ok_or("the class's logits node is not in the trace")?;
        let tok = job.decode.select(tensor).ok_or("the decode rule selects no token")?;
        generated.push(tok);
        stream.push(tok);
    }
    let fed_stream = claim_stream(job, &generated);
    let trace = palw_kernel_committed_trace_v1(&row.program, model, &fed_stream, lie)?;
    let w = WiringV1::new(&row.program).map_err(|e| format!("wiring: {e:?}"))?;
    let ev = build_evidence_v1(&w, &trace.evidence(), &fed_stream, row.header(job.class_binding_id), &row.descriptor, 2)?;
    let claim = KernelClaimV1 { job_id: job.id(), producer_bond: producer, generated, evidence_root: ev.root() };
    let id = claim.id();
    Ok((id, K::CommitClaim { claim, evidence: ev, commitments: trace.evidence().commitments }))
}

/// The seal and the reveal of `reveal` under the ledger's seal rule at the next block (`salt` used only once salted seals apply).
pub(crate) fn palw_kernel_seal_and_reveal_v1(ledger: &KernelLedgerV1, producer: Digest, reveal: &K, salt: Digest) -> Option<(K, K)> {
    let K::CommitClaim { claim, evidence, commitments } = reveal else { return None };
    let id = claim.id();
    Some(if ledger.salted_seals_from().is_some_and(|at| ledger.daa.saturating_add(1) >= at) {
        let commit = SaltedCommitV1::Claim { claim: claim.clone(), evidence: evidence.clone(), commitments: commitments.clone() };
        (K::SealClaim { producer, job: claim.job_id, seal: claim_seal_v2(&id, &salt) }, K::CommitClaimSalted { salt, commit })
    } else {
        (K::SealClaim { producer, job: claim.job_id, seal: claim_seal_v1(&id) }, reveal.clone())
    })
}

/// **A demanded position's response** (every committed value whole; derived values omitted), recomputed from the model.
pub(crate) fn palw_kernel_position_bytes_v1(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    model: &MapParams,
    lie: bool,
    position: u32,
) -> Result<Vec<u8>, String> {
    let row = ledger.claims.get(claim).ok_or("no such claim")?;
    let ClaimBodyV1::Program { claim: c, .. } = &row.body else { return Err("not a single-program claim".into()) };
    let job = ledger.jobs.get(&row.job_id).ok_or("the claim's job is gone")?;
    let class = ledger.classes.get(&row.class_binding_id).ok_or("the claim's class is gone")?;
    let trace = palw_kernel_committed_trace_v1(&class.program, model, &claim_stream(job, &c.generated), lie)?;
    let mask = derived_mask_v1(&class.program);
    let values = trace
        .values
        .get(position as usize)
        .ok_or("the position is outside the claim")?
        .iter()
        .enumerate()
        .map(|(s, o)| {
            o.iter()
                .enumerate()
                .map(|(n, t)| if mask[s][n] { MaterialResponseV1::Omitted } else { MaterialResponseV1::Whole(TensorWireV1::of(t)) })
                .collect()
        })
        .collect();
    borsh::to_vec(&PositionResponseV1 { values, inputs: vec![] }).map_err(|e| e.to_string())
}

/// **A wrong challenge** (drill): accuse the claim's first decoded token with its TRUE logits from this node's own replay.
pub(crate) fn palw_kernel_wrong_challenge_v1(ledger: &KernelLedgerV1, claim: &Digest, model: &MapParams) -> Result<ProsecutionV1, String> {
    let row = ledger.claims.get(claim).ok_or("no such claim")?;
    let ClaimBodyV1::Program { claim: c, .. } = &row.body else { return Err("not a single-program claim".into()) };
    let job = ledger.jobs.get(&row.job_id).ok_or("the claim's job is gone")?;
    let class = ledger.classes.get(&row.class_binding_id).ok_or("the claim's class is gone")?;
    let (post, logits) = class.logits_at();
    let p = c.select_position(job, 0);
    let trace = trace_v1(&class.program, model, &claim_stream(job, &c.generated)).map_err(|e| format!("{e:?}"))?;
    let t = trace
        .values
        .get(p as usize)
        .and_then(|v| v.get(post as usize))
        .and_then(|v| v.get(logits as usize))
        .ok_or("the logits are outside the trace")?;
    Ok(ProsecutionV1::Decode(DecodeFaultV1 { index: 0, logits: TensorWireV1::of(t) }))
}

/// **The Gate C duties of bond `me`**: kernel claims whose `'K'` row is drawn, undecided, before its deadline at `daa`, names `me`
/// among its draws and holds no attestation of `me`.
pub(crate) fn palw_kernel_cert_duties_v1(route: &PalwKernelRouteStateV1, ledger: &KernelLedgerV1, me: &Digest, daa: u64) -> Vec<Digest> {
    use kaspa_consensus_core::palw_security_certification_v1::{
        PALW_SECURITY_CERTIFICATION_TABLE_V1, PalwClaimCertificationRowV1, certification_kernel_claim_key_v1,
        palw_kernel_claim_certification_id_v1,
    };
    ledger
        .claims
        .keys()
        .filter(|claim| {
            let key = certification_kernel_claim_key_v1(&palw_kernel_claim_certification_id_v1(claim));
            route.aux_row::<PalwClaimCertificationRowV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, &key).is_some_and(|row| {
                row.anchor.is_some()
                    && row.outcome.is_none()
                    && daa < row.deadline_daa
                    && row.draws.contains(me)
                    && !row.attestations.iter().any(|a| &a.verifier == me)
            })
        })
        .copied()
        .collect()
}

/// A relay file's contents: `(claim, stage, position, response bytes)`.
pub(crate) type PalwKernelRelayItemV1 = (Digest, u8, u32, Vec<u8>);

pub(crate) fn palw_kernel_relay_file_name_v1(claim: &Digest, stage: u8, position: u32) -> String {
    format!("{}-{stage}-{position}.palwresp", Hash64::from_bytes(*claim))
}

/// Every readable `*.palwresp` in `dir` (an unreadable or malformed file is reported, never guessed).
pub(crate) fn palw_kernel_relay_read_v1(dir: &std::path::Path) -> (Vec<PalwKernelRelayItemV1>, Vec<String>) {
    let mut items = Vec::new();
    let mut bad = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        bad.push(format!("cannot read the relay directory {}", dir.display()));
        return (items, bad);
    };
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "palwresp")).collect();
    paths.sort();
    for p in paths {
        match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| borsh::from_slice::<PalwKernelRelayItemV1>(&b).map_err(|e| e.to_string())) {
            Ok(item) => items.push(item),
            Err(why) => bad.push(format!("{}: {why}", p.display())),
        }
    }
    (items, bad)
}

/// Is the demand `(claim, stage, position)` open and not yet answered?
fn demand_open(ledger: &KernelLedgerV1, key: &(Digest, u8, u32)) -> bool {
    ledger.demands.get(key).is_some_and(|d| d.last.is_none() && ledger.daa < d.deadline_daa) && !ledger.served.contains_key(key)
}

// ---- the tick --------------------------------------------------------------------------------------------------

impl PalwPanelService {
    /// One tick of the slice roles (module doc). Called once a tick, right after the kernel verifier's.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn kernel_roles_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwKernelRolesBookV1,
        verifier: &super::palw_kernel_verifier::PalwKernelVerifierBookV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        court_outbox: &mut CourtOutboxV1,
    ) {
        let cfg = &self.config.kernel_roles;
        let params = &self.consensus_config.params;
        let armed = params
            .palw_probabilistic_constraints_v1
            .is_some_and(|f| f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(current_daa))
            || params.palw_panel_free_active_at(current_daa);
        let mine = |r: u32| matches!(r, PALW_KERNEL_PRODUCER_ROUND_V1 | PALW_KERNEL_CERT_ROUND_V1 | PALW_KERNEL_RELAY_ROUND_V1 | PALW_KERNEL_INJECT_ROUND_V1);
        court_outbox.retain_submitted(|(claim, round, responder), at| {
            if !mine(*round) || *responder {
                return true;
            }
            info!("[{PALW_PANEL}] claim {claim}: a kernel slice role's object was carried at DAA {at} (KNODE)");
            false
        });
        if !(armed && self.config.fee_outpoint.is_some() && self.keypair.is_some()) {
            if book.last_daa.take().is_some() {
                court_outbox.retain_pending(|(_, round, responder, _)| !mine(*round) || *responder);
            }
            return;
        }
        if book.last_daa == Some(current_daa) {
            return;
        }
        book.last_daa = Some(current_daa);
        let Some(route) = session.clone().spawn_blocking(|c| c.palw_kernel_route_v1()).await else { return };
        let ledger = match route.ledger() {
            Ok(l) => l,
            Err(why) => {
                warn!("[{PALW_PANEL}] kernel slice roles: the route's rows do not rebuild: {why:?}");
                return;
            }
        };
        let ledger: &KernelLedgerV1 = &ledger;
        let me = palw_kernel_bond_id_v1(&bond_key);
        let model_of = |class: &Digest| -> Option<std::sync::Arc<MapParams>> {
            misaka_palw_remote::pursuit::kernel_class_model_root_v1(ledger, class).and_then(|root| verifier.model_for_root_v1(&root))
        };
        let mut out: Vec<(Digest, u32, PalwConsensusObjectV2, &'static str)> = Vec::new();
        let sign = |k: &K| self.kernel_roles_signed_v1(network_domain, bond_key, k);

        // -- poster --
        if let (Some((prompt, n)), false) = (&cfg.post_job, book.posted) {
            let class = ledger.classes.keys().find(|c| model_of(c).is_some()).copied();
            match class {
                Some(class) => {
                    let job = KernelJobV1 {
                        class_binding_id: class,
                        prompt: prompt.clone(),
                        max_new_tokens: *n,
                        decode: DecodeRuleV1::Greedy,
                        nonce: misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-poster/nonce/v1", &me),
                    };
                    if ledger.jobs.contains_key(&job.id()) {
                        book.posted = true;
                    } else if let Some(o) = sign(&K::PostJob { job: job.clone() }) {
                        out.push((job.id(), PALW_KERNEL_PRODUCER_ROUND_V1, o, "PostJob"));
                    }
                }
                None => crate::palw_backends::note_throttled_v1("kernel-poster-no-class", || {
                    format!("[{PALW_PANEL}] kernel poster: no registered class opens to a held --palw-kernel-model; nothing posted")
                }),
            }
        }

        // -- producer --
        if cfg.producer {
            let lie = cfg.injects(PalwKernelInjectV1::Lie);
            for (job_id, job) in &ledger.jobs {
                if ledger.job_claims.contains_key(job_id) || ledger.claims.values().any(|c| c.job_id == *job_id && c.producer == me) {
                    continue;
                }
                let Some(model) = model_of(&job.class_binding_id) else { continue };
                let (id, reveal) = match palw_kernel_produce_v1(ledger, job, me, &model, lie) {
                    Ok(x) => x,
                    Err(why) => {
                        warn!("[{PALW_PANEL}] kernel producer: job {} not produced: {why}", Hash64::from_bytes(*job_id));
                        continue;
                    }
                };
                let Some(salt_sig) = self.sign(&id, PALW_KERNEL_CLAIM_SALT_CONTEXT_V1) else { continue };
                let salt = misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-producer/salt/v1", &salt_sig);
                let Some((seal, reveal)) = palw_kernel_seal_and_reveal_v1(ledger, me, &reveal, salt) else { continue };
                let step = if ledger.seals.contains_key(&(*job_id, me)) { (reveal, "reveal") } else { (seal, "SealClaim") };
                if let Some(o) = sign(&step.0) {
                    out.push((id, PALW_KERNEL_PRODUCER_ROUND_V1, o, step.1));
                }
            }
            // Answer every open demand on this bond's claims (unless the drill keeps it silent).
            if !cfg.injects(PalwKernelInjectV1::Silent) {
                for key in ledger.demands.keys().filter(|k| demand_open(ledger, k)) {
                    let (claim, stage, position) = *key;
                    let Some(row) = ledger.claims.get(&claim).filter(|r| r.producer == me) else { continue };
                    let Some(model) = model_of(&row.class_binding_id) else { continue };
                    match palw_kernel_position_bytes_v1(ledger, &claim, &model, lie, position) {
                        Ok(bytes) => {
                            if let Some(dir) = &cfg.export_dir {
                                let path = dir.join(palw_kernel_relay_file_name_v1(&claim, stage, position));
                                let item: PalwKernelRelayItemV1 = (claim, stage, position, bytes.clone());
                                if let Err(e) = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, borsh::to_vec(&item).expect("serializes"))) {
                                    warn!("[{PALW_PANEL}] kernel producer: cannot export {}: {e}", path.display());
                                }
                            }
                            if let Some(o) = sign(&K::Respond { claim, stage, position, bytes }) {
                                out.push((claim, PALW_KERNEL_PRODUCER_ROUND_V1, o, "Respond"));
                            }
                        }
                        Err(why) => warn!("[{PALW_PANEL}] kernel producer: claim {}: cannot answer position {position}: {why}", Hash64::from_bytes(claim)),
                    }
                }
            }
        }

        // -- Gate C drawn verifier (identity-armed) --
        for claim in palw_kernel_cert_duties_v1(&route, ledger, &me, current_daa) {
            use misaka_palw_kernel::model_eligibility_v1::{ELIGIBILITY_ATTESTATION_VERSION_V1, EligibilityAttestationV1, EligibilityVerdictV1};
            let row = &ledger.claims[&claim];
            let (verdict, transcript) = match model_of(&row.class_binding_id) {
                None => (EligibilityVerdictV1::Unavailable, [0u8; 64]),
                Some(model) => {
                    let salt = misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-cert/salt/v1", &me);
                    match misaka_palw_remote::pursuit::kernel_check_program_v1(ledger, &claim, model.as_ref(), salt) {
                        Ok(None) => {
                            let mut seen = Vec::with_capacity(128);
                            seen.extend_from_slice(&claim);
                            seen.extend_from_slice(&me);
                            (EligibilityVerdictV1::Reproduced, misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-cert/transcript/v1", &seen))
                        }
                        Ok(Some(_)) => (EligibilityVerdictV1::NotReproducible, [0u8; 64]),
                        Err(why) => {
                            warn!("[{PALW_PANEL}] Gate C: claim {}: not checkable here ({why}) — attesting Unavailable", Hash64::from_bytes(claim));
                            (EligibilityVerdictV1::Unavailable, [0u8; 64])
                        }
                    }
                }
            };
            info!("[{PALW_PANEL}] Gate C: this drawn verifier attests {verdict:?} for claim {} (KNODE)", Hash64::from_bytes(claim));
            let attestation = EligibilityAttestationV1 {
                version: ELIGIBILITY_ATTESTATION_VERSION_V1,
                network: network_domain.as_bytes(),
                exam: kaspa_consensus_core::palw_security_certification_v1::palw_kernel_claim_certification_id_v1(&claim),
                verifier: me,
                verdict,
                transcript,
                claim_mismatches: vec![],
            };
            if let Some(o) = sign(&K::AttestEligibilityExamV1 { attestation }) {
                out.push((claim, PALW_KERNEL_CERT_ROUND_V1, o, "AttestEligibilityExamV1"));
            }
        }

        // -- relay --
        if let Some(dir) = &cfg.relay_dir {
            let (items, bad) = palw_kernel_relay_read_v1(dir);
            for why in bad {
                crate::palw_backends::note_throttled_v1("kernel-relay-bad", || format!("[{PALW_PANEL}] kernel relay: {why}"));
            }
            for (claim, stage, position, bytes) in items {
                if !demand_open(ledger, &(claim, stage, position)) {
                    continue;
                }
                if let Some(o) = sign(&K::Respond { claim, stage, position, bytes }) {
                    out.push((claim, PALW_KERNEL_RELAY_ROUND_V1, o, "Respond (relayed)"));
                }
            }
        }

        // -- drill-only injections (start-up refuses them without a drill salt) --
        if !cfg.inject.is_empty() {
            self.kernel_roles_inject_v1(book, &route, ledger, me, &model_of, network_domain, bond_key, current_daa, &mut out);
        }

        // -- queue --
        let (pooled, orphans) =
            self.flow_context.mining_manager().clone().get_all_transactions(kaspa_mining::model::tx_query::TransactionQuery::All).await;
        let in_pool: HashSet<Hash64> = pooled
            .iter()
            .chain(orphans.iter())
            .filter_map(|p| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&p.tx.payload).ok())
            .map(|payload| object_key(&payload.object))
            .collect();
        for (claim, round, object, name) in out {
            let key = object_key(&object);
            let recently = book.queued.get(&key).is_some_and(|at| current_daa < at.saturating_add(PALW_KERNEL_ROLES_RESEND_DAA_V1));
            let queued = court_outbox.pending().iter().any(|(_, r, _, o)| *r == round && *o == object);
            if recently || queued || in_pool.contains(&key) {
                continue;
            }
            let session_key = Hash64::from_bytes(claim);
            info!("[{PALW_PANEL}] claim {session_key}: kernel slice role queues {name} (KNODE)");
            court_outbox.set_due((session_key, round, false), current_daa);
            court_outbox.push_pending((session_key, round, false, object));
            book.queued.insert(key, current_daa);
            if round == PALW_KERNEL_PRODUCER_ROUND_V1 && name == "PostJob" {
                book.posted = true;
            }
        }
        book.queued.retain(|_, at| current_daa < at.saturating_add(4 * PALW_KERNEL_ROLES_RESEND_DAA_V1));
    }

    /// TEST ONLY: the drill injections (module doc). Each fires once per (claim, kind).
    #[allow(clippy::too_many_arguments)]
    fn kernel_roles_inject_v1(
        &self,
        book: &mut PalwKernelRolesBookV1,
        route: &PalwKernelRouteStateV1,
        ledger: &KernelLedgerV1,
        me: Digest,
        model_of: &dyn Fn(&Digest) -> Option<std::sync::Arc<MapParams>>,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        current_daa: u64,
        out: &mut Vec<(Digest, u32, PalwConsensusObjectV2, &'static str)>,
    ) {
        let cfg = &self.config.kernel_roles;
        let open: Vec<Digest> = ledger
            .claims
            .iter()
            .filter(|(_, r)| r.producer != me && !r.convicted && !matches!(r.life.state, ClaimStateV1::Convicted { .. } | ClaimStateV1::TimedOut { .. }))
            .map(|(id, _)| *id)
            .collect();
        for claim in open {
            if cfg.injects(PalwKernelInjectV1::ForgedReceipt) && !book.injected.contains(&(claim, 1)) {
                if let Some(o) = self.kernel_roles_forged_receipt_v1(route, ledger, &claim, current_daa) {
                    warn!("[{PALW_PANEL}] DRILL INJECTION: a forged seat receipt for claim {} (the fold must drop it)", Hash64::from_bytes(claim));
                    out.push((claim, PALW_KERNEL_INJECT_ROUND_V1, o, "forged KernelConstraintReceiptV1"));
                    book.injected.insert((claim, 1));
                }
            }
            if cfg.injects(PalwKernelInjectV1::WrongChallenge) && !book.injected.contains(&(claim, 2)) {
                let row = &ledger.claims[&claim];
                if let Some(model) = model_of(&row.class_binding_id) {
                    match palw_kernel_wrong_challenge_v1(ledger, &claim, &model) {
                        Ok(proof) => {
                            let sealed = ledger.proof_seals.contains_key(&(claim, me));
                            let k = if sealed {
                                K::FileProof { accuser: me, claim, proof }
                            } else {
                                K::SealProof { accuser: me, claim, seal: proof_seal_v1(&claim, &me, &proof) }
                            };
                            if let Some(o) = self.kernel_roles_signed_v1(network_domain, bond_key, &k) {
                                warn!(
                                    "[{PALW_PANEL}] DRILL INJECTION: a wrong challenge ({}) of claim {} (the court must dismiss it)",
                                    if sealed { "FileProof" } else { "SealProof" },
                                    Hash64::from_bytes(claim)
                                );
                                out.push((claim, PALW_KERNEL_INJECT_ROUND_V1, o, if sealed { "wrong FileProof" } else { "wrong SealProof" }));
                                if sealed {
                                    book.injected.insert((claim, 2));
                                }
                            }
                        }
                        Err(why) => warn!("[{PALW_PANEL}] DRILL INJECTION: no wrong challenge of {}: {why}", Hash64::from_bytes(claim)),
                    }
                }
            }
            if cfg.injects(PalwKernelInjectV1::JunkFlood) && !book.injected.contains(&(claim, 3)) {
                warn!("[{PALW_PANEL}] DRILL INJECTION: {} junk demands against claim {}", cfg.flood, Hash64::from_bytes(claim));
                for i in 0..cfg.flood {
                    let k = K::FileDemand { demander: me, claim, stage: 0, position: 1_000_000u32.saturating_add(i) };
                    if let Some(o) = self.kernel_roles_signed_v1(network_domain, bond_key, &k) {
                        out.push((claim, PALW_KERNEL_INJECT_ROUND_V1, o, "junk FileDemand"));
                    }
                }
                book.injected.insert((claim, 3));
            }
        }
    }

    /// TEST ONLY: a passing receipt for the claim's first seat, signed with THIS node's key (not the seat's).
    fn kernel_roles_forged_receipt_v1(
        &self,
        route: &PalwKernelRouteStateV1,
        ledger: &KernelLedgerV1,
        claim: &Digest,
        current_daa: u64,
    ) -> Option<PalwConsensusObjectV2> {
        use misaka_palw_kernel::receipt::{CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1, ReceiptInputsV1, receipt_for_v1};
        use misaka_palw_kernel::verify::{CheckCostV1, ScopeVerdictV1};
        let assignment = route.assignment_of(claim)?;
        let seat = assignment.seats.first()?.clone();
        let row = ledger.claims.get(claim)?;
        let ClaimBodyV1::Program { evidence, .. } = &row.body else { return None };
        let class = ledger.classes.get(&row.class_binding_id)?;
        let verdict = ScopeVerdictV1::Pass {
            scope_root: seat.scope.root(evidence),
            positions: evidence.positions,
            probabilistic_checks: 1_000_000,
            error_bits: 80,
            cost: CheckCostV1::default(),
        };
        let inputs = ReceiptInputsV1 {
            descriptor: &class.descriptor,
            evidence,
            claim_id: *claim,
            assignment_root: assignment.assignment_root,
            challenge_anchor: assignment.challenge_anchor,
            sample_seed: assignment.sample_seed,
            seat_bond: seat.kernel_bond,
            seat_operator: [0u8; 64],
            signed_daa: current_daa,
        };
        let receipt = receipt_for_v1(&inputs, &seat.scope, &verdict)?;
        let signature = self.sign(receipt.signing_message().as_ref(), CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1)?;
        Some(PalwConsensusObjectV2::KernelConstraintReceiptV1 { receipt: Box::new(receipt), signature })
    }

    fn kernel_roles_signed_v1(&self, network_domain: Hash64, bond: PalwBondKeyV2, object: &K) -> Option<PalwConsensusObjectV2> {
        let bytes = object.encode();
        let message = palw_kernel_route_message_v1(network_domain, &bond, &bytes);
        let signature = self.sign(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)?;
        Some(PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injections_and_post_job_parse_and_refuse_garbage() {
        assert_eq!("lie".parse::<PalwKernelInjectV1>(), Ok(PalwKernelInjectV1::Lie));
        assert_eq!("junk-flood".parse::<PalwKernelInjectV1>(), Ok(PalwKernelInjectV1::JunkFlood));
        assert!("everything".parse::<PalwKernelInjectV1>().is_err());
        assert_eq!(palw_kernel_post_job_parse_v1("3,17,9:3"), Ok((vec![3, 17, 9], 3)));
        assert!(palw_kernel_post_job_parse_v1(":3").is_err());
        assert!(palw_kernel_post_job_parse_v1("3,17:0").is_err());
        assert!(palw_kernel_post_job_parse_v1("3,x:2").is_err());
        assert!(PalwKernelRolesConfigV1::default().inject.is_empty(), "nothing injected by default");
    }

    #[test]
    fn relay_files_round_trip_and_bad_files_are_reported() {
        let dir = std::env::temp_dir().join(format!("knode-relay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let item: PalwKernelRelayItemV1 = ([7u8; 64], 0, 4, vec![1, 2, 3]);
        std::fs::write(dir.join(palw_kernel_relay_file_name_v1(&item.0, item.1, item.2)), borsh::to_vec(&item).unwrap()).unwrap();
        std::fs::write(dir.join("junk.palwresp"), b"\x01").unwrap();
        std::fs::write(dir.join("ignored.txt"), b"x").unwrap();
        let (items, bad) = palw_kernel_relay_read_v1(&dir);
        assert_eq!(items, vec![item]);
        assert_eq!(bad.len(), 1, "the malformed file is reported, not skipped silently: {bad:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
