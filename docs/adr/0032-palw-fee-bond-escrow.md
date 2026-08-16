# ADR-0032: PALW fee-bond escrow — pricing calls and paying challengers without new covenants

Status: **Accepted (design; activates nothing).** This decides HOW opening-call fees and
challenger bounties work — the piece ADR-0029 §7 deferred "needs the bond-UTXO covenant
discipline". The decision is that Stage 1 needs **no new covenant machinery at all**, and
Stage 2's escrow reuses the one covenant discipline the chain already enforces: the
consensus-recognized bond-UTXO spend gate.
Date: 2026-08-16
Relates to: ADR-0028 §4/§5 (the economics being carried: `(1+q·ρ_v)·base` issuance split,
fee-bonded audit calls, no-show slash floor = fee×100, challenger economics rivalrous by
construction), ADR-0027 §4 (slash allocation, challenger bounty ≤ 10 %, `slash_id`
idempotence), ADR-0016/0017 (stake-locked bond UTXOs — the existing spend-gate discipline),
ADR-0029 §2 (the no-outputs rule reserving the `(tx_id, 0)` reporter slot).

## Premises

* **No general covenants exist and none are being invented.** This chain's only
  outcome-conditioned spending is the bond spend gate (consensus rules that refuse to spend
  a recognized bond UTXO outside its lifecycle). Any escrow that needs "spendable only on
  outcome X" must be THAT mechanism, not a new script capability.
* **A fee's job is to price denial-of-service, not to fund the system.** ADR-0028 already
  decided verification funding is an issuance split; the call fee only has to make spamming
  opening calls cost more than answering them costs the answerer.
* **A bounty's job is promptness, not security.** Security is the permissionless window
  (ADR-0028 §4); the bounty rewards whoever moved first, capped so slashing never becomes a
  profit center that invites manufactured offenses (v0.1's ≤ 10 %).

## Decision

### Phase E1 (Stage 1) — fees are fees, bounties are consensus credits

1. **Opening-call fee = the transaction fee itself, made mandatory.** A Stage-1 opening-call
   transaction must carry `tx_fee ≥ F_call` (a registered network fact, placeholder until the
   economic simulation gate — ADR-0028's discipline for every such number). It is not
   refundable and not escrowed: the DoS pricing is achieved the moment the caller burned it
   to ordinary fee processing, with zero new state. An answered call costs the answerer one
   replay; an unanswered call within `W_answer` is the miner's `DATA_WITHHOLDING` — the fee
   does not need to move for either outcome to hold.
2. **Challenger bounty = a consensus credit at slash execution.** When a refutation's slash
   executes (Stage 2+), the slash transaction credits `min(10 % · slashed, B_cap)` to the
   `(tx_id, 0)` slot of the refutation-carrying transaction — the slot ADR-0029 §2's
   no-outputs rule reserved so this would never be a retrofit. The remainder burns. Dedup is
   `slash_id` idempotence (ADR-0027 §4): one offense, one bounty, first-accepted refutation
   wins — rivalrous by construction, exactly ADR-0028 §4's challenger economics.
3. **No-show slash floor** stays fee-denominated (`≥ 100 × F_call`, ADR-0028 §4's placeholder)
   so griefing-by-silence has negative ROI at any fee level.

Phase E1 requires: the fee minimum in the Stage-1 opening-call admission validator, and
nothing else. No escrow store, no covenant, no refund path.

### Phase E2 (Stage 2+) — the audit-call bond, as a bond

ADR-0028 §5's fee-BONDED audit call (the DA heartbeat) needs more than a burned fee: the
caller must stake something forfeitable if the audit is abusive, refundable if the answer is
late. That is a **bond lifecycle**, so it uses the bond machinery:

* An **audit-call bond UTXO** — the ADR-0016 pattern with a new recognized bond class
  (`AUDIT_CALL`), minimum value `F_audit`, and a spend gate keyed to the call's outcome
  window: spendable by the caller after `W_answer + settlement` if the answer never came
  (the miner's offense stands and the bond returns), spendable INTO the slash flow (burn +
  answerer compensation) if the call was answered and the caller abandoned it, unspendable
  before resolution. The gate reads the same Stage-1 carriage store the duty logic reads —
  the store is the oracle, the spend gate is the covenant, both already exist as disciplines.
* **What is deliberately refused**: escrow inside payloads (value must be UTXO-visible or
  the mass/UTXO accounting lies), third-party escrow agents (a trusted party in a BFT-free
  design), and per-outcome script predicates (a new covenant language for one use case).

### Numbers

`F_call`, `F_audit`, `B_cap`, and the no-show multiplier are **economic-simulation-gated**
(ADR-0028's rule: shipping placeholders as measured is a §15-class violation). This ADR
fixes the mechanisms and the flow of value; B15's simulation fixes the values.

## Consequences

* Stage-1 carriage needs exactly one new admission rule (the fee minimum); the reporter slot
  and dedup discipline it depends on already exist. B10's "未設計" is closed as design.
* The Stage-2 audit bond adds a bond class, not a covenant system — implementation rides the
  same rails as every bond change (ADR-0016 lineage), with its own drills.
* Bounty value flows are auditable on-chain by construction: burned remainder, credited slot,
  `slash_id` — no off-chain settlement anywhere.
