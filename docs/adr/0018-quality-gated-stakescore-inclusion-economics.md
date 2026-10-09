# ADR-0018: Quality-Gated StakeScore + Inclusion Economics (BFT-free)

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted (design freeze; implementation gated/deferred)
Date: 2026-05-30
Amends:
  - [ADR-0009](0009-dns-probabilistic-finality.md) — StakeScore credit
    rule, the DNS health signal, and the two-dimensional reorg-dominance
    path (the mainnet replacement for today's `HardCheckpoint` placeholder).
Supersedes:
  - [ADR-0013](0013-validator-reward-distribution.md) §"flat
    per-attestation reward" — replaced by the proportional inclusion
    economics below. The coinbase fan-out *mechanism* (ADR-0009
    Addendum B / ADR-0013 Addendum B–C) is reused; the *amount* model
    changes.
Depends on:
  - [ADR-0017](0017-all-active-staker-attestation.md) — all-active-staker
    attestation. There is **no sortition and no per-epoch ticket**; the
    "ticket weight" of earlier drafts is replaced throughout by
    **active-stake weight** (see Context).
  - [ADR-0008](0008-hash64-consensus-identity.md), [ADR-0016](0016-stake-locked-bond-utxos.md)
    (bonds lock stake; `effective_bond_status` is the activity predicate).

## Context

Kaspa-PQ's PoS layer is **not** a BFT finality gadget. PoW/GHOSTDAG
remains the sole block-production and tip-selection consensus; the PoS
overlay contributes a second, independent **StakeScore** dimension, and
DNS-confirmation requires superiority in **both** WorkScore and
StakeScore. This ADR freezes the refined, explicitly **BFT-free**
economics and the StakeScore quality gate.

### Why this ADR (the gap in the current code)

The implemented StakeScore credit is **linear and ungated**
(`dns_finality::stake_score_increment`):

```text
epoch_credit_e = included_stake_e / expected_stake_e × STAKE_SCORE_SCALE
```

so even a small honest-or-adversarial included fraction earns a small
positive credit every epoch. Over a long enough window a **minority
stake** can therefore accumulate StakeScore and edge a chain toward
DNS-confirmation. The fix is a **quality floor** `φS`: below it an epoch
earns **zero** StakeScore. This is **not** a BFT 2/3 supermajority — it
is a quality filter set so that `φS >` the assumed adversarial
stake-inclusion rate.

Likewise the reward model is a **flat** `per_attestation_reward_sompi ×
count`, which (a) is not stake-weighted and (b) does not resist a Worker
that includes only a few attacker attestations. This ADR replaces it
with **stake-proportional inclusion economics** whose denominator is the
epoch's **expected** active stake, so a minority inclusion can never
capture the whole pool.

### Terminology (ticket → active-stake), and the renames

Earlier drafts spoke of `qS = 2/3 quorum`, "tickets", and "quorum
certificates". Under ADR-0017 there are no tickets and no quorum. This
ADR uses:

| earlier term | this ADR |
|---|---|
| quorum / `qS = 2/3` | **stake-event quality floor `φS`** |
| quorum-certified epoch | **quality-passed epoch** |
| quorum completion bonus | **quality-gate bonus** |
| `expected ticket weight` | **expected active stake** at the epoch anchor |
| `included ticket weight` | **included attestation stake** |
| BFT finality | DNS probabilistic finality |

The denominator (`expected active stake`) and numerator (`included
attestation stake`) are **already computed today** by
`total_active_stake_by_epoch` and `aggregate_epoch_tallies` — so the
quality gate is a small change to one function (§B).

## Decision

### §A — Explicitly NOT adopted (BFT-free invariant, binding)

A future PR that adds any of the following requires its own superseding
ADR; none may land as an implementation detail:

- BFT committee, validator-vote finality, or fork choice by votes.
- `2/3` vote certificate / quorum certificate / finality gadget.
- A mempool-availability rule such as "must include X% of attestations
  visible to this node". Mempool contents are not consensus state. The hard
  inclusion rule in §G is allowed only because it is deterministic from the
  selected-parent chain, the selected-parent active-bond view, and the
  candidate block body.

What the overlay *does* use: PoW/GHOSTDAG (production + tip + WorkScore),
PoS attestation **events** (StakeScore), DNS-confirmation =
`WorkDepth ≥ cW ∧ StakeDepth ≥ cS`, confirmed-history replacement = the
two-dimensional WorkScore×StakeScore dominance (§H), and a **health**
signal that degrades (rather than forging finality) under censorship (§C).

### §B — Quality-gated StakeScore (the core change)

Per epoch `e`, evaluated over the existing bounded selected-chain window:

```text
expected_stake_e        = total active-bond stake at the epoch's anchor   (= total_active_stake_by_epoch)
included_stake_e(H)     = Σ stake of bonds whose valid attestation for e is on-chain in H  (= aggregate_epoch_tallies)
included_fraction_e(H)  = included_stake_e(H) / expected_stake_e
```

The **quality floor** `φS` (`DnsParams::stake_event_quality_floor_bps`)
gates the credit with a **smooth** (not binary) curve, to keep it visibly
non-BFT:

```text
epoch_credit_e(H) =
    0,                                                              if included_fraction_e < φS
    STAKE_SCORE_SCALE × (included_fraction_e − φS) / (1 − φS),       otherwise
```

```rust
/// Replaces `stake_score_increment`. `quality_floor_bps` = φS in basis points.
pub fn epoch_stake_credit(included_stake: u128, expected_stake: u128, quality_floor_bps: u16) -> u128 {
    if expected_stake == 0 {
        return 0;
    }
    let included_bps = (included_stake.min(expected_stake)) * 10_000 / expected_stake;
    let floor = quality_floor_bps as u128;
    if included_bps < floor {
        return 0;
    }
    (included_bps - floor) * STAKE_SCORE_SCALE / (10_000 - floor)
}
```

`StakeScore(H) = Σ_e epoch_stake_credit_e(H)` (the existing
`compute_stake_score` sum, swapping the increment). **`φS` is a quality
gate, not a supermajority**: pick `φS >` the modelled adversarial
stake-inclusion rate `αS_eff` (e.g. `φS ∈ [0.50, 0.60]` for
`αS_eff ≤ 0.33–0.40`; up to `0.67` only with little margin). Minority
inclusion ⇒ zero StakeScore.

### §C — DNS health / degraded mode (non-blocking signal)

A read-only **health** signal, orthogonal to the existing
`DnsRolloutStage` lifecycle (`Launch / Bootstrap / Active`):

```rust
pub enum DnsHealth {
    Active,                       // StakeScore advancing normally
    DegradedStakeQualityLow,      // included_fraction_e < φS for ≥ M epochs
    DegradedCertificateCensored,  // included_stake ≪ expected (Worker censorship signature)
    DisabledBeforeActivation,     // overlay not yet Active
}
```

Binding property: **the health signal itself never decides block validity.**
In the shipped liveness-first presets, degraded health only stops
`dns_confirmed_anchor` from advancing while PoW/GHOSTDAG and normal
transactions continue. Private/research deployments can still enable the
optional §G hard-inclusion fence, but that explicitly trades base-ledger
liveness for a consensus-level anti-censorship experiment. Note: there is no
`DegradedRandomnessStalled` state — that belonged to the commit-reveal
sortition removed by ADR-0017.

### §D — Worker inclusion bounty (proportional, anti-capture)

The Worker that includes attestation shards is paid for **quality
contribution**, not for the act of inclusion. The denominator is the
epoch's **expected** stake, so including a few attestations cannot drain
the pool (the rejected `pool / included_count` design).

```text
unit_bounty            = worker_inclusion_pool_e / expected_stake_e
worker_inclusion_bounty = unit_bounty × newly_included_valid_stake × urgency_multiplier
                          + quality_gate_bonus          (only if this block crosses included_fraction from < φS to ≥ φS)
```

```rust
pub fn worker_inclusion_bounty(
    pool: u128, newly_included_stake: u128, expected_stake: u128,
    urgency_multiplier_scaled: u128, crossed_quality_floor: bool, quality_gate_bonus: u128,
) -> u128 {
    if expected_stake == 0 { return 0; }
    let base = pool * newly_included_stake / expected_stake;
    let urgent = base * urgency_multiplier_scaled / STAKE_SCORE_SCALE;
    if crossed_quality_floor { urgent + quality_gate_bonus } else { urgent }
}
```

Eligible `newly_included_stake`: valid signature, valid bond, correct
epoch + target, **first** on-chain inclusion, within the reward window;
**ineligible**: duplicate / invalid / expired / wrong-target / already
included. The **unspent** remainder of the pool goes to a
`SecurityRollover` (or the next epoch's inclusion pool) — never
redistributed to the included few. The **quality-gate bonus** (renamed
from "quorum completion bonus") rewards pushing an epoch from zero
StakeScore credit to positive; it is an economic incentive, **not** a
certificate.

### §E — Validator reward (proportional, two pools)

The per-epoch validator pool splits into a participation pool and a
quality-bonus pool (`DnsParams`: `validator_participation_bps = 7000`,
`validator_quality_bonus_bps = 3000`), both proportional with
`expected_stake_e` as denominator:

```text
unit_participation = validator_participation_pool_e / expected_stake_e
reward_i           = unit_participation × included_valid_stake_i

quality_bonus_i    = (validator_quality_bonus_pool_e × included_valid_stake_i / expected_stake_e)
                     if included_fraction_e ≥ φS else 0
```

Unspent → rollover. Minority inclusion earns only its proportional slice,
never the whole pool.

### §F — Fee split (Worker / Validator / Service)

Three independent splits (`DnsParams`/`RewardParams`, bps; gated like all
overlay economics):

- **Block subsidy (DNS Active):** Worker `70%` (base `62%` + inclusion
  `8%`), Validator (finality) `25%`, Service reserve `5%`.
- **Normal-tx fees (permanent validator share so the layer outlives the
  subsidy):** Worker `85%` / Validator `10%` / Service `5%` for years
  0–10, ramping to `80% / 15% / 5%` for years 10–20. (A fixed
  `85/10/5` is acceptable; the ramp is recommended.)
- **DNS-finality fee:** Validator `60%` / Worker `25%` / Service `15%`
  (validators directly provide finality; Workers still carry inclusion).
  Live params: Validator `75%` / Worker `25%` / Service `0%`
  (`finality_fee_*_bps`; the validator takes the dust-free remainder).

**Finality-fee class — WIRED (bridge txs).** The finality-fee class is an
on-chain fee type: an accepted L1 tx that **creates ≥1 `EVM_DEPOSIT_LOCK`
output** (ADR-0020 §9.2 — the bridge deposit, the L1 action whose value most
depends on the validators' `finalized` head) has its whole fee classified as
finality-class. Classification happens at fee accumulation in
`calculate_utxo_state` (the single site shared by coinbase construction AND
validation ⇒ c==v structurally), recognised by the same
`parse_evm_deposit_lock` check the claim path uses (so "finality-class" and
"claimable lock" can never diverge), and recorded as
`BlockRewardData::finality_fees` (a subset of `total_fees`; the normal-class
part is `total − finality`, derived inside `split_block_reward` so callers
cannot mis-pair the two). DOUBLY gated on
`DnsParams::finality_fee_activation_daa_score` (`0` on every preset) AND the
net's `evm_activation_daa_score` — lock outputs are consensus-legal on every
net, but the bridge only exists on an EVM-active net, so EVM-inert nets
(mainnet/simnet today) are enforced-inert: below either fence the field stays
0 and every split is byte-identical to the pre-wiring math. Note the
deliberate incentive: a tx carrying a lock output pays its miner 25% instead
of 90% of the fee — a sender attaching a dust lock to an ordinary tx only
shifts its own fee from the miner to the §E pool (pure self-griefing of
inclusion priority; mempool ranks by total fee). The withdraw side is NOT
classified:
a withdraw's synthetic UTXO carries a user-chosen script (no recognizable
marker on a later spend), and the F002 cost is paid in EVM gas — the
deposit-lock tx is the L1-side bridge action. e2e:
`finality_fee_bridge_tx_pays_validator_primary_split`.

**Bridge execution freshness.** Deposit-claim credits and F002
EVM->UTXO withdrawals are finality-dependent bridge effects, so local
RPC/producers require a fresh DNS-confirmed anchor even though normal
PoW/GHOSTDAG liveness does not. The shared local-policy guard is:

`dns_confirmed == true
 && last_dns_confirmed_anchor != default
 && current_daa >= last_dns_confirmed_anchor_daa_score
 && current_daa - last_dns_confirmed_anchor_daa_score
      <= DnsParams::bridge_finality_max_staleness_daa_score`

If this fails, RPC deposit-claim submission is rejected and local templates emit
an empty EVM payload. That local producer policy pauses the whole EVM lane for
the template: deposit-claim system ops, normal EVM transactions, and the EVM
coinbase are all omitted. Base L1 transactions and PoW/GHOSTDAG progress remain
valid. Block validation deliberately does not read the node-local DNS state to
reject such blocks; making bridge freshness a consensus rule requires a
deterministic selected-parent freshness view in a separate fork.

### §G — Attestation lane (liveness-first by default)

Block mass is two budgets: `max_normal_block_mass` and
`max_attestation_shard_mass_per_block` (with `max_attestations_per_block`,
both already `DnsParams` fields).

Shipped presets set `DnsParams::mandatory_attestation_inclusion_daa_score =
u64::MAX`, so the lane is economic/finality-only (§D): an empty lane is
valid, and included attestations must only pass the existing
active-bond/signature eligibility checks. Below-floor participation yields
zero quality credit / degraded DNS health, but it does not make the base
PoW/GHOSTDAG block invalid.

Private or research deployments may lower
`mandatory_attestation_inclusion_daa_score` to exercise hard inclusion.
At/after that fence, a candidate block is valid only if the selected-parent
chain plus the candidate block body reaches `stake_event_quality_floor_bps`
for the **oldest ready canonical epoch** that is still under-certified,
provided that epoch has an active validator set meeting
`min_active_validators` and `min_active_stake_sompi`. The rule is
deliberately selected-parent based: it never asks whether some attestation
was seen in a local mempool. Once an epoch reaches `φS`, later blocks do not
need to include it again; if several ready epochs are below floor, the
backlog is cleared oldest-first, one mandatory epoch per block.

### §H — Two-dimensional reorg dominance (mainnet path)

Replace today's `DnsReorgMode::HardCheckpoint` placeholder (reject any
reorg abandoning a confirmed anchor) with the ADR-0009 mainnet rule:
replacing confirmed history requires the candidate chain to **dominate in
both dimensions** — strictly greater accumulated WorkScore **and**
StakeScore since the common ancestor. Neither dimension alone suffices;
this is the non-BFT analogue of "you cannot rewrite finalized history
without out-working and out-staking it".

### §I — Parameters + gating

New `DnsParams`/`RewardParams` fields (placeholders; calibrated pre-mainnet
like the existing reward placeholders): `stake_event_quality_floor_bps`
(φS), `degraded_stake_quality_epochs` (M), `mandatory_attestation_inclusion_daa_score`,
`validator_participation_bps` / `validator_quality_bonus_bps`, the three fee
splits, the worker inclusion pool + `quality_gate_bonus` + urgency params.
Everything remains inert below `dns_activation_daa_score`. Production,
testnet, devnet, and simnet presets keep
`mandatory_attestation_inclusion_daa_score = u64::MAX` so miners can advance
the base ledger while DNS finality degrades. Private/research presets can set
the fence to `0` only when they intentionally accept the hard-liveness
trade-off.

### §J — Reused primitives (incremental implementation)

Already present and reused as-is: `total_active_stake_by_epoch`
(expected), `aggregate_epoch_tallies` (included), the bounded-window
attestation walk + `attestations_from_accepted_txs`, the coinbase fan-out
mechanism (ADR-0009 Addendum B), `is_dns_confirmed` (two-threshold
confirmation), `DnsRolloutStage`. Changed: `stake_score_increment` →
`epoch_stake_credit` (§B); the flat reward → §D/§E; `HardCheckpoint` →
§H; add the `DnsHealth` derived signal (§C).

## Consequences

### Positive
- **Closes the minority-stake StakeScore-accumulation attack** (the φS
  floor zeroes sub-threshold epochs).
- **Censorship cannot fake finality.** With shipped liveness-first presets,
  censorship degrades `DnsHealth`, yields no quality credit, and stalls
  `dns_confirmed` without stopping the base ledger. Optional private
  hard-inclusion deployments can still make a below-`φS` oldest ready epoch
  block-invalid when they intentionally accept that liveness trade-off.
- **Anti-capture economics.** Expected-stake denominators + rollover mean
  a few included attestations never drain a pool.
- **Sustainable validator layer.** A permanent normal-tx-fee share keeps
  validators funded after the subsidy decays.
- **Still no BFT.** No quorum certificates, vote-based fork choice, or
  validator finality gadget; PoW/GHOSTDAG remains the block-production and
  tip-selection base.

### Negative / open
- **Hard-inclusion liveness trade-off.** At/after the fence, if enough active
  validators do not produce eligible signatures for a ready epoch, or the
  network cannot deliver them into blocks, the selected-parent chain stops
  instead of silently advancing a censorship branch.
- **`φS` calibration is security-critical** (must exceed `αS_eff` with
  margin) — a follow-up calibration note before mainnet.
- **Consensus-critical surface.** §B/§G/§H change StakeScore, block validity, and
  reorg-dominance — chain-split-class; implement spec-first, per-block
  deterministic, gated, with the same rigor as the ADR-0009 Addendum B
  bond-view work. The fee-split (§F) interacts with the coinbase
  construction==validation chokepoint.

## Implementation plan (gated slices, spec-first)

1. **φS quality gate** — `epoch_stake_credit` replaces
   `stake_score_increment`; `stake_event_quality_floor_bps` on `DnsParams`;
   pure-fn + property tests (sub-floor → 0, smooth above). Smallest slice,
   closes the attack. Gated/inert.
2. **DnsHealth signal** — derive `DnsHealth` from the window tallies;
   surface via `getDnsConfirmation`. Read-only, non-blocking.
3. **Inclusion economics** — §D worker bounty + §E validator two-pool
   reward, replacing the flat fan-out value (reuse the fan-out mechanism);
   construction==validation byte-identity preserved.
4. **Fee split** — §F worker/validator/service splits into the coinbase.
5. **Two-dimensional dominance** — §H replaces `HardCheckpoint`
   (needs per-chain Work/Stake-since-common-ancestor).

Each slice: gated below activation, pure-core + tested, no current-net
change.
