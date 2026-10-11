//! **FINX (2026-10-08) — P1: the finality guard and the PALW fork choice as an executable model, and
//! the change candidates compared over the same adversaries.**
//!
//! Record: `docs/design/palw/finality-palw-consistency.md` (the property, the violations, the pipeline
//! tests that back the model's key cases, and the table [`finx_p1_model_table`] prints).
//!
//! # What is modelled
//!
//! A tree of branches over one shared prefix, one model block per branch per DAA tick (testnet-12's
//! clock: one tick a slot, 120 s). A block carries cumulative blue score and blue work (a blue heartbeat
//! 2^24, an attempt header 2^20 — in units of 2^20: 16 and 1), the attempts of the bonds active on its
//! branch, and its chain's PALW claim lifecycle: an attempt makes a claim `Created` (weight 0 past F-W),
//! a quorum (3 of 5) of its panel's seats whose receipts reach the branch's producers licenses it, it is
//! `Final` one applied challenge window later and `Voided` past its deadline. The keys are the real
//! [`PalwCandidateOrderV1`] (safe frontier under the resolved-prefix rule, safe weight, live total). A
//! node holds a subset of the blocks, a sink, and a finality point (the chain block `finality_depth`
//! blue score under the sink, from the previous resolve, as the processor computes it).
//!
//! The shipped rule is modelled with the shipped functions: [`palw_reorg_strict_economic_win_v1`] with
//! the processor's shallow-tie question (the incumbent's chain above the fork spans at most
//! [`PALW_REORG_SHALLOW_TIE_DAA_V1`] ticks, GHOSTDAG-heavier, the DAA not lowered) is the deep-reorg
//! gate; [`palw_ibd_commit_strict_economic_v1`] is the IBD commit; the sink search pops GHOSTDAG's order
//! (blue work, hash) over the tips in the finality point's future, accepts an extension of the previous
//! sink, asks the gate otherwise and pushes a refused candidate's parent; the relay skips a block whose
//! blue work is not above the virtual's merge-depth root (`protocol/flows/src/v7/blockrelay/flow.rs`).
//!
//! # What is not modelled (and why no verdict rests on it)
//!
//! * Merging across branches. A branch whose fork is deeper than `merge_depth` (30 blue score, ten ticks
//!   of a 3-blue side) is merge-breaking for the other side's virtual; under that, a merged branch's
//!   carriers are accepted by the merging chain, which can at best TIE the keys — and a deep tie keeps
//!   the incumbent (pinned through the pipeline: `finx_p0_a_*`). So partitions of 2 and 3 ticks are run
//!   only where nothing is carried (no licence), and every licence case is at least 20 ticks long.
//! * UTXO conflicts. Each branch is valid by itself; a double spend is a payment carried by one branch
//!   only, and "reversed" means an honest node's sink chain stopped holding it past the shallow window.
//! * Bond stake. Bonds weigh one each (testnet-12's eight genesis cards are equal).
//!
//! Run: `cargo test -p kaspa-consensus-core --test finality_palw_consistency_model -- --nocapture`
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_fork_authority_v2::{
    PALW_REORG_SHALLOW_TIE_DAA_V1, PalwDeepReorgV2, PalwIbdCommitV2, palw_ibd_commit_strict_economic_v1,
    palw_reorg_strict_economic_win_v1,
};
use kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1;
use std::cmp::Ordering;
use std::fmt::Write as _;
use std::rc::Rc;

const BONDS: usize = 8;
const SEATS: u32 = 5;
const QUORUM: u32 = 3;
const PWU: u128 = 1_000;
/// Rule K's checkpoint: more than 2/3 of the bonds signed attempts on the candidate's branch above the
/// fork.
const SUPERMAJORITY: u32 = (2 * BONDS as u32) / 3 + 1;
/// Rule E's even split: where both sides' participation ties at no fewer than a third of the bonds,
/// GHOSTDAG's order decides (an adversary that ties the honest chain's participation holds as many
/// active bonds as the honest side — outside the assumption). Below it (a heartbeat-only period: zero
/// on both sides) the incumbent is kept.
const EVEN_SPLIT_MIN: u32 = (BONDS as u32).div_ceil(3);
/// Blue work in units of 2^20: an attempt header carries 2^20 (`PALW_ATTEMPT_BLUE_WORK_LOG2`), a blue
/// heartbeat 2^24 (`PALW_HEARTBEAT_WORK_LOG2`).
const ATTEMPT_WORK: u128 = 1;
const BEAT_WORK: u128 = 16;
/// The most blue heartbeats a slot can hold at `ghostdag_k` = 1 (`hb_probe_*`: four per mergeset).
const BLUE_CAP: u64 = 4;

// =====================================================================================================
// Parameters
// =====================================================================================================

#[derive(Clone, Copy, Debug)]
struct ModelParams {
    /// Blue score (`Params::finality_depth`; testnet-12: `window_challenge / 2` = 600).
    finality_depth: u64,
    /// Blue score (testnet-12: 30).
    merge_depth: u64,
    /// Blue score — the deepest rule F holds the finality point under the sink.
    pruning_depth: u64,
    /// Ticks from a claim's acceptance to the first block that can carry its licence (anchor delay 20,
    /// then the receipts).
    licence_delay: u64,
    /// Ticks after acceptance by which an unlicensed claim is voided.
    licence_deadline: u64,
    /// The applied challenge window: `Final` at `licensed + challenge + 1`.
    challenge: u64,
    /// An honest bond attempts once per `attempt_every` ticks (staggered by bond).
    attempt_every: u64,
    /// Rule E's `W_p`: participation is consulted only for a fork at least this many ticks under the
    /// incumbent. Sized between the honest attempt interval (so every active honest bond has signed above
    /// the fork) and the licence delay (so no self-licensed claim exists on a private branch before
    /// participation outranks it).
    participation_depth: u64,
}

impl ModelParams {
    /// testnet-12's depths (`finx_p0_facts` prints them off the harness), with the applied challenge
    /// window `challenge` — 120 where the short window applies, 1,200 unshortened.
    fn t12(challenge: u64) -> Self {
        Self {
            finality_depth: 600,
            merge_depth: 30,
            pruning_depth: 4_000,
            licence_delay: 25,
            licence_deadline: 1_200,
            challenge,
            attempt_every: 12,
            participation_depth: 24,
        }
    }
}

/// Ticks until a side making `blue_per_tick` blue score a tick seals its finality point past a fork.
fn t_seal(p: &ModelParams, blue_per_tick: u64) -> u64 {
    p.finality_depth.div_ceil(blue_per_tick)
}

// =====================================================================================================
// The rules
// =====================================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// The status quo: GHOSTDAG-ordered sink search; finality guard; `palw_reorg_strict_economic_win`.
    Sq,
    /// LIVE-R1's C1: the gate's winners over EVERY tip, the best of them (V2-max), and the relay fetches
    /// below the merge-depth root.
    C1,
    /// C2: SQ, and a reorg the gate refuses is allowed when the challenger's blue work above the fork is
    /// more than 4x the incumbent's (the DNS overlay's emergency override).
    C2,
    /// C3: SQ, with "portable" weight — claims accepted at or below the fork — left out of both sides'
    /// keys.
    C3,
    /// F: SQ, and the finality point is held at the fork of any heavier competing branch the node holds
    /// and refuses (never deeper than `pruning_depth`), so a refusal is never sealed.
    F,
    /// C1 + C3.
    C1C3,
    /// E′: C1 + C3, and stake participation (distinct pre-fork bonds that signed an attempt on the branch
    /// above the fork) as the key after the economic ones — before the incumbent's privilege.
    EEconFirst,
    /// E (proposed): C1's search and relay, C3's fork-relative keys, and — for a fork at least `W_p`
    /// ticks deep — participation FIRST: participation > safe frontier > safe > live > (an even split of
    /// at least a third of the bonds on each side: GHOSTDAG's order) > incumbent; shallower, C1 + C3
    /// exactly.
    E,
    /// E + F (the hold triggered only by a branch E refuses on the incumbent's privilege alone).
    EF,
    /// K: E, and a candidate below the finality point is admitted when more than 2/3 of the bonds signed
    /// attempts on its branch above the fork and it wins E's order (a protocol-recognised checkpointed
    /// resync).
    K,
}

impl Rule {
    const ALL: [Rule; 10] =
        [Rule::Sq, Rule::C1, Rule::C2, Rule::C3, Rule::F, Rule::C1C3, Rule::EEconFirst, Rule::E, Rule::EF, Rule::K];
    fn max_search(self) -> bool {
        !matches!(self, Rule::Sq | Rule::C2 | Rule::C3 | Rule::F)
    }
    fn fork_relative_keys(self) -> bool {
        !matches!(self, Rule::Sq | Rule::C1 | Rule::C2 | Rule::F)
    }
    fn participation(self) -> bool {
        matches!(self, Rule::EEconFirst | Rule::E | Rule::EF | Rule::K)
    }
    fn participation_first(self) -> bool {
        matches!(self, Rule::E | Rule::EF | Rule::K)
    }
    fn freeze(self) -> bool {
        matches!(self, Rule::F | Rule::EF)
    }
    fn name(self) -> &'static str {
        match self {
            Rule::Sq => "SQ",
            Rule::C1 => "C1",
            Rule::C2 => "C2",
            Rule::C3 => "C3",
            Rule::F => "F",
            Rule::C1C3 => "C1+C3",
            Rule::EEconFirst => "E'",
            Rule::E => "E",
            Rule::EF => "E+F",
            Rule::K => "K",
        }
    }
}

// =====================================================================================================
// Blocks and chain state
// =====================================================================================================

#[derive(Clone, Debug)]
struct OpenClaim {
    id: u32,
    accepted_bs: u64,
    accepted_daa: u64,
    seats: u8,
    licensed: Option<u64>,
    /// Made on an adversary's private branch: no honest seat ever sees it.
    private: bool,
}

#[derive(Clone, Debug, Default)]
struct ChainSt {
    /// Unresolved claims, in acceptance order (ascending `accepted_bs`).
    open: Vec<OpenClaim>,
    /// `(accepted_bs, pwu)` of every `Final` claim on the chain.
    finals: Rc<Vec<(u64, u128)>>,
    safe: u128,
    frontier: u64,
}

impl ChainSt {
    fn immature_above(&self, fork_bs: Option<u64>) -> u128 {
        self.open.iter().filter(|c| c.licensed.is_some() && fork_bs.is_none_or(|f| c.accepted_bs > f)).count() as u128 * PWU
    }
    fn safe_above(&self, fork_bs: Option<u64>) -> u128 {
        match fork_bs {
            None => self.safe,
            Some(f) => self.finals.iter().filter(|(bs, _)| *bs > f).map(|(_, w)| *w).sum(),
        }
    }
    fn frontier_above(&self, fork_bs: Option<u64>) -> u64 {
        match fork_bs {
            Some(f) if self.frontier <= f => 0,
            _ => self.frontier,
        }
    }
}

#[derive(Clone, Debug)]
struct Blk {
    parent: Option<usize>,
    /// Binary lifting: `up[k]` is the 2^k-th ancestor (the root for a shorter chain).
    up: [usize; 16],
    height: u32,
    daa: u64,
    bs: u64,
    bw: u128,
    hash: u64,
    /// `height + 1` of the last block on this chain carrying an attempt by each bond (0: none).
    last_attempt: [u32; BONDS],
    st: Rc<ChainSt>,
    /// Bit 0: the merchant's payment X rides this block.
    pays: u8,
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A claim's panel: `SEATS` distinct bonds drawn from its id.
fn seats_of(claim: u32, seed: u64) -> u8 {
    let mut mask = 0u8;
    let mut x = splitmix(seed ^ (claim as u64).wrapping_mul(0x1000_0001));
    while mask.count_ones() < SEATS {
        x = splitmix(x);
        mask |= 1 << (x % BONDS as u64);
    }
    mask
}

/// Five seats: every bond of `core` (up to five), filled from the lowest other bonds.
fn seats_around(core: u8) -> u8 {
    let mut m = 0u8;
    for b in 0..BONDS {
        if core & (1 << b) != 0 && m.count_ones() < SEATS {
            m |= 1 << b;
        }
    }
    for b in 0..BONDS {
        if m.count_ones() < SEATS && m & (1 << b) == 0 {
            m |= 1 << b;
        }
    }
    m
}

// =====================================================================================================
// The world
// =====================================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Maj,
    Min,
    /// A node that hears nothing until a script action connects it.
    Off,
}

/// Where a scenario's pinned claim can be licensed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// Only by honest producers on this side — during the partition AND after it (the carrier was
    /// mined on one side and never re-broadcast: devnet r1's portable licence).
    Stuck(Side),
    /// Only on honest producers' chains (the adversary's seats sign there, and withhold from its own).
    PublicOnly,
    /// Only on the adversary's branch (its seats sign there alone).
    PrivateOnly,
}

#[derive(Clone, Debug)]
struct Node {
    name: &'static str,
    side: Side,
    known: Vec<bool>,
    /// The known blocks with no known child (kept as blocks are learned).
    tips: Vec<usize>,
    sink: usize,
    fin: usize,
    hears_adversary: bool,
    hears_all: bool,
    online: bool,
}

#[derive(Clone, Debug)]
struct Producer {
    side: Side,
    /// The node whose sink this producer extends (an honest miner builds on its own node's template).
    home: usize,
    blue_per_tick: u64,
    /// Bonds attempting from this producer (bitmask).
    bonds: u8,
    /// Ticks between attempts per bond (`None`: the honest `attempt_every`).
    attempt_every: Option<u64>,
    active: bool,
    adversary: bool,
    /// Junk attempt headers a tick (losing draws: blue work, no claim, no bond).
    junk: u64,
    private_tip: usize,
    /// DAA ticks this producer's clock runs ahead of wall time (an adversary may run its private branch
    /// one tick ahead: the lead cap allows up to two in a burst, `hb_probe_b_future_*`).
    lead: u64,
}

#[derive(Clone, Debug, Default)]
struct Outcome {
    converged_at_end: bool,
    /// Ticks from the heal to the first tick from which every honest node's sink stayed on one chain.
    converge_after: Option<u64>,
    /// First tick (after the fork) an honest node's finality point excluded a branch another honest
    /// node was on.
    sealed_at: Option<u64>,
    /// Two honest finality points on conflicting chains at the end.
    conflicting_finality: bool,
    /// Resolves that moved a sink off its own finality point's chain (rule K's sanctioned crossing).
    finality_crossings: u32,
    /// Node-resolves during which the finality point was held (rules F, E+F).
    finality_held: u64,
    /// The deepest reversal (ticks) of the merchant's payment on an honest node past the shallow window.
    payment_reversed_depth: Option<u64>,
    /// A reorg past the shallow window decided by blue work alone (the C2 override).
    blue_work_decided_deep_reorg: bool,
    ibd: Vec<String>,
    notes: Vec<String>,
}

struct World {
    p: ModelParams,
    rule: Rule,
    seed: u64,
    blocks: Vec<Blk>,
    nodes: Vec<Node>,
    producers: Vec<Producer>,
    next_claim: u32,
    pinned: Vec<(u32, Reach)>,
    /// Ordinary claims are licensed (else only the pinned ones are).
    licensing: bool,
    adv_bonds: u8,
    maj_bonds: u8,
    min_bonds: u8,
    healed: bool,
    /// The public chain's court voids this claim from this tick on (the DA time bomb).
    void_public: Option<(u32, u64)>,
    fork_tick: u64,
    tick: u64,
    merchant_block: Option<usize>,
    out: Outcome,
}

impl World {
    fn new(p: ModelParams, rule: Rule, seed: u64) -> Self {
        let genesis = Blk {
            parent: None,
            up: [0; 16],
            height: 0,
            daa: 0,
            bs: 0,
            bw: 0,
            hash: splitmix(seed),
            last_attempt: [0; BONDS],
            st: Rc::new(ChainSt::default()),
            pays: 0,
        };
        Self {
            p,
            rule,
            seed,
            blocks: vec![genesis],
            nodes: Vec::new(),
            producers: Vec::new(),
            next_claim: 1,
            pinned: Vec::new(),
            licensing: false,
            adv_bonds: 0,
            maj_bonds: 0,
            min_bonds: 0,
            healed: true,
            void_public: None,
            fork_tick: 0,
            tick: 0,
            merchant_block: None,
            out: Outcome::default(),
        }
    }

    // ---- chain queries --------------------------------------------------------------------------

    fn ancestor_at_height(&self, mut b: usize, h: u32) -> usize {
        while self.blocks[b].height > h {
            let diff = self.blocks[b].height - h;
            let k = 31 - diff.leading_zeros();
            b = self.blocks[b].up[k as usize];
        }
        b
    }
    /// `a` is on `b`'s chain (inclusive).
    fn is_chain_ancestor(&self, a: usize, b: usize) -> bool {
        self.blocks[a].height <= self.blocks[b].height && self.ancestor_at_height(b, self.blocks[a].height) == a
    }
    fn common_ancestor(&self, a: usize, b: usize) -> usize {
        let h = self.blocks[a].height.min(self.blocks[b].height);
        let (mut x, mut y) = (self.ancestor_at_height(a, h), self.ancestor_at_height(b, h));
        if x == y {
            return x;
        }
        for k in (0..16).rev() {
            let (ux, uy) = (self.blocks[x].up[k], self.blocks[y].up[k]);
            if ux != uy {
                x = ux;
                y = uy;
            }
        }
        self.blocks[x].parent.unwrap_or(x)
    }
    /// The highest block on `b`'s chain whose blue score is at most `bs`.
    fn chain_block_at_or_below_bs(&self, mut b: usize, bs: u64) -> usize {
        if self.blocks[b].bs <= bs {
            return b;
        }
        for k in (0..16).rev() {
            let u = self.blocks[b].up[k];
            if self.blocks[u].bs > bs {
                b = u;
            }
        }
        self.blocks[b].parent.unwrap_or(b)
    }
    /// `calculate_block_at_depth`: the highest chain block whose blue score is strictly below
    /// `bs(sink) - depth` (genesis while the chain is shallower).
    fn block_at_depth(&self, sink: usize, depth: u64) -> usize {
        let bs = self.blocks[sink].bs;
        if bs <= depth { 0 } else { self.chain_block_at_or_below_bs(sink, bs - depth - 1) }
    }
    fn normal_finality_point(&self, sink: usize) -> usize {
        self.block_at_depth(sink, self.p.finality_depth)
    }
    fn merge_root(&self, sink: usize) -> usize {
        self.block_at_depth(sink, self.p.merge_depth)
    }
    fn gd(&self, b: usize) -> (u128, u64) {
        (self.blocks[b].bw, self.blocks[b].hash)
    }
    fn participation(&self, tip: usize, fork: usize) -> u32 {
        let floor = self.blocks[fork].height + 1;
        self.blocks[tip].last_attempt.iter().filter(|h| **h > floor).count() as u32
    }
    fn keys(&self, b: usize, fork_bs: Option<u64>) -> PalwCandidateOrderV1 {
        let st = &self.blocks[b].st;
        PalwCandidateOrderV1::new(
            st.frontier_above(fork_bs),
            st.safe_above(fork_bs),
            st.immature_above(fork_bs),
            Hash64::from_u64_word(self.blocks[b].hash),
        )
    }
    fn tips_known(&self, n: usize) -> Vec<usize> {
        self.nodes[n].tips.clone()
    }

    // ---- the gate ---------------------------------------------------------------------------------

    /// The processor's shallow-tie question (`palw_reorg_shallow_ghostdag_win_v1`).
    fn shallow_ghostdag_win(&self, c: usize, s: usize) -> bool {
        let fork = self.common_ancestor(c, s);
        self.gd(c) > self.gd(s)
            && self.blocks[c].daa >= self.blocks[s].daa
            && self.blocks[fork].daa >= self.blocks[s].daa.saturating_sub(PALW_REORG_SHALLOW_TIE_DAA_V1)
    }

    /// Rule E consults participation only for a fork at least `W_p` ticks under the incumbent.
    fn participation_counts(&self, fork: usize, s: usize) -> bool {
        self.blocks[s].daa >= self.blocks[fork].daa + self.p.participation_depth
    }

    /// E's order between two branches, relative to their fork: `Greater` = `c` ranks above `s`.
    fn e_order(&self, c: usize, s: usize) -> Ordering {
        let fork = self.common_ancestor(c, s);
        let fb = Some(self.blocks[fork].bs);
        let (kc, ks) = (self.keys(c, fb), self.keys(s, fb));
        let part = self.participation(c, fork).cmp(&self.participation(s, fork));
        let econ = (kc.safe_frontier_blue_score, kc.safe_weight, kc.live_total).cmp(&(
            ks.safe_frontier_blue_score,
            ks.safe_weight,
            ks.live_total,
        ));
        if !self.participation_counts(fork, s) && !self.participation_counts(fork, c) {
            return econ;
        }
        if self.rule.participation_first() { part.then(econ) } else { econ.then(part) }
    }

    /// Whether `c` may replace the incumbent `s` (this rule's deep-reorg gate), and whether that answer
    /// was blue work's alone past the shallow window.
    fn gate(&self, c: usize, s: usize) -> (bool, bool) {
        if self.is_chain_ancestor(s, c) {
            return (true, false);
        }
        let fork = self.common_ancestor(c, s);
        if self.rule.participation() && self.participation_counts(fork, s) {
            return match self.e_order(c, s) {
                Ordering::Greater => (true, false),
                Ordering::Less => (false, false),
                Ordering::Equal if self.participation(c, fork) >= EVEN_SPLIT_MIN && self.participation(s, fork) >= EVEN_SPLIT_MIN => {
                    (self.gd(c) > self.gd(s), false)
                }
                Ordering::Equal => (self.shallow_ghostdag_win(c, s), false),
            };
        }
        let fb = if self.rule.fork_relative_keys() { Some(self.blocks[fork].bs) } else { None };
        let (inc, chal) = (self.keys(s, fb), self.keys(c, fb));
        let allow = palw_reorg_strict_economic_win_v1(&inc, &chal, || self.shallow_ghostdag_win(c, s)) == PalwDeepReorgV2::Allow;
        if !allow && self.rule == Rule::C2 {
            let base = self.blocks[fork].bw;
            let (cw, sw) = (self.blocks[c].bw - base, self.blocks[s].bw - base);
            if cw > 4 * sw {
                let deep = self.blocks[fork].daa + PALW_REORG_SHALLOW_TIE_DAA_V1 < self.blocks[s].daa;
                return (true, deep);
            }
        }
        (allow, false)
    }

    // ---- the sink search --------------------------------------------------------------------------

    fn resolve(&mut self, n: usize) {
        if !self.nodes[n].online {
            return;
        }
        let (s, f) = (self.nodes[n].sink, self.nodes[n].fin);
        let tips = self.tips_known(n);
        let (new_sink, by_blue_work) =
            if self.rule.max_search() { self.search_max(s, f, &tips) } else { self.search_heap(s, f, &tips) };
        if by_blue_work {
            self.out.blue_work_decided_deep_reorg = true;
        }
        if !self.is_chain_ancestor(f, new_sink) {
            self.out.finality_crossings += 1;
        }
        if let Some(x) = self.merchant_block
            && self.nodes[n].side != Side::Off
            && self.is_chain_ancestor(x, s)
            && !self.is_chain_ancestor(x, new_sink)
        {
            let depth = self.blocks[s].daa - self.blocks[x].daa;
            if depth > PALW_REORG_SHALLOW_TIE_DAA_V1 {
                let d = self.out.payment_reversed_depth.get_or_insert(depth);
                *d = (*d).max(depth);
            }
        }
        self.nodes[n].sink = new_sink;
        self.nodes[n].fin = self.finality_point_after(n);
    }

    /// The finality point the next resolve uses: the normal one, or under F / E+F the fork of a competing
    /// branch the node holds and does not take (never deeper than `pruning_depth` under the sink).
    fn finality_point_after(&mut self, n: usize) -> usize {
        let sink = self.nodes[n].sink;
        let normal = self.normal_finality_point(sink);
        if !self.rule.freeze() {
            return normal;
        }
        // A HOLD, never a retreat: only a branch still admissible under the previous finality point
        // (its fork at or above it) holds the point; one revealed after the seal does not reopen it.
        let prev = self.nodes[n].fin;
        let floor_bs = self.blocks[sink].bs.saturating_sub(self.p.pruning_depth);
        let mut held = normal;
        for t in self.tips_known(n) {
            if self.is_chain_ancestor(t, sink) {
                continue;
            }
            // F: a heavier branch refused (LIVE-R1's refusal streak). E+F: a branch refused on the
            // incumbent's privilege alone.
            let trigger = match self.rule {
                Rule::F => self.gd(t) > self.gd(sink),
                _ => self.e_order(t, sink) == Ordering::Equal,
            };
            if !trigger {
                continue;
            }
            let fork = self.common_ancestor(t, sink);
            if self.is_chain_ancestor(prev, fork)
                && self.blocks[fork].bs >= floor_bs
                && self.blocks[fork].height < self.blocks[held].height
            {
                held = fork;
            }
        }
        if held != normal {
            self.out.finality_held += 1;
        }
        held
    }

    /// The shipped search: GHOSTDAG's heap over the tips in the finality point's future.
    fn search_heap(&self, s: usize, f: usize, tips: &[usize]) -> (usize, bool) {
        let mut heap: Vec<usize> = tips.iter().copied().filter(|t| self.is_chain_ancestor(f, *t)).collect();
        loop {
            heap.sort_by_key(|b| self.gd(*b));
            let Some(c) = heap.pop() else { return (s, false) };
            let (ok, bw) = self.gate(c, s);
            if ok {
                return (c, bw);
            }
            if let Some(p) = self.blocks[c].parent
                && self.is_chain_ancestor(f, p)
                && !heap.iter().any(|h| self.is_chain_ancestor(p, *h))
            {
                heap.push(p);
            }
        }
    }

    /// C1's search: the gate's winners over every tip, and the best of them in the rule's order; the
    /// incumbent's own extension when none wins. Under K a tip below the finality point is admitted on a
    /// supermajority of participation and a win in E's order.
    fn search_max(&self, s: usize, f: usize, tips: &[usize]) -> (usize, bool) {
        let ext = tips.iter().copied().filter(|t| self.is_chain_ancestor(s, *t)).max_by_key(|t| self.gd(*t)).unwrap_or(s);
        let (mut best, mut best_by_bw) = (ext, false);
        for &t in tips {
            if t == ext {
                continue;
            }
            let admissible = self.is_chain_ancestor(f, t)
                || (self.rule == Rule::K && {
                    let fork = self.common_ancestor(t, s);
                    self.participation(t, fork) >= SUPERMAJORITY && self.e_order(t, s) == Ordering::Greater
                });
            if !admissible {
                continue;
            }
            let (ok, bw) = self.gate(t, s);
            if !ok {
                continue;
            }
            // V2-max: the best of the incumbent's own extension and every admissible winner, in the rule's
            // order; the extension keeps an exact tie.
            let better = {
                if self.rule.participation() {
                    self.e_order(t, best).then_with(|| self.gd(t).cmp(&self.gd(best))) == Ordering::Greater
                } else {
                    let fb = if self.rule.fork_relative_keys() { Some(self.blocks[self.common_ancestor(t, best)].bs) } else { None };
                    let (kt, kb) = (self.keys(t, fb), self.keys(best, fb));
                    (kt.safe_frontier_blue_score, kt.safe_weight, kt.live_total, self.gd(t))
                        > (kb.safe_frontier_blue_score, kb.safe_weight, kb.live_total, self.gd(best))
                }
            };
            if better {
                best = t;
                best_by_bw = bw;
            }
        }
        (best, best_by_bw)
    }

    // ---- production -------------------------------------------------------------------------------

    fn add_node(&mut self, name: &'static str, side: Side) -> usize {
        let mut known = vec![false; self.blocks.len()];
        known[0] = true;
        self.nodes.push(Node {
            name,
            side,
            known,
            tips: vec![0],
            sink: 0,
            fin: 0,
            hears_adversary: false,
            hears_all: true,
            online: true,
        });
        self.nodes.len() - 1
    }

    fn add_producer(&mut self, side: Side, home: usize, blue_per_tick: u64, bonds: u8) -> usize {
        self.producers.push(Producer {
            side,
            home,
            blue_per_tick,
            bonds,
            attempt_every: None,
            active: true,
            adversary: false,
            junk: 0,
            private_tip: 0,
            lead: 0,
        });
        self.producers.len() - 1
    }

    /// The bonds whose receipts for `c` reach producer `pr` now.
    fn signers(&self, pr: &Producer, c: &OpenClaim) -> u8 {
        if let Some((_, reach)) = self.pinned.iter().find(|(id, _)| *id == c.id) {
            let ok = match reach {
                Reach::Stuck(side) => !pr.adversary && pr.side == *side,
                Reach::PublicOnly => !pr.adversary,
                Reach::PrivateOnly => pr.adversary,
            };
            return if ok { c.seats } else { 0 };
        }
        if !self.licensing {
            return 0;
        }
        let honest_all = !self.adv_bonds;
        let reach = if pr.adversary {
            // An adversary hears every public receipt; its own seats sign whatever it carries.
            self.adv_bonds | if c.private { 0 } else { honest_all }
        } else if self.healed {
            honest_all
        } else {
            honest_all
                & match pr.side {
                    Side::Maj => self.maj_bonds,
                    Side::Min => self.min_bonds,
                    Side::Off => 0,
                }
        };
        c.seats & reach
    }

    /// One block on `parent` by producers `members` at the current tick.
    fn produce(&mut self, members: &[usize], parent: usize, pays: u8) -> usize {
        let lead = self.producers[members[0]].clone();
        let t = self.tick + lead.lead;
        let blue = members.iter().map(|m| self.producers[*m].blue_per_tick).sum::<u64>().min(BLUE_CAP);
        let pb = self.blocks[parent].clone();
        let mut st = (*pb.st).clone();
        let mut finals = (*st.finals).clone();
        let mut finals_changed = false;
        // 1. The sweep: licensed past the challenge window → Final; unlicensed past the deadline → Voided;
        //    on an honest chain, the court's void of the time bomb's claim.
        let (ch, deadline) = (self.p.challenge, self.p.licence_deadline);
        let voided = match self.void_public {
            Some((id, at)) if !lead.adversary && t >= at => Some(id),
            _ => None,
        };
        st.open.retain(|c| {
            if Some(c.id) == voided {
                return false;
            }
            match c.licensed {
                Some(l) if t > l + ch => {
                    finals.push((c.accepted_bs, PWU));
                    finals_changed = true;
                    false
                }
                None if t > c.accepted_daa + deadline => false,
                _ => true,
            }
        });
        if finals_changed {
            st.safe = finals.iter().map(|(_, w)| *w).sum();
        }
        // 2. Licence carriers: every claim whose quorum's receipts reach these producers.
        let delay = self.p.licence_delay;
        let licensed: Vec<usize> = st
            .open
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.licensed.is_none()
                    && t >= c.accepted_daa + delay
                    && members.iter().fold(0u8, |acc, m| acc | self.signers(&self.producers[*m], c)).count_ones() >= QUORUM
            })
            .map(|(i, _)| i)
            .collect();
        for i in licensed {
            st.open[i].licensed = Some(t);
        }
        // 3. Attempts: each active bond on its schedule makes a claim. An adversary grinds its private
        //    panels onto its own seats.
        let bs = pb.bs + blue;
        let mut last_attempt = pb.last_attempt;
        let mut attempts = 0u128;
        for m in members {
            let pr = self.producers[*m].clone();
            let every = pr.attempt_every.unwrap_or(self.p.attempt_every);
            for b in 0..BONDS {
                if pr.bonds & (1 << b) != 0 && (t + 3 * b as u64) % every == 0 {
                    attempts += 1;
                    last_attempt[b] = pb.height + 2;
                    let id = self.next_claim;
                    self.next_claim += 1;
                    let seats = if pr.adversary { seats_around(self.adv_bonds) } else { seats_of(id, self.seed) };
                    st.open.push(OpenClaim { id, accepted_bs: bs, accepted_daa: t, seats, licensed: None, private: pr.adversary });
                }
            }
        }
        // 4. The frontier: the deepest Final claim inside the resolved prefix; it never retreats.
        let resolved_through = st.open.iter().map(|c| c.accepted_bs).min().map(|m| m.saturating_sub(1)).unwrap_or(u64::MAX);
        if let Some(fr) = finals.iter().filter(|(a, _)| *a <= resolved_through).map(|(a, _)| *a).max()
            && fr > st.frontier
        {
            st.frontier = fr;
        }
        st.finals = if finals_changed { Rc::new(finals) } else { pb.st.finals.clone() };
        let junk = members.iter().map(|m| self.producers[*m].junk).sum::<u64>() as u128;
        let id = self.blocks.len();
        let mut up = [parent; 16];
        for k in 1..16 {
            up[k] = self.blocks[up[k - 1]].up[k - 1];
        }
        self.blocks.push(Blk {
            parent: Some(parent),
            up,
            height: pb.height + 1,
            daa: t,
            bs,
            bw: pb.bw + blue as u128 * BEAT_WORK + (attempts + junk) * ATTEMPT_WORK,
            hash: splitmix(self.seed ^ ((id as u64) << 20) ^ t),
            last_attempt,
            st: Rc::new(st),
            pays,
        });
        for n in self.nodes.iter_mut() {
            n.known.push(false);
        }
        id
    }

    /// A pinned claim accepted on `parent`'s chain in a new block, licensable from the next tick on.
    fn pin_claim(&mut self, producer: usize, parent: usize, seats: u8, reach: Reach) -> (usize, u32) {
        self.tick += 1;
        let b = self.produce(&[producer], parent, 0);
        let id = self.next_claim;
        self.next_claim += 1;
        let mut st = (*self.blocks[b].st).clone();
        let accepted_daa = (self.tick + 1).saturating_sub(self.p.licence_delay);
        st.open.insert(0, OpenClaim { id, accepted_bs: self.blocks[b].bs, accepted_daa, seats, licensed: None, private: false });
        self.blocks[b].st = Rc::new(st);
        self.pinned.push((id, reach));
        (b, id)
    }

    /// Mark `b` and its whole chain known to node `n` (a relayed block brings its missing ancestors as
    /// orphan roots).
    fn learn(&mut self, n: usize, b: usize) {
        if self.nodes[n].known[b] {
            return;
        }
        let mut x = b;
        loop {
            self.nodes[n].known[x] = true;
            match self.blocks[x].parent {
                Some(p) if !self.nodes[n].known[p] => x = p,
                Some(p) => {
                    self.nodes[n].tips.retain(|t| *t != p);
                    break;
                }
                None => break,
            }
        }
        self.nodes[n].tips.push(b);
    }

    /// Hand node `n` the branch ending at `tip`, if the relay passes it.
    fn relay(&mut self, n: usize, tip: usize) {
        if !self.rule.max_search() {
            let root = self.merge_root(self.nodes[n].sink);
            if self.blocks[tip].bw <= self.blocks[root].bw && !self.is_chain_ancestor(self.nodes[n].sink, tip) {
                return;
            }
        }
        self.learn(n, tip);
    }

    /// One tick: honest producers that share a parent make one block between them (their siblings
    /// merge inside a slot), the adversary extends its private tip, blocks are relayed, every node
    /// resolves.
    fn step(&mut self, pay_x: bool) {
        self.tick += 1;
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for pi in 0..self.producers.len() {
            let pr = &self.producers[pi];
            if !pr.active {
                continue;
            }
            if pr.adversary {
                groups.push((pr.private_tip, vec![pi]));
                continue;
            }
            let parent = self.nodes[pr.home].sink;
            // Producers that cannot hear each other never share a block, whatever parent they extend.
            let healed = self.healed;
            match groups
                .iter_mut()
                .find(|(p, m)| *p == parent && !self.producers[m[0]].adversary && (healed || self.producers[m[0]].side == pr.side))
            {
                Some((_, members)) => members.push(pi),
                None => groups.push((parent, vec![pi])),
            }
        }
        let mut made = Vec::new();
        for (parent, members) in groups {
            let adversary = self.producers[members[0]].adversary;
            let pays = u8::from(pay_x && !adversary && self.producers[members[0]].side == Side::Maj && self.merchant_block.is_none());
            let b = self.produce(&members, parent, pays);
            if pays == 1 {
                self.merchant_block = Some(b);
            }
            if adversary {
                self.producers[members[0]].private_tip = b;
            }
            let sides: Vec<Side> = members.iter().map(|m| self.producers[*m].side).collect();
            made.push((b, adversary, sides));
        }
        for (b, adversary, sides) in made {
            for n in 0..self.nodes.len() {
                let node = &self.nodes[n];
                if !node.online {
                    continue;
                }
                let hears = if adversary { node.hears_adversary } else { node.hears_all || sides.contains(&node.side) };
                if hears {
                    self.relay(n, b);
                }
            }
        }
        for n in 0..self.nodes.len() {
            self.resolve(n);
        }
    }

    fn sinks_on_one_chain(&self, nodes: &[usize]) -> bool {
        nodes.iter().all(|a| {
            nodes.iter().all(|b| {
                let (x, y) = (self.nodes[*a].sink, self.nodes[*b].sink);
                self.is_chain_ancestor(x, y) || self.is_chain_ancestor(y, x)
            })
        })
    }

    fn note_seal(&mut self, nodes: &[usize]) {
        if self.out.sealed_at.is_some() {
            return;
        }
        for a in nodes {
            for b in nodes {
                let (fa, sa, sb) = (self.nodes[*a].fin, self.nodes[*a].sink, self.nodes[*b].sink);
                if !self.is_chain_ancestor(fa, sb) && !self.is_chain_ancestor(sb, sa) {
                    self.out.sealed_at = Some(self.tick - self.fork_tick);
                    return;
                }
            }
        }
    }

    fn conflicting_finality(&self, nodes: &[usize]) -> bool {
        nodes.iter().any(|a| {
            nodes.iter().any(|b| {
                let (fa, fb) = (self.nodes[*a].fin, self.nodes[*b].fin);
                !self.is_chain_ancestor(fa, fb) && !self.is_chain_ancestor(fb, fa)
            })
        })
    }
}

// =====================================================================================================
// Honest partitions
// =====================================================================================================

/// What the two sides carry while partitioned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Econ {
    /// Nobody attempts (a heartbeat-only period): every key ties, participation is 0 on both sides.
    HeartbeatOnly,
    /// Every bond attempts on its own side, nothing is licensed in the run: the economic keys tie and
    /// participation is the sides' bond counts.
    AttemptsNoLicence,
    /// `AttemptsNoLicence`, and a pre-fork claim whose quorum sits on the MINORITY side and whose
    /// carrier never reaches the majority — devnet r1's portable licence (and its `Final` one challenge
    /// window later).
    PortableOnMinority,
    /// The same claim's quorum on the majority side.
    PortableOnMajority,
    /// Every bond attempts and every claim whose quorum's receipts reach a side is licensed there
    /// (after the heal: everywhere).
    FullLicensing,
}

#[derive(Clone, Copy, Debug)]
struct Split {
    /// Partition length in ticks.
    d: u64,
    econ: Econ,
    /// Bonds on the majority side (the rest sit on the minority side).
    maj_bonds: u8,
    /// Blue score a tick on each side (two producers racing on the majority, one on the minority).
    maj_blue: u64,
    min_blue: u64,
    /// Ticks run after the heal.
    after: u64,
}

const MAJ6: u8 = 0b0011_1111;
const MAJ5: u8 = 0b0001_1111;
const MAJ4: u8 = 0b0000_1111;

/// An honest partition and its heal, with the nodes the task names: one node per side, a fresh node
/// joining half-way through the split from a peer on either side, and a minority node that IBDs from a
/// majority peer (old datadir) after the heal; a restart of each side's node after the heal.
fn run_split(p: ModelParams, rule: Rule, sc: Split, seed: u64) -> Outcome {
    let mut w = World::new(p, rule, seed);
    let maj = w.add_node("majority node", Side::Maj);
    let min = w.add_node("minority node", Side::Min);
    let fresh_min = w.add_node("fresh node, first peer minority", Side::Off);
    let fresh_maj = w.add_node("fresh node, first peer majority", Side::Off);
    let ibd_min = w.add_node("minority node IBDing from the majority", Side::Min);
    w.licensing = sc.econ == Econ::FullLicensing;
    w.maj_bonds = sc.maj_bonds;
    w.min_bonds = !sc.maj_bonds;
    let attempting = sc.econ != Econ::HeartbeatOnly;
    let pm = w.add_producer(Side::Maj, maj, sc.maj_blue, if attempting { sc.maj_bonds } else { 0 });
    let pn = w.add_producer(Side::Min, min, sc.min_blue, if attempting { !sc.maj_bonds } else { 0 });
    // Shared history.
    for _ in 0..60 {
        w.step(false);
    }
    let pin = match sc.econ {
        Econ::PortableOnMinority => Some((!sc.maj_bonds, Side::Min)),
        Econ::PortableOnMajority => Some((sc.maj_bonds, Side::Maj)),
        _ => None,
    };
    if let Some((core, side)) = pin {
        let tip = w.nodes[maj].sink;
        let (b, _) = w.pin_claim(pm, tip, seats_around(core), Reach::Stuck(side));
        for n in 0..w.nodes.len() {
            w.learn(n, b);
            w.resolve(n);
        }
    }
    let fork = w.nodes[maj].sink;
    w.fork_tick = w.tick;
    for n in [fresh_min, fresh_maj] {
        w.nodes[n].online = false;
        w.nodes[n].hears_all = false;
    }
    // ---- the partition -------------------------------------------------------------------------
    w.healed = false;
    for n in [maj, min, ibd_min] {
        w.nodes[n].hears_all = false;
    }
    for t in 0..sc.d {
        w.step(false);
        if t == sc.d / 2 {
            // A node joins fresh: an IBD onto an empty datadir commits whatever its first peer serves.
            for (n, from) in [(fresh_min, min), (fresh_maj, maj)] {
                w.nodes[n].online = true;
                w.nodes[n].side = w.nodes[from].side; // it goes on hearing its first peer's side
                let tip = w.nodes[from].sink;
                w.learn(n, tip);
                w.nodes[n].sink = tip;
                w.nodes[n].fin = w.normal_finality_point(tip);
            }
        }
        w.note_seal(&[maj, min]);
    }
    let sealed_in_partition = w.out.sealed_at;
    assert!(w.is_chain_ancestor(fork, w.nodes[maj].sink) && w.is_chain_ancestor(fork, w.nodes[min].sink));
    let (tmaj, tmin) = (w.nodes[maj].sink, w.nodes[min].sink);
    // ---- the heal ------------------------------------------------------------------------------
    w.healed = true;
    let heal_tick = w.tick;
    for n in 0..w.nodes.len() {
        w.nodes[n].hears_all = true;
        if w.nodes[n].online {
            w.relay(n, tmaj);
            w.relay(n, tmin);
        }
    }
    for n in 0..w.nodes.len() {
        w.resolve(n);
    }
    let all = [maj, min, fresh_min, fresh_maj, ibd_min];
    let mut converged_since: Option<u64> = None;
    for k in 0..sc.after {
        w.step(false);
        if k == 10 {
            // A restart on each side: a node's state is its DAG and its sink, both on disk, and it
            // resolves to where it stood.
            for n in [maj, min] {
                let before = w.nodes[n].sink;
                w.resolve(n);
                assert_eq!(w.nodes[n].sink, before, "a restart keeps the node where it was");
            }
        }
        if k == 50 {
            let (inc, chal) = (w.nodes[ibd_min].sink, w.nodes[maj].sink);
            let commit = if inc == chal || w.is_chain_ancestor(inc, chal) {
                true
            } else if rule.participation() {
                let fork = w.common_ancestor(chal, inc);
                let crosses = !w.is_chain_ancestor(w.nodes[ibd_min].fin, chal);
                w.gate(chal, inc).0 && (!crosses || (rule == Rule::K && w.participation(chal, fork) >= SUPERMAJORITY))
            } else {
                palw_ibd_commit_strict_economic_v1(&w.keys(inc, None), &w.keys(chal, None)) == PalwIbdCommitV2::Commit
            };
            w.out
                .ibd
                .push(format!("old-datadir IBD of a minority node from the majority: {}", if commit { "commits" } else { "refused" }));
            if commit {
                w.learn(ibd_min, chal);
                w.nodes[ibd_min].sink = chal;
                w.nodes[ibd_min].fin = w.normal_finality_point(chal);
            }
        }
        w.note_seal(&all);
        if w.sinks_on_one_chain(&all) {
            converged_since.get_or_insert(w.tick);
        } else {
            converged_since = None;
        }
    }
    w.out.converged_at_end = w.sinks_on_one_chain(&all);
    w.out.converge_after = converged_since.map(|t| t - heal_tick);
    w.out.conflicting_finality = w.conflicting_finality(&all);
    for n in all {
        let s = w.nodes[n].sink;
        let on = |tip: usize| w.is_chain_ancestor(tip, s) || w.is_chain_ancestor(s, tip);
        let side = if on(tmaj) {
            "majority's"
        } else if on(tmin) {
            "minority's"
        } else {
            "neither side's"
        };
        w.out.notes.push(format!("{} ends on the {side} branch", w.nodes[n].name));
    }
    if let Some(t) = sealed_in_partition {
        w.out.notes.push(format!("sealed {t} ticks into the partition"));
    }
    w.out
}

// =====================================================================================================
// Adversaries
// =====================================================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attack {
    /// A private heartbeat branch, four blue beats a tick against the public two: every key ties.
    PrivateHeartbeats,
    /// The same with junk attempt headers (losing draws: blue work, no claim, no bond).
    PrivateJunkAttempts,
    /// Panel collusion: the adversary (three bonds, a quorum) attempts at six times an honest bond's
    /// rate on its private branch, grinds its panels onto its own seats and self-licenses, and carries
    /// a pre-fork claim whose quorum it holds — receipts it signs only on its own branch.
    PanelCollusion,
    /// DA-withholding time bomb, in a heartbeat-only period: the public chain licenses a pre-fork claim
    /// of the adversary's that the DA court voids later; the private branch lacks it and carries one
    /// colluded licence of its own — it ties before the void and wins after it.
    DaTimeBomb,
    /// Sybil peers: a node joining fresh is served the adversary's branch first (IBD and relay), an
    /// honest peer twenty ticks later.
    SybilFreshNode,
}

#[derive(Clone, Copy, Debug)]
struct AttackSc {
    attack: Attack,
    /// Ticks the private branch runs before it is released (the reorg's depth on the victim).
    release_after: u64,
    /// Ticks after the release.
    after: u64,
    /// For the time bomb: ticks from the fork to the court's void on the public chain.
    void_at: u64,
}

const ADV3: u8 = 0b1110_0000;

fn run_attack(p: ModelParams, rule: Rule, sc: AttackSc, seed: u64) -> Outcome {
    let mut w = World::new(p, rule, seed);
    let victim = w.add_node("victim", Side::Maj);
    let fresh = w.add_node("fresh node behind Sybil peers", Side::Off);
    w.nodes[fresh].online = false;
    w.nodes[fresh].hears_all = false;
    // A heartbeat miner (and the junk-header and Sybil adversaries) holds no bond; the colluder and the time
    // bomb's adversary hold three of the eight.
    let adv_bonds = if matches!(sc.attack, Attack::PanelCollusion | Attack::DaTimeBomb) { ADV3 } else { 0 };
    let honest_bonds = !adv_bonds;
    w.adv_bonds = adv_bonds;
    w.maj_bonds = 0xFF;
    w.licensing = sc.attack != Attack::DaTimeBomb;
    let quiet = sc.attack == Attack::DaTimeBomb;
    let public = w.add_producer(Side::Maj, victim, 2, if quiet { 0 } else { 0xFF });
    for _ in 0..60 {
        w.step(false);
    }
    if matches!(sc.attack, Attack::PanelCollusion | Attack::DaTimeBomb) {
        let tip = w.nodes[victim].sink;
        let (b, _) = w.pin_claim(public, tip, seats_around(adv_bonds), Reach::PrivateOnly);
        w.learn(victim, b);
        w.resolve(victim);
    }
    if sc.attack == Attack::DaTimeBomb {
        let tip = w.nodes[victim].sink;
        let (b, id) = w.pin_claim(public, tip, seats_around(adv_bonds), Reach::PublicOnly);
        w.learn(victim, b);
        w.resolve(victim);
        w.void_public = Some((id, w.tick + sc.void_at));
    }
    let fork = w.nodes[victim].sink;
    w.fork_tick = w.tick;
    // The adversary leaves the public chain with its bonds (an equivocating adversary would count on
    // both sides and cancel out of participation; this is its best case).
    w.producers[public].bonds = if quiet { 0 } else { honest_bonds };
    let adv = w.add_producer(Side::Maj, victim, 4, 0);
    w.producers[adv].adversary = true;
    w.producers[adv].private_tip = fork;
    w.producers[adv].lead = 1;
    match sc.attack {
        // Enough losing draws to out-work the public chain more than 4x above the fork (C2's override).
        Attack::PrivateJunkAttempts => w.producers[adv].junk = 128,
        Attack::PanelCollusion => {
            w.producers[adv].bonds = adv_bonds;
            w.producers[adv].attempt_every = Some(2);
        }
        _ => {}
    }
    // X rides the first public block above the fork.
    w.step(true);
    for _ in 1..sc.release_after {
        w.step(false);
    }
    // ---- the release --------------------------------------------------------------------------
    w.nodes[victim].hears_adversary = true;
    let tip = w.producers[adv].private_tip;
    w.relay(victim, tip);
    w.resolve(victim);
    let x = w.merchant_block.expect("X rides the public chain");
    assert_eq!(w.blocks[x].pays, 1, "the merchant's block carries X");
    if sc.attack == Attack::SybilFreshNode {
        w.nodes[fresh].online = true;
        w.nodes[fresh].side = Side::Maj;
        w.nodes[fresh].hears_adversary = true;
        w.learn(fresh, tip);
        w.nodes[fresh].sink = tip;
        w.nodes[fresh].fin = w.normal_finality_point(tip);
    }
    for k in 0..sc.after {
        if sc.attack == Attack::SybilFreshNode && k == 20 {
            w.nodes[fresh].hears_all = true;
            let pt = w.nodes[victim].sink;
            w.relay(fresh, pt);
        }
        w.step(false);
    }
    if sc.attack == Attack::SybilFreshNode {
        let on_public = w.is_chain_ancestor(x, w.nodes[fresh].sink);
        w.out.notes.push(format!("the Sybil-fed fresh node ends on the {} branch", if on_public { "honest" } else { "adversary's" }));
        if !on_public {
            w.out.payment_reversed_depth = Some(w.blocks[w.nodes[fresh].sink].daa - w.blocks[x].daa);
        }
    }
    let honest: Vec<usize> = (0..w.nodes.len()).filter(|n| w.nodes[*n].online).collect();
    w.out.converged_at_end = w.sinks_on_one_chain(&honest);
    w.out
}

// =====================================================================================================
// The grid and the table
// =====================================================================================================

fn verdict_split(o: &Outcome) -> String {
    let mut v = match (o.converged_at_end, o.converge_after) {
        (true, Some(t)) => format!("ok+{t}"),
        (true, None) => "ok".to_owned(),
        (false, _) if o.conflicting_finality => "SPLIT/sealed".to_owned(),
        (false, _) if o.sealed_at.is_some() => "SPLIT/seal1".to_owned(),
        (false, _) => "SPLIT/wedged".to_owned(),
    };
    if o.finality_held > 0 {
        v.push_str(",held");
    }
    if o.finality_crossings > 0 {
        v.push_str(",crossed");
    }
    v
}

fn verdict_attack(o: &Outcome) -> String {
    match o.payment_reversed_depth {
        Some(d) => format!("REV@{d}{}", if o.blue_work_decided_deep_reorg { "/bw" } else { "" }),
        None => "ok".to_owned(),
    }
}

/// Partition lengths: inside the shallow window, just past it, devnet r1's 20 ticks (41 min), half the
/// minority's seal, and past it (the minority makes 2 blue a tick, so it seals 300 ticks in).
fn split_grid(p: ModelParams) -> Vec<(String, Split)> {
    let seal = t_seal(&p, 2);
    let mut rows = Vec::new();
    for econ in [Econ::HeartbeatOnly, Econ::AttemptsNoLicence, Econ::PortableOnMinority, Econ::PortableOnMajority, Econ::FullLicensing]
    {
        for (label, maj_bonds, maj_blue) in [("5:3", MAJ5, 3u64), ("4:4", MAJ4, 2)] {
            if econ == Econ::HeartbeatOnly && maj_bonds == MAJ4 {
                continue;
            }
            let ds: Vec<u64> = match econ {
                Econ::HeartbeatOnly | Econ::AttemptsNoLicence => vec![2, 3, 20, seal / 2, seal + 20],
                _ => vec![20, seal / 2, seal + 20],
            };
            for d in ds {
                rows.push((format!("{econ:?} {label} D={d}"), Split { d, econ, maj_bonds, maj_blue, min_blue: 2, after: 1_500 }));
            }
        }
    }
    rows.push((
        format!("AttemptsNoLicence 6:2 D={}", seal + 20),
        Split { d: seal + 20, econ: Econ::AttemptsNoLicence, maj_bonds: MAJ6, maj_blue: 3, min_blue: 2, after: 1_500 },
    ));
    rows
}

fn attack_grid(p: ModelParams) -> Vec<(String, AttackSc)> {
    let seal = t_seal(&p, 2);
    let mut rows = Vec::new();
    for attack in
        [Attack::PrivateHeartbeats, Attack::PrivateJunkAttempts, Attack::PanelCollusion, Attack::DaTimeBomb, Attack::SybilFreshNode]
    {
        let depths: Vec<u64> = match attack {
            Attack::DaTimeBomb | Attack::SybilFreshNode => vec![40],
            // X has release-1 ticks of confirmation: 3 is the last inside the shallow window, 4 the first
            // past it; then deep, just under the victim's seal, just past it.
            _ => vec![3, 4, 40, seal - 5, seal + 5],
        };
        for d in depths {
            rows.push((format!("{attack:?} @{d}"), AttackSc { attack, release_after: d, after: 400, void_at: seal + 60 }));
        }
    }
    rows
}

fn table(p: ModelParams) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "\n### applied challenge {} — finality {} blue: T_seal {} ticks at 3 blue/tick ({:.1} h), {} at 2 ({:.1} h); earliest Final {} ticks after acceptance ({:.1} h)\n",
        p.challenge,
        p.finality_depth,
        t_seal(&p, 3),
        t_seal(&p, 3) as f64 / 30.0,
        t_seal(&p, 2),
        t_seal(&p, 2) as f64 / 30.0,
        p.licence_delay + p.challenge + 1,
        (p.licence_delay + p.challenge + 1) as f64 / 30.0
    );
    let header: Vec<&str> = Rule::ALL.iter().map(|r| r.name()).collect();
    let _ = writeln!(out, "| honest partition | {} |", header.join(" | "));
    let _ = writeln!(out, "|---|{}", "---|".repeat(header.len()));
    for (label, sc) in split_grid(p) {
        let cells: Vec<String> = Rule::ALL.iter().map(|r| verdict_split(&run_split(p, *r, sc, 7))).collect();
        let _ = writeln!(out, "| {label} | {} |", cells.join(" | "));
    }
    let _ = writeln!(out, "\n| adversary (release depth) | {} |", header.join(" | "));
    let _ = writeln!(out, "|---|{}", "---|".repeat(header.len()));
    for (label, sc) in attack_grid(p) {
        let cells: Vec<String> = Rule::ALL.iter().map(|r| verdict_attack(&run_attack(p, *r, sc, 11))).collect();
        let _ = writeln!(out, "| {label} | {} |", cells.join(" | "));
    }
    out
}

/// **The table** (both challenge windows), printed for the record.
#[test]
fn finx_p1_model_table() {
    for challenge in [120u64, 1_200] {
        eprintln!("{}", table(ModelParams::t12(challenge)));
    }
}

/// Devnet r1's shape under each rule, with every node's end and the IBD (for the record's narrative).
#[test]
fn finx_p1_model_r1_narrative() {
    for challenge in [120u64, 1_200] {
        let p = ModelParams::t12(challenge);
        let r1 = Split { d: 20, econ: Econ::PortableOnMinority, maj_bonds: MAJ5, maj_blue: 3, min_blue: 2, after: 1_500 };
        for rule in Rule::ALL {
            let o = run_split(p, rule, r1, 7);
            eprintln!("[r1, challenge {challenge}, {}] {} — {:?} {:?}", rule.name(), verdict_split(&o), o.ibd, o.notes);
        }
    }
}

// =====================================================================================================
// The properties, asserted
// =====================================================================================================

/// **P0's violations, reproduced in the model under the status quo.**
///
/// * V2 — a deep all-economic tie keeps the incumbent on BOTH sides: a heartbeat-only partition three
///   ticks long, nothing economic anywhere, never heals (two ticks heals: GHOSTDAG's shallow tie); the
///   same with every bond attempting and nothing licensed.
/// * V1 — the search is not the comparator's maximum: devnet r1's shape never heals, every node
///   weighing the same keys; the majority never weighs the minority.
/// * V3 — finality seals what the gate refuses: with the unshortened challenge window the earliest
///   `Final` (1,226 ticks) comes long after `T_seal` (200 ticks).
/// * V4 — the IBD of an old minority datadir from the majority is refused.
/// * V5 — arrival order: two nodes joining fresh during the split end on different branches.
#[test]
fn finx_p1_status_quo_violations() {
    let p = ModelParams::t12(1_200);
    let sc = |d, econ| Split { d, econ, maj_bonds: MAJ5, maj_blue: 3, min_blue: 2, after: 600 };
    assert!(run_split(p, Rule::Sq, sc(2, Econ::HeartbeatOnly), 7).converged_at_end, "a two-tick tie is GHOSTDAG's");
    for econ in [Econ::HeartbeatOnly, Econ::AttemptsNoLicence] {
        let o = run_split(p, Rule::Sq, sc(3, econ), 7);
        assert!(!o.converged_at_end, "V2: a three-tick {econ:?} partition never heals: {:?}", o.notes);
        assert!(o.sealed_at.is_some(), "and the wedge is sealed by the finality depth");
    }
    let o = run_split(p, Rule::Sq, sc(20, Econ::PortableOnMinority), 7);
    assert!(!o.converged_at_end, "V1: devnet r1's shape stays split: {:?}", o.notes);
    assert!(o.conflicting_finality, "V3: and both sides' finality seals it: {:?}", o.notes);
    assert!(o.ibd.iter().any(|s| s.contains("refused")), "V4: {:?}", o.ibd);
    assert!(o.notes.iter().any(|s| s == "fresh node, first peer minority ends on the minority's branch"), "V5: {:?}", o.notes);
    assert!(o.notes.iter().any(|s| s == "fresh node, first peer majority ends on the majority's branch"), "V5: {:?}", o.notes);
}

/// **E heals every honest partition shorter than the seal whose sides' participation differs, on the
/// side with more participating bonds, whatever the licences; keeps every private branch of a
/// minority-stake adversary out; and never lets blue work decide past the shallow window.** Its named
/// residuals: a partition whose sides tie on participation (a heartbeat-only period: zero on both) and
/// one longer than the seal.
#[test]
fn finx_p1_rule_e_properties() {
    for challenge in [120u64, 1_200] {
        let p = ModelParams::t12(challenge);
        let seal = t_seal(&p, 2);
        for econ in [Econ::AttemptsNoLicence, Econ::PortableOnMinority, Econ::PortableOnMajority, Econ::FullLicensing] {
            for d in [20u64, seal / 2] {
                let sc = Split { d, econ, maj_bonds: MAJ5, maj_blue: 3, min_blue: 2, after: 600 };
                let o = run_split(p, Rule::E, sc, 7);
                assert!(o.converged_at_end, "E heals {econ:?} D={d} (challenge {challenge}): {:?}", o.notes);
                assert!(o.notes.iter().all(|n| !n.contains("minority's")), "E converges on the bond majority: {:?}", o.notes);
                assert!(o.ibd.iter().all(|s| s.contains("commits")), "E's IBD commits the same answer: {:?}", o.ibd);
            }
        }
        for econ in [Econ::AttemptsNoLicence, Econ::FullLicensing] {
            for d in [20u64, seal / 2] {
                let sc = Split { d, econ, maj_bonds: MAJ4, maj_blue: 2, min_blue: 2, after: 600 };
                let o = run_split(p, Rule::E, sc, 7);
                assert!(o.converged_at_end, "E heals an even 4:4 split ({econ:?} D={d}, challenge {challenge}): {:?}", o.notes);
            }
        }
        let hb = Split { d: 20, econ: Econ::HeartbeatOnly, maj_bonds: MAJ5, maj_blue: 3, min_blue: 2, after: 600 };
        assert!(!run_split(p, Rule::E, hb, 7).converged_at_end, "E's residual: a heartbeat-only partition");
        let long = Split { d: seal + 20, econ: Econ::AttemptsNoLicence, maj_bonds: MAJ5, maj_blue: 3, min_blue: 2, after: 600 };
        assert!(!run_split(p, Rule::E, long, 7).converged_at_end, "E's residual: a partition longer than the seal");
        for (label, sc) in attack_grid(p) {
            if sc.attack == Attack::DaTimeBomb && challenge == 120 {
                continue; // the court's void comes after the claim's Final under the short window
            }
            let o = run_attack(p, Rule::E, sc, 11);
            assert!(o.payment_reversed_depth.is_none(), "E keeps X: {label} (challenge {challenge}) {:?}", o.notes);
            assert!(!o.blue_work_decided_deep_reorg);
        }
    }
}

/// **The private-branch adversaries against every rule** — the safety columns the record states.
#[test]
fn finx_p1_private_branch_safety() {
    for challenge in [120u64, 1_200] {
        let p = ModelParams::t12(challenge);
        let seal = t_seal(&p, 2);
        let sc = |attack, d| AttackSc { attack, release_after: d, after: 400, void_at: seal + 60 };
        let short = challenge == 120;
        for rule in Rule::ALL {
            let n = format!("{} (challenge {challenge})", rule.name());
            let rev = |attack, d| run_attack(p, rule, sc(attack, d), 11);
            // Inside the shallow window a reversal is GHOSTDAG's under every rule — the stated price.
            assert!(rev(Attack::PrivateHeartbeats, 3).payment_reversed_depth.is_none(), "{n}");
            // V6, the stale incumbent: a bondless heartbeat branch released just past the window reverses X
            // under every rule that weighs portable (pre-fork) claims — its tip crosses a licence of a
            // pre-fork claim one tick before the victim's previous sink does, a strict `live` win the public
            // chain's own next block would have tied. With the short challenge window the pre-fork claims'
            // Finals give the same opening 120+ ticks deep.
            let o = rev(Attack::PrivateHeartbeats, 4);
            assert_eq!(o.payment_reversed_depth.is_some(), !rule.fork_relative_keys(), "{n}: heartbeats at 4");
            assert!(!o.blue_work_decided_deep_reorg, "{n}: decided on the economic keys, not blue work");
            let o = rev(Attack::PrivateHeartbeats, 40);
            assert_eq!(o.payment_reversed_depth.is_some(), short && !rule.fork_relative_keys(), "{n}: heartbeats at 40");
            // Junk attempt headers past 4x the public blue work: C2's override reverses X by blue work alone.
            let o = rev(Attack::PrivateJunkAttempts, 40);
            assert_eq!(
                o.payment_reversed_depth.is_some(),
                rule == Rule::C2 || (short && !rule.fork_relative_keys()),
                "{n}: junk at 40"
            );
            assert_eq!(o.blue_work_decided_deep_reorg, rule == Rule::C2, "{n}: junk at 40 — blue work decided");
            // Past the seal nothing reorgs X (K's checkpoint needs a 2/3 participation a bondless branch lacks).
            assert!(rev(Attack::PrivateHeartbeats, seal + 5).payment_reversed_depth.is_none(), "{n}: past the seal");
            // Panel collusion (a quorum-holding, compute-rich adversary self-licensing on its branch): every
            // rule that ranks the economic keys above participation lets it reverse X inside the finality depth.
            let o = rev(Attack::PanelCollusion, 40);
            assert_eq!(o.payment_reversed_depth.is_some(), !rule.participation_first(), "{n}: panel collusion at 40");
            if !short {
                // The DA time bomb: finality stops it under the status quo (the court's void lands after the
                // seal); F, holding the finality point open on the heavier refused branch, lets it land.
                let o = rev(Attack::DaTimeBomb, 40);
                assert_eq!(o.payment_reversed_depth.is_some(), rule == Rule::F, "{n}: DA time bomb");
                // Sybil peers capture a fresh node under every rule whose search never weighs a lighter branch.
                let o = rev(Attack::SybilFreshNode, 40);
                assert_eq!(o.payment_reversed_depth.is_some(), !rule.max_search(), "{n}: Sybil fresh node — {:?}", o.notes);
            }
        }
    }
}
