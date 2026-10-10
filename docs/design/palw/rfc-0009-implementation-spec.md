# RFC-0009 implementation spec — PALW remote miner (lane R9)

Status: implementation spec for branch `rfc9/remote-miner` (off `rcore/int-12` 0b1c11b87). Not part of the DAA-5,300 release.
Spec of record: [RFC-0009](../../rfc/0009-palw-remote-miner.md). This file fixes what the RFC leaves open: wire formats, versions, fences,
numbers, defaults for RFC §8, the test plan, and — per stage — **what is NOT guaranteed yet**.

## 0. Scope and honesty table

| Stage | What lands | Consensus? | What it does NOT guarantee |
| --- | --- | --- | --- |
| A0 remote model registration | `misaka-palw-remote::register` (quote by payer, pre-sign gate, exact-duplicate reuse, quorum registration tracker); `misaka model add --quote / --relay / --relay-min / --max-fee-sompi` | none (reads `getPalwRegistrationTerms`, `getPalwClaims`, `getPalwProducerFacts`, `getPalwClasses`, `getPalwModelRegistrationStatus`, `getBlockDagInfo`; submits with `submitTransaction`) | Not free: the wallet pays the carrier fee, the bond pays the 1 BILI burn and holds the exposure; a passing quote does not guarantee the fold accepts; the bond's backing is read from the node (no state proof, stage D); MISAKA Torrent integration is withdrawn (RFC14 §16); registration accepted ≠ Panel ready / first Final / reward / market |
| A remote claim | `misaka-palw-remote` client library (quorum view, idempotent relay, attempt-template digest, claim tracker, signed checkpoint); sighash audit tests; rail `--relay` mode (sign, no staging, many nodes) | none (no new wRPC op, tag, delta, tail, fence) | The miner PC must still serve the claim's material until B; an **attempt** block still needs the inference loop that lives in `kaspad/src/palw_producer.rs` (not extracted: A ships the template/relay/tracking half only); a majority of colluding nodes defeats the quorum (it is an operational defence, RFC §6) |
| C public redemption | `palw_receipt_spend_v4` (DORMANT): `PalwReceiptSpendEnvelopeV4`, admission, header carriage `PFS4`, coinbase split | yes, dormant | Nothing at runtime on any shipped preset (fence `None`); builder liveness depends on the fee cap being worth taking (activation gate: market test); a retiring executor bond cannot redeem; a claim is V4-redeemable only if its executor signed a redemption authorization (3.2) — the authorization rides the redemption, so any `Final` free-prompt claim can be redeemed once the miner has signed one |
| B independent DA | `EvidenceManifestV1`, provider storage receipt, any-provider fetch with root/claim verification (client + panel-side library); provider challenge/court as dormant design + types | none in this lane (court fence reserved, not added) | A storage receipt is a promise, not proof of future availability; nobody but the producer is slashed for withholding until the (unbuilt) court exists; **do not say "the PC can be off after claiming"** |
| D verified light client | requirements doc (6.) + Phase 1 safeguards (shared with A: multi-node, signed checkpoint, freshness, halt) | none | No state commitment / proof: bond, registry, fence and class facts are NOT cryptographically verified |

Until B and C are both done and armed, the product statement is only "a miner without kaspad can *claim*".

## 1. Allocations (lane R9)

* Consensus: **one fence**, `palw_receipt_spend_v4`. No algo id (the receipt lane keeps algo 7), no object tag, no state delta,
  no carriage tail, no DB prefix. The carriage is the header's `palw_commitment` with a new 4-byte magic `PFS4`.
* wRPC ops: none. A uses `getBlockTemplate`, `submitBlock`, `submitTransaction`, `getPalwProducerFacts`, `getInfo`/`getBlockDagInfo`,
  `getUtxosByAddresses`, `getMempoolEntry` as they stand, so no serverVersion gate is needed for A. (Any later op for B/D takes a
  number from the lead and ships behind the client's serverVersion gate: an old node drops the WebSocket on an unknown op.)
* B/D reserved (not used yet): object tags 150–153 (provider registration/receipt/challenge/response), deltas 140–141, tail 0xE9.
  Taken only when the court is built; listed so nobody else takes them.
* Domain strings: `misaka-palw/fp-v4/...` (receipt V4), `misaka-palw/remote/...` (client), `misaka-palw/evidence/...` (B).

## 2. Stage A — remote claim

### 2.1 Split signing from submission
The free-prompt path is already split: the rail signs (`ValidatorKey::build_fp_commitment_tx`, SIG_HASH_ALL ML-DSA-87, key from a seed
file or the `kaspa-pq-signer` sidecar) and `misaka-palw-fp-submit` submits. Stage A makes the *submit* half node-agnostic:

* `misaka-palw-remote::relay::broadcast_signed_tx(tx_bytes, nodes, policy)` — the miner computes the tx id **locally** from the signed
  bytes; every node's reply must equal it (a node that returns another id, or a verdict that is not accept / already-known, is
  recorded as `TamperedOrRefused` and never counted). Success = at least `policy.min_accept` nodes accept. **ACK is not inclusion.**
  Re-sending is idempotent: "already in mempool/accepted" counts as accept, and the tx id does not depend on the signature script
  (the kaspa id excludes it), so a relay cannot change the id by re-encoding.
* rail `--relay <rpc1,rpc2,...>` with `--bond-key-seed` and `--funding-outpoint/--funding-amount`: sign, write the raw tx, pre-flight
  the funding signature, relay to all, **stage no material** (the node-less miner has no retention dir; Stage B owns that) and print the
  per-node outcome. `--checkpoint <daa>:<hash>` first asks the nodes for a quorum view pinned to that checkpoint and sends nothing on any
  disagreement. `--submit` (single node + staging) is unchanged and is refused together with `--relay`.
* rail `--track <claim-id> --bond <txid:index> [--tx-id <hex>] (--relay a,b | --rpc a)` polls each node once through `getBlockDagInfo`,
  `getMempoolEntry` and `getPalwFreePromptClaim` and prints the tracker state per node and whether the nodes agree.

### 2.2 Sighash audit (result, with tests in `kaspa-pq-validator-core`)
`calc_mldsa87_signature_hash` (SIG_HASH_ALL) commits to: tx version; the hash of **all** input outpoints, **all** sequences, **all**
sig-op counts; the signed input's outpoint, script-public-key, **amount** and sequence; the hash of **all** outputs (value + script);
lock time; subnetwork id; gas; payload (length-prefixed). Consequences, now pinned by tests: fee (= input amount − Σ outputs), change
output, funding input, payload and the claim signature inside it cannot be altered by a relay without invalidating the funding
signature. Not covered, by design: the signature script itself (so the tx id is stable and re-submission is idempotent). Funding with
more than one input is not built by the rail (one input); a multi-input builder must sign every input.

### 2.3 Remote attempt template (no new op)
Inputs the miner assembles from existing ops, per node: `getBlockTemplate(pay_address = the miner's payout)`,
`getPalwProducerFacts(class, bond)` (chain point, DAA, class target, pwu, artifact root, retention, budget, bond key/exposure,
`not_ready_reason`), `getBlockDagInfo` (network, sink, pruning point, virtual DAA).

`AttemptTemplateV1` (client type) = { network_id, pre_checkpoint, template header fields (version, parents, daa_score, bits,
timestamp, pow_algo_id, palw_state_root), facts (chain_point, class_id, artifact_root, class_target, pwu,
min_trace_retention_daa, bond pubkey), node_id, observed_at_ms }.

**Template digest** (what nodes must agree on; excludes timestamp, nonce, transactions, coinbase, merkle roots, which legitimately
differ per node):
`template_digest_v1 = keyed-BLAKE2b-512[key = domain]( "misaka-palw/remote/attempt-template/v1" ‖ network_id_len ‖ network_id ‖ pruning_point ‖
version(u16 LE) ‖ pow_algo_id ‖ bits(u32 LE) ‖ daa_score(u64 LE) ‖ palw_state_root ‖ n_parents(u32) ‖ sorted_parents ‖
chain_point ‖ class_id ‖ artifact_root ‖ class_target(u128 LE) ‖ pwu(u64 LE) ‖ min_trace_retention_daa(u64 LE) )`
(the repo's keyed-domain BLAKE2b-512 helper; `Hash64`). Unanimity, not majority: any dissenting node is `Disagreement` and stops the miner.

Miner-side checks (all must pass, else **stop**, no inference is started):
1. ≥ `min_agree` (default 2, never 1) independent nodes (distinct endpoints; the library cannot prove distinct operators, the policy
   asks the user to list them) return the **same digest** and the same network id/genesis.
2. every node's reported pruning point/finality checkpoint is at or above the miner's **pinned checkpoint** (config or
   `SignedCheckpointV1`, 2.5) and none contradicts it (same DAA, other hash = fork → halt).
3. freshness: `virtual_daa − template.daa_score ≤ max_template_age_daa` (default 30) at start, and again immediately before
   submission (the inference takes time; stale parents/anchors invalidate it).
4. facts: `not_ready_reason` empty, bond pubkey equals the held key, class `artifact_root` equals the held artifact.
5. disagreement between nodes beyond `max_daa_skew` (default 12) or any digest mismatch → `Halt(reason)`; there is no "majority wins".

**Signed digest.** What the miner signs is unchanged consensus (the attempt envelope's ML-DSA signature over its own
`commitment_root`/challenge, `palw_attempt_v2`). The client *additionally* records `template_digest_v1` next to the produced block
and refuses to submit if a fresh quorum read no longer agrees on the same `chain_point` (the attempt was mounted on a stale view).
No signature is made before the inference is finished (RFC §3.2).

### 2.4 Completed-block submission
`template::submit_block_idempotent(block_bytes, nodes)`: the block hash is computed locally; nodes returning "duplicate/already
known" count as accept; a node that returns a different hash is `Tampered`. Double submission to many nodes is expected and safe.

### 2.5 Phase-1 verified-view safeguards (also Stage D phase 1)
`SignedCheckpointV1 { network_id, daa_score, block_hash, issued_at_daa, key_id, signature }`: the client verifies with an injected
verifier (ML-DSA-87 from `kaspa-txscript` in the binaries; the library takes a closure). Policy: checkpoint pinned from config or
signed; template/view rejected if `virtual_daa < checkpoint.daa` or the node reports a different hash at that DAA.

### 2.6 Claim tracking
`ClaimTracker` states: `Built → Relayed(txid) → Included{block,daa} → Licensed → ChallengeWindow{ends} → Final | Void`, each carrying
the observation it was derived from. Every poll feeds a `ChainObservation { sink, claim: Option<ClaimObs{bond, phase, accepted_block,
accepted_daa}>, tx_status }`; transitions may go **backwards** (reorg: included block leaves the selected chain → back to `Relayed`;
`Final` is only reported when the claim's accepted block is ≥ `finality_depth` below the sink). Mis-attribution: if the chain's claim
row names a different executor bond than the miner's, the state is `Misattributed` (terminal, loud). Tx id, claim id and block are
tracked separately.

### 2.7 Tests (Stage A)
tampering relay (id mismatch, refusal, byte flip rejected by sighash), stale template, template disagreement, checkpoint
contradiction, double submission, reorg → no mis-attribution, idempotent re-send, sighash coverage per field.

### 2.8 Stage A0 — model registration without a node (RFC §3.3–§3.6)

* **prepare/preflight** is `model add`'s existing gate (`catalog_row_admissible`, the artifact's root, the duplicate-root refusal), unchanged.
* **quote** (`register::quote_registration_v1`) is read from the node(s) and the locally built carrier: network domain, a digest of the live
  terms (keyed BLAKE2b over the `getPalwRegistrationTerms` answer), tip and an expiry (`QUOTE_VALIDITY_DAA` = 600), class/root and the digest of
  the UNSIGNED object, the carrier's mass and fee (priced with a 4,627-byte placeholder signature so the signed carrier costs the same), the
  burn (`PALW_CLASS_REGISTRATION_BURN_SOMPI_V1` past `palw_audit_2026_09_23`, else 0) and its payer (the bond), the exposure
  (`bundle.state.registration_exposure_sompi()`), the bond's backing (`max(bond_committed, bond_reserved_exposure) + bond_accuser_exposure`) and
  live locks, further filings (the Activation Pool sponsor). The chain's two gates are applied: `backing + exposure + burn ≤ collateral` and
  `live locks + burn ≤ collateral`; a shortfall is `NeedsGasOrBond { gas_short, bond_short, lock_short }`.
* **sign** only after the yes and after `pre_sign_gate_v1` on a fresh re-read: expired, terms/network changed, object swapped, fee rose, new
  shortfall → stop, nothing signed. The signed carrier's actual fee is compared again before relay; the funding signature is pre-flighted.
* **relay** to `--relay` nodes; success needs `--relay-min` (default majority) to return OUR id (`relay::fan_out_verdict`).
* **verify**: `RegistrationTrackerV1` over one observation per relay node per poll; `registration-accepted` needs the quorum; a row with
  another root (or bond, once reported) under our class from ANY node is `misattributed`; a lost inclusion is `reorged`, and only the same
  signed bytes are re-sent.

## 3. Stage C — public receipt redemption (DORMANT fence `palw_receipt_spend_v4`)

### 3.1 Fence and numbers
`Params::palw_receipt_spend_v4: Option<ForkActivation>` (four places; `None` on every preset; never() collapse; Some-only writes with a
companion value `[BUILDER_FEE_CAP_BPS = 1000]` hashed with the height, so changing the cap is a new network id).
`validate_palw_v2` requires, by name, at or below it: `palw_audit_2026_09_11` (B-5 per-mergeset quantum dedup),
`palw_audit_2026_09_23` (execution-root forfeiture closes the receipt lane), and a `ConsensusV2` bundle with the free-prompt params.
Drill height must be unique among fences.

### 3.2 Wire: `PalwReceiptSpendEnvelopeV4` (header `palw_commitment`, algo 7, magic `PFS4`)
Open question 5.1 decided: **the redemption authorization signs the beacon *rule* and a quantum range, not a beacon value.**
Reason: the draw beacon exists only after `Final + receipt_maturity`, i.e. after the miner's PC may be off; a value-signing
authorization would need the miner online at redemption. The beacon is determined by the chain from the claim's `final_daa`
(`fp_draw_slot_v3`), so signing the rule loses nothing.

```
PalwRedemptionAuthV4 {            // signed by the executor key, ONCE, position-free
  version: u16 = 1, network_domain: Hash64,
  claim_id: Hash64, executor_bond: TransactionOutpoint,
  quantum_lo: u32, quantum_hi: u32,        // half-open range of quanta any eligible builder may spend
  beacon_rule: u8 = 1,                     // 1 = ADR-0044 slot rule fp_draw_slot_v3 on the candidate chain; others refused
  builder_fee_bps: u16,                    // the share of the subsidy-derived worker reward the miner pays the builder; <= chain cap
  expiry_daa: u64                          // block daa must be <= expiry
}
PalwReceiptSpendUnsignedV4 {      // signed by the BUILDER key, bound to the header position
  version: u16 = 1, network_domain: Hash64, challenge: Hash64,   // spend_challenge_v4(domain,pre_pow,ts,nonce,claim,quantum,executor,builder)
  claim_id, quantum_index: u32, beacon_block: Hash64,
  executor_bond: TransactionOutpoint, builder_bond: TransactionOutpoint, builder_pubkey: Vec<u8>,
  authorization: PalwRedemptionAuthV4, executor_pubkey: Vec<u8>, authorization_signature: Vec<u8>
}
PalwReceiptSpendEnvelopeV4 { spend: PalwReceiptSpendUnsignedV4, builder_signature: Vec<u8> }
```
* Auth message = `canonical_id("misaka-palw/fp-v4/redeem-auth/v1", borsh(auth))`, signed under context
  `misaka-palw/fp-v4/redeem-auth-mldsa87/v1`. Spend id = `canonical_id("misaka-palw/fp-v4/spend-id/v1", borsh(spend))` (total over the
  authorization and signature, so the builder cannot swap them); builder signs the spend id under
  `misaka-palw/fp-v4/spend-mldsa87/v1`; the L1 tag the PoW arm expands is `Expand(spend_id_v4)` with its own domain.
* Size: two ML-DSA-87 signatures + two keys ≈ 14.9 KB > the 8,192-byte cap. The cap is raised **only** for a `PFS4` payload to
  16,384 (`PALW_COMMITMENT_MAX_BYTES_V4`); every other lane keeps 8,192. A `PFS4` header below the fence is refused by name
  (`ReceiptV4BelowFence`) at the header stage, so validity below the fence is byte-identical to today.

### 3.3 Validity (`check_palw_receipt_spend_admission_v5`, producer and verifier share it)
Items 1–5 and 8 of ADR-0044 Decision 6 are the V3 ones, unchanged. Item 6/7 replaced by:
6'. `executor_bond == claim.bond`; the bond exists and is not `Retiring` (default for §8: same as V3 — spend before you retire);
    `executor_pubkey == bond.pubkey`; authorization verifies; `auth.claim_id/executor_bond/network` equal the spend's;
    `quantum_lo ≤ quantum_index < quantum_hi ≤ quanta`; `beacon_rule == 1`; `builder_fee_bps ≤ cap`; `block_daa ≤ expiry_daa`.
7'. `builder_bond` exists, `Active`, `builder_pubkey == its key`, builder signature verifies (default for §8: any Active bond is an
    eligible builder; no class or stake minimum — the fee, not eligibility, is the incentive).
`ProducerNotExecutor` is **not** applied to V4 and V3 admission is untouched (a `PFS3` header decodes exactly as before).
The fold (`apply_receipt_spend`) is shared: it needs only `(claim_id, quantum_index)`, so a V4 spend folds through a V3-shaped view
(`to_fold_spend`), giving identical weight, census and single-use semantics (branch-scoped spent set → reorg revert unchanged).

### 3.4 Reward split (coinbase)
For an entitled merged block (blue or in-DAA red) whose header is a valid V4 spend, with
`subsidy_part` = the worker-base share derived from the block's subsidy and `fee_part` = the fee-derived worker share
(both from the existing carve arithmetic):
`builder_fee = floor(subsidy_part × builder_fee_bps / 10_000)`; **miner leg** = `subsidy_part − builder_fee` paid to
`p2pkh_mldsa87_spk(bond.payout_payload)` of the claim's executor bond; **builder** = `builder_fee + fee_part` to the block's own miner
script. Totals equal the V3 payment exactly (conservation: Σ outputs unchanged, no added issuance, no panel leg or reserve touched, no
maturity change). A missing claim/bond at the accepting state ⇒ the block is paid nothing (burn by don't-mint), deterministically.
Output order: the miner-leg output is placed immediately before its block's builder output, in mergeset iteration order. A zero
output is dropped (as V3 does).

### 3.4a Plumbing (what reads the fence)
* `Params::palw_receipt_spend_v4_fence()` feeds three readers, each resolved once at construction: the header processor (`PFS4` refused by
  name below the fence; past it, shape + challenge + both signatures on the relay path), the virtual processor
  (`palw_v2_check_receipt_spend` admits through `check_palw_receipt_spend_admission_full_v5`; `palw_v2_receipt_v4_payouts` builds the
  per-block payout map for the template AND the validating walk from the same selected-parent state), and the transaction validator
  (isolation's coinbase output cap widens by one output per mergeset block where the fence is declared — height-free, like the round lane).
* The PoW arm for algo 7 expands `Expand(spend_id_v4)` for a `PFS4` carriage and is unchanged for `PFS3`; the shape gate takes the V4 cap
  only for a `PFS4` payload on algo 7.
* No state-delta, object-tag, tail or DB change: the spent-quantum set, weight, census and reorg revert are the V3 fold's own.

### 3.5 Producer / template
`kaspa_consensus_core::palw_receipt_v4` also exposes the builder-side constructor so a producer and the verifier call one function. The
shipped producer (`kaspad/src/palw_producer.rs`) keeps building V3 for its own claims; a V4 *builder* mode (take authorizations from the
chain/relay) is out of this lane's first delivery and listed as open item C-1.

### 3.6 Tests (Stage C)
Admission: wrong executor, retiring executor, builder missing/retiring/wrong key, expired auth, quantum outside range, bad beacon
rule, fee above cap, tampered signature, already-spent quantum, V3 envelope still admitted, V4 refused below the fence. Coinbase:
conservation (V4 total == V3 total), miner leg to payout script, builder fee ≤ cap, rounding, zero-fee, red and blue paths, unentitled
block pays nothing. Fold: spent once by another party's block, weight/census equal to V3, reorg revert (delta round-trip), conflicting
siblings. Params: dormant on every preset, `t12-repin.sh --drift-only` clean, four-place fence round trip, validate prerequisites.

## 4. Stage B — independent DA

### 4.1 `EvidenceManifestV1` (canonical; library type, no chain object)
```
EvidenceManifestV1 { version: u16 = 1, network_domain: Hash64, preclaim_id: Hash64,
  trace_root, output_root, execution_root: Hash64, trace_chunk_count: u32, retention_until_daa: u64,
  encoding: u8 (1 = raw), max_expanded_bytes: u64, chunks: Vec<{ index: u32, len: u32, hash: Hash64 }> }
manifest_id = canonical_id("misaka-palw/evidence/manifest/v1", borsh(manifest))
preclaim_id = canonical_id("misaka-palw/evidence/preclaim/v1", network_domain ‖ executor_bond ‖ job_nonce ‖ trace_root ‖ output_root ‖ execution_root)
```
No circularity: `preclaim_id` is a function of the roots and the job nonce only, never of the claim id or the manifest hash; the claim id
may later include `manifest_id` because the manifest never includes the claim id (it includes `preclaim_id`).
Chunks must be contiguous from 0, sizes ≤ `max_chunk`, `Σ len ≤ max_expanded_bytes`, count = `trace_chunk_count` (+ witness chunks).
Verification of a fetched chunk: hash equals the manifest entry **and** the chunk's trace-chunk digest folds into `trace_root` via the
existing `fp_trace_manifest_root_v3`; the Panel re-derives roots and compares with the *claim's* roots (provider signatures are never proof).

### 4.2 Providers
`StorageReceiptV1 { version, network_domain, provider_id, manifest_id, chunk_set (bitmap or ranges), retain_until_daa, signature }`
(ML-DSA-87, provider key). `fetch_any_provider(manifest, providers, verify)` tries providers in a deterministic per-claim order, accepts the
first chunk that verifies, never trusts a failure message, records which provider failed which chunk (evidence for the future court).
Initial responsibility: **unchanged** — the claim's producer remains accountable (`DefaultAccused` paths untouched); the client fetch is
used by the Panel only as an additional source after the producer's own node.

### 4.3 Provider bond / challenge court (dormant design + types)
Reserved fence name `palw_evidence_court_v1` (not added to Params: there is no consensus rule yet to arm). Skeleton types:
`ProviderChallengeV1 { claim, manifest_id, chunk_index, challenger_bond, deadline_daa }`, `ProviderResponseV1 { opening }`.
Rules fixed for the future implementation: a challenge is on-chain and public; the provider has `response_deadline_daa` to publish an
opening that hashes to the manifest entry; only a missed deadline moves provider collateral; **never** slash on a single Panel's
timeout, local fetch failure or an unsigned/pre-signed receipt; common-mode failure of all providers ⇒ the claim lapses unpaid (not a
fraud slash on the miner); a failure is charged to exactly one party (provider XOR producer, selected by the claim's version/fence),
so no double slash with `DefaultAccused`.

## 5. Stage D — verified light client
Phase 1 (done in A): multi-node quorum, signed/pinned checkpoint, freshness bound, halt on disagreement.
Phase 2 requirements (design only): (a) a state commitment in the header covering bond registry, class registry, fence set and claim
phase (the V2 `palw_state_root` already commits the PALW state — the proof requirement is an inclusion-proof format per fact and a
light verification of the root against a pruning-proof/finality chain); (b) checkpoint update rule (new checkpoint must be a descendant
verified by header-chain PoW work ≥ W and an overlap with the old one); (c) reorg rewind rule; (d) the client refuses to sign when any
fact lacks a proof (fail closed); (e) resource budget (header-only sync + O(log n) proofs). A header alone cannot prove bond/registry/fence
state.

## 6. RFC §8 defaults (proposed; none irreversible)
1. Materials per class/form: PublicDa needs trace chunks + prompt/answer envelope; PanelDa the staged ids; held needs the held
   attention opening — only the first two are in B's first cut (held stays producer-served).
2. Provider bond/challenge/deadline/retention: design only (4.3); proposed starting values live in the court spec, not code.
3. V4 fee: miner-chosen `builder_fee_bps ≤ 1000` (10 %) of the subsidy-derived worker share; builder also keeps all fees of its block.
   Market test before arming (activation gate).
4. Remote client proof needs: section 5.
5. RFC-0008 work-slice blocks: a V4 spend and a work-slice credit must never credit one claim twice; the spent-quantum set is the single
   source of truth; RFC-0008 integration is deferred until it is armed (it is dormant on its own branch).

## 7. Open items after this lane
C-1 V4 builder mode in the shipped producer and authorization discovery (relay of authorizations); A-1 extract the attempt loop from the
node producer so a node-less miner can run attempts end to end; B-1 panel-side fetch wiring inside `kaspad` (library only here);
B-2 the court; D-2 state proofs.

## 8. Status at the end of the lane (what is built, what is not, what was run)

| Stage | Built | NOT done / NOT guaranteed |
| --- | --- | --- |
| Spec | this file; allocation line in `lanes/COMMON.md` (fences `palw_receipt_spend_v4`, `palw_evidence_court_v1`; no tag/delta/tail/op taken) | — |
| A0 | `register.rs` (6 tests: costs by payer and burn never the wallet's, split shortfalls incl. live locks, the pre-sign gate's stops, duplicate reuse vs conflict, quorum + reorg walk-back + same-bytes-only resend, misattribution from any node); CLI `model add --quote/--relay` (sign here, relay to several nodes, the node-returned id checked against our own bytes, quorum-tracked to the registry's row) | No registrant bond in `getPalwClasses` rows, so misattribution is checked on the root only (a bond field is a read-only follow-up); no live drill run against testnet-12 in this lane; MISAKA Torrent integration is withdrawn (RFC14 §16); a Web UI over the same library is not built |
| A | `misaka-palw-remote`: quorum view + pinned/signed checkpoint, idempotent relay, template digest, claim tracker, **`attempt`** (a remote attempt: miner-side executor and signer traits, one shared assembly function `palw_attempt_from_execution_v1` that the node's producer now calls too, class/network draw, sign-once-on-win, publish re-check); sighash audit tests; rail `--relay`, `--checkpoint`, `--track`, `--evidence-out`, `--redeem-auth-out` | The executor itself (the model backend in `kaspad/src/palw_backends`) is not extracted: a remote miner links its own behind `AttemptExecutor`; there is no ready-made remote-miner binary; the pure adapter `template::{producer_facts_from_wire_v1, template_observation_v1}` turns a `getBlockTemplate` header (via `TryFrom<RpcRawHeader> for Header`) and `getPalwProducerFacts` into a `TemplateObservation`, but no binary drives it yet; a colluding quorum defeats the view |
| B | `palw_evidence_v1` (manifest, preclaim id, chunk and claim-root checks, storage receipt) in consensus-core; directory provider and `fetch_claim_material`; **kaspad wiring**: `--palw-evidence-provider-dir`, the panel's material pull is also answered from providers (manifest agreeing with the claim's on-chain roots, every chunk verified), injected into the same inbox as a peer's answer (`PalwGossip::inject_local_material`) so the seat's own `verify_material`/re-execution still judge it; the producer pull is unchanged and is the fallback; **`palw_evidence_court_v1`**: a pure, tested provider challenge court (file/challenge/answer/sweep; deadline; charge once per (claim, provider); every filed provider defaulted ⇒ claim lapses with `miner_fraud = false`; a Panel's local timeout is a no-op; producer-withholding refused for a claim under the court, so one failure is one party's) + the dormant fence (four places, prerequisites, fork-id, tests) | **The court is not folded into `palw_state_v2`**: no object tags/deltas/carriage, no collateral movement, no claim-version bit — the fence has no reader in consensus; a storage receipt on chain does not exist yet; the directory transport serves only materials up to the 16 MiB gossip cap (interval transport for huge captures is unchanged); nothing about "PC can be off" is guaranteed until the court is folded and armed |
| C | as before + the redemption block tests through the processor's coinbase expression | dormant; no full-chain drill |
| D | `palw_state_proof_v1`: `state_root_preimage` (the state root is now hash(preimage), one spelling), `palw_collection_root_of_entries_v1`, header→pinned-block→state-root→collection→row proofs for bonds, classes and claims (presence AND absence), the client policy `proof::verify_bond_against_pin` | The state commitment is flat, so a proof is O(rows) (megabytes for a bond table); a tree-shaped commitment is a state-root version bump (design only); PoW verification of the header chain is not implemented — the pinned block's standing is the signed checkpoint's trust; fences are params, not state, so there is no state proof for them |
