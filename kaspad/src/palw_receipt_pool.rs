//! **The receipt pool a panel node licenses from** — node policy, never validity (the 2026-09-24
//! launch review of the licence-stall fix, its HIGH finding: the receipt-pool flush).
//!
//! # What broke
//!
//! Every receipt gossip delivered was pooled per claim, sixteen to a claim, oldest out, behind a
//! door that checked only that the signature was not empty; this node's OWN receipt was pushed into
//! the same list and broadcast exactly once. Gossip relays every fresh receipt to every peer after
//! checking its size alone. So once a claim's five receipts had arrived, sixteen borsh-valid receipts
//! naming it with one-to-eight-byte junk signatures — about 150 bytes each — evicted all five on
//! every node, each seat's own included. The licence selection then dropped the sixteen and found
//! nothing, and nothing ever re-delivered the genuine five: the claim sat `PanelBound` to its redraw
//! at `bound + 600`, the attacker repeated, and the second timeout voided the claim and slashed an
//! honest producer. The V2 pool had the same shape.
//!
//! # What holds now
//!
//! 1. **This node's own receipts are never in the evictable pool.** They are kept apart, by claim,
//!    and retired only by the sweep's rule ([`ReceiptSweepV1`]) — chain facts and this node's own
//!    acts: kept while a duty or a dispute names the claim, and otherwise until this node has
//!    submitted for it or the retention age has passed since it filed. No deadline is read: the
//!    duty's `receipt_deadline` is the network's global window, and a class's own window
//!    (ADR-0133 §11.3, armed from genesis on testnet-12) keeps its claim `PanelBound` — and the
//!    chain taking receipts for it — far past that. The collector's candidates are own ∪ pooled
//!    ([`PalwReceiptPoolV1::candidates`]). No arrival reaches them.
//! 2. **The pool is keyed per (claim, seat bond), [`RECEIPTS_PER_BOND`] slots a bond.** When the tip
//!    holds the claim `PanelBound`, a bond its panel does not name is not admitted at all, and a
//!    receipt signed before the panel bound is not kept. Before the tip has seen the bind, at most
//!    [`UNPLACED_BONDS_PER_CLAIM`] bonds share the claim — a PLACE cap: a newcomer across bonds
//!    takes a place only when the receipt it displaced was its bond's last — and the next tick's
//!    read prunes them to the panel ([`PalwReceiptPoolV1::prune_to_panels`]). The prune holds a bond
//!    off the tip's panel to what this node has verified of it, [`OFF_PANEL_BONDS_PER_CLAIM`] such
//!    bonds at most, so a tip that flips to a sibling panel for a tick and back finds the first
//!    panel's receipts where it left them; the collector is offered only the tip panel's.
//! 3. **Nothing takes an occupied place without a signature that verifies** under its seat bond's
//!    registered key — the check the acceptance validator makes, through the same function. Every
//!    arrival is checked at the door while the tick's [`VerifyBudgetV1`] lasts, and one that fails is
//!    refused there (and its verdict cached by digest, so a copy costs nothing). Past the budget a
//!    receipt may still take a FREE slot unchecked, but never an occupied one: to take its bond's
//!    occupied slot it must verify, and then it evicts an unchecked receipt of its own bond (only
//!    that bond's key-holder can produce one that verifies, so a stranger never evicts anything); to
//!    take a place across bonds it must verify AND the receipt it replaces must have been checked and
//!    failed. What budget a tick leaves over checks the unchecked receipts already pooled, kept
//!    claims first ([`PalwReceiptPoolV1::scrub`]), so junk that slipped into a free slot during a
//!    flood is gone by the next quiet tick.
//! 4. **A seat re-sends its own receipt while its panel stands**, re-signed with fresh randomness —
//!    ML-DSA-87's hedged mode (FIPS 204 `rnd`), so the copy is a new signature and a new digest that
//!    relay-once suppresses nowhere — on a backoff ([`own_receipt_rebroadcast_gap_v1`]), and stops
//!    the moment the chain no longer names that panel as the seat's duty ([`own_receipts_due_v1`]).
//!    A node that lost the receipt for any reason gets it back.
//!
//! What a flood can still do is spend the verify budget: past it a contested receipt is deferred
//! (dropped this tick), and the seat's re-send brings it back. A tick's arrivals are admitted in a
//! random order (the caller shuffles them), so a flooder cannot time its junk ahead of the genuine
//! receipt inside a tick; each re-send is a fresh draw, and the chance that every one of them is
//! crowded out falls geometrically with their number. A flood can no longer remove a receipt that
//! is in the pool, or keep out one that finds a free slot.
//!
//! Everything here decides what a node KEEPS. The assembler still puts every candidate to the
//! acceptance validator, so a pool that keeps too much costs memory and a pool that keeps too little
//! costs liveness — never safety.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::time::{Duration, Instant};

use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptPoolFactsV1, PalwReceiptVerdictV2, PalwSeatReceiptV2,
    PalwSeatReceiptV3, palw_receipt_message_v2, palw_receipt_message_v3,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_hashes::Hash64;

/// Slots one seat bond holds in one claim's pool: its receipt, and room beside it for its receipt to
/// a re-bound panel or a re-sent copy.
pub const RECEIPTS_PER_BOND: usize = 2;
/// Bond PLACES one claim's pool holds while the tip has not told this node who its panel is. Above
/// a panel's five seats with room to spare; the next tick's read prunes them to the panel. No
/// sequence of arrivals takes a claim past it: a free place goes to the first bonds, and a place
/// changes hands only when every receipt of the bond holding it is gone.
pub const UNPLACED_BONDS_PER_CLAIM: usize = 8;
/// Bonds the tip's panel does not seat whose verified receipts a claim keeps aside — for a sibling
/// panel the tip may flip back to (two blocks at the anchor slot bind two panels at one DAA).
/// Nothing is admitted for such a bond while the tip holds the panel, so what is held aside was
/// admitted before the bind (at most [`UNPLACED_BONDS_PER_CLAIM`] bonds) or for a sibling's seats.
pub const OFF_PANEL_BONDS_PER_CLAIM: usize = 8;
/// This node's own receipts kept per claim — one a panel: the first, a re-bound one, a sibling's.
pub const OWN_RECEIPTS_PER_CLAIM: usize = 4;
/// Claims one pool holds beyond the ones the caller keeps (its duties and its own filings). A
/// foreign claim's receipts are worth keeping only until this node sees the bind that makes the
/// claim its duty; an invented claim id never becomes one. A receipt that is kept at all carries a
/// signature of exactly [`kaspa_txscript::MLDSA87_SIG_LEN`] bytes, so one is under 4.8 KB. A claim
/// the tip has never bound — every invented claim id — holds at most `UNPLACED_BONDS_PER_CLAIM ×
/// RECEIPTS_PER_BOND` = 16 of them, so what a flood of invented claims can hold in one pool is at
/// most 256 × 16 × 4.8 KB ≈ 20 MB whatever peers send. A claim the tip binds holds its panel's
/// seats plus at most [`OFF_PANEL_BONDS_PER_CLAIM`] more bonds — (5 + 8) × 2 = 26 receipts on the
/// shipped panel — and is a chain fact (a claim somebody put on chain, a panel the chain drew): 256
/// of those are ≈ 32 MB a pool. The two pools (V2 and V3) together: about 40 MB for a flood of
/// invented claims, about 64 MB at the very worst — against the 64 MiB the V3 pool alone was sized
/// at before.
pub const RECEIPT_POOL_MAX_CLAIMS: usize = 256;
/// Signature checks one claim may cost in one tick. An honest panel's five receipts cost five at
/// the door; the worst honest case — five receipts arriving before the bind, each displacing junk
/// across bonds — costs two each, ten.
pub const RECEIPT_VERIFIES_PER_CLAIM_PER_TICK: u32 = 10;
/// Signature checks one tick may cost in total, over every claim and both pools — a few tens of
/// milliseconds of portable ML-DSA-87 verification a two-second tick, against the assembler's own
/// verification of every candidate it tries.
pub const RECEIPT_VERIFIES_PER_TICK: u32 = 64;
/// Receipts waiting for a tick with chain facts to be admitted against — two gossip inboxes' worth
/// (the inbox holds 256 events), which only accumulates across ticks the node skips.
pub const RECEIPT_ARRIVALS_MAX: usize = 512;
/// Signature verdicts kept by digest.
const VERDICT_CACHE_CAP: usize = 4096;

/// The first re-send of a seat's own receipt, after the broadcast that filed it.
pub const OWN_RECEIPT_REBROADCAST_FIRST: Duration = Duration::from_secs(30);
/// The longest gap between two re-sends.
pub const OWN_RECEIPT_REBROADCAST_MAX_GAP: Duration = Duration::from_secs(600);
/// Re-sends one tick may make — each is one ML-DSA-87 signature and one broadcast.
pub const OWN_RECEIPT_REBROADCASTS_PER_TICK: usize = 8;

/// What a pool reads of a receipt, whichever version it is.
pub trait PoolReceiptV1: Clone + PartialEq + borsh::BorshSerialize {
    fn claim(&self) -> Hash64;
    fn seat_bond(&self) -> PalwBondKeyV2;
    fn signed_daa(&self) -> u64;
    fn signature(&self) -> &[u8];
    /// The message the seat signed and the context it signed it under — the pair the acceptance
    /// validator verifies against the seat bond's registered key.
    fn signed_message(&self, network_domain: Hash64) -> (Hash64, &'static [u8]);
}

impl PoolReceiptV1 for PalwSeatReceiptV2 {
    fn claim(&self) -> Hash64 {
        self.claim
    }
    fn seat_bond(&self) -> PalwBondKeyV2 {
        self.seat_bond
    }
    fn signed_daa(&self) -> u64 {
        self.signed_daa
    }
    fn signature(&self) -> &[u8] {
        &self.signature
    }
    fn signed_message(&self, network_domain: Hash64) -> (Hash64, &'static [u8]) {
        (palw_receipt_message_v2(network_domain, self.claim, self.verdict, self.signed_daa), PALW_RECEIPT_V2_MLDSA87_CONTEXT)
    }
}

impl PoolReceiptV1 for PalwSeatReceiptV3 {
    fn claim(&self) -> Hash64 {
        self.receipt.claim
    }
    fn seat_bond(&self) -> PalwBondKeyV2 {
        self.receipt.seat_bond
    }
    fn signed_daa(&self) -> u64 {
        self.receipt.signed_daa
    }
    fn signature(&self) -> &[u8] {
        &self.receipt.signature
    }
    fn signed_message(&self, network_domain: Hash64) -> (Hash64, &'static [u8]) {
        let r = &self.receipt;
        (palw_receipt_message_v3(network_domain, r.claim, r.verdict, r.signed_daa, self.segments), PALW_RECEIPT_V3_MLDSA87_CONTEXT)
    }
}

/// One bound panel, as this tick's read reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelFactV1 {
    pub bound_daa: u64,
    pub anchor: Hash64,
    pub seats: Vec<PalwBondKeyV2>,
}

/// **What the chain says about the receipts a pool is offered**: the bound panels read this tick,
/// and the seat keys read so far. Filled only from `palw_receipt_pool_facts_v1` — chain facts, never
/// anything a peer sent.
#[derive(Default)]
pub struct ReceiptChainFactsV1 {
    panels: HashMap<Hash64, PanelFactV1>,
    keys: HashMap<PalwBondKeyV2, Arc<Vec<u8>>>,
}

impl ReceiptChainFactsV1 {
    /// Take one read: its panels replace the last tick's (a panel is a fact about the tip), its keys
    /// join the ones kept (a key is a fact about the bond: the outpoint names the transaction that
    /// registered it, so it never changes). Keys no receipt in `referenced` names any more are
    /// forgotten, so the cache is bounded by the pools and never by the registry.
    pub fn refresh(&mut self, read: PalwReceiptPoolFactsV1, referenced: &HashSet<PalwBondKeyV2>) {
        self.panels = read
            .panels
            .into_iter()
            .map(|p| (p.claim_id, PanelFactV1 { bound_daa: p.bound_daa, anchor: p.anchor, seats: p.seats }))
            .collect();
        self.keys.retain(|bond, _| referenced.contains(bond));
        for (bond, key) in read.seat_keys {
            self.keys.insert(bond, Arc::new(key));
        }
    }

    /// Whether the next read must ask for `bond`'s key.
    pub fn needs_key(&self, bond: &PalwBondKeyV2) -> bool {
        !self.keys.contains_key(bond)
    }

    pub fn panel(&self, claim: &Hash64) -> Option<&PanelFactV1> {
        self.panels.get(claim)
    }

    pub fn seat_key(&self, bond: &PalwBondKeyV2) -> Option<Arc<Vec<u8>>> {
        self.keys.get(bond).cloned()
    }
}

/// **What the sweep keeps of a pool** ([`PalwReceiptPoolV1::sweep`]), heard and own alike: a claim
/// a duty or a dispute names (`live`, the chain's word), and otherwise one this node has not
/// submitted for, first heard — or filed on — no more than `retention_daa` ago.
///
/// It is the rule the one per-claim list held this node's receipts under before the pools were
/// rebuilt, and it reads no receipt deadline (see [`PalwReceiptPoolV1::retain_own`] for why a
/// deadline was wrong): while the chain keeps the claim `PanelBound` it lists this seat's duty on
/// it, whatever window the claim's class has, and a tip that drops the duty for a tick (a sibling,
/// a short reorg) costs nothing inside the retention age. Only chain facts and this node's own acts
/// reach it.
pub struct ReceiptSweepV1<'a> {
    pub live: &'a HashSet<Hash64>,
    pub submitted: &'a HashMap<Hash64, u64>,
    pub current_daa: u64,
    pub retention_daa: u64,
}

impl ReceiptSweepV1<'_> {
    /// Whether `claim`, heard or filed on at `since_daa`, is kept.
    pub fn keeps(&self, claim: &Hash64, since_daa: u64) -> bool {
        self.live.contains(claim)
            || (!self.submitted.contains_key(claim) && self.current_daa <= since_daa.saturating_add(self.retention_daa))
    }
}

/// **How much signature checking one tick may spend**, per claim and in total.
pub struct VerifyBudgetV1 {
    left: u32,
    per_claim: u32,
    spent: HashMap<Hash64, u32>,
    /// Checks actually made — what a log line or a test reads.
    pub used: u32,
}

impl VerifyBudgetV1 {
    pub fn new(total: u32, per_claim: u32) -> Self {
        Self { left: total, per_claim, spent: HashMap::new(), used: 0 }
    }

    /// One tick's budget.
    pub fn per_tick() -> Self {
        Self::new(RECEIPT_VERIFIES_PER_TICK, RECEIPT_VERIFIES_PER_CLAIM_PER_TICK)
    }

    /// Checks spent on `claim` this tick.
    pub fn spent_on(&self, claim: &Hash64) -> u32 {
        self.spent.get(claim).copied().unwrap_or(0)
    }

    /// Whether the tick's whole budget is gone.
    pub fn exhausted(&self) -> bool {
        self.left == 0
    }

    fn take(&mut self, claim: &Hash64) -> bool {
        if self.left == 0 || self.spent_on(claim) >= self.per_claim {
            return false;
        }
        *self.spent.entry(*claim).or_insert(0) += 1;
        self.left -= 1;
        self.used += 1;
        true
    }
}

/// Signature verdicts already paid for, by receipt digest; the oldest is forgotten first.
struct VerdictCacheV1 {
    verdicts: HashMap<u64, bool>,
    order: VecDeque<u64>,
}

impl VerdictCacheV1 {
    fn new() -> Self {
        Self { verdicts: HashMap::new(), order: VecDeque::new() }
    }

    fn get(&self, digest: u64) -> Option<bool> {
        self.verdicts.get(&digest).copied()
    }

    fn put(&mut self, digest: u64, valid: bool) {
        if self.verdicts.insert(digest, valid).is_none() {
            self.order.push_back(digest);
            if self.order.len() > VERDICT_CACHE_CAP
                && let Some(old) = self.order.pop_front()
            {
                self.verdicts.remove(&old);
            }
        }
    }

    /// Check one signature: the cached verdict when there is one, else one check out of `budget`,
    /// cached. `None` when the budget is spent — the receipt is neither good nor bad yet.
    #[allow(clippy::too_many_arguments)]
    fn check<V>(
        &mut self,
        budget: &mut VerifyBudgetV1,
        claim: &Hash64,
        digest: u64,
        key: &[u8],
        message: &Hash64,
        context: &[u8],
        signature: &[u8],
        verify: &V,
    ) -> Option<bool>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        if let Some(valid) = self.get(digest) {
            return Some(valid);
        }
        if !budget.take(claim) {
            return None;
        }
        let valid = verify(key, message.as_byte_slice(), signature, context);
        self.put(digest, valid);
        Some(valid)
    }
}

/// What the pool did with one receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReceiptAdmitV1 {
    /// Took a free slot — checked if this tick's budget allowed, unchecked if not.
    Pooled,
    /// Verified, and took the place of a receipt that was unchecked, failed, or older.
    Displaced,
    /// Already held, pooled or as this node's own.
    Duplicate,
    /// A checked receipt of the same bond already says exactly this.
    Redundant,
    /// Its signature is not an ML-DSA-87 signature's length — no key verifies it.
    Malformed,
    /// Checked (now or before), and it does not verify.
    BadSignature,
    /// The tip holds the claim `PanelBound` and its panel does not seat this bond.
    NotOnPanel,
    /// Signed before the panel the tip holds was bound — it counts for no seat of it.
    Stale,
    /// The registry holds no such bond, so no receipt of it can verify.
    UnknownBond,
    /// Its place is taken by receipts that verify, or are not the newcomer's to take.
    NoRoom,
    /// Its place is contested and this tick's checks are spent: dropped for now. The seat's re-send
    /// brings it back.
    Deferred,
}

impl ReceiptAdmitV1 {
    /// Whether the receipt is in the pool now.
    pub fn kept(self) -> bool {
        matches!(self, Self::Pooled | Self::Displaced)
    }
}

struct PooledReceiptV1<R> {
    receipt: R,
    digest: u64,
    /// Checked by this node and verified. An unchecked receipt is `false` — nothing is known.
    checked: bool,
    seq: u64,
}

struct ClaimSliceV1<R> {
    /// When the claim first entered the pool, in the pool's own order (for the claim ceiling).
    arrival: u64,
    /// The virtual DAA it first entered at (for the caller's age bound).
    first_seen_daa: u64,
    bonds: BTreeMap<PalwBondKeyV2, Vec<PooledReceiptV1<R>>>,
}

impl<R: PartialEq> ClaimSliceV1<R> {
    fn holds(&self, bond: &PalwBondKeyV2, digest: u64, receipt: &R) -> bool {
        self.bonds.get(bond).is_some_and(|entries| entries.iter().any(|e| e.digest == digest && e.receipt == *receipt))
    }

    fn len(&self) -> usize {
        self.bonds.values().map(Vec::len).sum()
    }

    /// Its unchecked receipts, oldest first, as `(bond, seq)`.
    fn unchecked(&self) -> Vec<(PalwBondKeyV2, u64)> {
        let mut out: Vec<(PalwBondKeyV2, u64)> =
            self.bonds.iter().flat_map(|(bond, entries)| entries.iter().filter(|e| !e.checked).map(move |e| (*bond, e.seq))).collect();
        out.sort_by_key(|(_, seq)| *seq);
        out
    }
}

struct OwnReceiptV1<R> {
    receipt: R,
    /// The virtual DAA it was filed at — what the sweep ages it from once no duty names its claim.
    filed_daa: u64,
}

/// **One version's receipt pool** — see the module header for the four rules it keeps.
pub struct PalwReceiptPoolV1<R> {
    network_domain: Hash64,
    claims: HashMap<Hash64, ClaimSliceV1<R>>,
    own: HashMap<Hash64, Vec<OwnReceiptV1<R>>>,
    verdicts: VerdictCacheV1,
    seq: u64,
    hasher: RandomState,
}

impl<R: PoolReceiptV1> PalwReceiptPoolV1<R> {
    pub fn new(network_domain: Hash64) -> Self {
        Self {
            network_domain,
            claims: HashMap::new(),
            own: HashMap::new(),
            verdicts: VerdictCacheV1::new(),
            seq: 0,
            hasher: RandomState::new(),
        }
    }

    /// A process-keyed digest of the receipt as it travelled: nobody outside this process can aim a
    /// collision at a cached verdict.
    fn digest(&self, receipt: &R) -> u64 {
        self.hasher.hash_one(borsh::to_vec(receipt).expect("a receipt serializes"))
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// **Keep one of this node's own receipts**, filed at `filed_daa`, outside everything an arrival
    /// can evict. The collector reads it through [`Self::candidates`] like any other.
    pub fn insert_own(&mut self, receipt: R, filed_daa: u64) {
        let own = self.own.entry(receipt.claim()).or_default();
        if own.iter().any(|o| o.receipt == receipt) {
            return;
        }
        if own.len() >= OWN_RECEIPTS_PER_CLAIM {
            own.remove(0);
        }
        own.push(OwnReceiptV1 { receipt, filed_daa });
    }

    /// Keep the own receipts `keep` answers yes for, given the claim and the DAA each was filed at.
    /// `keep` is the sweep's rule ([`ReceiptSweepV1::keeps`]): chain facts and this node's own acts;
    /// nothing a peer sends reaches it.
    ///
    /// It was a deadline — the duty's `receipt_deadline`, `bound + window_receipt` — which is the
    /// network's global window and not the chain's: ADR-0133 §11.3 keeps a heavy class's claim
    /// `PanelBound`, and the acceptance validators taking receipts for it, until `bound +
    /// receipt_window_for_claim_v1`, on testnet-12 from genesis. No node holds a second copy of its
    /// own receipt (the gossip center marks its own broadcasts seen), so past the global deadline
    /// no seat node could offer its own and no coverage licence could form for the rest of the
    /// window the chain was still giving it.
    pub fn retain_own(&mut self, keep: impl Fn(&Hash64, u64) -> bool) {
        self.own.retain(|claim, own| {
            own.retain(|o| keep(claim, o.filed_daa));
            !own.is_empty()
        });
    }

    pub fn has_own(&self, claim: &Hash64) -> bool {
        self.own.contains_key(claim)
    }

    pub fn own_claim_ids(&self) -> impl Iterator<Item = &Hash64> {
        self.own.keys()
    }

    /// The claims with receipts heard from the network.
    pub fn pooled_claim_ids(&self) -> impl Iterator<Item = &Hash64> {
        self.claims.keys()
    }

    /// The bonds with receipts heard from the network, over every claim.
    pub fn pooled_bonds(&self) -> impl Iterator<Item = &PalwBondKeyV2> {
        self.claims.values().flat_map(|slice| slice.bonds.keys())
    }

    /// Every claim the pool holds anything for — heard or own — in id order.
    pub fn claim_ids(&self) -> Vec<Hash64> {
        let mut ids: Vec<Hash64> = self.claims.keys().chain(self.own.keys()).copied().collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn contains_claim(&self, claim: &Hash64) -> bool {
        self.claims.contains_key(claim) || self.own.contains_key(claim)
    }

    pub fn claim_count(&self) -> usize {
        self.claim_ids().len()
    }

    /// Receipts heard from the network, over every claim (own ones not counted).
    pub fn pooled_len(&self) -> usize {
        self.claims.values().map(ClaimSliceV1::len).sum()
    }

    /// The receipts heard from the network for `claim`, each with whether this node has checked it —
    /// what a test or a log line reads.
    pub fn pooled(&self, claim: &Hash64) -> Vec<(R, bool)> {
        let Some(slice) = self.claims.get(claim) else { return Vec::new() };
        let mut entries: Vec<&PooledReceiptV1<R>> = slice.bonds.values().flatten().collect();
        entries.sort_by_key(|e| e.seq);
        entries.into_iter().map(|e| (e.receipt.clone(), e.checked)).collect()
    }

    /// **What the collector offers the assembler for `claim`**: this node's own receipts first, then
    /// every pooled one of a bond the tip's panel seats (every pooled one when `facts` places no
    /// panel) — checked before unchecked, each in arrival order. The acceptance validator judges all
    /// of them; the order only decides which of two receipts of one seat is tried first.
    ///
    /// A bond off the tip's panel is held aside for a sibling the tip may return to
    /// ([`Self::prune_to_panels`]), and not offered: the validator refuses it (`NotOnPanel`), and the
    /// licence selection puts the whole set to the validator once per candidate, so every such
    /// receipt offered would cost a round of signature checks for nothing. `facts` is this tick's
    /// read, taken before the assembler reads the tip; a block in between costs one tick.
    pub fn candidates(&self, claim: &Hash64, facts: &ReceiptChainFactsV1) -> Vec<R> {
        let seated = |bond: &PalwBondKeyV2| facts.panel(claim).is_none_or(|panel| panel.seats.contains(bond));
        let mut out: Vec<R> = self.own.get(claim).into_iter().flatten().map(|o| o.receipt.clone()).collect();
        if let Some(slice) = self.claims.get(claim) {
            let mut pooled: Vec<&PooledReceiptV1<R>> =
                slice.bonds.iter().filter(|(bond, _)| seated(bond)).flat_map(|(_, entries)| entries.iter()).collect();
            pooled.sort_by_key(|e| (!e.checked, e.seq));
            for entry in pooled {
                if !out.contains(&entry.receipt) {
                    out.push(entry.receipt.clone());
                }
            }
        }
        out
    }

    /// Keep the pooled claims `keep` answers yes for, given the DAA each first entered at. Own
    /// receipts are not touched ([`Self::retain_own`] bounds them).
    pub fn retain_pooled(&mut self, keep: impl Fn(&Hash64, u64) -> bool) {
        self.claims.retain(|claim, slice| keep(claim, slice.first_seen_daa));
    }

    /// **The sweep, over both halves of the pool, by one rule** — what was heard aged from when the
    /// pool first heard its claim, what this node filed aged from when it filed.
    pub fn sweep(&mut self, rule: &ReceiptSweepV1<'_>) {
        self.retain_pooled(|claim, since| rule.keeps(claim, since));
        self.retain_own(|claim, filed| rule.keeps(claim, filed));
    }

    /// **Hold every claim to what the tip says about it now.** For a claim the tip holds
    /// `PanelBound`: a receipt signed before the bind is dropped (it counts for no seat of this
    /// panel, nor of a sibling of it — siblings bind at one DAA); a bond the panel seats keeps the
    /// rest; a bond it does not seat keeps only the receipts this node has checked and verified, and
    /// of those bonds at most [`OFF_PANEL_BONDS_PER_CLAIM`], the most recently heard. A claim the
    /// tip says nothing about is left as it is.
    ///
    /// The off-panel bonds were dropped outright, verified receipts and all. So a tip that flipped
    /// to a sibling panel for one tick emptied the pool of the other panel's receipts, and when it
    /// flipped back only the seats' re-sends — backed off up to ten minutes, and not reset by THIS
    /// node's flip — brought them back: a delay on testnet-12's 600-DAA window, perhaps the whole
    /// window on a short one. Held aside, they cost nothing: nothing is admitted for such a bond
    /// while the tip holds the panel, the collector is not offered them ([`Self::candidates`]), and
    /// only what verified is kept, so no junk rides on it.
    pub fn prune_to_panels(&mut self, facts: &ReceiptChainFactsV1) {
        self.claims.retain(|claim, slice| {
            let Some(panel) = facts.panel(claim) else { return true };
            for (bond, entries) in slice.bonds.iter_mut() {
                let seated = panel.seats.contains(bond);
                entries.retain(|e| e.receipt.signed_daa() >= panel.bound_daa && (seated || e.checked));
            }
            slice.bonds.retain(|_, entries| !entries.is_empty());
            let mut aside: Vec<(u64, PalwBondKeyV2)> = slice
                .bonds
                .iter()
                .filter(|(bond, _)| !panel.seats.contains(bond))
                .map(|(bond, entries)| (entries.iter().map(|e| e.seq).max().unwrap_or(0), *bond))
                .collect();
            if aside.len() > OFF_PANEL_BONDS_PER_CLAIM {
                aside.sort_unstable();
                for (_, bond) in &aside[..aside.len() - OFF_PANEL_BONDS_PER_CLAIM] {
                    slice.bonds.remove(bond);
                }
            }
            !slice.bonds.is_empty()
        });
    }

    /// **Offer one receipt heard from the network.** `kept` names the claims the caller keeps whatever
    /// the ceiling says — chain facts and this node's own filings, never anything a peer sent.
    pub fn admit<V>(
        &mut self,
        receipt: R,
        facts: &ReceiptChainFactsV1,
        verify: &V,
        budget: &mut VerifyBudgetV1,
        current_daa: u64,
        kept: &HashSet<Hash64>,
    ) -> ReceiptAdmitV1
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        use ReceiptAdmitV1 as A;
        // An ML-DSA-87 signature is exactly this long, and the acceptance validator's verify
        // refuses any other length before it looks at a byte — the audit's one-to-eight-byte junk
        // stops here, costing nothing.
        if receipt.signature().len() != kaspa_txscript::MLDSA87_SIG_LEN {
            return A::Malformed;
        }
        let claim = receipt.claim();
        let bond = receipt.seat_bond();
        if self.own.get(&claim).is_some_and(|own| own.iter().any(|o| o.receipt == receipt)) {
            return A::Duplicate;
        }
        let digest = self.digest(&receipt);
        if self.claims.get(&claim).is_some_and(|slice| slice.holds(&bond, digest, &receipt)) {
            return A::Duplicate;
        }
        if self.verdicts.get(digest) == Some(false) {
            return A::BadSignature;
        }
        let panel_known = match facts.panel(&claim) {
            Some(panel) => {
                if !panel.seats.contains(&bond) {
                    return A::NotOnPanel;
                }
                if receipt.signed_daa() < panel.bound_daa {
                    return A::Stale;
                }
                true
            }
            None => false,
        };
        let Some(key) = facts.seat_key(&bond) else { return A::UnknownBond };

        // A copy of what a checked receipt of its bond already says (a seat's re-send, re-signed) adds
        // nothing, and is refused before it costs a check.
        let network_domain = self.network_domain;
        let (message, context) = receipt.signed_message(network_domain);
        if self
            .claims
            .get(&claim)
            .and_then(|slice| slice.bonds.get(&bond))
            .is_some_and(|entries| entries.iter().any(|e| e.checked && e.receipt.signed_message(network_domain).0 == message))
        {
            return A::Redundant;
        }
        // Checked at the door while the budget lasts: junk is refused before it holds anything.
        let verified = match self.verdicts.check(budget, &claim, digest, &key, &message, context, receipt.signature(), verify) {
            Some(false) => return A::BadSignature,
            Some(true) => true,
            None => false,
        };

        let (bond_len, bond_count) = match self.claims.get(&claim) {
            Some(slice) => (slice.bonds.get(&bond).map_or(0, Vec::len), slice.bonds.len()),
            None => (0, 0),
        };
        let bond_room = bond_len < RECEIPTS_PER_BOND;
        let claim_room = bond_len > 0 || panel_known || bond_count < UNPLACED_BONDS_PER_CLAIM;
        if bond_room && claim_room {
            self.insert(claim, bond, receipt, digest, verified, current_daa, kept);
            return A::Pooled;
        }
        // Contested, and only a receipt that proved itself may contest.
        if !verified {
            return A::Deferred;
        }

        if !bond_room {
            // Its own bond's slots. An unchecked receipt claiming this bond goes first — only this
            // bond's key-holder produced the newcomer, so what it replaces is either junk or that
            // same seat's own other copy. With every slot checked, the oldest-signed goes, and only
            // for a newcomer signed later (a re-bound panel's receipt).
            let seq = self.next_seq();
            let entries = self.claims.get_mut(&claim).and_then(|slice| slice.bonds.get_mut(&bond)).expect("the bond's slots are full");
            let signed_daa = receipt.signed_daa();
            let victim = entries
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.checked)
                .min_by_key(|(_, e)| e.seq)
                .or_else(|| {
                    entries
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.receipt.signed_daa() < signed_daa)
                        .min_by_key(|(_, e)| (e.receipt.signed_daa(), e.seq))
                })
                .map(|(i, _)| i);
            let Some(victim) = victim else { return A::NoRoom };
            entries.remove(victim);
            entries.push(PooledReceiptV1 { receipt, digest, checked: true, seq });
            return A::Displaced;
        }

        // A new bond on a claim the tip has not placed, at the bond cap. Nothing of another bond is
        // taken on suspicion: the oldest unchecked receipts are checked, and one that fails is
        // removed; one that verifies stays, marked. A victim whose key this node does not hold
        // cannot be judged, and is passed over rather than evicted on suspicion.
        //
        // **The newcomer takes a bond PLACE, so it enters only once a place is free** (the launch
        // review of this fix, its first finding). It entered as soon as ONE failing receipt was
        // removed, and a victim bond that kept its other receipt kept its place too: the claim
        // grew by a bond, junk refilled the newcomer's second slot once the tick's checks were
        // spent, and the next registered key repeated it — forty bonds and forty-eight receipts on
        // one never-bound claim against a stated eight and sixteen. The removals stand either way
        // (what failed is junk), and the contest goes on to the next victim.
        let victims = self.claims.get(&claim).map(ClaimSliceV1::unchecked).unwrap_or_default();
        for (victim_bond, victim_seq) in victims {
            let Some(victim_key) = facts.seat_key(&victim_bond) else { continue };
            let slice = self.claims.get_mut(&claim).expect("the victims were read from it");
            let entries = slice.bonds.get_mut(&victim_bond).expect("the victims were read from it");
            let index = entries.iter().position(|e| e.seq == victim_seq).expect("the victims were read from it");
            let victim = &entries[index];
            let (victim_message, victim_context) = victim.receipt.signed_message(network_domain);
            let verdict = self.verdicts.check(
                budget,
                &claim,
                victim.digest,
                &victim_key,
                &victim_message,
                victim_context,
                victim.receipt.signature(),
                verify,
            );
            match verdict {
                Some(true) => entries[index].checked = true,
                Some(false) => {
                    entries.remove(index);
                    if entries.is_empty() {
                        slice.bonds.remove(&victim_bond);
                    }
                    if slice.bonds.len() < UNPLACED_BONDS_PER_CLAIM {
                        self.insert(claim, bond, receipt, digest, true, current_daa, kept);
                        return A::Displaced;
                    }
                }
                None => return A::Deferred,
            }
        }
        A::NoRoom
    }

    /// **Spend what budget a tick left over on the receipts already pooled unchecked**: each is
    /// checked, oldest first, kept claims before the rest; one that fails is removed, one that
    /// verifies is marked. Returns how many were removed.
    ///
    /// Junk takes a free slot only when a flood has spent the tick's checks; this is what clears it
    /// once the flood pauses, so the seat's re-send finds its bond's slot free — and what keeps a
    /// licence attempt from re-verifying the same junk every tick.
    pub fn scrub<V>(&mut self, facts: &ReceiptChainFactsV1, verify: &V, budget: &mut VerifyBudgetV1, kept: &HashSet<Hash64>) -> usize
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let network_domain = self.network_domain;
        let mut order: Vec<Hash64> = self.claims.keys().copied().collect();
        order.sort_by_key(|claim| (!kept.contains(claim), *claim));
        let mut removed = 0;
        for claim in order {
            if budget.exhausted() {
                break;
            }
            let unchecked = self.claims.get(&claim).map(ClaimSliceV1::unchecked).unwrap_or_default();
            for (bond, seq) in unchecked {
                let Some(key) = facts.seat_key(&bond) else { continue };
                let slice = self.claims.get_mut(&claim).expect("read from it above");
                let entries = slice.bonds.get_mut(&bond).expect("read from it above");
                let Some(index) = entries.iter().position(|e| e.seq == seq) else { continue };
                let entry = &entries[index];
                let (message, context) = entry.receipt.signed_message(network_domain);
                match self.verdicts.check(budget, &claim, entry.digest, &key, &message, context, entry.receipt.signature(), verify) {
                    Some(true) => entries[index].checked = true,
                    Some(false) => {
                        entries.remove(index);
                        if entries.is_empty() {
                            slice.bonds.remove(&bond);
                        }
                        removed += 1;
                    }
                    // This claim's share is spent; the next claim may still have its own.
                    None => break,
                }
            }
            if self.claims.get(&claim).is_some_and(|slice| slice.bonds.is_empty()) {
                self.claims.remove(&claim);
            }
        }
        removed
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        claim: Hash64,
        bond: PalwBondKeyV2,
        receipt: R,
        digest: u64,
        checked: bool,
        current_daa: u64,
        kept: &HashSet<Hash64>,
    ) {
        let seq = self.next_seq();
        let new_claim = !self.claims.contains_key(&claim);
        let slice = self.claims.entry(claim).or_insert_with(|| ClaimSliceV1 {
            arrival: seq,
            first_seen_daa: current_daa,
            bonds: BTreeMap::new(),
        });
        slice.bonds.entry(bond).or_default().push(PooledReceiptV1 { receipt, digest, checked, seq });
        if new_claim {
            self.hold_claim_ceiling(claim, kept);
        }
    }

    /// Hold the pool to [`RECEIPT_POOL_MAX_CLAIMS`] claims beyond the kept ones, evicting whole claims
    /// oldest-arrival first — never the one that just arrived, and never a kept one.
    fn hold_claim_ceiling(&mut self, arrived: Hash64, kept: &HashSet<Hash64>) {
        let mut evictable = self.claims.keys().filter(|id| !kept.contains(id)).count();
        while evictable > RECEIPT_POOL_MAX_CLAIMS {
            let Some(oldest) = self
                .claims
                .iter()
                .filter(|(id, _)| **id != arrived && !kept.contains(id))
                .min_by_key(|(_, slice)| slice.arrival)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.claims.remove(&oldest);
            evictable -= 1;
        }
    }
}

/// One receipt off the wire, in whichever version it decoded as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArrivedReceiptV1 {
    V2(PalwSeatReceiptV2),
    V3(PalwSeatReceiptV3),
}

impl ArrivedReceiptV1 {
    /// V3 first: a V3 receipt is a V2 one with a mask after it, and borsh refuses trailing bytes,
    /// so neither decodes as the other.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if let Ok(receipt) = borsh::from_slice::<PalwSeatReceiptV3>(bytes) {
            return Some(Self::V3(receipt));
        }
        borsh::from_slice::<PalwSeatReceiptV2>(bytes).ok().map(Self::V2)
    }

    pub fn claim(&self) -> Hash64 {
        match self {
            Self::V2(r) => r.claim(),
            Self::V3(r) => r.claim(),
        }
    }

    pub fn seat_bond(&self) -> PalwBondKeyV2 {
        match self {
            Self::V2(r) => r.seat_bond(),
            Self::V3(r) => r.seat_bond(),
        }
    }

    /// Whether its signature is an ML-DSA-87 signature's length — the one thing the inbox can refuse
    /// before a tick has chain facts to judge by.
    pub fn is_well_formed(&self) -> bool {
        let signature = match self {
            Self::V2(r) => r.signature(),
            Self::V3(r) => r.signature(),
        };
        signature.len() == kaspa_txscript::MLDSA87_SIG_LEN
    }
}

/// **Queue one receipt off the wire for the next admission** — the gossip drain's whole part. What
/// the drain can refuse without the chain it refuses: bytes that are no receipt, and a signature
/// that is not an ML-DSA-87 signature's length (the audit's one-to-eight-byte junk); no key
/// verifies either, and neither may take a place in the queue. The queue is held to
/// [`RECEIPT_ARRIVALS_MAX`], oldest out. Returns whether the receipt was queued.
pub fn receipt_arrival_push_v1(queue: &mut VecDeque<ArrivedReceiptV1>, bytes: &[u8]) -> bool {
    let Some(arrived) = ArrivedReceiptV1::decode(bytes).filter(ArrivedReceiptV1::is_well_formed) else { return false };
    if queue.len() >= RECEIPT_ARRIVALS_MAX {
        queue.pop_front();
    }
    queue.push_back(arrived);
    true
}

/// **What one tick asks the tip, for both pools and this tick's arrivals**: every claim either pool
/// holds or an arrival names (their panels are what [`PalwReceiptPoolV1::prune_to_panels`] holds the
/// pools to), the bonds whose keys are not yet cached, and every bond still referenced (the key
/// cache keeps exactly those).
pub struct ReceiptPoolReadV1 {
    pub claims: Vec<Hash64>,
    pub bonds: Vec<PalwBondKeyV2>,
    pub referenced: HashSet<PalwBondKeyV2>,
}

pub fn receipt_pool_read_v1(
    v2: &PalwReceiptPoolV1<PalwSeatReceiptV2>,
    v3: &PalwReceiptPoolV1<PalwSeatReceiptV3>,
    arrivals: &[ArrivedReceiptV1],
    facts: &ReceiptChainFactsV1,
) -> ReceiptPoolReadV1 {
    let mut claims: Vec<Hash64> =
        v2.claim_ids().into_iter().chain(v3.claim_ids()).chain(arrivals.iter().map(ArrivedReceiptV1::claim)).collect();
    claims.sort_unstable();
    claims.dedup();
    let referenced: HashSet<PalwBondKeyV2> =
        v2.pooled_bonds().chain(v3.pooled_bonds()).copied().chain(arrivals.iter().map(ArrivedReceiptV1::seat_bond)).collect();
    let mut bonds: Vec<PalwBondKeyV2> = referenced.iter().filter(|bond| facts.needs_key(bond)).copied().collect();
    bonds.sort_unstable();
    ReceiptPoolReadV1 { claims, bonds, referenced }
}

/// **Admit one tick's arrivals** against one read of the chain: the claims the caller keeps first
/// (so a flood of foreign claims cannot spend the checks this node's own licences need), each in
/// the order given — the caller shuffles it, so a flooder cannot time junk ahead of the genuine
/// receipt inside a tick — under one [`VerifyBudgetV1`]; then both pools pruned to the panels the
/// tip holds, and what budget is left spent on what is pooled unchecked. Returns what happened to
/// each arrival, in the order admitted.
#[allow(clippy::too_many_arguments)]
pub fn admit_receipt_arrivals_v1<V>(
    arrivals: Vec<ArrivedReceiptV1>,
    v2: &mut PalwReceiptPoolV1<PalwSeatReceiptV2>,
    v3: &mut PalwReceiptPoolV1<PalwSeatReceiptV3>,
    facts: &ReceiptChainFactsV1,
    verify: &V,
    budget: &mut VerifyBudgetV1,
    current_daa: u64,
    kept: &HashSet<Hash64>,
) -> Vec<ReceiptAdmitV1>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let mut arrivals = arrivals;
    // A stable sort: the given order survives within each half.
    arrivals.sort_by_key(|arrived| !kept.contains(&arrived.claim()));
    let outcomes = arrivals
        .into_iter()
        .map(|arrived| match arrived {
            ArrivedReceiptV1::V2(r) => v2.admit(r, facts, verify, budget, current_daa, kept),
            ArrivedReceiptV1::V3(r) => v3.admit(r, facts, verify, budget, current_daa, kept),
        })
        .collect();
    v2.prune_to_panels(facts);
    v3.prune_to_panels(facts);
    v3.scrub(facts, verify, budget, kept);
    v2.scrub(facts, verify, budget, kept);
    outcomes
}

/// **One tick of the receipt pools, as the service runs it**: the queued arrivals drained and
/// `shuffle`d, one read of the tip (`read_tip`, the session's `palw_receipt_pool_facts_v1`) for
/// what they and both pools name, `facts` refreshed from it, and the arrivals admitted under one
/// tick's [`VerifyBudgetV1`] ([`admit_receipt_arrivals_v1`]). `kept` is the caller's chain facts
/// and own filings (`receipt_pool_kept_v1` in the panel service).
///
/// `None` when the tip has no state to judge by: the arrivals go back in the queue for the next
/// tick (held to [`RECEIPT_ARRIVALS_MAX`]), and nothing is admitted or pruned on a read that did
/// not happen — "no read" is not "no such panel, no such bond".
#[allow(clippy::too_many_arguments)]
pub fn receipt_pool_tick_v1<Q, S, V>(
    queue: &mut VecDeque<ArrivedReceiptV1>,
    v2: &mut PalwReceiptPoolV1<PalwSeatReceiptV2>,
    v3: &mut PalwReceiptPoolV1<PalwSeatReceiptV3>,
    facts: &mut ReceiptChainFactsV1,
    read_tip: Q,
    shuffle: S,
    verify: &V,
    current_daa: u64,
    kept: &HashSet<Hash64>,
) -> Option<(Vec<ReceiptAdmitV1>, VerifyBudgetV1)>
where
    Q: FnOnce(Vec<Hash64>, Vec<PalwBondKeyV2>) -> Option<PalwReceiptPoolFactsV1>,
    S: FnOnce(&mut [ArrivedReceiptV1]),
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let mut arrivals: Vec<ArrivedReceiptV1> = queue.drain(..).collect();
    shuffle(arrivals.as_mut_slice());
    let read = receipt_pool_read_v1(v2, v3, &arrivals, facts);
    let Some(tip) = read_tip(read.claims, read.bonds) else {
        queue.extend(arrivals.into_iter().take(RECEIPT_ARRIVALS_MAX));
        return None;
    };
    facts.refresh(tip, &read.referenced);
    let mut budget = VerifyBudgetV1::per_tick();
    let outcomes = admit_receipt_arrivals_v1(arrivals, v2, v3, facts, verify, &mut budget, current_daa, kept);
    Some((outcomes, budget))
}

/// **The gap before an own receipt's next re-send**, after `sends` re-sends: 30 s, doubling, capped
/// at ten minutes. Wall-clock, because what it answers is how long a node that lost the receipt
/// waits to get it back — a loop count is not a clock, and a DAA is two minutes on testnet-12.
pub fn own_receipt_rebroadcast_gap_v1(sends: u32) -> Duration {
    OWN_RECEIPT_REBROADCAST_FIRST.saturating_mul(1u32 << sends.min(16)).min(OWN_RECEIPT_REBROADCAST_MAX_GAP)
}

/// When one own receipt next goes out again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnRebroadcastV1 {
    next_at: Instant,
    sends: u32,
}

impl OwnRebroadcastV1 {
    /// The schedule of a receipt broadcast (filed) at `now`.
    pub fn filed(now: Instant) -> Self {
        Self { next_at: now + own_receipt_rebroadcast_gap_v1(0), sends: 0 }
    }

    pub fn due(&self, now: Instant) -> bool {
        now >= self.next_at
    }

    /// It went out again at `now`.
    pub fn sent(&mut self, now: Instant) {
        self.sends = self.sends.saturating_add(1);
        self.next_at = now + own_receipt_rebroadcast_gap_v1(self.sends);
    }

    pub fn sends(&self) -> u32 {
        self.sends
    }
}

/// **One of this seat's own receipts, as the re-send keeps it**: what was signed, so it can be signed
/// again, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnFiledV1 {
    pub claim: Hash64,
    pub verdict: PalwReceiptVerdictV2,
    pub signed_daa: u64,
    /// The mask a V3 receipt attests; `None` for a V2 receipt.
    pub segments: Option<PalwSegmentMaskV2>,
    pub schedule: OwnRebroadcastV1,
}

/// **Which own receipts go out again now**: those whose panel is still a duty of this seat — its
/// claim `PanelBound` on that very panel, `duties` naming the panel by the seat's whole duty key —
/// and whose turn has come; the longest-waiting first, at most `limit`. A claim that has left
/// `PanelBound` is no duty, so its receipt is never re-sent again.
///
/// The standing duty is the whole window test. The chain keeps the claim `PanelBound` — and lists
/// the duty — until its own receipt window for the claim closes (`bound +
/// receipt_window_for_claim_v1`, where the sweep redraws or voids it), which for a heavy class is
/// far past the duty's `receipt_deadline`: a cut at that global deadline stopped the re-sends for
/// most of the window the chain was still giving the claim.
pub fn own_receipts_due_v1<K: Clone + Eq + std::hash::Hash + Ord>(
    filed: &HashMap<K, OwnFiledV1>,
    duties: &HashSet<K>,
    now: Instant,
    limit: usize,
) -> Vec<K> {
    let mut due: Vec<(&K, Instant)> = filed
        .iter()
        .filter(|(key, own)| duties.contains(*key) && own.schedule.due(now))
        .map(|(key, own)| (key, own.schedule.next_at))
        .collect();
    due.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
    due.into_iter().take(limit).map(|(key, _)| key.clone()).collect()
}

/// **The bytes of one own receipt's re-send**: the receipt `own` records, for `bond`, signed again
/// by `sign` — which the service binds to the hedged signer (fresh FIPS 204 `rnd`), so the copy is
/// new bytes, a new gossip digest, and a signature that verifies under the same key over the same
/// message. V3 when `own` carries a mask, V2 when not. `None` when signing fails.
pub fn own_receipt_resend_bytes_v1<S>(own: &OwnFiledV1, bond: PalwBondKeyV2, network_domain: Hash64, sign: S) -> Option<Vec<u8>>
where
    S: FnOnce(&[u8], &'static [u8]) -> Option<Vec<u8>>,
{
    let inner = |signature: Vec<u8>| PalwSeatReceiptV2 {
        claim: own.claim,
        verdict: own.verdict,
        seat_bond: bond,
        signed_daa: own.signed_daa,
        signature,
    };
    Some(match own.segments {
        Some(mask) => {
            let message = palw_receipt_message_v3(network_domain, own.claim, own.verdict, own.signed_daa, mask);
            let signature = sign(message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT)?;
            borsh::to_vec(&PalwSeatReceiptV3 { receipt: inner(signature), segments: mask }).expect("a V3 receipt serializes")
        }
        None => {
            let message = palw_receipt_message_v2(network_domain, own.claim, own.verdict, own.signed_daa);
            let signature = sign(message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT)?;
            borsh::to_vec(&inner(signature)).expect("a receipt serializes")
        }
    })
}

#[cfg(test)]
mod tests {
    //! The pool against the audit's flood, on a real bound five-seat panel: real ML-DSA-87 keys
    //! registered in a real `PalwChainStateV2`, the chain-fact read the node makes
    //! (`palw_receipt_pool_facts_v1`), the acceptance validators the assembler binds
    //! (`validate_receipt_coverage_v2`, `validate_receipt_quorum_v2`) and the licence selection it
    //! runs (`palw_select_coverage_licence_v2`) — so "the claim still licenses" is the chain's own
    //! answer, not this module's.
    use super::*;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
    };
    use kaspa_consensus_core::palw_panel_v2::{
        PalwPanelParamsV2, PalwPanelV2Error, PalwReceiptPanelFactV1, PalwReceiptQuorumV2, derive_panel_v2, palw_receipt_pool_facts_v1,
        palw_select_coverage_licence_v2, validate_receipt_coverage_v2, validate_receipt_quorum_v2,
    };
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateParamsV2, apply_palw_transition_v2,
        palw_operator_id_v2,
    };
    use kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use libcrux_ml_dsa::ml_dsa_87::{MLDSA87KeyPair, generate_key_pair};

    const SIGNED_DAA: u64 = 108;

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(h(v), 0))
    }

    fn verify(key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
        kaspa_txscript::verify_mldsa87_with_context(key, message, signature, context).unwrap_or(false)
    }

    fn sign(kp: &MLDSA87KeyPair, message: &Hash64, context: &[u8], rnd: u8) -> Vec<u8> {
        libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), context, [rnd; 32]).unwrap().as_ref().to_vec()
    }

    /// A chain holding one claim `PanelBound` on a five-seat panel (the shipped seats and quorum),
    /// every bond registered under a real ML-DSA-87 key. Bond 1 is the executor; 2, 3, 4, 7, 8, 9
    /// are six eligible operators for five seats, so one registered bond is a stranger to the panel.
    struct Chain {
        state: PalwChainStateV2,
        claim: Hash64,
        sp: PalwStateParamsV2,
        p: PalwPanelParamsV2,
        net: Hash64,
        seats: Vec<PalwBondKeyV2>,
        keys: HashMap<PalwBondKeyV2, MLDSA87KeyPair>,
    }

    impl Chain {
        fn new() -> Self {
            let ctx = |block: u64, daa: u64| PalwBlockContextV2 {
                block: kaspa_consensus_core::BlockHash::from_u64_word(block),
                daa_score: daa,
                blue_score: block,
                subsidy: 0,
            };
            let sp = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap();
            let registered = [(1u64, 0x21u8), (2, 0x22), (3, 0x23), (4, 0x24), (7, 0x25), (8, 0x26), (9, 0x27)];
            let keys: HashMap<PalwBondKeyV2, MLDSA87KeyPair> =
                registered.iter().map(|(b, _)| (bond(*b), generate_key_pair([*b as u8; 32]))).collect();
            let mut objects = vec![PalwConsensusObjectV2::ClassRegistered {
                class_id: h(1),
                artifact_root: h(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            }];
            for (b, operator) in registered {
                objects.push(PalwConsensusObjectV2::BondRegistered {
                    bond: bond(b),
                    pubkey: keys[&bond(b)].verification_key.as_ref().to_vec(),
                    operator_pubkey: vec![operator; 8],
                    collateral: 1_000_000,
                    payout_payload: h(0x9A11),
                    capable_classes: std::collections::BTreeSet::from([h(1)]),
                    signature: Vec::new(),
                });
            }
            let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100), &objects, None).unwrap();
            let executor = bond(1).0;
            let envelope = PalwAttemptEnvelopeV2 {
                attempt: PalwAttemptUnsignedV2 {
                    version: PALW_ATTEMPT_V2_VERSION,
                    network_domain: h(999),
                    challenge: challenge_v2(h(999), h(5), 1_700, 1, h(1), &executor),
                    class_id: h(1),
                    executor_bond: executor,
                    executor_pubkey: keys[&bond(1)].verification_key.as_ref().to_vec(),
                    operator_id: palw_operator_id_v2(&[0x21u8; 8]),
                    artifact_root: h(11),
                    trace_root: h(31),
                    output_root: h(32),
                    pwu: 40,
                    trace_manifest_root: h(33),
                    trace_chunk_count: 4,
                    trace_retention_daa: 999_999,
                    execution_root: h(41),
                },
                signature: vec![0x5A; kaspa_txscript::MLDSA87_SIG_LEN],
            };
            let claim = attempt_id_v2(&envelope.attempt);
            let (s2, _) = apply_palw_transition_v2(&s1, &sp, &ctx(2, 101), &[], Some(&envelope)).unwrap();
            let p = PalwPanelParamsV2::new(
                kaspa_consensus_core::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS,
                kaspa_consensus_core::palw_fp_devnet_v3::PALW_V2_PANEL_QUORUM,
                4,
            )
            .unwrap();
            let anchor = kaspa_consensus_core::BlockHash::from_u64_word(0xA0C0);
            let drawn = derive_panel_v2(&s2, &p, &claim, anchor, 0).expect("six eligible operators seat five");
            assert_eq!(drawn.len(), 5);
            let (state, _) = apply_palw_transition_v2(
                &s2,
                &sp,
                &ctx(3, 106),
                &[PalwConsensusObjectV2::PanelBound { claim, anchor, seats: drawn.clone() }],
                None,
            )
            .unwrap();
            Self { state, claim, sp, p, net: h(999), seats: drawn.iter().map(|seat| seat.bond).collect(), keys }
        }

        /// The one registered bond the panel did not seat (the executor aside).
        fn stranger(&self) -> PalwBondKeyV2 {
            *self.keys.keys().find(|b| **b != bond(1) && !self.seats.contains(b)).expect("six eligible, five seated")
        }

        fn every_bond(&self) -> Vec<PalwBondKeyV2> {
            let mut bonds: Vec<PalwBondKeyV2> = self.keys.keys().copied().collect();
            bonds.sort_unstable();
            bonds
        }

        /// The node's read, as the service makes it: the chain's own `palw_receipt_pool_facts_v1`.
        fn facts(&self) -> ReceiptChainFactsV1 {
            let mut facts = ReceiptChainFactsV1::default();
            let bonds = self.every_bond();
            facts.refresh(palw_receipt_pool_facts_v1(&self.state, &[self.claim], &bonds), &bonds.iter().copied().collect());
            facts
        }

        /// The same read before the tip has seen the bind: keys, and no panel.
        fn facts_before_the_bind(&self) -> ReceiptChainFactsV1 {
            let mut facts = ReceiptChainFactsV1::default();
            let bonds = self.every_bond();
            facts.refresh(palw_receipt_pool_facts_v1(&self.state, &[], &bonds), &bonds.iter().copied().collect());
            facts
        }

        fn seat_index(&self, seat: &PalwBondKeyV2) -> u16 {
            self.seats.iter().position(|s| s == seat).unwrap() as u16
        }

        /// Seat `seat`'s genuine V3 `Valid` receipt, its assigned mask, signed with `rnd` (a re-send
        /// is the same receipt signed with other randomness).
        fn v3(&self, seat: &PalwBondKeyV2, rnd: u8) -> PalwSeatReceiptV3 {
            let panel = self.state.panel(&self.claim).unwrap();
            let mask = palw_segment_assignment_v2(panel.anchor, self.claim, self.seats.len() as u16).mask_of(self.seat_index(seat));
            let message = palw_receipt_message_v3(self.net, self.claim, PalwReceiptVerdictV2::Valid, SIGNED_DAA, mask);
            PalwSeatReceiptV3 {
                receipt: PalwSeatReceiptV2 {
                    claim: self.claim,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: *seat,
                    signed_daa: SIGNED_DAA,
                    signature: sign(&self.keys[seat], &message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, rnd),
                },
                segments: mask,
            }
        }

        fn v2(&self, seat: &PalwBondKeyV2, rnd: u8) -> PalwSeatReceiptV2 {
            let message = palw_receipt_message_v2(self.net, self.claim, PalwReceiptVerdictV2::Valid, SIGNED_DAA);
            PalwSeatReceiptV2 {
                claim: self.claim,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: *seat,
                signed_daa: SIGNED_DAA,
                signature: sign(&self.keys[seat], &message, PALW_RECEIPT_V2_MLDSA87_CONTEXT, rnd),
            }
        }

        fn here(&self) -> PalwBlockContextV2 {
            PalwBlockContextV2 { block: kaspa_consensus_core::BlockHash::from_u64_word(9), daa_score: 110, blue_score: 9, subsidy: 0 }
        }

        /// Whether the coverage door licenses from `candidates`, as the assembler asks it.
        fn v3_licenses(&self, candidates: &[PalwSeatReceiptV3]) -> bool {
            let here = self.here();
            let coverage = |receipts: &[PalwSeatReceiptV3]| {
                validate_receipt_coverage_v2(
                    &self.state,
                    &self.p,
                    &self.sp,
                    &here,
                    self.net,
                    &self.claim,
                    receipts,
                    verify,
                    false,
                    None,
                )
            };
            palw_select_coverage_licence_v2(candidates, coverage, |_| true).is_some()
        }

        /// Whether the V1 quorum licenses from `candidates`, by the processor's greedy assembler
        /// (`palw_v2_receipt_quorum_assemble_impl`): a candidate that keeps the set clean stays, one
        /// that poisons it is dropped.
        fn v2_licenses(&self, candidates: &[PalwSeatReceiptV2]) -> bool {
            let here = self.here();
            let mut kept: Vec<PalwSeatReceiptV2> = Vec::new();
            let mut licensed = false;
            for candidate in candidates {
                let mut attempt = kept.clone();
                attempt.push(candidate.clone());
                match validate_receipt_quorum_v2(&self.state, &self.p, &self.sp, &here, self.net, &self.claim, &attempt, verify) {
                    Ok(quorum) => {
                        licensed |= matches!(quorum, PalwReceiptQuorumV2::Licensed { .. });
                        kept = attempt;
                    }
                    Err(PalwPanelV2Error::NoQuorum { .. }) => kept = attempt,
                    Err(_) => {}
                }
            }
            licensed
        }
    }

    /// Junk a flooder sends: a borsh-valid receipt naming the claim and `seat`, with a signature of
    /// ML-DSA-87's length that no key produced — distinct per `n`, so every copy is a new digest.
    fn junk_v3(chain: &Chain, seat: PalwBondKeyV2, n: u64) -> PalwSeatReceiptV3 {
        let mut r = chain.v3(&chain.seats[0], 0);
        r.receipt.seat_bond = seat;
        r.receipt.signature =
            (0..kaspa_txscript::MLDSA87_SIG_LEN).map(|i| (i as u64).wrapping_mul(n + 7).wrapping_add(n) as u8).collect();
        r
    }

    fn junk_v2(chain: &Chain, seat: PalwBondKeyV2, n: u64) -> PalwSeatReceiptV2 {
        junk_v3(chain, seat, n).receipt
    }

    /// The audit's own junk: one to eight bytes of signature, about 150 bytes a receipt.
    fn audit_junk_v3(chain: &Chain, n: u64) -> PalwSeatReceiptV3 {
        let mut r = chain.v3(&chain.seats[(n % 5) as usize], 0);
        r.receipt.signature = vec![n as u8; 1 + (n % 8) as usize];
        r
    }

    fn pools(chain: &Chain) -> (PalwReceiptPoolV1<PalwSeatReceiptV2>, PalwReceiptPoolV1<PalwSeatReceiptV3>) {
        (PalwReceiptPoolV1::new(chain.net), PalwReceiptPoolV1::new(chain.net))
    }

    fn tick(
        chain: &Chain,
        facts: &ReceiptChainFactsV1,
        v2: &mut PalwReceiptPoolV1<PalwSeatReceiptV2>,
        v3: &mut PalwReceiptPoolV1<PalwSeatReceiptV3>,
        arrivals: Vec<ArrivedReceiptV1>,
    ) -> (Vec<ReceiptAdmitV1>, VerifyBudgetV1) {
        let mut budget = VerifyBudgetV1::per_tick();
        let kept: HashSet<Hash64> = [chain.claim].into();
        let outcomes = admit_receipt_arrivals_v1(arrivals, v2, v3, facts, &verify, &mut budget, 110, &kept);
        (outcomes, budget)
    }

    /// **The audit's attack, after the genuine receipts: nothing is evicted, and the claim licenses**
    /// (V3 and V2). The five genuine receipts arrive and are checked at the door; then the flood —
    /// the audit's sixteen one-to-eight-byte receipts, and a stronger one of full-length junk naming
    /// every seat and the stranger, eight times the old sixteen-slot cap — arrives a tick later.
    /// Before the fix the sixteen alone emptied the claim's slice on every node.
    #[test]
    fn a_flood_after_the_genuine_receipts_evicts_nothing_and_the_claim_licenses() {
        let chain = Chain::new();
        let facts = chain.facts();
        let (mut v2, mut v3) = pools(&chain);
        let genuine: Vec<ArrivedReceiptV1> = chain
            .seats
            .iter()
            .flat_map(|seat| [ArrivedReceiptV1::V3(chain.v3(seat, 0)), ArrivedReceiptV1::V2(chain.v2(seat, 0))])
            .collect();
        let (outcomes, _) = tick(&chain, &facts, &mut v2, &mut v3, genuine);
        assert!(outcomes.iter().all(|o| *o == ReceiptAdmitV1::Pooled), "{outcomes:?}");
        assert!(chain.v3_licenses(&v3.candidates(&chain.claim, &facts)) && chain.v2_licenses(&v2.candidates(&chain.claim, &facts)));

        let mut flood: Vec<ArrivedReceiptV1> = (0..16).map(|n| ArrivedReceiptV1::V3(audit_junk_v3(&chain, n))).collect();
        let targets: Vec<PalwBondKeyV2> = chain.seats.iter().copied().chain([chain.stranger(), bond(0xDEAD)]).collect();
        for n in 0..128u64 {
            let target = targets[(n as usize) % targets.len()];
            flood.push(ArrivedReceiptV1::V3(junk_v3(&chain, target, n)));
            flood.push(ArrivedReceiptV1::V2(junk_v2(&chain, target, n)));
        }
        let (outcomes, budget) = tick(&chain, &facts, &mut v2, &mut v3, flood);
        assert_eq!(outcomes.iter().filter(|o| **o == ReceiptAdmitV1::Malformed).count(), 16, "the audit's junk stops at the length");
        assert!(outcomes.iter().all(|o| !matches!(o, ReceiptAdmitV1::Displaced)), "nothing genuine was displaced: {outcomes:?}");
        assert!(budget.spent_on(&chain.claim) <= RECEIPT_VERIFIES_PER_CLAIM_PER_TICK);

        for seat in &chain.seats {
            assert!(v3.candidates(&chain.claim, &facts).contains(&chain.v3(seat, 0)), "seat {seat:?}'s V3 receipt survived the flood");
            assert!(v2.candidates(&chain.claim, &facts).contains(&chain.v2(seat, 0)), "seat {seat:?}'s V2 receipt survived the flood");
        }
        assert!(chain.v3_licenses(&v3.candidates(&chain.claim, &facts)), "the claim still licenses by coverage");
        assert!(chain.v2_licenses(&v2.candidates(&chain.claim, &facts)), "the claim still licenses by the V1 quorum");
        // At most the free slot beside each seat's receipt took junk, and no bond off the panel did.
        assert!(v3.pooled(&chain.claim).len() <= chain.seats.len() * RECEIPTS_PER_BOND);
        assert!(v3.pooled(&chain.claim).iter().all(|(r, _)| chain.seats.contains(&r.receipt.seat_bond)));
    }

    /// **The same flood BEFORE the genuine receipts cannot keep them out** (V3 and V2). The flood
    /// spends the tick's checks and fills every seat's free slots with unchecked junk; a tick later
    /// the genuine receipts arrive, verify, and each displaces its own bond's junk; the leftover
    /// budget clears what junk remains. The harsher order — more junk ahead of them in their own
    /// tick, spending the budget again — defers them, and the seats' re-sends (the same receipts
    /// re-signed) bring them in on the next tick.
    #[test]
    fn a_flood_before_the_genuine_receipts_cannot_keep_them_out() {
        let chain = Chain::new();
        let facts = chain.facts();
        let flood = |from: u64| -> Vec<ArrivedReceiptV1> {
            (from..from + 40)
                .flat_map(|n| {
                    let seat = chain.seats[(n as usize) % chain.seats.len()];
                    [ArrivedReceiptV1::V3(junk_v3(&chain, seat, n)), ArrivedReceiptV1::V2(junk_v2(&chain, seat, n))]
                })
                .collect()
        };
        let genuine = |rnd: u8| -> Vec<ArrivedReceiptV1> {
            chain
                .seats
                .iter()
                .flat_map(|seat| [ArrivedReceiptV1::V3(chain.v3(seat, rnd)), ArrivedReceiptV1::V2(chain.v2(seat, rnd))])
                .collect()
        };

        // Tick 1: the flood alone. Ten checks refuse ten junk receipts; the rest fill free slots.
        let (mut v2, mut v3) = pools(&chain);
        let (_, budget) = tick(&chain, &facts, &mut v2, &mut v3, flood(0));
        assert_eq!(budget.spent_on(&chain.claim), RECEIPT_VERIFIES_PER_CLAIM_PER_TICK);
        assert_eq!(v3.pooled(&chain.claim).len(), chain.seats.len() * RECEIPTS_PER_BOND, "every seat's slots hold junk");
        assert!(!chain.v3_licenses(&v3.candidates(&chain.claim, &facts)));

        // Tick 2: the genuine receipts. Each verifies and takes its own bond's junk slot.
        let (outcomes, _) = tick(&chain, &facts, &mut v2, &mut v3, genuine(0));
        assert!(outcomes.iter().all(|o| *o == ReceiptAdmitV1::Displaced), "{outcomes:?}");
        assert!(chain.v3_licenses(&v3.candidates(&chain.claim, &facts)), "the claim licenses by coverage");
        assert!(chain.v2_licenses(&v2.candidates(&chain.claim, &facts)), "and by the V1 quorum");

        // The harsh order, on fresh pools: flood, then more junk AHEAD of the genuine receipts in
        // their own tick, spending its checks again.
        let (mut v2, mut v3) = pools(&chain);
        tick(&chain, &facts, &mut v2, &mut v3, flood(0));
        let mut second = flood(1_000);
        second.extend(genuine(0));
        let (outcomes, _) = tick(&chain, &facts, &mut v2, &mut v3, second);
        assert_eq!(outcomes[outcomes.len() - 10..], [ReceiptAdmitV1::Deferred; 10], "contested with no check left: deferred");
        assert!(!chain.v3_licenses(&v3.candidates(&chain.claim, &facts)), "not yet");
        // The re-sends: the same receipts, signed with fresh randomness — new bytes, new digests.
        let (outcomes, _) = tick(&chain, &facts, &mut v2, &mut v3, genuine(0x77));
        assert!(outcomes.iter().all(|o| o.kept()), "{outcomes:?}");
        assert!(chain.v3_licenses(&v3.candidates(&chain.claim, &facts)), "the re-sends license by coverage");
        assert!(chain.v2_licenses(&v2.candidates(&chain.claim, &facts)), "and by the V1 quorum");
    }

    /// **This node's own receipt survives any flood** — it is not in the evictable pool at all. A
    /// flood naming its own bond, other seats, strangers and hundreds of invented claims (past the
    /// claim ceiling) leaves it first among the candidates, and the claim licensing.
    #[test]
    fn this_nodes_own_receipt_survives_any_flood() {
        let chain = Chain::new();
        let facts = chain.facts();
        let (mut v2, mut v3) = pools(&chain);
        let me = chain.seats[0];
        v3.insert_own(chain.v3(&me, 0), SIGNED_DAA);
        v2.insert_own(chain.v2(&me, 0), SIGNED_DAA);
        for round in 0..4u64 {
            let mut flood: Vec<ArrivedReceiptV1> = Vec::new();
            for n in 0..64u64 {
                let seat = if n % 2 == 0 { me } else { chain.seats[(n as usize) % 5] };
                flood.push(ArrivedReceiptV1::V3(junk_v3(&chain, seat, round * 1_000 + n)));
                flood.push(ArrivedReceiptV1::V2(junk_v2(&chain, seat, round * 1_000 + n)));
            }
            for n in 0..(RECEIPT_POOL_MAX_CLAIMS as u64 + 40) {
                let mut other = junk_v3(&chain, me, round * 1_000 + n);
                other.receipt.claim = h(0xC1A1_0000 + n);
                flood.push(ArrivedReceiptV1::V3(other));
            }
            tick(&chain, &facts, &mut v2, &mut v3, flood);
            assert_eq!(v3.candidates(&chain.claim, &facts).first(), Some(&chain.v3(&me, 0)), "round {round}: own V3 receipt first");
            assert_eq!(v2.candidates(&chain.claim, &facts).first(), Some(&chain.v2(&me, 0)), "round {round}: own V2 receipt first");
        }
        assert!(v3.has_own(&chain.claim) && v2.has_own(&chain.claim));
        let rest: Vec<ArrivedReceiptV1> = chain.seats[1..].iter().map(|seat| ArrivedReceiptV1::V3(chain.v3(seat, 0))).collect();
        tick(&chain, &facts, &mut v2, &mut v3, rest);
        assert!(chain.v3_licenses(&v3.candidates(&chain.claim, &facts)), "own + the four others license");
        // Only the sweep's rule removes it — chain facts and this node's own acts: while a duty
        // names the claim it stays, however late; once none does, this node's submission or the
        // retention age retires it.
        let live: HashSet<Hash64> = [chain.claim].into();
        let (none, mut submitted): (HashSet<Hash64>, HashMap<Hash64, u64>) = (HashSet::new(), HashMap::new());
        fn sweep<'a>(live: &'a HashSet<Hash64>, submitted: &'a HashMap<Hash64, u64>, current_daa: u64) -> ReceiptSweepV1<'a> {
            ReceiptSweepV1 { live, submitted, current_daa, retention_daa: 4_000 }
        }
        v3.sweep(&sweep(&live, &submitted, 1_000_000));
        assert!(v3.has_own(&chain.claim), "a duty names the claim");
        v3.sweep(&sweep(&none, &submitted, SIGNED_DAA + 4_000));
        assert!(v3.has_own(&chain.claim), "no duty, not submitted, inside the retention age");
        v3.sweep(&sweep(&none, &submitted, SIGNED_DAA + 4_001));
        assert!(!v3.has_own(&chain.claim), "past it");
        submitted.insert(chain.claim, 110);
        v2.sweep(&sweep(&none, &submitted, 110));
        assert!(!v2.has_own(&chain.claim), "no duty, and this node submitted for it");
    }

    /// **A stranger — a bond the bound panel does not name — never evicts anything.** With the
    /// panel known it is refused at the door, validly signed or not. Before the bind, with the
    /// claim's bond places full of receipts that verify, a stranger's validly signed receipt checks
    /// them, finds them good, and is refused: it may only ever take the place of junk.
    #[test]
    fn a_stranger_never_evicts_anything() {
        let chain = Chain::new();
        let stranger = chain.stranger();
        let (mut v2, mut v3) = pools(&chain);
        let facts = chain.facts();
        tick(&chain, &facts, &mut v2, &mut v3, chain.seats.iter().map(|s| ArrivedReceiptV1::V3(chain.v3(s, 0))).collect());
        let before = v3.pooled(&chain.claim);
        let mut signed_by_stranger = chain.v3(&chain.seats[0], 0);
        signed_by_stranger.receipt.seat_bond = stranger;
        let message = signed_by_stranger.signed_message(chain.net).0;
        signed_by_stranger.receipt.signature = sign(&chain.keys[&stranger], &message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, 0);
        let (outcomes, budget) = tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(signed_by_stranger.clone())]);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::NotOnPanel]);
        assert_eq!(budget.used, 0, "refused before it cost a check");
        assert_eq!(v3.pooled(&chain.claim), before);

        // Before the bind: eight bonds' genuine receipts fill the claim's bond places.
        let mut extra: HashMap<PalwBondKeyV2, MLDSA87KeyPair> = HashMap::new();
        let mut read = palw_receipt_pool_facts_v1(&chain.state, &[], &chain.every_bond());
        for b in [0x51u64, 0x52, 0x53] {
            let kp = generate_key_pair([b as u8; 32]);
            read.seat_keys.push((bond(b), kp.verification_key.as_ref().to_vec()));
            extra.insert(bond(b), kp);
        }
        let mut unplaced = ReceiptChainFactsV1::default();
        let referenced: HashSet<PalwBondKeyV2> = read.seat_keys.iter().map(|(b, _)| *b).collect();
        unplaced.refresh(read, &referenced);
        let signed_as = |b: PalwBondKeyV2| {
            let mut r = chain.v3(&chain.seats[0], 0);
            r.receipt.seat_bond = b;
            let message = r.signed_message(chain.net).0;
            let kp = chain.keys.get(&b).or_else(|| extra.get(&b)).unwrap();
            r.receipt.signature = sign(kp, &message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, 0);
            r
        };
        let eight: Vec<PalwBondKeyV2> = chain.seats.iter().copied().chain([bond(0x51), bond(0x52), bond(0x53)]).collect();
        let (mut v2, mut v3) = pools(&chain);
        // Admitted past the budget, so they sit unchecked — the case where a stranger must check
        // before it may take anything.
        let mut spent = VerifyBudgetV1::new(0, 0);
        let kept = HashSet::new();
        for b in &eight {
            assert_eq!(v3.admit(signed_as(*b), &unplaced, &verify, &mut spent, 110, &kept), ReceiptAdmitV1::Pooled);
        }
        let before = v3.pooled(&chain.claim);
        assert!(before.iter().all(|(_, checked)| !checked));
        let mut budget = VerifyBudgetV1::per_tick();
        assert_eq!(v3.admit(signed_as(stranger), &unplaced, &verify, &mut budget, 110, &kept), ReceiptAdmitV1::NoRoom);
        let after = v3.pooled(&chain.claim);
        assert_eq!(
            after.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>(),
            before.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>()
        );
        assert!(after.iter().all(|(_, checked)| *checked), "the stranger's contest checked them, and they stood");
        // And junk from the stranger's bond is refused at the door.
        let (outcomes, _) = tick(&chain, &unplaced, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(junk_v3(&chain, stranger, 1))]);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::BadSignature]);
        assert_eq!(v3.pooled(&chain.claim).len(), 8);
    }

    /// **A receipt whose signature fails never displaces a verified one** — not within its bond,
    /// not across bonds, not with the budget spent, and its verdict is remembered, so sending it
    /// again costs nothing.
    #[test]
    fn a_receipt_that_fails_verification_never_displaces_a_verified_one() {
        let chain = Chain::new();
        let facts = chain.facts();
        let seat = chain.seats[1];
        let (mut v2, mut v3) = pools(&chain);
        // Two verified receipts of one seat fill its bond's slots: its receipt and a re-bound one
        // signed later.
        let mut later = chain.v3(&seat, 0);
        later.receipt.signed_daa = SIGNED_DAA + 1;
        let message = later.signed_message(chain.net).0;
        later.receipt.signature = sign(&chain.keys[&seat], &message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, 0);
        tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(chain.v3(&seat, 0)), ArrivedReceiptV1::V3(later)]);
        let before = v3.pooled(&chain.claim);
        assert_eq!(before.len(), 2);
        assert!(before.iter().all(|(_, checked)| *checked));

        let junk = junk_v3(&chain, seat, 9);
        let (outcomes, budget) = tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(junk.clone())]);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::BadSignature]);
        assert_eq!(budget.used, 1);
        let (outcomes, budget) = tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(junk)]);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::BadSignature], "remembered");
        assert_eq!(budget.used, 0, "…at no cost");
        // With the budget spent, a contested newcomer that has not proved itself is deferred.
        let mut spent = VerifyBudgetV1::new(0, 0);
        let kept = HashSet::new();
        assert_eq!(v3.admit(junk_v3(&chain, seat, 10), &facts, &verify, &mut spent, 110, &kept), ReceiptAdmitV1::Deferred);
        assert_eq!(v3.pooled(&chain.claim), before, "nothing moved");

        // Before the bind, with every registered bond's genuine receipt pooled and checked: junk
        // takes nothing, in its bond or across bonds — its own signature fails first.
        let unplaced = chain.facts_before_the_bind();
        let (mut v2, mut v3) = pools(&chain);
        let bonds = chain.every_bond();
        let genuine: Vec<ArrivedReceiptV1> = bonds
            .iter()
            .map(|b| {
                let mut r = chain.v3(&chain.seats[0], 0);
                r.receipt.seat_bond = *b;
                let message = r.signed_message(chain.net).0;
                r.receipt.signature = sign(&chain.keys[b], &message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, 0);
                ArrivedReceiptV1::V3(r)
            })
            .collect();
        tick(&chain, &unplaced, &mut v2, &mut v3, genuine);
        let before = v3.pooled(&chain.claim);
        assert_eq!(before.len(), bonds.len());
        // Junk saying what a checked receipt of its bond already says is refused before a check…
        let (outcomes, budget) = tick(&chain, &unplaced, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(junk_v3(&chain, bonds[0], 11))]);
        assert_eq!((outcomes, budget.used), (vec![ReceiptAdmitV1::Redundant], 0));
        // …and junk saying something else fails its own check, whether its bond is pooled or new.
        let mut other = junk_v3(&chain, bonds[0], 12);
        other.receipt.signed_daa = SIGNED_DAA + 2;
        let mut ninth = junk_v3(&chain, bond(0x99), 13);
        ninth.receipt.signed_daa = SIGNED_DAA + 2;
        let mut read = palw_receipt_pool_facts_v1(&chain.state, &[], &bonds);
        read.seat_keys.push((bond(0x99), generate_key_pair([0x99; 32]).verification_key.as_ref().to_vec()));
        let mut with_ninth = ReceiptChainFactsV1::default();
        with_ninth.refresh(read, &bonds.iter().copied().chain([bond(0x99)]).collect());
        let (outcomes, _) =
            tick(&chain, &with_ninth, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(other), ArrivedReceiptV1::V3(ninth)]);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::BadSignature; 2]);
        assert_eq!(v3.pooled(&chain.claim), before);
    }

    /// **The checks are bounded per claim and per tick.** A flood on one claim costs it at most
    /// `RECEIPT_VERIFIES_PER_CLAIM_PER_TICK`; a flood over many claims costs the tick at most
    /// `RECEIPT_VERIFIES_PER_TICK` — whatever arrives, and the scrub included.
    #[test]
    fn the_verify_budget_is_bounded_per_claim_and_per_tick() {
        let chain = Chain::new();
        let facts = chain.facts();
        let (mut v2, mut v3) = pools(&chain);
        let flood: Vec<ArrivedReceiptV1> =
            (0..200u64).map(|n| ArrivedReceiptV1::V3(junk_v3(&chain, chain.seats[(n % 5) as usize], n))).collect();
        let (_, budget) = tick(&chain, &facts, &mut v2, &mut v3, flood);
        assert_eq!(budget.spent_on(&chain.claim), RECEIPT_VERIFIES_PER_CLAIM_PER_TICK);
        assert_eq!(budget.used, RECEIPT_VERIFIES_PER_CLAIM_PER_TICK);

        // Many claims the tip does not place, each with junk naming registered bonds.
        let unplaced = chain.facts_before_the_bind();
        let (mut v2, mut v3) = pools(&chain);
        let flood: Vec<ArrivedReceiptV1> = (0..100u64)
            .flat_map(|c| {
                (0..4u64).map(move |n| (c, n)).map(|(c, n)| {
                    let mut r = junk_v3(&chain, chain.seats[(n % 5) as usize], c * 10 + n);
                    r.receipt.claim = h(0xC000 + c);
                    ArrivedReceiptV1::V3(r)
                })
            })
            .collect();
        let (_, budget) = tick(&chain, &unplaced, &mut v2, &mut v3, flood);
        assert_eq!(budget.used, RECEIPT_VERIFIES_PER_TICK, "the tick's whole budget and no more");
        assert!((0..100u64).all(|c| budget.spent_on(&h(0xC000 + c)) <= RECEIPT_VERIFIES_PER_CLAIM_PER_TICK));
    }

    /// **The pool is held to the panel the tip binds**: a receipt signed before the bind is pruned by
    /// the next read; one pooled before the bind for a bond the panel then does not seat is held
    /// aside — it verified, so a sibling panel may yet seat it — and never offered to the assembler.
    #[test]
    fn a_read_of_the_bind_prunes_the_pool_to_the_panel() {
        let chain = Chain::new();
        let stranger = chain.stranger();
        let (mut v2, mut v3) = pools(&chain);
        let mut by_stranger = chain.v3(&chain.seats[0], 0);
        by_stranger.receipt.seat_bond = stranger;
        let message = by_stranger.signed_message(chain.net).0;
        by_stranger.receipt.signature = sign(&chain.keys[&stranger], &message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, 0);
        let mut early = chain.v3(&chain.seats[1], 0);
        early.receipt.signed_daa = 100;
        let message = early.signed_message(chain.net).0;
        early.receipt.signature = sign(&chain.keys[&chain.seats[1]], &message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, 0);
        let arrivals = vec![
            ArrivedReceiptV1::V3(by_stranger),
            ArrivedReceiptV1::V3(early.clone()),
            ArrivedReceiptV1::V3(chain.v3(&chain.seats[2], 0)),
        ];
        let (outcomes, _) = tick(&chain, &chain.facts_before_the_bind(), &mut v2, &mut v3, arrivals);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::Pooled; 3], "before the bind the pool cannot tell");
        let facts = chain.facts();
        tick(&chain, &facts, &mut v2, &mut v3, Vec::new());
        assert_eq!(v3.candidates(&chain.claim, &facts), vec![chain.v3(&chain.seats[2], 0)], "only the panel's, signed after the bind");
        let held: Vec<PalwBondKeyV2> = v3.pooled(&chain.claim).into_iter().map(|(r, _)| r.receipt.seat_bond).collect();
        assert_eq!(held.len(), 2);
        assert!(held.contains(&stranger) && held.contains(&chain.seats[2]), "the early receipt went; the stranger's is held aside");
        let (outcomes, _) = tick(&chain, &chain.facts(), &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(early)]);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::Stale]);
    }

    /// **A seat's re-send is new bytes, and a pool that verified the first copy refuses it without a
    /// check.** Hedged ML-DSA-87 (fresh `rnd`) gives a new, valid signature over the same message.
    #[test]
    fn a_resend_is_a_new_digest_and_redundant_where_the_first_was_checked() {
        let chain = Chain::new();
        let facts = chain.facts();
        let seat = chain.seats[3];
        let (first, again) = (chain.v3(&seat, 0), chain.v3(&seat, 0x31));
        assert_ne!(first.receipt.signature, again.receipt.signature, "fresh randomness, a new signature");
        assert_ne!(borsh::to_vec(&first).unwrap(), borsh::to_vec(&again).unwrap(), "so new bytes, and a new gossip digest");
        assert_eq!(chain.v3(&seat, 0), first, "the same randomness signs the same bytes — the deterministic variant");
        let key = chain.keys[&seat].verification_key.as_ref().to_vec();
        let (message, context) = again.signed_message(chain.net);
        assert!(verify(&key, message.as_byte_slice(), &again.receipt.signature, context), "and it verifies");
        let (mut v2, mut v3) = pools(&chain);
        tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(first)]);
        let (outcomes, budget) = tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(again)]);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::Redundant]);
        assert_eq!(budget.used, 0);
    }

    /// **The claim ceiling**: a flood of invented claims cannot grow a pool past
    /// `RECEIPT_POOL_MAX_CLAIMS` beyond the kept ones, nor push out a kept claim — the old V3 pool's
    /// ceiling, kept.
    #[test]
    fn the_pool_is_held_to_its_ceiling_and_never_evicts_a_kept_claim() {
        let chain = Chain::new();
        let facts = chain.facts_before_the_bind();
        let mut v3: PalwReceiptPoolV1<PalwSeatReceiptV3> = PalwReceiptPoolV1::new(chain.net);
        let kept: HashSet<Hash64> = [h(0xC000 + 1), h(0xC000 + 2)].into();
        let mut spent = VerifyBudgetV1::new(0, 0);
        for c in 1..=(RECEIPT_POOL_MAX_CLAIMS as u64 + 100) {
            let mut r = junk_v3(&chain, chain.seats[0], c);
            r.receipt.claim = h(0xC000 + c);
            assert_eq!(v3.admit(r, &facts, &verify, &mut spent, 110, &kept), ReceiptAdmitV1::Pooled);
        }
        let held: Vec<Hash64> = v3.pooled_claim_ids().copied().collect();
        assert_eq!(held.len(), RECEIPT_POOL_MAX_CLAIMS + kept.len(), "the ceiling counts only what may be evicted");
        assert!(kept.iter().all(|c| held.contains(c)), "the kept claims stay, however old");
        assert!(held.contains(&h(0xC000 + RECEIPT_POOL_MAX_CLAIMS as u64 + 100)), "the claim that just arrived stays");
        // 354 evictable claims arrived for 256 places: claims 3..=100 went, in arrival order.
        assert!(!held.contains(&h(0xC000 + 3)) && !held.contains(&h(0xC000 + 100)), "the oldest evictable ones went");
        assert!(held.contains(&h(0xC000 + 101)), "…and only as many as the ceiling needed");
    }

    /// **The re-send fires while the panel stands and stops once it does not** — on a backoff of
    /// 30 s doubling to ten minutes, for the duty keys the chain still names, inside the window.
    #[test]
    fn the_rebroadcast_fires_while_panel_bound_and_stops_after() {
        assert_eq!(own_receipt_rebroadcast_gap_v1(0), Duration::from_secs(30));
        assert_eq!(own_receipt_rebroadcast_gap_v1(1), Duration::from_secs(60));
        assert_eq!(own_receipt_rebroadcast_gap_v1(4), Duration::from_secs(480));
        assert_eq!(own_receipt_rebroadcast_gap_v1(5), OWN_RECEIPT_REBROADCAST_MAX_GAP);
        assert_eq!(own_receipt_rebroadcast_gap_v1(u32::MAX), OWN_RECEIPT_REBROADCAST_MAX_GAP);

        let t0 = Instant::now();
        let filed = |claim: u64| OwnFiledV1 {
            claim: h(claim),
            verdict: PalwReceiptVerdictV2::Valid,
            signed_daa: SIGNED_DAA,
            segments: None,
            schedule: OwnRebroadcastV1::filed(t0),
        };
        let mut own: HashMap<u64, OwnFiledV1> = (1..=3).map(|k| (k, filed(k))).collect();
        let standing: HashSet<u64> = [1, 2, 3].into();
        let at = |s: u64| t0 + Duration::from_secs(s);
        assert!(own_receipts_due_v1(&own, &standing, at(10), 8).is_empty(), "not before its first gap");
        assert_eq!(own_receipts_due_v1(&own, &standing, at(31), 8), vec![1, 2, 3]);
        assert_eq!(own_receipts_due_v1(&own, &standing, at(31), 2), vec![1, 2], "at most `limit` a tick");
        own.get_mut(&1).unwrap().schedule.sent(at(31));
        assert_eq!(own_receipts_due_v1(&own, &standing, at(40), 8), vec![2, 3], "the one just sent waits");
        assert_eq!(own_receipts_due_v1(&own, &standing, at(91), 8), vec![2, 3, 1], "the longest-waiting first");
        // Claim 2 left `PanelBound` (licensed, voided, redrawn): its duty is gone, so is its re-send.
        let standing: HashSet<u64> = [1, 3].into();
        assert_eq!(own_receipts_due_v1(&own, &standing, at(91), 8), vec![3, 1]);
        // No duty, nothing goes out, however its schedule stands.
        assert!(own_receipts_due_v1(&own, &HashSet::new(), at(10_000), 8).is_empty());
    }

    /// **An own receipt outlives the duty's global `receipt_deadline` while its duty stands** (the
    /// launch review of this fix, its second finding). The duty is the chain's own read
    /// (`palw_seat_duties_v2`, which takes no DAA: it lists this seat for as long as the claim is
    /// `PanelBound`, which the bind arms to end at `bound + receipt_window_for_claim_v1` — for a heavy
    /// class on testnet-12, `1,399 × 10` DAA against a 600-DAA global window). Far past the duty's
    /// `receipt_deadline`, and past the retention age too, the own receipt stays first among the
    /// candidates and its re-send stays due; the claim leaving `PanelBound` ends both.
    #[test]
    fn an_own_receipt_outlives_the_global_receipt_deadline_while_its_duty_stands() {
        let chain = Chain::new();
        let facts = chain.facts();
        let me = chain.seats[0];
        let duties = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&chain.state, &chain.sp, &[me]);
        assert_eq!(duties.len(), 1, "the chain names this seat's duty");
        let duty = &duties[0];
        assert_eq!(duty.receipt_deadline, duty.bound_daa + chain.sp.window_receipt(), "the global window");
        let (mut v2, mut v3) = pools(&chain);
        v3.insert_own(chain.v3(&me, 0), SIGNED_DAA);
        let t0 = Instant::now();
        let key = (duty.claim_id, duty.bound_daa);
        let filed: HashMap<(Hash64, u64), OwnFiledV1> = [(
            key,
            OwnFiledV1 {
                claim: duty.claim_id,
                verdict: PalwReceiptVerdictV2::Valid,
                signed_daa: SIGNED_DAA,
                segments: Some(chain.v3(&me, 0).segments),
                schedule: OwnRebroadcastV1::filed(t0),
            },
        )]
        .into();

        let late = duty.receipt_deadline + 5_000;
        let live: HashSet<Hash64> = duties.iter().map(|d| d.claim_id).collect();
        let standing: HashSet<(Hash64, u64)> = duties.iter().map(|d| (d.claim_id, d.bound_daa)).collect();
        let submitted: HashMap<Hash64, u64> = [(chain.claim, late - 1)].into();
        let rule = ReceiptSweepV1 { live: &live, submitted: &submitted, current_daa: late, retention_daa: 4_000 };
        v3.sweep(&rule);
        v2.sweep(&rule);
        assert_eq!(v3.candidates(&chain.claim, &facts).first(), Some(&chain.v3(&me, 0)), "kept while the duty stands");
        assert_eq!(own_receipts_due_v1(&filed, &standing, t0 + Duration::from_secs(31), 8), vec![key], "and re-sent");

        // The claim leaves `PanelBound`: no duty names it, this node submitted for it — both end.
        let none = HashSet::new();
        v3.sweep(&ReceiptSweepV1 { live: &none, submitted: &submitted, current_daa: late, retention_daa: 4_000 });
        assert!(!v3.has_own(&chain.claim));
        assert!(own_receipts_due_v1(&filed, &HashSet::new(), t0 + Duration::from_secs(31), 8).is_empty());
    }

    /// **The gossip drain queues only what could be a receipt** — the service's drain is exactly
    /// `receipt_arrival_push_v1`. Bytes that are no receipt and the audit's short signatures are
    /// refused before they take a place; a V3 receipt queues as V3, a V2 as V2; the queue is held to
    /// `RECEIPT_ARRIVALS_MAX`, oldest out.
    #[test]
    fn the_gossip_drain_queues_only_well_formed_receipts() {
        let chain = Chain::new();
        let mut queue: VecDeque<ArrivedReceiptV1> = VecDeque::new();
        assert!(!receipt_arrival_push_v1(&mut queue, &[0xFF; 40]), "no receipt");
        assert!(!receipt_arrival_push_v1(&mut queue, &borsh::to_vec(&audit_junk_v3(&chain, 3)).unwrap()), "a short signature");
        assert!(queue.is_empty());
        let v3 = chain.v3(&chain.seats[0], 0);
        let v2 = chain.v2(&chain.seats[1], 0);
        assert!(receipt_arrival_push_v1(&mut queue, &borsh::to_vec(&v3).unwrap()));
        assert!(receipt_arrival_push_v1(&mut queue, &borsh::to_vec(&v2).unwrap()));
        assert_eq!(queue, VecDeque::from([ArrivedReceiptV1::V3(v3.clone()), ArrivedReceiptV1::V2(v2)]));
        for n in 0..RECEIPT_ARRIVALS_MAX as u64 {
            assert!(receipt_arrival_push_v1(&mut queue, &borsh::to_vec(&junk_v3(&chain, chain.seats[2], n)).unwrap()));
        }
        assert_eq!(queue.len(), RECEIPT_ARRIVALS_MAX);
        assert!(!queue.contains(&ArrivedReceiptV1::V3(v3)), "the oldest went first");
    }

    /// **The service's tick** (`receipt_pool_tick_v1`, which the panel service calls with its
    /// session's `palw_receipt_pool_facts_v1`): the read asks for every claim and seat bond the queued
    /// arrivals name; a tick with no tip state admits nothing and keeps them queued; the next tick
    /// with a read admits them against it, and the claim licenses from the candidates.
    #[test]
    fn the_service_tick_reads_the_tip_for_what_arrived_and_waits_without_a_read() {
        let chain = Chain::new();
        let (mut v2, mut v3) = pools(&chain);
        let mut facts = ReceiptChainFactsV1::default();
        let mut queue: VecDeque<ArrivedReceiptV1> = VecDeque::new();
        for seat in &chain.seats {
            assert!(receipt_arrival_push_v1(&mut queue, &borsh::to_vec(&chain.v3(seat, 0)).unwrap()));
        }
        let kept: HashSet<Hash64> = [chain.claim].into();
        let no_shuffle = |_: &mut [ArrivedReceiptV1]| {};

        let mut asked: Option<(Vec<Hash64>, Vec<PalwBondKeyV2>)> = None;
        let tick = receipt_pool_tick_v1(
            &mut queue,
            &mut v2,
            &mut v3,
            &mut facts,
            |claims, bonds| {
                asked = Some((claims, bonds));
                None
            },
            no_shuffle,
            &verify,
            110,
            &kept,
        );
        assert!(tick.is_none());
        let (claims, bonds) = asked.expect("the tip was asked");
        assert_eq!(claims, vec![chain.claim]);
        let mut seats = chain.seats.clone();
        seats.sort_unstable();
        assert_eq!(bonds, seats, "every seat bond an arrival names, none cached yet");
        assert_eq!(queue.len(), chain.seats.len(), "no read: the arrivals wait");
        assert_eq!(v3.pooled_len(), 0);

        let (outcomes, budget) = receipt_pool_tick_v1(
            &mut queue,
            &mut v2,
            &mut v3,
            &mut facts,
            |claims, bonds| Some(palw_receipt_pool_facts_v1(&chain.state, &claims, &bonds)),
            no_shuffle,
            &verify,
            110,
            &kept,
        )
        .expect("a read");
        assert_eq!(outcomes, vec![ReceiptAdmitV1::Pooled; 5]);
        assert_eq!(budget.used, 5);
        assert!(queue.is_empty());
        assert!(chain.v3_licenses(&v3.candidates(&chain.claim, &facts)), "the collector's candidates license");
    }

    /// **A re-send is the filed receipt, signed anew** — the bytes the service broadcasts
    /// (`own_receipt_resend_bytes_v1` bound to the hedged signer): they decode as the same receipt
    /// with a new signature that verifies, V3 for a filing with a mask and V2 for one without, and a
    /// pool that verified the first copy takes it as `Redundant` without a check.
    #[test]
    fn a_resend_is_the_filed_receipt_signed_anew() {
        let chain = Chain::new();
        let facts = chain.facts();
        let seat = chain.seats[1];
        let hedged = |rnd: u8| {
            let kp = &chain.keys[&seat];
            move |message: &[u8], context: &'static [u8]| {
                libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message, context, [rnd; 32]).ok().map(|sig| sig.as_ref().to_vec())
            }
        };
        let first = chain.v3(&seat, 0);
        let own = OwnFiledV1 {
            claim: chain.claim,
            verdict: PalwReceiptVerdictV2::Valid,
            signed_daa: SIGNED_DAA,
            segments: Some(first.segments),
            schedule: OwnRebroadcastV1::filed(Instant::now()),
        };
        let bytes = own_receipt_resend_bytes_v1(&own, seat, chain.net, hedged(0x5E)).unwrap();
        let Some(ArrivedReceiptV1::V3(again)) = ArrivedReceiptV1::decode(&bytes) else { panic!("a V3 receipt") };
        assert_eq!((again.segments, again.receipt.claim, again.receipt.seat_bond), (first.segments, chain.claim, seat));
        assert_ne!(again.receipt.signature, first.receipt.signature);
        assert!(
            chain
                .v3_licenses(&chain.seats.iter().map(|s| if *s == seat { again.clone() } else { chain.v3(s, 0) }).collect::<Vec<_>>())
        );
        let (mut v2, mut v3) = pools(&chain);
        tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(first)]);
        let (outcomes, budget) = tick(&chain, &facts, &mut v2, &mut v3, vec![ArrivedReceiptV1::V3(again)]);
        assert_eq!((outcomes, budget.used), (vec![ReceiptAdmitV1::Redundant], 0));

        let own_v2 = OwnFiledV1 { segments: None, ..own };
        let bytes = own_receipt_resend_bytes_v1(&own_v2, seat, chain.net, hedged(0x5F)).unwrap();
        let Some(ArrivedReceiptV1::V2(again)) = ArrivedReceiptV1::decode(&bytes) else { panic!("a V2 receipt") };
        let key = chain.keys[&seat].verification_key.as_ref().to_vec();
        let (message, context) = again.signed_message(chain.net);
        assert!(verify(&key, message.as_byte_slice(), &again.signature, context));
        assert_ne!(again, chain.v2(&seat, 0));
    }

    /// A stand-in verifier for the pool-logic tests below, which the pool treats as opaque: a
    /// signature whose first byte is `0xAA` verifies, any other does not.
    fn fake_verify(_key: &[u8], _message: &[u8], signature: &[u8], _context: &[u8]) -> bool {
        signature.first() == Some(&0xAA)
    }

    fn fake_receipt(claim: u64, b: u64, signed_daa: u64, good: bool, salt: u8) -> PalwSeatReceiptV2 {
        let mut signature = vec![salt; kaspa_txscript::MLDSA87_SIG_LEN];
        signature[0] = if good { 0xAA } else { 0x00 };
        PalwSeatReceiptV2 { claim: h(claim), verdict: PalwReceiptVerdictV2::Valid, seat_bond: bond(b), signed_daa, signature }
    }

    fn fake_facts(bonds: impl IntoIterator<Item = u64>, panels: Vec<PalwReceiptPanelFactV1>) -> ReceiptChainFactsV1 {
        let bonds: Vec<u64> = bonds.into_iter().collect();
        let mut facts = ReceiptChainFactsV1::default();
        let read = PalwReceiptPoolFactsV1 { panels, seat_keys: bonds.iter().map(|b| (bond(*b), vec![1u8; 8])).collect() };
        facts.refresh(read, &bonds.iter().map(|b| bond(*b)).collect());
        facts
    }

    fn bonds_in(pool: &PalwReceiptPoolV1<PalwSeatReceiptV2>, claim: u64) -> usize {
        pool.pooled(&h(claim)).iter().map(|(r, _)| r.seat_bond).collect::<HashSet<_>>().len()
    }

    /// **The cross-bond contest cannot grow a never-bound claim past its cap** (the launch review of
    /// this fix, its first finding; its probe, kept as the regression). A flood spends a tick's
    /// checks and fills eight bonds' places, each with a genuine receipt and junk beside it; then,
    /// tick after tick, a validly signed newcomer from a fresh registered bond contests the claim,
    /// and junk is offered for its second slot. The contest removes the junk it finds, but a bond
    /// that keeps its genuine receipt keeps its place, so the newcomer never gets one: the claim
    /// stays at eight bonds and sixteen receipts. Before the fix it ended at forty and forty-eight.
    #[test]
    fn the_cross_bond_contest_never_grows_an_unplaced_claim_past_its_cap() {
        const CLAIM: u64 = 0xC1A1;
        let facts = fake_facts(1..=40, Vec::new());
        let mut pool: PalwReceiptPoolV1<PalwSeatReceiptV2> = PalwReceiptPoolV1::new(h(999));
        let kept = HashSet::new();
        let within_cap = |pool: &PalwReceiptPoolV1<PalwSeatReceiptV2>| {
            bonds_in(pool, CLAIM) <= UNPLACED_BONDS_PER_CLAIM
                && pool.pooled(&h(CLAIM)).len() <= UNPLACED_BONDS_PER_CLAIM * RECEIPTS_PER_BOND
        };
        let mut spent = VerifyBudgetV1::new(0, 0);
        for b in 1..=8u64 {
            assert_eq!(
                pool.admit(fake_receipt(CLAIM, b, 100, true, 1), &facts, &fake_verify, &mut spent, 10, &kept),
                ReceiptAdmitV1::Pooled
            );
            assert_eq!(
                pool.admit(fake_receipt(CLAIM, b, 101, false, 2), &facts, &fake_verify, &mut spent, 10, &kept),
                ReceiptAdmitV1::Pooled
            );
        }
        assert_eq!((bonds_in(&pool, CLAIM), pool.pooled(&h(CLAIM)).len()), (8, 16));

        for b in 9..=40u64 {
            let mut budget = VerifyBudgetV1::per_tick();
            let outcome = pool.admit(fake_receipt(CLAIM, b, 100, true, 3), &facts, &fake_verify, &mut budget, 10, &kept);
            assert!(matches!(outcome, ReceiptAdmitV1::Deferred | ReceiptAdmitV1::NoRoom), "bond {b}: {outcome:?}");
            assert!(budget.spent_on(&h(CLAIM)) <= RECEIPT_VERIFIES_PER_CLAIM_PER_TICK);
            let mut spent = VerifyBudgetV1::new(0, 0);
            let junk = pool.admit(fake_receipt(CLAIM, b, 101, false, 4), &facts, &fake_verify, &mut spent, 10, &kept);
            assert_eq!(junk, ReceiptAdmitV1::Deferred, "bond {b}: no place to put junk in");
            assert!(within_cap(&pool), "bond {b}: {} bonds, {} receipts", bonds_in(&pool, CLAIM), pool.pooled(&h(CLAIM)).len());
        }
        // What is left is the eight genuine receipts, every one checked — the junk beside them was
        // what the contests removed.
        let left = pool.pooled(&h(CLAIM));
        assert_eq!(left.len(), 8);
        let first_eight: HashSet<PalwBondKeyV2> = (1..=8).map(bond).collect();
        assert!(left.iter().all(|(r, checked)| *checked && r.signature[0] == 0xAA && first_eight.contains(&r.seat_bond)));
    }

    /// **No sequence of arrivals takes a never-bound claim past `UNPLACED_BONDS_PER_CLAIM` bonds or
    /// sixteen receipts** — random arrivals from forty registered bonds, good and bad signatures,
    /// ticks with checks and ticks without, scrubs between, the invariant asked after every step.
    #[test]
    fn no_sequence_of_arrivals_grows_an_unplaced_claim_past_its_cap() {
        use rand::{Rng, SeedableRng};
        const CLAIM: u64 = 0xC1A3;
        let facts = fake_facts(1..=40, Vec::new());
        let kept = HashSet::new();
        for seed in 0..8u64 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let mut pool: PalwReceiptPoolV1<PalwSeatReceiptV2> = PalwReceiptPoolV1::new(h(999));
            let mut budget = VerifyBudgetV1::per_tick();
            for step in 0..4_000u32 {
                if rng.gen_ratio(1, 20) {
                    budget = if rng.gen_bool(0.5) { VerifyBudgetV1::per_tick() } else { VerifyBudgetV1::new(0, 0) };
                }
                if rng.gen_ratio(1, 50) {
                    pool.scrub(&facts, &fake_verify, &mut budget, &kept);
                }
                let receipt =
                    fake_receipt(CLAIM, rng.gen_range(1..=40), rng.gen_range(100..104), rng.gen_bool(0.4), rng.gen_range(0..=255));
                pool.admit(receipt, &facts, &fake_verify, &mut budget, 10, &kept);
                let (bonds, receipts) = (bonds_in(&pool, CLAIM), pool.pooled(&h(CLAIM)).len());
                assert!(
                    bonds <= UNPLACED_BONDS_PER_CLAIM && receipts <= UNPLACED_BONDS_PER_CLAIM * RECEIPTS_PER_BOND,
                    "seed {seed}, step {step}: {bonds} bonds, {receipts} receipts"
                );
            }
        }
    }

    /// **A one-tick flip to a sibling panel keeps the other panel's verified receipts** (the launch
    /// review of this fix, its third finding; its probe, kept as the regression). The tip holds
    /// panel A and the five A-seats' receipts are pooled and verified; one tick the tip holds
    /// sibling B (same bound DAA, another anchor, other seats): A's receipts are held aside and not
    /// offered; back on A, all five are offered again. The prune dropped them all, and only the
    /// seats' backed-off re-sends brought them back. An unchecked receipt of a bond off the tip's
    /// panel, and one signed before the bind, are still pruned.
    #[test]
    fn a_one_tick_sibling_flip_keeps_the_other_panels_verified_receipts() {
        const CLAIM: u64 = 0xC1A2;
        let a_seats: Vec<u64> = vec![1, 2, 3, 4, 5];
        let b_seats: Vec<u64> = vec![6, 7, 8, 9, 10];
        let panel = |anchor: u64, seats: &[u64]| PalwReceiptPanelFactV1 {
            claim_id: h(CLAIM),
            bound_daa: 100,
            anchor: h(anchor),
            seats: seats.iter().map(|b| bond(*b)).collect(),
        };
        let on_a = fake_facts(1..=10, vec![panel(0xA, &a_seats)]);
        let on_b = fake_facts(1..=10, vec![panel(0xB, &b_seats)]);
        let mut v2: PalwReceiptPoolV1<PalwSeatReceiptV2> = PalwReceiptPoolV1::new(h(999));
        let mut v3: PalwReceiptPoolV1<PalwSeatReceiptV3> = PalwReceiptPoolV1::new(h(999));
        let kept: HashSet<Hash64> = [h(CLAIM)].into();
        let tick = |facts: &ReceiptChainFactsV1,
                    v2: &mut PalwReceiptPoolV1<PalwSeatReceiptV2>,
                    v3: &mut PalwReceiptPoolV1<PalwSeatReceiptV3>,
                    arrivals: Vec<ArrivedReceiptV1>,
                    spend: bool| {
            let mut budget = if spend { VerifyBudgetV1::per_tick() } else { VerifyBudgetV1::new(0, 0) };
            admit_receipt_arrivals_v1(arrivals, v2, v3, facts, &fake_verify, &mut budget, 106, &kept)
        };

        let genuine: Vec<PalwSeatReceiptV2> = a_seats.iter().map(|b| fake_receipt(CLAIM, *b, 105, true, 5)).collect();
        let outcomes = tick(&on_a, &mut v2, &mut v3, genuine.iter().cloned().map(ArrivedReceiptV1::V2).collect(), true);
        assert_eq!(outcomes, vec![ReceiptAdmitV1::Pooled; 5]);
        // Beside them, unchecked (the tick's checks spent): junk in each A-seat's free slot, saying
        // something the genuine receipt does not (or it would be refused as `Redundant`).
        let junk: Vec<ArrivedReceiptV1> =
            a_seats.iter().map(|b| ArrivedReceiptV1::V2(fake_receipt(CLAIM, *b, 106, false, 6))).collect();
        assert_eq!(tick(&on_a, &mut v2, &mut v3, junk, false), vec![ReceiptAdmitV1::Pooled; 5]);
        assert_eq!(v2.pooled(&h(CLAIM)).len(), 10);

        // One tick on sibling B, with no checks left to scrub with.
        tick(&on_b, &mut v2, &mut v3, Vec::new(), false);
        assert!(v2.candidates(&h(CLAIM), &on_b).is_empty(), "A's receipts are not offered for B");
        assert_eq!(v2.pooled(&h(CLAIM)).len(), 5, "A's verified receipts held aside, the unchecked junk beside them pruned");
        // And back on A: all five, as they were.
        tick(&on_a, &mut v2, &mut v3, Vec::new(), false);
        let offered = v2.candidates(&h(CLAIM), &on_a);
        assert_eq!(offered.len(), 5);
        assert!(genuine.iter().all(|r| offered.contains(r)), "every A-seat receipt survived A -> B -> A");

        // A redraw binds later: what was signed before it counts for no seat of the new panel, nor
        // of a sibling of it, and goes.
        let redrawn = fake_facts(1..=10, vec![PalwReceiptPanelFactV1 { bound_daa: 700, ..panel(0xC, &a_seats) }]);
        tick(&redrawn, &mut v2, &mut v3, Vec::new(), false);
        assert!(!v2.contains_claim(&h(CLAIM)));
    }

    /// **What a claim holds aside off the tip's panel is bounded** by `OFF_PANEL_BONDS_PER_CLAIM`
    /// bonds, the most recently heard kept.
    #[test]
    fn the_receipts_held_aside_off_the_panel_are_bounded() {
        const CLAIM: u64 = 0xC1A4;
        let facts = fake_facts(1..=20, Vec::new());
        let mut pool: PalwReceiptPoolV1<PalwSeatReceiptV2> = PalwReceiptPoolV1::new(h(999));
        let kept = HashSet::new();
        // Before the bind: eight bonds, verified.
        for b in 1..=8u64 {
            let mut budget = VerifyBudgetV1::per_tick();
            assert_eq!(
                pool.admit(fake_receipt(CLAIM, b, 100, true, 1), &facts, &fake_verify, &mut budget, 10, &kept),
                ReceiptAdmitV1::Pooled
            );
        }
        // The tip binds panel A (bonds 11..=15), whose seats' receipts arrive and verify.
        let panel = |anchor: u64, seats: std::ops::RangeInclusive<u64>| {
            fake_facts(
                1..=20,
                vec![PalwReceiptPanelFactV1 {
                    claim_id: h(CLAIM),
                    bound_daa: 100,
                    anchor: h(anchor),
                    seats: seats.map(bond).collect(),
                }],
            )
        };
        let on_a = panel(0xA, 11..=15);
        let mut budget = VerifyBudgetV1::per_tick();
        pool.prune_to_panels(&on_a);
        for b in 11..=15u64 {
            assert_eq!(
                pool.admit(fake_receipt(CLAIM, b, 101, true, 2), &on_a, &fake_verify, &mut budget, 10, &kept),
                ReceiptAdmitV1::Pooled
            );
        }
        assert_eq!(bonds_in(&pool, CLAIM), 13, "A's five, and the eight from before the bind held aside");
        // A sibling seats 16..=20: A's five join the held-aside, and the oldest heard go.
        pool.prune_to_panels(&panel(0xB, 16..=20));
        let held: HashSet<PalwBondKeyV2> = pool.pooled(&h(CLAIM)).into_iter().map(|(r, _)| r.seat_bond).collect();
        assert_eq!(held.len(), OFF_PANEL_BONDS_PER_CLAIM);
        assert!((11..=15).all(|b| held.contains(&bond(b))), "the sibling's seats, heard last, are kept");
        assert!((1..=5).all(|b| !held.contains(&bond(b))), "the oldest heard went");
    }
}
