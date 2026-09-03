//! A **buildable, validated** free-prompt devnet bundle (ADR-0044 FP-09, stage 1).
//!
//! ADR-0042's own PR-10 note left the conforming bundle as a `pub(crate)` test fixture, so no
//! crate outside `kaspa-consensus-core` could construct a `ConsensusV2` params set at all — which
//! meant the first real assembly of one would happen under deadline, in a preset, by hand. This
//! module is that assembly done early and in the open: one public constructor, every value
//! commented with WHY it holds, and a test that runs the whole Decision-1 + ADR-0044 startup
//! gate over it.
//!
//! **These are devnet-shaped numbers, and the module name says so.** They are chosen for a
//! machine-paced devnet where a fraud drill must finish inside a coffee break, NOT measured on a
//! fleet: mainnet/RC parameters are soak outputs (ADR-0036), and the fleet drill (FP-09 stage 2)
//! is what replaces them. What this bundle proves today is narrower and still worth having: that
//! the constraints *interlock* — a set of windows, shares, splits and beacon gaps that satisfies
//! every invariant simultaneously EXISTS, and here is one. When the measured numbers arrive they
//! are edits to these constants, not a new discovery that the gate is unsatisfiable.
//!
//! Nothing here is installed on any preset. A network gets this bundle only by an explicit
//! `palw_consensus_mode = ConsensusV2(...)`, and no shipped preset does that.

use crate::Hash64;
use crate::palw_admission_v2::PalwAdmissionParamsV2;
use crate::palw_freeprompt_v3::PalwFreePromptParamsV3;
use crate::palw_mode_v2::{
    PALW_V2_FORK_CHOICE_VERSION, PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS, PALW_V2_TRACE_FORMAT_VERSION, PalwBondParamsV2,
    PalwConsensusParamsV2, PalwCourtParamsV2, PalwModeV2Error, palw_v2_signature_contexts_root,
};
use crate::palw_panel_v2::PalwPanelParamsV2;
use crate::palw_reward_v2::PalwRewardParamsV2;
use crate::palw_state_v2::PalwStateParamsV2;

/// Immature live-weight fraction. 100‰: a fresh tip counts, but a private fork stacking
/// unresolved claims gains a tenth of what resolving them would.
const BETA_PERMILLE: u16 = 100;

/// Lattice windows, in DAA score: bind 600, receipt 600, challenge 1200, court 3000.
///
/// **What these mean in wall-clock changed when the bundle got its cadence field.** They were
/// sized against a 1-DAA-per-block devnet as "a full commit→bind→license→challenge→court cycle
/// in minutes". A `ConsensusV2` network runs the frozen 120 s cadence (ADR-0038 Decision H,
/// enforced by `validate_palw_v2`), so at one DAA per block the same numbers are hours: bind is
/// 20 h and the full cycle about six days. That is the point of putting the cadence inside the
/// ruleset id — the windows never had wall-clock meaning on their own, and this constant block
/// is where the two are read together. A shorter devnet cycle is a decision about these
/// numbers, made in daylight; it is not something the cadence should be bent for.
// Read only by the `#[cfg(test)] mod tests` below (and by the prose above); `#[cfg(test)]`
// rather than `#[allow(dead_code)]` so that a non-test USE of one of these is a compile error
// and not a silently-revived constant.
#[cfg(test)]
const WINDOW_BIND: u64 = PALW_RC_WINDOWS_V1.window_bind;
#[cfg(test)]
const WINDOW_RECEIPT: u64 = PALW_RC_WINDOWS_V1.window_receipt;
#[cfg(test)]
const WINDOW_CHALLENGE: u64 = PALW_RC_WINDOWS_V1.window_challenge;

/// **The lattice's windows, as ONE named set** (ADR-0077 Decision 7).
///
/// Every DAA-denominated window of the claim lattice, the court's move clock, the retention spans
/// that are derived from them — and, since ADR-0080, the one court ceiling those derivations cannot
/// be performed without ([`Self::court_max_close_bytes`]). Two sets ship: [`PALW_RC_WINDOWS_V1`]
/// is what testnet-11 runs (the constants this module has always carried, byte for byte), and
/// [`PALW_DEVNET_WINDOWS_V1`] is the same lattice in MINUTES, for the devnet preset only — the
/// in-harness finding stands (a single-chain `TestConsensus` does not accrue the DAA the windows
/// need) and a multi-node devnet chain does, so a drill that has to reach `Final` and spend a
/// quantum inside one run needs windows a chain can cross in a session. Every interlock
/// `PalwConsensusParamsV2::validate` states holds on both sets; the set is inside the ruleset id,
/// so the two presets are two networks, which they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwLatticeWindowsV1 {
    pub window_bind: u64,
    pub window_receipt: u64,
    pub window_challenge: u64,
    pub window_court: u64,
    pub anchor_delay: u64,
    pub max_beacon_gap: u64,
    pub reorg_margin: u64,
    pub receipt_maturity: u64,
    pub receipt_use_window: u64,
    pub court_turn_deadline: u64,
    pub fp_abandon_hold: u64,
    pub claim_retirement: u64,
    pub withdrawal_delay: u64,
    /// **Not a window — the one court ceiling a window's own derivations cannot be computed
    /// without** (ADR-0080 W4).
    ///
    /// A court close may span several carriers, and the blocks a mover spends ASSEMBLING one are
    /// blocks of `window_court` that carry no move. So the reserve
    /// [`crate::palw_context_ladder::palw_close_assembly_daa_v1`] subtracts, and therefore the move
    /// clock `window_court` can afford, are functions of this number as much as of the window. It
    /// lives here because this struct is where a network's two answers diverge: everything else in
    /// the bundle is one source for both sets, and a set that could not say what its own closes
    /// cost would have to be handed a default — which is precisely the shortcut that charged the
    /// minutes lattice for the hours lattice's closes.
    ///
    /// It is [`PalwCourtParamsV2::max_close_bytes`] verbatim: `with_cost_ceilings` derives the
    /// chunk count from it, so this field and the bundle's court cannot disagree.
    pub court_max_close_bytes: u64,
}

impl PalwLatticeWindowsV1 {
    /// The longest a single claim can hold its executor's exposure under these windows: every
    /// window on the path from creation to a terminal phase, laid end to end, with the redraw's
    /// second bind+receipt pair (see `MAX_CLAIM_EXPOSURE_DAA`).
    pub const fn max_claim_exposure_daa(&self) -> u64 {
        2 * (self.window_bind + self.window_receipt) + self.window_challenge + self.window_court + self.fp_abandon_hold
    }

    /// **The carriers one close may span under this set's court** — derived from
    /// [`Self::court_max_close_bytes`] by the same inverse the admission gate and
    /// `with_cost_ceilings` use, so the number a clock is derived from and the number a class is
    /// admitted against are one number.
    pub const fn max_close_chunks(&self) -> u64 {
        crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(self.court_max_close_bytes)
    }

    /// **The close ceiling this set carries, taken from the DERIVATION over the rows a genesis
    /// registers** (ADR-0082 Decision 6; audit D M-5).
    ///
    /// Decision 6 makes the count "a derivation, evaluated at genesis over the rows the genesis set
    /// registers". [`crate::palw_context_ladder::palw_close_chunks_for_ladder_v1`] is that
    /// derivation and, until this, it had three test callers and no production one: the ceiling a
    /// lattice carried was a typed constant, and a genesis whose registered set is the graph-v5
    /// pair would have carried the graph-v2/v3 pair's 27 — a close bound about nine times looser
    /// than the transport and the economics the ADR sizes, with
    /// `palw_close_assembly_daa_v1(27) = 216` DAA of reserve charged to every window for carriers
    /// nothing files.
    ///
    /// It is a BUILDER and not a rewrite of the two shipped constants, deliberately: the ceiling
    /// is inside `palw_ruleset_id_v2`, so changing what a shipped set carries moves every
    /// fingerprint on that network and is a genesis decision, not a fix. An assembler that wants
    /// the derivation says so; the two constants stay what the 5f genesis card froze.
    /// `the_genesis_close_ceiling_is_the_derivation_over_the_set_it_registers` reports both
    /// numbers.
    #[must_use]
    pub fn with_derived_court_close_v1(
        mut self,
        families: &[crate::palw_context_ladder::PalwLadderFamilyV1],
        rows: &[u32],
        court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
        ladder: u64,
    ) -> Option<Self> {
        self.court_max_close_bytes = palw_derived_court_max_close_bytes_v1(families, rows, court, ladder)?;
        Some(self)
    }
}

/// **ADR-0082 Decision 6's derivation, in bytes** — the unit `PalwCourtParamsV2` carries.
///
/// `palw_close_bytes_for_chunks_v1` of `palw_close_chunks_for_ladder_v1` over the `(families,
/// rows)` a genesis registers, at that ruleset's own `court` and `ladder`. `None` means no pair
/// priced at all, which is a genesis set that registers no prosecutable row rather than a cheap
/// one — a caller must not read that as "take the default".
pub fn palw_derived_court_max_close_bytes_v1(
    families: &[crate::palw_context_ladder::PalwLadderFamilyV1],
    rows: &[u32],
    court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
    ladder: u64,
) -> Option<u64> {
    let chunks = crate::palw_context_ladder::palw_close_chunks_for_ladder_v1(families, rows, court, ladder)?;
    Some(crate::palw_mode_v2::palw_close_bytes_for_chunks_v1(chunks))
}

/// testnet-11's windows: bind 600, receipt 600, challenge 1,200, court 3,000, at the frozen 120 s
/// cadence. Unchanged by ADR-0077 Decision 7 — the devnet set is a second network, not an edit.
pub const PALW_RC_WINDOWS_V1: PalwLatticeWindowsV1 = PalwLatticeWindowsV1 {
    window_bind: 600,
    window_receipt: 600,
    window_challenge: 1_200,
    window_court: 3_000,
    anchor_delay: 20,
    max_beacon_gap: 400,
    reorg_margin: 300,
    receipt_maturity: 400,
    receipt_use_window: 600,
    // **51, down from 60, because the ladder got deeper on 2026-09-03.** The turn clock and the
    // ladder depth are one constraint, not two: `moves × turn_deadline + assembly reserve <
    // window_court`, and raising `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` to 2^26 took the move count
    // from 46 to 54. At the old 60 that is `54 × 60 + 216 = 3,456` against a 3,000-DAA window, and
    // `PalwConsensusParamsV2::validate` refuses the bundle outright — "window_court does not fit
    // the worst-case honest prosecution". It refused 59 tests before this line moved, which is the
    // gate doing its job: a deeper ladder that did not pay for itself never assembled.
    //
    // **42 rather than 2^26's own derivation of 51**, and that difference is the whole point of
    // the number. 51 is `(3,000 − 216 − 1) / 54`, correct for the ladder that ships today and
    // wrong the moment the `2^32` context-ladder fence arms: 66 moves × 51 + 216 = 3,582, past a
    // 3,000-DAA window, so a fence that is supposed to be a rollout would be a bundle that will
    // not assemble. 42 is the derivation at the DEEPEST ladder the tree can reach, and it fits
    // every shallower one — `54 × 42 + 216 = 2,484` here, `66 × 42 + 216 = 2,988` armed.
    //
    // A clock chosen for the ladder of the day is how a fence becomes a flag day. This is the same
    // rule ADR-0077 Decision 12 derives and the same 42 `palw_court_turn_deadline_v1` returns at
    // the armed depth, so the shipped constant and both derivations now agree.
    //
    // It is still far above every shipped row's replay floor (ADR-0077 SA-4: the hybrid's worst
    // rung is 2 DAA), so nothing here convicts an honest responder by clock.
    court_turn_deadline: 42,
    fp_abandon_hold: 600,
    claim_retirement: 3_000,
    withdrawal_delay: 7_500,
    // ADR-0080's ceiling: 27 carriers, 2,250,000 counted bytes, and a 216-DAA assembly reserve out
    // of the 3,000-DAA court window. A 100-hour window can afford a 54-minute move; the set below
    // cannot, which is why it is a field.
    court_max_close_bytes: COURT_MAX_CLOSE_BYTES,
};

/// **The devnet lattice, in minutes** (ADR-0077 Decision 7 — "a devnet preset whose windows are
/// minutes"). Same interlocks, one-fifteenth the spans: a commitment binds within 40 DAA, is
/// licensed within 40 more, finalizes 100 after that and its draw matures 20 later — ~200 DAA
/// from commitment to spendability, under an hour on a fixture-paced devnet. The court window
/// holds a 2^32 ladder at a 4-DAA move clock (`(2 × 32 + 2) × 4 = 264`, plus an 8-DAA assembly
/// reserve, against 300).
///
/// **These three numbers moved once and moved back, and the round trip is the lesson.** ADR-0080's
/// assembly reserve is `2 × 4 × max_close_chunks`, and read from the RC's 27 it is 216 DAA — which
/// 300 cannot hold at any clock, so `window_court` went to 600, the derived clock to 5 and
/// `withdrawal_delay` to 900 to keep the bond interlock. They are back at 300, 4 and 600 because
/// the reserve was never devnet's to pay: `max_close_chunks` is a RULESET quantity, this set now
/// carries its own single-carrier close ceiling below, and its reserve is 8. A preset constant that
/// moved for a reason that stopped being true is the defect, not the fix.
pub const PALW_DEVNET_WINDOWS_V1: PalwLatticeWindowsV1 = PalwLatticeWindowsV1 {
    window_bind: 40,
    window_receipt: 40,
    window_challenge: 100,
    window_court: 300,
    anchor_delay: 4,
    max_beacon_gap: 20,
    reorg_margin: 10,
    receipt_maturity: 20,
    receipt_use_window: 40,
    court_turn_deadline: 4,
    fp_abandon_hold: 40,
    claim_retirement: 300,
    withdrawal_delay: 600,
    court_max_close_bytes: DEVNET_COURT_MAX_CLOSE_BYTES,
};

/// **The devnet court's close ceiling: one carrier** (ADR-0080).
///
/// 81,920 — 80 KiB, the value every network carried before ADR-0080 widened the RC's, and the
/// largest close that fits a single `ObjectChunk` after framing
/// (`palw_close_chunks_for_bytes_v1(81_920)` = 1, with 1,696 carried bytes to spare).
///
/// **Why the devnet is the network that keeps it.** A split close buys context width by spending
/// BLOCKS: the reserve is `2 × 4 × chunks` off the court window and the floor under an honest move
/// is `replay + (chunks − 1)`, so the RC's 27 carriers cost 216 DAA of window and 26 DAA of every
/// move. At 120 s a block that is a 54-minute move inside a 100-hour window — correct for a chain
/// that has to prosecute the hybrid tier's 2,240,241-byte close. The devnet lattice is a DRILL that
/// must finish in a session; paying 216 out of a minutes window needs `window_court ≥ 1998`, 66
/// hours, and there is nothing to pay it FOR. Devnet registers the floor, whose worst close is
/// 52,704 bytes, and every class row this build ships closes in one carrier — 78,688 bytes at the
/// widest ([`tests::the_devnet_close_ceiling_admits_every_shipped_row`], which prints all five).
///
/// **What it costs, stated rather than glossed:** a narrower ceiling ADMITS LESS. Under the shipped
/// `2^22` ladder, `palw_class_admission_v2::tests::the_widest_context_each_family_admits` measures
/// the dense family at `n_ctx` 21 under 80 KiB against 39 under 2,250,000, and the hybrid at 8
/// against 12. So a devnet drill that wants to register a row wider than the ones this build ships
/// is refused here where testnet-11 would admit it — which is the trade, and it is the right way
/// round: a ceiling admits, it does not require, and this network buys a session-length court
/// instead of a width no drill has asked for.
///
/// This is the field's whole reason for existing, and `the_two_rulesets_derive_different_clocks`
/// is the test that neither number here nor the RC's is typed.
/// **The margin is real but it is not wide, and it is measured**: the widest shipped row is the
/// dense A16 at 78,688 bytes, 3,232 under this ceiling —
/// `the_devnet_close_ceiling_admits_every_shipped_row` prints all five and fails the day one of
/// them needs a second carrier. That day is a decision about the devnet WINDOW, taken in daylight,
/// and not a default to inherit.
const DEVNET_COURT_MAX_CLOSE_BYTES: u64 = 81_920;

/// **3,000, from the corrected worst case** (audit M2-24). The ladder's clock runs per MOVE, and a
/// bisection round is two of them — a disclosure and a verdict — so a 22-rung ladder plus two
/// terminal moves is `(2 x 22 + 2) x 60 = 2,760` DAA of honest prosecution. `worst_case_duration_daa`
/// counted rounds and returned 1,440, so 2,400 looked like generous headroom while being 360 DAA
/// short: two parties each moving at `deadline − 1` would have run past the backstop, and the
/// backstop closes on the challenger's side — a dispute lost for being played correctly. 3,000
/// leaves ~9 % margin and keeps the startup invariant that refuses a bundle where the window cannot
/// hold its own ladder.
#[cfg(test)]
const WINDOW_COURT: u64 = PALW_RC_WINDOWS_V1.window_court;

/// Class-production epoch / retarget span.
const EPOCH_LENGTH: u64 = 1_000;

/// ADR-0044 Decision 1's floor: 150‰ of combined production is the attempt lane, the beacon
/// source. Low enough that free-prompt work dominates the chain's weight; high enough that
/// beacons keep arriving (~1 in 7 blocks) and that `max_beacon_gap` below is a promise the lane
/// can keep.
/// **The whole cadence, because the attempt lane is the only lane that can produce** (launch
/// blockers §6).
///
/// This was 150‰, leaving 850‰ to the ADR-0044 receipt lane — a lane a `ConsensusV2` network makes
/// structurally impossible: `algorithm_id` pins the network to algo-6 and the header gate rejects
/// every algo-7 header before its admission path is reached.
///
/// The per-class retarget measures each lane against the COMBINED census, so with the receipt lane
/// producing nothing `combined` is the attempt lane alone: the floor holds 100% of what happened
/// while being expected to hold 15% of it. A 6.67x over-producer verdict at EVERY epoch boundary,
/// each dividing the target by up to `class_daa_max_factor`, with nothing bounding the walk — the
/// target reaches its floor of 1 and the class lottery then refuses every attempt. A chain that
/// stops with no path back, roughly 63 epochs in.
///
/// **The premise above is false, and the value moved back off the rail.**
///
/// `PalwConsensusMode::accepts_algo_id` returns true for `receipt_algorithm_id()`, so algo-7
/// headers are accepted and receipt blocks reach the census. Holding 1000‰ then produces the
/// error in the other direction and on an axis anyone can push: the attempt lane is expected to
/// hold the whole census while receipt blocks dilute it, so it is measured as an UNDER-producer at
/// every boundary and its target is eased without bound — while the receipt lane, holding no
/// share, never retargets at all.
///
/// 900‰/100‰ rather than the original 150‰/850‰. Two things changed under that number: receipt
/// blocks now carry no blue work at all (ADR-0055 D1), so a receipt-dominant cadence would grow
/// chain weight at a fraction of the block rate; and a receipt block spends a CERTIFIED quantum,
/// which only an attempt claim reaching `Final` can supply, so the receipt lane cannot outrun the
/// lane that feeds it. `ANCHOR_MAX_GAP` below was derived at 150‰ and stays correct here for the
/// safe reason: more attempt blocks make the wait for one shorter, never longer.
///
/// This is a genesis-time economic choice, and it rides the re-mint with the rest of ADR-0055.
const ATTEMPT_SHARE_PERMILLE: u16 = 900;

/// The panel's anchor delay: 20 DAA after acceptance the claim's beacon slot opens.
#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const ANCHOR_DELAY: u64 = PALW_RC_WINDOWS_V1.anchor_delay;
/// Declared worst-case wait for the first attempt-class block at or after a slot. At a 150‰
/// floor the expected gap is ~7 blocks; 400 is a wide margin, and the startup gate proves
/// `ANCHOR_DELAY + this < WINDOW_BIND`, so even a very unlucky lull still binds in time.
#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const MAX_BEACON_GAP: u64 = PALW_RC_WINDOWS_V1.max_beacon_gap;

/// Draw maturity and use window. Maturity must cover the reorg margin (the draw beacon must sit
/// past the reorgable fringe of the certification it draws for); the use window is generous
/// because a producer that misses it loses the win outright.
#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const REORG_MARGIN: u64 = PALW_RC_WINDOWS_V1.reorg_margin;
#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const RECEIPT_MATURITY: u64 = PALW_RC_WINDOWS_V1.receipt_maturity;
#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const RECEIPT_USE_WINDOW: u64 = PALW_RC_WINDOWS_V1.receipt_use_window;

/// Measured worst-case honest prosecution time — a PLACEHOLDER shaped like the real thing: the
/// gate demands `window_court > this`, so the constant is what a fleet measurement replaces, and
/// until it does, the ratio here (3000 vs 1200) is the safety factor.
/// The court's SHAPE, from which `worst_case_duration_daa` is derived (ADR-0042 Decision 8:
/// `(ceil(log2(leaves)) + terminal) × turn_deadline`).
///
/// **The whole step space, not a number sized for today's catalog** (ADR-0049 Decision C,
/// `assemble_palw_rc_identity_v2` gate 5). This was `1 << 16`, chosen when BASE-0's graph was
/// eighteen steps per layer; declaring the narrowings the engine performs took its longest job to
/// 366,728 leaves and the ladder could no longer reach its own liveness floor. Sizing a ladder to
/// the class set of the day is how that happens, and the value is inside `palw_ruleset_id_v2`, so
/// it cannot be raised afterwards.
///
/// 2^26 leaves = 26 bisection rounds, +2 terminal, so 54 moves; at the derived turn deadline of 51
/// that is `54 × 51 + 216 = 2,970`, inside `WINDOW_COURT` (3,000) with ADR-0080's assembly reserve
/// already charged. Nothing deeper than the cap is admissible at all, so this ladder cannot fail to
/// reach a class that exists.
///
/// **Raised from 2^22 with the RC set on 2026-09-03**; the measurement and the reasoning live on
/// `PALW_RC_COURT_MAX_STEP_LEAF_COUNT`, and this const reads it so the two rulesets cannot drift.
/// The short version: at 2^22 the A16 tier admits `n_ctx` 39, the card registers 512, and the
/// grammars need 38/60/104 decode tokens before prefill.
///
/// `pub` because W1b turned this from decoration into a choice: the executor reads the ruleset
/// now, so this is the width lever for every registered class, and a value a test cannot name
/// is a value nobody can hold. `the_shipped_ruleset_admits_the_row_the_genesis_registers` reads
/// it and was RED until this line moved.
pub const COURT_MAX_STEP_LEAVES: u64 = crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT;
#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const COURT_TURN_DEADLINE: u64 = PALW_RC_WINDOWS_V1.court_turn_deadline;
const COURT_TERMINAL_ROUNDS: u32 = 2;

/// **What a round may COST** (ADR-0049 Decision C), named here rather than left to the
/// constructor's defaults.
///
/// `PalwCourtParamsV2::new` installs the same values, so this changes no byte of the bundle — and
/// that is the point. These three are inside `palw_ruleset_id_v2` exactly as the ladder is, and
/// `assemble_palw_rc_identity_v2` gate 6 refuses an identity carrying anything else, so the RC's
/// own bundle must SAY them: a ruleset whose most consequential frozen numbers arrive as a
/// default is a ruleset nobody chose. See `palw_class_admission_v2::PALW_RC_COURT_MAX_CLOSE_BYTES`
/// for the carriage arithmetic that fixes the first.
///
/// **The close ceiling is the one of the three that is now per-NETWORK** (ADR-0080 W4). It reaches
/// the bundle through [`PalwLatticeWindowsV1::court_max_close_bytes`] rather than from here,
/// because the assembly reserve a window must fund is a function of it and the two shipped sets
/// need different answers; this constant is the RC set's value, and [`PALW_RC_WINDOWS_V1`] is where
/// it is spent.
const COURT_MAX_CLOSE_BYTES: u64 = crate::palw_class_admission_v2::PALW_RC_COURT_MAX_CLOSE_BYTES;
const COURT_MAX_TERMINAL_MACS: u64 = crate::palw_class_admission_v2::PALW_RC_COURT_MAX_TERMINAL_MACS;
const COURT_MAX_OPERAND_COUNT: u32 = crate::palw_class_admission_v2::PALW_RC_COURT_MAX_OPERAND_COUNT;

/// **A quantum is an eighth of the class's canonical job** (ADR-0074 Decision 5): the quantum
/// for a class is `max(1, pwu_per_inference / 8)` leaves. A job the size of the canonical one is
/// eight draws on every class; a floor free-prompt job of five prompt and two decode tokens lands
/// near the seven quanta the CU table used to give it; and `pwu = quanta × quantum` stays in
/// leaves — the attempt lane's unit — across lanes and classes. The CU weights (1 : 64),
/// `QUANTUM_CU` (100) and `PWU_PER_QUANTUM` (10) this replaced are recorded in ADR-0044
/// Decision 7 and in ADR-0074's supersession table.
const FP_QUANTA_PER_CANONICAL_JOB: u32 = 8;
/// Per-receipt jackpot bound: a single enormous job cannot buy unbounded consecutive blocks.
const MAX_QUANTA_PER_RECEIPT: u32 = 64;

/// Shape caps, inside the worker's own limits (single-batch prefill 512, trace-event cap 4096).
const MAX_PROMPT_TOKENS: u32 = 512;
const MAX_DECODE_TOKENS: u32 = 1_024;

/// Per-bond exposure ceiling, in permille of slashable collateral. 500‰: a bond may back
/// immature claims worth at most half its collateral, so the first slash always lands inside
/// what the bond can actually pay.
const MAX_EXPOSURE_RATIO_PERMILLE: u32 = 500;

/// Devnet bond floor and withdrawal delay. The delay must outlast bind+receipt+challenge+court
/// plus the reorg margin (5100 here), so 6000 leaves margin without making a devnet operator
/// wait a week to leave.
/// Sized against what a claim actually RESERVES, which is the number that binds: one inference's
/// worth, `pwu_per_inference × slash_value_per_pwu` (`palw_state_v2::palw_exposure_pwu_v1`). At
/// 4,096 per inference and 5 per pwu that is 20,480 per claim, so a bond must carry at least
/// `20_480 / (500‰)` = 40,960 to make ONE claim, and this floor funds several concurrent ones.
///
/// **The figures here used to be twice as large**, because a claim was priced on the derived `pwu`
/// — which at `initial_target = u128::MAX/2` carries a factor of 2, and at any other target a
/// different one. An exposure ceiling that moves with the difficulty locks a class's own producers
/// out for succeeding; the collateral floor inherited that motion and is now free of it.
///
/// The old 20,000 was chosen before the exposure ceiling had a consumer, and it made every
/// attempt on this bundle refusable. Measured the moment P0-10's check was wired into the
/// pipeline, which is exactly what that check is for.
const MIN_COLLATERAL_SOMPI: u64 = 400_000;

// ---------------------------------------------------------------------------------------------
// ADR-0056 Decisions 3 and 5: the class economy
// ---------------------------------------------------------------------------------------------

/// **What one live registration reserves against its registrant's bond** (Decision 3).
///
/// Sized against `MIN_COLLATERAL_SOMPI` and `MAX_EXPOSURE_RATIO_PERMILLE`, not chosen: a minimum
/// bond's ceiling is `400,000 × 500‰ = 200,000` sompi, so at 40,000 a smallest-possible bond can
/// hold **five** live registrations and nothing else — and each one it adds is a claim it can no
/// longer make, because both draw on the one ceiling. An operator running one honest model spends
/// a fifth of a minimum bond's headroom; a flooder wanting a hundred dead classes needs twenty
/// minimum bonds' worth of collateral, idle, for as long as the classes live.
///
/// A reservation, never a burn: it returns at reclamation or freezing, so what a flood pays is the
/// TIME VALUE of that collateral, and what an honest registrant pays is the same thing for as long
/// as their class is useful.
const REGISTRATION_EXPOSURE_SOMPI: u64 = 40_000;

/// Decision 5: twelve consecutive epochs of ZERO production reclaims the class. Three times the
/// decay window, because reclamation takes the whole share and frees the collateral — a heavier
/// move deserves a longer look, and a class that produced even one block in twelve epochs is not
/// what this rule is about.
const RECLAIM_EPOCHS: u32 = 12;

#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const WITHDRAWAL_DELAY: u64 = PALW_RC_WINDOWS_V1.withdrawal_delay;

/// Per-adjustment retarget clamp (ADR-0038 Decision D) and the ADR-0045 Decision 2 epoch-budget
/// tolerance, in permille of a class's cadence share. Unity is the floor (below it a budget
/// starves its own class); 1000‰ is the devnet's honest "exactly its share" setting.
const CLASS_DAA_MAX_FACTOR: u32 = 4;

/// **ADR-0054: how fast a class's cadence share follows its own production.**
///
/// A quarter of its own share per closed epoch, so a class needs sustained production to reach a
/// meaningful permille — the measured trajectory from the grant floor is 1, 2, 3, 4, 5, 6, 7, 8,
/// 10, 12, 15, 18, 22, 27, 33 over fourteen epochs — and gives it back at the same rate when it
/// stops. Fast enough that a class worth running is not waiting a year; slow enough that nobody
/// takes the cadence table before anyone has watched them produce.
const CLASS_GROWTH_PERMILLE: u16 = 250;

/// **The permille the liveness floor keeps** (ADR-0054 Decision 2; **20‰ since ADR-0068 Phase 2**,
/// was 500). Half the table was the pre-heartbeat figure: with no clock but the floor, a large
/// reserve WAS the liveness story. ADR-0068 armed the clock (heartbeat lane, drill-proven: the
/// nominal hour to the second, unattended re-entry, the exposure wedge released by ticks alone)
/// and with time supplied by a lane no bond can stop, a 50% reserve defends nothing the census
/// and the heartbeat do not already defend — it only caps the model classes at half the cadence
/// forever. Twenty permille keeps the three jobs the floor still has: the permissionless entry
/// ramp (one floor block funds ~10⁵ minimum collaterals), the artifact-less KAT class the dispute
/// machinery tests against, and the census's expansion seed (an epoch with silent model classes
/// hands the floor the whole budget regardless of this figure — the ambulance wage is ADR-0045
/// arithmetic, not this reserve). ADR-0039 W6' holds: never zero, and it does not leave.
///
/// Set through `with_min_base_class_share_permille`, not through the growth builder: the merge
/// converged three independent floor guards onto that one field, so a grant and a growth step check
/// the same number.
const BASE_CLASS_RESERVE_PERMILLE: u16 = 20;
const BUDGET_TOLERANCE_PERMILLE: u32 = 1_000;

/// Audit C5's abandon hold, in DAA: how long a free-prompt commitment abandoned at `BindTimeout`
/// keeps its collateral reserved. One bind window (600) — the span the producer had to bind and
/// chose not to use, which is the natural price for declining it, and long enough that a redraw
/// loop needs a fresh reservation per attempt rather than recycling one.
#[cfg(test)]
const FP_ABANDON_HOLD: u64 = PALW_RC_WINDOWS_V1.fp_abandon_hold;

/// **How long a terminal claim is kept before it is removed** (launch blockers §8, third bullet).
///
/// Set to the court window, which is the longest single window on the lattice, so a claim stays
/// inspectable for as long as the slowest dispute it could have been part of would have taken.
///
/// What it buys is a BOUND. A claim's whole live span is at most
/// `bind + receipt + challenge + court` = 4,800 blocks, so with this the claim map settles at
/// ~7,200 entries instead of growing by one per block forever: ~3.8 MB of tip row and ~6 ms of
/// `state_root` per block at the frozen 120 s cadence, flat, rather than 54 MB and 49 ms after
/// four months and no ceiling after that.
#[allow(dead_code)] // the RC set's named reading, kept beside its rationale (ADR-0077 Decision 7)
const CLAIM_RETIREMENT: u64 = PALW_RC_WINDOWS_V1.claim_retirement;

/// The worker share of the fixed subsidy (a carve, never an addition).
const WORKER_CARVE_PERMILLE: u16 = 620;

/// The devnet bundle. `base_class_id` is the caller's — the class id is a genesis artifact (the
/// registered BASE-0 class), and inventing one here would put a second source of that fact in
/// the tree. `class_catalog_root` / `court_catalog_root` likewise come from the genesis that
/// registers them; the boot path verifies their preimages.
///
/// Single-class table (BASE-0 at 1000‰), which is the honest devnet shape: an accelerated class
/// with `coverage < 100%` carries share 0 at RC anyway (ADR-0042 Decision 8).
/// **The target the collateral derivation prices a claim at — no longer any network's seed**
/// (ADR-0076).
///
/// It was the value every genesis registration carried, and the two uses shared one constant
/// honestly. They do not share one any more: a class's seed is now
/// `palw_class_daa::attempt_target_seed_v1(share, pwu_per_inference)`, which differs per class and
/// per network, while what `palw_v2_collateral_for_claim_lifetime_v1` needs here is a fixed factor
/// for a MARGIN — its own doc says the derivation is deliberately an over-estimate, because a
/// claim's real reservation is `palw_exposure_pwu_v1`, which is `pwu_per_inference` alone and does
/// not depend on any target at all.
///
/// So this stays exactly where it was and means something narrower: **a 2× margin on derived
/// collateral**, named by the target that used to produce it. Moving it with the seeds would
/// re-size every shipped genesis registry to chase a factor that is not in the reservation it
/// funds; funding a bond above its requirement costs an operator nothing the chain enforces, and
/// funding one below it is the permanent wedge that doc comment is about.
const GENESIS_CLASS_TARGET: u128 = u128::MAX / 2;
/// Slashable value per pwu — what an exposure ceiling is measured in.
const SLASH_VALUE_PER_PWU: u64 = 5;

/// The longest a single claim can hold its executor's exposure: every window on the path from
/// creation to a terminal phase, laid end to end.
///
/// `release_for_claim` runs on `Final` and on `Voided` — and nowhere else. Exposure therefore
/// stands for the WHOLE lifetime of a claim, not until a panel binds it. Sizing collateral
/// against `WINDOW_BIND` alone wedged testnet-12 at block 601 with `weight=0`: the ceiling
/// admitted 601 concurrent claims, while the earliest `Final` a chain can reach is one licensed
/// claim plus `WINDOW_CHALLENGE` away — DAA 1200 at one claim per block. Every claim the chain
/// was waiting to finalize was itself occupying the room the chain needed to keep producing.
/// Blocks were produced, receipts were licensed, and nothing ever finalized.
// TWO bind+receipt pairs — the redraw (`sweep_deadlines`' `PanelBound` arm) revives a claim once
// and binds a second panel, so a claim's exposure is held across the LONGER path. Sizing this at
// one pair under-funded the derived collateral by 20 % against the lifetime it actually reserves.
const MAX_CLAIM_EXPOSURE_DAA: u64 = PALW_RC_WINDOWS_V1.max_claim_exposure_daa();

/// One bond in a genesis registry.
///
/// A registry, not a bond: `derive_panel_v2` excludes a claim's own executor by bond, by operator
/// and by key, and seats one bond per OPERATOR — so a `seat_count`-seat panel needs `seat_count + 1`
/// distinct operators and `BondRegistered` may not ride a transaction. The registry is fixed at
/// genesis and there is no later repair, which is why this is a list and why
/// `verify_palw_genesis_v2` refuses a list too short to seat the panel it ships with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGenesisBondSpecV1 {
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub pubkey: Vec<u8>,
    pub operator_pubkey: Vec<u8>,
    /// Where this bond's matured rewards are paid — the 64-byte P2PKH-ML-DSA-87 owner payload. A
    /// field rather than a value derived from `operator_pubkey`, because paying rewards to a
    /// different key than the one that signs is an ordinary operational choice (a cold payout
    /// address), and deriving it would quietly forbid it.
    pub payout_payload: Hash64,
}

/// **The collateral a bond must declare to survive its own bind window.**
///
/// Every ConsensusV2 block is an attempt, so every block creates a claim reserving
/// `pwu_per_inference × slash_value_per_pwu` until `BindTimeout` at `+window_bind`; and DAA advances only when
/// blocks are produced. A bond whose ceiling admits fewer concurrent claims than the window is long
/// therefore deadlocks: the producer fills the ceiling and then needs DAA it can only get by
/// producing. Derived rather than declared so a change to the window, the target, the class or the
/// exposure ratio moves the requirement with it — the shipped bundle carried a hand-written
/// 400,000 against a requirement of 94,800,000 and stopped at block two.
///
/// **The window is not the count; the count is the window PLUS ONE, and the difference is a
/// permanent wedge.** Sizing to exactly `window_bind` moved the deadlock from block 2 to block 600
/// rather than removing it, which is what testnet-12's first run measured: 600 blocks produced,
/// then `holding: the bond's exposure ceiling leaves no room for another claim`, forever.
///
/// The arithmetic, with one claim per block and DAA advancing one per block. A claim accepted in
/// block `k` carries deadline `k + window_bind`, and the sweep voids deadlines STRICTLY before the
/// block's DAA. But admission runs against the PARENT state, before that block's own sweep, so
/// the room a producer needs is one claim MORE than the span it must cover. One short is not a
/// near miss: the chain cannot produce the block that would have released the room, and there is
/// no timeout that helps, because DAA only advances when blocks are produced.
///
/// The span to cover is `MAX_CLAIM_EXPOSURE_DAA`, not the bind window — a claim's exposure is
/// released at `Final`, and the road to `Final` runs through every window in the lattice.
///
/// This is the structural minimum, not a safety margin. A network that wants slack against DAA
/// advancing more slowly than one per block (parallel producers sharing a DAA score) must fund
/// beyond it — the ceiling is per BOND, so a bond that produces in parallel with itself needs a
/// multiple.
/// **Deliberately sized on the DERIVED pwu, which is now an over-estimate, and that is the safe
/// direction.** A claim reserves one inference's worth
/// (`palw_state_v2::palw_exposure_pwu_v1`); `palw_pwu_v1` multiplies that by the expected attempt
/// count at the genesis target. Funding a bond above its requirement costs an operator nothing the
/// chain enforces, while funding one below it is the permanent wedge this whole doc comment is
/// about — so the margin stays, named, rather than being tightened into the exact figure and
/// moving every shipped network's genesis registry to save collateral nobody is short of.
pub fn palw_v2_collateral_for_claim_lifetime_v1(pwu_per_inference: u64) -> u64 {
    let pwu = crate::palw_pwu::palw_pwu_v1(GENESIS_CLASS_TARGET, pwu_per_inference);
    let per_claim = (pwu as u128).saturating_mul(SLASH_VALUE_PER_PWU as u128).max(1);
    let ceiling = per_claim.saturating_mul(MAX_CLAIM_EXPOSURE_DAA as u128 + 1);
    let collateral = ceiling.saturating_mul(1000).div_ceil(MAX_EXPOSURE_RATIO_PERMILLE as u128);
    collateral.max(MIN_COLLATERAL_SOMPI as u128).min(u64::MAX as u128) as u64
}

/// The panel's seats and quorum.
///
/// Public because a genesis registry has to be SIZED against them — `derive_panel_v2` excludes a
/// claim's executor by bond, by operator and by key and seats one bond per operator, so a registry
/// needs `PALW_V2_PANEL_SEATS + 1` distinct operators or no claim is ever licensed.
/// `verify_palw_genesis_v2` refuses a shorter one; before it did, the shipped RC bundle carried a
/// single bond and would have produced blocks that carried no weight, forever, silently.
pub const PALW_V2_PANEL_SEATS: u16 = 5;
pub const PALW_V2_PANEL_QUORUM: u16 = 3;

/// How many distinct-operator bonds a genesis registry needs: the seats, plus the executor that
/// never sits on its own panel.
pub const fn palw_v2_min_genesis_bonds_v1() -> usize {
    PALW_V2_PANEL_SEATS as usize + 1
}

/// **How many a registry needs before ADR-0065 D1's maturity fence may be ARMED** — which is a
/// different and larger question than how many it needs to seat a panel at all.
///
/// [`palw_v2_min_genesis_bonds_v1`] is the minimum to seat ONE panel with nothing to spare. D1
/// refuses a seat to any bond registered later than `anchor_daa - bond_maturity_daa`, and the draw
/// is fail-closed: too few eligible bonds is not a smaller panel, it is `InsufficientEligibleBonds`
/// — no panel, so the claim voids at `BindTimeout` and the frontier stops. On a registry with no
/// spare, arming D1 turns any single departure into an outage for a whole maturity window:
///
/// * `SEATS + 1` — the executor is excluded from its own panel, so this is the floor to seat at all;
/// * `+ 1` — one bond may LEAVE eligibility (its holder retires it, or the escrow slash drives its
///   collateral under `min_collateral_sompi`) and panels must still bind. The registry is
///   append-only, so after a departure and an immediate replacement it holds `N + 1` rows of which
///   one is `Retiring` and one is immature: `available = N - 2`. Requiring `>= SEATS` gives
///   `N >= SEATS + 2`, which is the STRICT minimum.
/// * `+ 1` more — margin for a second concurrent departure inside one window.
///
/// **The `+1` for margin is the honest reading; an earlier version of this note justified it as
/// "the replacement's own maturity" and that double-counted.** The replacement's immaturity IS the
/// departing bond's gap — the same missing seat, not a second one. The number is unchanged because
/// `SEATS + 3` is what one would ship anyway; the reason for it is not.
///
/// Two things bound it from the other side, and both argue against reaching for more:
/// * **Operator uniqueness is not enforced at registration.** `DuplicateBondKey` refuses a second
///   bond for one KEY, but nothing refuses a second bond for one `operator_id`, and the draw seats
///   one bond per operator — so a replacement registered under an existing operator's key adds a
///   row and restores no seat. Every added card needs a fresh operator key as well as a fresh bond.
/// * **Slack nobody staffs is a liveness regression, not insurance.** The draw is liveness-blind:
///   an offline bond still takes a seat, so a panel reaches quorum by presence only while unstaffed
///   bonds are at most `SEATS - QUORUM`. At `SEATS + 3` that means at least six staffed hosts;
///   growing the registry past what the fleet can actually run makes panels fail more often, not
///   less.
///
/// `Params::validate_palw_v2` refuses to arm the fence on a genesis that registers fewer, so "do
/// not arm D1 on a network running at `seat_count + 1`" is a rule the node enforces rather than a
/// sentence in an ADR somebody has to remember.
pub const fn palw_v2_maturity_armable_bonds_v1() -> usize {
    PALW_V2_PANEL_SEATS as usize + 3
}

/// A seatable registry from one deterministic seed — the fixture shape every caller needs now that
/// a registry too small to seat a panel is refused. Each row gets its own outpoint, key AND
/// operator, because a registry of clones is one operator however long it is.
pub fn palw_devnet_bond_registry_v1(count: usize) -> Vec<PalwGenesisBondSpecV1> {
    (0..count as u64)
        .map(|n| PalwGenesisBondSpecV1 {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(0xB0 + n),
                index: 0,
            }),
            pubkey: vec![7u8.wrapping_add(n as u8); 4],
            operator_pubkey: vec![21u8, n as u8, 0, 0, 0, 0, 0, 0],
            payout_payload: Hash64::from_u64_word(0x9A11 + n),
        })
        .collect()
}

pub fn palw_fp_devnet_bundle_v3(
    base_class_id: Hash64,
    class_catalog_root: Hash64,
    court_catalog_root: Hash64,
    // The catalog's counted canonical step-leaf count for `base_class_id` — the value
    // `verify_palw_genesis_v2` demands the registration declare.
    genesis_pwu_per_inference: u64,
    genesis_artifact_root: Hash64,
    // The whole registry, because a panel is seated out of it — see `PalwGenesisBondSpecV1`.
    genesis_bonds: Vec<PalwGenesisBondSpecV1>,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    palw_fp_bundle_with_windows_v3(
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        genesis_pwu_per_inference,
        genesis_artifact_root,
        genesis_bonds,
        &PALW_RC_WINDOWS_V1,
    )
}

/// [`palw_fp_devnet_bundle_v3`] under a named window set (ADR-0077 Decision 7). The RC set
/// reproduces the bundle above byte for byte; the devnet set is what `devnet_shipped_params`
/// assembles. Everything that is NOT a window — the economy, the ladder, the caps, the
/// registrations, and the collateral derivation (sized against the RC lifetime so a devnet
/// genesis premine is unchanged and over-funded rather than re-laid) — is one source for both.
pub fn palw_fp_bundle_with_windows_v3(
    base_class_id: Hash64,
    class_catalog_root: Hash64,
    court_catalog_root: Hash64,
    genesis_pwu_per_inference: u64,
    genesis_artifact_root: Hash64,
    genesis_bonds: Vec<PalwGenesisBondSpecV1>,
    windows: &PalwLatticeWindowsV1,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    // Derived, not declared: see `palw_v2_collateral_for_claim_lifetime_v1`. Applied to the policy
    // minimum as well as to every registration, so "the minimum collateral" means "the smallest
    // stake that can carry a claim through the bind window" rather than a number chosen earlier.
    let genesis_collateral = palw_v2_collateral_for_claim_lifetime_v1(genesis_pwu_per_inference);
    // ADR-0045 Decision 3: no share table here. The chain grants shares at registration — the
    // first registration on the chain must be `base_class_id` at the whole 1000‰, which is the
    // single-class devnet shape this bundle used to spell out as a params constant.
    let state = PalwStateParamsV2::new(
        BETA_PERMILLE,
        windows.window_bind,
        windows.window_receipt,
        windows.window_challenge,
        windows.window_court,
        EPOCH_LENGTH,
        base_class_id,
        CLASS_DAA_MAX_FACTOR,
        BUDGET_TOLERANCE_PERMILLE,
        MIN_COLLATERAL_SOMPI,
        ATTEMPT_SHARE_PERMILLE,
        windows.fp_abandon_hold,
    )?
    // The free-prompt price (ADR-0074 Decision 5) lives in the state params because the
    // transition is what derives quanta and pwu — the SAME two numbers `freeprompt` declares
    // below, checked equal by `validate()`.
    .with_fp_quanta(FP_QUANTA_PER_CANONICAL_JOB, MAX_QUANTA_PER_RECEIPT)?
    // Admission item 8 on the free-prompt lane: the SAME ratio `admission` declares below.
    .with_fp_exposure_ceiling(MAX_EXPOSURE_RATIO_PERMILLE)?
    // The SAME constants the `reward` and `court` fields below declare — `validate()` requires
    // each pair to agree, so these are not second sources, they are the one source reaching both
    // readers. `COURT_TURN_DEADLINE` here is what turns the interactive ladder ON: it is strictly
    // inside `WINDOW_COURT`, which is what makes a rung deadline able to fire at all.
    .with_worker_carve_permille(WORKER_CARVE_PERMILLE)?
    .with_turn_deadline_daa(windows.court_turn_deadline)?
    .with_claim_retirement_daa(windows.claim_retirement)?
    // **ADR-0056 Decisions 3 and 5, as this network sets them**: the registration reservation and
    // the reclamation window. The share WALK is not here — it is the growth rule below, which
    // measures filled budget instead of a streak.
    .with_min_base_class_share_permille(BASE_CLASS_RESERVE_PERMILLE)?
    .with_class_economy_v1(REGISTRATION_EXPOSURE_SOMPI, RECLAIM_EPOCHS)?
    // ADR-0054: the share table follows production. Without it a post-genesis entrant holds
    // `min_grantable_share_permille` forever, its expectation and its budget are both one block per
    // epoch, and the per-class retarget has no reachable input — measured on a two-class chain
    // carrying the real Qwen3.6 class, whose target did not move across four epochs in either state
    // its share allowed it to be in.
    .with_class_share_growth_v1(CLASS_GROWTH_PERMILLE)?;
    // The epoch budget: what one class may produce per epoch, in pwu. Sized so a full epoch of
    // receipt blocks at `PWU_PER_QUANTUM` fits with headroom — a budget that binds before the
    // difficulty does would make the DAA a decoration.
    let admission = PalwAdmissionParamsV2::new(MAX_EXPOSURE_RATIO_PERMILLE)?;
    let freeprompt = PalwFreePromptParamsV3::new(
        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
        FP_QUANTA_PER_CANONICAL_JOB,
        MAX_QUANTA_PER_RECEIPT,
        MAX_PROMPT_TOKENS,
        MAX_DECODE_TOKENS,
        windows.receipt_maturity,
        windows.receipt_use_window,
        windows.max_beacon_gap,
    )?;
    let bundle = PalwConsensusParamsV2 {
        // **One panel, and it is the network's.** ADR-0051 put a `min_class_panel` floor of
        // `(2, 2)` here so a Metal class could license with two seats instead of six; ADR-0053
        // withdrew that family and this goes with it. Two things were wrong with it beyond the
        // family: the floor was checked BEFORE any family dispatch, so a deterministic class could
        // draw the thin panel too, on every preset that shipped this bundle — and the per-class
        // parameters never reached `derive_panel_v2`, which draws from the bundle's own
        // `PalwPanelParamsV2`, so a class admitted at 2-of-2 was still bound a 5-seat panel. A
        // ruleset field that weakened the gate and changed no behaviour.
        protocol_version: crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
        algorithm_id: crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        // **This build's own certified set** (ADR-0069 Decision 2), never an argument. The
        // sibling roots above are parameters because a caller assembling a genesis may be
        // describing a network it is not itself running; the E2E root is not that kind of value —
        // it says what THIS binary's court can play, and a caller allowed to name it could hand a
        // devnet a root claiming families no drill ever certified.
        court_e2e_root: crate::palw_e2e_adjudicability::palw_rc_court_e2e_root_v1(),
        state,
        admission,
        panel: PalwPanelParamsV2::new(PALW_V2_PANEL_SEATS, PALW_V2_PANEL_QUORUM, windows.anchor_delay)?,
        reward: PalwRewardParamsV2::new(WORKER_CARVE_PERMILLE)?,
        // The POLICY floor, deliberately not the derived one. `palw_v2_collateral_for_claim_lifetime_v1`
        // is what a bond must declare to keep a chain moving, and `verify_palw_genesis_v2` enforces
        // it on the registrations themselves; this is the smaller "a bond is not nothing" bar, and
        // keeping the two apart is what lets a fixture build a deliberately thin network to watch
        // the exposure ceiling bite.
        bond: PalwBondParamsV2::new(MIN_COLLATERAL_SOMPI, windows.withdrawal_delay)?,
        freeprompt,
        reorg_margin_daa: windows.reorg_margin,
        court: PalwCourtParamsV2::with_cost_ceilings(
            COURT_MAX_STEP_LEAVES,
            windows.court_turn_deadline,
            COURT_TERMINAL_ROUNDS,
            windows.court_max_close_bytes,
            COURT_MAX_TERMINAL_MACS,
            COURT_MAX_OPERAND_COUNT,
        )?,
        cadence_target_time_per_block_ms: PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS,
        fork_choice_version: PALW_V2_FORK_CHOICE_VERSION,
        trace_format_version: PALW_V2_TRACE_FORMAT_VERSION,
        signature_contexts_root: palw_v2_signature_contexts_root(),
        // The genesis artifact's own registrations: the liveness floor at the whole 1000‰, and
        // one bond to execute under. Without them the network cannot produce its first block —
        // admission refuses an attempt naming a bond the chain does not have — so a bundle
        // carrying none is a bundle that boots and then stalls.
        //
        // `pwu_per_inference` is the catalog's counted canonical step-leaf count, which
        // `verify_palw_genesis_v2` checks: the declaration is not the fact.
        genesis_objects: vec![crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id: base_class_id,
            artifact_root: genesis_artifact_root,
            slash_value_per_pwu: SLASH_VALUE_PER_PWU,
            pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: genesis_pwu_per_inference },
            initial_target: GENESIS_CLASS_TARGET,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        }]
        .into_iter()
        .chain(genesis_bonds.into_iter().map(|b| crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered {
            bond: b.bond,
            pubkey: b.pubkey,
            operator_pubkey: b.operator_pubkey,
            collateral: genesis_collateral,
            payout_payload: b.payout_payload,
            // **Every genesis bond judges every class this genesis registers** (ADR-0071
            // Decision 3). The registry has zero slack by construction — `seat_count + 1` bonds
            // and the draw excludes the executor — so a genesis seat that declared nothing would
            // make the first claim unpanellable and the chain would never leave `safe_frontier`
            // 0. Here that is exactly the base class; a bundle that adds the model tiers extends
            // this set at the same time it registers them, and `verify_palw_genesis_v2` refuses a
            // registry where any registered class cannot reach quorum.
            capable_classes: std::collections::BTreeSet::from([base_class_id]),
            // Genesis bonds carry no signature: `verify_palw_genesis_v2` establishes the whole
            // registry as one artifact, and there is no chain yet for a signature to be replayed
            // across. The field exists for the post-genesis path, where the carrier proves the
            // collateral and this proves the key.
            signature: Vec::new(),
        }))
        .collect(),
    };
    // Validated HERE, so the constructor cannot hand back a bundle that would refuse to boot:
    // a caller holding an `Ok` holds a bundle a node will start on.
    bundle.validate()?;
    Ok(bundle)
}

/// The devnet bundle with the fixture identities every caller in this tree used before the
/// genesis registrations became part of the ruleset. One helper rather than eight arguments
/// repeated at every call site, and `pub(crate)` so the fixture is the SAME one everywhere — two
/// fixtures for one bundle is how two tests come to disagree about a ruleset id.
#[cfg(test)]
pub(crate) fn palw_fp_devnet_bundle_for_tests(
    base_class_id: Hash64,
    class_catalog_root: Hash64,
    court_catalog_root: Hash64,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    palw_fp_devnet_bundle_v3(
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        4_096,
        Hash64::from_u64_word(0xA7),
        palw_devnet_bond_registry_v1(palw_v2_min_genesis_bonds_v1()),
    )
}

#[cfg(test)]
mod tests {

    /// **The ceiling must admit the window PLUS ONE claim, and one short is a permanent wedge.**
    ///
    /// Measured on testnet-12's first run: sized to exactly `window_bind`, the chain produced 600
    /// blocks and then held forever on `the bond's exposure ceiling leaves no room for another
    /// claim`. Admission runs against the PARENT state, so covering a span of N DAA needs room
    /// for N + 1 live claims — and the first sweep is not applied until one block later, which
    /// the chain can no longer reach.
    ///
    /// The span is the whole claim lifetime. Sized against the bind window alone, testnet-12
    /// produced 601 blocks, licensed receipts the whole way, and finalized nothing: `Final` is
    /// `WINDOW_CHALLENGE` past a license, so it could not arrive before DAA 1200, and every
    /// claim waiting for it was occupying the ceiling.
    #[test]
    fn the_collateral_admits_one_more_claim_than_a_claim_can_live() {
        let pwu_per_inference = 7_900;
        let collateral = palw_v2_collateral_for_claim_lifetime_v1(pwu_per_inference);
        let pwu = crate::palw_pwu::palw_pwu_v1(GENESIS_CLASS_TARGET, pwu_per_inference);
        let per_claim = (pwu as u128) * (SLASH_VALUE_PER_PWU as u128);
        // The ceiling admission actually enforces: collateral × ratio / 1000.
        let ceiling = (collateral as u128) * (MAX_EXPOSURE_RATIO_PERMILLE as u128) / 1000;
        let admits = ceiling / per_claim;
        assert!(
            admits > MAX_CLAIM_EXPOSURE_DAA as u128,
            "the ceiling admits {admits} concurrent claims; a claim can hold exposure for {} DAA",
            MAX_CLAIM_EXPOSURE_DAA
        );
        // And not wastefully more — this is a floor with a stated reason, not a round number.
        assert!(admits < MAX_CLAIM_EXPOSURE_DAA as u128 + 8, "the derivation should not quietly inflate: admits {admits}");
    }

    /// The invariant testnet-12 violated, stated at the layer that owns it: a chain must be able
    /// to produce its way to the first `Final` without production stopping first.
    ///
    /// Nothing is released before then — a claim's exposure stands until it finalizes or voids,
    /// and no claim can finalize before `WINDOW_CHALLENGE` — so the live claim count on a young
    /// chain IS the block height. If the ceiling admits fewer claims than that, the chain reaches
    /// the ceiling with zero finalized claims, and it is a permanent stop: the only thing that
    /// releases room is a claim finalizing, the only thing that advances DAA is a block, and the
    /// ceiling refuses the block. Every unit test passed while the fleet did exactly this.
    #[test]
    fn honest_production_reaches_the_first_final_before_it_reaches_the_ceiling() {
        let pwu_per_inference = 7_900;
        let collateral = palw_v2_collateral_for_claim_lifetime_v1(pwu_per_inference);
        let pwu = crate::palw_pwu::palw_pwu_v1(GENESIS_CLASS_TARGET, pwu_per_inference);
        // Mirrors `palw_admission_v2` step 8 exactly: collateral × ratio / 1000, floored.
        let per_claim = (pwu as u128) * (SLASH_VALUE_PER_PWU as u128);
        let ceiling = (collateral as u128) * (MAX_EXPOSURE_RATIO_PERMILLE as u128) / 1000;
        let admits = ceiling / per_claim;

        // The earliest `Final` reachable at all: bound and licensed in the block that made the
        // claim, then the whole challenge window. One attempt claim per block.
        assert!(
            admits > WINDOW_CHALLENGE as u128,
            "the ceiling admits {admits} concurrent claims, but no claim can finalize before block              {WINDOW_CHALLENGE} — production stops before the first release, and stays stopped"
        );
        // And the worst honest case: bound late, licensed late, taken to court.
        assert!(
            admits >= MAX_CLAIM_EXPOSURE_DAA as u128,
            "the ceiling admits {admits}, a contested claim can hold its room for {} blocks",
            MAX_CLAIM_EXPOSURE_DAA
        );
    }

    /// The span the collateral covers must be the span the state machine can actually hold
    /// exposure across — every window on the path from `Provisional` to a terminal phase. If a
    /// window is added to the lattice and left out of this sum, the ceiling silently becomes
    /// reachable by honest production again.
    #[test]
    fn the_covered_span_is_every_window_a_live_claim_can_wait_through() {
        // **Not a restatement of the definition — a WALK of the state machine's own arms.**
        //
        // The previous body asserted `MAX_CLAIM_EXPOSURE_DAA == <its own definition>`, which is
        // true however wrong the definition is: it could not catch a window entering the lattice,
        // which is exactly what its name promises and exactly what happened when the redraw
        // landed. This walks the longest path a claim can take instead, in the order
        // `sweep_deadlines` takes it, and demands the constant cover it.
        let accepted = 0u64;
        let first_bind_deadline = accepted + WINDOW_BIND; // PanelBound must arrive by here
        let receipt_timeout = first_bind_deadline + WINDOW_RECEIPT; // no quorum -> swept here
        let rebound = receipt_timeout; // the redraw dates the SECOND panel from the sweep
        let second_bind_deadline = rebound + WINDOW_BIND;
        let licensed = second_bind_deadline + WINDOW_RECEIPT; // receipts under the second panel
        let challenge_deadline = licensed + WINDOW_CHALLENGE;
        let court_terminal = challenge_deadline + WINDOW_COURT; // an opened court runs its window
        let longest_live_span = court_terminal + FP_ABANDON_HOLD; // and a free-prompt hold after
        assert!(
            MAX_CLAIM_EXPOSURE_DAA >= longest_live_span,
            "a claim can stay live for {longest_live_span} DAA but the exposure span funds only {MAX_CLAIM_EXPOSURE_DAA} —              a bond sized from this admits fewer concurrent claims than it can accumulate, which is the block-600 wedge"
        );
        // The one that made the difference originally: a licensed claim waits out the challenge
        // window while still holding its reservation, so the bind window alone is not the span.
        assert!(MAX_CLAIM_EXPOSURE_DAA > WINDOW_BIND + WINDOW_CHALLENGE);

        // **ADR-0062 SA-6: the data-availability phase, walked the same way.**
        //
        // A `DefaultDisputed` claim holds its reservation like every other live phase, so the
        // question this test asks — "can the exposure span fund the longest life a claim can
        // have?" — has to be asked with the DA session in it. It is answered by CONTAINMENT rather
        // than by addition, and that containment is a rule the accusation arm enforces per claim:
        //
        //   * the retention a producer owes is `bind + receipt + challenge + court` from
        //     ACCEPTANCE (`palw_min_trace_retention_daa_v1`, pinned by ADR-0072 Decision 8);
        //   * an accusation is refused unless the WHOLE disclose window still fits inside it;
        //   * so every session ends at or before `accepted + retention`, and the claim's own
        //     longest path — the redrawn one walked above — has already passed that point.
        //
        // Which is why `MAX_CLAIM_EXPOSURE_DAA` does not move for this ADR, and why that is a
        // measured statement rather than an omission. If a later change lets an accusation reach
        // past retention, or restarts a window on refutation, this assert is what fails.
        let retention = WINDOW_BIND + WINDOW_RECEIPT + WINDOW_CHALLENGE + WINDOW_COURT;
        let disclose = WINDOW_CHALLENGE; // palw_da_disclose_window_daa_v1
        let latest_accusation = retention - disclose; // the last DAA an accusation is admitted at
        let da_session_ends = latest_accusation + disclose;
        assert_eq!(da_session_ends, retention, "the DA session's own bound IS the retention obligation");
        assert!(
            da_session_ends <= longest_live_span,
            "a DA session can outlive the claim path this span was sized on: {da_session_ends} > {longest_live_span}"
        );
        assert!(
            MAX_CLAIM_EXPOSURE_DAA >= longest_live_span.max(da_session_ends),
            "the exposure span must fund the longest life a claim can have, DA session included"
        );
    }
    use super::*;
    use crate::palw_mode_v2::{PalwConsensusMode, palw_ruleset_id_v2};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bundle() -> PalwConsensusParamsV2 {
        palw_fp_devnet_bundle_v3(
            h64(0xBA5E),
            h64(0xCA7),
            h64(0xC0757),
            4_096,
            h64(0xA7),
            palw_devnet_bond_registry_v1(palw_v2_min_genesis_bonds_v1()),
        )
        .expect("the devnet bundle validates")
    }

    /// **The interlock exists.** Every Decision-1 and ADR-0044 startup invariant holds on one
    /// concrete parameter set — the claim this module was written to make checkable.
    /// **The RC ships the ceilings its own gate demands** (ADR-0049 Decision C).
    ///
    /// `assemble_palw_rc_identity_v2` gate 6 refuses an identity carrying anything else, so this is
    /// the other half of that gate: a bundle that could not pass it would make the network
    /// unmintable, and the failure would surface at genesis rather than here. Written against the
    /// constants rather than against literals, because a literal here would be a second opinion
    /// about a number whose whole point is that there is only one.
    #[test]
    fn the_bundle_carries_the_rc_cost_ceilings() {
        use crate::palw_class_admission_v2::{
            PALW_RC_COURT_MAX_CLOSE_BYTES, PALW_RC_COURT_MAX_OPERAND_COUNT, PALW_RC_COURT_MAX_STEP_LEAF_COUNT,
            PALW_RC_COURT_MAX_TERMINAL_MACS,
        };
        let b = bundle();
        assert_eq!(b.court.max_step_leaf_count(), PALW_RC_COURT_MAX_STEP_LEAF_COUNT, "gate 5's value");
        assert_eq!(b.court.max_close_bytes(), PALW_RC_COURT_MAX_CLOSE_BYTES, "gate 6's, and the one derived from carriage");
        assert_eq!(b.court.max_terminal_macs(), PALW_RC_COURT_MAX_TERMINAL_MACS);
        assert_eq!(b.court.max_operand_count(), PALW_RC_COURT_MAX_OPERAND_COUNT);
    }

    /// **Two window sets, one lattice** (ADR-0077 Decision 7): the RC set is the bundle this
    /// module always built, and the devnet set validates every interlock the RC set does — the
    /// beacon binds inside the bind window, the draw matures past the reorg margin, and the court
    /// window holds a 2^32 ladder at its own move clock.
    #[test]
    fn the_devnet_windows_validate_and_the_rc_windows_are_the_bundles_own() {
        let rc = bundle();
        let via_windows = palw_fp_bundle_with_windows_v3(
            h64(0xBA5E),
            h64(0xCA7),
            h64(0xC0757),
            4_096,
            h64(0xA7),
            palw_devnet_bond_registry_v1(palw_v2_min_genesis_bonds_v1()),
            &PALW_RC_WINDOWS_V1,
        )
        .expect("the RC windows validate");
        assert_eq!(palw_ruleset_id_v2(&rc), palw_ruleset_id_v2(&via_windows), "the RC set IS the bundle");
        let devnet = palw_fp_bundle_with_windows_v3(
            h64(0xBA5E),
            h64(0xCA7),
            h64(0xC0757),
            4_096,
            h64(0xA7),
            palw_devnet_bond_registry_v1(palw_v2_min_genesis_bonds_v1()),
            &PALW_DEVNET_WINDOWS_V1,
        )
        .expect("the devnet windows validate");
        assert_ne!(palw_ruleset_id_v2(&rc), palw_ruleset_id_v2(&devnet), "a window set is a ruleset");
        let w = PALW_DEVNET_WINDOWS_V1;
        assert!(w.anchor_delay + w.max_beacon_gap < w.window_bind);
        assert!(w.receipt_maturity >= w.reorg_margin);
        assert!(w.claim_retirement > w.fp_abandon_hold);
        // Decision 12's ladder, at the shipped per-move clock and with ADR-0080's assembly reserve:
        // `(2 × 32 + 2) × turn + reserve < court`, the reserve being THIS set's.
        assert!(
            crate::palw_context_ladder::palw_ladder_fits_window_court_v1(
                w.window_court,
                crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
                2,
                w.court_turn_deadline,
                w.max_close_chunks()
            ),
            "the devnet window does not hold the 2^32 ladder at its own clock and its own close ceiling"
        );
        assert!(w.withdrawal_delay > w.window_bind + w.window_receipt + w.window_challenge + w.window_court + w.reorg_margin);
        assert_eq!(devnet.state.window_bind(), 40);
        assert_eq!(devnet.court.turn_deadline_daa(), 4);

        // **The two sets carry two close ceilings, and the bundles they build carry them too.**
        // This is the field's whole job: the derived chunk count is what the assembly reserve, and
        // therefore the move clock each window can afford, is a function of. The devnet's ceiling is
        // the pre-ADR-0080 80 KiB, which frames to a single carrier; the RC's is the 27-carrier one.
        assert_eq!(PALW_DEVNET_WINDOWS_V1.court_max_close_bytes, DEVNET_COURT_MAX_CLOSE_BYTES);
        assert_eq!(DEVNET_COURT_MAX_CLOSE_BYTES, 81_920);
        assert_eq!(devnet.court.max_close_bytes(), DEVNET_COURT_MAX_CLOSE_BYTES);
        assert_eq!(devnet.court.max_close_chunks(), 1, "setting the bytes is what moves the count");
        assert_eq!(devnet.court.max_close_chunks(), PALW_DEVNET_WINDOWS_V1.max_close_chunks());
        assert_eq!(rc.court.max_close_bytes(), crate::palw_class_admission_v2::PALW_RC_COURT_MAX_CLOSE_BYTES);
        assert_eq!(rc.court.max_close_chunks(), 27);
        assert_eq!(rc.court.max_close_chunks(), PALW_RC_WINDOWS_V1.max_close_chunks());
        assert_ne!(devnet.court.max_close_chunks(), rc.court.max_close_chunks(), "a close ceiling is a ruleset");
    }

    /// **Audit D M-5: the close ceiling a genesis carries, as ADR-0082 Decision 6's derivation.**
    ///
    /// Decision 6 makes the count "a derivation, evaluated at genesis over the rows the genesis set
    /// registers". The derivation was implemented and had three test callers; the ceiling a lattice
    /// carried was a typed constant, so a genesis registering the graph-v5 pair would have carried
    /// the graph-v2/v3 pair's number — a close bound about nine times looser than the transport,
    /// with `palw_close_assembly_daa_v1(27) = 216` DAA of reserve charged to every window for
    /// carriers nothing files.
    ///
    /// **This test does not move a ceiling and must not**: `court_max_close_bytes` is inside
    /// `palw_ruleset_id_v2`, so what a shipped set carries is a genesis decision. It REPORTS both
    /// numbers, and pins the one that already agrees.
    #[test]
    fn the_genesis_close_ceiling_is_the_derivation_over_the_set_it_registers() {
        use crate::palw_context_ladder::{PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, PALW_LADDER_FAMILIES_V1, PALW_LADDER_FAMILIES_V5};

        // ---- t11 / the RC set: the graph-v2/v3 pair at ADR-0077 Decision 13's first row, which
        // is what `PALW_RC_WINDOWS_V1.court_max_close_bytes` was chosen for.
        let rc_derived =
            palw_derived_court_max_close_bytes_v1(&PALW_LADDER_FAMILIES_V1, &[512], None, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
                .expect("the RC set prices");
        println!(
            "t11 (graph-v2/v3 @ 512): derived {rc_derived} bytes = {} chunks; shipped {} bytes = {} chunks",
            crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(rc_derived),
            PALW_RC_WINDOWS_V1.court_max_close_bytes,
            PALW_RC_WINDOWS_V1.max_close_chunks()
        );
        assert_eq!(
            rc_derived, PALW_RC_WINDOWS_V1.court_max_close_bytes,
            "the RC ceiling stopped being the derivation over the set it was chosen for"
        );
        // The builder is the production door, and it must reproduce that set byte for byte.
        assert_eq!(
            PALW_RC_WINDOWS_V1
                .with_derived_court_close_v1(&PALW_LADDER_FAMILIES_V1, &[512], None, PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
                .expect("the RC set prices"),
            PALW_RC_WINDOWS_V1,
            "taking the derivation must not move the RC lattice"
        );

        // ---- the devnet's own set: the rows this build actually ships and registers, measured
        // through the same walk. REPORTED, not asserted equal — the devnet's 81,920 was taken when
        // one close was one transaction, and the derivation rounds to the carrier grid.
        let rows = crate::palw_court_deadline::palw_shipped_court_rows_v1().expect("the shipped rows project");
        let widest = rows
            .iter()
            .filter_map(|row| crate::palw_class_admission_v2::derive_court_cost_v1(&row.profile).ok())
            .map(|cost| cost.max_close_bytes)
            .max()
            .expect("at least one shipped row prices");
        let devnet_derived = crate::palw_mode_v2::palw_close_bytes_for_chunks_v1(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(widest));
        println!(
            "devnet (the shipped rows): widest close {widest} bytes = {} chunk(s) -> derived ceiling {devnet_derived} bytes; \
             shipped {} bytes = {} chunk(s)",
            crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(widest),
            PALW_DEVNET_WINDOWS_V1.court_max_close_bytes,
            PALW_DEVNET_WINDOWS_V1.max_close_chunks()
        );
        assert_eq!(
            crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(devnet_derived),
            PALW_DEVNET_WINDOWS_V1.max_close_chunks(),
            "the devnet's CHUNK count is what the reserve and the move clock are functions of, and it must agree"
        );

        // ---- and the set ADR-0082 is actually for, so the number a relaunch would take is on the
        // record rather than re-derived by hand at the genesis card.
        let court = crate::palw_class_admission_v2::PalwKaryCourtV1 {
            dissection_arity: 16,
            prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            window_court_daa: PALW_RC_WINDOWS_V1.window_court,
        };
        let v5 = palw_derived_court_max_close_bytes_v1(
            &PALW_LADDER_FAMILIES_V5,
            &[512],
            Some(court),
            PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        )
        .expect("the graph-v5 set prices");
        println!(
            "graph-v5 @ 512 under the dissection court: derived {v5} bytes = {} chunks",
            crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(v5)
        );
        assert!(v5 < rc_derived, "the whole point of ADR-0082 is that the v5 set's close is smaller: {v5} against {rc_derived}");
    }

    /// **Every class row this build ships fits the devnet court's ONE-carrier close** — the claim
    /// [`DEVNET_COURT_MAX_CLOSE_BYTES`] rests on, measured through the shipped cost derivation
    /// rather than asserted in a comment.
    ///
    /// The devnet ceiling is narrower than the RC's by design, and a narrower ceiling ADMITS LESS:
    /// under the shipped `2^22` ladder `the_widest_context_each_family_admits` measures the dense
    /// family at `n_ctx` 21 under 80 KiB against 39 under 2,250,000, and the hybrid at 8 against 12.
    /// So "devnet needs no second carrier" is a claim about the rows that exist, and this is the
    /// test that fails the day one of them outgrows it — which is the day the devnet lattice has a
    /// decision to make about its window, not a default to inherit. The margin at the time of
    /// writing is 3,232 bytes on the dense A16 row (78,688 of 81,920), which is thin enough that
    /// the failure is a real possibility rather than a formality.
    ///
    /// Run it with `-- --nocapture` for the per-row bytes.
    #[test]
    fn the_devnet_close_ceiling_admits_every_shipped_row() {
        let rows = crate::palw_court_deadline::palw_shipped_court_rows_v1().expect("the shipped rows project");
        assert!(!rows.is_empty(), "no shipped rows — this check verified nothing");
        for row in &rows {
            // Under the court this ruleset plays: the devnet ARMS `palw_kary_court` at genesis, and
            // a fused row priced at the binary court is priced at a court the devnet never deals
            // (`palw_shipped_row_court_cost_v1` — 42 carriers against one on the graph-v5 row).
            let cost = crate::palw_court_deadline::palw_shipped_row_court_cost_v1(
                &row.profile,
                COURT_MAX_STEP_LEAVES,
                crate::palw_mode_v2::PALW_COURT_BINARY_ARITY_V1,
                PALW_DEVNET_WINDOWS_V1.window_court,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("a shipped row prices");
            let chunks = crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes);
            println!("{}: n_ctx {} closes at {} bytes = {chunks} carrier(s)", row.cost.row, row.profile.n_ctx, cost.max_close_bytes);
            assert!(
                cost.max_close_bytes <= DEVNET_COURT_MAX_CLOSE_BYTES,
                "{}: closes at {} bytes against the devnet ceiling {DEVNET_COURT_MAX_CLOSE_BYTES}",
                row.cost.row,
                cost.max_close_bytes
            );
            assert_eq!(
                chunks,
                PALW_DEVNET_WINDOWS_V1.max_close_chunks(),
                "{}: needs {chunks} carriers and the devnet court pays for {}",
                row.cost.row,
                PALW_DEVNET_WINDOWS_V1.max_close_chunks()
            );
        }
    }

    #[test]
    fn the_devnet_bundle_boots() {
        let b = bundle();
        b.validate().expect("validated at construction, and again here");

        // Spot-check the interlocks by hand, so a future edit that satisfies `validate()` by
        // accident still has to face the arithmetic that made these numbers a set.
        let s = &b.state;
        assert!(b.panel.anchor_delay() + b.freeprompt.max_beacon_gap_daa() < s.window_bind(), "a late beacon still binds");
        let worst_case = b.court.worst_case_duration_daa().expect("the court shape has a finite worst case");
        assert!(s.window_court() > worst_case, "an honest prosecution fits its window");
        assert!(b.freeprompt.receipt_maturity_daa() >= b.reorg_margin_daa, "the draw beacon sits past the reorgable fringe");
        // The redrawn lattice, matching `validate_ruleset_shape` — see the comment there.
        let liability = 2 * (s.window_bind() + s.window_receipt()) + s.window_challenge() + s.window_court() + b.reorg_margin_daa;
        assert!(b.bond.withdrawal_delay_daa() > liability, "a bond cannot leave before its fraud is provable");
        // **The genesis/post-genesis asymmetry, and why it stays.**
        //
        // Genesis demands a bond fund a whole bind window of concurrent claims
        // (`palw_v2_collateral_for_claim_lifetime_v1`); post-genesis registration checks only
        // `min_collateral_sompi`. That reads like a hole and is not one: a genesis bond is the
        // ONLY producer, so exhausting its exposure headroom stops the DAA, and nothing then
        // releases the claims that would give the headroom back — the chain wedges. A bond joining
        // a RUNNING chain has no such property. Other bonds keep producing, DAA advances, its
        // claims mature and release; a thin bond is limited, and limited only for itself.
        //
        // What the floor must still do is fund at least one claim, or registration admits bonds
        // that can never produce. It does, by construction: the derived figure takes
        // `.max(MIN_COLLATERAL_SOMPI)`, so the floor is a lower bound on a number already sized
        // against the reserve.
        assert!(
            // The smallest free-prompt claim is one quantum of one leaf (ADR-0074 Decision 5);
            // the derivation must fund even that from the floor.
            palw_v2_collateral_for_claim_lifetime_v1(1) >= s.min_collateral_sompi(),
            "the derived lifetime collateral is what genesis demands, and it never falls below the floor"
        );

        // Both lanes are producible (`accepts_algo_id` takes algo-7), so both must hold a share —
        // see `ATTEMPT_SHARE_PERMILLE` for what each end of the range does to the retarget.
        let split = s.fp_attempt_share_permille();
        assert!(
            split > 0 && split < crate::palw_class_daa::PALW_CLASS_SHARE_DENOMINATOR,
            "a producible lane holds a real cadence share: {split}"
        );
    }

    /// The bundle is a *ruleset*: it fingerprints, it demands the V2 attempt id, and it accepts
    /// exactly the two lanes' ids.
    #[test]
    fn the_devnet_bundle_is_one_checkable_ruleset() {
        let b = bundle();
        let id = palw_ruleset_id_v2(&b);
        assert_eq!(id, palw_ruleset_id_v2(&bundle()), "the fingerprint is a pure function of the values");
        assert_ne!(id, palw_ruleset_id_v2(&palw_fp_devnet_bundle_for_tests(h64(0xBA5E), h64(0xCA8), h64(0xC0757)).unwrap()));
        assert_eq!(
            PalwConsensusMode::ConsensusV2(b.clone()).required_algo_id(),
            Some(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2)
        );
        assert!(b.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2));
        assert!(b.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3));
        assert!(!b.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH));
    }

    /// A params set carrying this bundle passes the config gate — the mixed-lineage rule
    /// included — so the first real preset assembly is a value edit, not a discovery.
    #[test]
    fn a_params_set_carrying_it_passes_the_config_gate() {
        use crate::config::params::{DEVNET_PARAMS, SIMNET_PARAMS};
        let mut v2 = SIMNET_PARAMS.clone();
        v2.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle());
        // ADR-0038 Decision H: a ConsensusV2 network runs the frozen cadence, and the bundle's
        // own `cadence_target_time_per_block_ms` must equal the network's — the two are one fact
        // in two places, which is what the H2 fix made checkable.
        // The whole cadence AND the bundle-derived depths, through the same two spellings the
        // real assembly uses. Patching `target_time_per_block` alone here used to pass, which is
        // exactly how a hand-built set can satisfy the gate while running 10 bps DAG parameters
        // and 10 bps DNS windows at 120 s.
        let v2 = v2.with_two_minute_cadence().with_palw_v2_depths(&bundle());
        v2.validate_palw_v2().expect("a pure V2 params set carrying the devnet bundle validates");

        // …and the mixed-lineage refusal still bites. DEVNET used to be the V1 PALW PoW preset
        // and carried the mixed lineage by itself; since ADR-0068 Phase 1 its base ships no V1
        // PoW (the shipped devnet IS a V2 network), so the V1 half is stated explicitly — the
        // rule under test is "a V2 mode beside ANY V1 PALW proof-of-work is refused", not a fact
        // about which preset happens to ship V1 today.
        let mut mixed = DEVNET_PARAMS.clone();
        mixed.pow_palw_activation = crate::config::params::ForkActivation::always();
        mixed.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle());
        let mixed = mixed.with_two_minute_cadence().with_palw_v2_depths(&bundle());
        assert!(mixed.validate_palw_v2().is_err());
    }

    /// The economics the numbers encode, asserted rather than assumed: an ordinary chat job earns
    /// several draws (variance smoothing), a tiny job earns none but still certifies, and the
    /// per-receipt cap bounds a huge one.
    /// **The price is the class's own canonical job, in eighths** (ADR-0074 Decision 5). The floor's
    /// canonical job is 7,708 leaves (the genesis rule pins `pwu_per_inference` to it), so its
    /// quantum is 963 leaves: a free-prompt job the size of the canonical one is eight draws, a
    /// job of a few thousand leaves a handful, a sub-quantum job none, an enormous one the cap —
    /// and `pwu = quanta × quantum` is in leaves, the attempt lane's unit, on every class.
    #[test]
    fn the_pricing_is_the_classs_own_canonical_job_in_eighths() {
        use crate::palw_freeprompt_v3::{fp_class_quantum_leaves_v1, fp_quanta_v3};
        let b = bundle();
        let per_job = b.freeprompt.quanta_per_canonical_job();
        let cap = b.freeprompt.max_quanta_per_receipt();
        assert_eq!(per_job, FP_QUANTA_PER_CANONICAL_JOB);
        assert_eq!(b.state.fp_quanta_per_canonical_job(), per_job, "the state params carry the same price the bundle declares");
        assert_eq!(b.state.fp_max_quanta_per_receipt(), cap);

        const FLOOR_LEAVES: u64 = 7_708;
        let quantum = fp_class_quantum_leaves_v1(FLOOR_LEAVES, per_job);
        assert_eq!(quantum, 963);
        assert_eq!(fp_quanta_v3(FLOOR_LEAVES, quantum, cap), 8, "one canonical job is eight draws");
        assert_eq!(fp_quanta_v3(7_000, quantum, cap), 7, "a floor free-prompt job of a few thousand leaves draws a handful");
        assert_eq!(fp_quanta_v3(500, quantum, cap), 0, "sub-quantum draws nothing");
        assert_eq!(fp_quanta_v3(10_000_000, quantum, cap), cap, "the largest job is capped, not unbounded");
        // A class whose canonical job is smaller than the divisor still has a one-leaf quantum.
        assert_eq!(fp_class_quantum_leaves_v1(3, per_job), 1);

        let (quanta, pwu) = b.freeprompt.derive_quanta_and_pwu(7_000, FLOOR_LEAVES).expect("a floor job derives");
        assert_eq!((quanta, pwu), (7, 7 * 963));
        assert!(pwu % (quanta as u64) == 0 && pwu / (quanta as u64) > 0, "uniform quanta, as the state machine demands");
        assert!(b.freeprompt.derive_quanta_and_pwu(500, FLOOR_LEAVES).is_none(), "a sub-quantum job never enters the chain");
        assert!(pwu <= 7_000, "the price never exceeds the leaves that were run");
    }
}
