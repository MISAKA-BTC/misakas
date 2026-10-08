//! **RFC-0015 Panel=0 on the kernel route: `OptimisticPublicVerification` (OPV), dormant.**
//!
//! A claim of an OPV class has no Panel. It is committed, its fixed public challenge window opens at once
//! ([`crate::lifecycle::ClaimStateV1::Challengeable`]), and it reaches Final only when the window has closed, no accepted dispute is
//! open, the retention obligation is met and the proof grace after the latest service has elapsed. Everything an ordinary bond
//! needs to stop it — demands for committed material, exact fault proofs, the default path for withheld material, post-Final
//! liability — is the kernel route's, unchanged. What this module adds is the part that used to be the Panel's:
//!
//! * **identity** — the mode is bound into the class id ([`crate::mode`]); registration of an OPV class needs this ledger's
//!   [`OpvPolicyV1`] to exist and its activation height to be reached (derived by the consumer from the `palw_panel_free_v1`
//!   fence), the code-derived `PUBLIC_PROSECUTION_COMPLETE` gate (always), and the declared worst filing / response / commitments
//!   to fit the carriers (RFC-0015 §4.2, §6.3);
//! * **time** — [`OpvWindowV1`] and [`OpvBudgetsV1`]: the window, the budgets of a fresh verifier's path (cold material fetch,
//!   check, localization, disclosure, court, carrier inclusion, reorg slack) and the relations that make "a verifier that starts at
//!   the cutoff still reaches the court before the hard deadline" a validated property of the policy, not a hope (RFC-0015 §6.1);
//! * **economics** — [`OpvEconomicsV1`]: the producer's reservation is sized to the claim's maximum gain plus the default penalty
//!   (and to the detection probability the network's operating assumptions claim), there are no Panel locks and no signer-lock
//!   division, concurrent claims cannot reuse a collateral (the bond's free collateral is the only budget), the bounty is a share
//!   of the **collected** slash, and fake-fraud and fake-default loops burn part of what they cycle (RFC-0015 §8);
//! * **job holding** — an OPV claim holds its job from its first reveal (a junk claim is prosecutable and slashed by any bond, so
//!   squatting a job costs the squatter its reservation or its default penalty);
//! * **Final facts** — [`FinalReceiptV1`] carries what RFC-0010's Panel-assignment beacon needs of a Final: canonical work id,
//!   execution commitment, accepted and settlement position, DA status and the path ([`FinalReceiptV1::to_work_final_event`]).
//!   An OPV Final is `FinalPathV1::PanelIndependent` and never anything else; a Panel-licensed Final is `PanelLicensed` and
//!   never anything else.
//!
//! # What this is not
//!
//! * **Not a proof that a computation is correct.** "No challenge" is displayed as exactly that ([`FinalAssuranceV1`]); the safety
//!   assumption — at least one capable honest verifier checks each claim in time — is the network's operating assumption, not
//!   something a kernel can establish.
//! * **Not armed.** Every default is dormant; the parameters here are *relations with validation*, not numbers chosen for a
//!   network. Choosing them, measuring the budgets on real hardware and the monitoring economics are external gates
//!   (`docs/design/palw/rfc-0015-panel-free-record.md`).
//! * **Not a replacement for the Panel route.** Panel-licensed classes keep their ids, their lifecycle and (while no OPV policy is
//!   set) their state root byte for byte.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::{FinalPathV1, WorkFinalEventV1, WorkSourceKindV1};

use crate::hash::{Digest, object_id};
use crate::ledger::{ClaimBodyV1, ClaimRowV1, KernelLedgerV1, KernelRefusalV1, LedgerPolicyV1};
use crate::lifecycle::ClaimStateV1;
use crate::mode::VerificationModeV1;
use crate::state::collection_root;

/// The version of the root form that commits the OPV state ([`StateRootPartsV2`]).
pub const OPV_STATE_VERSION_V2: u16 = 2;
pub const OPV_ROOT_DOMAIN_V2: &[u8] = b"misaka-palw/kernel/ledger-state-root/v2-opv";
pub const OPV_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/opv-policy/v1";
pub const CANONICAL_WORK_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/canonical-work/v1";
pub const EXECUTION_COMMITMENT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/execution-commitment/v1";

// ── policy ──────────────────────────────────────────────────────────────────────────────────────────────────────────────

/// The fixed public challenge window of an OPV claim, from its inclusion: the base window plus the class verification horizon
/// (RFC-0015 §6.2: "base challenge window + class verification horizon"). Both are fixed when the claim is admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OpvWindowV1 {
    pub base_challenge_window_daa: u64,
    pub verification_horizon_daa: u64,
}

/// The budgets (in DAA — the unit the network clock counts; seconds are never silently equated with it) of the path a **fresh**
/// verifier takes, RFC-0015 §6.1. They are claims about the operating network, measured outside this crate; the policy's
/// [`OpvPolicyV1::validate`] only checks that the clock rules leave enough room for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OpvBudgetsV1 {
    /// Fetching the cold class / artifact / claim material.
    pub cold_material_daa: u64,
    /// Running the check over the fetched material.
    pub check_daa: u64,
    /// Localizing a mismatch to one position / node.
    pub localize_daa: u64,
    /// The producer's on-chain disclosure of a demanded position.
    pub disclose_daa: u64,
    /// Assembling and having the court accept the exact proof.
    pub court_daa: u64,
    /// One object's inclusion in the canonical chain.
    pub carrier_daa: u64,
    /// Reorg slack.
    pub reorg_slack_daa: u64,
}

/// The capacities of the objects that carry a class's worst filing, response and commitments on this network (all bounded by the
/// route's own hard ceilings; `carrier_fit_v1` decides).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CarrierCapsV1 {
    pub filing_cap: u64,
    pub response_cap: u64,
    pub commit_cap: u64,
}

/// **RFC-0015 §8: the producer-centred economics once the Panel's signer locks are gone.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OpvEconomicsV1 {
    /// Locked from the producer's FREE collateral for each OPV claim, until its liability horizon ends. A claim that cannot
    /// reserve this much free collateral is refused at inclusion (no double use of one collateral by concurrent claims).
    pub reservation_per_claim: u64,
    /// The work / weight credit the consumer releases at Final besides `claim_reward` (the same unit as the reward). Part of the
    /// claim's maximum gain.
    pub work_credit_per_claim: u64,
    /// A stated bound on gains outside the claim's own credit (fork-choice, external settlement): never hidden inside the reward.
    pub external_gain_bound: u64,
    /// The share (permille) of claims the network's operating assumptions expect to be caught; `reservation × this ≥ 1000 × gain`.
    /// `1000` claims certain detection — and must then be defended; a lower number demands a proportionally larger reservation.
    pub assumed_detection_permille: u16,
    /// Most PRE-FINAL OPV claims one producer (bond) may have at once (C4 F-C4R3-05: a Final claim's liability needs its reservation,
    /// not an admission slot). Its whole exposure stays bounded by its free collateral (every claim reserves `reservation_per_claim`).
    pub max_live_claims_per_producer: u32,
    /// Most PRE-FINAL OPV claims in the whole ledger for producers that already hold one (C4 F-C4R3-05).
    pub max_live_claims_total: u32,
    /// **Slots past `max_live_claims_total` that only a producer holding NO pre-Final OPV claim may take** (C4 F-C4R3-05, round 2): a
    /// set of bonds that fills the total does not shut a new producer out; to do that it must also fill these, one fresh bond, one
    /// reservation and one fee each. `max_live_claims_total + fresh_producer_slots` is the lane's HARD ceiling, bounded by the
    /// prosecution room of the window (`OpvPolicyV1::validate`), so no flood of claims outgrows what outsiders can prosecute.
    pub fresh_producer_slots: u32,
    /// Of an OPV claim's pre-Final default penalty, the LEAST share (permille) that is burned instead of paid to the demanders, so a
    /// producer cannot cycle its own penalty through a Sybil demander for nothing. In `1..=1000`. The ledger burns the larger of
    /// this and `1000 − accuser_reward_permille` (a default is split at least like a slash, C4 F-C4R3-02).
    pub default_burn_permille: u16,
    /// **What admitting an OPV claim costs, non-refundably** (C4 F-C4R3-05): taken from the producer's free collateral at admission and
    /// burned whatever becomes of the claim (Final, convicted, defaulted), so holding the lane's slots by refilling them is never free.
    pub admission_fee: u64,
}

/// **The network's OPV policy** — a consensus constant fixed at genesis (like [`LedgerPolicyV1`]); absent = this network has no
/// OPV rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OpvPolicyV1 {
    /// The first DAA at which OPV classes register and OPV claims commit. The consumer derives it from the `palw_panel_free_v1`
    /// fence; `None` is a fence that never activates.
    pub activation_daa: Option<u64>,
    pub window: OpvWindowV1,
    pub budgets: OpvBudgetsV1,
    pub economics: OpvEconomicsV1,
    pub carrier: CarrierCapsV1,
}

impl OpvPolicyV1 {
    /// The whole fixed window of a claim admitted at some DAA.
    pub fn window_daa(&self) -> u64 {
        self.window.base_challenge_window_daa.saturating_add(self.window.verification_horizon_daa)
    }

    /// Whether the fence is reached at `daa`.
    pub fn allowed_at(&self, daa: u64) -> bool {
        self.activation_daa.is_some_and(|at| daa >= at)
    }

    /// The claim's maximum gain: its Final reward, the consumer's work credit and the stated external bound.
    pub fn max_gain_per_claim(&self, ledger: &LedgerPolicyV1) -> u128 {
        ledger.claim_reward as u128 + self.economics.work_credit_per_claim as u128 + self.economics.external_gain_bound as u128
    }

    /// **The smallest reservation RFC-0015 §8.1 allows**: the maximum gain plus the default penalty, and the maximum gain divided
    /// by the detection probability the operating assumptions claim (rounded up) — **both after self-recoup** (GAP-R7): a colluding
    /// producer can always convict itself first with a proof of its own and recoup the accuser's share of the slash, so what the
    /// relation counts on losing is only `reservation × (1 − accuser_reward_permille / 1000)`; the reservation is divided by that.
    pub fn required_reservation(&self, ledger: &LedgerPolicyV1) -> u128 {
        let gain = self.max_gain_per_claim(ledger);
        let by_penalty = gain + ledger.default_penalty as u128;
        let p = self.economics.assumed_detection_permille.max(1) as u128;
        let by_detection = (gain * 1000).div_ceil(p);
        let kept = 1000u128.saturating_sub(ledger.accuser_reward_permille as u128).max(1);
        (by_penalty.max(by_detection) * 1000).div_ceil(kept)
    }

    /// The latest DAA by which any claim admitted at `admitted` is Final or decided, whatever is filed: the window's end, a
    /// demand's deadline, and the grace after it (RFC-0015 §6.1's `dispute_hard_deadline`).
    pub fn hard_deadline(&self, admitted: u64, ledger: &LedgerPolicyV1) -> u64 {
        admitted.saturating_add(self.window_daa()).saturating_add(ledger.court_deadline_daa).saturating_add(ledger.proof_grace_daa)
    }

    /// **The latest DAA a fresh verifier may *start* from** and still reach a first filing inside the window of a claim admitted at
    /// `admitted`: the window's end less its cold fetch, check, one carrier inclusion and the reorg slack.
    pub fn verifier_start_cutoff(&self, admitted: u64) -> u64 {
        let b = &self.budgets;
        let need = b.cold_material_daa.saturating_add(b.check_daa).saturating_add(b.carrier_daa).saturating_add(b.reorg_slack_daa);
        admitted.saturating_add(self.window_daa()).saturating_sub(need)
    }

    /// **What it costs to keep honest prosecutors out of the court by saturating it** (RFC-0015 §6.3): every block's adjudication
    /// budget is `min(max_adjudications_per_block, ⌈max_court_work_per_block / worst_court_work⌉)` court runs of this class, each
    /// junk filing forfeits `dismissed_proof_fee`, and a fraudulent producer that wants to KEEP its gain must keep the court
    /// saturated through the window and then through the whole liability horizon. A class is admitted only if this exceeds its
    /// maximum gain. (A lower bound: the real attack also has to win inclusion for every block.)
    pub fn censorship_cost(&self, ledger: &LedgerPolicyV1, worst_court_work: u64) -> u128 {
        let by_work = (ledger.max_court_work_per_block as u128).div_ceil(worst_court_work.max(1) as u128);
        let runs = (ledger.max_adjudications_per_block as u128).min(by_work).max(1);
        runs.saturating_mul(ledger.dismissed_proof_fee as u128)
            .saturating_mul(self.window_daa() as u128 + ledger.liability_daa as u128)
    }

    /// **The relations the clock and the money must satisfy** (never values chosen for a network).
    pub fn validate(&self, l: &LedgerPolicyV1) -> Result<(), String> {
        let (w, b, e, c) = (&self.window, &self.budgets, &self.economics, &self.carrier);
        let window = w.base_challenge_window_daa as u128 + w.verification_horizon_daa as u128;
        let u = |x: u64| x as u128;
        let first_step = u(b.cold_material_daa) + u(b.check_daa) + u(b.carrier_daa) + u(b.reorg_slack_daa);
        let respond = u(b.disclose_daa) + u(b.carrier_daa);
        let prove = u(b.localize_daa) + u(b.court_daa) + u(b.carrier_daa) + u(b.reorg_slack_daa);
        let total = u(b.cold_material_daa)
            + u(b.check_daa)
            + u(b.localize_daa)
            + u(b.disclose_daa)
            + u(b.court_daa)
            + u(b.carrier_daa)
            + u(b.reorg_slack_daa);
        let hard = window + u(l.court_deadline_daa) + u(l.proof_grace_daa);
        let gain = self.max_gain_per_claim(l);
        let checks: [(bool, &str); 22] = [
            (w.base_challenge_window_daa > 0, "the base challenge window is non-empty"),
            (
                first_step <= window,
                "a fresh verifier cannot finish its cold fetch and check, and have a first filing included, inside the window \
                 (no demand opens after the window)",
            ),
            (respond <= u(l.court_deadline_daa), "a producer's disclosure of a demanded position does not fit a demand's deadline"),
            (
                prove <= u(l.proof_grace_daa),
                "the proof a served demand enables cannot be localized, assembled and included inside the proof grace",
            ),
            (total <= hard, "RFC-0015 §6.1: the verifier's whole path does not fit the dispute hard deadline"),
            (
                u(l.liability_daa) > window + u(l.court_deadline_daa) + u(l.proof_grace_daa),
                "the post-Final liability horizon must outlast the window's whole hard-deadline path",
            ),
            (l.default_penalty > 0, "withheld material costs the producer something (a free default is a free job squat)"),
            (l.accuser_reward_permille < 1000, "a convicted fraud burns part of the slash (a 100% bounty makes self-conviction free)"),
            (e.default_burn_permille >= 1 && e.default_burn_permille <= 1000, "a default burns part of its penalty"),
            (e.admission_fee > 0, "admitting a claim is not free (C4 F-C4R3-05: a refilled lane must cost its occupiers)"),
            // C4 F-C4R3-05 (round 2): every claim the lane can hold before Final has a reserved court run within its window, so a flood
            // of claims never leaves an outsider without room to file the proof of one of them.
            (
                (e.max_live_claims_total as u128 + e.fresh_producer_slots as u128) <= l.prosecution_reserved_runs() as u128 * window,
                "the lane's hard ceiling exceeds the proof runs the window reserves (max_adjudications × prosecution_reserve ‰ × window)",
            ),
            (
                e.assumed_detection_permille >= 1 && e.assumed_detection_permille <= 1000,
                "the assumed detection probability is a probability",
            ),
            (
                e.reservation_per_claim as u128 >= self.required_reservation(l),
                "the reservation covers the maximum gain plus the default penalty, and the gain over the assumed detection probability",
            ),
            (e.reservation_per_claim as u128 > gain, "the reservation exceeds the claim's maximum gain"),
            (
                e.max_live_claims_per_producer > 0 && e.max_live_claims_total >= e.max_live_claims_per_producer,
                "claim caps are non-empty and the total covers one producer",
            ),
            (c.filing_cap > 0 && c.response_cap > 0 && c.commit_cap > 0, "the carrier capacities are declared"),
            (c.filing_cap <= crate::route::MAX_FILE_PROOF_BYTES_V1 as u64, "a filing carrier past the route's own ceiling"),
            (c.response_cap <= crate::route::MAX_RESPOND_BYTES_V1 as u64, "a response carrier past the route's own ceiling"),
            (c.commit_cap <= crate::route::MAX_COMMIT_CLAIM_BYTES_V1 as u64, "a commitment carrier past the route's own ceiling"),
            (b.check_daa > 0 && b.carrier_daa > 0, "checking and inclusion take time"),
            (l.court_deadline_daa > 0 && l.proof_grace_daa > 0, "a demand has a deadline and a served demand a grace"),
            (
                self.activation_daa.is_none_or(|at| at < u64::MAX),
                "an activation at u64::MAX is the fence's never(): express a dormant network as no activation",
            ),
        ];
        match checks.iter().find(|(ok, _)| !ok) {
            Some((_, why)) => Err(format!("OPV policy: {why}")),
            None => Ok(()),
        }
    }
}

// ── state ───────────────────────────────────────────────────────────────────────────────────────────────────────────────

/// An OPV claim's fixed facts, set when it was admitted (a later fence or policy change never reinterprets a claim mid-window).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OpvClaimRowV1 {
    pub admitted_daa: u64,
    /// The end of the challenge window (the earliest Final).
    pub window_end_daa: u64,
    /// The latest Final / decision whatever is filed ([`OpvPolicyV1::hard_deadline`] at admission).
    pub hard_deadline_daa: u64,
    /// What was reserved from the producer's free collateral.
    pub reservation: u64,
    /// The claim's maximum gain at admission.
    pub max_gain: u64,
    /// A post-Final default forfeited the remaining reservation (the material stopped being available).
    pub forfeited_after_final: bool,
}

/// **The OPV part of the ledger's state.** Dormant (`policy: None`): byte-identical to a ledger that never heard of OPV, root
/// included.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpvStateV1 {
    pub policy: Option<OpvPolicyV1>,
    /// The (mode-bound) ids the network's policy has admitted for `OptimisticPublicVerification` (consumer-derived,
    /// [`KernelLedgerV1::admit_optimistic_class`]): only these may register under the mode.
    pub admitted: BTreeSet<Digest>,
    /// The ids of the classes registered under [`VerificationModeV1::OptimisticPublicVerification`] (single and pipeline).
    pub classes: BTreeSet<Digest>,
    pub claims: BTreeMap<Digest, OpvClaimRowV1>,
    /// `(producer, claim)` of every OPV claim whose reservation is still held. **Derived** (not committed by the root; rebuilt by
    /// [`KernelLedgerV1::opv_rebuild_live`] after a restore); the caps read it.
    pub(crate) live: BTreeSet<(Digest, Digest)>,
}

impl OpvStateV1 {
    /// The derived live-claim index: `(producer, claim)` of every OPV claim whose reservation is held.
    pub fn live_claims(&self) -> &BTreeSet<(Digest, Digest)> {
        &self.live
    }

    /// No OPV policy: the ledger is the Panel-licensed route and its root is the historical root.
    pub fn is_dormant(&self) -> bool {
        self.policy.is_none()
    }
}

/// What the root commits once an OPV policy is set: the historical root of the whole Panel-licensed route, the policy and the two
/// OPV collections. Versioned apart from [`crate::state::StateRootPartsV1`], which is unchanged.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StateRootPartsV2 {
    pub version: u16,
    /// `StateRootPartsV1::root()` of the same ledger.
    pub v1: Digest,
    pub opv_policy: Digest,
    pub opv_admitted: Digest,
    pub opv_classes: Digest,
    pub opv_claims: Digest,
}

impl StateRootPartsV2 {
    pub fn root(&self) -> Digest {
        object_id(OPV_ROOT_DOMAIN_V2, self)
    }
}

// ── receipts and views ──────────────────────────────────────────────────────────────────────────────────────────────────

/// What a Final means, for display (RFC-0015 §11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum FinalAssuranceV1 {
    PanelLicensed = 0,
    OptimisticPublicVerification = 1,
}

impl FinalAssuranceV1 {
    pub const fn of(mode: VerificationModeV1) -> Self {
        match mode {
            VerificationModeV1::PanelLicensed => Self::PanelLicensed,
            VerificationModeV1::OptimisticPublicVerification => Self::OptimisticPublicVerification,
        }
    }

    /// The statement an RPC / explorer may show. An OPV Final is never shown as a proof of correctness.
    pub const fn statement(self) -> &'static str {
        match self {
            Self::PanelLicensed => {
                "Final: the fixed Panel covered the claim and its challenge window closed with no accepted dispute."
            }
            Self::OptimisticPublicVerification => {
                "Final under optimistic public verification: the fixed challenge window closed with no accepted dispute and the \
                 retention obligation is met. This is not a proof that the computation is correct, and it assumes an honest \
                 verifier was able to check it."
            }
        }
    }
}

/// How a Final claim stands now (a later event may change it; a receipt built from an earlier ledger state is the earlier fact).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum FinalStandingV1 {
    Standing = 0,
    /// A post-Final proof convicted it: the work is void.
    ConvictedAfterFinal = 1,
    /// A post-Final default forfeited the reservation: the material stopped being available. Tracked for OPV claims only (a
    /// Panel-licensed row carries no such flag; its `PostFinalDefault` receipt is the record).
    DaForfeitedAfterFinal = 2,
}

/// **The facts of one Final** a consumer needs to build RFC-0007/0010's [`WorkFinalEventV1`], derived from ledger state alone.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FinalReceiptV1 {
    pub claim: Digest,
    pub mode: VerificationModeV1,
    /// The class id (it binds the mode): the source profile of the work.
    pub source_profile_id: Digest,
    /// `H(class id, job id)`: stable under re-wrapping of the claim (a different producer, carrier or header names the same work).
    pub canonical_work_id: Digest,
    /// The claim's evidence root: the producer's commitment to the whole execution.
    pub execution_commitment: Digest,
    /// The DAA the claim was admitted at.
    pub accepted_daa: u64,
    /// The DAA the claim reached Final.
    pub final_daa: u64,
    /// The DA obligation was satisfied *at Final* (Final needs no open dispute, no default and the retention obligation).
    pub da_satisfied: bool,
    pub standing: FinalStandingV1,
    pub assurance: FinalAssuranceV1,
}

/// The consumer's own facts when it turns a [`FinalReceiptV1`] into the beacon's event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkFinalContextV1 {
    /// The chain position of the claim's acceptance (the consumer maps the DAA to its position scale).
    pub accepted_position: u64,
    /// The chain position of the Final.
    pub settlement_position: u64,
    pub occurrence_index: u32,
    pub validity_independent: bool,
    pub depends_on_profiles: Vec<Digest>,
    /// A Panel-licensed Final: the licensing Panel's `(seed id, epoch)`, known to the consumer that derived the tally. Must be
    /// `Some` for a Panel-licensed receipt and `None` for an OPV one.
    pub panel: Option<(Digest, u64)>,
}

impl FinalReceiptV1 {
    /// How the work reached Final — decided by the mode, never by a caller.
    pub fn final_path(&self, panel: Option<(Digest, u64)>) -> Result<FinalPathV1, String> {
        match (self.mode, panel) {
            (VerificationModeV1::OptimisticPublicVerification, None) => Ok(FinalPathV1::PanelIndependent),
            (VerificationModeV1::OptimisticPublicVerification, Some(_)) => {
                Err("an OptimisticPublicVerification Final has no Panel in its path: a Panel licence is never attached to it".into())
            }
            (VerificationModeV1::PanelLicensed, Some((panel_seed_id, panel_epoch))) => {
                Ok(FinalPathV1::PanelLicensed { panel_seed_id, panel_epoch })
            }
            (VerificationModeV1::PanelLicensed, None) => {
                Err("a Panel-licensed Final names its Panel (seed and epoch): it is never reported as Panel-independent".into())
            }
        }
    }

    /// **The beacon's event for this Final.** A convicted work is not Final any more; a work whose material stopped being
    /// available is not DA-satisfied any more — both then fail the beacon's eligibility by their own reason.
    pub fn to_work_final_event(&self, ctx: &WorkFinalContextV1) -> Result<WorkFinalEventV1, String> {
        let final_path = self.final_path(ctx.panel)?;
        Ok(WorkFinalEventV1 {
            kind: WorkSourceKindV1::RealUsefulWork,
            source_profile_id: self.source_profile_id,
            canonical_work_id: self.canonical_work_id,
            execution_commitment: self.execution_commitment,
            accepted_position: ctx.accepted_position,
            settlement_position: ctx.settlement_position,
            occurrence_index: ctx.occurrence_index,
            claim_final: self.standing != FinalStandingV1::ConvictedAfterFinal,
            da_satisfied: self.da_satisfied && self.standing != FinalStandingV1::DaForfeitedAfterFinal,
            validity_independent: ctx.validity_independent,
            depends_on_profiles: ctx.depends_on_profiles.clone(),
            final_path,
        })
    }
}

/// **What public discovery returns for an OPV claim** (RFC-0015 §5): the clock facts a fresh verifier plans by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpvClaimViewV1 {
    pub claim: Digest,
    pub producer: Digest,
    pub class_binding_id: Digest,
    pub job_id: Digest,
    pub state: ClaimStateV1,
    /// When the claim became Challengeable.
    pub admitted_daa: u64,
    /// The latest DAA a fresh verifier may start from and still reach a first filing inside the window.
    pub verifier_start_cutoff_daa: u64,
    /// The earliest Final (the window's end).
    pub final_floor_daa: u64,
    /// The latest Final / decision, whatever is filed.
    pub hard_deadline_daa: u64,
    pub reservation: u64,
    pub max_gain: u64,
    /// Demand sessions open on the claim and positions already served (reusable by every verifier).
    pub open_demands: u32,
    pub served_positions: u32,
    pub assurance: FinalAssuranceV1,
}

// ── the ledger's OPV surface ────────────────────────────────────────────────────────────────────────────────────────────

impl KernelLedgerV1 {
    /// **Give a freshly built ledger its OPV policy** (genesis only: the policy is a consensus constant). Validated against the
    /// ledger's own policy.
    pub fn with_opv_policy(mut self, policy: OpvPolicyV1) -> Result<Self, String> {
        if self.daa != 0 || !self.opv.classes.is_empty() || !self.opv.claims.is_empty() || !self.claims.is_empty() {
            return Err("the OPV policy is a genesis constant".into());
        }
        policy.validate(&self.policy)?;
        self.opv.policy = Some(policy);
        Ok(self)
    }

    pub fn opv_policy(&self) -> Option<&OpvPolicyV1> {
        self.opv.policy.as_ref()
    }

    /// **The network's policy admits a class for `OptimisticPublicVerification`** (RFC-0015 §4.1; consumer-derived from
    /// authenticated chain state, like `attest_artifact`). `class` is the mode-bound id the registration will have
    /// ([`crate::ledger::single_class_id_v1`], or `class_id_for_mode_v1` of a pipeline binding). A ledger with no OPV policy refuses
    /// the admission (a hidden, un-rooted set would let nodes diverge silently). Idempotent.
    pub fn admit_optimistic_class(&mut self, class: Digest) -> Result<(), KernelRefusalV1> {
        if self.opv.policy.is_none() {
            return Err(KernelRefusalV1::rule("AdmitOptimisticClass", "this ledger has no OPV policy"));
        }
        self.opv.admitted.insert(class);
        Ok(())
    }

    /// Whether OPV classes may register and OPV claims commit at the ledger's current DAA.
    pub fn optimistic_allowed(&self) -> bool {
        self.opv.policy.is_some_and(|p| p.allowed_at(self.daa))
    }

    /// The mode a class (single or pipeline) was registered under.
    pub fn mode_of_class(&self, class: &Digest) -> VerificationModeV1 {
        if self.opv.classes.contains(class) {
            VerificationModeV1::OptimisticPublicVerification
        } else {
            VerificationModeV1::PanelLicensed
        }
    }

    /// The mode of a committed claim (`None`: no such claim).
    pub fn mode_of_claim(&self, claim: &Digest) -> Option<VerificationModeV1> {
        self.claims.get(claim).map(|r| self.mode_of_class(&r.class_binding_id))
    }

    /// **Rebuild the derived live-claim index** (after restoring a ledger from rows): every OPV claim whose reservation is held.
    pub fn opv_rebuild_live(&mut self) {
        self.opv.live = self
            .opv
            .claims
            .keys()
            .filter_map(|id| self.claims.get(id).filter(|r| r.reserved > 0).map(|r| (r.producer, *id)))
            .collect();
    }

    /// Drop a claim from the live index once its reservation is gone (called wherever a reservation is released or slashed).
    pub(crate) fn opv_sync_live(&mut self, claim: &Digest) {
        if !self.opv.claims.contains_key(claim) {
            return;
        }
        if let Some(r) = self.claims.get(claim)
            && r.reserved == 0
        {
            self.opv.live.remove(&(r.producer, *claim));
        }
    }

    /// **PRE-FINAL OPV claims** — those not yet Final, convicted, defaulted or timed out: for `producer` and in total. The admission
    /// caps count these (C4 F-C4R3-05); a claim in its liability phase keeps its reservation but holds no admission slot.
    pub fn opv_open_counts(&self, producer: &Digest) -> (u32, u32) {
        let open = |id: &Digest| self.claims.get(id).is_some_and(|r| !r.life.state.is_terminal());
        let mine = self.opv.live.range((*producer, [0u8; 64])..=(*producer, [0xFFu8; 64])).filter(|(_, id)| open(id)).count();
        let all = self.opv.live.iter().filter(|(_, id)| open(id)).count();
        (mine as u32, all as u32)
    }

    /// OPV claims with a held reservation: for `producer` and in total.
    pub fn opv_live_counts(&self, producer: &Digest) -> (u32, u32) {
        let mine = self.opv.live.range((*producer, [0u8; 64])..=(*producer, [0xFFu8; 64])).count() as u32;
        (mine, self.opv.live.len() as u32)
    }

    /// **The aggregate maximum gain `producer` holds in unsettled OPV claims** (RFC-0015 §4.2's aggregate-gain check). By the
    /// reservation rule it never exceeds the producer's reserved collateral.
    pub fn opv_unsettled_gain(&self, producer: &Digest) -> u128 {
        self.opv
            .live
            .range((*producer, [0u8; 64])..=(*producer, [0xFFu8; 64]))
            .filter_map(|(_, id)| self.opv.claims.get(id))
            .map(|r| r.max_gain as u128)
            .sum()
    }

    /// The caps and the reservation for a new OPV claim of `producer`, and the claim's fixed facts (checked before any mutation).
    pub(crate) fn opv_admission(&self, producer: &Digest) -> Result<(u64, OpvClaimRowV1), String> {
        let p = self.opv.policy.ok_or("this ledger has no OPV policy")?;
        if !p.allowed_at(self.daa) {
            return Err("the palw_panel_free_v1 fence is not reached".into());
        }
        // C4 F-C4R3-05: only PRE-FINAL claims hold admission slots (a Final claim's reservation stays held through its liability
        // horizon, so refilling the lane needs fresh collateral every window); a producer is capped; past the total only a producer with
        // no pre-Final claim is admitted, up to the HARD ceiling the window's reserved proof runs bound; and every admission pays a
        // non-refundable fee (`admit`). Holding the whole lane therefore costs one fresh bond, one reservation and one fee per slot.
        let (mine, all) = self.opv_open_counts(producer);
        let e = &p.economics;
        if mine >= e.max_live_claims_per_producer {
            return Err(format!("the producer already has {mine} pre-Final claims (cap {})", e.max_live_claims_per_producer));
        }
        let hard = e.max_live_claims_total.saturating_add(e.fresh_producer_slots);
        if all >= hard {
            return Err(format!("{all} pre-Final claims: the lane's hard ceiling (the window's reserved proof runs) is reached"));
        }
        if mine > 0 && all >= e.max_live_claims_total {
            return Err(format!(
                "{all} pre-Final claims in the ledger (cap {}; past it only a producer holding none is admitted)",
                e.max_live_claims_total
            ));
        }
        let gain = p.max_gain_per_claim(&self.policy).min(u64::MAX as u128) as u64;
        let row = OpvClaimRowV1 {
            admitted_daa: self.daa,
            window_end_daa: self.daa.saturating_add(p.window_daa()),
            hard_deadline_daa: p.hard_deadline(self.daa, &self.policy),
            reservation: p.economics.reservation_per_claim,
            max_gain: gain,
            forfeited_after_final: false,
        };
        Ok((p.economics.reservation_per_claim, row))
    }

    // ── views ───────────────────────────────────────────────────────────────────────────────────────────────────────────

    /// **Public discovery of an OPV claim's clock** (`None`: unknown, or not an OPV claim).
    pub fn opv_claim_view(&self, claim: &Digest) -> Option<OpvClaimViewV1> {
        let (row, o) = (self.claims.get(claim)?, self.opv.claims.get(claim)?);
        let p = self.opv.policy.as_ref()?;
        let (lo, hi) = ((*claim, 0u8, 0u32), (*claim, u8::MAX, u32::MAX));
        Some(OpvClaimViewV1 {
            claim: *claim,
            producer: row.producer,
            class_binding_id: row.class_binding_id,
            job_id: row.job_id,
            state: row.life.state.clone(),
            admitted_daa: o.admitted_daa,
            verifier_start_cutoff_daa: p.verifier_start_cutoff(o.admitted_daa),
            final_floor_daa: o.window_end_daa,
            hard_deadline_daa: o.hard_deadline_daa,
            reservation: o.reservation,
            max_gain: o.max_gain,
            open_demands: self.demands.range(lo..=hi).count() as u32,
            served_positions: self.served.range(lo..=hi).count() as u32,
            assurance: FinalAssuranceV1::OptimisticPublicVerification,
        })
    }

    /// **The Final receipt of one claim** (`None`: unknown, or not Final).
    pub fn final_receipt(&self, claim: &Digest) -> Option<FinalReceiptV1> {
        let row = self.claims.get(claim)?;
        let ClaimStateV1::Final { final_daa } = row.life.state else { return None };
        let mode = self.mode_of_class(&row.class_binding_id);
        let forfeited = self.opv.claims.get(claim).is_some_and(|o| o.forfeited_after_final);
        let standing = if row.convicted {
            FinalStandingV1::ConvictedAfterFinal
        } else if forfeited {
            FinalStandingV1::DaForfeitedAfterFinal
        } else {
            FinalStandingV1::Standing
        };
        Some(FinalReceiptV1 {
            claim: *claim,
            mode,
            source_profile_id: row.class_binding_id,
            canonical_work_id: object_id(CANONICAL_WORK_DOMAIN_V1, &(row.class_binding_id, row.job_id)),
            execution_commitment: object_id(EXECUTION_COMMITMENT_DOMAIN_V1, &execution_root(row)),
            accepted_daa: row.committed_daa,
            final_daa,
            da_satisfied: true,
            standing,
            assurance: FinalAssuranceV1::of(mode),
        })
    }

    /// **Every Final the ledger holds**, in canonical order `(final DAA, canonical work id, claim)` — the order is a function of
    /// ledger state, never of arrival.
    pub fn final_receipts(&self) -> Vec<FinalReceiptV1> {
        let mut v: Vec<FinalReceiptV1> = self.claims.keys().filter_map(|id| self.final_receipt(id)).collect();
        v.sort_by_key(|r| (r.final_daa, r.canonical_work_id, r.claim));
        v
    }

    // ── state root ──────────────────────────────────────────────────────────────────────────────────────────────────────

    /// The OPV root form (what [`Self::root`] commits to once an OPV policy is set).
    pub fn root_parts_v2(&self) -> StateRootPartsV2 {
        let d = |name: &str| format!("misaka-palw/kernel/ledger-collection/{name}/v1").into_bytes();
        StateRootPartsV2 {
            version: OPV_STATE_VERSION_V2,
            v1: self.root_parts().root(),
            opv_policy: object_id(OPV_POLICY_DOMAIN_V1, &self.opv.policy),
            opv_admitted: collection_root(&d("opv-admitted"), self.opv.admitted.len(), self.opv.admitted.iter().map(|k| (k, &()))),
            opv_classes: collection_root(&d("opv-classes"), self.opv.classes.len(), self.opv.classes.iter().map(|k| (k, &()))),
            opv_claims: collection_root(&d("opv-claims"), self.opv.claims.len(), self.opv.claims.iter()),
        }
    }

    // ── invariants (tests and audits) ───────────────────────────────────────────────────────────────────────────────────

    /// **The OPV invariants**, recomputed from the rows:
    /// * every OPV claim row belongs to a committed claim of an OPV class, and conversely;
    /// * the live index is exactly the OPV claims with a held reservation;
    /// * per producer, the aggregate maximum gain of live claims is at most their reservations, which are at most the bond's
    ///   reserved collateral (a collateral is never counted twice);
    /// * no OPV claim ever visited a Panel state.
    pub fn opv_invariants(&self) -> Result<(), String> {
        for id in self.opv.claims.keys() {
            let row = self.claims.get(id).ok_or("an OPV row without a claim")?;
            if !self.opv.classes.contains(&row.class_binding_id) {
                return Err("an OPV row whose class is not an OPV class".into());
            }
            if matches!(
                row.life.state,
                ClaimStateV1::ChallengeBound { .. } | ClaimStateV1::Checking { .. } | ClaimStateV1::ProbabilisticPass { .. }
            ) {
                return Err("an OPV claim is in a Panel state".into());
            }
            if self.job_claims.get(&row.job_id).is_none_or(|h| h != id) && row.holds_job() {
                return Err("an OPV claim that holds its job is not the job's holder".into());
            }
        }
        for (id, row) in &self.claims {
            if self.opv.classes.contains(&row.class_binding_id) && !self.opv.claims.contains_key(id) {
                return Err("a claim of an OPV class without an OPV row".into());
            }
        }
        let mut live: BTreeSet<(Digest, Digest)> = BTreeSet::new();
        for id in self.opv.claims.keys() {
            if let Some(r) = self.claims.get(id).filter(|r| r.reserved > 0) {
                live.insert((r.producer, *id));
            }
        }
        if live != self.opv.live {
            return Err("the live-claim index differs from the rows".into());
        }
        let producers: BTreeSet<Digest> = live.iter().map(|(p, _)| *p).collect();
        for p in producers {
            let reserved: u128 = live
                .range((p, [0u8; 64])..=(p, [0xFFu8; 64]))
                .filter_map(|(_, id)| self.claims.get(id))
                .map(|r| r.reserved as u128)
                .sum();
            let gain = self.opv_unsettled_gain(&p);
            if gain > reserved {
                return Err("a producer's unsettled gain exceeds its reservations".into());
            }
            if self.bonds.get(&p).is_none_or(|b| (b.reserved as u128) < reserved) {
                return Err("a producer's reservations exceed its bond's reserved collateral (a collateral counted twice)".into());
            }
        }
        Ok(())
    }
}

/// The evidence root a claim commits (single program or pipeline).
fn execution_root(row: &ClaimRowV1) -> Digest {
    match &row.body {
        ClaimBodyV1::Program { evidence, .. } => evidence.root(),
        ClaimBodyV1::Pipeline { evidence, .. } => evidence.root(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::ProsecutionPolicyV1;

    pub(crate) fn ledger_policy() -> LedgerPolicyV1 {
        LedgerPolicyV1 {
            network_domain: [9; 64],
            ruleset_digest: [3; 64],
            challenge_policy_id: [5; 64],
            claim_collateral: 1000,
            demand_bond: 10,
            check_window_daa: 100,
            challenge_window_daa: 50,
            court_deadline_daa: 20,
            proof_grace_daa: 10,
            liability_daa: 200,
            exit_delay_daa: 30,
            dismissed_proof_fee: 5,
            accuser_reward_permille: 500,
            default_penalty: 100,
            claim_reward: 7,
            job_fee: 2,
            job_escrow_ttl_daa: 300,
            max_adjudications_per_block: 64,
            prosecution_reserve_permille: 500,
            max_court_work_per_block: u64::MAX,
            claim_seal_delay_daa: 1,
            seal_ttl_daa: 100,
            seal_deposit: 1,
            prosecution: ProsecutionPolicyV1 {
                court_deadline_daa: 20,
                max_sessions_per_claim: 1 << 10,
                max_public_bytes: 1 << 40,
                max_verifier_ram: 1 << 36,
                max_retained_state: 1 << 32,
            },
        }
    }

    /// An example policy (values for tests, chosen for no network).
    pub(crate) fn example() -> OpvPolicyV1 {
        OpvPolicyV1 {
            activation_daa: Some(0),
            window: OpvWindowV1 { base_challenge_window_daa: 40, verification_horizon_daa: 10 },
            budgets: OpvBudgetsV1 {
                cold_material_daa: 10,
                check_daa: 10,
                localize_daa: 2,
                disclose_daa: 8,
                court_daa: 3,
                carrier_daa: 2,
                reorg_slack_daa: 2,
            },
            economics: OpvEconomicsV1 {
                reservation_per_claim: 1000,
                work_credit_per_claim: 13,
                external_gain_bound: 80,
                assumed_detection_permille: 500,
                max_live_claims_per_producer: 3,
                max_live_claims_total: 5,
                fresh_producer_slots: 2,
                default_burn_permille: 100,
                admission_fee: 3,
            },
            carrier: CarrierCapsV1 { filing_cap: 1 << 26, response_cap: 1 << 27, commit_cap: 1 << 27 },
        }
    }

    #[test]
    fn the_example_policy_satisfies_every_relation_and_each_relation_is_load_bearing() {
        let l = ledger_policy();
        let p = example();
        p.validate(&l).unwrap();
        assert_eq!(p.window_daa(), 50);
        assert_eq!(p.max_gain_per_claim(&l), 7 + 13 + 80);
        assert_eq!(
            p.required_reservation(&l),
            400,
            "gain 100 over a 50% assumed detection is 200, gain + penalty is also 200; after a self-recouped 50% bounty, 400"
        );

        let mut bad = Vec::new();
        let mut push = |name: &'static str, f: &dyn Fn(&mut OpvPolicyV1, &mut LedgerPolicyV1)| {
            let (mut p, mut l) = (example(), ledger_policy());
            f(&mut p, &mut l);
            bad.push((name, p.validate(&l)));
        };
        push("empty window", &|p, _| p.window.base_challenge_window_daa = 0);
        push("window too short for the verifier's first step", &|p, _| {
            p.window.base_challenge_window_daa = 5;
            p.window.verification_horizon_daa = 5;
        });
        push("disclosure past a demand's deadline", &|p, _| p.budgets.disclose_daa = 19);
        push("proof past the grace", &|p, _| p.budgets.court_daa = 9);
        push("liability does not outlast the path", &|_, l| l.liability_daa = 80);
        push("free default", &|_, l| l.default_penalty = 0);
        push("hundred percent bounty", &|_, l| l.accuser_reward_permille = 1000);
        push("no default burn", &|p, _| p.economics.default_burn_permille = 0);
        push("free admission", &|p, _| p.economics.admission_fee = 0);
        push("a hard ceiling past the window's proof room", &|p, _| p.economics.fresh_producer_slots = 32 * 50);
        push("no detection assumption", &|p, _| p.economics.assumed_detection_permille = 0);
        push("reservation below gain + penalty", &|p, _| p.economics.reservation_per_claim = 199);
        push("reservation below what self-recoup leaves (GAP-R7)", &|p, _| p.economics.reservation_per_claim = 399);
        push("detection assumption too weak for the reservation", &|p, _| p.economics.assumed_detection_permille = 90);
        push("no per-producer cap", &|p, _| p.economics.max_live_claims_per_producer = 0);
        push("total below per-producer", &|p, _| p.economics.max_live_claims_total = 2);
        push("no carrier", &|p, _| p.carrier.filing_cap = 0);
        push("carrier past the route's ceiling", &|p, _| p.carrier.response_cap = u64::MAX);
        for (name, r) in bad {
            assert!(r.is_err(), "{name} must be refused");
        }
    }

    #[test]
    fn saturating_the_court_for_the_whole_exposure_must_cost_more_than_the_claims_maximum_gain() {
        let mut l = ledger_policy();
        let p = example();
        // 64 runs per block, 5 per junk filing, 50 + 200 DAA of exposure.
        assert_eq!(p.censorship_cost(&l, 1 << 20), 64 * 5 * 250);
        assert!(p.censorship_cost(&l, 1 << 20) > p.max_gain_per_claim(&l));
        // A court budget one run wide and a one-unit fee: saturating it for the exposure costs 250, still above a gain of 100 ...
        l.max_adjudications_per_block = 1;
        l.dismissed_proof_fee = 1;
        assert_eq!(p.censorship_cost(&l, 1 << 20), 250);
        // ... but a block whose court work is only one court wide, with a gain above that, is censorable for less than it pays.
        let mut big = example();
        big.economics.external_gain_bound = 400;
        big.economics.reservation_per_claim = 1000;
        assert!(big.censorship_cost(&l, 1 << 20) < big.max_gain_per_claim(&l));
        // The court-work cap binds before the run cap: a block that fits two worst courts is saturated by two junk filings.
        l.max_adjudications_per_block = 64;
        l.max_court_work_per_block = 2 << 20;
        assert_eq!(p.censorship_cost(&l, 1 << 20), 2 * 250);
    }

    #[test]
    fn the_reservation_rule_reads_gain_penalty_and_the_assumed_detection() {
        let l = ledger_policy();
        let mut p = example();
        p.economics.assumed_detection_permille = 1000;
        assert_eq!(p.required_reservation(&l), 400, "certain detection: gain + penalty, after the self-recouped half (GAP-R7)");
        p.economics.assumed_detection_permille = 250;
        assert_eq!(p.required_reservation(&l), 800, "a 25% detection assumption quadruples the gain");
        let mut low = l;
        low.accuser_reward_permille = 0;
        assert_eq!(p.required_reservation(&low), 400, "no bounty, nothing to recoup");
        p.economics.external_gain_bound = 1080;
        assert_eq!(p.max_gain_per_claim(&l), 1100, "a stated external gain is never hidden in the reward");
        assert!(p.validate(&l).is_err(), "and the reservation must follow it");
    }

    #[test]
    fn the_clock_facts_a_fresh_verifier_plans_by() {
        let l = ledger_policy();
        let p = example();
        // admitted at 100: window ends 150, hard deadline 150 + 20 + 10, a verifier must start by 150 - (10 + 10 + 2 + 2).
        assert_eq!(p.hard_deadline(100, &l), 180);
        assert_eq!(p.verifier_start_cutoff(100), 126);
        assert!(p.allowed_at(0), "the example activates at genesis");
        let mut q = p;
        q.activation_daa = Some(500);
        assert!(!q.allowed_at(499) && q.allowed_at(500));
        q.activation_daa = None;
        assert!(!q.allowed_at(u64::MAX), "a fence that never activates never allows");
    }

    #[test]
    fn the_receipt_path_follows_the_mode_and_a_licence_is_never_fabricated_or_hidden() {
        let r = |mode| FinalReceiptV1 {
            claim: [1; 64],
            mode,
            source_profile_id: [2; 64],
            canonical_work_id: [3; 64],
            execution_commitment: [4; 64],
            accepted_daa: 10,
            final_daa: 70,
            da_satisfied: true,
            standing: FinalStandingV1::Standing,
            assurance: FinalAssuranceV1::of(mode),
        };
        let ctx = |panel| WorkFinalContextV1 {
            accepted_position: 10,
            settlement_position: 70,
            occurrence_index: 0,
            validity_independent: true,
            depends_on_profiles: vec![],
            panel,
        };
        let opv = r(VerificationModeV1::OptimisticPublicVerification);
        let ev = opv.to_work_final_event(&ctx(None)).unwrap();
        assert_eq!(ev.final_path, FinalPathV1::PanelIndependent);
        assert!(ev.claim_final && ev.da_satisfied && ev.kind == WorkSourceKindV1::RealUsefulWork);
        assert!(opv.to_work_final_event(&ctx(Some(([9; 64], 3)))).is_err(), "no Panel licence is attached to an OPV Final");
        let licensed = r(VerificationModeV1::PanelLicensed);
        assert_eq!(
            licensed.to_work_final_event(&ctx(Some(([9; 64], 3)))).unwrap().final_path,
            FinalPathV1::PanelLicensed { panel_seed_id: [9; 64], panel_epoch: 3 }
        );
        assert!(licensed.to_work_final_event(&ctx(None)).is_err(), "a Panel-licensed Final is never reported Panel-independent");
        // A convicted or forfeited work fails the beacon's eligibility by its own reason.
        let mut convicted = opv.clone();
        convicted.standing = FinalStandingV1::ConvictedAfterFinal;
        assert!(!convicted.to_work_final_event(&ctx(None)).unwrap().claim_final);
        let mut forfeited = opv;
        forfeited.standing = FinalStandingV1::DaForfeitedAfterFinal;
        assert!(!forfeited.to_work_final_event(&ctx(None)).unwrap().da_satisfied);
    }

    #[test]
    fn the_assurance_statement_never_claims_correctness() {
        let s = FinalAssuranceV1::OptimisticPublicVerification.statement();
        assert!(s.contains("not a proof"));
        assert!(!FinalAssuranceV1::PanelLicensed.statement().is_empty());
    }
}
