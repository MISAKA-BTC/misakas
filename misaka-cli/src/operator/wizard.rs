//! **`misaka mining setup`, `misaka verifier setup` and `misaka init`** — ADR-0122 Decision 7.
//!
//! Setup discovers instead of asking, and it discovers again instead of remembering. Every step's
//! "done when" is a fact on disk (the key file) or on the chain (a bond registered to the key, an
//! output at its address, the classes the bond declared), so an interrupted setup is resumed by
//! running it again: the finished steps read ✓ and the first unfinished one is where it waits.
//! The one thing it keeps is the node it started (`~/.misaka/<network>/run/setup.json`), so a node
//! an interrupted run left behind is adopted, not mistaken for one the operator runs.
//!
//! Nothing is spent without a yes. Registering a bond locks collateral; declaring capability and
//! splitting off a fee output each pay a fee. Each shows the move and asks. `--yes` answers for a
//! script; without a terminal and without `--yes`, setup stops at the question and says so.
//!
//! The bond itself is registered by `kaspad --palw-register-bond` — the node's own builder, with
//! everything it knows about relaying and naming a carrier — on a node setup starts for the
//! purpose, and always with `--palw-bond-collateral` set to the figure setup showed and the
//! operator confirmed. That figure is the purpose's: a producer's bond locks what the node sizes for
//! the class's claims; a verifier seat's locks at least what the panel draw seats
//! ([`seat_requirement`]: ten producer floors, 130,000 BILI, on testnet-12). Setup reads the outcome
//! from that node's log and from the chain; nobody copies an outpoint off a log line.

use crate::operator::finding::{Finding, Severity, paint};
use crate::operator::nodelog::{self, RegistrationNote};
use crate::operator::profile::{
    AdvancedSection, ArtifactList, MiningSection, MiningToml, Overrides, Profile, ValidatorSection, ValidatorToml,
};
use crate::operator::snapshot::{self, KeyFacts, NodeRead};
use crate::operator::supervisor::{self, Cmd, Role};
use crate::operator::tty::{Answer, Halt, Row, Step, Ui};
use crate::operator::{catalog, host, procs, status};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::api::rpc::RpcApi;
use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Beyond the collateral, what a registration's one funding output must hold: the carrier's fee
/// and a change output large enough to relay (the join doc's "+0.1 BILI": the smallest change a
/// carrier can leave is 0.0833 BILI, and the fee is a few hundred thousand sompi).
const REGISTRATION_MARGIN_SOMPI: u64 = 10_000_000;
/// The registration's change becomes the panel's fee float; this much keeps it paying for a while.
const FLOAT_RECOMMENDED_SOMPI: u64 = 50_000_000;
/// The least a fee outpoint should hold (ADR-0122 §7).
const FEE_OUTPOINT_MIN_SOMPI: u64 = 10_000_000;
/// What a self-send splits off to make a fee float when the key's address has none.
const FEE_FLOAT_SPLIT_SOMPI: u64 = 50_000_000;
const DOCS_SETUP: &str = "docs/adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md#8-d7--setup-resumable-and-it-discovers-instead-of-asking";

/// What this machine is being set up to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Purpose {
    Mine,
    Verify,
    /// The DNS-finality validator (ADR-0010's overlay): a stake bond and attestations, not PALW.
    Validate,
}

impl Purpose {
    fn title(self) -> &'static str {
        match self {
            Purpose::Mine => "mining setup",
            Purpose::Verify => "verifier setup",
            Purpose::Validate => "validator setup",
        }
    }
    fn start(self) -> &'static str {
        match self {
            Purpose::Mine => "misaka mining start",
            Purpose::Verify => "misaka verifier start",
            Purpose::Validate => "misaka validator status",
        }
    }
    fn role(self) -> Role {
        match self {
            Purpose::Mine => Role::Miner,
            Purpose::Verify | Purpose::Validate => Role::Verifier,
        }
    }
    fn noun(self) -> &'static str {
        match self {
            Purpose::Mine => "mining",
            Purpose::Verify => "verifier",
            Purpose::Validate => "validator",
        }
    }
}

/// Every flag setup takes. Each is optional: what it does not say is discovered, or asked.
#[derive(Clone, Debug, Default)]
pub(crate) struct SetupArgs {
    pub(crate) network: Option<String>,
    pub(crate) rpc: Option<String>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) key_file: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) appdir: Option<String>,
    pub(crate) bond: Option<String>,
    pub(crate) peers: Vec<String>,
    pub(crate) artifacts: Vec<String>,
    /// `auto`, or `<txid>:<index>`.
    pub(crate) fee_outpoint: Option<String>,
    /// Kept as a compatibility flag. Setup now always verifies a non-base artifact before it
    /// writes the profile; accepting an unchecked path is what caused late startup failures.
    pub(crate) verify_artifact: bool,
    /// The validator's stake, in sompi (default: the network's minimum bond).
    pub(crate) amount: Option<u64>,
    pub(crate) yes: bool,
    pub(crate) no_wait: bool,
}

/// Explicit bootstrap peers when a network has no DNS discovery.
///
/// Testnet-11 has live DNS seeders in its consensus-independent network parameters. Keeping a
/// literal fleet IP here made the wizard keep dialing a retired or temporarily stopped host even
/// though kaspad could use the current seeder set. Return no forced peer and let kaspad perform
/// its normal DNS bootstrap; operators can still pass a freshly resolved IP with `--peer` for an
/// isolated environment.
pub(crate) fn default_peers(network: &str) -> Vec<String> {
    match network {
        "testnet-11" => Vec::new(),
        _ => Vec::new(),
    }
}

fn faucet(network: &str) -> Option<&'static str> {
    (network == "testnet-11").then_some("https://misakascan.com/#/faucet — 12 tMSK per address, once")
}

/// The node directory a new setup uses: a datadir already on disk wins (kaspad's default
/// `~/.rusty-kaspa`, when it holds this network), else `~/.misaka/<network>/node`.
fn default_appdir(network: &str, home: &Path) -> PathBuf {
    let rusty = home.join(".rusty-kaspa");
    if rusty.join(format!("misaka-{network}")).is_dir() { rusty } else { home.join(".misaka").join(network).join("node") }
}

// ---------------------------------------------------------------------------------------------
// what setup knows
// ---------------------------------------------------------------------------------------------

/// The node setup started, or adopted from a run it left behind.
struct OwnNode {
    /// `None` when adopted: the process is not this one's child.
    child: Option<supervisor::Child>,
    pid: u32,
    register: bool,
    /// The `--palw-bond-collateral` it registers with; `None` when it registers nothing, or was
    /// started without the flag (an earlier setup's node, sizing its own default).
    collateral: Option<u64>,
    started_unix: i64,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct SetupState {
    pid: u32,
    appdir: String,
    register: bool,
    started_unix: i64,
}

impl SetupState {
    fn path(network: &str) -> PathBuf {
        supervisor::run_dir(network).join("setup.json")
    }
    fn read(network: &str) -> Option<SetupState> {
        serde_json::from_slice(&std::fs::read(Self::path(network)).ok()?).ok()
    }
    fn write(&self, network: &str) {
        let _ = std::fs::create_dir_all(supervisor::run_dir(network));
        let _ = std::fs::write(Self::path(network), serde_json::to_vec_pretty(self).unwrap_or_default());
    }
    fn remove(network: &str) {
        let _ = std::fs::remove_file(Self::path(network));
    }
}

/// One class, as the chooser shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassChoice {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) is_base: bool,
    pub(crate) artifact_root: String,
    pub(crate) fp_certified: bool,
    pub(crate) share_permille: Option<u16>,
    /// What a PRODUCER's bond for it locks: `kaspad --palw-register-bond`'s own default for the
    /// class — its whole-lifetime sizing, never below the producer floor. `None` when the node
    /// reports no facts for it. `u64::MAX` is a saturated sizing, not an amount ([`bond_lock`]).
    pub(crate) collateral: Option<u64>,
    /// What a SEAT's bond for it must hold before the panel draw seats it ([`seat_requirement`]).
    pub(crate) seat_collateral: Option<u64>,
    /// Why the class takes no new claim on this chain, if it does not ([`class_closed_reason`]): no
    /// bond is sized for it in any role.
    pub(crate) closed: Option<&'static str>,
}

impl ClassChoice {
    pub(crate) fn label(&self) -> String {
        match (self.is_base, self.name.is_empty()) {
            (true, _) => "base (the floor)".to_string(),
            (false, false) => self.name.clone(),
            (false, true) => format!("{}…", &self.id[..8.min(self.id.len())]),
        }
    }

    /// What a bond for this class locks in `role`.
    pub(crate) fn lock(&self, role: BondRole) -> BondLock {
        bond_lock(role, self.collateral, self.seat_collateral, self.closed.is_none())
    }
}

/// **Why a class takes no new claim on this chain**, when it does not — read off its status and the
/// chain's own params, so the chooser says so instead of offering a bond for it:
///
/// * `Frozen` (a proven fault) and `Dormant` (reclaimed for producing nothing) classes take no new
///   work until they are registered again;
/// * past `palw_class_verify_deadline` (ADR-0152 §4-quater V2(a)) a class whose canonical job derives
///   a verification deadline past `window_receipt` takes no claim on any lane until a measured row
///   names it — `palw_class_needs_measured_row_v1`'s clause (i), the 2M row at testnet-12's launch
///   (U-D1: 13,995 DAA against 600). `verification_ccu` is the registry row's (`None`: no row, so no
///   class-derived deadline). Clause (ii), a held class with a published `n_ctx` past 8,192, needs a
///   profile no read serves, and a class only it closes reads by its sizing, as before.
pub(crate) fn class_closed_reason(
    params: &kaspa_consensus_core::config::params::Params,
    daa: u64,
    class_id: &str,
    status: &str,
    share_permille: Option<u16>,
    verification_ccu: Option<u128>,
) -> Option<&'static str> {
    use crate::operator::market::{ClassStage, class_stage};
    if matches!(class_stage(status, share_permille), ClassStage::Frozen { .. } | ClassStage::Dormant { .. }) {
        return Some("the class is Frozen or Dormant: it takes no new work until it is registered again");
    }
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return None;
    };
    let state = &bundle.state;
    let ccu = verification_ccu?;
    let measured =
        state.class_verify_rows().iter().any(|row| row.class_id.to_string().eq_ignore_ascii_case(class_id) && row.is_active_at(daa));
    (state.class_verify_deadline_active_at(daa)
        && !measured
        && kaspa_consensus_core::palw_class_verify_deadline_v1::palw_derived_verify_daa_v1(ccu) > state.window_receipt())
    .then_some(
        "the class takes no claim until a measured verification row names it (ADR-0152 §4-quater): its derived verification \
         deadline is past the chain's receipt window",
    )
}

/// **What a bond is registered to do** — and so what the chain asks of its collateral.
///
/// A producer's collateral backs its own claims for their whole life; a seat's is what the panel
/// draw reads before it seats the bond at all (ADR-0124 Decision 4). Where the chain states a panel
/// floor the two are different numbers — testnet-12: a 13,000 BILI producer floor and a 130,000 BILI
/// seat floor — and a bond meant to do both holds the larger. The registry takes one bond per key
/// for the life of the chain and its collateral cannot be topped up, so the figure is settled
/// before the lock, never after.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BondRole {
    /// It opens claims (`--palw-produce`), backed by this collateral.
    pub(crate) produce: bool,
    /// It sits on panels and judges other producers' claims.
    pub(crate) seat: bool,
}

impl BondRole {
    pub(crate) const PRODUCER: BondRole = BondRole { produce: true, seat: false };
    pub(crate) const SEAT: BondRole = BondRole { produce: false, seat: true };

    /// The role a setup registers its bond for: `mining setup` a producer's, `verifier setup` a
    /// seat's. (`validator setup` stakes a DNS-finality bond of its own and registers no PALW bond;
    /// it reads as a seat only so that this is total.)
    pub(crate) fn of(purpose: Purpose) -> BondRole {
        match purpose {
            Purpose::Mine => BondRole::PRODUCER,
            Purpose::Verify | Purpose::Validate => BondRole::SEAT,
        }
    }

    /// The role, as the confirmation names it.
    pub(crate) fn name(self) -> &'static str {
        match (self.produce, self.seat) {
            (true, true) => "producer and verifier seat",
            (true, false) => "producer",
            (false, _) => "verifier seat",
        }
    }
}

/// What the chooser and the setup rows say for a class no bond can be sized for.
pub(crate) const NOT_AVAILABLE: &str = "not available on this chain";

/// **What a bond for one class locks in one role** — an amount, or why there is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BondLock {
    /// A postable amount, in sompi.
    Sompi(u64),
    /// No bond can be sized for it on this chain: the chain's own sizing saturates (`u64::MAX`) or
    /// exceeds what one output may carry (`MAX_SOMPI`), or the class takes no new work.
    NotAvailable,
    /// The node reports no facts to size it from (yet).
    Unknown,
}

impl BondLock {
    pub(crate) fn sompi(self) -> Option<u64> {
        match self {
            BondLock::Sompi(sompi) => Some(sompi),
            BondLock::NotAvailable | BondLock::Unknown => None,
        }
    }

    /// The chooser's cell: an amount, or words — a saturated sizing is never printed as BILI.
    pub(crate) fn shown(self) -> String {
        match self {
            BondLock::Sompi(sompi) => catalog::msk(sompi as u128),
            BondLock::NotAvailable => NOT_AVAILABLE.to_string(),
            BondLock::Unknown => "unknown".to_string(),
        }
    }
}

/// **The figure a role locks for a class**: the larger of the figures the role takes — the
/// producer's for `produce`, the seat's for `seat`, both for a bond meant to do both. A figure no
/// output can carry is [`BondLock::NotAvailable`] (the 2M row's producer sizing is `u64::MAX`), and
/// one the node could not size is [`BondLock::Unknown`]; neither is ever offered as a lock.
pub(crate) fn bond_lock(role: BondRole, producer: Option<u64>, seat: Option<u64>, takes_work: bool) -> BondLock {
    if !takes_work {
        return BondLock::NotAvailable;
    }
    let taken: Vec<Option<u64>> =
        [(role.produce, producer), (role.seat, seat)].into_iter().filter(|(wanted, _)| *wanted).map(|(_, figure)| figure).collect();
    if taken.iter().flatten().any(|figure| *figure > kaspa_consensus_core::constants::MAX_SOMPI) {
        return BondLock::NotAvailable;
    }
    match taken.into_iter().collect::<Option<Vec<u64>>>().and_then(|figures| figures.into_iter().max()) {
        Some(need) => BondLock::Sompi(need),
        None => BondLock::Unknown,
    }
}

/// A fence the network states — in force now or scheduled — as opposed to absent or `never()`.
fn stated(fence: Option<kaspa_consensus_core::config::params::ForkActivation>) -> bool {
    fence.is_some_and(|f| f != kaspa_consensus_core::config::params::ForkActivation::never())
}

/// **What the panel draw asks of a seat bond's collateral before it seats it**, read off the
/// chain's own params at `daa` — the rules the draw (`palw_panel_bonds_judging_v1`) and the
/// registry's ready predicate (`palw_seat_not_ready_reason_net_v1`) hold every seat to, whatever
/// claim it is drawn onto:
///
/// * **The seat floor** (ADR-0124 Decision 4): past the panel economy a bond is drawn only with
///   `palw_panel_collateral_floor_v1` of the producer floor posted — ten producer floors, 130,000
///   BILI on testnet-12. Taken wherever the network states the economy, in force or scheduled, since
///   a bond is registered once and outlives every fence; without it, the producer floor, as the
///   draw does.
/// * **A ready seat for a model class** (ADR-0135 Decision 4): under the registry a seat judges a
///   class other than the floor only while `readiness_collateral_multiple` producer floors of its
///   collateral are FREE, so the posted collateral also covers what its declaration reserves (the
///   class and the floor, `PALW_CAPABILITY_EXPOSURE_SOMPI` each, where the capability bound is on).
/// * **One panel's reservation.** Past R-core+ (ADR-0152 L-4b) the room a seat needs to be bound
///   is priced per claim, `max(duty_bind, lock_2)`, which no read serves before the claim exists;
///   the seat floor's 500 ‰ room holds it on every class testnet-12 carries (the dearest, the 2M
///   row's `lock_2`, is about 33,400 BILI against the 65,000 BILI room of a 130,000 BILI bond —
///   `rcore_s3_one_ledger` T78). Before R-core+ a seat reserved three times the claim's own
///   exposure (ADR-0124 Decision 3); there setup keeps the producer's whole-lifetime figure for
///   the class as the bound, as it always did, and is `None` where that figure is unknown.
///
/// `None` also off `ConsensusV2`, which seats no panel.
pub(crate) fn seat_requirement(
    params: &kaspa_consensus_core::config::params::Params,
    daa: u64,
    is_base: bool,
    producer: Option<u64>,
) -> Option<u64> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return None;
    };
    let producer_floor = bundle.state.min_collateral_sompi();
    let mut need = if stated(params.palw_panel_economy_fence()) {
        kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_v1(producer_floor)
    } else {
        producer_floor
    };
    if !is_base && stated(params.palw_model_registry) {
        let multiple =
            kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1(bundle).readiness_collateral_multiple;
        let declared = if stated(params.palw_capability_bound_fence()) {
            2 * kaspa_consensus_core::palw_state_v2::PALW_CAPABILITY_EXPOSURE_SOMPI
        } else {
            0
        };
        need = need.max(producer_floor.saturating_mul(u64::from(multiple)).saturating_add(declared));
    }
    if !params.palw_rcore_plus_active_at(daa) {
        need = need.max(producer?);
    }
    Some(need)
}

/// **How long a retired bond's collateral stays locked**, in DAA — `palw_v2_bond_withdrawal_delay_at_v1`,
/// the bundle's delay plus the data-availability court's lattice past its fence (12,900 DAA on
/// testnet-12), resolved at the later of `daa` and that fence: a retirement is always later than
/// now. `None` off `ConsensusV2`.
pub(crate) fn withdrawal_delay_daa(params: &kaspa_consensus_core::config::params::Params, daa: u64) -> Option<u64> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return None;
    };
    let at = params.palw_da_court.filter(|f| stated(Some(*f))).map_or(daa, |f| f.daa_score().max(daa));
    Some(kaspa_consensus_core::config::params::palw_v2_bond_withdrawal_delay_at_v1(bundle, params.palw_da_court, at))
}

/// `12,900` DAA → `about 18 days`, at the ConsensusV2 block target (one DAA a block,
/// `PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`). An estimate, and said as one.
fn daa_as_time(daa: u64) -> String {
    let hours = daa.saturating_mul(kaspa_consensus_core::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS) / 3_600_000;
    match hours {
        0 => "under an hour".to_string(),
        1 => "about 1 hour".to_string(),
        2..=47 => format!("about {hours} hours"),
        _ => format!("about {} days", (hours + 12) / 24),
    }
}

/// **`--model` resolved against the class table**: `base`/`floor`, a 128-hex id, an id prefix of 8
/// or more, or a founding line's name. Ambiguity is an error that lists the candidates.
pub(crate) fn resolve_model<'a>(classes: &'a [ClassChoice], selector: &str) -> Result<&'a ClassChoice, String> {
    let s = selector.trim();
    if s.eq_ignore_ascii_case("base") || s.eq_ignore_ascii_case("floor") {
        return classes.iter().find(|c| c.is_base).ok_or_else(|| "this network has no base class".to_string());
    }
    let lower = s.to_ascii_lowercase();
    if lower.len() >= 8 && lower.bytes().all(|b| b.is_ascii_hexdigit()) {
        let hits: Vec<&ClassChoice> = classes.iter().filter(|c| c.id.starts_with(&lower)).collect();
        return match hits.as_slice() {
            [one] => Ok(one),
            [] => Err(format!("no class id starts with {lower}")),
            many => {
                Err(format!("{lower} names {} classes: {}", many.len(), many.iter().map(|c| c.label()).collect::<Vec<_>>().join(", ")))
            }
        };
    }
    let exact: Vec<&ClassChoice> = classes.iter().filter(|c| c.name.eq_ignore_ascii_case(s)).collect();
    if let [one] = exact.as_slice() {
        return Ok(one);
    }
    let partial: Vec<&ClassChoice> = classes.iter().filter(|c| c.name.to_ascii_lowercase().contains(&lower)).collect();
    match partial.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no class is named {s}")),
        many => Err(format!("{s} matches {} classes: {}", many.len(), many.iter().map(|c| c.label()).collect::<Vec<_>>().join(", "))),
    }
}

/// **What a bond for `class_id` locks**, sized exactly as `kaspad --palw-register-bond` sizes it:
/// the class's whole-lifetime requirement from its per-inference pwu (`facts.pwu` over the expected
/// draws at its target), never below the chain's floor.
pub(crate) async fn class_collateral(node: &NodeRead, class_id: &str) -> Option<u64> {
    let f = node.client().get_palw_producer_facts(class_id.to_string(), String::new(), 0, false).await.ok()?;
    if !f.available {
        return None;
    }
    let target: u128 = f.class_target.parse().ok()?;
    let per_inference = f.pwu / kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target).max(1);
    let floor = match &node.nv.params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state.min_collateral_sompi(),
        _ => 0,
    };
    Some(kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_collateral_for_claim_lifetime_v1(per_inference).max(floor))
}

/// The class table as a person chooses from it: each class with its founding line's name and what
/// a bond for it locks — as a producer and as a seat — the floor first.
pub(crate) async fn class_choices(node: &NodeRead, rows: &[kaspa_rpc_core::RpcPalwClassRow]) -> Vec<ClassChoice> {
    let params = &node.nv.params;
    // The registry rows' verification work, for `class_closed_reason` — asked only where the chain
    // arms §4-quater's deadlines (testnet-12, whose every node serves the registry read): a node that
    // predates an op drops the connection on it, and every read after it would fail.
    let verification_ccu: std::collections::BTreeMap<String, u128> = if stated(params.palw_class_verify_deadline) {
        node.client()
            .get_palw_model_registry()
            .await
            .ok()
            .filter(|r| r.available)
            .map(|r| {
                r.classes
                    .into_iter()
                    .filter(|c| c.has_row)
                    .filter_map(|c| Some((c.class_id.to_ascii_lowercase(), c.verification_ccu.parse::<u128>().ok()?)))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Default::default()
    };
    let mut classes = Vec::new();
    for c in rows {
        let name = node
            .client()
            .get_palw_model_lines(c.class_id.clone())
            .await
            .ok()
            .and_then(|l| l.lines.first().map(|l| l.name.clone()))
            .unwrap_or_default();
        let collateral = class_collateral(node, &c.class_id).await;
        classes.push(ClassChoice {
            id: c.class_id.clone(),
            name,
            is_base: c.is_base_class,
            artifact_root: c.artifact_root.clone(),
            fp_certified: c.fp_certified,
            share_permille: c.share_permille,
            collateral,
            seat_collateral: seat_requirement(params, node.daa(), c.is_base_class, collateral),
            closed: class_closed_reason(
                params,
                node.daa(),
                &c.class_id,
                &c.status,
                c.share_permille,
                verification_ccu.get(&c.class_id.to_ascii_lowercase()).copied(),
            ),
        });
    }
    // The floor first: it is the one every node can run.
    classes.sort_by_key(|c| (!c.is_base, c.collateral.unwrap_or(u64::MAX)));
    classes
}

/// The `.palwart` files this host keeps where setup looks.
///
/// An operator may keep a large artifact outside the node directory (for example on a mounted
/// volume) without putting that path in a profile first:
///
/// * `MISAKA_PALW_ARTIFACT` names one exact file;
/// * `MISAKA_PALW_ARTIFACT_DIRS` is a platform-separated list of directories to scan.
///
/// The normal per-user and per-node locations remain fallbacks. Discovery never makes an
/// artifact trustworthy: `step_artifact` computes and matches the PALW root before writing a
/// mining profile.
pub(crate) fn artifacts_here(network: &str, appdir: Option<&Path>) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut dirs = vec![home.join(".misaka").join("models"), home.join(".misaka").join(network).join("models")];
    dirs.extend(appdir.map(|a| a.join("models")));
    if let Some(raw) = std::env::var_os("MISAKA_PALW_ARTIFACT_DIRS") {
        dirs.extend(std::env::split_paths(&raw));
    }
    let mut out = Vec::new();

    // An exact environment path has priority over directory discovery, but it is still checked
    // against the class root below. This is useful for a read-only mounted artifact volume.
    if let Some(path) = std::env::var_os("MISAKA_PALW_ARTIFACT") {
        let p = PathBuf::from(path);
        if p.is_file() {
            out.push(p);
        }
    }
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "palwart") && p.is_file() && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// A bond, as the registry reads it.
#[derive(Clone, Debug)]
struct BondSeen {
    outpoint: String,
    pubkey: String,
    collateral: u64,
    retiring: Option<u64>,
    /// The classes it is seated for; `None` when the node is too old to say.
    capable: Option<Vec<String>>,
}

/// The key's outputs, sorted the way a registration and a fee float are funded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Funds {
    /// The largest mature, unbonded, non-coinbase output: what a registration can spend.
    pub(crate) best: Option<(String, u64)>,
    pub(crate) spendable_plain: u64,
    pub(crate) spendable_coinbase: u64,
    pub(crate) maturing: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FundsVerdict {
    /// One output holds enough.
    Enough(String, u64),
    /// Enough in total, but spread over several outputs: a registration spends one.
    Split(u64),
    /// Enough only counting mining rewards, which a registration cannot spend.
    CoinbaseOnly(u64),
    Short(u64),
}

pub(crate) fn funds_verdict(f: &Funds, need: u64) -> FundsVerdict {
    match &f.best {
        Some((op, amount)) if *amount >= need => FundsVerdict::Enough(op.clone(), *amount),
        _ if f.spendable_plain >= need => FundsVerdict::Split(f.spendable_plain),
        _ if f.spendable_plain + f.spendable_coinbase >= need => FundsVerdict::CoinbaseOnly(f.spendable_coinbase),
        _ => FundsVerdict::Short(f.spendable_plain + f.spendable_coinbase),
    }
}

/// How far an existing bond is below the producer's whole-claim-lifetime sizing for the selected
/// class. A smaller bond can be registered and may draw for a while, but its exposure fills before
/// old claims leave the challenge window; "registered" must therefore never be printed as
/// "sufficient" by setup.
fn collateral_shortfall(current: u64, whole_lifetime: Option<u64>) -> Option<u64> {
    whole_lifetime.and_then(|need| need.checked_sub(current)).filter(|short| *short > 0)
}

fn funds_of(all: &[crate::wallet::Funding]) -> Funds {
    let mut f = Funds::default();
    for u in all {
        if u.bonded || u.reserved {
            continue;
        }
        if !u.mature {
            f.maturing += u.amount;
        } else if u.entry.is_coinbase {
            f.spendable_coinbase += u.amount;
        } else {
            f.spendable_plain += u.amount;
            if f.best.as_ref().is_none_or(|(_, a)| u.amount > *a) {
                f.best = Some((format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index), u.amount));
            }
        }
    }
    f
}

/// The classes a bond should declare: the one it serves and the floor (derived, so every node can
/// judge it), never dropping one already declared — a declaration REPLACES the set.
pub(crate) fn capability_set(declared: &[String], serve: &str, base: Option<&str>) -> (BTreeSet<String>, bool) {
    let mut want: BTreeSet<String> = declared.iter().cloned().collect();
    let before = want.len();
    want.insert(serve.to_string());
    if let Some(b) = base {
        want.insert(b.to_string());
    }
    let changed = want.len() != before;
    (want, changed)
}

/// **The chooser's table for a role**: each class with what a bond for it locks in that role. A
/// class no bond can be sized for reads [`NOT_AVAILABLE`], never a number, and the model and
/// collateral columns are as wide as their widest cells, so the columns after them stay aligned.
/// The header first, unpainted.
pub(crate) fn class_table(classes: &[ClassChoice], role: BondRole) -> (String, Vec<String>) {
    let labels: Vec<String> = classes.iter().map(ClassChoice::label).collect();
    let cells: Vec<String> = classes.iter().map(|c| c.lock(role).shown()).collect();
    let widest = |column: &[String], least: usize| column.iter().map(|c| c.chars().count()).max().unwrap_or(0).max(least) + 2;
    let (model, width) = (widest(&labels, 20), widest(&cells, 14));
    let head = format!("  {:<3}{:<model$}{:<width$}{:<10}{:<8}{}", "#", "MODEL", "COLLATERAL", "SHARE", "PROMPT", "ARTIFACT");
    let rows = classes
        .iter()
        .zip(labels.iter().zip(&cells))
        .enumerate()
        .map(|(i, (c, (label, cell)))| {
            format!(
                "  {:<3}{:<model$}{:<width$}{:<10}{:<8}{}",
                i + 1,
                label,
                cell,
                c.share_permille.map(|s| format!("{s} ‰")).unwrap_or_else(|| "—".into()),
                if c.fp_certified { "yes" } else { "no" },
                if c.is_base { "none — derived" } else { "a .palwart file" }
            )
        })
        .collect();
    (head, rows)
}

/// The margin of a registration's one funding output past the collateral, as the join doc says it.
fn margin_words() -> String {
    format!("{} for the carrier's fee and change", catalog::msk(REGISTRATION_MARGIN_SOMPI as u128))
}

/// What a bond registration is confirmed on — every figure read before the question is asked.
pub(crate) struct RegistrationTerms<'a> {
    pub(crate) role: BondRole,
    /// The class the bond is for, as the chooser names it.
    pub(crate) class: &'a str,
    pub(crate) class_is_base: bool,
    /// What the bond locks: the role's figure for the class, and what `--palw-bond-collateral` says.
    pub(crate) collateral: u64,
    /// What a seat needs here ([`seat_requirement`] for the class), when known.
    pub(crate) seat_requirement: Option<u64>,
    /// The one output the registration spends, shortened, and what it holds.
    pub(crate) funding: &'a str,
    pub(crate) funding_amount: u64,
    pub(crate) payee: &'a str,
    /// [`withdrawal_delay_daa`], when known.
    pub(crate) withdrawal_delay_daa: Option<u64>,
}

/// **What setup says before it asks to register a bond**: the role, the amount and why that
/// amount, the one-bond rule, and how and when the collateral comes back — the heading, then the
/// lines printed under it, in the operator's words.
pub(crate) fn registration_lines(t: &RegistrationTerms) -> (String, Vec<String>) {
    let heading = format!("Register a {} bond for this key:", t.role.name());
    let amount = catalog::msk(t.collateral as u128);
    let mut lines = Vec::new();
    lines.push(match (t.role.produce, t.role.seat) {
        (true, true) => format!("role      producer and verifier seat: it mines {} and sits on panels", t.class),
        (true, false) => format!("role      producer: it mines {}, and this collateral backs its claims", t.class),
        (false, _) => "role      verifier seat: it sits on panels and judges other producers' claims; it does not mine".to_string(),
    });
    lines.push(match (t.role.produce, t.role.seat) {
        (_, true) => format!("lock      {amount} as its collateral: the least a verifier seat needs on this chain"),
        (true, false) => {
            format!("lock      {amount} as its collateral: the node's own sizing for claims of {} over their whole life", t.class)
        }
        (false, false) => format!("lock      {amount} as its collateral"),
    });
    if t.role.seat {
        lines.push(if t.class_is_base { format!("judges    {}", t.class) } else { format!("judges    {} and the floor", t.class) });
    } else if let Some(seat) = t.seat_requirement.filter(|seat| t.collateral < *seat) {
        lines.push(format!(
            "seats     none: under the {} a verifier seat needs, the panel draw never seats this bond",
            catalog::msk(seat as u128)
        ));
    }
    lines.push(format!("from      {} ({})", t.funding, catalog::msk(t.funding_amount as u128)));
    lines.push(format!("payee     {} — its rewards, and the collateral once the bond is retired", t.payee));
    lines.push("one bond  this key can hold only ONE bond, ever: it can never register a second one (DuplicateBondKey),".to_string());
    lines.push("          retiring this one does not free the key, and the collateral cannot be topped up".to_string());
    lines.push("withdraw  the collateral stays locked while the bond is registered. To get it back: misaka bond retire,".to_string());
    lines.push(match t.withdrawal_delay_daa {
        Some(delay) => format!(
            "          then the withdrawal delay, {} DAA after the retirement ({} at the {}-second block target)",
            status::group(delay),
            daa_as_time(delay),
            kaspa_consensus_core::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS / 1000
        ),
        None => "          then the chain's withdrawal delay after the retirement".to_string(),
    });
    lines.push(
        "          A bond that signed panels may first wait for their locks to run out (misaka bond status lists them)".to_string(),
    );
    (heading, lines)
}

/// **Setup's refusal to register a verifier seat below what the panel draw seats** — said in place
/// of offering a smaller lock: such a bond locks the funds, is never drawn onto a panel, and uses
/// up the key's only registration.
pub(crate) fn seat_funds_refusal(class: &str, collateral: u64, have: u64, address: &str, network: &str, key: &str) -> Finding {
    let need = collateral.saturating_add(REGISTRATION_MARGIN_SOMPI);
    Finding::error(
        "E-FUNDS-BELOW-SEAT-FLOOR",
        exit::FUNDS,
        format!(
            "A verifier seat bond locks at least {}, and this key holds {}",
            catalog::msk(collateral as u128),
            catalog::msk(have as u128)
        ),
    )
    .reason(
        "the panel draw seats only a bond that holds the seat floor, so setup never registers a smaller seat bond: it would lock \
         the funds, never be drawn onto a panel, and use up this key's only bond",
    )
    .current(format!("{} spendable at {address}", catalog::msk(have as u128)))
    .required(format!(
        "one ordinary output of at least {} — {} of collateral for {class}, and {}",
        catalog::msk(need as u128),
        catalog::msk(collateral as u128),
        margin_words()
    ))
    .fix(format!("send {} or more to {address} in one transfer, then run misaka verifier setup again", catalog::msk(need as u128)))
    .fix(format!(
        "or send the missing {} and merge the outputs first: misaka --network {network} wallet utxo consolidate --key-file {key} --yes",
        catalog::msk(need.saturating_sub(have) as u128)
    ))
    .docs("docs/testnet12-join-mining.md#4-funds")
}

/// What setup's own node registers: `kaspad --palw-register-bond` for this key, the class it names
/// (empty: the floor), the one output it spends, and the collateral the operator confirmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Registration {
    pub(crate) class: String,
    pub(crate) funding: String,
    pub(crate) collateral: u64,
}

/// **The command line setup starts `kaspad` with** — the network, the node settings the file holds
/// and, when it registers, the registration: the key, the class, the funding output, and the
/// collateral as `--palw-bond-collateral`, always, so the node locks exactly the figure the
/// operator confirmed and never a default of its own. The operator's `extra_kaspad_args` follow,
/// minus the roles (this node produces and seats nothing) and minus any collateral of their own
/// (the confirmed figure is the only one).
pub(crate) fn setup_node_args(
    network: &str,
    appdir: &Path,
    advanced: &AdvancedSection,
    key: &Path,
    register: Option<&Registration>,
) -> Result<Vec<String>, Finding> {
    let mut a = supervisor::network_flags(network)?;
    a.push(format!("--appdir={}", appdir.display()));
    if let Some(listen) = &advanced.listen {
        a.push(format!("--listen={listen}"));
    }
    a.push(format!("--rpclisten-borsh={}", advanced.rpc_borsh.clone().unwrap_or_else(|| "default".into())));
    a.push("--utxoindex".into());
    for peer in &advanced.peers {
        a.push(format!("--addpeer={peer}"));
    }
    if let Some(r) = register {
        a.push("--palw-register-bond".into());
        a.push(format!("--palw-producer-key={}", key.display()));
        if !r.class.is_empty() {
            a.push(format!("--palw-producer-class={}", r.class));
        }
        a.push(format!("--palw-fee-outpoint={}", r.funding));
        a.push(format!("--palw-bond-collateral={}", r.collateral));
    }
    let mut extra = advanced.extra_kaspad_args.iter().peekable();
    while let Some(x) = extra.next() {
        match x.split('=').next().unwrap_or_default() {
            "--palw-produce" | "--palw-panel" => {}
            "--palw-bond-collateral" => {
                // `--flag value` spelled as two arguments: the value goes with it.
                if !x.contains('=') {
                    extra.next_if(|v| !v.starts_with("--"));
                }
            }
            _ => a.push(x.clone()),
        }
    }
    Ok(a)
}

// ---------------------------------------------------------------------------------------------
// the file
// ---------------------------------------------------------------------------------------------

fn q(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// **`mining.toml` as setup writes it** — ADR-0122 §7's layout, only the fields that say
/// something, each read back by `misaka mining start`.
pub(crate) fn render_toml(file: &MiningToml, purpose: Purpose, today: &str) -> String {
    let m = &file.mining;
    let a = &file.advanced;
    let mut out = format!(
        "# MISAKA {} — written by `misaka {}` on {today} (ADR-0122 §7).\n\
         # `{}` runs exactly this. Re-run setup to change it, or edit it by hand.\n\n[mining]\n",
        if purpose == Purpose::Mine { "mining" } else { "verifier" },
        if purpose == Purpose::Mine { "mining setup" } else { "verifier setup" },
        purpose.start()
    );
    let kv = |out: &mut String, k: &str, v: String| out.push_str(&format!("{k:<13} = {v}\n"));
    if let Some(v) = m.enabled {
        kv(&mut out, "enabled", v.to_string());
    }
    if let Some(v) = &m.network {
        kv(&mut out, "network", q(v));
    }
    if let Some(v) = &m.model {
        kv(&mut out, "model", q(v));
    }
    if let Some(v) = &m.wallet {
        kv(&mut out, "wallet", q(v));
    }
    if let Some(v) = &m.key {
        kv(&mut out, "key", q(v));
    }
    if let Some(v) = m.prompt {
        kv(&mut out, "prompt", v.to_string());
    }
    out.push_str("\n[advanced]\n");
    if let Some(v) = &a.appdir {
        kv(&mut out, "appdir", q(v));
    }
    if let Some(v) = &a.bond {
        kv(&mut out, "bond", q(v));
    }
    if let Some(v) = &a.fee_outpoint {
        kv(&mut out, "fee_outpoint", q(v));
    }
    match &a.artifact {
        Some(ArtifactList::One(v)) => kv(&mut out, "artifact", q(v)),
        Some(ArtifactList::Many(v)) => {
            kv(&mut out, "artifact", format!("[{}]", v.iter().map(|x| q(x)).collect::<Vec<_>>().join(", ")))
        }
        None => {}
    }
    if !a.peers.is_empty() {
        kv(&mut out, "peers", format!("[{}]", a.peers.iter().map(|x| q(x)).collect::<Vec<_>>().join(", ")));
    }
    for (k, v) in [("listen", &a.listen), ("rpc_borsh", &a.rpc_borsh), ("resident_bytes", &a.resident_bytes), ("kaspad", &a.kaspad)] {
        if let Some(v) = v {
            kv(&mut out, k, q(v));
        }
    }
    if let Some(v) = a.stop_grace_secs {
        kv(&mut out, "stop_grace_secs", v.to_string());
    }
    if let Some(v) = a.challenge {
        kv(&mut out, "challenge", v.to_string());
    }
    if !a.extra_kaspad_args.is_empty() {
        kv(&mut out, "extra_kaspad_args", format!("[{}]", a.extra_kaspad_args.iter().map(|x| q(x)).collect::<Vec<_>>().join(", ")));
    }
    let p = &a.prompt;
    let lane: Vec<(&str, &Option<String>)> = vec![
        ("listen", &p.listen),
        ("outbox", &p.outbox),
        ("worker", &p.worker),
        ("identity", &p.identity),
        ("artifact", &p.artifact),
        ("tokenizer", &p.tokenizer),
        ("gateway", &p.gateway),
        ("rail", &p.rail),
    ];
    if lane.iter().any(|(_, v)| v.is_some()) {
        out.push_str("\n[advanced.prompt]\n");
        for (k, v) in lane {
            if let Some(v) = v {
                kv(&mut out, k, q(v));
            }
        }
    }
    out
}

/// **The one command that runs a validator**: `kaspad` beside this `misaka`, the file's node
/// settings, and the overlay's validator in-process (`--enable-validator`), which keeps its own
/// equivocation guard in the appdir. Printed by `validator setup` and `validator status` alike.
pub(crate) fn validator_command(network: &str, file: &ValidatorToml) -> String {
    let a = &file.advanced;
    let program = supervisor::binary("kaspad", a.kaspad.as_deref().map(|k| PathBuf::from(procs::expand_home(k))).as_deref())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "kaspad".to_string());
    let mut args = supervisor::network_flags(network).unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();
    let appdir = a.appdir.as_deref().map(procs::expand_home).unwrap_or_else(|| default_appdir(network, &home).display().to_string());
    args.push(format!("--appdir={appdir}"));
    if let Some(listen) = &a.listen {
        args.push(format!("--listen={listen}"));
    }
    args.push(format!("--rpclisten-borsh={}", a.rpc_borsh.clone().unwrap_or_else(|| "default".into())));
    args.push("--utxoindex".into());
    args.extend(a.peers.iter().map(|p| format!("--addpeer={p}")));
    args.push("--enable-validator".into());
    if let Some(key) = &file.validator.key {
        args.push(format!("--validator-key={}", procs::expand_home(key)));
    }
    if let Some(bond) = &file.validator.bond {
        args.push(format!("--stake-bond={bond}"));
    }
    args.push("--validator-mode=active".into());
    args.extend(
        a.extra_kaspad_args
            .iter()
            .filter(|x| !matches!(x.split('=').next().unwrap_or_default(), "--palw-produce" | "--palw-panel"))
            .cloned(),
    );
    Cmd { name: "kaspad", program: PathBuf::from(program), args, env: Vec::new() }.shell_line()
}

/// **`validator.toml` as setup writes it** — the validator's section, then the node settings.
pub(crate) fn render_validator_toml(file: &ValidatorToml, today: &str) -> String {
    let v = &file.validator;
    let mut out = format!(
        "# MISAKA validator — written by `misaka validator setup` on {today} (ADR-0122 §8.2).\n\
         # The node runs the DNS-finality validator in-process; `misaka validator status` shows it.\n\n[validator]\n"
    );
    let kv = |out: &mut String, k: &str, v: String| out.push_str(&format!("{k:<13} = {v}\n"));
    if let Some(x) = &v.network {
        kv(&mut out, "network", q(x));
    }
    if let Some(x) = &v.key {
        kv(&mut out, "key", q(x));
    }
    if let Some(x) = &v.bond {
        kv(&mut out, "bond", q(x));
    }
    if let Some(x) = v.amount_sompi {
        kv(&mut out, "amount_sompi", x.to_string());
    }
    // The node settings, in mining.toml's own spelling.
    let node = MiningToml { mining: MiningSection::default(), advanced: file.advanced.clone() };
    let rendered = render_toml(&node, Purpose::Mine, today);
    if let Some((_, advanced)) = rendered.split_once("\n[advanced]\n") {
        out.push_str("\n[advanced]\n");
        out.push_str(advanced);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// the wizard
// ---------------------------------------------------------------------------------------------

struct Wizard<'a> {
    ctx: &'a crate::node::Ctx,
    purpose: Purpose,
    args: SetupArgs,
    ui: Ui,
    network: String,
    path: PathBuf,
    existing: Option<String>,
    file: MiningToml,
    rows: Vec<Row>,
    /// Nothing named the network (no flag, no file): it was read off the one node running, or is
    /// the public mining testnet — and the first line says how to choose another.
    network_guessed: bool,
    key_path: PathBuf,
    key: Option<KeyFacts>,
    appdir: PathBuf,
    rpc: Option<String>,
    own: Option<OwnNode>,
    external: Option<(procs::Proc, procs::KaspadArgs)>,
    node: Option<NodeRead>,
    classes: Vec<ClassChoice>,
    class: Option<ClassChoice>,
    bond: Option<BondSeen>,
    /// `validator setup`'s own fields (the stake bond and its amount).
    validator: ValidatorSection,
}

/// `misaka mining setup` / `misaka verifier setup`.
pub(crate) async fn run(ctx: &crate::node::Ctx, purpose: Purpose, args: SetupArgs) -> CliResult {
    let mut w = Wizard::new(ctx, purpose, args)?;
    let result = w.steps().await;
    w.release_node().await;
    w.finish(result)
}

impl<'a> Wizard<'a> {
    fn new(ctx: &'a crate::node::Ctx, purpose: Purpose, args: SetupArgs) -> Result<Wizard<'a>, CliError> {
        let ui = Ui::new(ctx.output, args.yes);
        let default_path = if purpose == Purpose::Validate { ValidatorToml::default_path() } else { MiningToml::default_path() };
        let path = args
            .config
            .clone()
            .or(default_path)
            .ok_or_else(|| CliError::new(exit::HOST, "no home directory to keep ~/.misaka's configuration in"))?;
        let existing = std::fs::read_to_string(&path).ok();
        let bad = |e: toml::de::Error| CliError::new(exit::CONFIG, format!("{}: {e}", path.display()));
        // The validator's file carries its own section and the same `[advanced]` node settings; the
        // node steps read them through the mining shape, and the file is written back as its own.
        let (parsed, validator) = match (&existing, purpose) {
            (Some(text), Purpose::Validate) => {
                let v = toml::from_str::<ValidatorToml>(text).map_err(bad)?;
                let as_mining = MiningToml {
                    mining: MiningSection { network: v.validator.network.clone(), key: v.validator.key.clone(), ..Default::default() },
                    advanced: v.advanced.clone(),
                };
                (Some(as_mining), v.validator)
            }
            (Some(text), _) => (Some(toml::from_str::<MiningToml>(text).map_err(bad)?), ValidatorSection::default()),
            (None, _) => (None, ValidatorSection::default()),
        };
        // The network: the one named, else the file's, else the one node running here, else the
        // public mining testnet (testnet-12 since the 2026-09-22 regenesis) — said on the first line
        // either way.
        let running = procs::find(procs::Component::Kaspad);
        let single = (running.len() == 1).then(|| procs::parse_kaspad_args(&running[0].args).network);
        let named = args.network.clone().or_else(|| parsed.as_ref().and_then(|f| f.mining.network.clone()));
        let network_guessed = named.is_none();
        let network = named.or(single).unwrap_or_else(|| crate::node::DEFAULT_NETWORK.to_string());
        // A file for another network is not this setup's starting point; it is replaced at the end
        // (with a question), and its values do not leak into this network's.
        let file = parsed.filter(|f| f.mining.network.as_deref().is_none_or(|n| n == network)).unwrap_or_default();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        // The node setup starts listens where the file says; `--rpc` points at another.
        let rpc = args.rpc.clone().or_else(|| file.advanced.rpc_borsh.clone().map(|v| v.replace("0.0.0.0", "127.0.0.1")));
        Ok(Wizard {
            ctx,
            purpose,
            ui,
            path,
            existing,
            file,
            rows: Vec::new(),
            network_guessed,
            // A validator signs with a key of its own: roles do not share a seed.
            key_path: home.join(".misaka").join(if purpose == Purpose::Validate { "validator.seed" } else { "miner.seed" }),
            key: None,
            appdir: default_appdir(&network, &home),
            rpc,
            own: None,
            external: None,
            node: None,
            classes: Vec::new(),
            class: None,
            bond: None,
            validator,
            network,
            args,
        })
    }

    fn row(&mut self, sev: Severity, step: &'static str, value: impl Into<String>) {
        let value = value.into();
        self.ui.mark(sev, step, &value);
        let state = match sev {
            Severity::Ok => "ok",
            Severity::Skip => "skipped",
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "blocked",
        };
        self.rows.push(Row { step, state, value });
    }

    fn node(&self) -> &NodeRead {
        self.node.as_ref().expect("the node step ran")
    }

    fn params(&self) -> Option<kaspa_consensus_core::config::params::Params> {
        self.network.parse::<kaspa_consensus_core::network::NetworkId>().ok().map(kaspa_consensus_core::config::params::Params::from)
    }

    fn key_source(&self) -> crate::keys::KeySource {
        crate::keys::KeySource { key_file: Some(self.key_path.display().to_string()), key_stdin: false }
    }

    fn log_file(&self) -> PathBuf {
        match &self.external {
            Some((_, args)) => match &args.logdir {
                Some(dir) => PathBuf::from(procs::expand_home(dir)).join("rusty-kaspa.log"),
                None => nodelog::default_log_file(&self.appdir, &self.network),
            },
            None => nodelog::default_log_file(&self.appdir, &self.network),
        }
    }

    async fn steps(&mut self) -> Step {
        let mut title = format!("MISAKA {} · {}", self.purpose.title(), self.network);
        if self.network_guessed {
            title.push_str("   (--network to set up another)");
        }
        self.ui.say(&paint::bold(&title));
        self.ui.say(&paint::dim("  Each step reads what is already true, so running this again resumes where it stopped."));
        self.ui.say("");
        let Some(params) = self.params() else {
            return Err(Halt::Blocked(
                Finding::error("E-CONFIG-NETWORK", exit::CONFIG, format!("'{}' is not a network id", self.network))
                    .fix("misaka --network testnet-11 mining setup"),
            ));
        };
        let dns = params.dns_params.clone();
        match (self.purpose, &dns) {
            (Purpose::Validate, None) => {
                return Err(Halt::Blocked(
                    Finding::error(
                        "E-SETUP-NO-DNS",
                        exit::CONFIG,
                        format!("{} has no DNS-finality overlay in this build", self.network),
                    )
                    .reason("a validator stakes a bond and attests finality epochs, and only a network with the overlay has them")
                    .fix("misaka --network testnet-10 validator setup")
                    .docs("docs/validator-runbook.md"),
                ));
            }
            (Purpose::Validate, Some(_)) => {}
            _ if !matches!(params.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) => {
                return Err(Halt::Blocked(
                    Finding::error("E-SETUP-NO-PALW", exit::CONFIG, format!("{} has no PALW mining in this build", self.network))
                        .reason("mining and verifying are PALW roles, and only a ConsensusV2 network has them")
                        .fix("misaka --network testnet-11 mining setup"),
                ));
            }
            _ => {}
        }
        if let Err(f) = supervisor::network_flags(&self.network) {
            return Err(Halt::Blocked(f));
        }
        self.row(Severity::Ok, "network", self.network.clone());
        if let Some(text) = &self.existing
            && let Ok(other) = toml::from_str::<MiningToml>(text)
            && other.mining.network.as_deref().is_some_and(|n| n != self.network)
        {
            let msg = format!(
                "{} configures {} — setup writes {} over it at the end (you will be asked)",
                host::tilde(&self.path),
                other.mining.network.unwrap_or_default(),
                self.network
            );
            self.row(Severity::Warning, "config", msg);
        }
        self.step_key(&params).await?;
        self.step_node(&params).await?;
        if let (Purpose::Validate, Some(dns)) = (self.purpose, dns) {
            self.step_stake_bond(&params, &dns).await?;
            return self.step_write_validator().await;
        }
        self.step_model(&params).await?;
        self.step_bond().await?;
        if self.bond.is_none() {
            let (op, amount) = self.step_funds().await?;
            self.step_register(&op, amount).await?;
        }
        self.step_artifact()?;
        self.step_capability().await?;
        self.step_fee().await?;
        self.step_write().await?;
        Ok(())
    }

    // -- key ------------------------------------------------------------------------------------

    async fn step_key(&mut self, params: &kaspa_consensus_core::config::params::Params) -> Step {
        // A running node's `--palw-producer-key` is the miner's key, never the validator's.
        let external_key = (self.purpose != Purpose::Validate)
            .then(|| {
                procs::the_kaspad(&self.network, self.args.appdir.as_deref().or(self.file.advanced.appdir.as_deref())).ok().flatten()
            })
            .flatten()
            .and_then(|(_, a)| a.key);
        if let Some(named) = self.args.key_file.clone().or_else(|| self.file.mining.key.clone()).or(external_key) {
            self.key_path = PathBuf::from(procs::expand_home(&named));
        }
        let shown = host::tilde(&self.key_path);
        if !self.key_path.exists() {
            let answer =
                self.ui.confirm(&format!("There is no key at {shown}. Make one? It is written 0600 and never printed."), true).await;
            match answer {
                Answer::Yes => {}
                Answer::Interrupted => return Err(Halt::Interrupted),
                Answer::No => {
                    return Err(Halt::Declined("setup needs a key: answer yes to make one, or name yours with --key-file".into()));
                }
                Answer::Unanswered => {
                    return Err(Halt::Declined(format!(
                        "no key at {shown}: re-run with --yes to make one there, or name yours with --key-file"
                    )));
                }
            }
            if let Some(dir) = self.key_path.parent() {
                let _ = std::fs::create_dir_all(dir);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
                }
            }
            let address = crate::keys::generate(&self.key_path.display().to_string(), params.prefix()).map_err(|e| {
                Halt::Blocked(Finding::error("E-KEY-WRITE", exit::IDENTITY, "The key could not be written").current(e.msg))
            })?;
            self.row(Severity::Ok, "key", format!("made {shown} · {}", crate::operator::doctor::short_address(&address.to_string())));
            self.ui.sub("back it up: the bond, its collateral and every reward belong to this file");
        }
        match snapshot::key_facts(&self.key_path, params.prefix()) {
            Ok(k) => {
                if !self.rows.iter().any(|r| r.step == "key") {
                    self.row(Severity::Ok, "key", format!("{shown} · {}", crate::operator::doctor::short_address(&k.address)));
                }
                #[cfg(unix)]
                if crate::keys::require_key_file_mode(&self.key_path.display().to_string()).is_err() {
                    self.ui.sub(&paint::yellow(&format!("! its mode lets others read it — chmod 600 {shown}")));
                }
                self.key = Some(k);
            }
            Err(e) => return Err(Halt::Blocked(catalog::key_unreadable(&self.key_path.display().to_string(), &e))),
        }
        self.file.mining.key = Some(host::tilde(&self.key_path));
        Ok(())
    }

    // -- node -----------------------------------------------------------------------------------

    fn node_args(&self, register: Option<&Registration>) -> Result<Vec<String>, Finding> {
        setup_node_args(&self.network, &self.appdir, &self.file.advanced, &self.key_path, register)
    }

    fn spawn_own(&mut self, register: Option<&Registration>) -> Step {
        let program = supervisor::binary(
            "kaspad",
            self.file.advanced.kaspad.as_deref().map(|k| PathBuf::from(procs::expand_home(k))).as_deref(),
        )
        .map_err(Halt::Blocked)?;
        let cmd = Cmd { name: "setup-kaspad", program, args: self.node_args(register).map_err(Halt::Blocked)?, env: Vec::new() };
        let run_dir = supervisor::run_dir(&self.network);
        std::fs::create_dir_all(&run_dir)
            .map_err(|e| Halt::Blocked(Finding::error("E-HOST-RUN-DIR", exit::HOST, format!("{}: {e}", run_dir.display()))))?;
        let child = supervisor::spawn(&cmd, &run_dir).map_err(|e| {
            Halt::Blocked(Finding::error("E-SETUP-NODE-SPAWN", exit::COMPONENT_DOWN, "kaspad could not be started").current(e))
        })?;
        let pid = child.child.id();
        let started_unix = procs::now_unix() as i64;
        SetupState { pid, appdir: self.appdir.display().to_string(), register: register.is_some(), started_unix }.write(&self.network);
        self.own = Some(OwnNode {
            child: Some(child),
            pid,
            register: register.is_some(),
            collateral: register.map(|r| r.collateral),
            started_unix,
        });
        Ok(())
    }

    /// Stop the node setup runs, if it runs one.
    async fn stop_own(&mut self) {
        let Some(mut own) = self.own.take() else { return };
        match own.child.as_mut() {
            Some(child) => {
                let said = supervisor::stop_child(child, Duration::from_secs(90)).await;
                self.ui.sub(&paint::dim(&said));
            }
            None => {
                supervisor::signal(own.pid, libc::SIGTERM);
                let deadline = Instant::now() + Duration::from_secs(90);
                while supervisor::alive(own.pid) && Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                if supervisor::alive(own.pid) {
                    supervisor::signal(own.pid, libc::SIGKILL);
                }
            }
        }
        SetupState::remove(&self.network);
    }

    fn own_exited(&mut self) -> Option<String> {
        let own = self.own.as_mut()?;
        let exited = match own.child.as_mut() {
            Some(c) => !matches!(c.child.try_wait(), Ok(None)),
            None => !supervisor::alive(own.pid),
        };
        exited.then(|| supervisor::tail(&supervisor::run_dir(&self.network), "setup-kaspad", 12))
    }

    async fn step_node(&mut self, params: &kaspa_consensus_core::config::params::Params) -> Step {
        let hint = self.args.appdir.clone().or_else(|| self.file.advanced.appdir.clone());
        match procs::the_kaspad(&self.network, hint.as_deref()) {
            Err(pids) => {
                return Err(Halt::Blocked(
                    Finding::error("E-SETUP-NODE-AMBIGUOUS", exit::CONFIG, format!("Several kaspad for {} run here", self.network))
                        .current(format!("pids {}", pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")))
                        .fix("name the one to set up with --appdir <its --appdir>"),
                ));
            }
            Ok(Some((proc_, args))) => {
                if let Some(dir) = &args.appdir {
                    self.appdir = PathBuf::from(procs::expand_home(dir));
                }
                self.rpc = self
                    .rpc
                    .clone()
                    .or_else(|| args.rpclisten_borsh.clone().filter(|v| v != "default").map(|v| v.replace("0.0.0.0", "127.0.0.1")));
                let adopted = SetupState::read(&self.network).filter(|s| s.pid == proc_.pid);
                if let Some(state) = adopted {
                    self.own = Some(OwnNode {
                        child: None,
                        pid: proc_.pid,
                        register: state.register,
                        // Read off its command line: what it will register is what it was started with.
                        collateral: args.bond_collateral,
                        started_unix: state.started_unix,
                    });
                    self.row(Severity::Ok, "node", format!("setup's own node from an earlier run, pid {} — adopted", proc_.pid));
                } else {
                    let what = if args.produce {
                        "a miner"
                    } else if args.panel {
                        "a verifier"
                    } else {
                        "a node"
                    };
                    self.row(
                        Severity::Ok,
                        "node",
                        format!("{what} already runs here: pid {} · {}", proc_.pid, host::tilde(&self.appdir)),
                    );
                    self.external = Some((proc_, args));
                }
            }
            Ok(None) => {
                if let Some(dir) = hint {
                    self.appdir = PathBuf::from(procs::expand_home(&dir));
                }
                if self.file.advanced.peers.is_empty() {
                    self.file.advanced.peers =
                        if self.args.peers.is_empty() { default_peers(&self.network) } else { self.args.peers.clone() };
                }
                self.spawn_own(None)?;
                let pid = self.own.as_ref().map(|o| o.pid).unwrap_or_default();
                self.row(Severity::Ok, "node", format!("started kaspad for setup: pid {pid} · {}", host::tilde(&self.appdir)));
                let after = match self.purpose {
                    Purpose::Validate => "the command setup prints at the end runs the node after that".to_string(),
                    other => format!("{} runs the node after that", other.start()),
                };
                self.ui.sub(&paint::dim(&format!(
                    "output {} · setup stops it when it ends; {after}",
                    host::tilde(&supervisor::run_dir(&self.network).join("setup-kaspad.out")),
                )));
            }
        }
        if !self.args.peers.is_empty() {
            self.file.advanced.peers = self.args.peers.clone();
        }
        self.file.advanced.appdir = Some(host::tilde(&self.appdir));

        // Its RPC.
        let deadline = Instant::now() + if self.own.is_some() { Duration::from_secs(300) } else { Duration::from_secs(20) };
        let node = loop {
            match snapshot::connect_to(&self.network, self.rpc.as_deref(), Duration::from_secs(3)).await {
                Ok(n) => break n,
                Err((url, e)) => {
                    if let Some(tail) = self.own_exited() {
                        return Err(Halt::Blocked(
                            Finding::error("E-SETUP-NODE-EXITED", exit::COMPONENT_DOWN, "The node setup started exited")
                                .current(tail)
                                .fix("read the lines above; a database from another version is refused rather than deleted"),
                        ));
                    }
                    if Instant::now() >= deadline {
                        return Err(Halt::Blocked(
                            Finding::error("E-NODE-RPC-UNREACHABLE", exit::COMPONENT_DOWN, "The node's RPC does not answer")
                                .current(format!("{url}: {e}"))
                                .fix("start the node with --rpclisten-borsh, or point setup at it: misaka --rpc <host:port> mining setup"),
                        ));
                    }
                }
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        };
        if node.server.network_id.to_string() != self.network {
            return Err(Halt::Blocked(
                Finding::error("E-SETUP-WRONG-NETWORK", exit::CONFIG, "The node at that RPC is another network")
                    .current(format!("{} answers as {}", node.url, node.server.network_id))
                    .fix(format!("point setup at a {} node: misaka --rpc <host:port> mining setup", self.network)),
            ));
        }
        if !node.server.has_utxo_index {
            return Err(Halt::Blocked(
                catalog::utxoindex_off(true)
                    .reason("setup reads the key's outputs to size the bond and pick a fee output, and only --utxoindex answers that")
                    .fix("restart the node with --utxoindex, or stop it and re-run setup (the node setup starts has it)"),
            ));
        }
        // The ruleset: the node's, against this CLI's. A node started with a ruleset flag runs its
        // own parameters, and says so.
        let own_ruleset = self.external.as_ref().and_then(|(_, a)| a.ruleset_flags.first().cloned()).or_else(|| {
            // argv[0] is the binary to the parser.
            let argv: Vec<String> =
                std::iter::once("kaspad".to_string()).chain(self.file.advanced.extra_kaspad_args.iter().cloned()).collect();
            procs::parse_kaspad_args(&argv).ruleset_flags.first().cloned()
        });
        if let (Some(rt), Some((fp, heights))) = (node.node_status.as_ref(), status::expected(&self.network)) {
            let node_fp = crate::operator::work::short_id(&rt.consensus_params_id).to_string();
            if let Some(flag) = own_ruleset {
                self.row(Severity::Info, "ruleset", format!("{node_fp} — the node's own ({flag})"));
            } else if rt.consensus_params_id != fp || rt.fence_schedule != heights {
                self.row(
                    Severity::Warning,
                    "ruleset",
                    format!(
                        "the node runs {node_fp} and this CLI {} — one of the two is not the release (misaka doctor node)",
                        crate::operator::work::short_id(&fp)
                    ),
                );
            } else {
                let fences = heights.iter().map(|h| status::group(*h)).collect::<Vec<_>>().join(", ");
                self.row(Severity::Ok, "ruleset", format!("{node_fp} · fences {fences} — this CLI's"));
            }
        }
        let _ = params;
        self.node = Some(node);
        self.wait_synced().await
    }

    async fn wait_synced(&mut self) -> Step {
        let started = Instant::now();
        let mut said = Instant::now() - Duration::from_secs(60);
        loop {
            let (synced, daa, peers) = {
                let node = self.node();
                let info = node.client().get_server_info().await;
                let peers = node.client().get_connected_peer_info().await.map(|p| p.peer_info.len()).unwrap_or(0);
                match info {
                    Ok(i) => (i.is_synced, i.virtual_daa_score, peers),
                    Err(_) => (false, node.daa(), peers),
                }
            };
            if synced {
                self.row(
                    Severity::Ok,
                    "sync",
                    format!("synced · DAA {} · {peers} peer{}", status::group(daa), if peers == 1 { "" } else { "s" }),
                );
                return Ok(());
            }
            if self.args.no_wait {
                return Err(Halt::Waiting(
                    format!("the node to sync (DAA {} so far, {peers} peers)", status::group(daa)),
                    exit::NOT_READY,
                ));
            }
            if said.elapsed() >= Duration::from_secs(30) {
                let mut line = format!("syncing · DAA {} · {peers} peer{}", status::group(daa), if peers == 1 { "" } else { "s" });
                if peers == 0 && started.elapsed() > Duration::from_secs(120) {
                    line.push_str(" — no peer after 2 minutes: check --peer / [advanced] peers");
                }
                self.ui.mark(Severity::Info, "sync", &line);
                said = Instant::now();
            }
            if let Some(tail) = self.own_exited() {
                return Err(Halt::Blocked(
                    Finding::error("E-SETUP-NODE-EXITED", exit::COMPONENT_DOWN, "The node setup started exited while syncing")
                        .current(tail),
                ));
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    }

    // -- model ----------------------------------------------------------------------------------

    async fn step_model(&mut self, _params: &kaspa_consensus_core::config::params::Params) -> Step {
        if !self.node().ops_0122 {
            return Err(Halt::Blocked(
                Finding::error("E-NODE-TOO-OLD", exit::COMPONENT_DOWN, "The node predates the reads setup needs")
                    .reason("the class table, a bond's own record and the node's runtime are ADR-0122 reads, and this node was built before them")
                    .current(format!("{} drops the connection on getPalwClasses", self.node().url))
                    .fix("rebuild the node from this tree, or stop it and re-run setup (it starts one built with this CLI)"),
            ));
        }
        let table = self.node().client().get_palw_classes().await.map_err(|e| {
            Halt::Blocked(
                Finding::error("E-NODE-TOO-OLD", exit::COMPONENT_DOWN, "The node does not serve the class table")
                    .current(format!("getPalwClasses: {e}"))
                    .reason("a node older than ADR-0122 answers every other question but not this one")
                    .fix("run a node built with this CLI (setup starts one when none runs)"),
            )
        })?;
        if !table.available || table.classes.is_empty() {
            return Err(Halt::Blocked(Finding::error("E-SETUP-NO-CLASSES", exit::MODEL, "The node reports no PALW classes")));
        }
        self.classes = class_choices(self.node(), &table.classes).await;
        let external_class = self.external.as_ref().and_then(|(_, a)| a.class.clone());
        let selector = self.args.model.clone().or_else(|| self.file.mining.model.clone()).or(external_class);
        let chosen = match selector {
            Some(s) => resolve_model(&self.classes, &s).cloned().map_err(|why| {
                Halt::Blocked(
                    Finding::error("E-MODEL-UNKNOWN", exit::MODEL, format!("'{s}' is not a model this network runs"))
                        .current(why)
                        .fix("misaka model list — then --model base, a name, or a class id"),
                )
            })?,
            None => {
                self.print_classes();
                let question = match self.purpose {
                    Purpose::Mine => "Which model will this node mine?",
                    Purpose::Verify | Purpose::Validate => "Which model will this seat judge (besides the floor)?",
                };
                let i = self.ui.choose(question, self.classes.len(), 0).await.ok_or(Halt::Interrupted)?;
                self.classes[i].clone()
            }
        };
        let role = BondRole::of(self.purpose);
        let collateral = match chosen.lock(role) {
            BondLock::Sompi(c) if role.seat => format!(" · a seat bond for it locks {}", catalog::msk(c as u128)),
            BondLock::Sompi(c) => format!(" · a bond for it locks {}", catalog::msk(c as u128)),
            BondLock::NotAvailable => format!(" · a bond for it: {NOT_AVAILABLE}"),
            BondLock::Unknown => String::new(),
        };
        let lane = if chosen.fp_certified { " · prompt lane certified" } else { "" };
        self.row(Severity::Ok, "model", format!("{}{collateral}{lane}", chosen.label()));
        self.file.mining.model = Some(if chosen.is_base { "base".to_string() } else { chosen.id.clone() });
        self.class = Some(chosen);
        Ok(())
    }

    fn print_classes(&self) {
        let (head, rows) = class_table(&self.classes, BondRole::of(self.purpose));
        self.ui.say(&paint::dim(&head));
        for row in rows {
            self.ui.say(&row);
        }
    }

    // -- bond -----------------------------------------------------------------------------------

    /// The registry's record of `op`: `Ok(None)` when it holds no bond there.
    async fn read_bond(&self, op: &str) -> Result<Option<BondSeen>, String> {
        let node = self.node();
        let read =
            if node.ops_0122 { Some(node.client().get_palw_claims(op.to_string(), "seat".into(), false, 1).await) } else { None };
        match read {
            Some(Ok(r)) if r.available => Ok(r.bond_known.then(|| BondSeen {
                outpoint: op.to_string(),
                pubkey: r.bond_pubkey.clone(),
                collateral: r.bond_collateral,
                retiring: r.bond_retiring_since_daa,
                capable: Some(r.bond_capable_classes.clone()),
            })),
            Some(Ok(_)) => Err("the node reports no PALW state".into()),
            _ => {
                // A node older than the bond's own read: the facts read, which needs a class and
                // cannot say what the bond declared.
                let class = self.class.as_ref().map(|c| c.id.clone()).unwrap_or_default();
                let o = crate::bond::parse_outpoint(op).map_err(|e| e.msg)?;
                let f = node
                    .client()
                    .get_palw_producer_facts(class, o.transaction_id.to_string(), o.index, true)
                    .await
                    .map_err(|e| format!("getPalwProducerFacts: {e}"))?;
                Ok(f.bond_known.then(|| BondSeen {
                    outpoint: op.to_string(),
                    pubkey: f.bond_registered_pubkey.clone(),
                    collateral: f.bond_collateral,
                    retiring: None,
                    capable: None,
                }))
            }
        }
    }

    /// Every bond registered to this key, from the registry: the network's locked outpoints, each
    /// asked who registered it. A genesis bond's collateral need not sit at the key's address, so
    /// the address alone would miss it.
    async fn bonds_of_key(&self) -> Result<Vec<BondSeen>, String> {
        let ours = self.key.as_ref().map(|k| k.pubkey_hex.clone()).unwrap_or_default();
        let facts = self
            .node()
            .client()
            .get_palw_producer_facts(String::new(), String::new(), 0, false)
            .await
            .map_err(|e| format!("getPalwProducerFacts: {e}"))?;
        let mut out = Vec::new();
        for op in &facts.locked_bond_outpoints {
            if let Ok(Some(b)) = self.read_bond(op).await
                && b.pubkey.eq_ignore_ascii_case(&ours)
            {
                out.push(b);
            }
        }
        Ok(out)
    }

    fn accept_bond(&mut self, b: BondSeen, how: &str) -> Step {
        if let Some(since) = b.retiring {
            return Err(Halt::Blocked(
                Finding::error("E-IDENT-BOND-RETIRING", exit::IDENTITY, "This key's bond is retiring, and the key cannot bond again")
                    .reason("a retiring bond takes no new work, and the registry refuses a second bond from any key it has held (DuplicateBondKey)")
                    .current(format!("{} · retiring since DAA {}", b.outpoint, status::group(since)))
                    .fix("misaka key gen --out <a NEW seed file>, fund it, and re-run setup with --key-file <it>")
                    .docs("docs/testnet11-join-mining.md#3-register-a-bond"),
            ));
        }
        let role = BondRole::of(self.purpose);
        // **A seat below what the panel draw seats is never drawn, and nothing makes it one**: its
        // collateral cannot be topped up and its key cannot bond again. Said here, before a
        // declaration's fee and a profile that would read "ready" for a verifier that never sits.
        if role.seat
            && let Some(class) = self.class.as_ref()
            && let BondLock::Sompi(need) = class.lock(role)
            && b.collateral < need
        {
            return Err(Halt::Blocked(
                Finding::error(
                    "E-IDENT-BOND-BELOW-SEAT-FLOOR",
                    exit::IDENTITY,
                    "This key's bond holds less than a verifier seat needs, so the panel draw never seats it",
                )
                .reason(
                    "the draw seats only a bond that holds the seat floor, a bond's collateral cannot be topped up, and the \
                     registry refuses a second bond from any key it has held (DuplicateBondKey)",
                )
                .current(format!("{} · {how} · collateral {}", b.outpoint, catalog::msk(b.collateral as u128)))
                .required(format!("a bond of at least {} for {}", catalog::msk(need as u128), class.label()))
                .fix(format!(
                    "take this bond's collateral back: misaka --network {} bond retire --key-file {} --bond {} --class-id {} --yes \
                     (it moves after the withdrawal delay)",
                    self.network,
                    host::tilde(&self.key_path),
                    b.outpoint,
                    class.id
                ))
                .fix(format!(
                    "a seat needs a NEW key: misaka key gen --out <new seed>, fund it with {} or more in one output, then \
                     misaka verifier setup --key-file <new seed>",
                    catalog::msk(need.saturating_add(REGISTRATION_MARGIN_SOMPI) as u128)
                ))
                .docs("docs/testnet12-join-mining.md#4-funds"),
            ));
        }
        let recommended = self.class.as_ref().and_then(|class| class.lock(role).sompi());
        let shortfall = (self.purpose == Purpose::Mine).then(|| collateral_shortfall(b.collateral, recommended)).flatten();
        self.row(
            if shortfall.is_some() { Severity::Warning } else { Severity::Ok },
            "bond",
            format!(
                "{} · {how} · collateral {}{}",
                short_op(&b.outpoint),
                catalog::msk(b.collateral as u128),
                if shortfall.is_some() { " · REGISTERED, but undersized for sustained mining" } else { "" }
            ),
        );
        if let Some(short) = shortfall {
            let class = self.class.as_ref().expect("the model step ran");
            let need = recommended.expect("a shortfall has a recommendation");
            self.ui.sub(&paint::yellow(&format!(
                "! {} needs {} for the whole claim lifetime; this bond is short {}",
                class.label(),
                catalog::msk(need as u128),
                catalog::msk(short as u128)
            )));
            self.ui.sub(&paint::yellow(
                "! it may start while exposure room remains, then HOLD at the bond exposure ceiling until claims become final",
            ));
            self.ui.sub(&paint::yellow(
                "! collateral is fixed: this bond cannot be topped up, and this key cannot register a second bond",
            ));
            self.ui.sub("  sustained mining needs a NEW key and a NEW bond sized for the selected model");
        }
        if self.purpose == Purpose::Mine
            && let Some(class) = self.class.as_ref()
            && class.lock(role) == BondLock::NotAvailable
        {
            self.ui.sub(&paint::yellow(&format!(
                "! a producer bond for {} is {NOT_AVAILABLE}: {}",
                class.label(),
                class.closed.unwrap_or("its whole-lifetime sizing is more than any output can hold")
            )));
        }
        self.file.advanced.bond = Some(b.outpoint.clone());
        self.bond = Some(b);
        Ok(())
    }

    async fn step_bond(&mut self) -> Step {
        let ours = self.key.as_ref().map(|k| k.pubkey_hex.clone()).unwrap_or_default();
        let external_bond = self.external.as_ref().and_then(|(_, a)| a.bond.clone());
        if let Some(op) = self.args.bond.clone().or_else(|| self.file.advanced.bond.clone()).or(external_bond) {
            return match self.read_bond(&op).await {
                Ok(Some(b)) if b.pubkey.eq_ignore_ascii_case(&ours) => self.accept_bond(b, "registered to this key"),
                Ok(Some(b)) => Err(Halt::Blocked(catalog::key_mismatch(&op, Some(&ours), Some(&b.pubkey)))),
                Ok(None) => Err(Halt::Blocked(
                    Finding::error("E-IDENT-BOND-UNKNOWN", exit::IDENTITY, "The chain knows no bond at the configured outpoint")
                        .current(op)
                        .fix("remove it (--bond, or [advanced] bond in mining.toml) and re-run: setup finds this key's bond, or registers one"),
                )),
                Err(e) => Err(Halt::Blocked(Finding::error("E-SETUP-BOND-UNREAD", exit::COMPONENT_DOWN, "The bond could not be read").current(e))),
            };
        }
        let found = self.bonds_of_key().await.map_err(|e| {
            Halt::Blocked(
                Finding::error("E-SETUP-BOND-UNREAD", exit::COMPONENT_DOWN, "Which bonds this key holds could not be asked")
                    .current(e),
            )
        })?;
        let active: Vec<BondSeen> = found.iter().filter(|b| b.retiring.is_none()).cloned().collect();
        match (found.len(), active.len()) {
            (0, _) => {
                self.row(Severity::Info, "bond", "none registered to this key yet");
                Ok(())
            }
            (_, 1) => self.accept_bond(active.into_iter().next().expect("one"), "found, registered to this key"),
            (_, 0) => self.accept_bond(found.into_iter().next().expect("one"), "found"),
            _ => Err(Halt::Blocked(
                Finding::error("E-SETUP-BONDS-SEVERAL", exit::IDENTITY, "This key holds several bonds")
                    .current(active.iter().map(|b| b.outpoint.clone()).collect::<Vec<_>>().join(", "))
                    .fix("name the one to mine with: misaka mining setup --bond <txid>:<index>"),
            )),
        }
    }

    // -- funds and registration -----------------------------------------------------------------

    async fn read_funds(&self) -> Result<(Vec<crate::wallet::Funding>, Funds), Halt> {
        let key = self.key.as_ref().expect("the key step ran");
        let addr = kaspa_addresses::Address::try_from(key.address.as_str())
            .map_err(|e| Halt::Blocked(Finding::error("E-KEY-ADDRESS", exit::IDENTITY, format!("{}: {e}", key.address))))?;
        let all = crate::wallet::page_all(&self.node().nv, &addr).await.map_err(|e| {
            Halt::Blocked(
                Finding::error("E-SETUP-FUNDS-UNREAD", exit::COMPONENT_DOWN, "The key's outputs could not be read").current(e.msg),
            )
        })?;
        let funds = funds_of(&all);
        Ok((all, funds))
    }

    /// The collateral a registration for the chosen class locks in this setup's role — or the
    /// finding that says why no registration can be sized.
    fn registration_collateral(&self) -> Result<u64, Halt> {
        let class = self.class.as_ref().expect("the model step ran");
        let role = BondRole::of(self.purpose);
        match class.lock(role) {
            BondLock::Sompi(collateral) => Ok(collateral),
            BondLock::NotAvailable => Err(Halt::Blocked(
                Finding::error(
                    "E-SETUP-COLLATERAL-NOT-AVAILABLE",
                    exit::MODEL,
                    format!("A {} bond for this class is {NOT_AVAILABLE}", role.name()),
                )
                .reason(class.closed.unwrap_or(
                    "the chain's own sizing for it is more than any one output can hold, so no bond for it can be registered",
                ))
                .current(format!("class {}", class.label()))
                .fix("misaka model list — then choose another class with --model (the floor: --model base)"),
            )),
            BondLock::Unknown => Err(Halt::Blocked(
                Finding::error("E-SETUP-COLLATERAL-UNKNOWN", exit::MODEL, "What a bond for this class locks could not be read")
                    .reason("the collateral is sized from the class's facts, and the node reports none for it yet")
                    .current(format!("class {}", class.label())),
            )),
        }
    }

    async fn step_funds(&mut self) -> Result<(String, u64), Halt> {
        let class = self.class.clone().expect("the model step ran");
        let collateral = self.registration_collateral()?;
        let need = collateral.saturating_add(REGISTRATION_MARGIN_SOMPI);
        let recommended = collateral.saturating_add(FLOAT_RECOMMENDED_SOMPI);
        let address = self.key.as_ref().map(|k| k.address.clone()).unwrap_or_default();
        let mut told = false;
        let mut last: Option<Funds> = None;
        loop {
            let (_, funds) = self.read_funds().await?;
            match funds_verdict(&funds, need) {
                FundsVerdict::Enough(op, amount) => {
                    self.row(
                        Severity::Ok,
                        "funds",
                        format!(
                            "{} in one output at {}",
                            catalog::msk(amount as u128),
                            crate::operator::doctor::short_address(&address)
                        ),
                    );
                    if amount < recommended {
                        self.ui.sub(&paint::dim(&format!(
                            "after the collateral, {} is left: it becomes the panel's fee float; {} or more keeps it paying longer",
                            catalog::msk((amount - collateral) as u128),
                            catalog::msk(recommended as u128)
                        )));
                    }
                    return Ok((op, amount));
                }
                FundsVerdict::Split(total) => {
                    return Err(Halt::Blocked(
                        Finding::error("E-FUNDS-ONE-UTXO", exit::FUNDS, "Enough in total, but no single output holds it")
                            .reason("a registration spends ONE output: the collateral and the carrier's fee come from the same input")
                            .current(format!(
                                "{} across several outputs; the largest is {}",
                                catalog::msk(total as u128),
                                funds.best.as_ref().map(|(_, a)| catalog::msk(*a as u128)).unwrap_or_default()
                            ))
                            .required(format!("one output ≥ {}", catalog::msk(need as u128)))
                            .fix(format!(
                                "misaka --network {} wallet utxo consolidate --key-file {} --yes",
                                self.network,
                                host::tilde(&self.key_path)
                            )),
                    ));
                }
                FundsVerdict::CoinbaseOnly(coinbase) => {
                    // Mining rewards are coinbase outputs, which the registration's funding scan
                    // skips; one send to the same address makes them an ordinary output.
                    self.ui.mark(
                        Severity::Info,
                        "funds",
                        &format!("{} of it is mining rewards, which a registration cannot spend", catalog::msk(coinbase as u128)),
                    );
                    self.self_send(need, "turn the rewards into one ordinary output the registration can spend").await?;
                    continue;
                }
                // A seat below the floor is never drawn: refused and said, never offered smaller.
                FundsVerdict::Short(have) if BondRole::of(self.purpose).seat => {
                    let mut refusal =
                        seat_funds_refusal(&class.label(), collateral, have, &address, &self.network, &host::tilde(&self.key_path));
                    if funds.maturing > 0 {
                        refusal = refusal.current(format!("{} more is still maturing", catalog::msk(funds.maturing as u128)));
                    }
                    if let Some(f) = faucet(&self.network) {
                        refusal = refusal.fix(format!("faucet: {f}"));
                    }
                    return Err(Halt::Blocked(refusal));
                }
                FundsVerdict::Short(have) => {
                    if !told {
                        self.ui.mark(
                            Severity::Info,
                            "funds",
                            &format!(
                                "{} spendable — a bond for {} needs {} in ONE output",
                                catalog::msk(have as u128),
                                class.label(),
                                catalog::msk(need as u128)
                            ),
                        );
                        self.ui.sub(&format!("send it to   {}", paint::bold(&address)));
                        self.ui.sub(&paint::dim(&format!(
                            "{} collateral + {} for the carrier's fee and change · a normal transfer, not mining rewards",
                            catalog::msk(collateral as u128),
                            catalog::msk(REGISTRATION_MARGIN_SOMPI as u128)
                        )));
                        if let Some(f) = faucet(&self.network) {
                            self.ui.sub(&format!("faucet       {f}"));
                        }
                        told = true;
                    }
                    if self.args.no_wait {
                        return Err(Halt::Waiting(format!("{} at {address}", catalog::msk(need as u128)), exit::FUNDS));
                    }
                    if last.as_ref() != Some(&funds) && last.is_some() {
                        self.ui.sub(&format!(
                            "now {} spendable{}",
                            catalog::msk(have as u128),
                            if funds.maturing > 0 {
                                format!(" · {} maturing", catalog::msk(funds.maturing as u128))
                            } else {
                                String::new()
                            }
                        ));
                    } else if last.is_none() {
                        self.ui
                            .sub(&paint::dim("waiting for it — checking every 10 s (Ctrl-C stops; running setup again resumes here)"));
                    }
                    last = Some(funds);
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                        _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                    }
                }
            }
        }
    }

    /// One send to the key's own address: an ordinary output of `amount`, made from whatever the
    /// address holds (rewards included). Waits until the chain shows it.
    async fn self_send(&mut self, amount: u64, why: &str) -> Step {
        let address = self.key.as_ref().map(|k| k.address.clone()).unwrap_or_default();
        if self.ui.json {
            return Err(Halt::Declined(format!(
                "{why}: misaka --network {} wallet send --to {address} --amount {amount} --key-file {} --yes",
                self.network,
                host::tilde(&self.key_path)
            )));
        }
        let q = format!("Send {} to this key's own address ({why})? It costs a small fee.", catalog::msk(amount as u128));
        match self.ui.confirm(&q, true).await {
            Answer::Yes => {}
            Answer::Interrupted => return Err(Halt::Interrupted),
            Answer::No => return Err(Halt::Declined(format!("{why}: declined"))),
            Answer::Unanswered => return Err(Halt::Declined(format!("{why}: re-run with --yes to send it"))),
        }
        let before: BTreeSet<String> =
            self.read_funds().await?.0.iter().map(|u| format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index)).collect();
        let ctx = crate::node::Ctx {
            output: OutputFormat::Human,
            network: self.network.clone(),
            rpc: Some(self.node().url.trim_start_matches("ws://").to_string()),
            node_grpc: self.ctx.node_grpc.clone(),
            evm_rpc: self.ctx.evm_rpc.clone(),
            timeout_secs: self.ctx.timeout_secs,
            quiet: true,
            palw_drill_genesis_salt: self.ctx.palw_drill_genesis_salt.clone(),
        };
        crate::wallet::send(&ctx, &self.key_source(), &address, amount, false, true, false)
            .await
            .map_err(|e| Halt::Blocked(Finding::error("E-SETUP-SEND", exit::FUNDS, "The self-send was refused").current(e.msg)))?;
        let deadline = Instant::now() + Duration::from_secs(600);
        while Instant::now() < deadline {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
            let (all, _) = self.read_funds().await?;
            if all.iter().any(|u| {
                !u.entry.is_coinbase
                    && u.amount == amount
                    && !before.contains(&format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index))
            }) {
                self.ui.sub("the send is on the chain");
                return Ok(());
            }
        }
        Err(Halt::Waiting("the self-send to be mined".into(), exit::FUNDS))
    }

    async fn step_register(&mut self, funding: &str, amount: u64) -> Step {
        let class = self.class.clone().expect("the model step ran");
        let role = BondRole::of(self.purpose);
        let collateral = self.registration_collateral()?;
        let address = self.key.as_ref().map(|k| k.address.clone()).unwrap_or_default();
        let label = class.label();
        let (heading, lines) = registration_lines(&RegistrationTerms {
            role,
            class: &label,
            class_is_base: class.is_base,
            collateral,
            seat_requirement: class.seat_collateral,
            funding: &short_op(funding),
            funding_amount: amount,
            payee: &address,
            withdrawal_delay_daa: withdrawal_delay_daa(&self.node().nv.params, self.node().daa()),
        });
        self.ui.say("");
        self.ui.say(&paint::bold(&format!("  {heading}")));
        for line in &lines {
            self.ui.sub(line);
        }
        let registration = Registration {
            class: if class.is_base { String::new() } else { class.id.clone() },
            funding: funding.to_string(),
            collateral,
        };
        match self.ui.confirm("Register it?", false).await {
            Answer::Yes => {}
            Answer::Interrupted => return Err(Halt::Interrupted),
            Answer::No => return Err(Halt::Declined("the bond was not registered".into())),
            Answer::Unanswered => {
                return Err(Halt::Declined("registering locks collateral: re-run with --yes, or in a terminal".into()));
            }
        }
        if let Some((proc_, _)) = &self.external {
            let mut cmd = vec!["kaspad".to_string()];
            cmd.extend(self.node_args(Some(&registration)).unwrap_or_default());
            return Err(Halt::Blocked(
                Finding::error("E-SETUP-REGISTER-OWN-NODE", exit::NOT_READY, "Registering a bond needs a node started for it")
                    .reason("the bond is built and signed by kaspad --palw-register-bond, and the node running here was not started with it")
                    .current(format!("kaspad pid {} · {}", proc_.pid, host::tilde(&self.appdir)))
                    .fix("stop it (misaka mining stop, or its service manager) and re-run setup: it starts one with the flag and stops it once the bond is on the chain")
                    .fix(format!("or run it yourself: {}", cmd.join(" ")))
                    .docs(DOCS_SETUP),
            ));
        }
        // Restart setup's node with the registration flags — unless the node an earlier run left
        // behind is already registering the collateral just confirmed. One started for another
        // figure (or with none: an older setup's, sizing its own default) is stopped and started
        // again, so what is registered is what was confirmed. The chain is synced in its appdir,
        // so this costs seconds.
        if self.own.as_ref().is_some_and(|o| o.register && o.collateral == Some(collateral)) {
            let pid = self.own.as_ref().map(|o| o.pid).unwrap_or_default();
            self.row(Severity::Info, "register", format!("the earlier run's kaspad --palw-register-bond is still at it (pid {pid})"));
        } else {
            self.stop_own().await;
            self.spawn_own(Some(&registration))?;
            let pid = self.own.as_ref().map(|o| o.pid).unwrap_or_default();
            self.row(
                Severity::Info,
                "register",
                format!(
                    "kaspad --palw-register-bond --palw-bond-collateral={collateral} running (pid {pid}) · locks {}",
                    catalog::msk(collateral as u128)
                ),
            );
        }
        let started = self.own.as_ref().map(|o| o.started_unix).unwrap_or_default();
        // The node restarted: reconnect before asking the chain anything.
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if let Ok(n) = snapshot::connect_to(&self.network, self.rpc.as_deref(), Duration::from_secs(3)).await {
                self.node = Some(n);
                break;
            }
            if let Some(tail) = self.own_exited() {
                return Err(Halt::Blocked(
                    Finding::error("E-SETUP-NODE-EXITED", exit::COMPONENT_DOWN, "The registration node exited").current(tail),
                ));
            }
            if Instant::now() >= deadline {
                return Err(Halt::Blocked(Finding::error(
                    "E-NODE-RPC-UNREACHABLE",
                    exit::COMPONENT_DOWN,
                    "The registration node's RPC does not answer",
                )));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let log_file = self.log_file();
        let mut shown: BTreeSet<String> = BTreeSet::new();
        let mut last_chain_check = Instant::now();
        let mut last_beat = Instant::now();
        loop {
            if let Some(tail) = self.own_exited() {
                return Err(Halt::Blocked(
                    Finding::error("E-SETUP-NODE-EXITED", exit::COMPONENT_DOWN, "The registration node exited").current(tail),
                ));
            }
            let notes: Vec<(i64, String)> = nodelog::read(&log_file, 4 << 20)
                .map(|log| log.registration.into_iter().filter(|(ts, _)| *ts >= started - 2).collect())
                .unwrap_or_default();
            let mut settled: Option<String> = None;
            for (_, line) in &notes {
                let Some(note) = nodelog::registration_note(line) else { continue };
                if shown.insert(line.clone()) {
                    match &note {
                        RegistrationNote::Waiting(why) => self.ui.sub(&paint::dim(why)),
                        RegistrationNote::Admitted(txid) => self.ui.sub(&format!(
                            "carrier {} is in the node's mempool — waiting for a block to carry it",
                            crate::operator::work::short_id(txid)
                        )),
                        _ => {}
                    }
                }
                match note {
                    RegistrationNote::Registered(op) | RegistrationNote::AlreadyHolds(op) => settled = Some(op),
                    RegistrationNote::Retiring(op) => {
                        return Err(Halt::Blocked(
                            Finding::error(
                                "E-IDENT-BOND-RETIRING",
                                exit::IDENTITY,
                                "This key's bond is retiring, and the key cannot bond again",
                            )
                            .current(op)
                            .fix("misaka key gen --out <a NEW seed file>, fund it, and re-run setup with --key-file <it>"),
                        ));
                    }
                    RegistrationNote::Failed(line) => {
                        return Err(Halt::Blocked(
                            Finding::error("E-SETUP-REGISTRATION-FAILED", exit::NOT_READY, "The node gave up on this registration")
                                .current(line)
                                .fix("read the node's sentence above: it says whether anything was spent and what to do")
                                .fix("re-running setup is safe — the node asks the chain before it registers again"),
                        ));
                    }
                    _ => {}
                }
            }
            if settled.is_some() || last_chain_check.elapsed() > Duration::from_secs(30) {
                last_chain_check = Instant::now();
                let found = match &settled {
                    Some(op) => self.read_bond(op).await.ok().flatten().into_iter().collect(),
                    None => self.bonds_of_key().await.unwrap_or_default(),
                };
                if let Some(b) = found.into_iter().next() {
                    return self.accept_bond(b, "registered just now");
                }
            }
            if last_beat.elapsed() > Duration::from_secs(60) {
                self.ui.sub(&paint::dim("still waiting on the node (Ctrl-C stops it; running setup again resumes)"));
                last_beat = Instant::now();
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            }
        }
    }

    // -- the validator's stake bond (Purpose::Validate) -------------------------------------------

    /// Every stake bond this validator key holds, from the registry (paged, all statuses).
    async fn stake_bonds_of_key(&self, validator_id: &str) -> Result<Vec<kaspa_rpc_core::RpcStakeBondEntry>, String> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let resp = self
                .node()
                .client()
                .get_stake_bonds(kaspa_rpc_core::GetStakeBondsRequest {
                    owner_pubkey_hash: None,
                    status_in: None,
                    cursor: cursor.clone(),
                    limit: 1000,
                    pov_daa_score: None,
                })
                .await
                .map_err(|e| format!("getStakeBonds: {e}"))?;
            out.extend(resp.bonds.into_iter().filter(|b| b.validator_id.eq_ignore_ascii_case(validator_id)));
            match resp.next_cursor {
                Some(next) if !next.is_empty() && Some(&next) != cursor.as_ref() => cursor = Some(next),
                _ => break,
            }
        }
        Ok(out)
    }

    async fn step_stake_bond(
        &mut self,
        params: &kaspa_consensus_core::config::params::Params,
        dns: &kaspa_consensus_core::dns_finality::DnsParams,
    ) -> Step {
        let key = self
            .key_source()
            .load_key()
            .map_err(|e| Halt::Blocked(catalog::key_unreadable(&self.key_path.display().to_string(), &e.msg)))?;
        let validator_id = key.validator_id.to_string();
        let accept = |me: &mut Self, op: &str, amount: u64, status: &str, how: &str| {
            me.row(Severity::Ok, "stake bond", format!("{} · {how} · {} · {status}", short_op(op), catalog::msk(amount as u128)));
            me.validator.bond = Some(op.to_string());
            me.validator.amount_sompi = Some(amount);
        };
        // A bond named (a flag, or the file), or found by the key's validator id.
        if let Some(op) = self.args.bond.clone().or_else(|| self.validator.bond.clone()) {
            let b =
                self.node().client().get_stake_bond(kaspa_rpc_core::GetStakeBondRequest { bond_outpoint: op.clone() }).await.map_err(
                    |e| {
                        Halt::Blocked(
                            Finding::error("E-SETUP-BOND-UNREAD", exit::COMPONENT_DOWN, "The stake bond could not be read")
                                .current(e.to_string()),
                        )
                    },
                )?;
            if !b.available {
                return Err(Halt::Blocked(
                    Finding::error(
                        "E-VALIDATOR-BOND-UNKNOWN",
                        exit::IDENTITY,
                        "The chain knows no stake bond at the configured outpoint",
                    )
                    .current(op)
                    .fix("remove it (--bond, or [validator] bond) and re-run: setup finds this key's bond, or makes one"),
                ));
            }
            if !b.validator_id.eq_ignore_ascii_case(&validator_id) {
                return Err(Halt::Blocked(
                    Finding::error("E-VALIDATOR-BOND-OTHER-KEY", exit::IDENTITY, "That stake bond belongs to another validator key")
                        .current(format!(
                            "{op} · validator {} · this key {}",
                            &b.validator_id[..16.min(b.validator_id.len())],
                            &validator_id[..16.min(validator_id.len())]
                        ))
                        .fix("name this key's own bond, or the key that made that one (--key-file)"),
                ));
            }
            accept(self, &op, b.amount, &b.effective_status, "registered to this key");
            return Ok(());
        }
        let found = self.stake_bonds_of_key(&validator_id).await.map_err(|e| {
            Halt::Blocked(Finding::error("E-SETUP-BOND-UNREAD", exit::COMPONENT_DOWN, "The stake bonds could not be read").current(e))
        })?;
        if let Some(b) = found.iter().find(|b| b.effective_status == "active").or(found.first()) {
            let (op, amount, status) = (b.bond_outpoint.clone(), b.amount, b.effective_status.clone());
            accept(self, &op, amount, &status, "found, registered to this key");
            return Ok(());
        }
        self.row(Severity::Info, "stake bond", "none registered to this validator key yet");

        // Stake one. The amount: the flag, else the network's minimum (1 BILI where it has none).
        let min = dns.min_bond_amount_sompi;
        let amount = self.args.amount.or(self.validator.amount_sompi).unwrap_or(min.max(100_000_000));
        if amount < min {
            return Err(Halt::Blocked(
                Finding::error("E-VALIDATOR-BOND-BELOW-MIN", exit::FUNDS, "The stake is below this network's minimum bond")
                    .current(catalog::msk(amount as u128))
                    .required(format!("≥ {} on {}", catalog::msk(min as u128), self.network))
                    .fix("misaka validator setup --amount <BILI>"),
            ));
        }
        let prefix = params.prefix();
        let address = key.funding_address(prefix);
        let mass = kaspa_consensus_core::mass::MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        // At most 20 inputs, largest first — a bond's inputs each carry a ~7 KB ML-DSA-87 signature
        // and the transaction has to fit a block. Coinbase outputs count once mature, and bonded
        // collateral never does.
        const MAX_BOND_INPUTS: usize = 20;
        let mut told = false;
        let (fundings, fee) = loop {
            let all = crate::wallet::page_all(&self.node().nv, &address).await.map_err(|e| {
                Halt::Blocked(
                    Finding::error("E-SETUP-FUNDS-UNREAD", exit::COMPONENT_DOWN, "The key's outputs could not be read").current(e.msg),
                )
            })?;
            let mut spendable: Vec<&crate::wallet::Funding> = all.iter().filter(|u| u.selectable()).collect();
            spendable.sort_by(|a, b| b.amount.cmp(&a.amount));
            let (mut sum, mut picked, mut fee) = (0u64, Vec::new(), key.estimate_bond_fee_for_inputs(&mass, prefix, 1));
            for u in spendable.iter().take(MAX_BOND_INPUTS) {
                sum = sum.saturating_add(u.amount);
                picked.push((u.outpoint, u.entry.clone()));
                fee = key.estimate_bond_fee_for_inputs(&mass, prefix, picked.len());
                if sum >= amount.saturating_add(fee) {
                    break;
                }
            }
            if sum >= amount.saturating_add(fee) {
                self.row(
                    Severity::Ok,
                    "funds",
                    format!(
                        "{} in {} output(s) — the stake {} and a {} sompi fee",
                        catalog::msk(sum as u128),
                        picked.len(),
                        catalog::msk(amount as u128),
                        status::group(fee)
                    ),
                );
                break (picked, fee);
            }
            let maturing: u64 = all.iter().filter(|u| !u.mature && !u.bonded).map(|u| u.amount).sum();
            if !told {
                self.ui.mark(
                    Severity::Info,
                    "funds",
                    &format!(
                        "{} spendable — a stake of {} needs that plus a fee of about {} sompi",
                        catalog::msk(sum as u128),
                        catalog::msk(amount as u128),
                        status::group(fee)
                    ),
                );
                self.ui.sub(&format!("send it to   {}", paint::bold(&address.to_string())));
                self.ui.sub(&paint::dim("mining rewards count here once mature; the bond gathers up to 20 outputs"));
                told = true;
            }
            if self.args.no_wait {
                return Err(Halt::Waiting(
                    format!("{} plus a {} sompi fee at {address}", catalog::msk(amount as u128), status::group(fee)),
                    exit::FUNDS,
                ));
            }
            if maturing > 0 {
                self.ui.sub(&paint::dim(&format!("{} is still maturing", catalog::msk(maturing as u128))));
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                _ = tokio::time::sleep(Duration::from_secs(10)) => {}
            }
        };
        self.ui.say("");
        self.ui.say(&paint::bold("  Stake a validator bond with this key:"));
        self.ui.sub(&format!("stake     {} locked in the bond's output 0", catalog::msk(amount as u128)));
        self.ui.sub(&format!("validator {}…", &validator_id[..16.min(validator_id.len())]));
        self.ui.sub(&format!("rewards   {address} — a stake-proportional share of the validator pool"));
        // Consensus clamps a bond's unbonding period up to the network's floor: sign the period
        // that will be enforced, and say it.
        let unbonding = dns.unbonding_period_blocks.max(700);
        self.ui
            .sub(&format!("release   {} blocks after an unbond request (the network's unbonding period)", status::group(unbonding)));
        self.ui.sub("rule      run this key on ONE host: signing an epoch twice is the one slashable fault");
        match self.ui.confirm("Stake it?", false).await {
            Answer::Yes => {}
            Answer::Interrupted => return Err(Halt::Interrupted),
            Answer::No => return Err(Halt::Declined("no bond was staked".into())),
            Answer::Unanswered => return Err(Halt::Declined("staking locks funds: re-run with --yes, or at a terminal".into())),
        }
        let tx =
            key.build_funded_stake_bond_tx_multi(amount, 0, unbonding, key.reward_spk_payload(), &fundings, fee).map_err(|e| {
                Halt::Blocked(Finding::error("E-VALIDATOR-BOND-BUILD", exit::FUNDS, "The stake bond could not be built").current(e))
            })?;
        let txid = self.node().client().submit_transaction((&tx).into(), false).await.map_err(|e| {
            Halt::Blocked(
                Finding::error("E-VALIDATOR-BOND-REFUSED", exit::FUNDS, "The node refused the stake bond").current(e.to_string()),
            )
        })?;
        let op = format!("{txid}:0");
        self.ui.sub(&format!("submitted {} — waiting for a block to carry it", short_op(&op)));
        let deadline = Instant::now() + Duration::from_secs(900);
        while Instant::now() < deadline {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                _ = tokio::time::sleep(Duration::from_secs(4)) => {}
            }
            if let Ok(b) = self.node().client().get_stake_bond(kaspa_rpc_core::GetStakeBondRequest { bond_outpoint: op.clone() }).await
                && b.available
            {
                accept(self, &op, b.amount, &b.effective_status, "staked just now");
                return Ok(());
            }
        }
        self.validator.bond = Some(op.clone());
        Err(Halt::Waiting(format!("stake bond {} to be mined", short_op(&op)), exit::NOT_READY))
    }

    /// The command that runs this validator: the node, with the overlay's validator in-process.
    fn validator_command(&self) -> String {
        validator_command(
            &self.network,
            &ValidatorToml {
                validator: ValidatorSection {
                    network: Some(self.network.clone()),
                    key: Some(self.key_path.display().to_string()),
                    bond: self.validator.bond.clone(),
                    amount_sompi: self.validator.amount_sompi,
                },
                advanced: AdvancedSection { appdir: Some(self.appdir.display().to_string()), ..self.file.advanced.clone() },
            },
        )
    }

    async fn step_write_validator(&mut self) -> Step {
        let v = ValidatorToml {
            validator: ValidatorSection {
                network: Some(self.network.clone()),
                key: Some(host::tilde(&self.key_path)),
                bond: self.validator.bond.clone(),
                amount_sompi: self.validator.amount_sompi,
            },
            advanced: self.file.advanced.clone(),
        };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let text = render_validator_toml(&v, &today);
        let same = self.existing.as_deref().and_then(|e| toml::from_str::<ValidatorToml>(e).ok()).is_some_and(|e| e == v);
        if same {
            self.row(Severity::Ok, "validator.toml", format!("{} already says this", host::tilde(&self.path)));
            return Ok(());
        }
        self.ui.say("");
        match &self.existing {
            Some(old) => {
                self.ui.say(&paint::bold(&format!("  Update {}:", host::tilde(&self.path))));
                for (sign, line) in changed_lines(old, &text) {
                    let shown = format!("    {sign} {line}");
                    self.ui.say(&if sign == '+' { paint::green(&shown) } else { paint::red(&shown) });
                }
                match self.ui.confirm(&format!("Replace {}?", host::tilde(&self.path)), true).await {
                    Answer::Yes => {}
                    Answer::Interrupted => return Err(Halt::Interrupted),
                    _ => return Err(Halt::Declined(format!("{} was left as it was", host::tilde(&self.path)))),
                }
            }
            None => {
                self.ui.say(&paint::bold(&format!("  Write {}:", host::tilde(&self.path))));
                for line in text.lines() {
                    self.ui.say(&paint::dim(&format!("    {line}")));
                }
            }
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = self.path.with_extension("toml.partial");
        std::fs::write(&tmp, &text)
            .and_then(|_| std::fs::rename(&tmp, &self.path))
            .map_err(|e| Halt::Blocked(Finding::error("E-CONFIG-WRITE", exit::CONFIG, format!("{}: {e}", self.path.display()))))?;
        self.row(Severity::Ok, "validator.toml", format!("written {}", host::tilde(&self.path)));
        Ok(())
    }

    // -- artifact -------------------------------------------------------------------------------

    fn step_artifact(&mut self) -> Step {
        let class = self.class.clone().expect("the model step ran");
        if class.is_base {
            self.row(Severity::Ok, "artifact", "none needed — the floor is derived, every node can run it");
            return Ok(());
        }
        let mut named: Vec<PathBuf> = self.args.artifacts.iter().map(|a| PathBuf::from(procs::expand_home(a))).collect();
        match &self.file.advanced.artifact {
            Some(ArtifactList::One(a)) => named.push(PathBuf::from(procs::expand_home(a))),
            Some(ArtifactList::Many(v)) => named.extend(v.iter().map(|a| PathBuf::from(procs::expand_home(a)))),
            None => {}
        }
        if let Some((_, a)) = &self.external {
            named.extend(a.artifacts.iter().map(|x| PathBuf::from(procs::expand_home(x))));
        }
        let mut candidates: Vec<PathBuf> = Vec::new();
        for p in named {
            if p.is_file() && !candidates.contains(&p) {
                candidates.push(p);
            }
        }
        if candidates.is_empty() {
            candidates = artifacts_here(&self.network, Some(&self.appdir));
        }
        // A non-base artifact is executable consensus input. Do not write a profile that merely
        // points at an existing file: if its root is wrong, kaspad will reject the seat later,
        // after the operator has already paid the setup costs. The old optional check was the
        // source of that late and opaque startup wall. Keep --verify-artifact for compatibility,
        // but make this gate unconditional.
        let mut chosen = None;
        for p in &candidates {
            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            let purpose = if self.args.verify_artifact {
                "reading for its root (explicit verification)…"
            } else {
                "reading for its required class root…"
            };
            self.ui.mark(Severity::Info, "artifact", &format!("{} {} ({})", purpose, host::tilde(p), host::human_bytes(size)));
            match artifact_roots(&self.network, p) {
                Ok(roots) if roots.iter().any(|(_, r)| r.eq_ignore_ascii_case(&class.artifact_root)) => {
                    chosen = Some(p.clone());
                    break;
                }
                Ok(_) => self.ui.sub(&paint::yellow(&format!("! its root is not {}'s", class.label()))),
                Err(e) => self.ui.sub(&paint::yellow(&format!("! {e}"))),
            }
        }
        let Some(path) = chosen else {
            let f = if candidates.is_empty() {
                Finding::error("E-MODEL-ARTIFACT-MISSING", exit::MODEL, format!("{} needs its artifact, and none is here", class.label()))
                    .reason("a node mines and judges a model class by running it, from the class's .palwart file")
                    .current("no --artifact, no [advanced] artifact, no MISAKA_PALW_ARTIFACT, and no artifact in the configured search directories")
                    .fix("get the class's artifact (docs/testnet11-free-prompt-mining.md#2-the-artifact-bound-and-the-same-file-everywhere)")
                    .fix("then: set MISAKA_PALW_ARTIFACT=/path/to/file or pass --artifact <file>")
            } else {
                Finding::error("E-MODEL-ARTIFACT-ROOT", exit::MODEL, format!("No artifact here is {}'s", class.label()))
                    .reason("the chain pins the class's artifact root; a file with another root cannot run its claims")
                    .current(format!("checked {}", candidates.iter().map(|p| host::tilde(p)).collect::<Vec<_>>().join(", ")))
                    .required(format!("root {}…", &class.artifact_root[..16.min(class.artifact_root.len())]))
                    .fix("misaka mining setup --artifact <the class's file> --verify-artifact")
            };
            return Err(Halt::Blocked(f));
        };
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let checked = "its root is the class's ✓";
        self.row(Severity::Ok, "artifact", format!("{} · {} · {checked}", host::tilde(&path), host::human_bytes(size)));
        let needed = (8u64 << 30) + size / 5;
        if let Some(avail) = host::mem_available()
            && avail < needed
        {
            self.ui.sub(&paint::yellow(&format!(
                "! {} of memory available; a node holding this artifact wants about {}",
                host::human_bytes(avail),
                host::human_bytes(needed)
            )));
        }
        self.file.advanced.artifact = Some(ArtifactList::One(host::tilde(&path)));
        Ok(())
    }

    // -- capability ----------------------------------------------------------------------------

    async fn step_capability(&mut self) -> Step {
        let class = self.class.clone().expect("the model step ran");
        let Some(bond) = self.bond.clone() else { return Ok(()) };
        let base = self.classes.iter().find(|c| c.is_base).map(|c| c.id.clone());
        let label = |id: &str, classes: &[ClassChoice]| {
            classes.iter().find(|c| c.id == id).map(|c| c.label()).unwrap_or_else(|| format!("{}…", &id[..8.min(id.len())]))
        };
        let Some(declared) = bond.capable.clone() else {
            self.row(Severity::Info, "seats", "the node cannot say which classes this bond judges (it predates ADR-0122's bond read)");
            return Ok(());
        };
        let (want, changed) = capability_set(&declared, &class.id, base.as_deref());
        let names =
            |set: &BTreeSet<String>, classes: &[ClassChoice]| set.iter().map(|c| label(c, classes)).collect::<Vec<_>>().join(", ");
        if !changed {
            let n = names(&want, &self.classes);
            self.row(Severity::Ok, "seats", format!("this bond judges {n}"));
            return Ok(());
        }
        self.ui.say("");
        self.ui.say(&paint::bold("  Declare what this bond can judge:"));
        self.ui.sub(&format!("classes   {}", names(&want, &self.classes)));
        self.ui.sub(&format!(
            "replaces  {}",
            if declared.is_empty() {
                "nothing — a registered bond judges no class until it declares".to_string()
            } else {
                names(&declared.iter().cloned().collect(), &self.classes)
            }
        ));
        self.ui.sub(&format!(
            "costs     a carrier's fee, and {} sompi of the bond's room per class, held while declared",
            status::group(kaspa_consensus_core::palw_state_v2::PALW_CAPABILITY_EXPOSURE_SOMPI)
        ));
        self.ui.sub(&paint::dim(
            "seats are not paid; they are how claims — this node's own included — become final. A seat that disagrees with the quorum is charged.",
        ));
        let default_yes = true;
        match self.ui.confirm("Declare it?", default_yes).await {
            Answer::Yes => {}
            Answer::Interrupted => return Err(Halt::Interrupted),
            Answer::No if self.purpose == Purpose::Mine => {
                self.row(
                    Severity::Info,
                    "seats",
                    "not declared — this bond judges nothing (misaka mining setup again, or misaka bond capability)",
                );
                return Ok(());
            }
            Answer::No => return Err(Halt::Declined("a verifier that declares nothing is never seated".into())),
            Answer::Unanswered => return Err(Halt::Declined("declaring pays a fee: re-run with --yes, or in a terminal".into())),
        }
        let key = self
            .key_source()
            .load_key()
            .map_err(|e| Halt::Blocked(catalog::key_unreadable(&self.key_path.display().to_string(), &e.msg)))?;
        let (all, _) = self.read_funds().await?;
        let set: BTreeSet<kaspa_consensus_core::Hash64> = want
            .iter()
            .filter_map(|id| {
                let mut b = [0u8; 64];
                faster_hex::hex_decode(id.as_bytes(), &mut b).ok().map(|_| kaspa_consensus_core::Hash64::from_bytes(b))
            })
            .collect();
        let bond_op = crate::bond::parse_outpoint(&bond.outpoint)
            .map_err(|e| Halt::Blocked(Finding::error("E-SETUP-BOND", exit::IDENTITY, e.msg)))?;
        let (tx, fee, _) = crate::bond::capability_carrier(&self.node().nv, &key, &all, bond_op, &set).map_err(|e| {
            Halt::Blocked(Finding::error("E-SETUP-CAPABILITY", exit::FUNDS, "The declaration could not be built").current(e.msg))
        })?;
        let txid = tx.id();
        self.node().client().submit_transaction((&tx).into(), false).await.map_err(|e| {
            Halt::Blocked(Finding::error("E-SETUP-CAPABILITY", exit::FUNDS, "The declaration was refused").current(e.to_string()))
        })?;
        self.ui.sub(&format!(
            "submitted {} (fee {} sompi) — waiting for a block to carry it",
            crate::operator::work::short_id(&txid.to_string()),
            fee
        ));
        let deadline = Instant::now() + Duration::from_secs(600);
        while Instant::now() < deadline {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Halt::Interrupted),
                _ = tokio::time::sleep(Duration::from_secs(4)) => {}
            }
            if let Ok(Some(b)) = self.read_bond(&bond.outpoint).await
                && b.capable.as_ref().is_some_and(|c| want.iter().all(|w| c.contains(w)))
            {
                let n = names(&want, &self.classes);
                self.row(Severity::Ok, "seats", format!("this bond judges {n}"));
                self.bond = Some(b);
                return Ok(());
            }
        }
        Err(Halt::Waiting(format!("declaration {} to be mined", crate::operator::work::short_id(&txid.to_string())), exit::NOT_READY))
    }

    // -- fee outpoint ---------------------------------------------------------------------------

    async fn step_fee(&mut self) -> Step {
        let (all, funds) = self.read_funds().await?;
        let op_of = |u: &crate::wallet::Funding| format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index);
        let find = |op: &str| all.iter().find(|u| op_of(u) == op).map(|u| u.amount);
        let bond_op = self.bond.as_ref().map(|b| b.outpoint.clone()).unwrap_or_default();
        if let Some(named) = self.args.fee_outpoint.clone().filter(|v| !v.eq_ignore_ascii_case("auto")) {
            return match find(&named) {
                Some(amount) => self.accept_fee(&named, amount, "named"),
                None => Err(Halt::Blocked(
                    Finding::error("E-FUNDS-FEE-OUTPOINT-SPENT", exit::FUNDS, "The named fee outpoint is not at the key's address")
                        .current(format!("{named}: not among the key's unspent outputs"))
                        .fix("misaka mining setup --fee-outpoint auto"),
                )),
            };
        }
        // The newest line: the file holds the fee chain's lineage, newest first (T12-046).
        let persisted =
            std::fs::read_to_string(self.appdir.join(format!("misaka-{}", self.network)).join("palw-panel").join("palw-fee-outpoint"))
                .ok()
                .and_then(|s| crate::operator::profile::newest_fee_outpoint_line(&s));
        let remembered: Vec<(&str, String)> = persisted
            .into_iter()
            .map(|o| ("the panel's own, left by the registration", o))
            .chain(self.file.advanced.fee_outpoint.clone().map(|o| ("already configured", o)))
            .collect();
        for (whence, op) in &remembered {
            if let Some(amount) = find(op)
                && amount >= FEE_OUTPOINT_MIN_SOMPI
            {
                return self.accept_fee(op, amount, whence);
            }
        }
        let best = all
            .iter()
            .filter(|u| u.selectable() && !u.entry.is_coinbase && u.amount >= FEE_OUTPOINT_MIN_SOMPI && op_of(u) != bond_op)
            .max_by_key(|u| u.amount);
        if let Some(u) = best {
            return self.accept_fee(&op_of(u), u.amount, "chosen: the largest ordinary output at the key");
        }
        let spendable = funds.spendable_plain + funds.spendable_coinbase;
        if spendable >= FEE_FLOAT_SPLIT_SOMPI + REGISTRATION_MARGIN_SOMPI {
            self.self_send(FEE_FLOAT_SPLIT_SOMPI, "make the panel a fee float").await?;
            let (all, _) = self.read_funds().await?;
            if let Some(u) =
                all.iter().filter(|u| u.selectable() && !u.entry.is_coinbase && op_of(u) != bond_op).max_by_key(|u| u.amount)
            {
                let op = op_of(u);
                return self.accept_fee(&op, u.amount, "made by the self-send");
            }
        }
        match self.purpose {
            Purpose::Verify | Purpose::Validate => {
                self.row(Severity::Info, "fee outpoint", "none — this seat files receipts only; another seat carries the quorum");
                Ok(())
            }
            Purpose::Mine => Err(Halt::Blocked(
                Finding::error("E-FUNDS-FEE-FLOAT", exit::FUNDS, "The panel has nothing to pay its carriers' fees with")
                    .reason("a miner's panel carries its quorums and answers its courts, and every carrier pays a fee")
                    .current(format!("{} spendable at the key's address", catalog::msk(spendable as u128)))
                    .required(format!("one ordinary output ≥ {}", catalog::msk(FEE_OUTPOINT_MIN_SOMPI as u128)))
                    .fix(format!(
                        "send {} or more to {} and re-run setup",
                        catalog::msk((FEE_FLOAT_SPLIT_SOMPI + REGISTRATION_MARGIN_SOMPI) as u128),
                        self.key.as_ref().map(|k| k.address.clone()).unwrap_or_default()
                    )),
            )),
        }
    }

    fn accept_fee(&mut self, op: &str, amount: u64, whence: &str) -> Step {
        self.row(Severity::Ok, "fee outpoint", format!("{} · {} · {whence}", short_op(op), catalog::msk(amount as u128)));
        self.file.advanced.fee_outpoint = Some(op.to_string());
        Ok(())
    }

    // -- the file -------------------------------------------------------------------------------

    async fn step_write(&mut self) -> Step {
        self.file.mining.enabled = Some(true);
        self.file.mining.network = Some(self.network.clone());
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let text = render_toml(&self.file, self.purpose, &today);
        let same = self.existing.as_deref().and_then(|e| toml::from_str::<MiningToml>(e).ok()).is_some_and(|e| e == self.file);
        if same {
            self.row(Severity::Ok, "mining.toml", format!("{} already says this", host::tilde(&self.path)));
        } else {
            self.ui.say("");
            match &self.existing {
                // A re-run changes a line or two: show those, not the whole file again.
                Some(old) => {
                    self.ui.say(&paint::bold(&format!("  Update {}:", host::tilde(&self.path))));
                    for (sign, line) in changed_lines(old, &text) {
                        let shown = format!("    {sign} {line}");
                        self.ui.say(&if sign == '+' { paint::green(&shown) } else { paint::red(&shown) });
                    }
                }
                None => {
                    self.ui.say(&paint::bold(&format!("  Write {}:", host::tilde(&self.path))));
                    for line in text.lines() {
                        self.ui.say(&paint::dim(&format!("    {line}")));
                    }
                }
            }
            if self.existing.is_some() {
                match self.ui.confirm(&format!("Replace {}?", host::tilde(&self.path)), true).await {
                    Answer::Yes => {}
                    Answer::Interrupted => return Err(Halt::Interrupted),
                    _ => return Err(Halt::Declined(format!("{} was left as it was", host::tilde(&self.path)))),
                }
            }
            if let Some(dir) = self.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let tmp = self.path.with_extension("toml.partial");
            std::fs::write(&tmp, &text)
                .and_then(|_| std::fs::rename(&tmp, &self.path))
                .map_err(|e| Halt::Blocked(Finding::error("E-CONFIG-WRITE", exit::CONFIG, format!("{}: {e}", self.path.display()))))?;
            self.row(Severity::Ok, "mining.toml", format!("written {}", host::tilde(&self.path)));
        }
        // What `start` will make of it: the same plan, built now, so a refusal is said here.
        let ov = Overrides { config: Some(self.path.clone()), ..Default::default() };
        match Profile::resolve(&ov, Some(&self.network), self.rpc.as_deref()) {
            Ok(mut p) => {
                p.produce = self.purpose == Purpose::Mine;
                p.panel = true;
                if let Err(f) = supervisor::plan(&p, self.purpose.role()) {
                    self.row(Severity::Warning, "start", format!("{} would refuse: {}", self.purpose.start(), f.title));
                }
            }
            Err(e) => self.row(Severity::Warning, "start", format!("the file does not load back: {}", e.msg)),
        }
        Ok(())
    }

    // -- the end --------------------------------------------------------------------------------

    async fn release_node(&mut self) {
        if self.own.is_some() {
            let pid = self.own.as_ref().map(|o| o.pid).unwrap_or_default();
            self.ui.say("");
            self.ui.mark(Severity::Info, "node", &format!("stopping the node setup started (pid {pid})…"));
            self.stop_own().await;
        }
    }

    fn finish(&mut self, result: Step) -> CliResult {
        let (code, state, detail): (i32, &str, Option<Finding>) = match result {
            Ok(()) => (0, "ready", None),
            Err(Halt::Blocked(f)) => (f.exit, "blocked", Some(f)),
            Err(Halt::Waiting(what, code)) => {
                self.ui.say("");
                self.ui.say(&paint::yellow(&format!("◐ waiting for {what} — run setup again to pick up here")));
                (code, "waiting", None)
            }
            Err(Halt::Declined(why)) => {
                self.ui.say("");
                self.ui.say(&paint::yellow(&format!("◐ stopped: {why}")));
                (exit::NOT_READY, "stopped", None)
            }
            Err(Halt::Interrupted) => {
                self.ui.say("");
                self.ui.say(&paint::yellow("◐ interrupted — run setup again to pick up here"));
                (exit::NOT_READY, "interrupted", None)
            }
        };
        if let Some(f) = &detail {
            self.ui.say("");
            self.ui.say(&f.render());
        }
        // Set up and already running as this purpose: the next command is not a start.
        let validating = |a: &procs::KaspadArgs| a.enable_validator;
        let running = self.external.as_ref().filter(|(_, a)| match self.purpose {
            Purpose::Mine => a.produce,
            Purpose::Verify => a.panel,
            Purpose::Validate => validating(a),
        });
        let noun = self.purpose.noun();
        if let (0, Purpose::Validate, None) = (code, self.purpose, running) {
            // One process runs a validator: the node, with the overlay's validator in it.
            self.ui.say("");
            self.ui.say(&paint::green("● ready. Next: run the node as a validator —"));
            self.ui.say(&format!("    {}", self.validator_command()));
            self.ui.say(&paint::dim("    then: misaka validator status   (it attests every epoch once the bond is active)"));
        } else if let (0, Some((proc_, _))) = (code, running) {
            self.ui.say("");
            self.ui.say(&paint::green(&format!("● set up, and already running (kaspad pid {}).", proc_.pid)));
            self.ui.say(&paint::dim(&format!("    misaka {noun} status · misaka doctor · misaka {noun} stop")));
        } else if code == 0 {
            self.ui.say("");
            self.ui.say(&paint::green(&format!("● ready. Next: {}", self.purpose.start())));
            self.ui.say(&paint::dim(&format!(
                "    {0}             in the foreground (Ctrl-C drains safely)\n    {0} --detach    in the background\n    {0} --service   a systemd unit / launchd agent to install\n    misaka doctor · misaka {1} status",
                self.purpose.start(),
                if self.purpose == Purpose::Mine { "mining" } else { "verifier" }
            )));
        }
        if self.ui.json {
            let doc = serde_json::json!({
                "schema": "misaka.setup.v1",
                "purpose": self.purpose,
                "network": self.network,
                "state": state,
                "config": self.path.display().to_string(),
                "steps": self.rows,
                "finding": detail,
                "running": running.map(|(p, _)| p.pid),
                "next": (code == 0 && running.is_none()).then(|| if self.purpose == Purpose::Validate { self.validator_command() } else { self.purpose.start().to_string() }),
            });
            println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
        }
        if code == 0 { Ok(()) } else { Err(CliError::new(code, String::new())) }
    }
}

/// The lines one text has and the other does not — `-` for the old, `+` for the new, in file order.
/// Comments are left out: setup writes the date in one.
pub(crate) fn changed_lines(old: &str, new: &str) -> Vec<(char, String)> {
    let keep = |l: &&str| !l.trim().is_empty() && !l.trim_start().starts_with('#');
    // Realigning `key = value` is not a change: lines compare with their whitespace collapsed.
    let norm = |l: &str| l.split_whitespace().collect::<Vec<_>>().join(" ").replace(" = ", "=");
    let old_lines: Vec<&str> = old.lines().filter(keep).collect();
    let new_lines: Vec<&str> = new.lines().filter(keep).collect();
    let old_norm: Vec<String> = old_lines.iter().map(|l| norm(l)).collect();
    let new_norm: Vec<String> = new_lines.iter().map(|l| norm(l)).collect();
    let mut out: Vec<(char, String)> =
        old_lines.iter().zip(&old_norm).filter(|(_, n)| !new_norm.contains(n)).map(|(l, _)| ('-', l.to_string())).collect();
    out.extend(new_lines.iter().zip(&new_norm).filter(|(_, n)| !old_norm.contains(n)).map(|(l, _)| ('+', l.to_string())));
    out
}

fn short_op(op: &str) -> String {
    match op.rsplit_once(':') {
        Some((txid, i)) if txid.len() > 16 => format!("{}…:{i}", &txid[..8]),
        _ => op.to_string(),
    }
}

/// The roots this build computes for an artifact file, one per class entry its lineage pairs it
/// with — what `--verify-artifact` compares against the class's pinned root. Reads the whole file.
fn artifact_roots(network: &str, path: &Path) -> Result<Vec<(String, String)>, String> {
    let net = network.parse::<kaspa_consensus_core::network::NetworkId>().map_err(|e| e.to_string())?;
    let params = kaspa_consensus_core::config::params::Params::from(net);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(format!("{network} has no PALW bundle"));
    };
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), net.to_string().into_bytes());
    let loaded = sdk.load_artifact(path)?;
    Ok(sdk
        .pairings(&loaded)
        .into_iter()
        .filter_map(|(entry, root)| root.ok().map(|r| (entry.model_id.to_string(), r.to_string())))
        .collect())
}

// ---------------------------------------------------------------------------------------------
// misaka init
// ---------------------------------------------------------------------------------------------

/// **`misaka init`** — what should this machine do? Then the setup for that purpose, or the
/// commands that are that purpose.
pub(crate) async fn init(ctx: &crate::node::Ctx, args: SetupArgs) -> CliResult {
    let purposes: [(&str, &str); 5] = [
        ("Mine", "produce blocks and earn rewards — a bond locks collateral"),
        ("Verify", "sit on panels and judge other miners' claims — not paid; it is how claims become final"),
        ("Validate", "a DNS-finality validator — a stake bond, attesting every epoch"),
        ("Add a model", "register a class, certify it, open its market"),
        ("Hold positions", "buy or sell a model's positions"),
    ];
    let interactive = ctx.output == OutputFormat::Human && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    println!("{}", paint::bold("What should this machine do?"));
    for (i, (name, what)) in purposes.iter().enumerate() {
        println!("  {}  {:<16}{}", i + 1, name, paint::dim(what));
    }
    let ui = Ui { interactive, yes: false, json: false };
    if !interactive {
        println!();
        println!("  misaka mining setup · misaka verifier setup · misaka validator --help · misaka model list · misaka position list");
        return Ok(());
    }
    match ui.choose("Choose", purposes.len(), 0).await.unwrap_or(usize::MAX) {
        usize::MAX => Err(CliError::new(exit::NOT_READY, String::new())),
        0 => run(ctx, Purpose::Mine, args).await,
        1 => run(ctx, Purpose::Verify, args).await,
        2 => run(ctx, Purpose::Validate, args).await,
        3 => {
            println!();
            println!("  misaka model add                 this build's catalog, and which models the chain holds");
            println!("  misaka model add <model>         register it and certify its lanes — drills and files a family only when");
            println!("                                   none on the chain covers it (asks before it spends)");
            println!("  misaka model status <model>      where a class is, and the next command");
            println!("  misaka model market open <model> seed its line so its positions can be bought");
            println!("  (a bond comes first: misaka mining setup registers one)");
            Ok(())
        }
        _ => {
            println!();
            println!("  misaka model list                           every model, its share and its market");
            println!("  misaka position quote <model> --msk <N>    what a buy would do now");
            println!("  misaka position buy <model> --msk <N>      buy, with a computed floor (asks first)");
            println!("  misaka position list                        what this key holds");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(id: &str, name: &str, base: bool) -> ClassChoice {
        ClassChoice {
            id: id.repeat(128 / id.len()),
            name: name.into(),
            is_base: base,
            artifact_root: String::new(),
            fp_certified: false,
            share_permille: None,
            collateral: Some(1),
            seat_collateral: Some(1),
            closed: None,
        }
    }

    const MSK: u64 = kaspa_consensus_core::constants::SOMPI_PER_KASPA;

    /// testnet-12 as this build ships it (the DAA-750 release), and its bundle.
    fn t12() -> kaspa_consensus_core::config::params::Params {
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        kaspa_consensus_core::config::params::Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
    }

    fn producer_floor_of(params: &kaspa_consensus_core::config::params::Params) -> u64 {
        match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state.min_collateral_sompi(),
            _ => panic!("a ConsensusV2 network"),
        }
    }

    /// The figures the lifecycle auditor read off public testnet-12 (TB-W1, 2026-09-26): the node's
    /// own producer sizing for the floor (3,119,145,986,560 sompi = 31,191.45 BILI), for the 8k row
    /// (2,000,332,625.67 BILI) and for the 2M row (`u64::MAX`, saturated).
    const TB_W1_FLOOR: u64 = 3_119_145_986_560;
    const TB_W1_8K: u64 = 200_033_262_567 * 1_000_000;
    const TB_W1_2M: u64 = u64::MAX;
    /// A `verification_ccu` whose derived deadline is the 8k row's 15 DAA, and one whose is the 2M
    /// row's 13,995 (`palw_derived_verify_daa_v1`; the registry row carries the real ones).
    const CCU_8K: u128 = 2_000_000_000_000;
    const CCU_2M: u128 = 3_357_600_000_000_000;

    /// The auditor's three rows as the chooser holds them on testnet-12: the floor (no registry
    /// row), the 8k row and the 2M row, each with its producer sizing, its seat figure and whether
    /// the chain takes a claim of it.
    fn tb_w1_classes(params: &kaspa_consensus_core::config::params::Params, daa: u64) -> Vec<ClassChoice> {
        [("8f", true, TB_W1_FLOOR, 998, None), ("eb", false, TB_W1_8K, 1, Some(CCU_8K)), ("74", false, TB_W1_2M, 1, Some(CCU_2M))]
            .into_iter()
            .map(|(id, base, producer, share, ccu)| {
                let choice = class(id, "", base);
                ClassChoice {
                    share_permille: Some(share),
                    fp_certified: true,
                    collateral: Some(producer),
                    seat_collateral: seat_requirement(params, daa, base, Some(producer)),
                    closed: class_closed_reason(params, daa, &choice.id, "Active", Some(share), ccu),
                    ..choice
                }
            })
            .collect()
    }

    /// **A class the chain takes no claim of is closed for every role**: past §4-quater's fence a
    /// class whose derived deadline is past `window_receipt` waits for a measured row (the 2M row at
    /// testnet-12's launch, U-D1), and a Frozen or Dormant class takes no new work. Everything else —
    /// the floor, the 8k row, any class on a network without the fence — is open.
    #[test]
    fn a_class_the_chain_takes_no_claim_of_is_closed_for_every_role() {
        use kaspa_consensus_core::palw_class_verify_deadline_v1::palw_derived_verify_daa_v1;
        assert_eq!((palw_derived_verify_daa_v1(CCU_8K), palw_derived_verify_daa_v1(CCU_2M)), (15, 13_995), "the rows' own deadlines");
        let t12 = t12();
        let id = |hex: &str| hex.repeat(64);
        for daa in [0, 750, 1_000] {
            assert_eq!(class_closed_reason(&t12, daa, &id("8f"), "Active", Some(998), None), None, "the floor has no registry row");
            assert_eq!(class_closed_reason(&t12, daa, &id("eb"), "Active", Some(1), Some(CCU_8K)), None, "15 DAA fits the window");
            let two_m = class_closed_reason(&t12, daa, &id("74"), "Active", Some(1), Some(CCU_2M));
            assert!(two_m.is_some_and(|why| why.contains("measured verification row")), "at DAA {daa}: {two_m:?}");
        }
        let frozen = class_closed_reason(&t12, 750, &id("eb"), "Frozen { since_daa: 7 }", Some(1), Some(CCU_8K));
        assert!(frozen.is_some_and(|why| why.contains("Frozen")), "{frozen:?}");
        assert!(class_closed_reason(&t12, 750, &id("eb"), "Dormant { since_daa: 7 }", None, None).is_some());
        let t11 = kaspa_consensus_core::config::params::Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(
            kaspa_consensus_core::network::NetworkType::Testnet,
            11,
        ));
        assert_eq!(
            class_closed_reason(&t11, 10_000, &id("74"), "Active", Some(1), Some(CCU_2M)),
            None,
            "testnet-11 has no such fence"
        );

        let classes = tb_w1_classes(&t12, 750);
        for role in [BondRole::PRODUCER, BondRole::SEAT, BondRole { produce: true, seat: true }] {
            assert_eq!(classes[2].lock(role), BondLock::NotAvailable, "the 2M row, {role:?}");
            assert!(matches!(classes[1].lock(role), BondLock::Sompi(_)), "the 8k row, {role:?}");
        }
    }

    /// **The bug TB-W1 found, closed: a verifier seat is sized by the seat floor the draw reads,
    /// a producer's bond by the node's own sizing, unchanged.** The wizard offered a seat
    /// 31,191.45 BILI — the floor's PRODUCER figure — on a chain whose panel draw seats nothing under
    /// 130,000 BILI, spending the key's only bond on a seat that could never sit.
    #[test]
    fn a_verifier_seat_locks_at_least_the_seat_floor_and_a_producer_bond_is_unchanged() {
        let t12 = t12();
        let producer_floor = producer_floor_of(&t12);
        let daa = 750;
        let panel_floor = t12.palw_seat_economy_at(daa).expect("testnet-12 states the seat economy").panel_floor_sompi;
        assert_eq!(panel_floor, kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_v1(producer_floor));
        assert_eq!(panel_floor, 130_000 * MSK, "the join doc's seat floor: ten producer floors of 13,000 BILI");
        assert!(TB_W1_FLOOR < panel_floor, "the figure the wizard offered a seat was below the floor the draw reads");
        for daa in [0, 750, 1_000] {
            for (is_base, producer) in [(true, Some(TB_W1_FLOOR)), (false, Some(TB_W1_8K)), (false, Some(TB_W1_2M)), (true, None)] {
                let seat = seat_requirement(&t12, daa, is_base, producer).expect("testnet-12 seats panels");
                assert!(seat >= panel_floor, "at DAA {daa}: a seat of {seat} is below the floor {panel_floor}");
                // Past R-core+ the seat floor binds on every testnet-12 class: the readiness bar (three
                // producer floors free) is below it, and a seat does not post a producer's sizing.
                assert_eq!(seat, panel_floor, "at DAA {daa}, base {is_base}, producer {producer:?}");
            }
        }
        let classes = tb_w1_classes(&t12, daa);
        let base = &classes[0];
        assert_eq!(base.lock(BondRole::PRODUCER), BondLock::Sompi(TB_W1_FLOOR), "mining setup's figure is unchanged");
        assert_eq!(base.lock(BondRole::SEAT), BondLock::Sompi(panel_floor), "verifier setup's is the seat floor");
        assert_eq!(BondRole::of(Purpose::Verify), BondRole::SEAT);
        assert_eq!(BondRole::of(Purpose::Mine), BondRole::PRODUCER);
        assert_eq!(classes[1].lock(BondRole::PRODUCER), BondLock::Sompi(TB_W1_8K), "a finite producer sizing stays a number");
        assert_eq!(classes[1].lock(BondRole::SEAT), BondLock::Sompi(panel_floor));
        // A bond meant to do both holds the larger of the two figures.
        let both = BondRole { produce: true, seat: true };
        assert_eq!(base.lock(both), BondLock::Sompi(panel_floor.max(TB_W1_FLOOR)));
        assert_eq!(classes[1].lock(both), BondLock::Sompi(TB_W1_8K.max(panel_floor)));
        assert_eq!(bond_lock(both, Some(5 * MSK), Some(3 * MSK), true), BondLock::Sompi(5 * MSK));

        // A network with the economy but not R-core+ (testnet-11): the seat floor, and the producer's
        // whole-lifetime sizing as the bound on one panel's reservation, as setup always asked.
        let t11 = kaspa_consensus_core::config::params::Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(
            kaspa_consensus_core::network::NetworkType::Testnet,
            11,
        ));
        let t11_floor = kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_v1(producer_floor_of(&t11));
        assert!(!t11.palw_rcore_plus_active_at(u64::MAX - 1), "testnet-11 has no R-core+");
        assert_eq!(seat_requirement(&t11, 10_000, true, Some(1)), Some(t11_floor));
        assert_eq!(seat_requirement(&t11, 10_000, true, Some(t11_floor + 7)), Some(t11_floor + 7));
        assert_eq!(seat_requirement(&t11, 10_000, true, None), None, "the bound unknown, the seat is not sized");
    }

    /// **A figure no output can carry is words, never a number** — the 2M row's `u64::MAX` printed
    /// as 184,467,440,737.09 BILI in the chooser, and anything past `MAX_SOMPI`, and a class that
    /// takes no new work.
    #[test]
    fn a_saturated_figure_is_not_available_on_this_chain_and_never_a_number() {
        use kaspa_consensus_core::constants::MAX_SOMPI;
        let seat = Some(130_000 * MSK);
        assert_eq!(bond_lock(BondRole::PRODUCER, Some(u64::MAX), seat, true), BondLock::NotAvailable);
        assert_eq!(bond_lock(BondRole::PRODUCER, Some(MAX_SOMPI + 1), seat, true), BondLock::NotAvailable);
        assert_eq!(bond_lock(BondRole::PRODUCER, Some(MAX_SOMPI), seat, true), BondLock::Sompi(MAX_SOMPI));
        assert_eq!(
            bond_lock(BondRole::SEAT, Some(u64::MAX), seat, true),
            BondLock::Sompi(130_000 * MSK),
            "a seat posts no producer sizing"
        );
        assert_eq!(bond_lock(BondRole { produce: true, seat: true }, Some(u64::MAX), seat, true), BondLock::NotAvailable);
        assert_eq!(bond_lock(BondRole::SEAT, Some(1), Some(u64::MAX), true), BondLock::NotAvailable);
        assert_eq!(bond_lock(BondRole::PRODUCER, None, seat, true), BondLock::Unknown);
        assert_eq!(bond_lock(BondRole::SEAT, Some(1), None, true), BondLock::Unknown);
        assert_eq!(bond_lock(BondRole::SEAT, Some(1), seat, false), BondLock::NotAvailable, "a Frozen or Dormant class");
        assert_eq!(BondLock::NotAvailable.shown(), "not available on this chain");
        assert_eq!(BondLock::NotAvailable.sompi(), None);
        assert_eq!(BondLock::Unknown.shown(), "unknown");

        // The chooser, both ways: the 2M row reads as words, and every column after the collateral
        // starts where its header does (TB-W1's "2,000,332,625.67 MSK1 ‰" ran two columns together).
        let t12 = t12();
        let classes = tb_w1_classes(&t12, 750);
        for role in [BondRole::PRODUCER, BondRole::SEAT] {
            let (head, rows) = class_table(&classes, role);
            let share_col = head.find("SHARE").expect("a SHARE column");
            println!("{head}\n{}", rows.join("\n"));
            for (row, share) in rows.iter().zip(["998 ‰", "1 ‰", "1 ‰"]) {
                assert!(!row.contains("184,467,440,737.09"), "{row}");
                let from: String = row.chars().skip(share_col).collect();
                assert!(from.starts_with(share), "the SHARE column is misaligned: {row:?}");
            }
            assert!(rows[2].contains(NOT_AVAILABLE), "the 2M row: {rows:?}");
            match role.produce {
                true => assert!(rows[0].contains("31,191.45 BILI") && rows[1].contains("2,000,332,625.67 BILI"), "{rows:?}"),
                false => assert!(rows[..2].iter().all(|r| r.contains("130,000.00 BILI")), "{rows:?}"),
            }
        }
        // A founding line's long name widens its column instead of running into the next one.
        let mut named = classes.clone();
        named[1].name = "Qwen/Qwen2.5-1.5B/graph-v7@8192".into();
        let (head, rows) = class_table(&named, BondRole::SEAT);
        let collateral_col = head.find("COLLATERAL").expect("a COLLATERAL column");
        for (row, cell) in rows.iter().zip(["130,000.00 BILI", "130,000.00 BILI", NOT_AVAILABLE]) {
            let from: String = row.chars().skip(collateral_col).collect();
            assert!(from.starts_with(cell), "the COLLATERAL column is misaligned: {row:?}");
        }
    }

    /// **Setup's node registers exactly the collateral the operator confirmed**: the registration
    /// always carries `--palw-bond-collateral` (TB-W1: it carried none, and the node locked its own
    /// default), a collateral in the operator's own extra arguments cannot override it, and the
    /// line reads back as kaspad reads it.
    #[test]
    fn setup_registers_the_confirmed_collateral_explicitly() {
        let advanced = AdvancedSection {
            peers: vec!["10.0.0.1:26311".into()],
            extra_kaspad_args: vec![
                "--palw-bond-collateral".into(),
                "5".into(),
                "--palw-panel".into(),
                "--palw-bond-collateral=6".into(),
                "--palw-devnet-floor-only".into(),
            ],
            ..Default::default()
        };
        let key = Path::new("/keys/seat.seed");
        let seat = Registration { class: String::new(), funding: "aa:0".into(), collateral: 130_000 * MSK };
        let args = setup_node_args("testnet-12", Path::new("/node"), &advanced, key, Some(&seat)).expect("testnet-12 starts");
        for flag in [
            "--palw-register-bond",
            "--palw-producer-key=/keys/seat.seed",
            "--palw-fee-outpoint=aa:0",
            "--palw-bond-collateral=13000000000000",
        ] {
            assert!(args.iter().any(|a| a == flag), "{flag} missing: {args:?}");
        }
        assert_eq!(args.iter().filter(|a| a.starts_with("--palw-bond-collateral")).count(), 1, "the confirmed figure only: {args:?}");
        assert!(!args.iter().any(|a| a == "5" || a == "--palw-panel"), "{args:?}");
        assert!(args.iter().any(|a| a == "--palw-devnet-floor-only"), "the operator's other additions stay: {args:?}");
        assert!(!args.iter().any(|a| a.starts_with("--palw-producer-class")), "the floor names no class");
        let argv: Vec<String> = std::iter::once("kaspad".to_string()).chain(args).collect();
        let read = procs::parse_kaspad_args(&argv);
        assert!(read.register_bond && read.network == "testnet-12");
        assert_eq!(read.bond_collateral, Some(130_000 * MSK), "what the node will lock");
        assert_eq!(read.fee_outpoint.as_deref(), Some("aa:0"));

        let model = Registration { class: "eb".repeat(64), funding: "bb:1".into(), collateral: 7 };
        let args = setup_node_args("testnet-12", Path::new("/node"), &AdvancedSection::default(), key, Some(&model)).unwrap();
        assert!(
            args.contains(&format!("--palw-producer-class={}", "eb".repeat(64)))
                && args.contains(&"--palw-bond-collateral=7".to_string())
        );

        // Setup's plain node registers nothing.
        let plain = setup_node_args("testnet-12", Path::new("/node"), &advanced, key, None).unwrap();
        assert!(
            !plain.iter().any(|a| a.starts_with("--palw-register-bond")
                || a.starts_with("--palw-bond-collateral")
                || a.starts_with("--palw-producer-key")),
            "{plain:?}"
        );
        assert_eq!(
            procs::parse_kaspad_args(&std::iter::once("kaspad".to_string()).chain(plain).collect::<Vec<_>>()).bond_collateral,
            None
        );
    }

    /// **The confirmation says the role, the amount, the one-bond rule and the way back out, in
    /// words** — the lines a verifier and a miner read before a registration on testnet-12, whole.
    #[test]
    fn the_confirmation_says_the_role_the_amount_the_one_bond_rule_and_the_way_out() {
        let t12 = t12();
        let delay = withdrawal_delay_daa(&t12, 750).expect("testnet-12 is ConsensusV2");
        assert_eq!(delay, 12_900, "docs/testnet12-join-mining.md §9 says 12,900 DAA");
        let seat_floor = seat_requirement(&t12, 750, true, Some(TB_W1_FLOOR));
        let terms = |role: BondRole, collateral: u64| RegistrationTerms {
            role,
            class: "base (the floor)",
            class_is_base: true,
            collateral,
            seat_requirement: seat_floor,
            funding: "2eca7516…:0",
            funding_amount: 400_000 * MSK,
            payee: "misakatest:qz…",
            withdrawal_delay_daa: Some(delay),
        };
        let text = |t: &RegistrationTerms| {
            let (heading, lines) = registration_lines(t);
            format!("{heading}\n{}", lines.join("\n"))
        };
        let way_out = "\
payee     misakatest:qz… — its rewards, and the collateral once the bond is retired
one bond  this key can hold only ONE bond, ever: it can never register a second one (DuplicateBondKey),
          retiring this one does not free the key, and the collateral cannot be topped up
withdraw  the collateral stays locked while the bond is registered. To get it back: misaka bond retire,
          then the withdrawal delay, 12,900 DAA after the retirement (about 18 days at the 120-second block target)
          A bond that signed panels may first wait for their locks to run out (misaka bond status lists them)";

        let seat = text(&terms(BondRole::SEAT, 130_000 * MSK));
        println!("{seat}\n");
        assert_eq!(
            seat,
            format!(
                "\
Register a verifier seat bond for this key:
role      verifier seat: it sits on panels and judges other producers' claims; it does not mine
lock      130,000.00 BILI as its collateral: the least a verifier seat needs on this chain
judges    base (the floor)
from      2eca7516…:0 (400,000.00 BILI)
{way_out}"
            )
        );

        let producer = text(&terms(BondRole::PRODUCER, TB_W1_FLOOR));
        println!("{producer}\n");
        assert_eq!(
            producer,
            format!(
                "\
Register a producer bond for this key:
role      producer: it mines base (the floor), and this collateral backs its claims
lock      31,191.45 BILI as its collateral: the node's own sizing for claims of base (the floor) over their whole life
seats     none: under the 130,000.00 BILI a verifier seat needs, the panel draw never seats this bond
from      2eca7516…:0 (400,000.00 BILI)
{way_out}"
            )
        );

        // A seat for a model class judges it and the floor; a delay the chain cannot say is not guessed.
        let model = RegistrationTerms {
            class: "ebf44d0a…",
            class_is_base: false,
            withdrawal_delay_daa: None,
            ..terms(BondRole::SEAT, 130_000 * MSK)
        };
        let (_, lines) = registration_lines(&model);
        assert!(lines.contains(&"judges    ebf44d0a… and the floor".to_string()), "{lines:?}");
        assert!(lines.contains(&"          then the chain's withdrawal delay after the retirement".to_string()), "{lines:?}");
        assert_eq!(daa_as_time(30), "about 1 hour");
        assert_eq!(daa_as_time(2_000), "about 3 days");
    }

    /// **Funds below what a seat needs are refused, never met with a smaller lock** (TB-W1's
    /// second half: 100,000 BILI at the key would have bought a 31,191.45 BILI seat that never sits).
    #[test]
    fn a_verifier_short_of_the_seat_floor_is_refused_and_never_offered_a_smaller_lock() {
        let f = seat_funds_refusal(
            "base (the floor)",
            130_000 * MSK,
            100_000 * MSK,
            "misakatest:qz",
            "testnet-12",
            "~/.misaka/miner.seed",
        );
        assert_eq!((f.code, f.exit), ("E-FUNDS-BELOW-SEAT-FLOOR", exit::FUNDS));
        let text = f.render();
        println!("{text}");
        assert_eq!(
            text,
            "\
✗ A verifier seat bond locks at least 130,000.00 BILI, and this key holds 100,000.00 BILI   [E-FUNDS-BELOW-SEAT-FLOOR]
  Reason    the panel draw seats only a bond that holds the seat floor, so setup never registers a smaller seat bond: it would lock the funds, never be drawn onto a panel, and use up this key's only bond
  Current   100,000.00 BILI spendable at misakatest:qz
  Required  one ordinary output of at least 130,000.10 BILI — 130,000.00 BILI of collateral for base (the floor), and 0.10 BILI for the carrier's fee and change
  Fix       send 130,000.10 BILI or more to misakatest:qz in one transfer, then run misaka verifier setup again
            or send the missing 30,000.10 BILI and merge the outputs first: misaka --network testnet-12 wallet utxo consolidate --key-file ~/.misaka/miner.seed --yes
  Docs      docs/testnet12-join-mining.md#4-funds"
        );
    }

    /// `--model` takes what an operator would type: the floor's names, an id or its prefix, or the
    /// founding line's name — and an ambiguous one is refused with the candidates.
    #[test]
    fn a_model_is_named_the_way_an_operator_would_name_it() {
        let classes = vec![
            class("8f", "", true),
            class("71", "qwen25-a16", false),
            class("5b", "qwen36-35b", false),
            class("7a", "qwen36-9b", false),
        ];
        assert!(resolve_model(&classes, "base").unwrap().is_base);
        assert!(resolve_model(&classes, "FLOOR").unwrap().is_base);
        assert_eq!(resolve_model(&classes, "qwen25-a16").unwrap().name, "qwen25-a16");
        assert_eq!(resolve_model(&classes, "QWEN25").unwrap().name, "qwen25-a16", "a unique part of a name");
        assert_eq!(resolve_model(&classes, "5b5b5b5b").unwrap().name, "qwen36-35b", "an id prefix of eight or more");
        assert_eq!(resolve_model(&classes, &"71".repeat(64)).unwrap().name, "qwen25-a16");
        let err = resolve_model(&classes, "qwen36").unwrap_err();
        assert!(err.contains("2 classes") && err.contains("qwen36-9b"), "{err}");
        assert!(resolve_model(&classes, "llama").is_err());
        assert!(resolve_model(&classes, "abcd").is_err(), "four hex is a name, and no class is named so");
    }

    fn funding(amount: u64, mature: bool, coinbase: bool, bonded: bool, n: u8) -> crate::wallet::Funding {
        let mut id = [0u8; 64];
        id[0] = n;
        crate::wallet::Funding {
            outpoint: kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_bytes(id), 0),
            entry: kaspa_consensus_core::tx::UtxoEntry::new(amount, Default::default(), 0, coinbase),
            mature,
            amount,
            bonded,
            reserved: false,
        }
    }

    /// A registration spends one ordinary output; the verdict says which of the ways to fall short
    /// the address is in, because each has a different fix.
    #[test]
    fn a_registration_is_funded_by_one_ordinary_output() {
        let msk = 100_000_000u64;
        let need = 11 * msk;
        let one = funds_of(&[funding(12 * msk, true, false, false, 1), funding(msk, true, false, false, 2)]);
        assert!(matches!(funds_verdict(&one, need), FundsVerdict::Enough(_, a) if a == 12 * msk));
        let split = funds_of(&[funding(6 * msk, true, false, false, 1), funding(6 * msk, true, false, false, 2)]);
        assert_eq!(funds_verdict(&split, need), FundsVerdict::Split(12 * msk));
        let rewards = funds_of(&[funding(20 * msk, true, true, false, 1)]);
        assert_eq!(funds_verdict(&rewards, need), FundsVerdict::CoinbaseOnly(20 * msk), "rewards are coinbase: the scan skips them");
        let bonded = funds_of(&[funding(20 * msk, true, false, true, 1), funding(msk, false, false, false, 2)]);
        assert_eq!(funds_verdict(&bonded, need), FundsVerdict::Short(0), "collateral and immature outputs are not funds");
        assert_eq!(bonded.maturing, msk);
    }

    /// Registry membership and useful capacity are different facts. Setup calls out an old small
    /// carrier instead of silently treating it as a bond sized for the newly selected class.
    #[test]
    fn a_registered_bond_can_still_be_undersized_for_the_class() {
        assert_eq!(collateral_shortfall(8_333_751, Some(1_110_106_160)), Some(1_101_772_409));
        assert_eq!(collateral_shortfall(1_110_106_160, Some(1_110_106_160)), None);
        assert_eq!(collateral_shortfall(2_000_000_000, Some(1_110_106_160)), None);
        assert_eq!(collateral_shortfall(8_333_751, None), None);
    }

    /// A declaration replaces the set, so the set setup declares keeps every class already there.
    #[test]
    fn a_declaration_adds_and_never_drops() {
        let (set, changed) = capability_set(&[], "aa", Some("bb"));
        assert!(changed && set.len() == 2);
        let (set, changed) = capability_set(&["cc".into(), "aa".into(), "bb".into()], "aa", Some("bb"));
        assert!(!changed && set.contains("cc"), "an operator's own declaration survives: {set:?}");
        let (set, changed) = capability_set(&["aa".into()], "aa", None);
        assert!(!changed && set.len() == 1);
    }

    /// What setup writes is what `start` reads: the text parses back to the same configuration.
    #[test]
    fn the_written_file_reads_back_as_written() {
        let mut file = MiningToml::default();
        file.mining.enabled = Some(true);
        file.mining.network = Some("testnet-11".into());
        file.mining.model = Some("base".into());
        file.mining.key = Some("~/.misaka/miner.seed".into());
        file.advanced.appdir = Some("~/.misaka/testnet-11/node".into());
        file.advanced.bond = Some("aa:0".into());
        file.advanced.fee_outpoint = Some("aa:1".into());
        file.advanced.peers = vec!["127.0.0.1:26311".into()];
        file.advanced.artifact = Some(ArtifactList::One("~/m/a \"b\".palwart".into()));
        file.advanced.extra_kaspad_args = vec!["--palw-devnet-floor-only".into()];
        file.advanced.stop_grace_secs = Some(60);
        file.advanced.prompt.outbox = Some("~/.misaka/testnet-11/outbox".into());
        let text = render_toml(&file, Purpose::Mine, "2026-09-12");
        let back: MiningToml = toml::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(back, file, "{text}");
        assert!(text.contains("misaka mining start"));
        assert!(!text.contains("wallet"), "an unset field is not written");
    }

    /// A re-run shows what it changes, not the file again.
    #[test]
    fn a_rerun_shows_only_the_lines_it_changes() {
        let old = "# written 2026-09-11\n[mining]\nkey = \"a\"\n\n[advanced]\nfee_outpoint = \"x:1\"\n";
        let new = "# written 2026-09-12\n[mining]\nkey = \"a\"\n\n[advanced]\nfee_outpoint = \"y:0\"\nbond = \"b:0\"\n";
        assert_eq!(
            changed_lines(old, new),
            vec![
                ('-', "fee_outpoint = \"x:1\"".to_string()),
                ('+', "fee_outpoint = \"y:0\"".to_string()),
                ('+', "bond = \"b:0\"".to_string())
            ]
        );
        assert!(changed_lines(new, new).is_empty());
        assert!(
            changed_lines("[mining]\nnetwork = \"devnet\"\n", "[mining]\nnetwork       = \"devnet\"\n").is_empty(),
            "realigned, not changed"
        );
    }

    #[test]
    fn a_new_node_directory_is_the_one_already_on_disk() {
        let home = std::env::temp_dir().join(format!("misaka-setup-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(default_appdir("testnet-11", &home), home.join(".misaka/testnet-11/node"));
        std::fs::create_dir_all(home.join(".rusty-kaspa/misaka-testnet-11")).unwrap();
        assert_eq!(default_appdir("testnet-11", &home), home.join(".rusty-kaspa"), "a synced chain is not synced again");
        assert_eq!(default_appdir("devnet", &home), home.join(".misaka/devnet/node"), "per network");
        let _ = std::fs::remove_dir_all(&home);
        assert!(default_peers("testnet-11").is_empty(), "testnet-11 uses DNS seeders instead of a stale literal IP");
        assert!(default_peers("devnet").is_empty());
    }
}
