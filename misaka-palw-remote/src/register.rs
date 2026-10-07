//! **RFC-0009 §3.3–§3.6 / stage A0 — model registration without a node: prepare → quote → sign → relay → verify.**
//!
//! A registrant needs no `kaspad` and no Panel to add a model. It does need everything the chain will ask: an Active
//! registrant bond with room for the registration's exposure and its burn, and spendable BILI for the carrier's fee. This
//! module is the pure half of that workflow; the CLI adapts its RPC reads to [`RegistrationFactsV1`] and its signer to the
//! object digest.
//!
//! * [`quote_registration_v1`] turns the facts read from the chain into a [`RegistrationQuoteV1`]: the network and terms it
//!   was read under, the tip and an expiry, the class/root/object it is about, and **every cost by its payer** — the
//!   registration burn (out of the bond's collateral, never the wallet), the carrier fee (out of the funding wallet), the
//!   exposure the bond must still have room for, and any further filing. A shortfall is returned *split* (gas vs bond vs
//!   live locks) so the user knows what to top up. GAS here is the native carrier fee: never an EVM `gasLimit × gasPrice`.
//! * [`pre_sign_gate_v1`] re-reads the facts just before signing and stops on anything that moved: an expired quote, other
//!   terms, another tip network, another object to sign, a fee above the user's maximum, a new shortfall. Raising the fee is
//!   a new quote, a new approval and a new signature; a relay has no say in it.
//! * [`exact_duplicate_v1`] reuses an accepted exact class (no second fee) and refuses a near duplicate; it shows the
//!   class's lifecycle as it stands and never implies a Frozen/Dormant class was revived.
//! * [`RegistrationTrackerV1`] keeps `prepared / preflight-passed / needs-gas-or-bond / signed / relay-accepted /
//!   tx-included / registration-accepted / refused / reorged` apart, takes a **quorum** of node observations, walks back on
//!   a reorg, and flags a registry row that is not what was signed (`Misattributed`). It never re-submits anything that
//!   costs a fee on its own; re-sending the *same* signed bytes is the only automatic retry.
//!
//! These state names are a client display, not a consensus lifecycle. Accepted registration, Panel readiness, a first
//! Final claim, reward eligibility, block production and a market are separate facts ([`RegistrationReadinessV1`]).

use kaspa_hashes::Hash64;

use crate::{finish, keyed, put_len};

pub const QUOTE_DOMAIN_V1: &[u8] = b"misaka-palw/remote/registration-quote/v1";

/// The registrant bond as the chain reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondFactsV1 {
    pub outpoint: String,
    pub known: bool,
    /// The bond was registered under the key that will sign.
    pub key_matches: bool,
    pub retiring: bool,
    pub collateral_sompi: u64,
    /// Everything the bond already stands behind on the ledger the chain applies the ceiling to (claims, accusations,
    /// registrations, declared capability).
    pub backing_sompi: u128,
    /// Live slashable `Valid` locks (the burn must leave them covered).
    pub live_locked_sompi: u128,
}

/// One further filing the user would pay for (certification rent, a sponsor) — listed apart, approved apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilingV1 {
    pub what: String,
    pub sompi: u64,
    /// Out of the funding wallet (`true`) or the bond (`false`).
    pub from_wallet: bool,
}

/// What a quote is read from: one node's (or a quorum's agreed) view, plus the locally built object and carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistrationFactsV1 {
    /// `palw_network_domain_v2` of the network and genesis the object is signed for.
    pub network_domain: Hash64,
    /// A digest of the live registration terms the object was built with.
    pub terms_digest: Hash64,
    pub tip_hash: Hash64,
    pub tip_daa: u64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// The digest of the exact object bytes the bond key will sign over (`palw_registration_object_id_v1`).
    pub object_digest: Hash64,
    /// The registration exposure the chain reserves on the bond for this class.
    pub exposure_sompi: u64,
    /// The registration burn at this DAA (0 below the fence that prices it).
    pub burn_sompi: u64,
    pub bond: BondFactsV1,
    /// The carrier's compute mass and the fee the node's fee policy prices it at.
    pub carrier_mass: u64,
    pub carrier_fee_sompi: u64,
    /// The funding wallet's spendable sompi (mature, unbonded, not spent in the mempool).
    pub wallet_spendable_sompi: u64,
    pub filings: Vec<FilingV1>,
}

/// The quote the user approves. Its digest is what the approval names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistrationQuoteV1 {
    pub facts: RegistrationFactsV1,
    /// The quote lapses at this DAA: re-read and re-quote past it.
    pub expiry_daa: u64,
    /// What the wallet pays: the carrier fee plus every wallet filing.
    pub wallet_total_sompi: u64,
    /// What leaves the bond's collateral for good: the burn plus every bond filing.
    pub bond_debit_sompi: u64,
    /// The room the bond must have: backing + exposure + burn ≤ collateral.
    pub bond_required_sompi: u128,
    /// The user's cap on what the wallet pays.
    pub max_wallet_sompi: u64,
}

impl RegistrationQuoteV1 {
    pub fn digest(&self) -> Hash64 {
        let f = &self.facts;
        let mut s = keyed(QUOTE_DOMAIN_V1);
        for h in [f.network_domain, f.terms_digest, f.tip_hash, f.class_id, f.artifact_root, f.object_digest] {
            s.update(&h.as_bytes());
        }
        for n in
            [f.tip_daa, f.exposure_sompi, f.burn_sompi, f.carrier_mass, f.carrier_fee_sompi, self.expiry_daa, self.max_wallet_sompi]
        {
            s.update(&n.to_le_bytes());
        }
        put_len(&mut s, f.bond.outpoint.as_bytes());
        s.update(&(f.filings.len() as u64).to_le_bytes());
        for x in &f.filings {
            put_len(&mut s, x.what.as_bytes());
            s.update(&x.sompi.to_le_bytes());
            s.update(&[x.from_wallet as u8]);
        }
        finish(s)
    }

    /// Lines for a person: every cost by its payer, never "1 BILI total".
    pub fn lines(&self) -> Vec<String> {
        let f = &self.facts;
        let msk = |s: u128| format!("{}.{:08} BILI", s / 100_000_000, s % 100_000_000);
        let mut out = vec![
            format!("class      {}", f.class_id),
            format!("root       {}", f.artifact_root),
            format!("object     {}  (what the bond key signs)", f.object_digest),
            format!("read at    DAA {} (tip {}); this quote lapses at DAA {}", f.tip_daa, f.tip_hash, self.expiry_daa),
            format!("carrier    {} fee (mass {}) — from the funding wallet", msk(f.carrier_fee_sompi as u128), f.carrier_mass),
        ];
        if f.burn_sompi > 0 {
            out.push(format!(
                "burn       {} — out of bond {}'s collateral when it folds; never refunded",
                msk(f.burn_sompi as u128),
                f.bond.outpoint
            ));
        }
        out.push(format!(
            "exposure   {} reserved on the bond (needs backing + exposure + burn = {} ≤ collateral {})",
            msk(f.exposure_sompi as u128),
            msk(self.bond_required_sompi),
            msk(f.bond.collateral_sompi as u128)
        ));
        for x in &f.filings {
            out.push(format!(
                "filing     {}: {} — from the {}",
                x.what,
                msk(x.sompi as u128),
                if x.from_wallet { "wallet" } else { "bond" }
            ));
        }
        out.push(format!(
            "wallet pays {} (your cap {}); the bond loses {}",
            msk(self.wallet_total_sompi as u128),
            msk(self.max_wallet_sompi as u128),
            msk(self.bond_debit_sompi as u128)
        ));
        out.push("a fee is spent once the carrier is mined, even if the chain then refuses the registration".into());
        out
    }
}

/// Why no quote (or no signature) — nothing was signed or funded.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QuoteRefusalV1 {
    #[error("the bond cannot register: {0}")]
    BondNotRegistrant(String),
    /// What to top up, split by payer. Each figure is what is missing, 0 where nothing is.
    #[error("needs gas or bond: wallet short {gas_short} sompi, bond short {bond_short} sompi, live locks short {lock_short} sompi")]
    NeedsGasOrBond { gas_short: u64, bond_short: u128, lock_short: u128 },
    #[error("the wallet would pay {need} sompi, above the cap of {cap}")]
    AboveCap { need: u64, cap: u64 },
}

/// **Quote a registration.** `validity_daa`: how long the quote stands (the CLI uses a few minutes of DAA).
pub fn quote_registration_v1(
    facts: RegistrationFactsV1,
    validity_daa: u64,
    max_wallet_sompi: u64,
) -> Result<RegistrationQuoteV1, QuoteRefusalV1> {
    let b = &facts.bond;
    if !b.known {
        return Err(QuoteRefusalV1::BondNotRegistrant(format!("{}: the registry holds no bond there", b.outpoint)));
    }
    if !b.key_matches {
        return Err(QuoteRefusalV1::BondNotRegistrant(format!("{}: it was registered by another key", b.outpoint)));
    }
    if b.retiring {
        return Err(QuoteRefusalV1::BondNotRegistrant(format!("{}: it is retiring", b.outpoint)));
    }
    let wallet_filings: u64 = facts.filings.iter().filter(|f| f.from_wallet).map(|f| f.sompi).sum();
    let bond_filings: u64 = facts.filings.iter().filter(|f| !f.from_wallet).map(|f| f.sompi).sum();
    let wallet_total = facts.carrier_fee_sompi.saturating_add(wallet_filings);
    let bond_debit = facts.burn_sompi.saturating_add(bond_filings);
    // The chain's two gates (palw_state_v2 registration): backing + exposure (+ burn when bought) ≤ collateral, and
    // live locks + burn ≤ collateral.
    let required = b.backing_sompi.saturating_add(facts.exposure_sompi as u128).saturating_add(bond_debit as u128);
    let collateral = b.collateral_sompi as u128;
    let gas_short = wallet_total.saturating_sub(facts.wallet_spendable_sompi);
    let bond_short = required.saturating_sub(collateral);
    let lock_short = b.live_locked_sompi.saturating_add(bond_debit as u128).saturating_sub(collateral);
    if gas_short > 0 || bond_short > 0 || lock_short > 0 {
        return Err(QuoteRefusalV1::NeedsGasOrBond { gas_short, bond_short, lock_short });
    }
    if wallet_total > max_wallet_sompi {
        return Err(QuoteRefusalV1::AboveCap { need: wallet_total, cap: max_wallet_sompi });
    }
    Ok(RegistrationQuoteV1 {
        expiry_daa: facts.tip_daa.saturating_add(validity_daa),
        facts,
        wallet_total_sompi: wallet_total,
        bond_debit_sompi: bond_debit,
        bond_required_sompi: required,
        max_wallet_sompi,
    })
}

/// Why signing stops.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PreSignStopV1 {
    #[error("the quote lapsed at DAA {expiry} (the chain is at {now}): re-quote")]
    Expired { expiry: u64, now: u64 },
    #[error("the network the object is signed for changed")]
    NetworkChanged,
    #[error("the chain's registration terms changed since the quote: re-quote")]
    TermsChanged,
    #[error("the object to sign is not the one quoted")]
    ObjectSwapped,
    #[error("the fee rose from {quoted} to {now} sompi: a higher fee is a new quote, approval and signature")]
    FeeRose { quoted: u64, now: u64 },
    #[error("{0}")]
    Refused(QuoteRefusalV1),
}

/// **The last gate before the bond key signs**: `fresh` is a re-read of the same facts.
pub fn pre_sign_gate_v1(quote: &RegistrationQuoteV1, fresh: &RegistrationFactsV1, about_to_sign: Hash64) -> Result<(), PreSignStopV1> {
    let q = &quote.facts;
    if fresh.tip_daa > quote.expiry_daa {
        return Err(PreSignStopV1::Expired { expiry: quote.expiry_daa, now: fresh.tip_daa });
    }
    if fresh.network_domain != q.network_domain {
        return Err(PreSignStopV1::NetworkChanged);
    }
    if fresh.terms_digest != q.terms_digest || fresh.burn_sompi != q.burn_sompi || fresh.exposure_sompi != q.exposure_sompi {
        return Err(PreSignStopV1::TermsChanged);
    }
    if about_to_sign != q.object_digest
        || fresh.object_digest != q.object_digest
        || fresh.class_id != q.class_id
        || fresh.artifact_root != q.artifact_root
    {
        return Err(PreSignStopV1::ObjectSwapped);
    }
    if fresh.carrier_fee_sompi > q.carrier_fee_sompi {
        return Err(PreSignStopV1::FeeRose { quoted: q.carrier_fee_sompi, now: fresh.carrier_fee_sompi });
    }
    // The same quote rules, on the fresh facts (a balance spent elsewhere, a new lock).
    quote_registration_v1(fresh.clone(), 0, quote.max_wallet_sompi).map(|_| ()).map_err(PreSignStopV1::Refused)
}

/// A class row as the registry reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryRowV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// `None` where the node's class read does not report the registrant.
    pub registrant_bond: Option<String>,
    /// The lifecycle as the registry names it (`Registered`, `Candidate`, `Active…`, `Frozen`, `Dormant`, …).
    pub lifecycle: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DuplicateVerdictV1 {
    /// Not on chain: register.
    New,
    /// The exact class is on chain: reuse it, pay nothing. The lifecycle is shown as it stands — reuse does not revive a
    /// Frozen or Dormant class, which keeps its own signed transitions.
    Reuse { lifecycle: String, registrant_bond: Option<String> },
    /// Same class id over other weights, or these weights under another class: a near duplicate is not a duplicate.
    Conflict(String),
}

/// **Is `(class, root)` already registered?** `rows`: the registry's rows (all, or the ones the node returned).
pub fn exact_duplicate_v1(class_id: Hash64, artifact_root: Hash64, rows: &[RegistryRowV1]) -> DuplicateVerdictV1 {
    if let Some(r) = rows.iter().find(|r| r.class_id == class_id) {
        if r.artifact_root == artifact_root {
            return DuplicateVerdictV1::Reuse { lifecycle: r.lifecycle.clone(), registrant_bond: r.registrant_bond.clone() };
        }
        return DuplicateVerdictV1::Conflict(format!("class {class_id} is registered over another root ({})", r.artifact_root));
    }
    if let Some(r) = rows.iter().find(|r| r.artifact_root == artifact_root) {
        return DuplicateVerdictV1::Conflict(format!("these weights are already registered as class {}", r.class_id));
    }
    DuplicateVerdictV1::New
}

/// The client's display states (RFC-0009 §3.6). Not a consensus lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegistrationStateV1 {
    Prepared,
    PreflightPassed,
    NeedsGasOrBond,
    Signed,
    RelayAccepted,
    TxIncluded {
        block: Hash64,
        daa: u64,
    },
    RegistrationAccepted {
        daa: u64,
    },
    /// The carrier was mined and the fold refused the object; the fee is spent.
    Refused {
        code: String,
    },
    /// Was included or accepted; a quorum no longer sees it on the selected chain. Re-check funding and the registry;
    /// nothing that costs a fee is re-sent automatically.
    Reorged,
    /// A registry row for this class names another bond or root than the one signed. Terminal, loud.
    Misattributed(String),
}

impl RegistrationStateV1 {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::PreflightPassed => "preflight-passed",
            Self::NeedsGasOrBond => "needs-gas-or-bond",
            Self::Signed => "signed",
            Self::RelayAccepted => "relay-accepted",
            Self::TxIncluded { .. } => "tx-included",
            Self::RegistrationAccepted { .. } => "registration-accepted",
            Self::Refused { .. } => "refused",
            Self::Reorged => "reorged",
            Self::Misattributed(_) => "misattributed",
        }
    }

    /// The forward rank a node observation reports (higher = further along the happy path).
    fn rank(&self) -> u8 {
        match self {
            Self::Prepared | Self::NeedsGasOrBond => 0,
            Self::PreflightPassed => 1,
            Self::Signed => 2,
            Self::RelayAccepted | Self::Reorged => 3,
            Self::TxIncluded { .. } => 4,
            Self::Refused { .. } => 5,
            Self::RegistrationAccepted { .. } => 6,
            Self::Misattributed(_) => 7,
        }
    }
}

/// One node's view of the submitted registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistrationObservationV1 {
    pub node: String,
    pub in_mempool: bool,
    /// The carrier's accepting block on the node's selected chain.
    pub included: Option<(Hash64, u64)>,
    /// The class row, if the registry holds one.
    pub row: Option<RegistryRowV1>,
    /// The fold's refusal of the mined object (`REGISTRATION_DROPPED` with the processor's reason), if any.
    pub refused: Option<String>,
    /// The row was written at this DAA.
    pub accepted_daa: Option<u64>,
}

/// Follows one signed registration across nodes and reorgs.
#[derive(Clone, Debug)]
pub struct RegistrationTrackerV1 {
    pub tx_id: Hash64,
    pub object_digest: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub bond: String,
    pub state: RegistrationStateV1,
    /// Agreement needed before a forward state is believed.
    pub min_agree: usize,
}

impl RegistrationTrackerV1 {
    pub fn new(tx_id: Hash64, object_digest: Hash64, class_id: Hash64, artifact_root: Hash64, bond: String, min_agree: usize) -> Self {
        Self { tx_id, object_digest, class_id, artifact_root, bond, state: RegistrationStateV1::Signed, min_agree: min_agree.max(1) }
    }

    /// The relay reached `min_agree` nodes.
    pub fn relayed(&mut self) {
        if self.state.rank() < RegistrationStateV1::RelayAccepted.rank() {
            self.state = RegistrationStateV1::RelayAccepted;
        }
    }

    fn judge(&self, o: &RegistrationObservationV1) -> RegistrationStateV1 {
        if let Some(row) = &o.row
            && row.class_id == self.class_id
        {
            let other_bond = row.registrant_bond.as_ref().is_some_and(|b| *b != self.bond);
            if row.artifact_root != self.artifact_root || other_bond {
                return RegistrationStateV1::Misattributed(format!(
                    "{}: the registry's row names bond {:?} and root {}, this registration signed bond {} and root {}",
                    o.node, row.registrant_bond, row.artifact_root, self.bond, self.artifact_root
                ));
            }
            return RegistrationStateV1::RegistrationAccepted { daa: o.accepted_daa.unwrap_or(0) };
        }
        if let Some(code) = &o.refused {
            return RegistrationStateV1::Refused { code: code.clone() };
        }
        if let Some((block, daa)) = o.included {
            return RegistrationStateV1::TxIncluded { block, daa };
        }
        if o.in_mempool {
            return RegistrationStateV1::RelayAccepted;
        }
        RegistrationStateV1::Signed
    }

    /// **Take one round of observations.** A forward state is believed when `min_agree` nodes report it (or further); a
    /// misattribution from ANY node is reported at once (it is a loud alarm, not a vote). A tracker that had seen inclusion
    /// and no longer gets a quorum for it is `Reorged`.
    pub fn observe(&mut self, round: &[RegistrationObservationV1]) -> &RegistrationStateV1 {
        let judged: Vec<RegistrationStateV1> = round.iter().map(|o| self.judge(o)).collect();
        if let Some(m) = judged.iter().find(|s| matches!(s, RegistrationStateV1::Misattributed(_))) {
            self.state = m.clone();
            return &self.state;
        }
        if matches!(self.state, RegistrationStateV1::Misattributed(_)) {
            return &self.state;
        }
        // The furthest state at least `min_agree` nodes reached (counting nodes at that rank or beyond).
        let mut best: Option<RegistrationStateV1> = None;
        for s in &judged {
            let reach = judged.iter().filter(|t| t.rank() >= s.rank()).count();
            if reach >= self.min_agree && best.as_ref().is_none_or(|b| s.rank() > b.rank()) {
                best = Some(s.clone());
            }
        }
        let had_inclusion = self.state.rank() >= RegistrationStateV1::TxIncluded { block: Hash64::default(), daa: 0 }.rank();
        match best {
            Some(s) if s.rank() >= RegistrationStateV1::TxIncluded { block: Hash64::default(), daa: 0 }.rank() => self.state = s,
            _ if had_inclusion => self.state = RegistrationStateV1::Reorged,
            Some(s) if s.rank() > self.state.rank() => self.state = s,
            _ => {}
        }
        &self.state
    }

    /// Re-sending the SAME signed bytes is safe (idempotent, no new fee). Anything else — a new carrier, a higher fee —
    /// is a new quote and a new approval, never automatic.
    pub fn may_rebroadcast_same_bytes(&self) -> bool {
        matches!(self.state, RegistrationStateV1::Signed | RegistrationStateV1::RelayAccepted | RegistrationStateV1::Reorged)
    }
}

/// RFC-0009 §3.6: the facts a registration's completion is NOT, each shown apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegistrationReadinessV1 {
    pub registration_accepted: bool,
    pub panel_ready: bool,
    pub first_final_claim: bool,
    pub reward_eligible: bool,
    pub block_production: bool,
    pub market_open: bool,
}

impl RegistrationReadinessV1 {
    pub fn lines(&self) -> Vec<String> {
        let yn = |b: bool| if b { "yes" } else { "not yet" };
        vec![
            format!("registration accepted : {}", yn(self.registration_accepted)),
            format!("panel ready           : {}", yn(self.panel_ready)),
            format!("first Final claim     : {}", yn(self.first_final_claim)),
            format!("reward eligible       : {}", yn(self.reward_eligible)),
            format!("block production      : {}", yn(self.block_production)),
            format!("market open           : {}", yn(self.market_open)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn facts() -> RegistrationFactsV1 {
        RegistrationFactsV1 {
            network_domain: h(1),
            terms_digest: h(2),
            tip_hash: h(3),
            tip_daa: 1_000,
            class_id: h(4),
            artifact_root: h(5),
            object_digest: h(6),
            exposure_sompi: 10 * 100_000_000,
            burn_sompi: 100_000_000,
            bond: BondFactsV1 {
                outpoint: "aa:0".into(),
                known: true,
                key_matches: true,
                retiring: false,
                collateral_sompi: 100 * 100_000_000,
                backing_sompi: 50 * 100_000_000,
                live_locked_sompi: 0,
            },
            carrier_mass: 40_000,
            carrier_fee_sompi: 40_000,
            wallet_spendable_sompi: 1_000_000,
            filings: vec![],
        }
    }

    #[test]
    fn a_quote_names_every_cost_by_payer_and_the_burn_is_never_the_wallets() {
        let q = quote_registration_v1(facts(), 100, 1_000_000).unwrap();
        assert_eq!(q.wallet_total_sompi, 40_000, "the wallet pays the carrier fee only");
        assert_eq!(q.bond_debit_sompi, 100_000_000, "the burn comes out of the bond");
        assert_eq!(q.bond_required_sompi, 61 * 100_000_000);
        assert_eq!(q.expiry_daa, 1_100);
        assert!(q.lines().iter().any(|l| l.starts_with("burn") && l.contains("collateral")));
        assert!(!q.lines().iter().any(|l| l.contains("total GAS")));
    }

    #[test]
    fn shortfalls_are_split_by_payer_and_nothing_is_signed() {
        let mut f = facts();
        f.wallet_spendable_sompi = 10_000;
        f.bond.backing_sompi = 95 * 100_000_000;
        let e = quote_registration_v1(f.clone(), 100, u64::MAX).unwrap_err();
        assert_eq!(e, QuoteRefusalV1::NeedsGasOrBond { gas_short: 30_000, bond_short: 6 * 100_000_000, lock_short: 0 });
        // Live locks: the burn must leave them covered even when the reservations fit.
        let mut f = facts();
        f.bond.live_locked_sompi = 99_5000_0000;
        assert!(
            matches!(quote_registration_v1(f, 100, u64::MAX), Err(QuoteRefusalV1::NeedsGasOrBond { lock_short, .. }) if lock_short > 0)
        );
        let mut f = facts();
        f.bond.retiring = true;
        assert!(matches!(quote_registration_v1(f, 100, u64::MAX), Err(QuoteRefusalV1::BondNotRegistrant(_))));
        assert!(matches!(quote_registration_v1(facts(), 100, 39_999), Err(QuoteRefusalV1::AboveCap { .. })));
    }

    #[test]
    fn the_pre_sign_gate_stops_on_anything_that_moved() {
        let q = quote_registration_v1(facts(), 100, 1_000_000).unwrap();
        pre_sign_gate_v1(&q, &facts(), h(6)).unwrap();
        let mut late = facts();
        late.tip_daa = 1_101;
        assert!(matches!(pre_sign_gate_v1(&q, &late, h(6)), Err(PreSignStopV1::Expired { .. })));
        let mut terms = facts();
        terms.terms_digest = h(9);
        assert_eq!(pre_sign_gate_v1(&q, &terms, h(6)), Err(PreSignStopV1::TermsChanged));
        assert_eq!(pre_sign_gate_v1(&q, &facts(), h(7)), Err(PreSignStopV1::ObjectSwapped), "the signer is handed other bytes");
        let mut net = facts();
        net.network_domain = h(8);
        assert_eq!(pre_sign_gate_v1(&q, &net, h(6)), Err(PreSignStopV1::NetworkChanged));
        let mut fee = facts();
        fee.carrier_fee_sompi += 1;
        assert!(matches!(pre_sign_gate_v1(&q, &fee, h(6)), Err(PreSignStopV1::FeeRose { .. })));
        let mut spent = facts();
        spent.wallet_spendable_sompi = 0;
        assert!(matches!(pre_sign_gate_v1(&q, &spent, h(6)), Err(PreSignStopV1::Refused(QuoteRefusalV1::NeedsGasOrBond { .. }))));
        // Any change to the quote is a different approval.
        let mut q2 = q.clone();
        q2.max_wallet_sompi += 1;
        assert_ne!(q.digest(), q2.digest());
    }

    #[test]
    fn an_exact_duplicate_is_reused_with_its_lifecycle_and_a_near_one_is_a_conflict() {
        let rows = vec![RegistryRowV1 {
            class_id: h(4),
            artifact_root: h(5),
            registrant_bond: Some("bb:1".into()),
            lifecycle: "Frozen".into(),
        }];
        assert_eq!(
            exact_duplicate_v1(h(4), h(5), &rows),
            DuplicateVerdictV1::Reuse { lifecycle: "Frozen".into(), registrant_bond: Some("bb:1".into()) }
        );
        assert!(matches!(exact_duplicate_v1(h(4), h(9), &rows), DuplicateVerdictV1::Conflict(_)), "same class, other weights");
        assert!(
            matches!(exact_duplicate_v1(h(7), h(5), &rows), DuplicateVerdictV1::Conflict(_)),
            "same weights, another class (context/tokenizer)"
        );
        assert_eq!(exact_duplicate_v1(h(7), h(8), &rows), DuplicateVerdictV1::New);
    }

    fn obs(node: &str) -> RegistrationObservationV1 {
        RegistrationObservationV1 {
            node: node.into(),
            in_mempool: false,
            included: None,
            row: None,
            refused: None,
            accepted_daa: None,
        }
    }

    fn row(bond: &str, root: u8) -> Option<RegistryRowV1> {
        Some(RegistryRowV1 {
            class_id: h(4),
            artifact_root: h(root),
            registrant_bond: Some(bond.into()),
            lifecycle: "Candidate".into(),
        })
    }

    #[test]
    fn the_tracker_needs_a_quorum_walks_back_on_a_reorg_and_never_pays_again_by_itself() {
        let mut t = RegistrationTrackerV1::new(h(10), h(6), h(4), h(5), "aa:0".into(), 2);
        t.relayed();
        assert_eq!(t.state.name(), "relay-accepted");
        // One node alone claims inclusion and acceptance: not believed.
        let lone =
            RegistrationObservationV1 { included: Some((h(11), 1_010)), row: row("aa:0", 5), accepted_daa: Some(1_011), ..obs("a") };
        t.observe(&[lone.clone(), RegistrationObservationV1 { in_mempool: true, ..obs("b") }]);
        assert_eq!(t.state.name(), "relay-accepted");
        // Two agree.
        t.observe(&[lone.clone(), RegistrationObservationV1 { node: "b".into(), ..lone.clone() }]);
        assert_eq!(t.state, RegistrationStateV1::RegistrationAccepted { daa: 1_011 });
        assert!(!t.may_rebroadcast_same_bytes());
        // The block leaves the selected chain on both nodes.
        t.observe(&[RegistrationObservationV1 { in_mempool: true, ..obs("a") }, obs("b")]);
        assert_eq!(t.state, RegistrationStateV1::Reorged);
        assert!(t.may_rebroadcast_same_bytes(), "only the same bytes may be re-sent");
        // A mined carrier the fold refused.
        let refused =
            RegistrationObservationV1 { included: Some((h(12), 1_020)), refused: Some("REGISTRATION_DROPPED".into()), ..obs("a") };
        t.observe(&[refused.clone(), RegistrationObservationV1 { node: "b".into(), ..refused }]);
        assert_eq!(t.state, RegistrationStateV1::Refused { code: "REGISTRATION_DROPPED".into() });
    }

    #[test]
    fn a_row_that_names_another_bond_or_root_is_loud_from_any_single_node() {
        let mut t = RegistrationTrackerV1::new(h(10), h(6), h(4), h(5), "aa:0".into(), 3);
        t.observe(&[RegistrationObservationV1 { row: row("evil:0", 5), ..obs("a") }, obs("b"), obs("c")]);
        assert!(matches!(t.state, RegistrationStateV1::Misattributed(_)));
        t.observe(&[obs("a"), obs("b"), obs("c")]);
        assert!(matches!(t.state, RegistrationStateV1::Misattributed(_)), "terminal");
        let mut t = RegistrationTrackerV1::new(h(10), h(6), h(4), h(5), "aa:0".into(), 1);
        t.observe(&[RegistrationObservationV1 { row: row("aa:0", 9), ..obs("a") }]);
        assert!(matches!(t.state, RegistrationStateV1::Misattributed(_)), "another root under our class");
    }
}
