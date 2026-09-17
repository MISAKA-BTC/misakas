# ADR-0134 — The compute overlay's committee beacon retires at a height, and its machinery goes

* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`. testnet-11
  schedules the retirement at DAA 6,201 — its own height, not the 6,001 flag day's (§4). The stateful machinery
  is deleted in the same change (§3), on the evidence of §2.
* Operator's direction, in the operator's words: "DNS/VLT の committee beacon の使用されてない経路 コードの削除も追加で
  完了して — これはバリデータに依存してる — PALW はバリデーターを巻き込まないように進めて — 現在の PoW を LLM に置き換える
  方針からずれているため使用しない"; and, on the order of work: "DNS/VLT/beacon 削除も今回と混ぜない … 同じ activation
  fence には入れない方がデバッグしやすい".
* Builds on: [0024](0024-verified-llm-token-weighted-bft.md) (the overlay this retires), [0126](0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md)
  (what ADR-0126 already removed and what it kept "because testnet-10 and testnet-11 run its shadow"),
  [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) /
  [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md)
  (the fences this one is kept apart from), the memo *Codex's beacon/VLT cleanup is a silent fork* (why a deletion
  needs a height and evidence).

## 0. The sentence this ADR is

**The VLT compute overlay — a verifier committee drawn by a DNS-epoch beacon, a credit walk, an audit fee and a
challenge slash, all of it validator-dependent — has never been reached by a transaction on any shipped chain;
its five subnetworks are refused past a fenced height and accepted-and-ignored below it, and the machinery that
would have read them is deleted now, because no block a node will ever validate contains one.**

## 1. What the overlay was, and what PALW never used

ADR-0024's v0.1: an executor commits a job (`ComputeCommitment`), the DNS epoch *after* the commitment's anchors a
sortition beacon (`commitment_beacon_epoch`), a committee is drawn from validators' capability declarations
(`select_verifiers`, `capability_candidate_pool`), the certificate (`ComputeCertificate`) collects the committee's
verdicts (`ComputeVerdict`, `verdicts_for_certificate`), a credit walk (`walk_compute_overlay`,
`resolve_certificate`) resolves it, an audit fee pays the verifiers from the validator pool
(`compute_audit_fee_outputs`), and a fraud proof (`ComputeChallenge`) slashes a bond after adjudication
(`compute_challenge_adjudication_slashes`, `adjudicate_compute_challenge`, `check_compute_challenge_genuine`).
Every step names a DNS validator: the committee is validators, the beacon is a DNS epoch anchor, the fee is the
validator pool. ADR-0126 removed the voting-weight half; this ADR removes the rest.

PALW's own lotteries never read any of it (memo *PALW block production has no beacon*): the attempt's ticket is
`H(execution ‖ anchor)` (ADR-0072), the panel is drawn from PALW bonds with `H(anchor ‖ claim ‖ operator)`
(ADR-0124/0130), the execution lane's seed is an attempt-carrying chain block (ADR-0130), and the free-prompt
lane's beacon is a fold of attempt blocks (ADR-0074) — a different beacon, kept. What PALW shares with `vlt.rs`
is types: the job spec, the receipt hash, the runtime class id and the pinned model entries. Those stay.

## 2. The evidence: no shipped chain carried one of these transactions

A read-only walk of testnet-11 through the explorer node on 2026-09-17 (DAA 5,797; `getBlocks` from the pruning
point, every block, every transaction — [`0134-testnet-11-subnetwork-census-2026-09-17.json`](0134-testnet-11-subnetwork-census-2026-09-17.json)):
5,798 blocks, 17,522 transactions; by subnetwork: coinbase 5,798, `0x4b` (PALW) 1,918, native 196, `0x4a` 107,
stake-bond 10, attestation-shard 9,493; **compute certificate / challenge / capability / commitment / verdict
(`0x14`–`0x18`): 0**. testnet-10 is not a supported entry point; devnet and simnet are fresh chains; mainnet has
no chain. The VLT worker that would have produced these transactions was deleted by ADR-0126.

## 3. Decisions

**Decision 1 — a fence, its own.** `Params::palw_compute_overlay_retired: Option<ForkActivation>`, hashed
`Some`-only, named to the fork-id gate. Past it a block carrying a transaction on any of the five compute
subnetworks is invalid (`RuleError::ComputeOverlayRetired`) and the mempool refuses one
(`TxRuleError::ComputeOverlayRetired`); `SubnetworkId::is_compute_overlay` names the five. Below it the
transactions are accepted and do nothing, as they always did. testnet-11: **DAA 6,201**
(`PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA`), two hundred past the 6,001 flag day, so the two builds and the
two failure domains are told apart (the operator's order of work, ADR-0133 §9a). Every other preset: `None`
(a card decides for mainnet). The fingerprint moves; the identity does not.

**Decision 2 — the machinery is deleted now, on §2.** Deleted: `walk_compute_overlay`, `resolve_certificate`,
`ComputeOverlayWalk`, `ResolvedCertificate`, `stage_compute_capabilities`, `compute_audit_fee_outputs`,
`compute_challenge_adjudication_slashes`, `verified_capability`, `verified_commitment` (the virtual processor);
`check_compute_challenge_genuine`, `compute_challenge_genuine` and the challenge slash at acceptance (UTXO
validation); `DbComputeCapabilityStore` and its store module; `commitment_beacon_epoch`,
`verdicts_for_certificate`, `capability_candidate_pool`, the `Compute*Record`s and the `compute_*_from_accepted_txs`
readers (`dns_finality.rs`); `select_verifiers`, `adjudicate_compute_challenge`, `ChallengeOutcome`,
`VltCreditSkipReason`, `commitment_dependency_horizon`, `commitment_within_dependency_horizon`,
`verify_compute_certificate`, `refutation_quorum_reached` (`vlt.rs`); and their tests. Why now and not after
the height is pruned: the only block-validity rules among them were the challenge-genuineness refusal and the
challenge slash, and both act only on a block that carries a compute challenge — §2 says no block does. A node
syncing testnet-11 from genesis validates the same blocks the same way with or without the deleted code; the
fence is what keeps the future from ever needing it.

**Decision 3 — what stays, and why.** The payload types and the stateless shape validators
(`validate_compute_*_payload`, `validate_compute_challenge_tx`) stay: below the fence a well-formed compute
transaction is still *accepted* (and ignored), so the shape check is still the rule in force there. `DnsParams`
and `VltParams` keep every field (the whole struct is hashed; `VltParams::INERT`, the shadow fence and the cost
table are fingerprint inputs). The database prefixes of the deleted store stay reserved. The ADR-0022 overlay
commitment (`compute_overlay_snapshot`: bonds, reserve, window — not the VLT) stays; its name is a verb.

**Decision 4 — the PALW lotteries' domain separation from the VLT's is kept by the key.** The tests that drew
a VLT panel to show the PALW draws differ from it now draw under `VERIFIER_SORTITION_KEY` directly; the key
stays pinned apart in the domain-uniqueness tests.

## 4. Security amendments

* **SA-1 — a flag day, stated as one.** A node without the fence accepts a compute transaction past 6,201; a
  node with it refuses the block. Every testnet-11 node runs a build that schedules it before 6,201 — the same
  discipline as the 6,001 flag day, and the fork-id gate names the height, so a build without it is refused past
  it rather than forked silently.
* **SA-2 — the deletion is history-safe by measurement, not by argument.** §2 is a census of every block; a
  chain that had carried a compute challenge would have needed the deleted rules to validate it, and this ADR
  would have had to keep them until the height was pruned.
* **SA-3 — the tenth of the validator pool the audit fee could have spent** stays in the pool's remainder: no
  new issuance, no output that a block before 6,201 could have carried and now cannot.

## 5. What this does not do

It does not retire the DNS validator overlay: attestations, the stake reorg gate, ADR-0128's BFT vote and
ADR-0126's carve are live on testnet-11 and stay. It does not touch PALW's own beacon (ADR-0074's fold, the
free-prompt lane). It is not in Fence 1 (liveness), Fence 2 (the single lottery) or Fence 3 (economics) of
ADR-0133 §9a.

## 6. Implementation record

* 2026-09-17, `feat/palw-exec-lane-and-validator-retirement` (`4e1757af` and the closing commit): the fence,
  the block and mempool rules, the deletions and the test rewrites; testnet-11's fingerprint re-pinned
  `ab4e7b9c…` → `dd805c9f2c4e9db3c0d6ffa2d87fa6ffb4263078ab8b7eb857f8fb11f8aa010c` (the schedule gained 7,201, so
  the fork-id gate separates this build from the 7,001 union build and the 7,000 release), and the same evening
  → **`135b6ee07ba0c5e5951c3cb765ba9dfec8b85af4c6edccc5a2246a52338e766b`** when the operator moved the heights
  (held regime and deep audit 7,000 → 6,000, the flag day 7,001 → 6,001, this retirement 7,201 → 6,201, the two
  hundred kept; the tip was 5,798); the fork-id and
  fence-set pins extended (`the_shipped_schedules_are_measured_not_assumed`, the gate sets, the flag-day
  counterfactual, the carded-mainnet comparison — a card arms the retirement from genesis); the operator docs'
  schedule lines print `…, 6000, 6001, 6201, 6900, 2125000`. New pins: `adr0134_the_compute_overlay_retires_at_its_own_height`,
  `adr0134_the_five_compute_subnetworks_are_the_overlays`. Verification (this branch, the closing run):
  consensus-core 2,140 (nine VLT/DNS committee tests deleted with their subject), consensus (evm) 296 (nine
  compute-challenge tests deleted likewise), kaspad 98, misaka-cli 176, rpc-core 148, rpc-service 1, misaka-palw
  24, misaka-palw-extension 2, pq-validator-core 12 + pq-validator 42, mining 92, integration
  `rpc_tests::sanity_test` 1 — all green; clippy `-D warnings` over fourteen crates clean.

## 7. Number hygiene

0134 was free when written; the next free number is 0135.
