//! **Lane A — the operator-anchored panel (testnet-12, post-launch stopgap, user decision
//! 2026-09-26 01:00 JST).** Past `Params::palw_operator_anchor` only an attempt produced by one of the
//! named operator bonds — on testnet-12, the eight genesis bonds (premine `5e0d5f1b…:0..7`) — may be
//! a claim's panel anchor.
//!
//! **Why.** The panel seed is a function of the anchor attempt (lane F1: its execution commitment and
//! the claim). While audit P0-10 is open a lottery win costs ~279 junk BLAKE2b draws on the testnet-12
//! floor, so whoever may produce the anchor can re-roll the panel of any claim whose slot it reaches by
//! drawing fresh wins until it likes one (docs/t12-panel-seed-2026-09-25.md). No seed rule closes that
//! (every rule has a last contributor, and under P0-10 every chain position after the claim is
//! attacker-ownable). This rule removes the attacker from the anchor position instead: the operator's
//! future execution commitment is not the attacker's to choose or to predict, so the claim id it fixed
//! before the slot buys it one fair draw. The price is operator trust in the draw until the structural
//! fix (a lottery ticket that costs a certified execution) lands.
//!
//! **The rule, precisely.** For a claim with anchor slot `s = bind_base_daa + anchor_delay`, the anchor
//! is the FIRST block on the candidate's selected chain whose DAA score is at or past `s` and which may
//! anchor a panel. Past `palw_rcore_plus` "may anchor" was "is an attempt block"; past this fence —
//! resolved at THAT block's own DAA score, the key lane F1 resolves the seed rule at — it is "is an
//! attempt block produced by an operator bond" ([`PalwOperatorAnchorRuleV1::operator_of_v1`]): the
//! envelope decodes, its `executor_bond` is an operator bond, and its `executor_pubkey` is that bond's
//! genesis-registered key. The header stage has already verified the envelope's ML-DSA-87 signature
//! against that carried key (the relay path's `palw_carriage_stateless_v1`), so the predicate is a
//! function of the header and the genesis registry alone, and naming an operator's outpoint under
//! another key is not operator production. Everything else follows from the processor's existing
//! machinery, which reads this one predicate (`palw_block_may_anchor_a_panel_v1`):
//!
//! * a non-operator attempt at or past the slot is not an anchor — it binds nothing and (SW-8 step
//!   4c, `sw8_anchor_delay = None`) voids nothing; the claim waits for the next operator attempt;
//! * the first operator attempt at or past the slot binds the claim in its own acceptance (SW-8) or,
//!   where the draw refuses, voids it there (S0, no forfeit) — or re-anchors it at its next slot past
//!   the registry-resilience fence (V03);
//! * a slot no operator attempt reaches before the bind window lapses voids `BindTimeout` at the
//!   window's backstop `bind_base + window_bind` (S0, no forfeit), exactly as a slot no attempt at all
//!   reaches did.
//!
//! **The seed's source, and why it is not simply the anchor.** "First on the selected chain" is a
//! race a non-operator can enter after the fact: an operator's attempt is published before anyone
//! builds on it, its panel for every waiting claim can be read off its header (lane F1's seed is a
//! function of the header and the claim), and a heavier sibling released at that moment (one extra
//! withheld blue parent, or the hash tie-break between equal-work siblings) takes the selected-parent
//! slot and merges the operator's attempt instead. Under a pure chain rule each such displacement
//! would be a fresh draw at the next operator attempt — hundreds per 580-DAA bind window. So the
//! processor's walk reads the seed off the EARLIEST operator attempt at or past the slot in the
//! anchor's past (the minimal `(DAA score, hash)` among the operator attempts that are, or are merged
//! by, the chain blocks from the slot to the anchor): undisturbed that is the anchor itself;
//! displaced, it is the displaced attempt, which the next chain block merges whoever wins the race.
//! A displacement then moves where the claim binds, never what it draws. `validate_palw_v2` requires
//! lane F1 at or below this fence, so the seed is always that attempt's execution commitment.
//!
//! Deterministic: the predicate reads the header (algorithm id, DAA score, carried envelope) and the
//! genesis registry, both chain data every node holds, so reorg, IBD and a pruning-proof sync resolve
//! it alike; the switch key is the candidate anchor's own DAA score, so a claim whose slot is below the
//! fence and whose first attempt past the slot is past it takes the new rule.

use crate::config::params::ForkActivation;
use crate::header::Header;
use crate::palw_attempt_v2::PalwAttemptEnvelopeV2;
use crate::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use std::collections::BTreeMap;

/// The rule's version, written beside the fence's height in `consensus_params_id` (the rule rides
/// its fence: two builds arming different anchor rules at one height announce different rulesets).
pub const PALW_OPERATOR_ANCHOR_DOMAIN_V1: &[u8] = b"misaka-palw/panel-v2/operator-anchor/v1";

/// **`Params::palw_operator_anchor`: the fence's height and the operator bonds it trusts.**
///
/// `operators` is sorted and distinct (the ruleset's canonical spelling, hashed as listed) and every
/// entry is a bond the ruleset's genesis registers — the key an operator attempt must carry is that
/// registration's, so the list names bonds and never keys. `validate_palw_v2` refuses anything else
/// ([`Self::refusal_v1`]). testnet-12's armed value is [`Self::of_genesis_bonds_v1`]: all eight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwOperatorAnchorV1 {
    pub activation: ForkActivation,
    pub operators: Vec<PalwBondKeyV2>,
}

/// Every bond `genesis_objects` registers, with the key it registered (the first registration of a
/// bond: `verify_palw_genesis_v2` refuses a genesis that registers one twice).
fn genesis_bond_keys_v1(genesis_objects: &[PalwConsensusObjectV2]) -> BTreeMap<PalwBondKeyV2, Vec<u8>> {
    let mut keys = BTreeMap::new();
    for object in genesis_objects {
        if let PalwConsensusObjectV2::BondRegistered { bond, pubkey, .. } = object {
            keys.entry(*bond).or_insert_with(|| pubkey.clone());
        }
    }
    keys
}

impl PalwOperatorAnchorV1 {
    /// **The fence over every bond the genesis registers** — testnet-12's armed value (its genesis
    /// registers exactly the operator's eight cards).
    pub fn of_genesis_bonds_v1(activation: ForkActivation, genesis_objects: &[PalwConsensusObjectV2]) -> Self {
        Self { activation, operators: genesis_bond_keys_v1(genesis_objects).into_keys().collect() }
    }

    /// **What `validate_palw_v2` refuses in the value itself** (the prerequisites are the caller's):
    /// an empty list (no block could anchor a panel past the fence: every claim would void at its
    /// backstop), a list that is not sorted and distinct (two spellings of one rule would be two
    /// rulesets), and a bond the genesis does not register (its key would be nobody's, or a
    /// stranger's registered later).
    pub fn refusal_v1(&self, genesis_objects: &[PalwConsensusObjectV2]) -> Option<&'static str> {
        if self.operators.is_empty() {
            return Some(
                "palw_operator_anchor names no operator bond: past it no block could anchor a panel and every claim would void \
                 at its bind window",
            );
        }
        if !self.operators.windows(2).all(|pair| pair[0] < pair[1]) {
            return Some("palw_operator_anchor's operator bonds are not sorted and distinct: one rule would have two spellings");
        }
        let genesis = genesis_bond_keys_v1(genesis_objects);
        if self.operators.iter().any(|bond| !genesis.contains_key(bond)) {
            return Some(
                "palw_operator_anchor names a bond the genesis does not register: an operator is a genesis bond, whose key the \
                 ruleset itself carries",
            );
        }
        None
    }

    /// The rule a processor holds: the fence and each operator bond's genesis key. An operator the
    /// genesis does not register ([`Self::refusal_v1`] refuses the ruleset) resolves to no key and so
    /// never anchors.
    pub fn rule_v1(&self, genesis_objects: &[PalwConsensusObjectV2]) -> PalwOperatorAnchorRuleV1 {
        let genesis = genesis_bond_keys_v1(genesis_objects);
        let keys = self.operators.iter().filter_map(|bond| genesis.get(bond).map(|key| (*bond, key.clone()))).collect();
        PalwOperatorAnchorRuleV1 { activation: self.activation, keys }
    }
}

/// **The resolved rule** — what `VirtualStateProcessor` holds and what the anchor predicate reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwOperatorAnchorRuleV1 {
    activation: ForkActivation,
    keys: BTreeMap<PalwBondKeyV2, Vec<u8>>,
}

impl PalwOperatorAnchorRuleV1 {
    /// Whether a candidate anchor at `anchor_daa` is under the rule (the key lane F1 resolves at).
    pub fn active_at(&self, anchor_daa: u64) -> bool {
        self.activation.is_active(anchor_daa)
    }

    pub fn activation(&self) -> ForkActivation {
        self.activation
    }

    /// The operator bonds with the keys their attempts must carry.
    pub fn operators(&self) -> impl Iterator<Item = (&PalwBondKeyV2, &[u8])> {
        self.keys.iter().map(|(bond, key)| (bond, key.as_slice()))
    }

    /// **The operator bond that produced `header`'s attempt, if one did**: an attempt-lane header
    /// whose envelope decodes and names an operator bond as its executor, under that bond's genesis
    /// key. The envelope's signature over that key is the header stage's (`palw_carriage_stateless_v1`,
    /// on every header a node stores), so a header naming an operator's outpoint under any other key —
    /// which only its own key could have signed — is not operator production, whether or not the
    /// stateful admission would later disqualify it.
    pub fn operator_of_v1(&self, header: &Header) -> Option<PalwBondKeyV2> {
        if !crate::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id) {
            return None;
        }
        let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).ok()?;
        let bond = PalwBondKeyV2(envelope.attempt.executor_bond);
        (self.keys.get(&bond)? == &envelope.attempt.executor_pubkey).then_some(bond)
    }

    /// **Whether the rule lets `header` anchor a panel**: below the fence (at the header's OWN DAA
    /// score) always — the lane rule the caller applies decides alone, byte for byte the released
    /// behaviour; at or past it only an operator-produced attempt ([`Self::operator_of_v1`]).
    pub fn admits_anchor_v1(&self, header: &Header) -> bool {
        !self.active_at(header.daa_score) || self.operator_of_v1(header).is_some()
    }
}

/// [`PalwOperatorAnchorRuleV1::admits_anchor_v1`] where the rule may be absent (every network that
/// does not arm the fence): `None` admits every header, so the caller's lane rule is unchanged.
pub fn palw_operator_anchor_admits_v1(rule: Option<&PalwOperatorAnchorRuleV1>, header: &Header) -> bool {
    rule.is_none_or(|rule| rule.admits_anchor_v1(header))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_attempt_v2::PalwAttemptUnsignedV2;
    use crate::pow_layer0::{POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3};
    use crate::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x0A0A_0000 + n), index: n as u32 })
    }

    fn key(n: u64) -> Vec<u8> {
        vec![0x40 + n as u8; 48]
    }

    fn registered(n: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: bond(n),
            pubkey: key(n),
            operator_pubkey: vec![0x0B; 8],
            collateral: 1,
            payout_payload: Hash64::default(),
            capable_classes: Default::default(),
            signature: Vec::new(),
        }
    }

    fn genesis() -> Vec<PalwConsensusObjectV2> {
        (0..4).map(registered).collect()
    }

    /// An attempt header at `daa` on lane `algo`, whose envelope names `executor` under `pubkey`.
    fn attempt_header(algo: u8, daa: u64, executor: PalwBondKeyV2, pubkey: Vec<u8>) -> Header {
        let attempt = PalwAttemptUnsignedV2 {
            version: 2,
            network_domain: Hash64::default(),
            challenge: Hash64::default(),
            class_id: Hash64::from_u64_word(0xF1),
            executor_bond: executor.0,
            executor_pubkey: pubkey,
            operator_id: Hash64::default(),
            artifact_root: Hash64::default(),
            trace_root: Hash64::default(),
            output_root: Hash64::default(),
            pwu: 1,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 0,
            execution_root: Hash64::default(),
        };
        let mut header = Header::from_precomputed_hash(Hash64::from_u64_word(daa), vec![]);
        header.pow_algo_id = algo;
        header.daa_score = daa;
        header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature: vec![0u8; 16] }.encode_wire();
        header
    }

    /// **The value**: genesis-derived, sorted, and refused when empty, unsorted, duplicated or naming
    /// a bond the genesis does not register.
    #[test]
    fn the_operator_list_is_the_genesis_registry_sorted_and_nothing_else() {
        let g = genesis();
        let all = PalwOperatorAnchorV1::of_genesis_bonds_v1(ForkActivation::new(60), &g);
        assert_eq!(all.operators.len(), 4);
        assert!(all.operators.windows(2).all(|p| p[0] < p[1]), "sorted and distinct");
        assert_eq!(all.refusal_v1(&g), None);
        let subset = PalwOperatorAnchorV1 { activation: ForkActivation::new(60), operators: vec![bond(0), bond(2)] };
        assert_eq!(subset.refusal_v1(&g), None, "a subset of the genesis bonds is a legal operator set");
        let empty = PalwOperatorAnchorV1 { activation: ForkActivation::new(60), operators: vec![] };
        assert!(empty.refusal_v1(&g).is_some_and(|why| why.contains("names no operator")));
        let mut unsorted = all.clone();
        unsorted.operators.reverse();
        assert!(unsorted.refusal_v1(&g).is_some_and(|why| why.contains("sorted")));
        let mut dup = all.clone();
        dup.operators.push(*dup.operators.last().unwrap());
        assert!(dup.refusal_v1(&g).is_some_and(|why| why.contains("sorted")));
        let stranger = PalwOperatorAnchorV1 { activation: ForkActivation::new(60), operators: vec![bond(0), bond(9)] };
        assert!(stranger.refusal_v1(&g).is_some_and(|why| why.contains("does not register")));
        // A stranger resolves to no key: it never anchors even where a ruleset slipped past validation.
        let rule = stranger.rule_v1(&g);
        assert_eq!(rule.operators().count(), 1);
        assert!(!rule.admits_anchor_v1(&attempt_header(POW_ALGO_ID_PALW_EXEC_V3, 60, bond(9), key(9))));
    }

    /// **The predicate**: below the fence every header passes (the lane rule decides alone); at or past
    /// it — at the header's OWN DAA — only an attempt on either attempt lane naming an operator bond
    /// under that bond's genesis key.
    #[test]
    fn past_the_fence_only_an_operator_attempt_may_anchor() {
        let g = genesis();
        let rule = PalwOperatorAnchorV1 { activation: ForkActivation::new(60), operators: vec![bond(0), bond(1)] }.rule_v1(&g);
        for algo in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3] {
            let operator = |daa| attempt_header(algo, daa, bond(1), key(1));
            // A genesis bond that is not on the list, a bond the genesis never registered, and an
            // impersonation (an operator's outpoint under another key).
            let listed_out = |daa| attempt_header(algo, daa, bond(2), key(2));
            let stranger = |daa| attempt_header(algo, daa, bond(7), key(7));
            let impersonation = |daa| attempt_header(algo, daa, bond(0), key(7));
            for daa in [0, 59] {
                for h in [operator(daa), listed_out(daa), stranger(daa), impersonation(daa)] {
                    assert!(rule.admits_anchor_v1(&h), "algo {algo}, DAA {daa}: below the fence the rule admits every header");
                }
            }
            for daa in [60, 61, 1_000_000] {
                assert_eq!(rule.operator_of_v1(&operator(daa)), Some(bond(1)));
                assert!(rule.admits_anchor_v1(&operator(daa)), "algo {algo}, DAA {daa}: an operator's attempt anchors");
                assert!(!rule.admits_anchor_v1(&listed_out(daa)), "algo {algo}, DAA {daa}: a genesis bond off the list does not");
                assert!(!rule.admits_anchor_v1(&stranger(daa)), "algo {algo}, DAA {daa}: a non-operator's attempt does not");
                assert!(!rule.admits_anchor_v1(&impersonation(daa)), "algo {algo}, DAA {daa}: an operator's outpoint under another key does not");
            }
        }
        // Not an attempt, or an envelope that does not decode: never an operator's.
        let mut beat = attempt_header(POW_ALGO_ID_PALW_EXEC_V3, 61, bond(0), key(0));
        beat.pow_algo_id = POW_ALGO_ID_HEARTBEAT_V1;
        assert!(!rule.admits_anchor_v1(&beat));
        let mut garbled = attempt_header(POW_ALGO_ID_PALW_EXEC_V3, 61, bond(0), key(0));
        garbled.palw_commitment.truncate(garbled.palw_commitment.len() - 1);
        assert!(!rule.admits_anchor_v1(&garbled));
        // Absent rule: every header passes.
        assert!(palw_operator_anchor_admits_v1(None, &attempt_header(POW_ALGO_ID_PALW_EXEC_V3, 61, bond(7), key(7))));
    }
}
