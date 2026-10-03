//! **RFC-0007 Part IV.1, the node's side of the audit mesh** (node policy; a child of `palw_panel`). Nothing here is a rule: the fold draws
//! the auditors, counts the `Audited` leaves and settles the traps; below `Params::palw_audit_mesh_v1` none of this runs.
//!
//! * **The audit duty** ([`PalwPanelService::mesh_audit_pass_v1`]): the chain has drawn this seat to audit a claim (`mesh_audit_duties`).
//!   A seat that **holds the claim's class** replays the claim's job and compares the roots with the claim's own — the one-move court's
//!   leaf check at its widest: a claim whose execution the seat reproduces is attested a match (`result` 0), one it does not a mismatch
//!   (1). The result goes into the seat's vertex as an `Audited { claim, leaf: ticket, result }` leaf. **A seat that does not hold the class
//!   says nothing** — silence is never a match (PALW-AM-2), earns nothing and is charged nothing. One audit replay runs at a time, off the
//!   loop, so the mesh never takes the panel's replay slots from its own duties for more than one replay's worth of CPU.
//! * **Traps** ([`PalwTrapBookV1`]): a bonded seat that wants to set a trap writes a plan (the claim it produced with a planted fault, the
//!   fault's tile, how many tiles the claim has, a salt) to `palw-traps.json` in the state dir; the node carries `TrapCommitted` when the
//!   bond is drawn by the slot lottery and `TrapRevealed` once the claim's audit window has closed, each once. Producing the faulty claim
//!   itself is the drill's (`--palw-drill-tamper-leaf`); the book only keeps the commitment honest and on time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_mesh_v1::{PalwTrapCommittedV1, PalwTrapRevealedV1, palw_trap_slot_drawn_v1};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_vertex_v1::{PalwClaimRefV1, PalwVertexLeafV1};
use kaspa_core::{info, warn};

use super::PALW_PANEL;
use super::vertex::PalwVertexBookV1;

/// How many DAA a seat waits before it asks for a claim's material again.
const PALW_MESH_NOTE_DAA_V1: u64 = 120;

/// **The `Audited` leaf of a finished audit**: the claim named by the DAA its audit was drawn at and a 16-byte prefix (or whole, if the
/// operator asked for whole ids), the drawn ticket, and `0` for a match, `1` for a mismatch.
pub(crate) fn palw_mesh_audit_leaf_v1(claim: &Hash64, drawn_daa: u64, ticket: u64, reproduced: bool, full_refs: bool) -> PalwVertexLeafV1 {
    let claim = if full_refs {
        PalwClaimRefV1::Full(*claim)
    } else {
        PalwClaimRefV1::compact_of(claim, drawn_daa).unwrap_or(PalwClaimRefV1::Full(*claim))
    };
    PalwVertexLeafV1::Audited { claim, leaf: ticket, result: u8::from(!reproduced) }
}

/// **The mesh node's book**: the audits this seat has answered or is running, and the counters the status reads.
#[derive(Default)]
pub(crate) struct PalwMeshNodeV1 {
    /// The claims whose audit this seat has put in a vertex (this run).
    answered: HashSet<Hash64>,
    /// The audit replays in flight, by claim: the handle, the drawn DAA and the ticket the leaf will carry.
    running: HashMap<Hash64, (tokio::task::JoinHandle<Result<bool, String>>, u64, u64)>,
    /// When a claim last had a "cannot audit" note, so the log says it once a while.
    noted: HashMap<Hash64, u64>,
    pub audits_started: u64,
    pub audits_matched: u64,
    pub audits_mismatched: u64,
    pub audits_failed: u64,
    pub audits_skipped: u64,
}

impl PalwMeshNodeV1 {
    /// The `key=value` pairs the node status carries.
    pub(crate) fn status_v1(&self, tip: &kaspa_consensus_core::palw_mesh_v1::PalwMeshStatusV1) -> String {
        let fence = |at: Option<u64>| at.map(|at| at.to_string()).unwrap_or_else(|| "off".to_string());
        format!(
            "witness_fence={} audit_fence={} capped_fence={} mesh_audits_started={} mesh_audits_matched={} mesh_audits_mismatched={} \
             mesh_audits_failed={} mesh_audits_skipped={} mesh_audits_running={} mesh_tip_witness_profiles={} mesh_tip_audit_rows={} \
             mesh_tip_traps_open={} mesh_tip_capped_claims={} mesh_tip_own_audits={}",
            fence(tip.witness_fence_daa),
            fence(tip.audit_fence_daa),
            fence(tip.capped_fence_daa),
            self.audits_started,
            self.audits_matched,
            self.audits_mismatched,
            self.audits_failed,
            self.audits_skipped,
            self.running.len(),
            tip.witness_profiles,
            tip.audit_rows,
            tip.traps_open,
            tip.capped_claims,
            tip.own_audits.len()
        )
    }

    fn note_once(&mut self, claim: &Hash64, now_daa: u64) -> bool {
        match self.noted.get(claim) {
            Some(at) if now_daa < at.saturating_add(PALW_MESH_NOTE_DAA_V1) => false,
            _ => {
                self.noted.insert(*claim, now_daa);
                true
            }
        }
    }
}

impl super::PalwPanelService {
    /// **One tick of the audit duty** (module note): reap a finished audit replay into its leaf, and start the next audit this seat owes
    /// if it holds the class and no replay is in flight.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn mesh_audit_pass_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        book: &mut PalwVertexBookV1,
        mesh: &mut PalwMeshNodeV1,
    ) {
        // 1. A replay that has finished becomes a leaf.
        let finished: Vec<Hash64> = mesh.running.iter().filter(|(_, (handle, _, _))| handle.is_finished()).map(|(claim, _)| *claim).collect();
        for claim in finished {
            let Some((handle, drawn_daa, ticket)) = mesh.running.remove(&claim) else { continue };
            match handle.await {
                Ok(Ok(reproduced)) => {
                    book.record(palw_mesh_audit_leaf_v1(&claim, drawn_daa, ticket, reproduced, self.config.vertex_full_refs));
                    mesh.answered.insert(claim);
                    if reproduced {
                        mesh.audits_matched += 1;
                        info!("[{PALW_PANEL}] audit of claim {claim}: this seat reproduces its execution — attesting a match (RFC-0007 Part IV.1)");
                    } else {
                        mesh.audits_mismatched += 1;
                        warn!(
                            "[{PALW_PANEL}] audit of claim {claim}: this seat does NOT reproduce its execution — attesting a mismatch \
                             (RFC-0007 Part IV.1); the one-move court pass takes it from here"
                        );
                    }
                }
                Ok(Err(why)) => {
                    mesh.audits_failed += 1;
                    warn!("[{PALW_PANEL}] audit of claim {claim}: the replay did not finish ({why}); nothing attested");
                }
                Err(e) => {
                    mesh.audits_failed += 1;
                    warn!("[{PALW_PANEL}] audit of claim {claim}: the replay task did not finish ({e}); nothing attested");
                }
            }
        }
        if !mesh.running.is_empty() {
            return;
        }
        // 2. The next audit this seat owes and can do.
        for duty in session.palw_v2_mesh_audit_duties_v1(bond_key) {
            if mesh.answered.contains(&duty.claim_id) || mesh.running.contains_key(&duty.claim_id) {
                continue;
            }
            // Only a seat that holds the class can judge it; the rest are silent (never a match).
            let held = match self.backends().resolve_tir_v1(duty.class_id, duty.artifact_root) {
                Some(Ok(_)) => true,
                Some(Err(why)) => {
                    if mesh.note_once(&duty.claim_id, current_daa) {
                        warn!("[{PALW_PANEL}] audit of claim {}: the IR backend does not build for its class ({why}); nothing attested", duty.claim_id);
                    }
                    false
                }
                None => false,
            };
            if !held {
                mesh.audits_skipped += 1;
                if mesh.note_once(&duty.claim_id, current_daa) {
                    info!(
                        "[{PALW_PANEL}] audit of claim {}: this seat does not hold class {} — saying nothing (silence is never a match)",
                        duty.claim_id, duty.class_id
                    );
                }
                continue;
            }
            let Ok(backend) = self.resolve_backend(session, duty.class_id, duty.artifact_root) else { continue };
            let Some((job, prompt)) =
                self.attempt_job_for_claim(session, backend.as_ref(), network_domain, duty.accepted_block, duty.class_id, &duty.executor_bond)
            else {
                continue;
            };
            let (execution_root, trace_root) = (duty.execution_root, duty.trace_root);
            info!(
                "[{PALW_PANEL}] audit of claim {}: replaying its job to attest the leaf drawn for this seat (ticket {}, until DAA {})",
                duty.claim_id, duty.ticket, duty.audit_end_daa
            );
            mesh.audits_started += 1;
            let handle = tokio::task::spawn_blocking(move || {
                backend
                    .execute(&job, &prompt)
                    .map(|run| run.execution_root == execution_root && run.trace_root == trace_root)
                    .map_err(|e| e.to_string())
            });
            mesh.running.insert(duty.claim_id, (handle, duty.drawn_daa, duty.ticket));
            // One replay at a time.
            break;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The trap book
// ---------------------------------------------------------------------------------------------

/// **One trap the operator plans** (a line of `palw-traps.json`): the claim this seat produced with a fault planted in committed tile
/// `fault_leaf` of `tiles`, and the salt of the commitment. `committed` and `revealed` are the node's own bookkeeping.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct PalwTrapPlanV1 {
    /// The claim's id, hex.
    pub claim: String,
    pub fault_leaf: u64,
    pub tiles: u64,
    /// 32 bytes, hex.
    pub salt: String,
    #[serde(default)]
    pub committed: bool,
    #[serde(default)]
    pub revealed: bool,
}

impl PalwTrapPlanV1 {
    fn claim_id(&self) -> Option<Hash64> {
        self.claim.parse::<Hash64>().ok()
    }

    fn salt_bytes(&self) -> Option<[u8; 32]> {
        let raw = self.salt.trim_start_matches("0x");
        if raw.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(raw.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        Some(out)
    }
}

/// **The seat's trap plans**, kept in the state dir and written whole (beside, then renamed over) whenever one moves.
#[derive(Debug, Default)]
pub(crate) struct PalwTrapBookV1 {
    path: PathBuf,
    plans: Vec<PalwTrapPlanV1>,
}

impl PalwTrapBookV1 {
    /// Load the book at `path` (empty when the file is absent; an unreadable file is an empty book and a warning, never a panic).
    pub(crate) fn load(path: &Path) -> Self {
        let plans = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Vec<PalwTrapPlanV1>>(&text).unwrap_or_else(|e| {
                warn!("[{PALW_PANEL}] {} is not a list of trap plans ({e}); ignored", path.display());
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        Self { path: path.to_path_buf(), plans }
    }

    /// Re-read the file (an operator or a drill may add a plan while the node runs); plans already known keep their bookkeeping.
    pub(crate) fn refresh(&mut self) {
        let fresh = Self::load(&self.path).plans;
        for plan in fresh {
            if !self.plans.iter().any(|known| known.claim == plan.claim && known.salt == plan.salt) {
                self.plans.push(plan);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.plans.len()
    }

    fn save(&self) {
        let tmp = self.path.with_extension("tmp");
        let written = serde_json::to_vec_pretty(&self.plans)
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, &self.path)).map_err(|e| e.to_string()));
        if let Err(e) = written {
            warn!("[{PALW_PANEL}] cannot save the trap plans to {}: {e}", self.path.display());
        }
    }

    /// **The commitment to carry now**, if a plan has none yet and `setter` is drawn by the slot lottery at `daa`.
    pub(crate) fn next_commit(
        &self,
        network_domain: Hash64,
        setter: PalwBondKeyV2,
        daa: u64,
        sign: &dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Option<(usize, PalwTrapCommittedV1)> {
        if !palw_trap_slot_drawn_v1(&setter, daa) {
            return None;
        }
        let (index, plan) = self.plans.iter().enumerate().find(|(_, plan)| !plan.committed)?;
        let (claim, salt) = (plan.claim_id()?, plan.salt_bytes()?);
        let object = PalwTrapCommittedV1::sign_v1(network_domain, setter, &claim, plan.fault_leaf, plan.tiles, &salt, |m, c| sign(m, c))?;
        Some((index, object))
    }

    /// **The reveal to carry now**: a committed, unrevealed plan whose claim's audit window has closed and whose reveal window is open
    /// (`window(claim)` is the chain's `(audit_end, row_end)` for the claim).
    pub(crate) fn next_reveal(
        &self,
        network_domain: Hash64,
        setter: PalwBondKeyV2,
        daa: u64,
        window: &dyn Fn(&Hash64) -> Option<(u64, u64)>,
        sign: &dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Option<(usize, PalwTrapRevealedV1)> {
        self.plans.iter().enumerate().find_map(|(index, plan)| {
            if !plan.committed || plan.revealed {
                return None;
            }
            let claim = plan.claim_id()?;
            let (audit_end, row_end) = window(&claim)?;
            if daa < audit_end || daa > row_end {
                return None;
            }
            let object =
                PalwTrapRevealedV1::sign_v1(network_domain, setter, claim, plan.fault_leaf, plan.tiles, plan.salt_bytes()?, |m, c| sign(m, c))?;
            Some((index, object))
        })
    }

    /// Record that plan `index`'s commitment has been carried.
    pub(crate) fn mark_committed(&mut self, index: usize) {
        if let Some(plan) = self.plans.get_mut(index) {
            plan.committed = true;
            self.save();
        }
    }

    /// Record that plan `index`'s reveal has been carried.
    pub(crate) fn mark_revealed(&mut self, index: usize) {
        if let Some(plan) = self.plans.get_mut(index) {
            plan.revealed = true;
            self.save();
        }
    }

    /// The `key=value` pairs of the node status.
    pub(crate) fn status_v1(&self) -> String {
        format!(
            "trap_plans={} trap_committed={} trap_revealed={}",
            self.plans.len(),
            self.plans.iter().filter(|p| p.committed).count(),
            self.plans.iter().filter(|p| p.revealed).count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_mesh_v1::{PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1, PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1, PALW_TRAP_SLOT_DAA_V1};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn setter(i: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([i; 64]), u32::from(i)))
    }

    fn plan(claim: &str) -> PalwTrapPlanV1 {
        PalwTrapPlanV1 { claim: claim.to_string(), fault_leaf: 2, tiles: 8, salt: "07".repeat(32), committed: false, revealed: false }
    }

    fn sign_with(context_wanted: &'static [u8]) -> impl Fn(&[u8], &[u8]) -> Option<Vec<u8>> {
        move |message, context| {
            assert_eq!(context, context_wanted, "signed under the right context");
            let mut signature = message.to_vec();
            signature.resize(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN, 0);
            Some(signature)
        }
    }

    fn drawn_daa(bond: &PalwBondKeyV2) -> u64 {
        (1..).map(|slot| slot * PALW_TRAP_SLOT_DAA_V1).find(|daa| palw_trap_slot_drawn_v1(bond, *daa)).unwrap()
    }

    #[test]
    fn an_audit_leaf_names_the_claim_by_its_draw_and_says_zero_for_a_match() {
        let claim = Hash64::from_bytes([9; 64]);
        let compact = palw_mesh_audit_leaf_v1(&claim, 123, 77, true, false);
        let PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Compact { bound_daa, id_prefix }, leaf, result } = compact else {
            panic!("a compact audited leaf")
        };
        assert_eq!((bound_daa, leaf, result, id_prefix), (123, 77, 0, [9u8; 16]));
        assert!(matches!(
            palw_mesh_audit_leaf_v1(&claim, 123, 77, false, true),
            PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Full(c), result: 1, .. } if c == claim
        ));
        // A DAA that does not fit 32 bits falls back to the whole id.
        assert!(matches!(
            palw_mesh_audit_leaf_v1(&claim, u64::MAX, 1, true, false),
            PalwVertexLeafV1::Audited { claim: PalwClaimRefV1::Full(_), .. }
        ));
    }

    #[test]
    fn the_trap_book_commits_in_a_drawn_slot_and_reveals_only_after_the_audit_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("palw-traps.json");
        let claim = Hash64::from_bytes([5; 64]);
        std::fs::write(&path, serde_json::to_vec(&vec![plan(&claim.to_string())]).unwrap()).unwrap();
        let mut book = PalwTrapBookV1::load(&path);
        assert_eq!(book.len(), 1);
        let domain = Hash64::from_bytes([0xD0; 64]);
        let me = setter(4);
        let drawn = drawn_daa(&me);
        let undrawn = (1..).map(|slot| slot * PALW_TRAP_SLOT_DAA_V1).find(|daa| !palw_trap_slot_drawn_v1(&me, *daa)).unwrap();
        let sign_commit = sign_with(PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1);
        assert!(book.next_commit(domain, me, undrawn, &sign_commit).is_none(), "not drawn in this slot: no commitment");
        let (index, commit) = book.next_commit(domain, me, drawn, &sign_commit).expect("drawn: commit");
        assert_eq!(commit.setter_bond, me);
        assert_eq!(commit.commitment, kaspa_consensus_core::palw_mesh_v1::palw_trap_commitment_v1(&claim, 2, 8, &[7u8; 32]));
        // No reveal before the commitment is carried; none before the audit window closes; none past the reveal window.
        let sign_reveal = sign_with(PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1);
        let window = |c: &Hash64| (*c == claim).then_some((1_000u64, 1_240u64));
        assert!(book.next_reveal(domain, me, 1_100, &window, &sign_reveal).is_none(), "uncommitted");
        book.mark_committed(index);
        assert!(book.next_commit(domain, me, drawn, &sign_commit).is_none(), "committed once");
        assert!(book.next_reveal(domain, me, 999, &window, &sign_reveal).is_none(), "the audit window is open");
        assert!(book.next_reveal(domain, me, 1_241, &window, &sign_reveal).is_none(), "past the reveal window");
        assert!(book.next_reveal(domain, me, 1_100, &|_| None, &sign_reveal).is_none(), "the chain holds no audit row");
        let (index, reveal) = book.next_reveal(domain, me, 1_100, &window, &sign_reveal).expect("reveal");
        assert_eq!((reveal.claim, reveal.fault_leaf, reveal.tiles), (claim, 2, 8));
        assert_eq!(reveal.commitment(), commit.commitment, "the reveal opens the commitment");
        book.mark_revealed(index);
        assert!(book.next_reveal(domain, me, 1_100, &window, &sign_reveal).is_none(), "revealed once");
        // The book survives a restart with its bookkeeping, and takes a plan added later.
        let mut again = PalwTrapBookV1::load(&path);
        assert!(again.next_commit(domain, me, drawn, &sign_commit).is_none() && again.status_v1().contains("trap_revealed=1"));
        let mut file: Vec<PalwTrapPlanV1> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        file.push(plan(&Hash64::from_bytes([6; 64]).to_string()));
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        again.refresh();
        assert_eq!(again.len(), 2);
        assert!(again.next_commit(domain, me, drawn, &sign_commit).is_some(), "the new plan is next");
    }

    #[test]
    fn a_bad_plan_file_or_a_bad_plan_is_ignored_never_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("palw-traps.json");
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(PalwTrapBookV1::load(&path).len(), 0);
        let mut bad = plan("zz");
        bad.salt = "00".to_string();
        std::fs::write(&path, serde_json::to_vec(&vec![bad]).unwrap()).unwrap();
        let book = PalwTrapBookV1::load(&path);
        assert_eq!(book.len(), 1);
        let me = setter(1);
        let sign = |_: &[u8], _: &[u8]| Some(vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]);
        assert!(book.next_commit(Hash64::from_bytes([1; 64]), me, drawn_daa(&me), &sign).is_none(), "a plan with a bad claim or salt carries nothing");
    }
}
