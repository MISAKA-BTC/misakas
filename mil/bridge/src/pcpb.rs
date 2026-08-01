//! **Seam 5 — PCPB evidence production** (ADR-0045 D3-b, `docs/palw-pcpb-leaf-v2-wiring-design.md` §7).
//!
//! The consensus side of D3-b landed first: clauses 11/12 re-derive a leaf's job challenge and re-run
//! its dispatch evidence before the leaf may be stored, and clause 13 re-checks the anchor at mint.
//! Nothing on-chain produces that evidence — which is the failure mode ADR-0040 §5.15.9 names in one
//! sentence: *a verifier without a producer is a bricked lane, and it bricks SILENTLY* (the acceptance
//! arm's error is discarded by the virtual processor). This module is the producer.
//!
//! # What a producer must do that a verifier does not
//!
//! A verifier resolves the epoch snapshot from its own stores and only checks membership proofs the
//! evidence carries. A producer has to BUILD those proofs, and it cannot reconstruct "the bonded
//! provider set as of epoch e" from the current registry — the registry is current state, not
//! history. So the node serves the entry set it derived at the time ([`PcpbContext`], fetched over
//! `getPalwState`'s PCPB selector), and this module rebuilds the canonical tree from it with the
//! SAME `palw_build_snapshot_witnesses` consensus used, then checks the rebuilt roots against the
//! served commitment ([`PcpbContext::witness_set`]). A node that lies about entries is caught here,
//! by the producer, before it wastes a batch registration.
//!
//! # The two branches
//!
//! * **External** ([`external_witness`]): the scheduler issues a job challenge at epoch `E` (Seam 1),
//!   and the pair is drawn from `R_{E+Δ}` — a per-epoch draw, deliberately NOT per-job, so a
//!   requester cannot spam challenges and submit only the jobs whose draw it liked.
//! * **Self-serial** ([`SelfSerialFlow`]): A computes first, commits to its receipt, ANCHORS that
//!   commitment on-chain (`0x45`), and only then is B drawn from `R_{a_commit_epoch + Δ}`. The
//!   ordering is the whole point, and it is why the anchor's registration epoch is read back from
//!   the chain rather than declared: the leaf names an epoch, and clause 12 refuses unless the
//!   registry agrees the anchor was there at-or-before it.
//!
//! # Honest boundary
//!
//! This module is pure assembly and verification over values the caller supplies. It does not submit
//! transactions, does not wait, and does not hold keys. The state machine that polls for the anchor's
//! registration ([`SelfSerialFlow::step`]) is explicit about what it is waiting for, so a caller can
//! drive it from any transport without this module owning a runtime.

use kaspa_consensus_core::palw::{
    BeaconAssignedProof, PALW_DISPATCH_KIND_BEACON_ASSIGNED, PALW_DISPATCH_KIND_SELF_SERIAL, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT,
    PalwACommitV1, PalwDispatchEvidence, PalwLeafPcpbWitnessV1, PalwProviderSnapshotEntry, PalwSnapshotCommitment,
    PalwSnapshotWitnessSet, SelfSerialProof, palw_assignment_draw_seed, palw_build_snapshot_witnesses, palw_job_challenge,
    palw_pcpb_derive_b, palw_pcpb_receipt_preimage, palw_provider_id, palw_provider_pk_hash, palw_receipt_embeds_a_commit,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_ACOMMIT;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::{Hash64, blake2b_512_keyed};

/// Domain for the self-serial `a_commit` (design §4.1's `A_commit`). A is committing to "this exact
/// receipt, for this exact job, blinded by `r_blind`" — the blind is what keeps the commitment from
/// revealing the answer before B is drawn.
pub const BRIDGE_A_COMMIT_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/a-commit";

/// Why a PCPB witness could not be produced. Every variant is a REFUSAL to emit evidence that would
/// be rejected on-chain: a producer that guesses here pays a registration fee for an unmintable leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PcpbError {
    /// The node could not resolve the snapshot epoch (outside its retained window). Nothing to build
    /// against; ask an archival node or pick a fresher anchor.
    SnapshotUnavailable { epoch: u64 },
    /// The node served a commitment but no entries: it can verify that epoch, not help produce for it
    /// (a pruned joiner, see the prefix-70 note). Distinct from `SnapshotUnavailable` because the fix
    /// is different — this node is fine, it just cannot serve THIS.
    EntriesUnavailable { epoch: u64 },
    /// The rebuilt tree disagrees with the served roots. Either the entry set was tampered with in
    /// transit or the node is buggy; either way, evidence built on it would fail clause 0.
    SnapshotRootMismatch { epoch: u64 },
    /// `R_{anchor}` or `R_{anchor + Δ}` is not resolvable yet (or has aged out). For the draw seed
    /// on a fresh anchor this is the NORMAL state and means "wait", which is why the flow surfaces
    /// it as a step rather than swallowing it.
    SeedUnavailable { epoch: u64, what: &'static str },
    /// The draw did not land on any provider — only possible with an empty snapshot.
    DrawEmpty { slot: &'static str, total_bond: u128 },
    /// Both external slots drew the same provider, or A drew itself. Consensus rejects that pair (it
    /// is not a real k=2), so the honest response is a different anchor, never a substituted partner.
    DrawNotDistinct,
    /// A drawn provider is not one this bridge can reach / has no bond outpoint for.
    UnknownProvider { provider_id: String },
    /// B returned a receipt that does not embed `a_commit`, or whose signature does not verify under
    /// its committed key. Checked HERE so a bad partner is caught before the leaf is built.
    PartnerReceiptInvalid(&'static str),
    /// A self-serial flow was asked for evidence before its anchor was on-chain.
    AnchorNotRegistered,
}

impl std::fmt::Display for PcpbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SnapshotUnavailable { epoch } => {
                write!(f, "no provider snapshot at epoch {epoch} — the node's PCPB window does not cover it")
            }
            Self::EntriesUnavailable { epoch } => {
                write!(f, "node has epoch {epoch}'s roots but not its entry set (imported from a pruning snapshot)")
            }
            Self::SnapshotRootMismatch { epoch } => {
                write!(f, "rebuilt snapshot roots disagree with the served commitment at epoch {epoch} — refusing to build on it")
            }
            Self::SeedUnavailable { epoch, what } => write!(f, "beacon seed for epoch {epoch} is unavailable ({what})"),
            Self::DrawEmpty { slot, total_bond } => write!(f, "the {slot} draw selected no provider (total bond {total_bond})"),
            Self::DrawNotDistinct => write!(f, "the draw did not yield two distinct providers — this anchor cannot host the job"),
            Self::UnknownProvider { provider_id } => {
                write!(f, "drawn provider {provider_id} is not resolvable to a known bond outpoint")
            }
            Self::PartnerReceiptInvalid(reason) => write!(f, "partner B's receipt failed verification: {reason}"),
            Self::AnchorNotRegistered => write!(f, "the A-commit anchor is not registered on-chain yet"),
        }
    }
}

impl std::error::Error for PcpbError {}

/// The node-served PCPB context for one anchor epoch, plus the tree rebuilt from it.
///
/// Constructing this VALIDATES the served data (see [`PcpbContext::new`]), so every later step works
/// against a snapshot the producer has itself confirmed reduces to the committed roots.
#[derive(Clone, Debug)]
pub struct PcpbContext {
    pub anchor_epoch: u64,
    pub snapshot_epoch: u64,
    pub draw_epoch: u64,
    pub commitment: PalwSnapshotCommitment,
    witnesses: PalwSnapshotWitnessSet,
    /// `R_{anchor}` — clause 11's challenge re-derivation runs under this.
    pub anchor_seed: Hash64,
    /// `R_{anchor + Δ}` — both branches draw from this.
    pub draw_seed: Hash64,
}

impl PcpbContext {
    /// Rebuild and CHECK the served context.
    ///
    /// The check is the point: `palw_build_snapshot_witnesses` is the same canonicalization consensus
    /// applied when it committed the roots, so if the rebuilt roots differ, the entry set is not the
    /// one behind the commitment and every witness built from it would die at clause 0. Catching it
    /// here turns a silent on-chain rejection into a local error with a cause.
    pub fn new(
        anchor_epoch: u64,
        snapshot_epoch: u64,
        draw_epoch: u64,
        commitment: PalwSnapshotCommitment,
        entries: &[PalwProviderSnapshotEntry],
        anchor_seed: Hash64,
        draw_seed: Hash64,
    ) -> Result<Self, PcpbError> {
        if entries.is_empty() {
            return Err(PcpbError::EntriesUnavailable { epoch: snapshot_epoch });
        }
        let witnesses = palw_build_snapshot_witnesses(entries);
        if witnesses.commitment != commitment {
            return Err(PcpbError::SnapshotRootMismatch { epoch: snapshot_epoch });
        }
        Ok(Self { anchor_epoch, snapshot_epoch, draw_epoch, commitment, witnesses, anchor_seed, draw_seed })
    }

    /// Build a context from the node's RPC projection, returning it alongside the queried anchor's
    /// registration epoch.
    ///
    /// `Ok((None, epoch))` is a real, useful answer, not a failure: the node knows the A-commit's
    /// epoch but cannot yet resolve the draw seed (or the snapshot window does not reach). A
    /// self-serial producer polls exactly this shape until both halves arrive, which is the ordering
    /// guarantee expressing itself rather than an error to retry through.
    pub fn from_rpc(served: &kaspa_rpc_core::RpcPalwPcpbContext) -> Result<(Option<Self>, Option<u64>), PcpbError> {
        let acommit_epoch = served.acommit_epoch;
        let parse = |hex: &str| -> Option<Hash64> {
            if hex.is_empty() {
                None
            } else {
                hex.parse::<Hash64>().ok()
            }
        };
        let (Some(snapshot_root), Some(assignment_root)) = (parse(&served.snapshot_root), parse(&served.assignment_root)) else {
            // Outside the retained window: nothing to build on. Distinguished from "no entries" so
            // an operator can tell "ask an archival node" from "ask a different node".
            return Ok((None, acommit_epoch));
        };
        let (Some(anchor_seed), Some(draw_seed)) = (parse(&served.anchor_seed), parse(&served.draw_seed)) else {
            // The draw beacon has not closed yet — the honest producer WAITS here.
            return Ok((None, acommit_epoch));
        };
        let total_bond: u128 =
            served.total_bond.parse().map_err(|_| PcpbError::SnapshotRootMismatch { epoch: served.snapshot_epoch })?;
        let entries: Vec<PalwProviderSnapshotEntry> = served
            .entries
            .iter()
            .map(|e| {
                Ok(PalwProviderSnapshotEntry {
                    provider_id: parse(&e.provider_id).ok_or(PcpbError::SnapshotRootMismatch { epoch: served.snapshot_epoch })?,
                    ml_dsa_pk_hash: parse(&e.ml_dsa_pk_hash)
                        .ok_or(PcpbError::SnapshotRootMismatch { epoch: served.snapshot_epoch })?,
                    bond_sompi: e.bond_sompi,
                    reward_script_commitment: parse(&e.reward_script_commitment)
                        .ok_or(PcpbError::SnapshotRootMismatch { epoch: served.snapshot_epoch })?,
                })
            })
            .collect::<Result<_, PcpbError>>()?;
        let commitment = PalwSnapshotCommitment { snapshot_root, assignment_root, total_bond, provider_count: served.provider_count };
        let ctx = Self::new(
            served.anchor_epoch,
            served.snapshot_epoch,
            served.draw_epoch,
            commitment,
            &entries,
            anchor_seed,
            draw_seed,
        )?;
        Ok((Some(ctx), acommit_epoch))
    }

    /// The canonical witness set — producers pick slots from it, verifiers re-derive the same tree.
    pub fn witness_set(&self) -> &PalwSnapshotWitnessSet {
        &self.witnesses
    }

    /// The bond outpoint of a drawn slot, resolved through the SAME `palw_provider_id` derivation the
    /// acceptance arm applies to `leaf.provider_{a,b}_bond`. A producer must name the seat consensus
    /// will compute, not the one it happens to have on file.
    pub fn bond_of_slot(&self, slot: usize, bonds: &[TransactionOutpoint]) -> Result<TransactionOutpoint, PcpbError> {
        let id = self.witnesses.slots[slot].entry.provider_id;
        bonds
            .iter()
            .copied()
            .find(|outpoint| palw_provider_id(outpoint) == id)
            .ok_or_else(|| PcpbError::UnknownProvider { provider_id: id.to_string() })
    }
}

/// The five leaf fields a producer must copy verbatim onto its leaf, plus the seats the draw fixed.
///
/// Returned as one unit because they are only meaningful together: a leaf carrying the roots but the
/// wrong seats, or the right seats against another epoch's roots, is refused by clause 12.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcpbLeafBinding {
    pub a_commit: Hash64,
    pub a_commit_epoch: u64,
    pub provider_snapshot_root: Hash64,
    pub assignment_proof_root: Hash64,
    pub dispatch_kind: u8,
    pub provider_a_bond: TransactionOutpoint,
    pub provider_b_bond: TransactionOutpoint,
    /// `receipt_v3_issued_epoch` — the challenge epoch. On the external branch this IS the anchor.
    pub issued_epoch: u64,
    /// `receipt_v3_job_challenge` (and, for Object V2, `job_nullifier`).
    pub job_challenge: Hash64,
}

/// A produced witness and the leaf binding that must accompany it.
#[derive(Clone, Debug)]
pub struct ProducedWitness {
    pub binding: PcpbLeafBinding,
    pub witness: PalwLeafPcpbWitnessV1,
}

/// The job-scoped challenge preimage (Seam 1's triple).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobPreimage {
    pub scheduler_job_id: Hash64,
    pub requester_credential: Hash64,
    pub request_commitment: Hash64,
}

/// **External branch.** Draw the two slots from `R_{anchor+Δ}`, resolve them to bond outpoints, and
/// emit the witness plus the leaf binding.
///
/// The draw is re-run here exactly as the verifier will re-run it, so the producer cannot "choose" a
/// pair: the only freedom it has is which anchor epoch to condition on, and clause 11's freshness
/// window `w` bounds that.
pub fn external_witness(
    ctx: &PcpbContext,
    network_id: u32,
    preimage: JobPreimage,
    shape_id: u16,
    known_bonds: &[TransactionOutpoint],
) -> Result<ProducedWitness, PcpbError> {
    let slot_a = ctx
        .witnesses
        .select(&palw_assignment_draw_seed(&ctx.draw_seed, 0))
        .ok_or(PcpbError::DrawEmpty { slot: "slot A", total_bond: ctx.commitment.total_bond })?;
    let slot_b = ctx
        .witnesses
        .select(&palw_assignment_draw_seed(&ctx.draw_seed, 1))
        .ok_or(PcpbError::DrawEmpty { slot: "slot B", total_bond: ctx.commitment.total_bond })?;
    if slot_a == slot_b {
        return Err(PcpbError::DrawNotDistinct);
    }
    let job_challenge = palw_job_challenge(
        network_id,
        ctx.anchor_epoch,
        &ctx.anchor_seed,
        &preimage.scheduler_job_id,
        &preimage.requester_credential,
        &preimage.request_commitment,
        shape_id,
    );
    Ok(ProducedWitness {
        binding: PcpbLeafBinding {
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: ctx.commitment.snapshot_root,
            assignment_proof_root: ctx.commitment.assignment_root,
            dispatch_kind: PALW_DISPATCH_KIND_BEACON_ASSIGNED,
            provider_a_bond: ctx.bond_of_slot(slot_a, known_bonds)?,
            provider_b_bond: ctx.bond_of_slot(slot_b, known_bonds)?,
            issued_epoch: ctx.anchor_epoch,
            job_challenge,
        },
        witness: PalwLeafPcpbWitnessV1 {
            scheduler_job_id: preimage.scheduler_job_id,
            requester_credential: preimage.requester_credential,
            request_commitment: preimage.request_commitment,
            dispatch: PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof {
                slot_a: ctx.witnesses.slots[slot_a].clone(),
                slot_b: ctx.witnesses.slots[slot_b].clone(),
            }),
        },
    })
}

/// A's receipt commitment: `H(a-commit, job_descriptor ‖ receipt_fields ‖ r_blind)` (design §4.1).
///
/// `r_blind` is A's secret until the reveal; without it the commitment would leak the answer to
/// anyone watching the anchor, and B's draw is only meaningful if the answer is still hidden when it
/// happens.
pub fn a_commit(job_descriptor: &[u8], receipt_fields: &Hash64, r_blind: &[u8; 32]) -> Hash64 {
    let mut preimage = Vec::with_capacity(job_descriptor.len() + 64 + 32);
    preimage.extend_from_slice(&(job_descriptor.len() as u32).to_le_bytes());
    preimage.extend_from_slice(job_descriptor);
    preimage.extend_from_slice(receipt_fields.as_byte_slice());
    preimage.extend_from_slice(r_blind);
    blake2b_512_keyed(BRIDGE_A_COMMIT_DOMAIN, &preimage)
}

/// The `0x45` anchor transaction payload for `a_commit`, ready to be funded and submitted.
///
/// Returns `(subnetwork_byte, borsh payload)` in the same shape as the miner's registration builders.
pub fn acommit_payload(a_commit: Hash64) -> Result<(u8, Vec<u8>), PcpbError> {
    let payload = PalwACommitV1 { version: 1, a_commit };
    let bytes = borsh::to_vec(&payload).expect("PalwACommitV1 has an infallible Borsh encoding");
    Ok((SUBNETWORK_ID_PALW_ACOMMIT.palw_pcpb_tx_kind().expect("0x45 is the PCPB band"), bytes))
}

/// What a self-serial flow is waiting for. Explicit so a caller can drive the flow from any
/// transport (and log a cause) instead of guessing why no witness came out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelfSerialStep {
    /// Submit this `0x45` payload and keep polling `getPalwState` with `pcpb_a_commit`.
    SubmitAnchor { subnetwork_byte: u8, payload: Vec<u8> },
    /// The anchor is on-chain at `a_commit_epoch`; the draw beacon `R_{epoch + Δ}` is not closed yet.
    /// This is the ordering guarantee doing its job — B genuinely cannot be known before this.
    AwaitDrawBeacon { a_commit_epoch: u64, draw_epoch: u64 },
    /// B is drawn. Send `receipt_preimage` to the provider at `partner_bond` and collect its
    /// ML-DSA-87 signature over it under [`PALW_PCPB_RECEIPT_MLDSA87_CONTEXT`].
    AwaitPartnerReceipt { partner_bond: TransactionOutpoint, receipt_preimage: Vec<u8> },
    /// Everything needed is in hand.
    Ready,
}

/// The self-serial producer flow. Holds no keys and performs no I/O — the caller feeds it what it
/// learns and it says what is still missing.
#[derive(Clone, Debug)]
pub struct SelfSerialFlow {
    a_commit: Hash64,
    /// A's own bond — the seat the leaf declares for provider A. Clause 12 requires A to be a bonded
    /// member of the same snapshot, so self-ordering is a bonded privilege, not an open door.
    a_bond: TransactionOutpoint,
    preimage: JobPreimage,
    shape_id: u16,
    /// The receipt tail B signs after the `TAG ‖ a_commit` prefix.
    receipt_tail: Vec<u8>,
    a_commit_epoch: Option<u64>,
}

impl SelfSerialFlow {
    pub fn new(
        a_commit: Hash64,
        a_bond: TransactionOutpoint,
        preimage: JobPreimage,
        shape_id: u16,
        receipt_tail: Vec<u8>,
    ) -> Self {
        Self { a_commit, a_bond, preimage, shape_id, receipt_tail, a_commit_epoch: None }
    }

    /// Record the registration epoch the chain reported for this anchor.
    pub fn observe_anchor(&mut self, a_commit_epoch: u64) {
        self.a_commit_epoch = Some(a_commit_epoch);
    }

    /// What to do next, given what the node currently reports.
    ///
    /// `ctx` is `None` while the anchor is unregistered (there is no anchor epoch to ask about yet).
    /// `known_bonds` resolves the drawn partner to a bond outpoint the caller can route to; an empty
    /// slice yields the zero-index placeholder (provider id in the txid position, display only).
    pub fn step(&self, ctx: Option<&PcpbContext>, known_bonds: &[TransactionOutpoint]) -> Result<SelfSerialStep, PcpbError> {
        let Some(a_commit_epoch) = self.a_commit_epoch else {
            let (subnetwork_byte, payload) = acommit_payload(self.a_commit)?;
            return Ok(SelfSerialStep::SubmitAnchor { subnetwork_byte, payload });
        };
        let Some(ctx) = ctx else {
            return Ok(SelfSerialStep::AwaitDrawBeacon { a_commit_epoch, draw_epoch: a_commit_epoch });
        };
        if ctx.draw_seed == Hash64::default() {
            return Ok(SelfSerialStep::AwaitDrawBeacon { a_commit_epoch, draw_epoch: ctx.draw_epoch });
        }
        let partner_bond = self.partner_bond(ctx, known_bonds)?;
        Ok(SelfSerialStep::AwaitPartnerReceipt { partner_bond, receipt_preimage: self.receipt_preimage() })
    }

    /// The commitment this flow is anchored on — its identity everywhere (journal key, `0x45`
    /// payload, registry row, leaf field).
    pub fn a_commit(&self) -> Hash64 {
        self.a_commit
    }

    /// The exact bytes B must sign: `TAG ‖ a_commit ‖ tail`. Consensus checks the prefix itself
    /// ([`palw_receipt_embeds_a_commit`]), so this binding is verified, never declared.
    pub fn receipt_preimage(&self) -> Vec<u8> {
        palw_pcpb_receipt_preimage(&self.a_commit, &self.receipt_tail)
    }

    /// The slot the post-commit beacon drew for B, resolved to a bond outpoint when `known_bonds`
    /// contains it (an empty slice yields the zero outpoint, which callers use only for display).
    fn partner_bond(&self, ctx: &PcpbContext, known_bonds: &[TransactionOutpoint]) -> Result<TransactionOutpoint, PcpbError> {
        let slot = self.partner_slot(ctx)?;
        if known_bonds.is_empty() {
            return Ok(TransactionOutpoint::new(ctx.witness_set().slots[slot].entry.provider_id, 0));
        }
        ctx.bond_of_slot(slot, known_bonds)
    }

    fn partner_slot(&self, ctx: &PcpbContext) -> Result<usize, PcpbError> {
        ctx.witness_set()
            .select(&palw_pcpb_derive_b(&ctx.draw_seed, &self.a_commit))
            .ok_or(PcpbError::DrawEmpty { slot: "partner B", total_bond: ctx.commitment.total_bond })
    }

    /// Assemble the self-serial witness once B's signed receipt is in hand.
    ///
    /// Every check consensus will make is made here first — B is the drawn provider, its key hashes
    /// to the committed `ml_dsa_pk_hash`, the preimage embeds `a_commit`, and the signature verifies.
    /// A producer that skips these ships a leaf that dies at clause 12 after paying for the chunk.
    pub fn finish(
        &self,
        ctx: &PcpbContext,
        b_ml_dsa_pk: Vec<u8>,
        b_receipt_preimage: Vec<u8>,
        b_signature: Vec<u8>,
        known_bonds: &[TransactionOutpoint],
        network_id: u32,
    ) -> Result<ProducedWitness, PcpbError> {
        let a_commit_epoch = self.a_commit_epoch.ok_or(PcpbError::AnchorNotRegistered)?;
        let b_slot = self.partner_slot(ctx)?;
        let b = &ctx.witness_set().slots[b_slot];
        let a_slot = ctx
            .witness_set()
            .slots
            .iter()
            .position(|slot| slot.entry.provider_id == palw_provider_id(&self.a_bond))
            .ok_or_else(|| PcpbError::UnknownProvider { provider_id: palw_provider_id(&self.a_bond).to_string() })?;
        if a_slot == b_slot {
            // A drew itself. Self-pairing defeats k=2 and clause 12 rejects it, so the honest move is
            // to re-anchor with a fresh `r_blind` rather than to substitute a partner.
            return Err(PcpbError::DrawNotDistinct);
        }
        if palw_provider_pk_hash(&b_ml_dsa_pk) != b.entry.ml_dsa_pk_hash {
            return Err(PcpbError::PartnerReceiptInvalid("verification key does not hash to B's committed pk_hash"));
        }
        if !palw_receipt_embeds_a_commit(&b_receipt_preimage, &self.a_commit) {
            return Err(PcpbError::PartnerReceiptInvalid("receipt does not embed a_commit"));
        }
        if !matches!(
            kaspa_txscript::verify_mldsa87_with_context(
                &b_ml_dsa_pk,
                &b_receipt_preimage,
                &b_signature,
                PALW_PCPB_RECEIPT_MLDSA87_CONTEXT
            ),
            Ok(true)
        ) {
            return Err(PcpbError::PartnerReceiptInvalid("signature does not verify"));
        }
        let a = &ctx.witness_set().slots[a_slot];
        let job_challenge = palw_job_challenge(
            network_id,
            ctx.anchor_epoch,
            &ctx.anchor_seed,
            &self.preimage.scheduler_job_id,
            &self.preimage.requester_credential,
            &self.preimage.request_commitment,
            self.shape_id,
        );
        Ok(ProducedWitness {
            binding: PcpbLeafBinding {
                a_commit: self.a_commit,
                a_commit_epoch,
                provider_snapshot_root: ctx.commitment.snapshot_root,
                assignment_proof_root: ctx.commitment.assignment_root,
                dispatch_kind: PALW_DISPATCH_KIND_SELF_SERIAL,
                provider_a_bond: self.a_bond,
                provider_b_bond: ctx.bond_of_slot(b_slot, known_bonds)?,
                issued_epoch: ctx.anchor_epoch,
                job_challenge,
            },
            witness: PalwLeafPcpbWitnessV1 {
                scheduler_job_id: self.preimage.scheduler_job_id,
                requester_credential: self.preimage.requester_credential,
                request_commitment: self.preimage.request_commitment,
                dispatch: PalwDispatchEvidence::SelfSerial(SelfSerialProof {
                    a_commit: self.a_commit,
                    a_entry: a.entry.clone(),
                    a_snapshot_membership: a.snapshot_membership.clone(),
                    b_entry: b.entry.clone(),
                    b_snapshot_membership: b.snapshot_membership.clone(),
                    b_interval: b.interval.clone(),
                    b_assignment_membership: b.assignment_membership.clone(),
                    b_ml_dsa_pk,
                    b_receipt_preimage,
                    b_signature,
                }),
            },
        })
    }
}

// ---- journal / wire records --------------------------------------------------------------------
//
// The flow above is pure; these are its durable shadows. A self-serial flow spans epochs (commit →
// anchor burial → draw beacon → partner round-trip), so a bridge restart in the middle must resume
// from the journal rather than forget an anchored commitment — an anchor is on-chain money spent,
// and re-anchoring under a fresh `r_blind` would also re-roll B, which is exactly the freedom the
// ordering exists to deny.

/// A self-serial flow as journaled at open. Everything `SelfSerialFlow::new` needs, in hex — the
/// journal is JSONL and the receipt tail is caller bytes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PcpbSelfFlowRecordV1 {
    pub a_commit_hex: String,
    /// A's own bond (`txid:index`) — the seat the leaf will declare for provider A.
    pub a_bond: String,
    pub scheduler_job_id_hex: String,
    pub requester_credential_hex: String,
    pub request_commitment_hex: String,
    pub shape_id: u16,
    pub receipt_tail_hex: String,
}

impl PcpbSelfFlowRecordV1 {
    /// Rebuild the pure flow, optionally at an already-observed anchor epoch.
    pub fn to_flow(&self, a_commit_epoch: Option<u64>) -> Result<SelfSerialFlow, String> {
        let parse = crate::chain::parse_hash64;
        let mut flow = SelfSerialFlow::new(
            parse(&self.a_commit_hex).map_err(|e| format!("a_commit: {e}"))?,
            crate::chain::parse_outpoint(&self.a_bond).map_err(|e| format!("a_bond: {e}"))?,
            JobPreimage {
                scheduler_job_id: parse(&self.scheduler_job_id_hex).map_err(|e| format!("scheduler_job_id: {e}"))?,
                requester_credential: parse(&self.requester_credential_hex).map_err(|e| format!("requester_credential: {e}"))?,
                request_commitment: parse(&self.request_commitment_hex).map_err(|e| format!("request_commitment: {e}"))?,
            },
            self.shape_id,
            crate::match_key::decode_hex(&self.receipt_tail_hex).map_err(|e| format!("receipt_tail: {e}"))?,
        );
        if let Some(epoch) = a_commit_epoch {
            flow.observe_anchor(epoch);
        }
        Ok(flow)
    }
}

/// A produced witness as journaled and served: the five leaf fields + seats (the binding) in hex,
/// and the witness itself as borsh bytes — EXACTLY what `PalwLeafChunkV1.witnesses[i]` carries, so
/// the miner-side chunk builder consumes it without re-encoding.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PcpbProducedWitnessV1 {
    /// The job challenge the leaf must carry (`receipt_v3_job_challenge`) — the record's key, and
    /// what clause 11 re-derives. On the external branch this EQUALS the Seam-1 lease challenge:
    /// D3-b promoted the bridge derivation into consensus byte-for-byte (same domain string, same
    /// preimage), precisely so issued leases keep verifying.
    pub leaf_challenge_hex: String,
    /// The Seam-1 lease this witness was produced from (external branch only; equal to
    /// `leaf_challenge_hex` by the byte-parity above — kept as explicit provenance).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub lease_job_challenge_hex: Option<String>,
    pub dispatch_kind: u8,
    pub a_commit_hex: String,
    pub a_commit_epoch: u64,
    pub provider_snapshot_root_hex: String,
    pub assignment_proof_root_hex: String,
    pub provider_a_bond: String,
    pub provider_b_bond: String,
    pub issued_epoch: u64,
    /// borsh(`PalwLeafPcpbWitnessV1`), hex.
    pub witness_borsh_hex: String,
}

impl PcpbProducedWitnessV1 {
    pub fn from_produced(produced: &ProducedWitness, lease_job_challenge_hex: Option<String>) -> Self {
        let hex = crate::match_key::hash64_hex;
        Self {
            leaf_challenge_hex: hex(&produced.binding.job_challenge),
            lease_job_challenge_hex,
            dispatch_kind: produced.binding.dispatch_kind,
            a_commit_hex: hex(&produced.binding.a_commit),
            a_commit_epoch: produced.binding.a_commit_epoch,
            provider_snapshot_root_hex: hex(&produced.binding.provider_snapshot_root),
            assignment_proof_root_hex: hex(&produced.binding.assignment_proof_root),
            provider_a_bond: crate::chain::format_outpoint(&produced.binding.provider_a_bond),
            provider_b_bond: crate::chain::format_outpoint(&produced.binding.provider_b_bond),
            issued_epoch: produced.binding.issued_epoch,
            witness_borsh_hex: crate::match_key::bytes_hex(
                &borsh::to_vec(&produced.witness).expect("PalwLeafPcpbWitnessV1 has an infallible Borsh encoding"),
            ),
        }
    }

    /// Decode the wire witness back to the consensus type (what a chunk builder embeds).
    pub fn witness(&self) -> Result<PalwLeafPcpbWitnessV1, String> {
        let bytes = crate::match_key::decode_hex(&self.witness_borsh_hex)?;
        borsh::from_slice(&bytes).map_err(|e| format!("witness borsh: {e}"))
    }
}

/// [`SelfSerialStep`] on the wire, tagged by phase. `a_commit_epoch` rides along once observed so a
/// caller sees the flow's position without a second query.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum PcpbSelfStepWire {
    /// Fund and submit a `0x45` transaction with this payload, then keep polling.
    SubmitAnchor { subnetwork_byte: u8, payload_hex: String },
    AwaitDrawBeacon { a_commit_epoch: u64, draw_epoch: u64 },
    /// Send `receipt_preimage_hex` to the provider at `partner_bond` and collect its ML-DSA-87
    /// signature over it (context [`PALW_PCPB_RECEIPT_MLDSA87_CONTEXT`]).
    AwaitPartnerReceipt { a_commit_epoch: u64, partner_bond: String, receipt_preimage_hex: String },
    /// The witness exists; fetch it by `leaf_challenge_hex`.
    Ready { a_commit_epoch: u64, leaf_challenge_hex: String },
}

impl PcpbSelfStepWire {
    pub fn from_step(step: &SelfSerialStep, a_commit_epoch: Option<u64>) -> Self {
        match step {
            SelfSerialStep::SubmitAnchor { subnetwork_byte, payload } => Self::SubmitAnchor {
                subnetwork_byte: *subnetwork_byte,
                payload_hex: crate::match_key::bytes_hex(payload),
            },
            SelfSerialStep::AwaitDrawBeacon { a_commit_epoch, draw_epoch } => {
                Self::AwaitDrawBeacon { a_commit_epoch: *a_commit_epoch, draw_epoch: *draw_epoch }
            }
            SelfSerialStep::AwaitPartnerReceipt { partner_bond, receipt_preimage } => Self::AwaitPartnerReceipt {
                a_commit_epoch: a_commit_epoch.unwrap_or_default(),
                partner_bond: crate::chain::format_outpoint(partner_bond),
                receipt_preimage_hex: crate::match_key::bytes_hex(receipt_preimage),
            },
            SelfSerialStep::Ready => Self::Ready { a_commit_epoch: a_commit_epoch.unwrap_or_default(), leaf_challenge_hex: String::new() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw::{PalwDispatchLeafFacts, PalwMlDsaVerifier, palw_dispatch_evidence_valid};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;

    /// The production verifier binding, so these tests exercise the REAL clause-12 predicate rather
    /// than a stub that could agree with a producer bug.
    struct TxscriptVerifier;
    impl PalwMlDsaVerifier for TxscriptVerifier {
        fn verify(&self, vk: &[u8], msg: &[u8], ctx: &[u8], sig: &[u8]) -> bool {
            matches!(kaspa_txscript::verify_mldsa87_with_context(vk, msg, sig, ctx), Ok(true))
        }
    }

    fn th(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    struct Fixture {
        ctx: PcpbContext,
        bonds: Vec<TransactionOutpoint>,
        keys: Vec<mldsa::MLDSA87KeyPair>,
    }

    /// Four bonded providers, keyed by REAL ML-DSA-87 keypairs and identified by REAL bond
    /// outpoints, with the context built through `PcpbContext::new` (i.e. through the rebuild check).
    fn fixture(draw_seed: Hash64) -> Fixture {
        let mut keys = Vec::new();
        let mut bonds = Vec::new();
        let mut entries = Vec::new();
        for i in 0..4u8 {
            let kp = mldsa::generate_key_pair([i + 1; 32]);
            let outpoint = TransactionOutpoint::new(th(0xB0 + i), i as u32);
            entries.push(PalwProviderSnapshotEntry {
                provider_id: palw_provider_id(&outpoint),
                ml_dsa_pk_hash: palw_provider_pk_hash(kp.verification_key.as_ref()),
                bond_sompi: 250,
                reward_script_commitment: th(0xD0 + i),
            });
            bonds.push(outpoint);
            keys.push(kp);
        }
        let commitment = palw_build_snapshot_witnesses(&entries).commitment;
        let ctx = PcpbContext::new(9, 7, 11, commitment, &entries, th(0x11), draw_seed).expect("served context is coherent");
        Fixture { ctx, bonds, keys }
    }

    fn preimage() -> JobPreimage {
        JobPreimage { scheduler_job_id: th(0xE1), requester_credential: th(0xE2), request_commitment: th(0xE3) }
    }

    fn facts(w: &ProducedWitness) -> PalwDispatchLeafFacts {
        PalwDispatchLeafFacts {
            snapshot_root: w.binding.provider_snapshot_root,
            assignment_root: w.binding.assignment_proof_root,
            a_commit: w.binding.a_commit,
            dispatch_kind: w.binding.dispatch_kind,
            provider_a_id: palw_provider_id(&w.binding.provider_a_bond),
            provider_b_id: palw_provider_id(&w.binding.provider_b_bond),
        }
    }

    /// A served entry set that does not reduce to the served roots is REFUSED, not built on. This is
    /// the producer's own clause 0: catching it here converts a silent on-chain rejection (the
    /// acceptance arm's error is discarded) into a local error naming the epoch.
    #[test]
    fn a_tampered_entry_set_is_refused_before_any_evidence_is_built() {
        let f = fixture(th(0x40));
        let mut entries: Vec<_> = f.ctx.witness_set().slots.iter().map(|s| s.entry.clone()).collect();
        entries[1].bond_sompi += 1; // one satoshi of collateral that was never committed
        let err = PcpbContext::new(9, 7, 11, f.ctx.commitment, &entries, th(0x11), th(0x40)).unwrap_err();
        assert_eq!(err, PcpbError::SnapshotRootMismatch { epoch: 7 });

        // And an empty entry set is a DIFFERENT error, because the operator response differs: the
        // node is healthy, it simply imported that epoch from a pruning snapshot.
        let err = PcpbContext::new(9, 7, 11, f.ctx.commitment, &[], th(0x11), th(0x40)).unwrap_err();
        assert_eq!(err, PcpbError::EntriesUnavailable { epoch: 7 });
    }

    /// The external branch's output validates under the REAL clause-12 verifier, and the leaf seats
    /// it names are the ones the draw actually selected.
    #[test]
    fn external_witness_validates_under_the_real_verifier() {
        // Find a draw seed whose two slots are distinct — what an honest scheduler waits for.
        let (mut f, mut produced) = (fixture(th(0x40)), None);
        for k in 0..64u8 {
            f = fixture(th(0x40u8.wrapping_add(k)));
            match external_witness(&f.ctx, 110, preimage(), 1, &f.bonds) {
                Ok(w) => {
                    produced = Some(w);
                    break;
                }
                Err(PcpbError::DrawNotDistinct) => continue,
                Err(other) => panic!("unexpected producer error: {other}"),
            }
        }
        let w = produced.expect("some epoch draws a distinct pair");
        assert_eq!(w.binding.dispatch_kind, PALW_DISPATCH_KIND_BEACON_ASSIGNED);
        assert_eq!(w.binding.a_commit, Hash64::default(), "the external branch carries the anchor sentinels");
        assert_eq!(w.binding.a_commit_epoch, 0);
        assert_ne!(w.binding.provider_a_bond, w.binding.provider_b_bond);

        assert!(
            palw_dispatch_evidence_valid(&w.witness.dispatch, &f.ctx.commitment, &f.ctx.draw_seed, &facts(&w), &TxscriptVerifier),
            "the producer's external evidence must satisfy the verifier that will judge it"
        );

        // The challenge the binding carries is the one clause 11 re-derives from the witness triple.
        assert_eq!(
            w.binding.job_challenge,
            palw_job_challenge(
                110,
                f.ctx.anchor_epoch,
                &f.ctx.anchor_seed,
                &w.witness.scheduler_job_id,
                &w.witness.requester_credential,
                &w.witness.request_commitment,
                1
            )
        );
    }

    /// The full self-serial ordering: commit → anchor → wait for the draw beacon → collect B's signed
    /// receipt → witness. Each step is asserted, because the ORDER is the security property.
    #[test]
    fn self_serial_flow_orders_commit_anchor_draw_receipt() {
        let f = fixture(th(0x5E));
        let commitment = a_commit(b"job-descriptor", &th(0x77), &[0x33; 32]);
        let mut flow = SelfSerialFlow::new(commitment, f.bonds[0], preimage(), 1, b"self-tail".to_vec());

        // (1) Before anything is on-chain the only move is to submit the anchor.
        match flow.step(None, &f.bonds).unwrap() {
            SelfSerialStep::SubmitAnchor { subnetwork_byte, payload } => {
                assert_eq!(subnetwork_byte, 0x45);
                let decoded: PalwACommitV1 = borsh::from_slice(&payload).unwrap();
                assert_eq!(decoded.a_commit, commitment);
                // The payload the node will actually validate.
                assert_eq!(kaspa_consensus_core::palw::validate_palw_acommit_tx(&payload), Ok(()));
            }
            other => panic!("expected SubmitAnchor, got {other:?}"),
        }

        // (2) Anchored but the draw beacon has not closed: the flow WAITS. This is the ordering
        // guarantee — B is not knowable yet, and a producer that guessed would be picking a sybil.
        flow.observe_anchor(9);
        match flow.step(None, &f.bonds).unwrap() {
            SelfSerialStep::AwaitDrawBeacon { a_commit_epoch, .. } => assert_eq!(a_commit_epoch, 9),
            other => panic!("expected AwaitDrawBeacon, got {other:?}"),
        }

        // (3) With the draw beacon in hand the flow names B and the exact bytes it must sign.
        let (partner_bond, preimage_bytes) = match flow.step(Some(&f.ctx), &f.bonds).unwrap() {
            SelfSerialStep::AwaitPartnerReceipt { partner_bond, receipt_preimage } => (partner_bond, receipt_preimage),
            other => panic!("expected AwaitPartnerReceipt, got {other:?}"),
        };
        assert!(palw_receipt_embeds_a_commit(&preimage_bytes, &commitment), "B signs bytes that bind the commitment");
        let _ = partner_bond;

        // (4) B signs; the producer assembles and the REAL verifier accepts.
        let b_slot = f.ctx.witness_set().select(&palw_pcpb_derive_b(&f.ctx.draw_seed, &commitment)).unwrap();
        let b_id = f.ctx.witness_set().slots[b_slot].entry.provider_id;
        let b_key = f.bonds.iter().position(|o| palw_provider_id(o) == b_id).unwrap();
        // A must not be B; pick A's seat accordingly (a real A would re-anchor instead).
        let a_key = (0..4).find(|&i| i != b_key).unwrap();
        let mut flow = SelfSerialFlow::new(commitment, f.bonds[a_key], preimage(), 1, b"self-tail".to_vec());
        flow.observe_anchor(9);
        let sig = mldsa::sign(&f.keys[b_key].signing_key, &preimage_bytes, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x44; 32]).unwrap();
        let w = flow
            .finish(
                &f.ctx,
                f.keys[b_key].verification_key.as_ref().to_vec(),
                preimage_bytes.clone(),
                sig.as_ref().to_vec(),
                &f.bonds,
                110,
            )
            .expect("a well-formed self-serial round assembles");
        assert_eq!(w.binding.dispatch_kind, PALW_DISPATCH_KIND_SELF_SERIAL);
        assert_eq!(w.binding.a_commit_epoch, 9, "the leaf declares the epoch the CHAIN reported");
        assert!(
            palw_dispatch_evidence_valid(&w.witness.dispatch, &f.ctx.commitment, &f.ctx.draw_seed, &facts(&w), &TxscriptVerifier),
            "the producer's self-serial evidence must satisfy the verifier that will judge it"
        );
    }

    /// A bad partner is caught by the PRODUCER, before the leaf exists. Each rejection mirrors one
    /// clause-12 check, so a producer bug cannot ship evidence the chain will refuse.
    #[test]
    fn a_dishonest_partner_is_rejected_before_the_leaf_is_built() {
        let f = fixture(th(0x5E));
        let commitment = a_commit(b"job", &th(0x77), &[0x33; 32]);
        let b_slot = f.ctx.witness_set().select(&palw_pcpb_derive_b(&f.ctx.draw_seed, &commitment)).unwrap();
        let b_id = f.ctx.witness_set().slots[b_slot].entry.provider_id;
        let b_key = f.bonds.iter().position(|o| palw_provider_id(o) == b_id).unwrap();
        let a_key = (0..4).find(|&i| i != b_key).unwrap();
        let mut flow = SelfSerialFlow::new(commitment, f.bonds[a_key], preimage(), 1, b"tail".to_vec());
        flow.observe_anchor(9);
        let good_preimage = flow.receipt_preimage();
        let good_sig =
            mldsa::sign(&f.keys[b_key].signing_key, &good_preimage, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x44; 32]).unwrap();
        let good_pk = f.keys[b_key].verification_key.as_ref().to_vec();

        // (a) a receipt that binds a DIFFERENT commitment.
        let wrong = palw_pcpb_receipt_preimage(&th(0xFF), b"tail");
        let wrong_sig = mldsa::sign(&f.keys[b_key].signing_key, &wrong, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x45; 32]).unwrap();
        assert_eq!(
            flow.finish(&f.ctx, good_pk.clone(), wrong, wrong_sig.as_ref().to_vec(), &f.bonds, 110).unwrap_err(),
            PcpbError::PartnerReceiptInvalid("receipt does not embed a_commit")
        );

        // (b) a forged signature.
        let mut forged = good_sig.as_ref().to_vec();
        forged[0] ^= 0x01;
        assert_eq!(
            flow.finish(&f.ctx, good_pk.clone(), good_preimage.clone(), forged, &f.bonds, 110).unwrap_err(),
            PcpbError::PartnerReceiptInvalid("signature does not verify")
        );

        // (c) a DIFFERENT provider answering in B's place — its key does not hash to B's committed
        // pk_hash, which is the check that stops A from swapping in a friendly partner.
        let other = (0..4).find(|&i| i != b_key).unwrap();
        let other_sig =
            mldsa::sign(&f.keys[other].signing_key, &good_preimage, PALW_PCPB_RECEIPT_MLDSA87_CONTEXT, [0x46; 32]).unwrap();
        assert_eq!(
            flow.finish(
                &f.ctx,
                f.keys[other].verification_key.as_ref().to_vec(),
                good_preimage.clone(),
                other_sig.as_ref().to_vec(),
                &f.bonds,
                110
            )
            .unwrap_err(),
            PcpbError::PartnerReceiptInvalid("verification key does not hash to B's committed pk_hash")
        );

        // (d) A trying to be its own partner.
        let mut self_pair = SelfSerialFlow::new(commitment, f.bonds[b_key], preimage(), 1, b"tail".to_vec());
        self_pair.observe_anchor(9);
        assert_eq!(
            self_pair.finish(&f.ctx, good_pk, good_preimage, good_sig.as_ref().to_vec(), &f.bonds, 110).unwrap_err(),
            PcpbError::DrawNotDistinct
        );
    }

    /// The node's RPC projection round-trips into a usable context, and its "cannot serve" shapes
    /// come back as `None` rather than as zeros a producer might build on.
    #[test]
    fn rpc_projection_round_trips_and_absences_stay_absent() {
        let f = fixture(th(0x40));
        let hex = |h: &Hash64| h.to_string();
        let served = kaspa_rpc_core::RpcPalwPcpbContext {
            anchor_epoch: 9,
            snapshot_epoch: 7,
            draw_epoch: 11,
            snapshot_root: hex(&f.ctx.commitment.snapshot_root),
            assignment_root: hex(&f.ctx.commitment.assignment_root),
            total_bond: f.ctx.commitment.total_bond.to_string(),
            provider_count: f.ctx.commitment.provider_count,
            entries: f
                .ctx
                .witness_set()
                .slots
                .iter()
                .map(|s| kaspa_rpc_core::RpcPalwSnapshotEntry {
                    provider_id: hex(&s.entry.provider_id),
                    ml_dsa_pk_hash: hex(&s.entry.ml_dsa_pk_hash),
                    bond_sompi: s.entry.bond_sompi,
                    reward_script_commitment: hex(&s.entry.reward_script_commitment),
                })
                .collect(),
            anchor_seed: hex(&f.ctx.anchor_seed),
            draw_seed: hex(&f.ctx.draw_seed),
            acommit_epoch: Some(9),
        };
        let (ctx, acommit_epoch) = PcpbContext::from_rpc(&served).expect("a coherent projection rebuilds");
        let ctx = ctx.expect("roots and seeds were all present");
        assert_eq!(acommit_epoch, Some(9));
        assert_eq!(ctx.commitment, f.ctx.commitment, "the rebuilt commitment must equal the served one");

        // An unclosed draw beacon is `None` + the anchor epoch — "wait", not "error", and definitely
        // not a zero seed a producer would draw against.
        let mut waiting = served.clone();
        waiting.draw_seed = String::new();
        let (ctx, acommit_epoch) = PcpbContext::from_rpc(&waiting).unwrap();
        assert!(ctx.is_none(), "an unresolvable draw seed yields no context");
        assert_eq!(acommit_epoch, Some(9), "...while still reporting what the chain DOES know");

        // A node that serves roots but no entries can verify that epoch, not help produce for it.
        let mut pruned = served.clone();
        pruned.entries.clear();
        assert_eq!(PcpbContext::from_rpc(&pruned).unwrap_err(), PcpbError::EntriesUnavailable { epoch: 7 });

        // And a doctored entry set is refused rather than built on.
        let mut doctored = served;
        doctored.entries[0].bond_sompi += 1;
        assert_eq!(PcpbContext::from_rpc(&doctored).unwrap_err(), PcpbError::SnapshotRootMismatch { epoch: 7 });
    }

    /// Evidence cannot be assembled before the anchor is on-chain: without a registration epoch there
    /// is nothing for the leaf to declare and nothing for clause 12 to compare against.
    #[test]
    fn evidence_requires_an_on_chain_anchor() {
        let f = fixture(th(0x5E));
        let commitment = a_commit(b"job", &th(0x77), &[0x33; 32]);
        let flow = SelfSerialFlow::new(commitment, f.bonds[0], preimage(), 1, b"tail".to_vec());
        assert_eq!(
            flow.finish(&f.ctx, vec![0u8; 2592], vec![], vec![0u8; 4627], &f.bonds, 110).unwrap_err(),
            PcpbError::AnchorNotRegistered
        );
    }
}
