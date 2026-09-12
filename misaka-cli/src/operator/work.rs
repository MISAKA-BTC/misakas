//! **One work, one id, one state machine across both lanes** — ADR-0122 Decision 2.
//!
//! A piece of work is followed from the request (the prompt lane) or the won draw (the block lane)
//! to the reward, through the same named stages whichever lane it rides:
//!
//! ```text
//! RECEIVED → EXECUTING → EXECUTED → COMMITTED → SUBMITTING → SUBMITTED → ON_CHAIN
//!          → WAITING_RECEIPTS → QUORUM_REACHED → ACCEPTED → REWARD_PENDING → REWARDED
//! ```
//!
//! and it can end short of that in a named way (`NOT_COMMITTED`, `EXPIRED`, `GAVE_UP`, `DROPPED`,
//! `VOIDED`, `NO_REWARD`) or pause (`DISPUTED`). **Only `ACCEPTED` — the chain's `final` — counts
//! as mined.** A computed answer is `EXECUTED`, a carrier on the wire is `SUBMITTED`, and neither is
//! called mining anywhere this module's names are printed.
//!
//! Everything here is a pure function of what the CLI has read — the outbox's files, the rail's
//! watch state, the node's mempool, `getPalwFreePromptClaim` — and of the network's windows, so
//! the tests can pin every transition without a node. The one date that is an estimate (a block
//! lane escrow's payout, which the claim read does not carry) says so where it is printed.

use crate::palw_claim::{OutboxRow, OutboxState, Windows};
use kaspa_rpc_core::GetPalwFreePromptClaimResponse;
use serde::Serialize;

/// Which lane a work rides. The block lane is the attempt lane inside `kaspad`; the prompt lane is
/// the free-prompt lane through the gateway, the worker and the rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Lane {
    Block,
    Prompt,
}

impl Lane {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Lane::Block => "block",
            Lane::Prompt => "prompt",
        }
    }

    /// The lane's success path, in order. The block lane has no request and no commitment: a work
    /// exists from the won draw, and the block is its submission.
    pub(crate) fn path(self) -> &'static [WorkState] {
        use WorkState::*;
        match self {
            Lane::Prompt => &[
                Received,
                Executing,
                Executed,
                Committed,
                Submitting,
                Submitted,
                OnChain,
                WaitingReceipts,
                QuorumReached,
                Accepted,
                RewardPending,
                Rewarded,
            ],
            Lane::Block => &[Executed, Submitted, OnChain, WaitingReceipts, QuorumReached, Accepted, RewardPending, Rewarded],
        }
    }
}

/// Every state a work can be in. The success path first, then the pause, then the ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum WorkState {
    Received,
    Executing,
    Executed,
    Committed,
    Submitting,
    Submitted,
    OnChain,
    WaitingReceipts,
    QuorumReached,
    Accepted,
    RewardPending,
    Rewarded,
    /// A court is open, or a data-availability accusation froze the claim. It resumes or voids.
    Disputed,
    NotCommitted,
    Expired,
    GaveUp,
    Dropped,
    Voided,
    /// `final`, and no quantum won at the draw (or the use window closed unspent). The prompt lane
    /// pays by lottery (ADR-0058): this is not a failure of the work.
    NoReward,
    /// A phase or a file this build cannot name. Nothing is inferred from it.
    Unknown,
}

/// What a state means for the operator's counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Outcome {
    /// Still on its way to `final`.
    InFlight,
    /// A court or an accusation holds it.
    Paused,
    /// `final`: counted as mined, whatever its reward did.
    Mined,
    /// Ended short of `final`.
    Failed,
    Unknown,
}

impl WorkState {
    pub(crate) fn name(self) -> &'static str {
        use WorkState::*;
        match self {
            Received => "RECEIVED",
            Executing => "EXECUTING",
            Executed => "EXECUTED",
            Committed => "COMMITTED",
            Submitting => "SUBMITTING",
            Submitted => "SUBMITTED",
            OnChain => "ON_CHAIN",
            WaitingReceipts => "WAITING_RECEIPTS",
            QuorumReached => "QUORUM_REACHED",
            Accepted => "ACCEPTED",
            RewardPending => "REWARD_PENDING",
            Rewarded => "REWARDED",
            Disputed => "DISPUTED",
            NotCommitted => "NOT_COMMITTED",
            Expired => "EXPIRED",
            GaveUp => "GAVE_UP",
            Dropped => "DROPPED",
            Voided => "VOIDED",
            NoReward => "NO_REWARD",
            Unknown => "UNKNOWN",
        }
    }

    pub(crate) fn outcome(self) -> Outcome {
        use WorkState::*;
        match self {
            Received | Executing | Executed | Committed | Submitting | Submitted | OnChain | WaitingReceipts | QuorumReached => {
                Outcome::InFlight
            }
            Disputed => Outcome::Paused,
            Accepted | RewardPending | Rewarded | NoReward => Outcome::Mined,
            NotCommitted | Expired | GaveUp | Dropped | Voided => Outcome::Failed,
            Unknown => Outcome::Unknown,
        }
    }

    /// "Computed": the work produced an answer (or, on the block lane, won its draw). Every state
    /// from `EXECUTED` on except the ones that ended before a run finished.
    pub(crate) fn was_computed(self) -> bool {
        !matches!(self, WorkState::Received | WorkState::Executing | WorkState::Unknown)
    }

    /// "Submitted": the work left this host for the chain.
    pub(crate) fn was_submitted(self) -> bool {
        use WorkState::*;
        matches!(
            self,
            Submitted
                | OnChain
                | WaitingReceipts
                | QuorumReached
                | Disputed
                | Accepted
                | RewardPending
                | Rewarded
                | NoReward
                | Dropped
                | Voided
        )
    }
}

/// A state with the one line a table prints beside it and the next date that matters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Reading {
    pub(crate) state: WorkState,
    /// One line for a table row: the fact that makes this state what it is.
    pub(crate) detail: String,
    /// The DAA at which this state next changes by itself (a window closes), when there is one.
    pub(crate) deadline_daa: Option<u64>,
    /// True when `deadline_daa` or `detail` rests on an estimate rather than a read.
    pub(crate) estimated: bool,
}

impl Reading {
    fn new(state: WorkState, detail: impl Into<String>) -> Self {
        Self { state, detail: detail.into(), deadline_daa: None, estimated: false }
    }

    fn by(mut self, daa: u64) -> Self {
        self.deadline_daa = Some(daa);
        self
    }

    fn estimated(mut self) -> Self {
        self.estimated = true;
        self
    }
}

/// The rail's own verdict on a job it watched (`rail-watch-state.json`): `on-chain`, `dropped`,
/// `expired`, `retired` or `gave-up`, or the submission it is still waiting on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RailVerdict {
    pub(crate) settled: Option<String>,
    /// The DAA the carrier was submitted at, while the rail waits for it to land.
    pub(crate) awaiting_since_daa: Option<u64>,
}

/// How long a carrier that left the mempool may take to become a claim before the rail calls it
/// dropped (`misaka-palw-fp-rail`'s `LANDING_GRACE_DAA`; the rail is a binary this crate does not
/// link, so the one number is spelled twice).
pub(crate) const LANDING_GRACE_DAA: u64 = 30;

/// Where a work stands, from everything read about it. `chain` is `getPalwFreePromptClaim`'s
/// answer when the chain was asked; `in_mempool` whether the node's mempool holds its carrier.
///
/// The chain outranks the files: once a claim exists the outbox has nothing more to say about it.
/// A chain that does not hold the claim is read with the rail's verdict and the mempool, because
/// "not yet in a block" and "dropped" are opposite instructions — wait, or stop paying fees into a
/// full bond.
pub(crate) fn classify(
    lane: Lane,
    outbox: Option<&OutboxRow>,
    rail: Option<&RailVerdict>,
    chain: Option<&GetPalwFreePromptClaimResponse>,
    in_mempool: bool,
    windows: Option<&Windows>,
    coinbase_maturity: u64,
    now: u64,
) -> Reading {
    if let Some(r) = chain.filter(|r| r.found) {
        return from_chain(lane, r, windows, coinbase_maturity, now);
    }
    if in_mempool {
        return Reading::new(WorkState::Submitted, "the carrier is in the node's mempool, not in a block yet");
    }
    match lane {
        Lane::Prompt => from_outbox(outbox, rail, chain.is_some(), now),
        Lane::Block => match chain {
            // The block was found in the log and its claim is not in the state: the block never
            // became a chain or blue-merged block, or the claim ended and was retired.
            Some(_) => Reading::new(
                WorkState::Dropped,
                match windows {
                    Some(w) if w.claim_retirement > 0 => format!(
                        "no claim for this block: it was not accepted as a chain or blue-merged block, or its claim ended more \
                         than {} DAA ago and was retired",
                        w.claim_retirement
                    ),
                    _ => "no claim for this block: it was not accepted as a chain or blue-merged block, or its claim was retired"
                        .to_string(),
                },
            ),
            None => Reading::new(WorkState::Submitted, "produced; the chain has not been asked about it"),
        },
    }
}

/// A prompt-lane job the chain does not hold (or was not asked about), from its files.
fn from_outbox(outbox: Option<&OutboxRow>, rail: Option<&RailVerdict>, chain_asked: bool, now: u64) -> Reading {
    let Some(row) = outbox else {
        return Reading::new(WorkState::Unknown, "no outbox record and no claim on the chain");
    };
    match &row.state {
        OutboxState::NotCommitted => {
            let why = row.not_committed_because.as_deref().filter(|w| !w.is_empty()).unwrap_or("the gateway recorded no reason");
            Reading::new(WorkState::NotCommitted, format!("answered, not committed: {why}"))
        }
        OutboxState::CommittedNotSubmitted { .. } => {
            let reading = Reading::new(WorkState::Committed, "committed, waiting for the rail to submit it");
            match row.commit_by_anchor_daa {
                Some(by) if now > by => {
                    Reading::new(WorkState::Expired, format!("its anchor lapsed at DAA {by} before anything submitted it"))
                }
                Some(by) => reading.by(by),
                None => reading,
            }
        }
        OutboxState::SignedNotSubmitted { .. } => {
            let reading = Reading::new(WorkState::Submitting, "signed by the rail, not submitted");
            match row.commit_by_anchor_daa {
                Some(by) if now > by => {
                    Reading::new(WorkState::Expired, format!("its anchor lapsed at DAA {by} before the carrier was sent"))
                }
                Some(by) => reading.by(by),
                None => reading,
            }
        }
        OutboxState::Expired { .. } => Reading::new(
            WorkState::Expired,
            match row.commit_by_anchor_daa {
                Some(by) => format!("the gateway retired it: its anchor lapsed at DAA {by} before anything submitted it"),
                None => "the gateway retired it: its anchor lapsed before anything submitted it".to_string(),
            },
        ),
        OutboxState::GaveUp { .. } => {
            Reading::new(WorkState::GaveUp, "the rail stopped retrying it (the rail's log says whether the ceiling or a refusal)")
        }
        OutboxState::Unreadable { why } => Reading::new(WorkState::Unknown, format!("unreadable: {why}")),
        OutboxState::Submitted => {
            let settled = rail.and_then(|r| r.settled.as_deref());
            let awaiting = rail.and_then(|r| r.awaiting_since_daa);
            match (settled, awaiting) {
                (Some("dropped"), _) => Reading::new(
                    WorkState::Dropped,
                    "the rail saw the carrier leave the mempool and no claim appear: the chain dropped the commitment (a full \
                     exposure ceiling, usually) or the carrier was never mined",
                ),
                (Some("retired"), _) => Reading::new(WorkState::Accepted, "the rail saw it on chain; the claim has since retired"),
                (_, Some(since)) if now < since.saturating_add(LANDING_GRACE_DAA) => Reading::new(
                    WorkState::Submitted,
                    format!("sent at DAA {since}; a carrier can take up to {LANDING_GRACE_DAA} DAA to become a claim"),
                )
                .by(since.saturating_add(LANDING_GRACE_DAA)),
                _ if chain_asked => Reading::new(
                    WorkState::Dropped,
                    "submitted, and the chain holds no claim and the mempool no carrier: dropped at the fold, never mined, or \
                     ended and retired",
                ),
                _ => Reading::new(WorkState::Submitted, "submitted; the chain has not been asked about it"),
            }
        }
    }
}

/// A claim the chain holds: its phase, dated by the network's windows.
fn from_chain(lane: Lane, r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, coinbase_maturity: u64, now: u64) -> Reading {
    let at = r.phase_daa;
    match r.phase.as_str() {
        "provisional" => {
            let accepted = r.accepted_daa;
            match w {
                // Past its first bind window and still provisional: it is on its one redraw, whose
                // base the RPC does not carry — said as what it is rather than dated wrongly.
                Some(w) if now > accepted.saturating_add(w.window_bind) => {
                    Reading::new(WorkState::OnChain, "on its one redraw: no quorum sent it back for a second panel")
                }
                Some(w) if now < accepted.saturating_add(w.anchor_delay) => Reading::new(
                    WorkState::OnChain,
                    format!("accepted at DAA {accepted}; its panel is drawn at DAA {}", accepted.saturating_add(w.anchor_delay)),
                )
                .by(accepted.saturating_add(w.window_bind)),
                Some(w) => Reading::new(
                    WorkState::OnChain,
                    format!(
                        "accepted at DAA {accepted}; waiting for a full panel to bind (by DAA {})",
                        accepted.saturating_add(w.window_bind)
                    ),
                )
                .by(accepted.saturating_add(w.window_bind)),
                None => Reading::new(WorkState::OnChain, format!("accepted at DAA {accepted}; no panel yet")),
            }
        }
        "panel_bound" => match w {
            Some(w) => Reading::new(
                WorkState::WaitingReceipts,
                format!("panel bound at DAA {at}; receipts due by DAA {}", at.saturating_add(w.window_receipt)),
            )
            .by(at.saturating_add(w.window_receipt)),
            None => Reading::new(WorkState::WaitingReceipts, format!("panel bound at DAA {at}")),
        },
        "receipt_licensed" => match w {
            Some(w) => Reading::new(
                WorkState::QuorumReached,
                format!("licensed at DAA {at}; final at DAA {} unless challenged", at.saturating_add(w.window_challenge)),
            )
            .by(at.saturating_add(w.window_challenge)),
            None => Reading::new(WorkState::QuorumReached, format!("licensed at DAA {at}")),
        },
        "default_disputed" => match w {
            Some(w) => Reading::new(
                WorkState::Disputed,
                format!("accused at DAA {at}: frozen until this executor discloses (by DAA {})", at.saturating_add(w.disclose_window)),
            )
            .by(at.saturating_add(w.disclose_window)),
            None => Reading::new(WorkState::Disputed, format!("accused at DAA {at}: frozen until this executor discloses")),
        },
        "voided" => Reading::new(WorkState::Voided, format!("{} at DAA {at} — {}", r.void_reason, who_paid(&r.void_reason))),
        "final" => rewarded(lane, r, w, coinbase_maturity, now),
        other => Reading::new(WorkState::Unknown, format!("the node reports phase '{other}', which this build cannot name")),
    }
}

/// What `getPalwClaims` (ADR-0122 §6.5) adds to a claim read: the date the chain itself computes
/// for the current phase, and where the escrow is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClaimExtra {
    pub(crate) deadline_daa: Option<u64>,
    pub(crate) escrow_sompi: u64,
    pub(crate) payout_pending_sompi: Option<u64>,
}

/// **A reading, sharpened by what the node's claim row knows** and the plain claim read does not:
/// the phase's own deadline (the chain's arithmetic, not this CLI's), and for a block-lane claim at
/// `final`, whether its escrow is still queued, already paid, or was never there.
pub(crate) fn refine(lane: Lane, mut reading: Reading, extra: &ClaimExtra) -> Reading {
    use WorkState::*;
    if matches!(reading.state, OnChain | WaitingReceipts | QuorumReached | Disputed) && extra.deadline_daa.is_some() {
        reading.deadline_daa = extra.deadline_daa;
    }
    if lane == Lane::Block && matches!(reading.state, RewardPending | Rewarded) {
        let msk = crate::operator::catalog::msk;
        if let Some(amount) = extra.payout_pending_sompi {
            return Reading {
                state: RewardPending,
                detail: format!("escrow {} queued for the next coinbase", msk(amount as u128)),
                deadline_daa: None,
                estimated: false,
            };
        }
        if extra.escrow_sompi == 0 {
            return Reading {
                state: Rewarded,
                detail: "no escrow — a merged-blue attempt is paid by its block share alone".to_string(),
                deadline_daa: None,
                estimated: false,
            };
        }
        reading.detail = format!("escrow {} paid; {}", msk(extra.escrow_sompi as u128), reading.detail);
    }
    reading
}

/// The two timeouts slash nobody; the two convictions take the collateral the claim reserved.
pub(crate) fn who_paid(void_reason: &str) -> &'static str {
    match void_reason {
        "bind_timeout" => "no panel could be seated; nothing was slashed",
        "receipt_timeout" => "the seats filed no quorum in time; nothing was slashed",
        "court_fraud" => "a court proved the run wrong; the claim's collateral was slashed",
        "producer_withholding" => "an accusation went unanswered; the claim's collateral was slashed",
        _ => "a reason this build cannot name",
    }
}

/// **`final`, and what its reward has done since.**
///
/// A prompt-lane claim is paid by lottery: its quanta are drawn at the first attempt-class chain
/// block at or after `final + receipt_maturity`, and a winning quantum must be spent as a receipt
/// block by this bond's own producer within `receipt_use_window` of that beacon. The beacon can sit
/// a little past the slot, so "the window has closed" is only said a margin after it could have.
///
/// A block-lane claim's escrow goes into `pending_payouts` at `final` and is paid by a following
/// coinbase, which matures `coinbase_maturity` later. The claim read carries neither the escrow nor
/// the payout's block, so this date is an estimate and is marked as one.
fn rewarded(lane: Lane, r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, coinbase_maturity: u64, now: u64) -> Reading {
    let at = r.phase_daa;
    match lane {
        Lane::Prompt => {
            if r.quanta == 0 {
                return Reading::new(WorkState::NoReward, format!("final at DAA {at}; the claim holds no quanta"));
            }
            if r.quanta_spent >= r.quanta {
                return Reading::new(
                    WorkState::Rewarded,
                    format!("final at DAA {at}; all {} quanta spent as receipt blocks", r.quanta),
                );
            }
            let Some(w) = w else {
                return Reading::new(
                    WorkState::RewardPending,
                    format!("final at DAA {at}; {} of {} quanta spent", r.quanta_spent, r.quanta),
                );
            };
            let slot = at.saturating_add(w.receipt_maturity);
            if now < slot {
                return Reading::new(
                    WorkState::RewardPending,
                    format!("final at DAA {at}; its {} quanta are drawn at DAA {slot}", r.quanta),
                )
                .by(slot);
            }
            // The beacon is the first attempt-class chain block at or after the slot, so the window
            // can end a little later than `slot + use_window`; a margin keeps "closed" from being said
            // early. The margin is an estimate and the reading says so.
            let closes = slot.saturating_add(w.receipt_use_window);
            let margin = w.anchor_delay.max(20);
            if now > closes.saturating_add(margin) {
                return if r.quanta_spent > 0 {
                    Reading::new(
                        WorkState::Rewarded,
                        format!("{} of {} quanta won and spent as receipt blocks", r.quanta_spent, r.quanta),
                    )
                } else {
                    Reading::new(WorkState::NoReward, format!("no quantum won at the draw (or none was spent before DAA ≈ {closes})"))
                        .estimated()
                };
            }
            Reading::new(
                WorkState::RewardPending,
                format!("drawn; {} of {} quanta spent, the window closes ≈ DAA {closes}", r.quanta_spent, r.quanta),
            )
            .by(closes)
            .estimated()
        }
        Lane::Block => {
            let spendable = at.saturating_add(1).saturating_add(coinbase_maturity);
            if now < spendable {
                Reading::new(
                    WorkState::RewardPending,
                    format!("final at DAA {at}; its escrow is paid by a following coinbase, spendable ≈ DAA {spendable}"),
                )
                .by(spendable)
                .estimated()
            } else {
                Reading::new(WorkState::Rewarded, format!("final at DAA {at}; its escrow was paid and has matured")).estimated()
            }
        }
    }
}

/// The lane's path with each stage marked against where the work stands: `✓` entered, `◐` the
/// current one, `·` still ahead. A work that ended off the path gets its end placed after the last
/// stage it entered — `void_reason` says which that was for a void — and the success stages after
/// it are blanked rather than shown as ahead, because it will never reach them. A dispute is the
/// one exception: it resumes, so what follows it is still ahead.
pub(crate) fn timeline(lane: Lane, state: WorkState, void_reason: Option<&str>) -> Vec<(WorkState, Mark)> {
    use WorkState::*;
    let path = lane.path();
    let index = |s: WorkState| path.iter().position(|p| *p == s);
    if let Some(i) = index(state) {
        return path
            .iter()
            .enumerate()
            .map(|(j, s)| {
                let mark = if j < i || (j == i && state == Rewarded) {
                    Mark::Done
                } else if j == i {
                    Mark::Current
                } else {
                    Mark::Ahead
                };
                (*s, mark)
            })
            .collect();
    }
    let left_after = match state {
        Disputed => Some(QuorumReached),
        NoReward => Some(RewardPending),
        NotCommitted => Some(Executed),
        Expired | GaveUp => Some(Committed),
        Dropped => Some(Submitted),
        Voided => Some(match void_reason {
            Some("bind_timeout") => OnChain,
            Some("court_fraud") => QuorumReached,
            _ => WaitingReceipts,
        }),
        _ => None,
    };
    let Some(i) = left_after.and_then(index) else {
        // Unknown, or an end this lane cannot reach: nothing on the path is claimed as passed.
        let mut out: Vec<(WorkState, Mark)> = path.iter().map(|s| (*s, Mark::Ahead)).collect();
        out.push((state, Mark::End));
        return out;
    };
    let resumes = state == Disputed;
    let mut out: Vec<(WorkState, Mark)> = path
        .iter()
        .enumerate()
        .map(|(j, s)| {
            let mark = if j <= i {
                Mark::Done
            } else if resumes {
                Mark::Ahead
            } else {
                Mark::NotReached
            };
            (*s, mark)
        })
        .collect();
    out.insert(i + 1, (state, if resumes { Mark::Current } else { Mark::End }));
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mark {
    Done,
    Current,
    Ahead,
    /// Where the work ended off the success path.
    End,
    /// A success stage the work will never reach, having ended before it.
    NotReached,
}

impl Mark {
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Mark::Done => "✓",
            Mark::Current => "◐",
            Mark::Ahead => "·",
            Mark::End => "✗",
            Mark::NotReached => " ",
        }
    }
}

/// A claim id shown the way a git hash is: its first 8 hex. JSON always carries the full id.
pub(crate) fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

/// **Resolve a prefix to the one id it names**, the way `git` resolves an abbreviated hash: the
/// prefix must be at least 4 hex and match exactly one candidate. Returns the candidates it
/// matched when that is not exactly one, so the caller can name them.
pub(crate) fn resolve_prefix<'a>(prefix: &str, ids: impl IntoIterator<Item = &'a str>) -> Result<&'a str, Vec<&'a str>> {
    let prefix = prefix.trim().to_ascii_lowercase();
    let prefix = prefix.strip_prefix("job:").unwrap_or(&prefix);
    if prefix.len() < 4 || !prefix.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Vec::new());
    }
    let matched: Vec<&str> = ids.into_iter().filter(|id| id.to_ascii_lowercase().starts_with(prefix)).collect();
    let mut unique = matched.clone();
    unique.sort_unstable();
    unique.dedup();
    match unique.as_slice() {
        [one] => Ok(one),
        _ => Err(unique),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: u64 = 10_000;

    /// testnet-11's windows (`windows:165-235` in the lifecycle survey), which the RC shares.
    fn t11() -> Windows {
        Windows {
            anchor_delay: 20,
            window_bind: 600,
            window_receipt: 600,
            window_challenge: 1200,
            disclose_window: 1200,
            receipt_maturity: 400,
            receipt_use_window: 600,
            fp_abandon_hold: 600,
            claim_retirement: 3000,
        }
    }

    fn claim(phase: &str, phase_daa: u64) -> GetPalwFreePromptClaimResponse {
        GetPalwFreePromptClaimResponse { found: true, phase: phase.to_string(), phase_daa, accepted_daa: 9_000, ..Default::default() }
    }

    fn prompt_final(quanta: u32, spent: u32, final_daa: u64) -> GetPalwFreePromptClaimResponse {
        GetPalwFreePromptClaimResponse { is_free_prompt: true, quanta, quanta_spent: spent, ..claim("final", final_daa) }
    }

    fn row(state: OutboxState) -> OutboxRow {
        OutboxRow {
            job_file: PathBuf::from("/outbox/fp-job-0123456789abcdef.json"),
            stem: "fp-job-0123456789abcdef".to_string(),
            modified: None,
            claim_id: Some("ab".repeat(64)),
            committed: Some(true),
            not_committed_because: None,
            commit_by_anchor_daa: Some(NOW + 500),
            work_leaves: None,
            decode_tokens_executed: None,
            submitted_txid: None,
            state,
        }
    }

    fn at(lane: Lane, c: &GetPalwFreePromptClaimResponse) -> Reading {
        classify(lane, None, None, Some(c), false, Some(&t11()), 600, NOW)
    }

    /// Every chain phase lands on its stage, dated by the network's windows.
    #[test]
    fn each_chain_phase_is_its_stage_and_its_date() {
        let r = at(Lane::Block, &claim("provisional", 9_990));
        assert_eq!(r.state, WorkState::OnChain);
        assert_eq!(r.deadline_daa, None, "past accepted + bind the claim is on its redraw, whose base the RPC lacks");
        let fresh = GetPalwFreePromptClaimResponse { accepted_daa: 9_995, ..claim("provisional", 9_995) };
        let r = at(Lane::Block, &fresh);
        assert_eq!(r.state, WorkState::OnChain);
        assert_eq!(r.deadline_daa, Some(9_995 + 600));
        assert!(r.detail.contains("drawn at DAA 10015"), "{}", r.detail);

        let r = at(Lane::Block, &claim("panel_bound", 9_800));
        assert_eq!((r.state, r.deadline_daa), (WorkState::WaitingReceipts, Some(10_400)));
        let r = at(Lane::Prompt, &claim("receipt_licensed", 9_800));
        assert_eq!((r.state, r.deadline_daa), (WorkState::QuorumReached, Some(11_000)));
        let r = at(Lane::Prompt, &claim("default_disputed", 9_900));
        assert_eq!((r.state, r.deadline_daa), (WorkState::Disputed, Some(11_100)));
        let voided = GetPalwFreePromptClaimResponse { void_reason: "receipt_timeout".into(), ..claim("voided", 9_950) };
        let r = at(Lane::Prompt, &voided);
        assert_eq!(r.state, WorkState::Voided);
        assert!(r.detail.contains("nothing was slashed"), "{}", r.detail);
        let r = at(Lane::Prompt, &claim("some_new_phase", 1));
        assert_eq!(r.state, WorkState::Unknown);
    }

    /// **Only `final` counts as mined**, and every state short of it is in flight or failed — the
    /// line ADR-0122 draws between computing and mining.
    #[test]
    fn only_final_counts_as_mined() {
        use WorkState::*;
        for s in [Received, Executing, Executed, Committed, Submitting, Submitted, OnChain, WaitingReceipts, QuorumReached] {
            assert_eq!(s.outcome(), Outcome::InFlight, "{s:?}");
        }
        for s in [Accepted, RewardPending, Rewarded, NoReward] {
            assert_eq!(s.outcome(), Outcome::Mined, "{s:?}");
        }
        for s in [NotCommitted, Expired, GaveUp, Dropped, Voided] {
            assert_eq!(s.outcome(), Outcome::Failed, "{s:?}");
        }
        assert_eq!(Disputed.outcome(), Outcome::Paused);
        assert!(Executed.was_computed() && !Executed.was_submitted(), "a computed answer is not a submitted one");
        assert!(!Received.was_computed());
    }

    /// A prompt claim's reward track: drawn at `final + receipt_maturity`, spent inside the use
    /// window, and — only well past the window — nothing won is `NO_REWARD`, not a failure.
    #[test]
    fn a_prompt_claim_is_paid_by_its_draw_and_nothing_won_is_not_a_failure() {
        let r = at(Lane::Prompt, &prompt_final(8, 0, NOW - 100));
        assert_eq!((r.state, r.deadline_daa), (WorkState::RewardPending, Some(NOW - 100 + 400)));
        let r = at(Lane::Prompt, &prompt_final(8, 2, NOW - 500));
        assert_eq!(r.state, WorkState::RewardPending);
        assert!(r.estimated);
        let long_ago = NOW - 400 - 600 - 21 - 1;
        assert_eq!(at(Lane::Prompt, &prompt_final(8, 0, long_ago)).state, WorkState::NoReward);
        assert_eq!(at(Lane::Prompt, &prompt_final(8, 0, long_ago)).state.outcome(), Outcome::Mined);
        assert_eq!(at(Lane::Prompt, &prompt_final(8, 3, long_ago)).state, WorkState::Rewarded);
        assert_eq!(at(Lane::Prompt, &prompt_final(8, 8, NOW - 1)).state, WorkState::Rewarded);
        assert_eq!(at(Lane::Prompt, &prompt_final(0, 0, NOW - 1)).state, WorkState::NoReward);
    }

    /// A block claim's escrow is paid by a following coinbase; the date is an estimate and says so.
    #[test]
    fn a_block_claims_escrow_matures_a_coinbase_maturity_after_final() {
        let r = at(Lane::Block, &claim("final", NOW - 10));
        assert_eq!((r.state, r.deadline_daa), (WorkState::RewardPending, Some(NOW - 10 + 1 + 600)));
        assert!(r.estimated, "the claim read carries no payout block");
        assert_eq!(at(Lane::Block, &claim("final", NOW - 700)).state, WorkState::Rewarded);
    }

    /// The node's claim row sharpens a reading: its deadline is the chain's, and a final block
    /// claim's escrow is queued, paid, or was never there — no longer an estimate.
    #[test]
    fn the_claim_row_turns_estimates_into_reads() {
        let bound = at(Lane::Block, &claim("panel_bound", 9_800));
        let r = refine(Lane::Block, bound, &ClaimExtra { deadline_daa: Some(10_444), ..Default::default() });
        assert_eq!(r.deadline_daa, Some(10_444), "the chain's date wins over this CLI's arithmetic");
        let fin = at(Lane::Block, &claim("final", NOW - 10));
        assert!(fin.estimated);
        let queued = refine(
            Lane::Block,
            fin.clone(),
            &ClaimExtra { escrow_sompi: 170_800_000_000, payout_pending_sompi: Some(170_800_000_000), ..Default::default() },
        );
        assert_eq!((queued.state, queued.estimated), (WorkState::RewardPending, false));
        assert!(queued.detail.contains("1,708.00 MSK queued"), "{}", queued.detail);
        let merged = refine(Lane::Block, fin.clone(), &ClaimExtra::default());
        assert_eq!((merged.state, merged.estimated), (WorkState::Rewarded, false), "no escrow: nothing is pending");
        let paid = refine(Lane::Block, fin, &ClaimExtra { escrow_sompi: 170_800_000_000, ..Default::default() });
        assert!(paid.detail.starts_with("escrow 1,708.00 MSK paid"), "{}", paid.detail);
        let prompt = at(Lane::Prompt, &prompt_final(8, 0, NOW - 100));
        assert_eq!(refine(Lane::Prompt, prompt.clone(), &ClaimExtra::default()), prompt, "the prompt lane has no escrow to read");
    }

    /// The outbox's states, before the chain has a claim.
    #[test]
    fn a_prompt_job_before_the_chain_is_read_from_its_files() {
        let cases = [
            (OutboxState::NotCommitted, WorkState::NotCommitted),
            (OutboxState::CommittedNotSubmitted { signed_tx: None }, WorkState::Committed),
            (OutboxState::SignedNotSubmitted { tx_file: None }, WorkState::Submitting),
            (OutboxState::Expired { marker: PathBuf::from("m"), gave_up_too: false }, WorkState::Expired),
            (OutboxState::GaveUp { marker: PathBuf::from("m") }, WorkState::GaveUp),
            (OutboxState::Unreadable { why: "x".into() }, WorkState::Unknown),
        ];
        for (state, want) in cases {
            let r = classify(Lane::Prompt, Some(&row(state.clone())), None, None, false, Some(&t11()), 600, NOW);
            assert_eq!(r.state, want, "{state:?}");
        }
        // A queued commitment whose anchor has lapsed is expired whatever its file says.
        let mut lapsed = row(OutboxState::CommittedNotSubmitted { signed_tx: None });
        lapsed.commit_by_anchor_daa = Some(NOW - 1);
        assert_eq!(classify(Lane::Prompt, Some(&lapsed), None, None, false, None, 600, NOW).state, WorkState::Expired);
    }

    /// **Submitted and absent is two opposite facts**, told apart by the mempool, the rail's own
    /// verdict and its landing grace — never by guessing.
    #[test]
    fn a_submitted_carrier_is_submitted_until_the_rail_or_the_grace_says_dropped() {
        let submitted = row(OutboxState::Submitted);
        let absent = GetPalwFreePromptClaimResponse { found: false, ..Default::default() };
        let r = classify(Lane::Prompt, Some(&submitted), None, Some(&absent), true, None, 600, NOW);
        assert_eq!(r.state, WorkState::Submitted, "a carrier in the mempool is not dropped");
        let waiting = RailVerdict { settled: None, awaiting_since_daa: Some(NOW - 5) };
        let r = classify(Lane::Prompt, Some(&submitted), Some(&waiting), Some(&absent), false, None, 600, NOW);
        assert_eq!((r.state, r.deadline_daa), (WorkState::Submitted, Some(NOW - 5 + LANDING_GRACE_DAA)));
        let dropped = RailVerdict { settled: Some("dropped".into()), awaiting_since_daa: None };
        let r = classify(Lane::Prompt, Some(&submitted), Some(&dropped), Some(&absent), false, None, 600, NOW);
        assert_eq!(r.state, WorkState::Dropped);
        let r = classify(Lane::Prompt, Some(&submitted), None, Some(&absent), false, None, 600, NOW);
        assert_eq!(r.state, WorkState::Dropped, "asked, absent, not in the mempool and no rail to wait on");
        let r = classify(Lane::Prompt, Some(&submitted), None, None, false, None, 600, NOW);
        assert_eq!(r.state, WorkState::Submitted, "not asked is not absent");
    }

    /// The chain outranks every file: a claim that exists is read from its phase.
    #[test]
    fn the_chain_outranks_the_outbox() {
        let gave_up = row(OutboxState::GaveUp { marker: PathBuf::from("m") });
        let r = classify(Lane::Prompt, Some(&gave_up), None, Some(&claim("panel_bound", NOW - 1)), false, Some(&t11()), 600, NOW);
        assert_eq!(r.state, WorkState::WaitingReceipts);
    }

    /// The timeline marks the path against the state, and an end off the path is placed where the
    /// work left it, with the stages it will never reach blanked rather than shown as ahead.
    #[test]
    fn the_timeline_marks_the_path_and_places_an_end_where_the_work_left_it() {
        let t = timeline(Lane::Block, WorkState::WaitingReceipts, None);
        let marks: Vec<Mark> = t.iter().map(|(_, m)| *m).collect();
        assert_eq!(marks, vec![Mark::Done, Mark::Done, Mark::Done, Mark::Current, Mark::Ahead, Mark::Ahead, Mark::Ahead, Mark::Ahead]);
        let t = timeline(Lane::Prompt, WorkState::Voided, Some("bind_timeout"));
        let voided = t.iter().position(|(s, _)| *s == WorkState::Voided).unwrap();
        assert_eq!(t[voided - 1].0, WorkState::OnChain);
        assert_eq!(t[voided].1, Mark::End);
        assert!(t[voided + 1..].iter().all(|(_, m)| *m == Mark::NotReached));
        let t = timeline(Lane::Block, WorkState::Rewarded, None);
        assert!(t.iter().all(|(_, m)| *m == Mark::Done), "{t:?}");
    }

    /// A void is placed after the stage its reason says it was in: a receipt timeout after the
    /// wait for receipts, a conviction after the quorum; a dispute resumes, so what follows it is
    /// still ahead.
    #[test]
    fn a_void_is_placed_by_its_reason_and_a_dispute_keeps_the_road_ahead() {
        let t = timeline(Lane::Block, WorkState::Voided, Some("receipt_timeout"));
        let v = t.iter().position(|(s, _)| *s == WorkState::Voided).unwrap();
        assert_eq!(t[v - 1].0, WorkState::WaitingReceipts);
        let t = timeline(Lane::Block, WorkState::Voided, Some("court_fraud"));
        let v = t.iter().position(|(s, _)| *s == WorkState::Voided).unwrap();
        assert_eq!(t[v - 1].0, WorkState::QuorumReached);
        let t = timeline(Lane::Block, WorkState::Disputed, None);
        let d = t.iter().position(|(s, _)| *s == WorkState::Disputed).unwrap();
        assert_eq!(t[d].1, Mark::Current);
        assert!(t[d + 1..].iter().all(|(_, m)| *m == Mark::Ahead), "{t:?}");
        let t = timeline(Lane::Block, WorkState::Unknown, None);
        assert!(t.iter().take(t.len() - 1).all(|(_, m)| *m == Mark::Ahead), "an unknown state claims nothing passed");
    }

    /// A prefix resolves like a git hash: at least 4 hex, exactly one match, and an ambiguous or
    /// empty match names what it found.
    #[test]
    fn a_prefix_resolves_to_exactly_one_id() {
        let a = "3f9a1c2e".to_string() + &"0".repeat(120);
        let b = "3f9b0000".to_string() + &"0".repeat(120);
        let c = "3f9a1c2e".to_string() + &"1".repeat(120);
        assert_eq!(resolve_prefix("3f9a", [a.as_str(), b.as_str()]), Ok(a.as_str()));
        assert_eq!(resolve_prefix("3F9A1", [a.as_str(), b.as_str()]), Ok(a.as_str()), "case does not matter");
        assert_eq!(resolve_prefix("job:3f9a", [a.as_str(), b.as_str()]), Ok(a.as_str()), "the job: spelling resolves too");
        assert_eq!(resolve_prefix("3f9a", [a.as_str(), a.as_str()]), Ok(a.as_str()), "one id listed twice is one id");
        assert_eq!(resolve_prefix("3f9", [a.as_str(), b.as_str()]), Err(vec![]), "under four hex is refused");
        assert_eq!(resolve_prefix("zzzz", [a.as_str()]), Err(vec![]), "not hex is refused");
        assert_eq!(resolve_prefix("3f90", [a.as_str(), b.as_str()]), Err(vec![]), "no match");
        assert_eq!(resolve_prefix("3f9a1c2e", [a.as_str(), c.as_str()]).unwrap_err().len(), 2, "ambiguous names both");
    }
}
