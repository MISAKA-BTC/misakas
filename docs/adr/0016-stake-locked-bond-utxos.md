# ADR-0016: Stake-locked bond UTXOs

Status: Accepted (design; implementation deferred to the Phase 10/11 PR series)
Date: 2026-05-29
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md) (the P2PKH-ML-DSA
            script the bond output and its eventual unbond-spend
            use), [ADR-0009](0009-dns-probabilistic-finality.md)
            §"Stake bonds" + Addendum A.1/A.4 (the bond outpoint
            convention + the selected-chain bond lifecycle),
            [ADR-0013](0013-validator-reward-distribution.md)
            Addendum C (the slashing distribution that consumes the
            locked UTXO). Consumed by the per-block active-bond view
            of [ADR-0009 Addendum B](0009-dns-probabilistic-finality.md#addendum-b--per-block-active-bond-view--reward-eligibility-binding).

## Context

The DNS Probabilistic Finality Overlay (ADR-0009) weights every
consensus-relevant decision — `StakeScore`, committee eligibility,
sortition (ADR-0012), reward APY (ADR-0013) — by a validator's
**bonded stake**. ADR-0009 and its addenda model a bond as a
`StakeBondPayload` whose `amount` field declares how much the
validator has staked, recorded in the `StakeBonds` store.

Implementing the slashing distribution (ADR-0013 Addendum C)
surfaced that **`amount` is not backed by locked funds**. Today:

- `validate_stake_bond_payload` checks only the payload version, a
  non-zero `amount`, and the 1952-byte validator-key length.
- `bond_mutations_from_accepted_txs` records `amount` straight from
  the payload.
- The `StakeBond` transaction's output-0 (the bond outpoint, A.1) is
  an **ordinary spendable UTXO** whose value is unrelated to
  `amount`.

So `amount` is a **self-declared number**. Two consequences make this
a security hole rather than a missing nicety:

1. **Fake stake.** A validator can declare an arbitrarily large
   `amount` while locking nothing, inflating its `StakeScore`,
   committee-selection probability, and reward share — a costless
   Sybil/grinding vector against the entire PoS weighting.
2. **Nothing to slash.** ADR-0013's equivocation/unreveal penalties
   are defined as "burn the slashed amount `S`," but there is no
   locked UTXO holding `S`; marking the bond `Slashed` stops it
   *earning* but does not destroy any value, so the deterrent is
   hollow.

The overlay is dormant (gated) on every network, so this is latent —
but it must be closed before any network activates the overlay.

## Decision

A bond's `amount` must be **locked in a real UTXO** that the owner
cannot freely reclaim and that slashing can consume. We do this
**without a bespoke script** — the lock is a **consensus spend-gate
keyed on the bond outpoint**, leveraging the per-block active-bond
view (ADR-0009 Addendum B) that already exists.

### D.1 Bond output value rule (acceptance-time)

A `StakeBond` transaction is **invalid** unless its output-0 (the
bond outpoint, A.1):

- has `value == payload.amount`, and
- pays to a standard ML-DSA-65 P2PKH script (ADR-0002) — the
  **owner's** address (`script_public_key == p2pkh_mldsa65_spk(
  owner_reward_spk_payload)`, reusing the Addendum B payload the
  bond already declares).

This is a **stateless** check (it needs only the transaction and its
payload), so it lands in the PR-10.4 isolation validator alongside
the existing payload checks. It makes `amount` a genuine quantity of
the owner's coins, parked at output-0.

### D.2 Bond-UTXO spend-gate (stateful, consensus rule)

The bond's locked stake is enforced by a **consensus rule on
spending**, not by the output script (which is a normal P2PKH so the
owner ultimately controls it):

> A transaction input that spends an outpoint which is a **known
> bond outpoint** (present in the block's selected-parent active-bond
> view) whose bond is **not releasable** is **invalid**.

"Releasable" = the bond is in `Unbonding` **and** the spending
block's DAA score `≥ bond_release_daa_score(record)` (=
`unbond_request_daa_score + unbonding_period_blocks`, the helper that
already exists). A `Pending`/`Active` bond, or an `Unbonding` bond
before its release height, **cannot** have its stake spent.

This is a **per-block-deterministic** rule (it reads the same
`ActiveBondView` the reward fan-out and slashing rules use, resolved
against the block's selected parent — never the virtual-commit-time
global store), so it composes with the existing overlay validation
and is reorg-safe by construction. It is **inert** unless the overlay
is configured **and** the block is past `dns_activation_daa_score`
(`u64::MAX` on every current network).

### D.3 Unbond → release flow

Unbonding is the existing overlay lifecycle:

1. The owner submits an **unbond-request** (the transition that sets
   `unbond_request_daa_score`; `Active → Unbonding` in
   `effective_bond_status`).
2. After `unbonding_period_blocks`, the bond is **releasable** and
   D.2 permits the owner's normal P2PKH spend of output-0 — the
   validator reclaims the stake.

`unbonding_period_blocks ≥ max_reorg_horizon_blocks +
evidence_window_blocks` (ADR-0009 §"Long-range bound") guarantees the
stake stays locked long enough for any equivocation within the
attested range to be slashable **before** release.

### D.4 Slash → consume

When a genuine `SlashingEvidence`/`UnrevealSlashingEvidence` is
accepted (ADR-0013 Addendum C, as amended by **Addendum C.2**),
consensus consumes the bond's output-0 UTXO as a side-effect: `S =
amount` leaves the supply, the reporter-reward UTXO (`R`) is minted in
the **same atomic per-bond side-effect** at outpoint
`(slashing_tx_id, 0)` — *not* as an output on the slashing transaction
— and the remainder `S − R` burns implicitly (Addendum C.2). Pairing
the mint with the removal makes the net supply change exactly `R − S`
per slashed bond regardless of how many duplicate reports a block
merges. Because D.2 forbids the owner from spending a non-releasable
bond, and `unbonding_period` outlasts the evidence window (D.3), a
misbehaving validator can never front-run a slash by unbonding first.

### D.5 Interaction with `StakeScore` / eligibility

`StakeScore`, committee membership, sortition weight, and reward
share continue to read `amount` from the bond record — but `amount`
is now provably locked (D.1) and unspendable while the bond is
`Active` (D.2), so the weighting is backed by real, at-risk capital.
No change to those formulas; only their integrity improves.

## Consequences

### Positive

- **Real PoS security.** Stake weight is backed by locked,
  at-risk coins; the fake-stake Sybil/grinding vector is closed.
- **Slashing has teeth.** There is a concrete UTXO holding `S` for
  Addendum C to consume; the deterrent is real.
- **No bespoke script / no new opcode.** The lock is a consensus
  spend-gate over a standard ADR-0002 P2PKH output, reusing the
  per-block active-bond view — no `txscript` change, no new address
  type.
- **Reorg-safe + deterministic.** The gate reads the block's own
  selected-parent bond view (ADR-0009 Addendum B), so it never
  depends on point-of-view-mutable global state.

### Negative

- **A new stateful tx-input rule.** Spending validation must, for
  overlay-active blocks, consult the active-bond view for each input
  outpoint (a bounded set — active validators). Inert + zero-cost
  while the overlay is dormant.
- **Bond creation costs real coins.** Operators must fund `amount`
  at bond time (intended — that is the stake).

### Neutral

- **Pre-activation wire/rule change.** No `StakeBond` tx exists on
  any live network, so D.1–D.4 are added before activation with no
  migration.

## Implementation slots

| PR | Title | Gated? |
|---|---|---|
| 16.1 | This ADR | this doc |
| 16.2 | D.1 bond-output value+script rule in the PR-10.4 isolation validator (needs the tx, not just the payload); reject `StakeBond` unless output-0 `value == amount` and pays the owner P2PKH. | yes (overlay-configured) |
| 16.3 | D.2 bond-UTXO spend-gate: reject a tx input spending a non-releasable bond outpoint, resolved against the block's selected-parent active-bond view; reorg-safe. | yes (activation) |
| 16.4 | (then) ADR-0013 **10.12-b2** can land: the slashing side-effect consumes the now-real locked UTXO (D.4) + emits the reporter output. | yes (activation) |

16.2–16.4 only become live once a network sets a finite
`dns_activation_daa_score`; until then they are dead code paths
behind the `dns_params` + activation guard.

## References

- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md) (the locked
  output's script).
- [ADR-0009 — DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md)
  §"Stake bonds", Addendum A.1 (bond outpoint), A.4 (lifecycle),
  [Addendum B](0009-dns-probabilistic-finality.md#addendum-b--per-block-active-bond-view--reward-eligibility-binding)
  (the per-block active-bond view the spend-gate reads).
- [ADR-0013 — Validator Reward Distribution](0013-validator-reward-distribution.md)
  Addendum C (the slashing distribution that consumes the locked UTXO).
