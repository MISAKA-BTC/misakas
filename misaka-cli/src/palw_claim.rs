//! **`misaka palw claim` — what became of a free-prompt claim, and what to do about it.**
//!
//! A participant on testnet-11 runs the whole free-prompt lane on their own hosts: the gateway
//! answers a prompt and queues a commitment (`<outbox>/fp-job-<16hex>.json`), the rail signs and
//! submits it (`<stem>.rail.json`), and from there the claim is the chain's — a panel drawn, a
//! quorum licensing it, `final` past its challenge window, its quanta drawn at a beacon and spent
//! as receipt blocks, or voided somewhere on the way. The node has answered
//! `getPalwFreePromptClaim` for every one of those steps since ADR-0077 R0, and nothing a
//! participant runs ever called it: "did my claim do anything?" was answered by reading consensus
//! source, or by asking somebody who had.
//!
//! **A phase name is not an answer.** `voided (receipt_timeout)` is a fact about the chain; what
//! the person reading it needs is the fact about THEIR NODE it usually implies — the seats could
//! not get openings from it — and the one thing to change. So every row carries three strings,
//! `state`, `meaning` and `next`, and they come from pure functions: [`explain`] over what the
//! chain said, [`outbox_reading`] over a job that never got that far, and `in_mempool` for a
//! carrier still waiting to be mined. Those are what the tests pin; everything else here is
//! plumbing between them and a terminal.
//!
//! **The dates are the network's own.** Every window quoted — the anchor delay, the bind, receipt
//! and challenge windows, the receipt maturity and use window, the abandon hold, the retirement —
//! is read off the `ConsensusV2` bundle of `Params::from(<the node's network id>)`, the bundle the
//! node runs (it is inside `palw_ruleset_id_v2`, so two peers of one network cannot hold different
//! ones). A network this build holds no bundle for is quoted no dates at all, and the text says
//! so: testnet-11's numbers printed for a chain that does not run them would be a guess dressed as
//! a reading.
//!
//! Nothing here signs, spends or writes. The outbox is read, and only its bookkeeping is printed:
//! the gateway's summary also carries the answer, and neither the answer nor the prompt leaves
//! this module.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_rpc_core::GetPalwFreePromptClaimResponse;
use kaspa_rpc_core::api::rpc::RpcApi;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Every file of one gateway job is named `fp-job-<the first 16 hex of its job id>.<kind>`.
const JOB_PREFIX: &str = "fp-job-";
/// The gateway's own summary schema. A `fp-job-*.json` naming another is not a job summary.
const GATEWAY_SCHEMA: &str = "misaka.palw.fp-v3-gateway-artifact.v1";
/// What the gateway's anchor sweep appends to a queued commitment it retires
/// (`misaka_palw_fp_submit::EXPIRED_SUFFIX`; this crate does not link the submitter, so the one
/// string is spelled twice).
const EXPIRED_SUFFIX: &str = ".expired";
/// What the rail's `--watch` loop leaves beside a job it stopped retrying.
const GAVE_UP_SUFFIX: &str = ".submit-failed";
/// Said wherever a date would otherwise have been quoted.
const NO_WINDOWS: &str = "this CLI holds no ConsensusV2 bundle for this network, so it quotes no windows";
/// The executor's standing duty while a claim is live: its seats, and any challenger, ask THIS node.
const SERVE: &str = "keep this executor's node up with --palw-panel and --palw-class-artifact for the class, and the claim's \
                     material in its palw-retention directory";

// ---------------------------------------------------------------------------------------------
// the network's windows
// ---------------------------------------------------------------------------------------------

/// **The lattice windows, in DAA, as this network's `ConsensusV2` bundle declares them.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Windows {
    /// A panel is drawn from the first chain block at or after acceptance + this.
    pub(crate) anchor_delay: u64,
    /// A panel must bind within this of acceptance (or of the redraw), else `bind_timeout`.
    pub(crate) window_bind: u64,
    /// A bound panel's receipts are due within this of binding.
    pub(crate) window_receipt: u64,
    /// A licensed claim is final this long after licensing, unless challenged.
    pub(crate) window_challenge: u64,
    /// How long an accused executor has to disclose (ADR-0062 SA-3's `W_disclose`).
    pub(crate) disclose_window: u64,
    /// A final claim's draw slot is this far past `final`.
    pub(crate) receipt_maturity: u64,
    /// A winning quantum is spendable this long past its beacon.
    pub(crate) receipt_use_window: u64,
    /// An abandoned (`bind_timeout`) free-prompt claim keeps its collateral reserved this long.
    pub(crate) fp_abandon_hold: u64,
    /// A final or voided claim leaves the state this long after it ended; 0 = never.
    pub(crate) claim_retirement: u64,
}

impl Windows {
    /// `None` off `ConsensusV2`: every caller then says so instead of dating anything.
    pub(crate) fn of(params: &Params) -> Option<Self> {
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return None;
        };
        Some(Self {
            anchor_delay: bundle.panel.anchor_delay(),
            window_bind: bundle.state.window_bind(),
            window_receipt: bundle.state.window_receipt(),
            window_challenge: bundle.state.window_challenge(),
            disclose_window: kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&bundle.state),
            receipt_maturity: bundle.freeprompt.receipt_maturity_daa(),
            receipt_use_window: bundle.freeprompt.receipt_use_window_daa(),
            fp_abandon_hold: bundle.state.fp_abandon_hold_daa(),
            claim_retirement: bundle.state.claim_retirement_daa(),
        })
    }
}

/// `DAA 1234 (in 56 DAA)`, `DAA 1234 (56 DAA ago)` or `DAA 1234 (now)`: a date, and where the tip is.
fn when(daa: u64, now: u64) -> String {
    match daa.cmp(&now) {
        std::cmp::Ordering::Greater => format!("DAA {daa} (in {} DAA)", daa - now),
        std::cmp::Ordering::Less => format!("DAA {daa} ({} DAA ago)", now - daa),
        std::cmp::Ordering::Equal => format!("DAA {daa} (now)"),
    }
}

// ---------------------------------------------------------------------------------------------
// what a chain read means
// ---------------------------------------------------------------------------------------------

/// One row's answer: a stable token for scripts, what it means, and the next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    /// The phase the node named, `not_on_chain`, `in_mempool`, `unknown_phase`, or an outbox
    /// state (`not_committed`, `committed_not_submitted`, `signed_not_submitted`, `expired`,
    /// `gave_up`, `unreadable`).
    pub(crate) state: &'static str,
    pub(crate) meaning: String,
    pub(crate) next: String,
}

/// **The phase → (meaning, next) mapping, as one pure function of what the node said.**
///
/// `now` is the node's virtual DAA, so a deadline reads as "in N" or "N ago" rather than as a bare
/// number the reader has to subtract. `carrier` is the commitment transaction when the caller knows
/// it (an outbox row), so the not-found answer can name the log line to look for.
///
/// Every phase `palw_claim_phase_named` can return is an arm, and anything else is
/// `unknown_phase` — named, and nothing inferred from it. A node newer than this build could add
/// a phase, and a guess about what it means would be the one answer here that is worse than none.
pub(crate) fn explain(r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, now: u64, carrier: Option<&str>) -> Reading {
    if !r.found {
        return not_on_chain(w, carrier);
    }
    match r.phase.as_str() {
        "provisional" => provisional(r, w, now),
        "panel_bound" => panel_bound(r, w, now),
        "receipt_licensed" => receipt_licensed(r, w, now),
        "final" => finalized(r, w, now),
        "default_disputed" => default_disputed(r, w, now),
        "voided" => voided(r, w, now),
        other => Reading {
            state: "unknown_phase",
            meaning: format!("the node reports phase '{other}', which this CLI does not know: the node is newer than this build"),
            next: "read it with a misaka CLI as new as the node; nothing is inferred from a phase this build cannot name".to_string(),
        },
    }
}

/// **`found: false` is two facts the RPC does not tell apart**, so the row names both: the chain
/// never took the commitment in (dropped at the fold, or never mined), or the claim ended long
/// enough ago that the state retired it. The log lines named are the node's own words — the fold's
/// drop line, and the carrier walk's refusal — because "the claim is not there" is only useful
/// with the place its reason was written.
fn not_on_chain(w: Option<&Windows>, carrier: Option<&str>) -> Reading {
    let retired = match w {
        Some(w) if w.claim_retirement > 0 => format!(
            " A claim that ended (final or voided) more than {} DAA ago has been retired from the state and reads the same way.",
            w.claim_retirement
        ),
        Some(_) => String::new(),
        None => format!(" ({NO_WINDOWS}; a network without a ConsensusV2 ruleset holds no claim at all.)"),
    };
    let carrier = carrier.unwrap_or("<txid>");
    Reading {
        state: "not_on_chain",
        meaning: format!("this chain holds no such claim.{retired}"),
        next: format!(
            "if its carrier transaction was accepted, the chain dropped the commitment: grep the node log for 'PALW lifecycle \
             object was dropped' (FreePromptExposureCeiling reads '... above its exposure ceiling': the bond's exposure ceiling \
             was full when it landed) and for '[palw-fp] carrier {carrier} produced no object'. No such line: the transaction \
             was never mined (one that left the mempool moments ago may still be on its way in; ask again in a few blocks). \
             Never submitted: submit it."
        ),
    }
}

/// **Submitted and not mined yet** — the ordinary state for the first blocks after a submit, and
/// the one a bare `not_on_chain` would have misread as a drop. Only an outbox row can be asked
/// this: it knows its carrier, and a claim id alone names no transaction.
fn in_mempool(txid: &str) -> Reading {
    Reading {
        state: "in_mempool",
        meaning: format!("submitted, not mined yet: the carrier {txid} is in the node's mempool"),
        next: "wait for a block to take it; the claim then appears here as provisional. A carrier that stays there for long \
               is underpaying its fee, or is an orphan waiting on a parent transaction that never landed"
            .to_string(),
    }
}

fn provisional(r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, now: u64) -> Reading {
    let accepted = r.accepted_daa;
    let meaning = match w {
        // The sweep voids a claim whose FIRST bind window closes unbound, so one still provisional
        // past it is on its redraw — whose base (`rebound_daa`) the RPC does not carry. Said as
        // what it is rather than dated from a base this command cannot see.
        Some(w) if now > accepted.saturating_add(w.window_bind) => format!(
            "accepted at DAA {accepted}; its first bind window closed at DAA {}, so this is its one redraw: a receipt timeout \
             sent it back for a second panel, drawn {} DAA after the redraw (later if an accusation paused it) and bound \
             within {} DAA of it, or it voids bind_timeout",
            accepted.saturating_add(w.window_bind),
            w.anchor_delay,
            w.window_bind
        ),
        Some(w) => {
            let slot = accepted.saturating_add(w.anchor_delay);
            let deadline = when(accepted.saturating_add(w.window_bind), now);
            if now >= slot {
                // The anchor exists once the chain is past the slot, and binding is retried every
                // chain block after it; a registry too small to seat a FULL panel yields nothing.
                format!(
                    "accepted at DAA {accepted}; its anchor slot (DAA {slot}) has passed and no panel has bound, which usually \
                     means the registry cannot seat a full panel for this class. Binding is retried every chain block until \
                     {deadline}, then it voids bind_timeout"
                )
            } else {
                format!(
                    "accepted at DAA {accepted}; no panel yet. Its panel is drawn from the first chain block at or after DAA \
                     {slot} (acceptance + anchor_delay {}) and must bind by {deadline}, or it voids bind_timeout",
                    w.anchor_delay
                )
            }
        }
        None => format!("accepted at DAA {accepted}; no panel yet ({NO_WINDOWS})"),
    };
    Reading {
        state: "provisional",
        meaning,
        next: format!(
            "nothing to submit. {SERVE}: once the panel is drawn, its seats ask this node for the material and interval openings"
        ),
    }
}

fn panel_bound(r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, now: u64) -> Reading {
    let bound = r.phase_daa;
    // Whether this is the first panel or the redraw's is NOT inferred from the dates: a
    // data-availability session shifts `bound_daa` by its pause, so a first panel can bind "late".
    // The rule is stated instead, and it is true either way.
    let miss = "no quorum in the window sends it back for one redraw, or voids it receipt_timeout if it was redrawn already";
    let meaning = match w {
        Some(w) => format!(
            "a panel bound at DAA {bound}; its seats replay the job and must file receipts by {}. A quorum of Valid licenses \
             it; {miss}",
            when(bound.saturating_add(w.window_receipt), now)
        ),
        None => format!("a panel bound at DAA {bound}; its seats replay the job and file receipts ({NO_WINDOWS}); {miss}"),
    };
    Reading {
        state: "panel_bound",
        meaning,
        next: format!(
            "{SERVE}: the seats ask THIS node for the claim's material and interval openings, and a node that cannot serve \
             them (down, no panel, material or artifact missing) is how claims void receipt_timeout. Its log names every \
             refused request ('refused an opening request for claim ...')"
        ),
    }
}

fn receipt_licensed(r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, now: u64) -> Reading {
    let licensed = r.phase_daa;
    let meaning = match w {
        Some(w) => format!(
            "a quorum of seats filed Valid at DAA {licensed}; unless challenged it becomes final at licensed + \
             window_challenge {} = {}, and an open court holds it until the court closes",
            w.window_challenge,
            when(licensed.saturating_add(w.window_challenge), now)
        ),
        None => format!(
            "a quorum of seats filed Valid at DAA {licensed}; it becomes final after its challenge window unless challenged \
             ({NO_WINDOWS})"
        ),
    };
    Reading {
        state: "receipt_licensed",
        meaning,
        next: format!(
            "{SERVE}, and its retained trace, until it is final: a challenger may open a court or a data-availability \
             accusation against it, and only this executor's node can answer"
        ),
    }
}

/// **`final` is where a free-prompt claim starts to pay, and the paying is not automatic.**
///
/// The quanta are drawn at a beacon the claim cannot see yet, a winning quantum is spendable only
/// inside its use window, and only the claim's OWN bond's producer can spend it (receipts do not
/// transfer) — so the one step that turns a certified claim into money is a producer this operator
/// has to be running, in a window this row dates.
fn finalized(r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, now: u64) -> Reading {
    let at = r.phase_daa;
    let retire = retirement_note(at, w, now);
    if !r.is_free_prompt {
        return Reading {
            state: "final",
            meaning: format!(
                "certified at DAA {at}. This is an attempt-lane claim (a block's own), not a free prompt: its escrowed reward \
                 became spendable at final, and it has no quanta to draw.{retire}"
            ),
            next: "nothing to do: an attempt claim is paid through its block's escrow".to_string(),
        };
    }
    let spent = format!("{} of {} quanta spent so far", r.quanta_spent, r.quanta);
    let meaning = match w {
        Some(w) => format!(
            "certified at DAA {at}. Its draw slot is final + receipt_maturity {} = {}: the beacon is the first attempt-class \
             chain block at or after it, and each of its {} quanta whose ticket wins there can be spent as one RECEIPT block \
             within {} DAA of that beacon. {spent}.{retire}",
            w.receipt_maturity,
            when(at.saturating_add(w.receipt_maturity), now),
            r.quanta,
            w.receipt_use_window
        ),
        None => format!(
            "certified at DAA {at}. Its {} quanta are drawn at a beacon after the receipt maturity, and a winning one can be \
             spent as one RECEIPT block inside the use window ({NO_WINDOWS}). {spent}.",
            r.quanta
        ),
    };
    let next = if r.quanta == 0 {
        "nothing to spend: the claim holds no quanta".to_string()
    } else if r.quanta_spent >= r.quanta {
        "nothing left: every quantum is spent".to_string()
    } else {
        format!(
            "only THIS bond's own producer can spend them: run kaspad --palw-produce --palw-producer-bond {} with that bond's \
             --palw-producer-key through the window. Each receipt block's coinbase pays that producer's \
             --palw-producer-pay-address (default: the key's own address) like any block it mines; a winning quantum left \
             unspent when its window closes is lost",
            r.executor_bond
        )
    };
    Reading { state: "final", meaning, next }
}

fn default_disputed(r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, now: u64) -> Reading {
    let accused = r.phase_daa;
    let by = match w {
        Some(w) => format!(", by accusation + {} = {}", w.disclose_window, when(accused.saturating_add(w.disclose_window), now)),
        None => format!(" ({NO_WINDOWS})"),
    };
    Reading {
        state: "default_disputed",
        meaning: format!(
            "a data-availability accusation was filed at DAA {accused}: the claim is frozen, not voided, until this executor \
             discloses the accused trace event on chain{by}"
        ),
        next: "keep this executor's node running with --palw-panel and the claim's retained capture: its responder opens the \
               accused event and files the disclosure. No disclosure in time voids the claim producer_withholding and slashes \
               the collateral it reserved"
            .to_string(),
    }
}

/// **A void reason is named with who paid for it**, because the four are not one outcome: the two
/// timeouts slash nobody (the chain cannot tell a producer that withheld from a panel that could
/// not judge), and the two convictions take the collateral the claim reserved.
fn voided(r: &GetPalwFreePromptClaimResponse, w: Option<&Windows>, now: u64) -> Reading {
    let at = r.phase_daa;
    let retire = retirement_note(at, w, now);
    let (meaning, next) = match r.void_reason.as_str() {
        "bind_timeout" => {
            let hold = match w {
                Some(w) if r.is_free_prompt && w.fp_abandon_hold > 0 => format!(
                    " Nothing was slashed, but its collateral stays reserved on the bond until {} (the free-prompt abandon hold) \
                     and counts against the bond's exposure ceiling until then.",
                    when(at.saturating_add(w.fp_abandon_hold), now)
                ),
                _ => " Nothing was slashed.".to_string(),
            };
            (
                format!(
                    "voided at DAA {at}: bind_timeout. No panel could be bound inside the bind window, which usually means the \
                     registry could not seat a full panel for this class.{hold}"
                ),
                "nothing to recover on this claim; a claim on this class binds only once enough eligible bonds (mature, \
                 collateralized, declaring the class) exist to seat a full panel"
                    .to_string(),
            )
        }
        "receipt_timeout" => (
            format!(
                "voided at DAA {at}: receipt_timeout. Two panels in a row (the claim gets one redraw) filed no quorum of Valid \
                 receipts inside the receipt window, which usually means the seats could not obtain the material or interval \
                 openings from the executor's node. The executor is not slashed."
            ),
            "nothing to recover on this claim. Before the next one make sure this node can serve its seats: --palw-panel, \
             --palw-class-artifact for the class, <claim>.material and <claim>.answer in its palw-retention directory, and a P2P \
             port the seats can reach. Its log's 'refused an opening request for claim' lines say what the seats were told"
                .to_string(),
        ),
        "court_fraud" => (
            format!(
                "voided at DAA {at}: court_fraud. A court proved the execution wrong; the collateral this claim reserved was \
                 slashed from the bond."
            ),
            "nothing to recover. Before committing again check that this host runs the class's pinned artifact: an executor \
             convicted on an honest run usually holds a different artifact for the same class"
                .to_string(),
        ),
        "producer_withholding" => (
            format!(
                "voided at DAA {at}: producer_withholding. The executor failed a data-availability obligation (an accusation \
                 went unanswered inside its disclose window, or, before ADR-0065's fence, a quorum of seats filed \
                 Unavailable); the collateral this claim reserved was slashed from the bond."
            ),
            "nothing to recover. Keep --palw-panel and the retained capture on this node until each claim is final: that is \
             what answers an accusation"
                .to_string(),
        ),
        other => (
            format!("voided at DAA {at} for a reason this CLI does not know ('{other}')."),
            "read it with a misaka CLI as new as the node; nothing is inferred from a reason this build cannot name".to_string(),
        ),
    };
    Reading { state: "voided", meaning: format!("{meaning}{retire}"), next }
}

/// A terminal record leaves the state `claim_retirement` after it ended, and from then on this
/// command cannot tell it from a claim that never existed — so the row says so while it still can.
fn retirement_note(ended: u64, w: Option<&Windows>, now: u64) -> String {
    match w {
        Some(w) if w.claim_retirement > 0 => format!(
            " Its record leaves the state at {}; after that this command reports it as not on the chain.",
            when(ended.saturating_add(w.claim_retirement), now)
        ),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------------------------
// what an outbox says
// ---------------------------------------------------------------------------------------------

/// **Where one gateway job got to, from its files alone.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutboxState {
    /// The gateway answered and wrote no commitment (`committed` is not `true`).
    NotCommitted,
    /// A commitment is queued and no rail summary says anything carried it.
    CommittedNotSubmitted { signed_tx: Option<PathBuf> },
    /// The rail signed it (`<stem>.rail.json`) and recorded no submission.
    SignedNotSubmitted { tx_file: Option<PathBuf> },
    /// The gateway's sweep retired the queued commitment: its anchor lapsed first.
    Expired { marker: PathBuf, gave_up_too: bool },
    /// The rail's `--watch` loop stopped retrying it.
    GaveUp { marker: PathBuf },
    /// The rail recorded a transaction id: from here on the chain is the only authority.
    Submitted,
    /// A file of this job could not be read as what its name says it is.
    Unreadable { why: String },
}

/// One job of the outbox, as its files describe it. Only bookkeeping is kept: the summary's answer
/// text is never read into this struct.
#[derive(Clone, Debug)]
pub(crate) struct OutboxRow {
    /// The gateway's `<stem>.json` — or, for a job whose summary is gone, the file that still
    /// speaks for it (a marker, or the rail's summary).
    pub(crate) job_file: PathBuf,
    /// `fp-job-<16 hex>`, the name every file of one job shares.
    pub(crate) stem: String,
    pub(crate) modified: Option<SystemTime>,
    /// The rail's `fp_claim_id` when the rail signed (that is the claim that went to the chain),
    /// else the gateway's.
    pub(crate) claim_id: Option<String>,
    pub(crate) committed: Option<bool>,
    pub(crate) not_committed_because: Option<String>,
    /// The last virtual DAA at which the commitment may still be submitted (ADR-0077 SA-1b).
    pub(crate) commit_by_anchor_daa: Option<u64>,
    pub(crate) work_leaves: Option<u64>,
    pub(crate) decode_tokens_executed: Option<u64>,
    pub(crate) submitted_txid: Option<String>,
    pub(crate) state: OutboxState,
}

/// The files of one job that say anything about its fate.
#[derive(Default)]
struct JobFiles {
    summary: Option<PathBuf>,
    rail: Option<PathBuf>,
    signed_tx: Option<PathBuf>,
    expired: Vec<PathBuf>,
    gave_up: Vec<PathBuf>,
}

/// **Read every job of a gateway outbox, oldest first.**
///
/// Files are grouped by their shared `fp-job-<16hex>` stem, so the classification is about a JOB
/// rather than about whichever file a directory listing happened to return first: its summary
/// says whether it was committed, `.rail.json` whether it was signed and submitted, and an
/// `.expired` or `.submit-failed` marker on any of its files what stopped it. `.derived.json`,
/// `.result.borsh` and the rest belong to a job but say nothing about where it got to.
///
/// Only an unreadable DIRECTORY is an error. A file that cannot be read is a row saying so: one
/// damaged summary must not hide the other three hundred jobs.
pub(crate) fn scan_outbox(dir: &Path) -> Result<Vec<OutboxRow>, CliError> {
    let unreadable = |e: std::io::Error| CliError::new(exit::GENERIC, format!("--outbox {}: {e}", dir.display()));
    let mut jobs: BTreeMap<String, JobFiles> = BTreeMap::new();
    for entry in std::fs::read_dir(dir).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        // `traces/` lives here too, and a directory is never a job's file whatever it is named.
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else { continue };
        let Some((stem, rest)) = job_stem(&name) else { continue };
        let files = jobs.entry(stem.to_string()).or_default();
        let path = entry.path();
        if rest == ".json" {
            files.summary = Some(path);
        } else if rest == ".rail.json" {
            files.rail = Some(path);
        } else if rest == ".commitment-tx.borsh" {
            files.signed_tx = Some(path);
        } else if name.ends_with(EXPIRED_SUFFIX) {
            files.expired.push(path);
        } else if name.ends_with(GAVE_UP_SUFFIX) {
            files.gave_up.push(path);
        }
    }
    let mut rows: Vec<OutboxRow> = jobs.into_iter().filter_map(|(stem, files)| row_of(stem, files)).collect();
    rows.sort_by(|a, b| a.modified.cmp(&b.modified).then_with(|| a.job_file.cmp(&b.job_file)));
    Ok(rows)
}

/// `fp-job-<id>` and the rest of the name (`.json`, `.rail.json`, `...borsh.expired`), or `None`
/// for a file that is no job's.
fn job_stem(name: &str) -> Option<(&str, &str)> {
    if !name.starts_with(JOB_PREFIX) {
        return None;
    }
    let (stem, rest) = name.split_at(name.find('.')?);
    (stem.len() > JOB_PREFIX.len()).then_some((stem, rest))
}

fn read_json_object(path: &Path) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("{} is not JSON: {e}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{} is JSON but not an object", path.display()));
    }
    Ok(value)
}

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string)
}

/// One job's row. `None` for a stem with nothing that speaks for the job (only a `.result.borsh`,
/// say): there is no summary, no marker and no rail record to report.
fn row_of(stem: String, files: JobFiles) -> Option<OutboxRow> {
    let expired = files.expired.into_iter().min();
    let gave_up = files.gave_up.into_iter().min();
    let job_file = files.summary.clone().or_else(|| expired.clone()).or_else(|| gave_up.clone()).or_else(|| files.rail.clone())?;
    let mut row = OutboxRow {
        modified: std::fs::metadata(&job_file).and_then(|m| m.modified()).ok(),
        job_file,
        stem,
        claim_id: None,
        committed: None,
        not_committed_because: None,
        commit_by_anchor_daa: None,
        work_leaves: None,
        decode_tokens_executed: None,
        submitted_txid: None,
        state: OutboxState::Submitted,
    };
    // The job's own record: the gateway's summary — or, when that is gone, a marker that is one
    // renamed (a JSON marker reads the same; any other marker names no claim).
    let record = match &files.summary {
        Some(path) => match read_json_object(path) {
            Ok(v) => Some(v),
            Err(why) => {
                row.state = OutboxState::Unreadable { why };
                return Some(row);
            }
        },
        None => [&expired, &gave_up].into_iter().flatten().find_map(|marker| read_json_object(marker).ok()),
    };
    if let Some(v) = &record {
        if let Some(schema) = v.get("schema").and_then(|s| s.as_str())
            && schema != GATEWAY_SCHEMA
        {
            row.state =
                OutboxState::Unreadable { why: format!("{} is not a gateway job summary (schema {schema})", row.job_file.display()) };
            return Some(row);
        }
        row.claim_id = str_field(v, "fp_claim_id");
        row.committed = v.get("committed").and_then(|c| c.as_bool());
        row.not_committed_because = str_field(v, "not_committed_because");
        row.commit_by_anchor_daa = v.get("commit_by_anchor_daa").and_then(|d| d.as_u64());
        row.work_leaves = v.get("work_leaves").and_then(|d| d.as_u64());
        row.decode_tokens_executed = v.get("decode_tokens_executed").and_then(|d| d.as_u64());
    }
    // The rail's record, when it signed. Its claim id wins: `--class-id` rewrites the class before
    // signing, and the id that went to the chain is the one the rail signed.
    let rail = match &files.rail {
        Some(path) => match read_json_object(path) {
            Ok(v) => Some(v),
            Err(why) => {
                row.state = OutboxState::Unreadable { why };
                return Some(row);
            }
        },
        None => None,
    };
    if let Some(v) = &rail {
        if let Some(claim) = str_field(v, "fp_claim_id") {
            row.claim_id = Some(claim);
        }
        row.submitted_txid = str_field(v, "submitted");
    }
    // A transaction id outranks every marker: once the rail says it submitted, only the chain can
    // say what happened, and a marker left by an earlier attempt must not speak over it.
    row.state = if files.summary.is_some() && row.committed != Some(true) {
        OutboxState::NotCommitted
    } else if row.submitted_txid.is_some() {
        OutboxState::Submitted
    } else if let Some(marker) = expired {
        OutboxState::Expired { marker, gave_up_too: gave_up.is_some() }
    } else if let Some(marker) = gave_up {
        OutboxState::GaveUp { marker }
    } else if let Some(v) = &rail {
        OutboxState::SignedNotSubmitted { tx_file: str_field(v, "tx_file").map(PathBuf::from).or(files.signed_tx) }
    } else {
        OutboxState::CommittedNotSubmitted { signed_tx: files.signed_tx }
    };
    Some(row)
}

/// **What a job that never reached the chain means, and what is left to do** — the outbox's pure
/// function, beside [`explain`]. `None` for a submitted job: the files have nothing more to say
/// about it, and the chain has to be asked.
///
/// The deadline is the gateway's own `commit_by_anchor_daa` read against the node's DAA with the
/// submitter's comparison (strictly past is expired), so a row never offers a command the
/// submitter would refuse.
pub(crate) fn outbox_reading(row: &OutboxRow, outbox: &Path, now: u64) -> Option<Reading> {
    let watch = format!("misaka-palw-fp-rail --watch {} --bond-key-seed <file> --rpc <host:port>", outbox.display());
    let rail = format!(
        "misaka-palw-fp-rail --artifact {} --bond-key-seed <file> --funding-outpoint <txid:index> --funding-amount <sompi> \
         --submit --rpc <host:port>",
        outbox.join(&row.stem).display()
    );
    let lapsed = row.commit_by_anchor_daa.is_some_and(|by| now > by);
    let deadline = match row.commit_by_anchor_daa {
        Some(by) if now > by => format!(" Its anchor lapsed at DAA {by} (now {now}): no node will accept it any more."),
        Some(by) => format!(" It must reach the chain by {}.", when(by, now)),
        None => String::new(),
    };
    let too_late = format!(
        "nothing can revive this one (ask the prompt again for a fresh claim); run a submitter beside the gateway so later jobs \
         go out in time: {watch}"
    );
    let reading = match &row.state {
        OutboxState::Submitted => return None,
        OutboxState::NotCommitted => {
            let because = match (row.committed, row.not_committed_because.as_deref()) {
                (_, Some(why)) if !why.is_empty() => why.to_string(),
                (None, _) => "the summary has no `committed` field (a gateway build older than the field)".to_string(),
                _ => "the gateway recorded no reason".to_string(),
            };
            Reading {
                state: "not_committed",
                meaning: format!(
                    "answered, not committed: {because}. The answer was served; no commitment was written, so there is no \
                     claim to look for"
                ),
                next: "nothing to submit for this job; clear the reason above before the next one".to_string(),
            }
        }
        OutboxState::CommittedNotSubmitted { signed_tx } => {
            let signed = signed_tx
                .as_ref()
                .map(|tx| {
                    format!(
                        " A signed transaction is here ({}) but no rail summary recorded a submission: the submit failed or was \
                         interrupted.",
                        tx.display()
                    )
                })
                .unwrap_or_default();
            Reading {
                state: "committed_not_submitted",
                meaning: format!(
                    "committed, NOT submitted: the gateway queued a commitment and nothing has carried it to the chain.{signed}{deadline}"
                ),
                next: if lapsed {
                    too_late
                } else {
                    format!(
                        "run {watch} (it submits every queued job, oldest first, and logs why it holds one: the bond's exposure \
                         ceiling, funding, the node's sync), or the rail on this job alone: {rail}"
                    )
                },
            }
        }
        OutboxState::SignedNotSubmitted { tx_file } => {
            let tx = tx_file.as_ref().map(|tx| format!(" ({})", tx.display())).unwrap_or_default();
            Reading {
                state: "signed_not_submitted",
                meaning: format!(
                    "signed, not submitted: the rail signed the transaction{tx} but was not asked to submit it.{deadline}"
                ),
                next: if lapsed {
                    too_late
                } else {
                    format!("re-run the rail on this job with --submit (it also stages the material the seats need): {rail}")
                },
            }
        }
        OutboxState::Expired { marker, gave_up_too } => {
            let by = row.commit_by_anchor_daa.map(|by| format!(" at DAA {by}")).unwrap_or_default();
            let gave_up = if *gave_up_too { "; the rail's --watch loop had given up on it as well" } else { "" };
            Reading {
                state: "expired",
                meaning: format!(
                    "expired: the gateway retired the queued commitment ({}) because its anchor lapsed{by} before anything \
                     submitted it{gave_up}. The answer was served; this claim can never be made",
                    marker.display()
                ),
                next: format!(
                    "nothing to recover (ask the prompt again for a fresh claim); run a submitter beside the gateway so later \
                     jobs go out before their anchor lapses: {watch}"
                ),
            }
        }
        // The watcher gives up for one of two reasons and only its log says which: a claim larger
        // than the bond's whole exposure ceiling (no retry can land it), or `--max-attempts`
        // refused submissions. The row names both rather than guess.
        OutboxState::GaveUp { marker } => Reading {
            state: "gave_up",
            meaning: format!(
                "gave up: the rail's --watch loop took this job out of its queue ({}): either its claim can never fit the bond's \
                 exposure ceiling, or every attempt to submit it was refused. The rail's log says which.{deadline}",
                marker.display()
            ),
            next: if lapsed {
                "nothing can revive this one; fix what the rail's log names (a bond too small for jobs this long, or funding, fee, \
                 node reachability) before the next jobs"
                    .to_string()
            } else {
                format!(
                    "if the log names the exposure ceiling, this job cannot fit this bond (a bond sized for jobs this long, or fewer \
                     tokens per job, is the fix); otherwise fix what it names (funding, fee, node reachability) and submit this \
                     job by hand before its anchor lapses: {rail}"
                )
            },
        },
        OutboxState::Unreadable { why } => Reading {
            state: "unreadable",
            meaning: format!("unreadable: {why}"),
            next: "inspect the file: it is not a gateway job record this command can read".to_string(),
        },
    };
    Some(reading)
}

// ---------------------------------------------------------------------------------------------
// `misaka palw claim` — the command
// ---------------------------------------------------------------------------------------------

/// Where a row came from, which decides whether "not on the chain" fails the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Named,
    Outbox,
}

struct Entry {
    source: Source,
    claim_id: Option<String>,
    /// What the chain said, when it was asked.
    chain: Option<GetPalwFreePromptClaimResponse>,
    outbox: Option<OutboxRow>,
    reading: Reading,
}

/// The clap group already refuses an empty invocation; this is the same refusal for a caller that
/// is not clap, in the same words.
fn something_named(claim_ids: &[String], outbox: Option<&Path>) -> CliResult {
    if claim_ids.is_empty() && outbox.is_none() {
        return Err(CliError::new(
            exit::GENERIC,
            "name at least one claim id (the gateway's `fp_claim_id`, 128 hex) or --outbox <DIR> (the gateway's outbox), or both",
        ));
    }
    Ok(())
}

/// **Only a NAMED claim the chain does not hold fails the command** — the shape `palw derived` and
/// `palw certified` take, so a script can branch on it.
///
/// An outbox is a history, not a question. Its rows include jobs that were never meant to become
/// claims (answered, not committed) and claims that ended long enough ago to be retired from the
/// state; "one of your three hundred jobs is not on the chain" as an exit code would make the
/// command fail in exactly the directory it exists to read. So an outbox row says what it is, and
/// the exit code says whether the directory could be read at all.
fn verdict(entries: &[Entry]) -> CliResult {
    let missing: Vec<&str> = entries
        .iter()
        .filter(|e| e.source == Source::Named && e.chain.as_ref().is_some_and(|r| !r.found))
        .filter_map(|e| e.claim_id.as_deref())
        .collect();
    match missing.as_slice() {
        [] => Ok(()),
        [one] => Err(CliError::new(exit::GENERIC, format!("this chain holds no claim {one}"))),
        many => Err(CliError::new(exit::GENERIC, format!("{} named claims are not on this chain: {}", many.len(), many.join(", ")))),
    }
}

async fn ask(reader: &crate::palw_derived::Reader, claim: Hash64) -> Result<GetPalwFreePromptClaimResponse, CliError> {
    reader
        .client
        .get_palw_free_prompt_claim(claim.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwFreePromptClaim {claim}: {e}")))
}

/// **A submitted job is the chain's to answer** — and when the chain does not hold its claim, the
/// node's mempool is asked about the carrier, because "not mined yet" and "dropped" are opposite
/// instructions: wait, or stop paying fees into a full bond.
async fn follow_submitted(
    reader: &crate::palw_derived::Reader,
    row: &OutboxRow,
    windows: Option<&Windows>,
    now: u64,
) -> Result<(Option<GetPalwFreePromptClaimResponse>, Reading), CliError> {
    // Checked here rather than handed to the node, which refuses a malformed id as an ERROR — and
    // one bad file must not stop the rest of the outbox being read.
    let Some(Ok(claim)) = row.claim_id.as_deref().map(crate::palw_derived::parse_claim_id) else {
        let reading = Reading {
            state: "unreadable",
            meaning: format!(
                "submitted as {}, but its files name no readable claim id ({})",
                row.submitted_txid.as_deref().unwrap_or("?"),
                row.claim_id.as_deref().unwrap_or("none")
            ),
            next: "take the claim id from the rail's own output and run misaka palw claim <claim-id>".to_string(),
        };
        return Ok((None, reading));
    };
    let chain = ask(reader, claim).await?;
    if !chain.found
        && let Some(txid) = row.submitted_txid.as_deref()
        && let Ok(id) = txid.parse::<kaspa_consensus_core::tx::TransactionId>()
        && reader.client.get_mempool_entry(id, true, false).await.is_ok()
    {
        return Ok((Some(chain), in_mempool(txid)));
    }
    let reading = explain(&chain, windows, now, row.submitted_txid.as_deref());
    Ok((Some(chain), reading))
}

/// **`misaka palw claim [<claim-id>...] [--outbox <DIR>]`** — the named claims first, in the order
/// given, then every outbox job oldest first, each put to the chain only when its files say it
/// reached one. The exit code is [`verdict`]'s.
pub async fn show(ctx: &Ctx, claim_ids: &[String], outbox: Option<&Path>, json: bool) -> CliResult {
    something_named(claim_ids, outbox)?;
    // Parsed before a connection is opened, so a typo is a message about the argument rather than
    // a round trip that comes back "not on this chain", which is a different fact.
    let mut named: Vec<Hash64> = Vec::with_capacity(claim_ids.len());
    for id in claim_ids {
        let claim = crate::palw_derived::parse_claim_id(id)?;
        if !named.contains(&claim) {
            named.push(claim);
        }
    }
    // Read before the node for the same reason: an unreadable directory is the one outbox failure.
    let rows = match outbox {
        Some(dir) => scan_outbox(dir)?,
        None => Vec::new(),
    };

    let reader = crate::palw_derived::connect(ctx).await?;
    let server = reader.client.get_server_info().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getServerInfo: {e}")))?;
    let now = server.virtual_daa_score;
    let windows = Windows::of(&Params::from(server.network_id));

    let mut entries = Vec::with_capacity(named.len() + rows.len());
    for claim in named {
        let chain = ask(&reader, claim).await?;
        let reading = explain(&chain, windows.as_ref(), now, None);
        entries.push(Entry { source: Source::Named, claim_id: Some(claim.to_string()), chain: Some(chain), outbox: None, reading });
    }
    if let Some(dir) = outbox {
        for row in rows {
            let (chain, reading) = match outbox_reading(&row, dir, now) {
                Some(reading) => (None, reading),
                None => follow_submitted(&reader, &row, windows.as_ref(), now).await?,
            };
            entries.push(Entry { source: Source::Outbox, claim_id: row.claim_id.clone(), chain, outbox: Some(row), reading });
        }
    }

    let header = Header { network: server.network_id.to_string(), now, synced: server.is_synced, windows, outbox };
    if json || ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&document(&header, &entries)).expect("serializable"));
    } else {
        println!("{}", render_human(&header, &entries));
    }
    verdict(&entries)
}

/// What the node said about itself, read once and printed above every row.
struct Header<'a> {
    network: String,
    now: u64,
    synced: bool,
    windows: Option<Windows>,
    outbox: Option<&'a Path>,
}

fn document(h: &Header<'_>, entries: &[Entry]) -> serde_json::Value {
    serde_json::json!({
        "schema": "misaka.palw.claims.v1",
        "network": h.network,
        "daa_score": h.now,
        "node_synced": h.synced,
        "windows": h.windows,
        "windows_note": if h.windows.is_none() { Some(NO_WINDOWS) } else { None },
        "outbox": h.outbox.map(|dir| dir.display().to_string()),
        "claims": entries.iter().map(entry_json).collect::<Vec<_>>(),
    })
}

/// One row as JSON. Every key is present on every row, `null` where it does not apply, so a script
/// never has to ask whether a key exists before reading it; chain fields are `null` when the chain
/// was not asked or does not hold the claim, rather than a default that reads like a value.
#[derive(serde::Serialize)]
struct EntryJson<'a> {
    source: &'static str,
    claim_id: Option<&'a str>,
    state: &'static str,
    meaning: &'a str,
    next: &'a str,
    found: Option<bool>,
    is_free_prompt: Option<bool>,
    phase: Option<&'a str>,
    void_reason: Option<&'a str>,
    phase_daa: Option<u64>,
    accepted_daa: Option<u64>,
    accepted_block: Option<&'a str>,
    quanta: Option<u32>,
    quanta_spent: Option<u32>,
    executor_bond: Option<&'a str>,
    class_id: Option<&'a str>,
    work_leaves: Option<u64>,
    trace_retention_daa: Option<u64>,
    derived_count: Option<u32>,
    job_file: Option<String>,
    committed: Option<bool>,
    not_committed_because: Option<&'a str>,
    commit_by_anchor_daa: Option<u64>,
    decode_tokens_executed: Option<u64>,
    submitted_txid: Option<&'a str>,
}

fn entry_json(e: &Entry) -> EntryJson<'_> {
    let held = e.chain.as_ref().filter(|r| r.found);
    let row = e.outbox.as_ref();
    EntryJson {
        source: match e.source {
            Source::Named => "argument",
            Source::Outbox => "outbox",
        },
        claim_id: e.claim_id.as_deref(),
        state: e.reading.state,
        meaning: &e.reading.meaning,
        next: &e.reading.next,
        found: e.chain.as_ref().map(|r| r.found),
        is_free_prompt: held.map(|r| r.is_free_prompt),
        phase: held.map(|r| r.phase.as_str()),
        void_reason: held.map(|r| r.void_reason.as_str()).filter(|reason| !reason.is_empty()),
        phase_daa: held.map(|r| r.phase_daa),
        accepted_daa: held.map(|r| r.accepted_daa),
        accepted_block: held.map(|r| r.accepted_block.as_str()),
        quanta: held.map(|r| r.quanta),
        quanta_spent: held.map(|r| r.quanta_spent),
        executor_bond: held.map(|r| r.executor_bond.as_str()),
        class_id: held.map(|r| r.class_id.as_str()),
        // The chain's price when it holds the claim; the gateway's own count otherwise.
        work_leaves: held.map(|r| r.work_leaves).or_else(|| row.and_then(|o| o.work_leaves)),
        trace_retention_daa: held.map(|r| r.trace_retention_daa),
        derived_count: held.map(|r| r.derived_count),
        job_file: row.map(|o| o.job_file.display().to_string()),
        committed: row.and_then(|o| o.committed),
        not_committed_because: row.and_then(|o| o.not_committed_because.as_deref()),
        commit_by_anchor_daa: row.and_then(|o| o.commit_by_anchor_daa),
        decode_tokens_executed: row.and_then(|o| o.decode_tokens_executed),
        submitted_txid: row.and_then(|o| o.submitted_txid.as_deref()),
    }
}

/// The first 16 hex of an id and an ellipsis; the whole id when it is that short already.
fn short(id: &str) -> String {
    if id.chars().count() > 16 { format!("{}…", id.chars().take(16).collect::<String>()) } else { id.to_string() }
}

/// The human view: the node's DAA and the windows once, then one block per row, then a tally.
fn render_human(h: &Header<'_>, entries: &[Entry]) -> String {
    let mut lines = vec![format!(
        "node at DAA {} on {}{}",
        h.now,
        h.network,
        if h.synced { "" } else { " (NOT synced: its view of the chain may be behind)" }
    )];
    lines.push(match &h.windows {
        Some(w) => format!(
            "windows (DAA, this network's ConsensusV2 bundle): anchor_delay {}, bind {}, receipt {}, challenge {}, disclose {}, \
             receipt_maturity {}, receipt_use {}, abandon_hold {}, claim_retirement {}",
            w.anchor_delay,
            w.window_bind,
            w.window_receipt,
            w.window_challenge,
            w.disclose_window,
            w.receipt_maturity,
            w.receipt_use_window,
            w.fp_abandon_hold,
            w.claim_retirement
        ),
        None => format!("windows: none ({NO_WINDOWS})"),
    });
    if let Some(dir) = h.outbox {
        let jobs = entries.iter().filter(|e| e.source == Source::Outbox).count();
        lines.push(format!("outbox {}: {jobs} job(s), oldest first", dir.display()));
    }
    for entry in entries {
        lines.push(String::new());
        render_entry(entry, &mut lines);
    }
    if entries.len() > 1 {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for entry in entries {
            match counts.iter_mut().find(|(state, _)| *state == entry.reading.state) {
                Some((_, n)) => *n += 1,
                None => counts.push((entry.reading.state, 1)),
            }
        }
        lines.push(String::new());
        lines.push(format!("summary: {}", counts.iter().map(|(state, n)| format!("{n} {state}")).collect::<Vec<_>>().join(", ")));
    }
    lines.join("\n")
}

fn render_entry(e: &Entry, lines: &mut Vec<String>) {
    let head = match (&e.claim_id, &e.outbox) {
        (Some(claim), _) => format!("claim {}", short(claim)),
        (None, Some(row)) => format!("job {}", row.stem),
        (None, None) => "claim ?".to_string(),
    };
    lines.push(format!("{head}  [{}]", e.reading.state));
    if let Some(row) = &e.outbox {
        lines.push(format!("  job        {}", row.job_file.display()));
        if let Some(txid) = &row.submitted_txid {
            lines.push(format!("  carrier    {txid}"));
        }
    }
    if let Some(r) = e.chain.as_ref().filter(|r| r.found) {
        let reason = if r.void_reason.is_empty() { String::new() } else { format!(" ({})", r.void_reason) };
        // 0 is what the node reports while provisional: no phase has been entered yet.
        let at = if r.phase_daa == 0 { String::new() } else { format!(" at DAA {}", r.phase_daa) };
        lines.push(format!("  phase      {}{reason}{at}", r.phase));
        lines.push(format!("  accepted   DAA {} in block {}", r.accepted_daa, short(&r.accepted_block)));
        lines.push(if r.is_free_prompt {
            format!("  quanta     {} of {} spent", r.quanta_spent, r.quanta)
        } else {
            "  quanta     none (an attempt-lane claim)".to_string()
        });
        lines.push(format!("  bond       {}", r.executor_bond));
        lines.push(format!("  class      {}", short(&r.class_id)));
    }
    lines.push(format!("  meaning    {}", e.reading.meaning));
    lines.push(format!("  next       {}", e.reading.next));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// testnet-11-sized windows with every one DIFFERENT, so a date computed off the wrong field is
    /// a different number. testnet-11 itself has bind, receipt, use window and abandon hold all at
    /// 600 (and disclose equal to challenge), and a test on those numbers could not tell them apart.
    fn lattice() -> Windows {
        Windows {
            anchor_delay: 21,
            window_bind: 601,
            window_receipt: 602,
            window_challenge: 1_203,
            disclose_window: 1_204,
            receipt_maturity: 405,
            receipt_use_window: 606,
            fp_abandon_hold: 607,
            claim_retirement: 3_008,
        }
    }

    fn claim(phase: &str, phase_daa: u64) -> GetPalwFreePromptClaimResponse {
        GetPalwFreePromptClaimResponse {
            found: true,
            claim_id: "ab".repeat(64),
            is_free_prompt: true,
            class_id: "cd".repeat(64),
            executor_bond: format!("{}:0", "ef".repeat(64)),
            phase: phase.to_string(),
            phase_daa,
            accepted_daa: 10_000,
            accepted_block: "12".repeat(64),
            quanta: 8,
            ..Default::default()
        }
    }

    /// **Every phase the node can name reads as itself, dated in the network's own windows.**
    ///
    /// The date each row must carry is the phase's own deadline, computed here by hand from a
    /// lattice whose windows all differ, so a window read off the wrong field is a failing row,
    /// not a plausible-looking number.
    #[test]
    fn every_phase_the_node_can_name_reads_as_itself_with_its_own_deadline() {
        let w = lattice();
        let now = 10_300;
        for (phase, phase_daa, date) in [
            ("provisional", 0, "DAA 10601 (in 301 DAA)"),            // accepted + bind
            ("panel_bound", 10_050, "DAA 10652 (in 352 DAA)"),       // bound + receipt
            ("receipt_licensed", 10_200, "DAA 11403 (in 1103 DAA)"), // licensed + challenge
            ("final", 11_400, "DAA 11805 (in 1505 DAA)"),            // final + receipt_maturity
            ("default_disputed", 10_250, "DAA 11454 (in 1154 DAA)"), // accused + disclose
            ("voided", 10_200, "DAA 10807 (in 507 DAA)"),            // voided + abandon hold
        ] {
            let mut r = claim(phase, phase_daa);
            if phase == "voided" {
                r.void_reason = "bind_timeout".to_string();
            }
            let reading = explain(&r, Some(&w), now, None);
            assert_eq!(reading.state, phase, "{reading:?}");
            assert!(reading.meaning.contains(date), "{phase} must be dated {date}: {reading:?}");
            assert!(!reading.next.is_empty(), "{phase} names no next step");
            assert!(!reading.meaning.contains(NO_WINDOWS), "{phase} had windows and said it had none");
        }
    }

    /// Past its anchor slot and still unbound is the registry, not the clock; past its FIRST bind
    /// window and still provisional is the redraw — whose base the RPC does not carry, so it is
    /// named rather than dated from the wrong one.
    #[test]
    fn a_provisional_claim_says_which_wait_it_is_in() {
        let w = lattice();
        let early = explain(&claim("provisional", 0), Some(&w), 10_010, None);
        assert!(early.meaning.contains("at or after DAA 10021"), "{early:?}");
        let unseated = explain(&claim("provisional", 0), Some(&w), 10_300, None);
        assert!(unseated.meaning.contains("cannot seat a full panel"), "{unseated:?}");
        let redraw = explain(&claim("provisional", 0), Some(&w), 10_700, None);
        assert!(redraw.meaning.contains("redraw") && !redraw.meaning.contains("(in "), "{redraw:?}");
        // A bound panel is NOT sorted into first or second by its dates — an accusation's pause
        // shifts `bound_daa`, so a late first panel exists. Both readings state the rule instead.
        for bound in [10_050, 10_700] {
            let reading = explain(&claim("panel_bound", bound), Some(&w), 10_800, None);
            assert!(reading.meaning.contains("one redraw") && reading.meaning.contains("redrawn already"), "{reading:?}");
            assert!(!reading.meaning.contains("second panel"), "{bound}: a first panel was called the redraw's: {reading:?}");
        }
    }

    /// **Each void reason says who paid**: the timeouts slash nobody, the convictions take the
    /// claim's reserved collateral — and a reason this build cannot name is named, not guessed.
    #[test]
    fn every_void_reason_says_who_paid_for_it() {
        let w = lattice();
        for (reason, must, must_not) in [
            ("bind_timeout", "Nothing was slashed", "was slashed from"),
            ("receipt_timeout", "not slashed", "was slashed from"),
            ("court_fraud", "was slashed from the bond", "not slashed"),
            ("producer_withholding", "was slashed from the bond", "not slashed"),
        ] {
            let mut r = claim("voided", 11_000);
            r.void_reason = reason.to_string();
            let reading = explain(&r, Some(&w), 11_100, None);
            assert_eq!(reading.state, "voided");
            assert!(reading.meaning.contains(reason) && reading.meaning.contains(must), "{reason}: {reading:?}");
            assert!(!reading.meaning.contains(must_not), "{reason}: {reading:?}");
            // voided + claim_retirement: after that the record is gone, and the row says so now.
            assert!(reading.meaning.contains("DAA 14008"), "{reason}: {reading:?}");
        }
        let mut hold = claim("voided", 11_000);
        hold.void_reason = "bind_timeout".to_string();
        assert!(explain(&hold, Some(&w), 11_100, None).meaning.contains("reserved on the bond until DAA 11607"));
        hold.is_free_prompt = false;
        assert!(!explain(&hold, Some(&w), 11_100, None).meaning.contains("reserved"), "the abandon hold is the free-prompt lane's");
        let receipt = {
            let mut r = claim("voided", 11_000);
            r.void_reason = "receipt_timeout".to_string();
            explain(&r, Some(&w), 11_100, None)
        };
        assert!(receipt.next.contains("--palw-panel") && receipt.next.contains("refused an opening request"), "{receipt:?}");
        let mut odd = claim("voided", 11_000);
        odd.void_reason = "gamma_ray".to_string();
        let reading = explain(&odd, Some(&w), 11_100, None);
        assert!(reading.meaning.contains("'gamma_ray'") && reading.meaning.contains("does not know"), "{reading:?}");
        assert!(!reading.meaning.contains("slashed"), "an unknown reason must not be assigned a payer: {reading:?}");
    }

    /// **An unknown phase is named and nothing is inferred from it** — neither a deadline nor a
    /// next step that belongs to some other phase.
    #[test]
    fn an_unknown_phase_is_named_and_nothing_is_inferred() {
        for phase in ["zombie", "", "Final"] {
            let reading = explain(&claim(phase, 12_345), Some(&lattice()), 12_400, None);
            assert_eq!(reading.state, "unknown_phase", "{phase:?}");
            assert!(reading.meaning.contains(&format!("'{phase}'")), "{reading:?}");
            for inferred in ["DAA 12345", "--palw-produce", "slashed", "quanta", "final at"] {
                assert!(!reading.meaning.contains(inferred) && !reading.next.contains(inferred), "{phase:?} inferred {inferred}");
            }
        }
    }

    /// Not found names both facts the RPC folds into one `false`, the node's own log lines for the
    /// first, and the carrier when the caller knows it.
    #[test]
    fn a_claim_the_chain_does_not_hold_points_at_where_its_reason_was_written() {
        let missing = GetPalwFreePromptClaimResponse { found: false, claim_id: "ab".repeat(64), ..Default::default() };
        let txid = "9f".repeat(64);
        let reading = explain(&missing, Some(&lattice()), 20_000, Some(&txid));
        assert_eq!(reading.state, "not_on_chain");
        assert!(reading.meaning.contains("3008 DAA ago has been retired"), "{reading:?}");
        for line in ["PALW lifecycle object was dropped", "above its exposure ceiling", "FreePromptExposureCeiling"] {
            assert!(reading.next.contains(line), "{line}: {reading:?}");
        }
        assert!(reading.next.contains(&format!("[palw-fp] carrier {txid} produced no object")), "{reading:?}");
        let unknown_carrier = explain(&missing, None, 20_000, None);
        assert!(unknown_carrier.next.contains("carrier <txid>") && unknown_carrier.meaning.contains(NO_WINDOWS));
    }

    /// Submitted and not mined yet is its own state: the first blocks after every submit look like
    /// this, and reading them as a drop would tell the operator to stop exactly when to wait.
    #[test]
    fn a_carrier_still_in_the_mempool_is_waiting_not_dropped() {
        let txid = "9f".repeat(64);
        let reading = in_mempool(&txid);
        assert_eq!(reading.state, "in_mempool");
        assert!(reading.meaning.contains(&txid), "{reading:?}");
        assert!(reading.next.contains("provisional") && !reading.next.contains("dropped"), "{reading:?}");
    }

    /// Without a bundle nothing is dated: the phase still reads as itself, and says why it is
    /// undated instead of quoting another network's numbers.
    #[test]
    fn without_a_bundle_no_window_is_quoted() {
        for phase in ["provisional", "panel_bound", "receipt_licensed", "final", "default_disputed"] {
            let reading = explain(&claim(phase, 10_050), None, 10_300, None);
            assert_eq!(reading.state, phase);
            assert!(reading.meaning.contains(NO_WINDOWS), "{phase}: {reading:?}");
            assert!(!reading.meaning.contains("(in ") && !reading.meaning.contains(" ago)"), "{phase} dated something: {reading:?}");
        }
    }

    /// `final` is where the money is and it is not automatic: the row names the bond whose producer
    /// must spend, counts what is spent, and says so plainly once nothing is left.
    #[test]
    fn a_final_claim_names_the_producer_that_must_spend_it() {
        let mut r = claim("final", 11_400);
        r.quanta_spent = 3;
        let reading = explain(&r, Some(&lattice()), 11_500, None);
        assert!(reading.meaning.contains("3 of 8 quanta spent") && reading.meaning.contains("within 606 DAA"), "{reading:?}");
        assert!(reading.next.contains("--palw-produce") && reading.next.contains(&r.executor_bond), "{reading:?}");
        r.quanta_spent = 8;
        assert_eq!(explain(&r, Some(&lattice()), 11_500, None).next, "nothing left: every quantum is spent");
        r.is_free_prompt = false;
        let attempt = explain(&r, Some(&lattice()), 11_500, None);
        assert!(attempt.meaning.contains("attempt-lane") && !attempt.next.contains("--palw-produce"), "{attempt:?}");
    }

    /// A scratch directory of this test's own, so two tests (or two runs) never share files.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("misaka-palw-claim-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(dir: &Path, name: &str, body: &str, age: u64) {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + age)).unwrap();
    }

    fn summary(claim: char, committed: bool, because: Option<&str>) -> String {
        serde_json::json!({
            "schema": GATEWAY_SCHEMA,
            "fp_claim_id": claim.to_string().repeat(128),
            "committed": committed,
            "not_committed_because": because,
            "commit_by_anchor_daa": 5_000,
            "work_leaves": 77,
            "decode_tokens_executed": 12,
            "answer_untrimmed": "the answer, which this command never prints",
        })
        .to_string()
    }

    /// **An outbox is read job by job, oldest first, and each job says how far it got.**
    ///
    /// One job per state, plus the files that belong to a job without saying where it got to, a
    /// directory named like a job, and a stranger's file — so a classifier keyed to the first file
    /// a listing returns, or to a file name instead of a stem, fails a row here.
    #[test]
    fn an_outbox_is_read_job_by_job_oldest_first() {
        let dir = scratch("outbox");
        let txid = "aa".repeat(64);
        put(&dir, "fp-job-1111111111111111.json", &summary('1', false, Some("the bond's exposure ceiling is full")), 1);
        put(&dir, "fp-job-1111111111111111.result.borsh", "x", 1);
        put(&dir, "fp-job-1111111111111111.derived.json", "{\"claim_id\": \"1\"}", 1);
        put(&dir, "fp-job-2222222222222222.json", &summary('2', true, None), 2);
        put(&dir, "fp-job-2222222222222222.commitment-unsigned.borsh", "x", 2);
        put(&dir, "fp-job-2222222222222222.commitment-tx.borsh", "x", 2);
        put(&dir, "fp-job-3333333333333333.json", &summary('3', true, None), 3);
        put(
            &dir,
            "fp-job-3333333333333333.rail.json",
            "{\"fp_claim_id\": \"x\", \"submitted\": null, \"tx_file\": \"/o/t.borsh\"}",
            3,
        );
        // The rail rewrote the class (`--class-id`), so the claim it signed is not the gateway's.
        put(&dir, "fp-job-4444444444444444.json", &summary('4', true, None), 4);
        let rail = serde_json::json!({ "fp_claim_id": "5".repeat(128), "submitted": txid }).to_string();
        put(&dir, "fp-job-4444444444444444.rail.json", &rail, 4);
        // A stale marker beside a submitted job: the transaction id outranks it.
        put(&dir, "fp-job-4444444444444444.commitment-unsigned.borsh.submit-failed", "x", 4);
        put(&dir, "fp-job-6666666666666666.json", &summary('6', true, None), 5);
        put(&dir, "fp-job-6666666666666666.commitment-unsigned.borsh.expired", "x", 5);
        put(&dir, "fp-job-6666666666666666.commitment-tx.borsh.submit-failed", "x", 5);
        put(&dir, "fp-job-7777777777777777.json", &summary('7', true, None), 6);
        put(&dir, "fp-job-7777777777777777.commitment-unsigned.borsh.submit-failed", "x", 6);
        // A job whose summary survives only as a renamed marker still names its claim.
        put(&dir, "fp-job-8888888888888888.json.submit-failed", &summary('8', true, None), 7);
        put(&dir, "fp-job-9999999999999999.json", "{ not json", 8);
        put(&dir, "fp-job-bbbbbbbbbbbbbbbb.json", "{\"schema\": \"somebody.else.v1\"}", 9);
        put(&dir, "fp-job-cccccccccccccccc.result.borsh", "x", 0);
        put(&dir, "README", "x", 0);
        std::fs::create_dir(dir.join("traces")).unwrap();
        std::fs::create_dir(dir.join("fp-job-dddddddddddddddd.json")).unwrap();

        let rows = scan_outbox(&dir).unwrap();
        let stems: Vec<&str> = rows.iter().map(|r| r.stem.as_str()).collect();
        assert_eq!(
            stems,
            [
                "fp-job-1111111111111111",
                "fp-job-2222222222222222",
                "fp-job-3333333333333333",
                "fp-job-4444444444444444",
                "fp-job-6666666666666666",
                "fp-job-7777777777777777",
                "fp-job-8888888888888888",
                "fp-job-9999999999999999",
                "fp-job-bbbbbbbbbbbbbbbb",
            ],
            "one row per job that says anything, oldest first"
        );
        assert_eq!(rows[0].state, OutboxState::NotCommitted);
        assert_eq!(rows[0].not_committed_because.as_deref(), Some("the bond's exposure ceiling is full"));
        assert_eq!(
            rows[1].state,
            OutboxState::CommittedNotSubmitted { signed_tx: Some(dir.join("fp-job-2222222222222222.commitment-tx.borsh")) }
        );
        assert_eq!(rows[1].commit_by_anchor_daa, Some(5_000));
        assert_eq!(rows[2].state, OutboxState::SignedNotSubmitted { tx_file: Some(PathBuf::from("/o/t.borsh")) });
        assert_eq!(rows[3].state, OutboxState::Submitted);
        assert_eq!(rows[3].submitted_txid.as_deref(), Some(txid.as_str()));
        assert_eq!(rows[3].claim_id, Some("5".repeat(128)), "the claim the rail signed is the claim on the chain");
        assert_eq!(
            rows[4].state,
            OutboxState::Expired { marker: dir.join("fp-job-6666666666666666.commitment-unsigned.borsh.expired"), gave_up_too: true }
        );
        assert_eq!(
            rows[5].state,
            OutboxState::GaveUp { marker: dir.join("fp-job-7777777777777777.commitment-unsigned.borsh.submit-failed") }
        );
        assert_eq!(rows[6].state, OutboxState::GaveUp { marker: dir.join("fp-job-8888888888888888.json.submit-failed") });
        assert_eq!(rows[6].claim_id, Some("8".repeat(128)), "a renamed summary still names its claim");
        assert!(matches!(&rows[7].state, OutboxState::Unreadable { why } if why.contains("not JSON")), "{:?}", rows[7].state);
        assert!(matches!(&rows[8].state, OutboxState::Unreadable { why } if why.contains("somebody.else.v1")), "{:?}", rows[8].state);

        // Each state reads as itself; only the submitted job is left for the chain to answer.
        let states: Vec<Option<&str>> = rows.iter().map(|r| outbox_reading(r, &dir, 4_000).map(|reading| reading.state)).collect();
        assert_eq!(
            states,
            [
                Some("not_committed"),
                Some("committed_not_submitted"),
                Some("signed_not_submitted"),
                None,
                Some("expired"),
                Some("gave_up"),
                Some("gave_up"),
                Some("unreadable"),
                Some("unreadable"),
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Only an unreadable DIRECTORY fails the outbox, and it fails by name.
    #[test]
    fn an_outbox_that_cannot_be_read_is_the_one_outbox_failure() {
        // Never created, so there is nothing to clean up afterwards.
        let missing = std::env::temp_dir().join(format!("misaka-palw-claim-{}-absent", std::process::id())).join("not-here");
        let e = scan_outbox(&missing).unwrap_err();
        assert_eq!(e.code, exit::GENERIC);
        assert!(e.msg.contains("not-here"), "{}", e.msg);
    }

    /// **A job that never reached the chain says how far it got and what is left** — with the
    /// submitter's own comparison, so a row never offers a command the submitter would refuse.
    #[test]
    fn a_job_off_the_chain_offers_only_commands_that_can_still_work() {
        let outbox = Path::new("/srv/outbox");
        let row = |state: OutboxState| OutboxRow {
            job_file: outbox.join("fp-job-2222222222222222.json"),
            stem: "fp-job-2222222222222222".to_string(),
            modified: None,
            claim_id: Some("2".repeat(128)),
            committed: Some(true),
            not_committed_because: None,
            commit_by_anchor_daa: Some(5_000),
            work_leaves: None,
            decode_tokens_executed: None,
            submitted_txid: None,
            state,
        };
        let queued = row(OutboxState::CommittedNotSubmitted { signed_tx: None });
        let artifact = format!("--artifact {}", outbox.join("fp-job-2222222222222222").display());
        let watch = format!("--watch {}", outbox.display());
        let fresh = outbox_reading(&queued, outbox, 4_000).unwrap();
        assert_eq!(fresh.state, "committed_not_submitted");
        assert!(fresh.meaning.starts_with("committed, NOT submitted") && fresh.meaning.contains("DAA 5000 (in 1000 DAA)"));
        assert!(fresh.next.contains(&artifact) && fresh.next.contains(&watch), "{fresh:?}");
        // AT the deadline it is still fresh — the gateway's sweep and the submitter both say `>`.
        assert!(outbox_reading(&queued, outbox, 5_000).unwrap().next.contains(&artifact));
        let late = outbox_reading(&queued, outbox, 5_001).unwrap();
        assert!(late.meaning.contains("lapsed at DAA 5000") && !late.next.contains(&artifact), "{late:?}");
        let signed = outbox_reading(&row(OutboxState::SignedNotSubmitted { tx_file: None }), outbox, 4_000).unwrap();
        assert!(signed.meaning.starts_with("signed, not submitted") && signed.next.contains("--submit"), "{signed:?}");
        let gave_up = row(OutboxState::GaveUp { marker: outbox.join("fp-job-2222222222222222.json.submit-failed") });
        let reading = outbox_reading(&gave_up, outbox, 4_000).unwrap();
        assert!(reading.meaning.contains("exposure ceiling") && reading.next.contains(&artifact), "{reading:?}");
        assert!(!outbox_reading(&gave_up, outbox, 5_001).unwrap().next.contains(&artifact), "a lapsed job is offered no command");
        let expired = row(OutboxState::Expired { marker: outbox.join("x.expired"), gave_up_too: false });
        let reading = outbox_reading(&expired, outbox, 5_001).unwrap();
        assert!(reading.meaning.starts_with("expired") && reading.next.contains(&watch) && !reading.next.contains(&artifact));
        let mut declined = row(OutboxState::NotCommitted);
        declined.committed = Some(false);
        declined.not_committed_because = Some("the chain is not healthy".to_string());
        let reading = outbox_reading(&declined, outbox, 4_000).unwrap();
        assert!(reading.meaning.starts_with("answered, not committed: the chain is not healthy"), "{reading:?}");
        assert!(outbox_reading(&row(OutboxState::Submitted), outbox, 4_000).is_none(), "a submitted job is the chain's to answer");
    }

    /// **Both views carry every row**: the human one as one block per row under the node's DAA,
    /// the JSON one with every key on every row and the chain's fields `null` where the chain
    /// holds nothing — never a default that reads like a value.
    #[test]
    fn both_views_carry_every_row() {
        let (w, now, outbox) = (lattice(), 11_500, Path::new("/srv/outbox"));
        let mut held = claim("final", 11_400);
        held.quanta_spent = 3;
        let missing = GetPalwFreePromptClaimResponse { found: false, claim_id: "cd".repeat(64), ..Default::default() };
        let declined = OutboxRow {
            job_file: outbox.join("fp-job-1111111111111111.json"),
            stem: "fp-job-1111111111111111".to_string(),
            modified: None,
            claim_id: None,
            committed: Some(false),
            not_committed_because: Some("the chain is not healthy".to_string()),
            commit_by_anchor_daa: None,
            work_leaves: Some(77),
            decode_tokens_executed: Some(12),
            submitted_txid: None,
            state: OutboxState::NotCommitted,
        };
        let (held_reading, missing_reading) = (explain(&held, Some(&w), now, None), explain(&missing, Some(&w), now, None));
        let declined_reading = outbox_reading(&declined, outbox, now).unwrap();
        let entries = vec![
            Entry {
                source: Source::Named,
                claim_id: Some(held.claim_id.clone()),
                chain: Some(held),
                outbox: None,
                reading: held_reading,
            },
            Entry {
                source: Source::Named,
                claim_id: Some(missing.claim_id.clone()),
                chain: Some(missing),
                outbox: None,
                reading: missing_reading,
            },
            Entry { source: Source::Outbox, claim_id: None, chain: None, outbox: Some(declined), reading: declined_reading },
        ];
        let header = Header { network: "testnet-11".to_string(), now, synced: true, windows: Some(w), outbox: Some(outbox) };

        let human = render_human(&header, &entries);
        for line in [
            "node at DAA 11500 on testnet-11",
            "claim abababababababab…  [final]",
            "  phase      final at DAA 11400",
            "  quanta     3 of 8 spent",
            "claim cdcdcdcdcdcdcdcd…  [not_on_chain]",
            "job fp-job-1111111111111111  [not_committed]",
            "summary: 1 final, 1 not_on_chain, 1 not_committed",
        ] {
            assert!(human.lines().any(|l| l.starts_with(line)), "missing {line:?} in:\n{human}");
        }
        assert_eq!(human.matches("\n  phase ").count(), 1, "only a claim the chain holds has a phase line:\n{human}");
        assert_eq!(human.matches("\n  meaning ").count(), 3, "{human}");

        let doc = document(&header, &entries);
        assert_eq!(doc["schema"], "misaka.palw.claims.v1");
        assert_eq!(doc["daa_score"], 11_500);
        assert_eq!(doc["windows"]["window_bind"], 601);
        let claims = doc["claims"].as_array().unwrap();
        assert_eq!(claims.len(), 3);
        assert_eq!(claims[0]["claim_id"], "ab".repeat(64), "JSON carries the whole id");
        assert_eq!((claims[0]["state"].as_str(), claims[0]["quanta_spent"].as_u64()), (Some("final"), Some(3)));
        assert_eq!(claims[1]["found"], false);
        assert!(claims[1]["phase"].is_null() && claims[1]["quanta"].is_null(), "no default may read like a value: {}", claims[1]);
        assert!(claims[2]["found"].is_null(), "the chain was never asked about a job that made no claim");
        assert_eq!((claims[2]["committed"].as_bool(), claims[2]["work_leaves"].as_u64()), (Some(false), Some(77)));
        let keys = |i: usize| claims[i].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(0), keys(2), "every key on every row");
    }

    /// Only a NAMED claim missing from the chain fails the command; an outbox row never does.
    #[test]
    fn only_a_named_claim_missing_from_the_chain_fails_the_command() {
        let entry = |source: Source, found: Option<bool>| Entry {
            source,
            claim_id: Some("ab".repeat(64)),
            chain: found.map(|found| GetPalwFreePromptClaimResponse { found, ..Default::default() }),
            outbox: None,
            reading: Reading { state: "x", meaning: String::new(), next: String::new() },
        };
        assert!(verdict(&[entry(Source::Outbox, Some(false)), entry(Source::Outbox, None)]).is_ok(), "an outbox is a history");
        assert!(verdict(&[entry(Source::Named, Some(true)), entry(Source::Outbox, Some(false))]).is_ok());
        let e = verdict(&[entry(Source::Named, Some(true)), entry(Source::Named, Some(false))]).unwrap_err();
        assert_eq!(e.code, exit::GENERIC);
        assert!(e.msg.contains(&"ab".repeat(64)), "{}", e.msg);
    }

    /// **The command needs a claim or an outbox, takes both, and refuses neither by name** — at the
    /// parser, and again in the handler for a caller that is not clap.
    #[test]
    fn the_command_needs_a_claim_or_an_outbox_and_takes_both() {
        use clap::Parser;
        let id = "ab".repeat(64);
        let empty = crate::Cli::try_parse_from(["misaka", "palw", "claim"]).unwrap_err().to_string();
        assert!(empty.contains("--outbox") && empty.contains("CLAIM_ID"), "{empty}");
        let both =
            crate::Cli::try_parse_from(["misaka", "palw", "claim", id.as_str(), id.as_str(), "--outbox", "/o", "--json"]).unwrap();
        match both.command {
            crate::Command::Palw(crate::PalwCmd::Claim { claim_ids, outbox, json }) => {
                assert_eq!(claim_ids.len(), 2);
                assert_eq!(outbox.as_deref(), Some(Path::new("/o")));
                assert!(json);
            }
            other => panic!("parsed as {other:?}"),
        }
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "claim", "--outbox", "/o"]).is_ok());
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "claim", id.as_str()]).is_ok());
        assert!(something_named(&[], None).unwrap_err().msg.contains("--outbox"));
        assert!(something_named(&[id], None).is_ok());
    }

    /// **The windows quoted are the network's own**: testnet-11's are the RC lattice its bundle is
    /// built from, and a network without a bundle is quoted none.
    #[test]
    fn the_windows_are_read_off_the_networks_own_bundle() {
        use kaspa_consensus_core::network::NetworkId;
        use std::str::FromStr;
        let t11 = Windows::of(&Params::from(NetworkId::from_str("testnet-11").unwrap())).expect("testnet-11 runs ConsensusV2");
        let rc = kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
        assert_eq!(
            (t11.anchor_delay, t11.window_bind, t11.window_receipt, t11.window_challenge),
            (rc.anchor_delay, rc.window_bind, rc.window_receipt, rc.window_challenge)
        );
        assert_eq!(
            (t11.receipt_maturity, t11.receipt_use_window, t11.fp_abandon_hold, t11.claim_retirement),
            (rc.receipt_maturity, rc.receipt_use_window, rc.fp_abandon_hold, rc.claim_retirement)
        );
        assert_eq!(t11.disclose_window, rc.window_challenge, "W_disclose is the challenge window by derivation");
        assert_eq!(Windows::of(&Params::from(NetworkId::from_str("testnet-10").unwrap())), None);
    }
}
