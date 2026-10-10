//! **A mini consensus consumer around `KernelLedgerV1`** for the ledger tests: it drives the per-object API exactly as a
//! consensus fold would (`begin_block`, each input, `tick`), maps the harness's transactions onto signed route objects and
//! consumer-derived inputs, keeps a reference [`SettlementBookV1`] that applies every settlement instruction, and checks on the way:
//!
//! * a refused object leaves the state root byte-identical;
//! * every object round-trips its strict canonical encoding;
//! * the settlement instructions are applicable to a real bond book, conserve what a slash takes, and leave the book's collateral
//!   and reservations equal to the ledger's own view of every bond.
#![allow(dead_code)]

use misaka_palw_kernel::evidence::VerificationEvidenceV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerBlockV1, LedgerEventV1, LedgerTxV1, ProsecutionV1, SaltedCommitV1,
};
use misaka_palw_kernel::pipeline::PipelineEvidenceV1;
use misaka_palw_kernel::pipeline::PipelinePlanV1;
use misaka_palw_kernel::pipeline_public::{PipelineClaimV1, PipelineJobPostV1, StageCommitmentsV1};
use misaka_palw_kernel::plan::VerificationPlanV1;
use misaka_palw_kernel::settle::{SettlementBookV1, SettlementInstructionV1};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::MapParams;

/// **GAP-5: the harness's job poster** — a consumer bond that posts every job (and pays its price, which funds the job's Final
/// reward), so the producers' own collateral reads exactly as it did before jobs had a price. Every world registers it.
pub const POSTER: Digest = [0x9B; 64];
/// What every world registers [`POSTER`] with.
pub const POSTER_COLLATERAL: u64 = 1_000_000;

/// The harness's transactions: what the ledger tests have always written. `into_txs` signs each with the actor it names (or the
/// harness signer) and splits the consumer-derived inputs from the signed objects.
#[derive(Clone, Debug)]
pub enum T {
    /// Consumer-derived: the bond's real locked collateral.
    RegisterBond {
        bond: Digest,
        collateral: u64,
    },
    RequestExit {
        bond: Digest,
    },
    Withdraw {
        bond: Digest,
    },
    /// The harness holds the artifact; the ledger gets only its commitments, after the consumer attests the artifact public.
    RegisterClass {
        descriptor: Digest,
        program_bytes: Vec<u8>,
        plan: VerificationPlanV1,
        params: MapParams,
    },
    RegisterPipelineClass {
        descriptor: Digest,
        pipeline_bytes: Vec<u8>,
        program_bytes: Vec<Vec<u8>>,
        plan: PipelinePlanV1,
        params: Vec<MapParams>,
        decode: Option<DecodeRuleV1>,
    },
    PostJob {
        job: KernelJobV1,
    },
    PostPipelineJob {
        job: PipelineJobPostV1,
    },
    CommitClaim {
        claim: KernelClaimV1,
        evidence: VerificationEvidenceV1,
        commitments: Vec<Vec<Vec<Digest>>>,
    },
    CommitPipelineClaim {
        claim: PipelineClaimV1,
        evidence: PipelineEvidenceV1,
        stages: Vec<StageCommitmentsV1>,
    },
    /// Consumer-derived from the Panel's receipts.
    PanelCovered {
        claim: Digest,
    },
    FileProof {
        accuser: Digest,
        claim: Digest,
        proof: ProsecutionV1,
    },
    FileDemand {
        demander: Digest,
        claim: Digest,
        stage: u8,
        position: u32,
    },
    Respond {
        claim: Digest,
        stage: u8,
        position: u32,
        bytes: Vec<u8>,
    },
    SealClaim {
        producer: Digest,
        job: Digest,
        seal: Digest,
    },
    /// GAP-R7: an accuser's seal of its proof.
    SealProof {
        accuser: Digest,
        claim: Digest,
        seal: Digest,
    },
    /// OPV-BOOT GAP-B1a: a salted reveal (what [`salted`] makes of a commit past `palw_panel_free_v1`).
    CommitClaimSalted {
        salt: Digest,
        commit: SaltedCommitV1,
    },
}

/// The harness's salt of a claim (a producer draws its own from its CSPRNG).
pub fn test_salt(claim_id: &Digest) -> Digest {
    misaka_palw_kernel::hash::id(b"misaka-palw/test/claim-salt", claim_id)
}

/// **The commits among `txs` as `l` must see them**: past `palw_panel_free_v1` (at the DAA the harness seals them, the ledger's
/// clock) every reveal carries its seal's salt (OPV-BOOT GAP-B1a), so each commit becomes a salted reveal under [`test_salt`];
/// before the fence (or with no OPV policy) `txs` is returned as it is.
pub fn salted(l: &KernelLedgerV1, txs: Vec<T>) -> Vec<T> {
    if !l.salted_seals_from().is_some_and(|at| l.daa >= at) {
        return txs;
    }
    txs.into_iter()
        .map(|t| match t {
            T::CommitClaim { claim, evidence, commitments } => {
                T::CommitClaimSalted { salt: test_salt(&claim.id()), commit: SaltedCommitV1::Claim { claim, evidence, commitments } }
            }
            T::CommitPipelineClaim { claim, evidence, stages } => {
                T::CommitClaimSalted { salt: test_salt(&claim.id()), commit: SaltedCommitV1::Pipeline { claim, evidence, stages } }
            }
            other => other,
        })
        .collect()
}

/// The seals the claims among `txs` need (the harness seals every claim one block before revealing it, as a producer would).
pub fn seals_for(txs: &[T]) -> Vec<T> {
    use misaka_palw_kernel::ledger::{claim_seal_v1, claim_seal_v2};
    txs.iter()
        .filter_map(|t| match t {
            T::CommitClaimSalted { salt, commit } => {
                let (job, id) = match commit {
                    SaltedCommitV1::Claim { claim, .. } => (claim.job_id, claim.id()),
                    SaltedCommitV1::Pipeline { claim, .. } => (claim.job_id, claim.id()),
                    SaltedCommitV1::Spec { claim } => (claim.job_id(), claim.id()),
                };
                Some(T::SealClaim { producer: commit.producer(), job, seal: claim_seal_v2(&id, salt) })
            }
            T::CommitClaim { claim, .. } => {
                Some(T::SealClaim { producer: claim.producer_bond, job: claim.job_id, seal: claim_seal_v1(&claim.id()) })
            }
            T::CommitPipelineClaim { claim, .. } => {
                Some(T::SealClaim { producer: claim.producer_bond, job: claim.job_id, seal: claim_seal_v1(&claim.id()) })
            }
            _ => None,
        })
        .collect()
}

impl T {
    /// The signed object of an object transaction (panics for the consumer-derived ones).
    pub fn signed(self, signer: Digest) -> (AuthV1, O) {
        for tx in self.into_txs(signer) {
            if let LedgerTxV1::Object { auth, object } = tx {
                return (auth, object);
            }
        }
        panic!("not an object transaction")
    }

    /// The ledger inputs of this transaction, signed by the actor it names (`signer` for objects that name none).
    pub fn into_txs(self, signer: Digest) -> Vec<LedgerTxV1> {
        let obj = |auth: Digest, object: O| LedgerTxV1::Object { auth: AuthV1 { signer_bond: auth }, object };
        match self {
            T::RegisterBond { bond, collateral } => vec![LedgerTxV1::SyncBond { bond, collateral }],
            T::PanelCovered { claim } => vec![LedgerTxV1::PanelCovered { claim }],
            T::RequestExit { bond } => vec![obj(bond, O::RequestExit { bond })],
            T::Withdraw { bond } => vec![obj(bond, O::Withdraw { bond })],
            T::RegisterClass { descriptor, program_bytes, plan, params } => {
                let pc = ParamCommitmentsV1::of(&params);
                vec![
                    LedgerTxV1::AttestArtifact { artifact_root: pc.root() },
                    obj(signer, O::RegisterClass { descriptor, program_bytes, plan, param_commitments: pc }),
                ]
            }
            T::RegisterPipelineClass { descriptor, pipeline_bytes, program_bytes, plan, params, decode } => {
                let pcs: Vec<ParamCommitmentsV1> = params.iter().map(ParamCommitmentsV1::of).collect();
                let mut v: Vec<LedgerTxV1> = pcs.iter().map(|p| LedgerTxV1::AttestArtifact { artifact_root: p.root() }).collect();
                v.push(obj(
                    signer,
                    O::RegisterPipelineClass { descriptor, pipeline_bytes, program_bytes, plan, param_commitments: pcs, decode },
                ));
                v
            }
            T::PostJob { job } => vec![obj(POSTER, O::PostJob { job })],
            T::PostPipelineJob { job } => vec![obj(POSTER, O::PostPipelineJob { job })],
            T::CommitClaim { claim, evidence, commitments } => {
                vec![obj(claim.producer_bond, O::CommitClaim { claim, evidence, commitments })]
            }
            T::CommitPipelineClaim { claim, evidence, stages } => {
                vec![obj(claim.producer_bond, O::CommitPipelineClaim { claim, evidence, stages })]
            }
            T::FileProof { accuser, claim, proof } => vec![obj(accuser, O::FileProof { accuser, claim, proof })],
            T::FileDemand { demander, claim, stage, position } => {
                vec![obj(demander, O::FileDemand { demander, claim, stage, position })]
            }
            T::Respond { claim, stage, position, bytes } => vec![obj(signer, O::Respond { claim, stage, position, bytes })],
            T::SealClaim { producer, job, seal } => vec![obj(producer, O::SealClaim { producer, job, seal })],
            T::SealProof { accuser, claim, seal } => vec![obj(accuser, O::SealProof { accuser, claim, seal })],
            T::CommitClaimSalted { salt, commit } => vec![obj(commit.producer(), O::CommitClaimSalted { salt, commit })],
        }
    }
}

/// A block of harness transactions.
pub fn block_of(daa: u64, txs: Vec<T>, signer: Digest) -> LedgerBlockV1 {
    LedgerBlockV1 { daa, txs: txs.into_iter().flat_map(|t| t.into_txs(signer)).collect() }
}

/// The consumer: a bond book and the receipts it applied.
#[derive(Clone, Debug, Default)]
pub struct Consumer {
    pub book: SettlementBookV1,
    pub settlements: Vec<(u64, SettlementInstructionV1)>,
    /// The conservation identity's inputs (the user's ruling, 2026-10-09): Σ collateral the consumer declared through `SyncBond`
    /// (deltas), and Σ collateral that left the route by a `Withdraw`.
    pub declared: i128,
    pub withdrawn: i128,
}

impl Consumer {
    /// Paid out to `bond`'s owner so far (accuser rewards, default shares, final rewards).
    pub fn paid(&self, bond: &Digest) -> u64 {
        self.book.paid.get(bond).copied().unwrap_or(0)
    }

    fn absorb(&mut self, l: &KernelLedgerV1, ev: &[LedgerEventV1]) {
        for e in ev {
            if let LedgerEventV1::Settlement(s) = e {
                self.settlements.push((l.daa, *s));
                if s.kind == misaka_palw_kernel::settle::SettlementKindV1::Withdraw {
                    self.withdrawn += i128::from(s.amount);
                }
            }
        }
        self.book.apply_events(ev).unwrap_or_else(|why| panic!("a settlement the bond book refuses: {why}"));
    }

    /// The ledger's view of every bond equals the book's.
    pub fn check_view(&self, l: &KernelLedgerV1) {
        for (bond, row) in &l.bonds {
            assert_eq!(self.book.collateral.get(bond).copied().unwrap_or(0), row.collateral, "collateral of {:02x?}", &bond[..2]);
            assert_eq!(self.book.reserved.get(bond).copied().unwrap_or(0), row.reserved, "reserved of {:02x?}", &bond[..2]);
        }
        assert_eq!(l.burned, self.book.burned, "the burn checksum is the burn instructions'");
        self.book.balanced().unwrap();
        // **Conservation** (the user's ruling, 2026-10-09): every unit the consumer declared is still a bond's collateral (reserved or
        // free), was paid out (rewards, shares, Final rewards — each out of a debit), was burned, or left by a withdrawal. Nothing is
        // minted: escrow, burn and payouts agree in every ledger test.
        let collateral: i128 = self.book.collateral.values().map(|c| i128::from(*c)).sum();
        let paid: i128 = self.book.paid.values().map(|p| i128::from(*p)).sum();
        assert_eq!(
            collateral + paid + i128::from(self.book.burned) + self.withdrawn,
            self.declared,
            "conservation: collateral {collateral} + paid {paid} + burned {} + withdrawn {} ≠ declared {}",
            self.book.burned,
            self.withdrawn,
            self.declared
        );
    }

    /// Apply one block as a consensus fold would; returns the receipts without the settlement instructions (those are in the book
    /// and in `settlements`).
    pub fn apply(&mut self, l: &mut KernelLedgerV1, b: &LedgerBlockV1) -> Vec<LedgerEventV1> {
        let mut out = Vec::new();
        if l.begin_block(b.daa).is_err() {
            return out; // a stale block is not applied
        }
        for tx in &b.txs {
            let before = l.root();
            let r = match tx {
                LedgerTxV1::SyncBond { bond, collateral } => {
                    l.sync_bond(*bond, *collateral);
                    let old = self.book.collateral.get(bond).copied().unwrap_or(0);
                    self.declared += i128::from(*collateral) - i128::from(old);
                    self.book.set_collateral(*bond, *collateral);
                    Ok(Vec::new())
                }
                LedgerTxV1::AttestArtifact { artifact_root } => {
                    l.attest_artifact(*artifact_root);
                    Ok(Vec::new())
                }
                LedgerTxV1::PanelCovered { claim } => l.apply_panel_tally(claim, true),
                LedgerTxV1::AdmitOptimisticClass { class } => l.admit_optimistic_class(*class).map(|()| Vec::new()),
                LedgerTxV1::Object { auth, object } => {
                    assert_eq!(&O::decode(&object.encode()).expect("canonical"), object, "{} round-trips", object.name());
                    l.apply_object(object, auth)
                }
            };
            match r {
                Ok(ev) => {
                    self.absorb(l, &ev);
                    out.extend(ev);
                }
                Err(r) => {
                    assert_eq!(l.root(), before, "a refusal must leave the state byte-identical: {r}");
                    out.push(LedgerEventV1::Refused { tx: r.object, why: r.why });
                }
            }
        }
        let ev = l.tick();
        self.absorb(l, &ev);
        out.extend(ev);
        self.check_view(l);
        out.retain(|e| !matches!(e, LedgerEventV1::Settlement(_)));
        out
    }
}
