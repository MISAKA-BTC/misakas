# RFC-0009 remote client / relay / redemption — implementation record (Agent C1)

Branch `rfc9/c1-remote` (base `f9c2c4308`). Matrix: [`remaining-rfc-integration-matrix.md`](remaining-rfc-integration-matrix.md) (RFC-0009 rows).
Spec of record: [RFC-0009](../../rfc/0009-palw-remote-miner.md), [implementation spec](rfc-0009-implementation-spec.md).

Statuses (only these): **IMPLEMENTED_AND_TESTED** · **IMPLEMENTED_REFERENCE_ONLY** · **DORMANT_NOT_INTEGRATED** · **CODE_GAP** ·
**DESIGN_GAP** · **EXTERNAL_GATE_PENDING**. "Tested" means a unit, file-level, local-socket or real-virtual-processor test in this tree. **No case below
has been run against a live node or network** — every `Real node` entry is "not run" unless a row says otherwise. A fixture or reference PASS is never
production completeness.

Invariants kept: a relay forwards bytes a node validates itself and is never a truth authority; a DA provider is never a computation authority; multiple
RPCs agreeing is **not** a proof (every unproven remote fact is labelled `UNVERIFIED_REMOTE_STATE`); signing keys never leave the owner's process
(builders and relays see unsigned objects and signed bytes only). Frozen FP Job V4, V3 redemption, consensus params and fingerprints, the registration
wire, `misaka-palw-challenge/`, `misaka-palw-kernel/` and the heartbeat/REAL/BASE-0/EXEC roles are untouched. No object tag, delta number, carriage tail
or fence was taken; the reserved 150–153 / 140–141 / 0xE9 stay unused.

## How to run the tests

All targeted (never workspace-wide). `misaka-cli` is a bin-only package, so its tests are `--bin misaka`, not `--lib`.

```
cargo test --offline -p misaka-palw-remote                          # 71 lib + 2 CLI end-to-end
cargo test --offline -p misaka-cli --bin misaka operator::model     # 8
cargo test --offline -p kaspa-pq-validator-core --lib               # 48
cargo test --offline -p kaspa-pq-signer --lib                       # 17
cargo test --offline -p kaspa-rpc-core --lib palw_state_proof       # wRPC serializers
cargo test --offline -p kaspa-grpc-core --lib palw_state_proof      # grpc round trip
cargo test --offline -p kaspa-consensus --lib t12_state_proof       # real processor, op 202's consensus half
cargo test --offline -p kaspa-consensus --lib rfc9_redemption_v4    # real processor, 5 redemption tests
cargo build --offline -p misaka-palw-remote --features rpc --bin palw-remote-miner
cargo build --offline -p misaka-palw-gateway --bin misaka-palw-fp-rail
```

Pre-existing, unrelated: `cargo fmt --check` already differs in `misaka-palw-remote/src/attempt.rs`, `misaka-cli/src/{main,bond,node,…}.rs` and others
(not touched by this lane except where noted); the many `unused variable: ctx_hash` warnings are in `misaka-palw-base0`.

## 1. Node-less model registration, detached signing

Three steps, three possible machines; the key exists only at step 2.

| Step | Command | Holds a key? | Reads a node? | Writes |
|---|---|---|---|---|
| 1 export | `misaka model add <model> --export-bundle FILE --payer-address ADDR --rpc A --quote-rpc B,C [--owner-pubkey HEX] [--pin BLOCK]` | no | yes (≥ 2 nodes; `--allow-single-rpc` for a node you run) | unsigned `RegistrationBundleV1` (JSON) |
| 2 sign | `misaka model sign FILE --key-file OWNER [--payer-key-file PAYER] --expect-class C --expect-root R --expect-owner TXID:I [--max-fee-sompi N] [--pin BLOCK] [--owner-only \| --carrier-only]` | yes | no (`--rpc` only for the expiry clock) | `FILE.owner-signed.json`, `FILE.signed.json` (never overwritten) |
| 3 submit | `misaka model submit SIGNED --relay N1,N2,N3 [--relay-min K] [--allow-duplicate] [--pin BLOCK]` | no | yes (relays) | `~/.misaka/<net>/model-add/<class16>/sent.jsonl` |
| verify | `misaka model verify [SIGNED \| --class C --root R --owner O] --pin BLOCK --rpc N` | no | yes (one node, proof only) | — |

**Bundle** (`misaka.palw.registration-bundle.v1`): network name, network domain (name + genesis), ruleset id (= `Params::consensus_params_id()`), class id,
artifact root, owner bond and its public key (optionally a state proof of both), the borsh `ClassRegistered` object (owner signature empty), the digest the
quote names, the exact message the bond key signs and its context, the payer (address, script, the single funding input), the carrier plan (fee, mass,
change, change script, priced payload length), the quote (every cost by payer, expiry DAA, totals) and the quoting nodes (`UNVERIFIED_REMOTE_STATE`).

**What `sign` re-derives itself** (`bundle::check_bundle_v1`; nothing is taken from the bundle's display text): the network domain and ruleset id from its
own build; the object's class id from its profile; class / root / owner against `--expect-*` (the user's, or confirmed at the prompt — never with
`--yes`); the digest the quote names; the registration message it then signs; that the owner key is the bond's registered key (against `--pin`, if given);
the quote's totals; the registration exposure and burn this build knows (a quote understating them is refused); the carrier's arithmetic, that the **only
output is the change back to the payer's own script**, and the fee against the signer's own cap. The payer signs only after the owner's signature is in
the object (the carrier sighash covers the payload), may be a different key than the bond's, and refuses a funding input that is not locked to its own
address.

| Case | Test |
|---|---|
| builder/relay swaps the change **recipient** → `ChangeNotPayer`; the funded script → `PayerIsNotThisKey` | `bundle::tests::a_builder_that_swaps_the_change_recipient_is_refused_before_anything_is_signed` |
| inflated **fee** → `QuoteInconsistent` / `FeeAboveCap` / `CarrierArithmetic` | `…a_builder_that_inflates_the_fee_is_refused_by_the_cap_and_by_the_quote` |
| swapped **class root** / **owner** / class id (self-consistent forgery) → `RootSwapped` / `OwnerSwapped` / `ClassIsNotItsProfile`; wrong key → `OwnerKeyMismatch` | `…swaps_the_class_root_or_the_owner_is_refused_against_what_the_user_expects` |
| network, ruleset, expiry, understated exposure/burn | `…network_ruleset_expiry_and_understated_costs_are_refused` |
| stages cannot be skipped/repeated; forged owner signature refused by the payer | `…the_stages_cannot_be_skipped_or_repeated` |
| bytes altered after signing (recipient, fee, root, owner, signature bit, declared id, expiry, cap) | `…a_relay_that_alters_the_signed_bytes_is_caught_before_and_after_the_wire` |
| **resend** (same bytes) vs **duplicate registration** (other carrier) vs registry-wins | `…resend_is_idempotent_a_second_carrier_is_a_duplicate_and_the_registry_wins` |
| multi-RPC quote: terms/burn/exposure/root/network/bond/mass disagreement stops everything; balances merge worst-case; one node is not agreement | `…nodes_that_quote_different_terms_are_never_averaged` |
| accepted ≠ Active: shared lifecycle codes; later stages and `BEACON_UNAVAILABLE` / `PUBLIC_PROSECUTION_INCOMPLETE` shown `DORMANT_NOT_INTEGRATED` | `…accepted_is_not_active_the_view_names_the_shared_codes_and_infers_nothing` |
| a signer that pinned a block checks the owner key offline; pin without proof, wrong pin, wrong key, absent bond, tampered rows all refused | `…a_signer_that_pinned_a_block_checks_the_owner_key_against_it_offline` |
| key-free pricing = the shipped signing builder's mass/fee; the detached carrier has the shipped builder's payload and outputs | `model_bundle::tests::the_key_free_price_is_the_shipped_builders_price_and_the_detached_carrier_has_its_id` |
| the CLI's `ValidatorKey` through both steps with two different keys; the carrier decodes as the node's fold reads it | `…the_validator_key_signs_both_detached_steps_with_two_different_keys` |
| `model sign` on files: never overwrites; refuses tampered recipient / fee cap / root / owner / bare `--yes` / wrong key and writes nothing | `…model_sign_writes_a_signed_file…`, `…model_sign_refuses_a_tampered_bundle…` |

A defect fixed on the way: the in-process `model add --relay` path hashed `getPalwRegistrationTerms` **including `tip_daa`** (the node's clock), so the
pre-sign gate reported "terms changed" whenever a block arrived between the quote and the signature, and no two nodes at different tips could agree on a
quote. The digest now excludes `tip_daa`.

| Item | Status |
|---|---|
| detached export / sign / submit, relay-tamper refusal, fee payer ≠ bond key, resend vs duplicate, multi-RPC agreement, `--pin` | IMPLEMENTED_AND_TESTED (unit + file-level CLI tests); **no real-node run** |
| lifecycle display with the shared codes; Dormant / ChallengePending / `BEACON_UNAVAILABLE` / `PUBLIC_PROSECUTION_INCOMPLETE` | IMPLEMENTED_REFERENCE_ONLY — display only; the codes are tracked `DORMANT_NOT_INTEGRATED` (no on-chain source exists, so none is ever inferred from a registry row or an ACK) |
| class registration ≠ model line creation | shown as a separate line (`MODEL_LINE: not created`); the line object is not touched |
| registrant bond in the class-row RPC (misattribution was checked on the root only) | **closed without an RPC change**: `registration_at_pin_v1` reads `registrant_bond` out of the *proven* class record (`t12_state_proof`, `proof::tests::a_registration_is_proven_present_absent_or_misattributed…`). The unproven class row still lacks the field (CODE_GAP, read-only, low value now) |
| **expiry binding** | **CODE_GAP / consensus gap G-EXPIRY** |
| **ruleset-id binding inside the signed object** | **CODE_GAP / consensus gap G-RULESET** |
| web UI over the same library | not built |

### Consensus gaps for the Lead (the registration wire is NOT changed)

* **G-EXPIRY.** The owner's signature (`palw_class_registration_message_v2`) covers network domain, class, share, activation DAA, owner bond, root, slash
  value, target, pwu rule and the canonical job — **no expiry**. A carrier cannot carry one: `lock_time` is a not-before bound, and the lifecycle
  carrier's inputs use the final sequence number, so it is not even evaluated. What exists instead is client-side only: the quote's `expiry_daa` is in the
  bundle and the signed file, `sign --rpc` and `submit` refuse past it, and the only consensus-level cancel is spending the funding input elsewhere. A
  leaked signed file stays valid until then. Closing it needs a versioned object field (a signed `valid_until_daa` checked by the fold) — new wire + fence;
  proposal only.
* **G-RULESET.** The ruleset id is not inside the signature either; the network domain (name + genesis) is. The signer recomputes the ruleset id from its
  own build and compares it with the bundle's, and the builder refuses quote nodes whose `consensus_params_id` differs from its build's; neither is
  enforced by consensus. A signed `ruleset_id` in the registration message would need the same versioned bump as G-EXPIRY.

## 2. Remote client verification

New read-only RPC op **`getPalwStateProof` = 202** (follows `getPalwCapacityShadow` = 201 end to end): rpc-core types + wRPC serializers, ops enum, trait
methods, grpc proto + convert + ops + server factory + client route, wRPC router + client, the rpc service, the `ConsensusApi` default method
`palw_state_proof_v1`, and the virtual-processor implementation. It returns the header of a block (the sink when empty) and **the state that header
commits — the state as-of its selected parent** — as the state-root preimage plus every row of one collection (`bonds` / `classes` / `claims`); the node
refuses unless its own state hashes to exactly what the header commits, and is bounded (`PALW_STATE_PROOF_MAX_LAG_DAA = 4,000` behind the sink,
`PALW_STATE_PROOF_MAX_BYTES = 24 MiB` per answer). **A node built before op 202 drops the WebSocket on it**: ask it last, or reconnect.

Client side (`misaka-palw-remote::{trust, proof}`): `Provenance` (`UnverifiedRemoteState` / `ProvenAtPin` / `ProvenAbsentAtPin`), `bond_at_pin_v1`,
`class_at_pin_v1`, `claim_at_pin_v1` (absence is proven only from a proof that opens), `registration_at_pin_v1` (registrant bond and root from the proven
class record), `StateProofV1` (the file form a bundle embeds). The trust root is the **pin** — a block hash the user got from a signed checkpoint or their
own node — and nothing the serving node said. `AgreedView::provenance()` labels a quorum view `UNVERIFIED_REMOTE_STATE`; `rail --track` and `--relay` print
the label.

| Case | Test | Status |
|---|---|---|
| the node proves bonds/classes/claims of its real testnet-12 chain against the header; absence proven; a swapped header, tampered preimage, forged row, wrong collection each refused; unknown block / collection named | `consensus …::t12_state_proof::the_node_proves_the_state_its_header_commits_to_a_client_that_pinned_only_the_block` | IMPLEMENTED_AND_TESTED (real processor) |
| op 202 survives the wRPC serializers and the grpc wire, header included | `kaspa-rpc-core` `test_wrpc_serializer_get_palw_state_proof_*`; `kaspa-grpc-core` `the_state_proof_survives_the_grpc_round_trip` | IMPLEMENTED_AND_TESTED |
| registration standing: present / absent / misattributed (other registrant, other root, genesis class) from a proven record; a hidden row cannot make a bond absent | `proof::tests::a_registration_is_proven_present_absent_or_misattributed_against_the_pin_and_never_by_a_nodes_word`, `…bonds_and_claims_have_the_same_present_absent_standing` | IMPLEMENTED_AND_TESTED |
| only a header-anchored proof removes the label | `trust::tests::…`, `view::tests::an_agreed_view_is_labelled_unverified_remote_state_however_many_nodes_agree` | IMPLEMENTED_AND_TESTED |
| a live node answering op 202 to the CLI/miner over wRPC | not run | no live node in this lane |
| header-chain PoW verification (is the pinned block on the heaviest chain?) | — | **CODE_GAP** (the pin is a signed/own checkpoint's trust) |
| a cheap proof | — | **DESIGN_GAP**: the state commitment is flat, so a proof is O(rows) (a bond table with 2.6 KB keys is megabytes); a tree commitment is a state-root version bump |
| a state proof of a fence | — | none by design: fences are params committed through the ruleset id, which the client recomputes |

## 3. Remote ordinary attempt

`misaka-palw-remote::miner` is the loop body; the `palw-remote-miner` binary (feature `rpc`) wires it to wRPC. One step: quorum view
(`view::agree`: ≥ 2 distinct nodes, one network and pruning point, DAA skew, none contradicting the pinned checkpoint) → a template from every node →
`check_templates` (**unanimity**, fresh, the held key is the bond's registered key, the held artifact is the class's) → the facts must name **our class**
→ mount (execute on the miner's machine, class lottery, network lottery, **sign once only on a win**, through the producer's equivocation journal) → a
**fresh** quorum and template re-check after the inference → publish to every node idempotently (a node answering another hash never counts) → follow
the block (`BlockTracker`: known → on-chain → settled, a reorg walks it backwards, `Lost` re-sends the SAME bytes) and the claim (`ClaimTracker` per
node; a row naming another executor bond is `Misattributed`). One position, one attempt, one block (`Equivocation` refuses a second).

| Case | Test | Status |
|---|---|---|
| won draw published to every node after a fresh re-check, signed once; the block carries the signed attempt for this position | `miner::tests::a_won_draw_is_published_to_every_node_after_a_fresh_recheck_and_signed_once` | IMPLEMENTED_AND_TESTED (fake nodes) |
| stale template refused before any inference | `…a_stale_template_is_refused_before_any_inference` | IMPLEMENTED_AND_TESTED |
| template ages during the inference → dropped, nothing sent | `…a_template_that_ages_during_the_inference_is_dropped_not_published` | IMPLEMENTED_AND_TESTED |
| wrong bond key / foreign class / one dissenting node / one node alone / checkpoint contradiction each stop the miner | `…a_wrong_bond_key_a_foreign_class_and_a_disagreeing_node_each_stop_the_miner` | IMPLEMENTED_AND_TESTED |
| double submission idempotent; a second attempt at a position refused | `…publishing_the_same_block_twice_is_idempotent_and_a_second_attempt_at_a_position_is_refused` | IMPLEMENTED_AND_TESTED |
| a lying hash never counts; nothing recorded as published | `…a_node_that_answers_with_another_hash_never_counts_as_having_taken_the_block` | IMPLEMENTED_AND_TESTED |
| reorg tracking: known → chain → settled, reorg walks back, lost | `…a_published_block_is_followed_to_settled_and_a_reorg_walks_it_backwards` | IMPLEMENTED_AND_TESTED |
| the binary (wRPC adapters, journaled signer, executor seam, `--pin`) | builds; arg refusals smoke-checked | IMPLEMENTED_REFERENCE_ONLY — **never run against a node** |
| the executor (model backend) | the binary takes `--executor-cmd`: a program printing the roots as JSON; there is deliberately no built-in demo executor | **CODE_GAP**: extraction from `kaspad/src/palw_backends` |
| the node validating the block independently | by construction — nothing here is an authority | n/a |
| keeping/serving the capture after the PC is off | the binary keeps `state-dir/material/<attempt id>`; the PC must stay up until a provider serves it | see §5 |

## 4. Remote free-prompt claim

* **Sidecar custody.** `kaspa-pq-validator-core::MessageSigner` (the bond key in-process, or a sidecar) with `build_fp_commitment_tx_with` and
  `estimate_overlay_fee_with`; `ValidatorKey::build_fp_commitment_tx` / `estimate_overlay_fee` are now the same functions over the in-process key.
  `kaspa-pq-signer::sidecar::SidecarSigner` asks the daemon for exactly two typed signatures per carrier (the claim id as `PalwFpCommitmentV3`, the funding
  sighash as `Transaction`) and refuses any other context before the wire. `misaka-palw-fp-rail --signer-socket PATH --bond-pubkey HEX` builds the carrier
  through it: the seed never enters the rail process.
* **Relay by anyone.** `misaka-palw-fp-rail --relay-signed TX --funding-amount N --relay a,b,c` relays a carrier somebody else signed:
  `relay::check_fp_carrier_v1` (free-prompt subnetwork, payload decodes, claim signature verifies under the executor key the commitment names), the funding
  entry rebuilt from the public amount and the key in the signature script, then the existing fan-out whose pre-flight verifies the funding signature.

| Case | Test | Status |
|---|---|---|
| the carrier built through a `MessageSigner` asks for exactly two typed signatures; the method is that function; a length-only signer measures the same size | `kaspa-pq-validator-core …fp_commitment_through_a_message_signer_asks_for_exactly_two_typed_signatures` | IMPLEMENTED_AND_TESTED |
| the carrier built THROUGH the daemon over a real Unix socket matches the in-process key's (shape, outputs, commitment, prompt ids), claim and funding signatures verify, other contexts/sizes refused | `kaspa-pq-signer …the_free_prompt_carrier_is_built_through_the_sidecar_and_the_key_never_leaves_it` | IMPLEMENTED_AND_TESTED (local socket) |
| a relay forwards a carrier it did not sign; wrong funding amount, flipped claim-signature bit, foreign tx refused before any node is asked | `relay::tests::a_relay_forwards_a_carrier_it_did_not_sign_and_refuses_one_that_cannot_stand` | IMPLEMENTED_AND_TESTED |
| the rail's new flags (`--signer-socket`, `--relay-signed`) | builds; the pieces they call are the tested ones | IMPLEMENTED_REFERENCE_ONLY (the rail binary has no test harness; **not run against a node**) |
| the V4 redemption authorization through the sidecar (`--redeem-auth-out` with `--signer-socket`) | **closed (SMALL lane)**: `SigningPurpose::PalwReceiptAuthV4 = 8` (appended; 0–7 unchanged), visible only under `palw_receipt_spend_v4` — see "SMALL lane" below | IMPLEMENTED_AND_TESTED (dormant: the fence is `None` on every preset) |
| funding with more than one input | not built (the rail uses one input; `carrier_funding_signature_valid` requires one) | CODE_GAP |
| the claim's material path independent of the miner | §5 | CODE_GAP pending public material read (workstream A) |
| network-bundle (not devnet) pricing in the rail | unchanged (devnet bundle) | CODE_GAP |

## 5. Independent DA transport

`misaka-palw-remote::transport` + the `palw-evidence` binary (std only; no node, no key): the content-addressed layout of `evidence` over HTTP
(`StdHttp` for `http://`, `CurlHttp` for `https://` — no TLS stack is linked), a provider list from a file or flag (`dir:`, a path, `http(s)://`), a
reference provider (`server`) that verifies a manifest against its chunks before serving it, upload with **read-back**, availability, repair and a
retention monitor, and one fetch for everyone (`fetch_claim_material_any`; `evidence::fs::fetch_claim_material` is now that function over directories, so
kaspad's `--palw-evidence-provider-dir` and a public verifier on HTTP read the same bytes under the same checks).

| Case | Test | Status |
|---|---|---|
| three providers (2 HTTP + 1 dir) hold the evidence; the node (dir), the Panel (mixed) and a public verifier (HTTP) read byte-identical material; a dead provider and a corrupt chunk are recorded not believed; a manifest disagreeing with the CLAIM's roots is refused | `transport::tests::three_providers_hold_the_evidence_and_the_node_the_panel_and_a_public_verifier_read_the_same_bytes` | IMPLEMENTED_AND_TESTED (127.0.0.1) |
| the reference provider refuses a manifest before its chunks (409), a chunk not matching the manifest, an oversized chunk (413), junk, path traversal, other verbs | `…a_reference_provider_refuses_what_it_cannot_back` | IMPLEMENTED_AND_TESTED |
| availability: verified / missing / corrupt / unreachable are different; a different manifest under the claim is recorded; expiry; always `LOCAL_OBSERVATION` | `…availability_is_a_local_observation_and_distinguishes_missing_corrupt_and_unreachable` | IMPLEMENTED_AND_TESTED |
| a watcher repairs a thin provider from verified copies after the miner is gone; the monitor drops expired claims and is loud (`AtRisk`) when nothing is left to copy | `…a_watcher_repairs_a_thin_provider_from_verified_copies_after_the_miner_is_gone` | IMPLEMENTED_AND_TESTED |
| provider list parsing; chunked/oversized HTTP responses | `…a_provider_list_parses_dedups…`, `…a_response_that_chunks_its_body…` | IMPLEMENTED_AND_TESTED |
| `palw-evidence publish / status / repair / fetch` as an operator runs them | `tests/evidence_cli.rs::publish_status_repair_and_fetch_through_the_command_line` | IMPLEMENTED_AND_TESTED |
| `https://` providers via curl | compiled; not exercised (no external network) | IMPLEMENTED_REFERENCE_ONLY |
| provider discovery | a config/list only | **DESIGN_GAP** (no on-chain discovery) |
| `misaka-model-transport` | absent in-tree | CODE_GAP — not integrated |
| kaspad's `--palw-evidence-provider-dir` accepts directories only | a 5-line change to parse `ProviderSpecV1` in `palw_panel.rs` would add URLs; not made (kaspad is a Lead/seat-owned build; a directory kept in sync by `palw-evidence fetch`/`rsync` serves the same purpose today) | CODE_GAP (small) |
| public material read for outsiders; objective demand/default (who is slashed for withholding) | workstream A | EXTERNAL_GATE_PENDING |
| provider challenge court / storage receipts on chain | types + pure court exist; reserved tags 150–153, deltas 140–141, tail 0xE9 unused | DORMANT_NOT_INTEGRATED |

**Not guaranteed, in so many words:** a provider's HTTP 200, an ACK, a storage receipt or an infohash is not availability; the `status` line is this
machine's observation only; nobody but the producer is accountable for withholding until the court is folded and armed; **do not say "the PC can be off
after claiming"** until a provider court exists and is armed.

## 6. Public receipt redemption V4 (dormant fence `palw_receipt_spend_v4`)

**Discovery/relay of authorizations** (no gossip, no RPC op, no consensus change): the providers of §5 also carry `redemptions/<claim>.rda4`
(`palw-evidence redemption-publish` — read back from every provider; `redemption-sync --into DIR` mirrors every sound, unexpired bundle into the
directory the node's builder mode already reads, `--palw-redemption-auth-dir`). The reference provider stores a bundle only if it is sound by itself
(versions, beacon rule, fee cap, the executor's signature, filed under its own claim); that the executor bond exists, holds that key and owns the claim is
the chain's — the node's admission checks it again. A provider keeps one bundle per claim; an authorization is not revocable except by its expiry.

**On the real virtual processor**, fence armed on a TEST copy of the params (`None` on every preset), `rfc9_redemption_v4.rs`: the executor signs the
`RDA4` bundle once and its key is never used again; two OTHER bonds with other keys build receipt blocks from it.

| Case | Test | Status |
|---|---|---|
| miner and builder different bonds; miner leg to the executor bond's registered payout; two builders racing for one quantum → paid once; the first builder gone → the other redeems; unregistered builder, builder bond named with another key, foreign executor, expired authorization → not entitled | `rfc9_c1_a_builder_other_than_the_miner_redeems_and_the_miner_is_paid_while_offline` | IMPLEMENTED_AND_TESTED (processor methods, fold) |
| fee above the cap (1,001 bps), quantum outside the authorized range, forged and altered authorizations → refused at the header stage **for the named reason** | `rfc9_c2_the_header_stage_refuses…` | IMPLEMENTED_AND_TESTED |
| builder fee ≤ cap and carved out of the worker reward; totals equal V3's; every other output (Panel leg, reserve, tip block's reward) untouched pair for pair; rounding; 0 bps; fence-off twin = V3 | `rfc9_c3_the_builder_fee_is_carved_out_of_the_worker_reward_and_nothing_else_moves` | IMPLEMENTED_AND_TESTED (coinbase manager) |
| spent-quantum ledger is branch-scoped; a reverted delta (reorg) returns usage and weight bit for bit; two builders' folds are the SAME state; a receipt quantum touches no round permit/final | `rfc9_c4_the_spent_quantum_is_branch_scoped…` | IMPLEMENTED_AND_TESTED |
| V3 unchanged: a V3 spend is paid with no split; V3+V4 spends of one quantum are one double spend (either order) | `rfc9_c5_v3_is_unchanged…` | IMPLEMENTED_AND_TESTED |
| a `PFS4` header below the fence is refused by name | pre-existing `rfc9_a_pfs4_receipt_header_is_refused_below_the_fence_and_signature_checked_past_it` | IMPLEMENTED_AND_TESTED (not mine) |
| discovery/relay of authorizations | `transport::tests::a_miner_files_its_authorization_and_a_builder_on_another_machine_mirrors_it_into_its_node_directory`; `evidence_cli::a_miner_files_a_redemption_authorization_…` | IMPLEMENTED_AND_TESTED |
| retiring / missing executor and builder bonds | `consensus-core palw_receipt_v4` unit tests (admission items 6'/7') — not repeated at the processor | IMPLEMENTED_AND_TESTED (consensus-core) |
| coinbase maturity | unchanged: the leg is an output of the same coinbase transaction, which matures under the one coinbase rule | by construction (no receipt-specific rule) |
| not exercised: a full chain block that merges a receipt block (accepting block's UTXO), and builder mode against a live node | the existing receipt-lane tests are also at the processor-method level; a planted-tip harness exists (`set_tip_for_tests`) but its history/monotonicity constraints were not worth the risk here | CODE_GAP (E2E) |
| arming plan + T12 drill; fee-cap market test (a builder must find the fee worth taking) | activation gates | EXTERNAL_GATE_PENDING |
| Redemption V4 vs work-slice credit: one claim credited once | EXEC v2 not built | DESIGN_GAP |

## Proposals for Lead-owned interfaces

* **RPC op number.** `getPalwStateProof = 202` — inside this lane's allocation (the Lead's addendum: C1 owns 202–209; 210–219 / 230–239 are lane D's, 220–229 C2's). 203–209 are unused; the tables touched are `rpc/core/src/api/ops.rs`, `rpc/grpc/core/src/ops.rs`, `messages.proto` 1218/1219, the wRPC router/client lists.
* **G-EXPIRY / G-RULESET** (above): a signed `valid_until_daa` and `ruleset_id` in a versioned registration message + fence; or accept client-side checks.
* ~~**`SigningPurpose` for the V4 redemption authorization**~~ — done (SMALL lane, below).
* **kaspad `--palw-evidence-provider-dir` URL providers** (5 lines in `palw_panel.rs`, `ProviderSpecV1`).
* **Tree-shaped PALW state commitment** (state-root version bump) if light-client proofs are to become cheap.

---

## Round 2 — 2026-10-08 (agent C1r2, branch `rfc9/c1r2-verified-remote` from `183a761bf`)

The user's ruling of 2026-10-08 stands throughout: node-less mining stays, a pool is never required, and "not running a full node" is
not "trusting what a node says". Statuses use the matrix vocabulary. **Nothing below ran against a live node or network.**

### Commits

| Commit | What |
|---|---|
| `e93fdff2d` | consensus test `rfc9_v4_chain_e2e` — mandatory test 4 at chain-block level (P1) |
| `4c5fa5f93` | `audit-combined/rfc9-v4-leg.sh` (the drill's fence-4 leg) + `misaka-palw-rfc1-drill-claim --kind plain / fp-certification --prompt-form`, rail `--watch` pass-through of `--evidence-out` / `--redeem-auth-out` |
| `e4655dfbe` | `palw submit-object`: DUPLICATE_CLASS / CLASS_CONFLICT / DUPLICATE_CHECK_UNAVAILABLE before any fee (`--allow-duplicate`) |
| `da3964d30` | `misaka-palw-remote::verify` — L1 / L3 / L2 status, the four mode labels, the signing gate |
| `404491f42` | `palw-remote-miner`: L1/L3 over wRPC (`--verify-headers`, `--checkpoint-trust own-node\|pinned`, `--own-node`, `--accept-unverified-state`), gate before inference and again before the signature |
| `f70120960` | mode label + gate in `misaka model add/sign/submit` and `misaka-palw-fp-rail` |
| `4c6fa1f57` | `misaka bond register-export / register-sign / register-submit` (`misaka-palw-remote::bondreg`) — node-less bond registration |
| `04e446a28` | `track`: an ACK no observer confirms within a bound → rebroadcast the same bytes through another relay |
| `43efde0d9` | remote miner: a pool is only a job service; a template paying anyone but the miner is refused (non-custodial by default) |
| `e65f6998a`, `8239d4572` | `model add --artifact <.palwtir>` (IR class) through quote / export / detached sign / submit; the detached bundle is kind-aware (Class \| TirClass) with ONE marked arm for OB-P0's tag-108; `palw tir-registration` gated (mode + DUPLICATE_CLASS) |

### P1 — mandatory test 4 at chain-block level (fence 4 of `PALW_T12_INT13_FENCES_V1`)

`cargo test --offline -p kaspa-consensus --lib rfc9_v4_chain_e2e` → **1 passed** (~14 min: one ~700-block chain + three replays). Ruleset:
`t12_int13_flag_day_crossing`'s compressed testnet-12 release with the int-13 list (all four fences) at H = 80. Every block is the node's own
template through `validate_and_insert_block`; every PALW object rides a funded carrier; nothing is planted in state.

| Requirement | Shown | Status |
|---|---|---|
| executor's FP claim reaches Final, then the executor is gone | floor FP lane certified on chain (FamilyCertified as 6 ObjectChunks + ClassLaneCertified); claim on a funded 0x4a carrier at DAA 5; material + RDA4 filed with a dir provider and the 127.0.0.1 reference server (read back verified); key/node/process unused afterwards except two marked adversarial/counterfactual probes; Final at DAA 147 | IMPLEMENTED_AND_TESTED |
| panel verification from providers only | each seat `fetch_claim_material_any` against the claim's on-chain roots, `verify_material` with the chain's roots, output root and job pin, then signs; node's assembler; funded 0x4b carrier | IMPLEMENTED_AND_TESTED |
| another bonded builder's PFS4 + RDA4 block accepted | builder mirrors RDA4 from providers (`sync_redemptions_v1`), kaspad builder mode reproduced step for step (`palw_fp_spendable_v3`), node template + algo 7 + PFS4 (> 8,192 B); the receipt block takes the tip (its own fold applies the spend) | IMPLEMENTED_AND_TESTED |
| coinbase: executor's registered payout + builder fee within cap | leg 304,080,417,576 to the executor bond's payout, fee 16,004,232,504 to builder A = exactly 500 bps of the worker reward 320,084,650,080 | IMPLEMENTED_AND_TESTED |
| supply / weight / Panel legs unchanged but the fee | V3 counterfactual twin node (executor spends the same quantum at the same position): totals equal, every other output equal, same safe weight, same claim row | IMPLEMENTED_AND_TESTED |
| second spend refused (V3 or V4, same or sibling block) | late sibling PFS4: one fee and one leg on the selected chain whichever branch the PALW order keeps; V3 re-spend: `StatusDisqualifiedFromChain`, paid nothing; same-mergeset race on the reorg branch: exactly one builder paid | IMPLEMENTED_AND_TESTED |
| reorg reverts the spend, quantum spendable again | a LONGER spend-less branch is refused (PALW order, not blue work); a heavier branch (two other quanta) reverts A's spend and the quantum is spent again exactly once; a replaying node agrees | IMPLEMENTED_AND_TESTED |
| PFS4 below the fence refused by name | `a V4 receipt spend is not valid below palw_receipt_spend_v4` at the header stage; a PFS3 header above 8,192 B still refused past the fence | IMPLEMENTED_AND_TESTED |
| multi-node drill | `audit-combined/rfc9-v4-leg.sh` (plan/dry/prepare/certify/claim/sync/status/verdict) — `dry` smoke-run only | EXTERNAL_GATE_PENDING (lead schedules) |

Findings for the go/no-go: (1) the leg needs **~19.5 h of chain** at ~125 s/DAA (claim DAA 5 → Final 147 → slot 547: the 400-DAA receipt
maturity is not in the plan's ~12 h); (2) on public testnet-12 FP commitments on the floor are skipped until FamilyCertified +
ClassLaneCertified (FreePrompt) are filed — V4 is unreachable there until someone files them; (3) a receipt block is a chain block.
The sidecar `SigningPurpose` for RDA4 was not needed for that drill; it was added afterwards (SMALL lane, below).

### P2 — mandatory test 1: verified chain state, three layers

`misaka-palw-remote::verify` (`da3964d30`), wired into `palw-remote-miner` (`404491f42`) and the CLI/rail labels (`f70120960`).

* **L1 header/DAG** (`verify_header_chain_v1`): from a checkpoint trusted before any node is asked — recomputed header hashes, each
  header naming the previous as a parent, blue work / blue score strictly rising, DAA never falling, no future timestamp, a fresh tip, every
  PALW carriage well-formed and signed (an attempt's challenge recomputed from the header's own position; V3/V4 receipt envelopes), walk
  bounded. Views compared by **containment only** (`merge_views_v1`): conflicting views STOP the client whatever their blue work.
  Templates refused off the verified tip (stale parent, job after a reorg) or with a target outside ×2 of the tip's. Ruleset: network,
  genesis, `consensus_params_id`, `consensus_schedule_id` must be this build's.
* **L3 claim state**: bond / class rows proven (op 202) only against a header ON the L1 chain; a node's job facts contradicting the proof
  stop everything before inference.
* **L2 PALW fork choice**: established only when the facts are proven at a trusted checkpoint (own node, or a verified signed checkpoint)
  that is the decision point. Otherwise `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED`.
* **Labels** `FULL_NODE` / `VERIFIED_REMOTE` / `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED` / `UNVERIFIED_REMOTE` on every output of
  `palw-remote-miner` (each step), `misaka model add/sign/submit`, `misaka bond register-*`, `misaka-palw-fp-rail` (summary, `--track`,
  `--relay-signed`), `palw tir-registration`. Below VERIFIED_REMOTE nothing is executed or signed without `--accept-unverified-state <LABEL>`
  (the named class or a weaker one); the miner re-verifies right before the signature. A loopback `--rpc` is taken as the user's own full
  node. No text claims full-node equivalence below FULL_NODE (tested).

| Attack test | Test | Status |
|---|---|---|
| fake class row / fake bond eligibility | `verify::…l3_refuses_fake_rows…`, `miner::…a_lying_class_row…` | IMPLEMENTED_AND_TESTED |
| wrong target, stale parent, job right after a reorg | `verify::…a_template_off_the_verified_tip_or_with_a_wrong_target…`, `miner::…a_reorg_between_the_gate_and_the_signature…` | IMPLEMENTED_AND_TESTED |
| wrong ruleset / fence schedule | `verify::…another_ruleset_or_fence_schedule…`, `miner::…a_wrong_schedule…` | IMPLEMENTED_AND_TESTED |
| forked view with less (or more) blue work; RPC hiding a competing tip, revealed by a second peer | `verify::conflicting_views_stop_the_client_whatever_their_blue_work` (heavier-blue-work branch alone → HEADER_VERIFIED, gate refuses; both → STOP) | IMPLEMENTED_AND_TESTED |
| proof against an unlinked header; forged rows | `verify::l1_refuses…`, `verify::l3_refuses…` | IMPLEMENTED_AND_TESTED |
| non-canonical state with a correct Merkle proof | proven past the checkpoint → never VERIFIED_REMOTE (`verify::l3_…`) | IMPLEMENTED_AND_TESTED |
| stale checkpoint / stale tip | `checkpoint::verify_signed_checkpoint` (Stale); `StaleTip` | IMPLEMENTED_AND_TESTED |
| restart, another peer: same verdict, no carried trust | `verify::a_restarted_client_re_verifies_from_scratch_whoever_serves` | IMPLEMENTED_AND_TESTED |
| no opt-in → no inference, no signature | `miner::an_unverified_quorum_neither_executes_nor_signs_without_the_opt_in`, `model_bundle::model_sign_refuses_to_sign_unverified_state…`, `bond_remote::register_sign_needs_the_class_named…` | IMPLEMENTED_AND_TESTED |
| the wRPC adapters (`header_chain` via getVirtualChainFromBlock + getBlock, `state_proof` op 202, `ruleset` getPalwNodeStatus) | run against a live 9-node salted drill on the integration build (Lead E2E 2026-10-10, below) | IMPLEMENTED_AND_TESTED (devnet) |

**What a header-only client cannot prove on this DAG — DESIGN_GAP, stated:** GHOSTDAG's selected-parent choice and heaviest-chain
selection (the other parents' headers / mergesets are not served); `bits` against the DAA window (the window's mergesets); heartbeat PoW
(kaspa-pow is not linked into the light path); and, decisively, the **PALW fork choice** (safe frontier, safe weight, live total come from
accepted transactions and the PALW fold — a header cannot carry them; a state proof shows a row under a root, not that the chain carrying
the root wins).

**L2 — DESIGN_GAP, candidate design and the cost of each step** (not measured):
1. *Checkpoint*: a fresh trusted checkpoint (own node, or a signed checkpoint from issuers the user chose). Cost: key distribution and a
   freshness SLA; a stale checkpoint is refused (exists).
2. *DAG/header proof from it*: the selected chain's headers (implemented) plus each chain block's mergeset headers for GHOSTDAG and the DAA
   window. Cost: an attempt header carries ~8 KB of PALW carriage; at ~29 DAA/h a day is ~5–6 MB of chain headers before mergesets.
3. *PALW state commitment*: today a flat preimage — a proof is O(rows) (a bond table with 2.6 KB keys is megabytes), and the fork-choice
   scalars (safe frontier, safe weight, bounded immature) are not an addressable opening (`safe_weight` sits mid-preimage behind Some-only
   blocks). Needs a versioned tree-shaped commitment with a fork-choice opening: a state-root version bump (consensus change, a fence).
4. *Verified state transition to that root*: without re-executing the PALW fold over block bodies (a pruned full node's PALW work) or a
   succinct proof of it, a root on a valid-header branch is not shown to be the canonical PALW state. Cost: block bodies + the fold, or
   research-grade proofs.
5. *Candidate-order check*: `palw_fork_choice::compare_palw_candidates_v1` on the verified inputs — cheap once 3–4 hold; never re-implemented.
6. *Fresh multi-peer observation*: ≥ 2 independent peers, any conflict STOPs (implemented: `merge_views_v1`).

Until 1–5 exist the honest state for a remote miner past its checkpoint is `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED`, with the risk shown
before inference and before signing and a stop path — node-less mining stays (rule 7).

### P3 — mandatory test 2: malicious relay (modify / steal / obstruct)

| Object | Modify | Steal (re-attribute work or payout) | Obstruct |
|---|---|---|---|
| class registration (catalog and IR) | refused from the signed bytes (`bundle::…alters_the_signed_bytes…`, IR: `bundle::an_ir_class_registration_rides…`) and by the node | owner signature binds registrant bond, class, root; payer signature the carrier | resend idempotent (`bundle::resend_is_idempotent…`); DUPLICATE_CLASS before a second fee |
| bond registration (new) | `bondreg::a_relay_cannot_redirect…` (byte flip) | owner signature binds payout + collateral; the chain binds output 0 to the payout's script — a relay moving the payout is refused | `register-submit` re-run sends the same bytes; follows `<carrier>:0` |
| attempt block | a node answering another hash never counts (`miner::…another_hash…`) | the attempt is signed by the executor key and its challenge binds the header position | `BlockTracker`: Lost → the SAME bytes re-sent |
| FP carrier | `relay::…refuses_one_that_cannot_stand` | claim signature binds the executor bond; funding signature | **new** `track::a_relay_that_swallows_the_carrier_is_routed_around_with_the_same_bytes`: ACK with no observer within the bound → rebroadcast through another relay, same tx id, no second fee, one claim |
| V4 authorization / receipt | altered after signing refused at the header stage (`rfc9_c2`, chain E2E) | the miner leg is the chain's bond record, the builder only ever gets the fee (chain E2E, twin) | any bonded builder redeems the published bundle; the first builder gone → another redeems (`rfc9_c1`, chain E2E race) |

Status: modify / steal IMPLEMENTED_AND_TESTED for all five; obstruct IMPLEMENTED_AND_TESTED in the library. The rail's `--track` is
stateless; automatic re-submission through another relay is now wired into `misaka-palw-fp-rail` (SMALL lane, below).

### P4 — mode C (pool) without privilege

No consensus code mentions a pool; a claim is valid with no pool anywhere (every test above). A pool is a job service, relay, provider or
builder: `miner::a_pool_is_only_a_job_service_it_gets_no_trust_and_no_custody` — two "pool" endpoints land the miner in the same class two
nodes would, a pool lying about the class row is stopped the same way, and a template whose coinbase pays the pool is refused before any
work (`MinerHalt::Custodial`; `palw-remote-miner` sets `pay_to` to `--pay-address`). Reward pooling stays an off-consensus contract.
Status: IMPLEMENTED_AND_TESTED (driver). A custodial-pool opt-in does not exist (by design: non-custodial by default).

### Routed items from H1 (2026-10-08)

* **NODELESS_BOND_REGISTRATION_ABSENT** — `misaka bond register-export | register-sign | register-submit` (`4c6fa1f57`): export with no key
  (≥ 2 nodes on this build's ruleset agreeing on the funding output; the floor from this build's own ruleset; fee priced on a body with a
  signature script of the real length, refused past the relay storage-mass limit), offline sign (owner + optional different payer; refused
  on a moved payout / collateral / change / fee / network / expired quote; gate), submit (verify the bytes, relay, follow `<carrier>:0`).
  Tests: bondreg 3 + CLI 1. IMPLEMENTED_AND_TESTED (library + files); no live node.
* **REGISTRATION_QUOTE_INVALID** — DUPLICATE_CLASS before any fee in `palw submit-object` (`e4655dfbe`), `palw tir-registration` and the
  new IR route (`e65f6998a`): `model add --artifact <.palwtir>` → pack gate → class/root → DUPLICATE_CLASS → bond → `--quote` (≥ 2 nodes)
  or `--export-bundle` → `model sign` → `model submit` (the detached bundle carries `ClassRegisteredTirV1`; the one arm for OB-P0's
  tag-108 envelope is marked in `bundle::parts_of`). `hf://repo@rev` explains the conversion it needs. In-process IR signing inside
  `model add` is not built (the route points to `palw tir-registration` + `submit-object`, both gated); `model_onboard.rs` is left to OB-P0.
  IMPLEMENTED_AND_TESTED (bundle 14, CLI operator 82) for the library and the gate; the IR route's node reads are IMPLEMENTED_REFERENCE_ONLY.

### The 8 safety conditions after round 2

| Condition | Status |
|---|---|
| Local signing / full-tx sighash | IMPLEMENTED_AND_TESTED (class + IR + bond registration detached; rail signer socket). V4 authorization `SigningPurpose`: IMPLEMENTED_AND_TESTED (dormant, SMALL lane) |
| Verified chain state / stale detection | L1 + L3 + labels + gate IMPLEMENTED_AND_TESTED (fixtures, fake nodes); wRPC adapters IMPLEMENTED_REFERENCE_ONLY; **L2 DESIGN_GAP** |
| Canonical job/input/output binding | unchanged (FP Job V4 DORMANT_NOT_INTEGRATED) |
| Public authenticated DA | IMPLEMENTED_AND_TESTED (local; now on the chain E2E's panel path); discovery DESIGN_GAP |
| Objective DA/default | unchanged; provider court DESIGN_GAP (out of this lane) |
| Miner-bound payout / V4 redemption | **chain-block E2E IMPLEMENTED_AND_TESTED**; multi-node drill EXTERNAL_GATE_PENDING (scripted) |
| Multi-relay / censorship fallback | IMPLEMENTED_AND_TESTED (library, incl. obstruction); rail auto-resubmit IMPLEMENTED_AND_TESTED (SMALL lane; state machine + rail wiring, fake nodes — not run against a live node) |
| Fresh outsider G14 prosecution | C4r3's (mandatory test 3) |

### Tests (targeted)

```
cargo test --offline -p kaspa-consensus --lib rfc9_v4_chain_e2e                 # 1 passed (~14 min)
cargo test --offline -p misaka-palw-remote                                      # 89 lib + 2 CLI end-to-end
cargo test --offline -p misaka-cli --bin misaka -- operator:: palw_fp::         # 85
cargo build --offline -p misaka-palw-remote --features rpc --bin palw-remote-miner
cargo build --offline -p misaka-palw-gateway --bin misaka-palw-fp-rail --bin misaka-palw-rfc1-drill-claim
```

### Allocations and ids

No RPC op (203–209 unused), object tag, enum discriminant, delta, tail or `SigningPurpose` was taken; RFC-0009's reserved 150–153 /
140–141 / 0xE9 stay unused. No params id, schedule id or fork id moved (test, client and tooling changes only). New dev-dependency:
`kaspa-consensus` → `misaka-palw-remote` (library only).

### Open gaps

L2 (above); the multi-node V4 drill (~19.5 h, the lead's); signed-checkpoint files in the
miner binary (`--checkpoint-trust signed` not taken until a file format and key distribution exist); in-process IR signing inside
`model add` and HF conversion (OB-P0 / H1); provider court (separate lane); every wRPC path is unexercised against a live node.

---

## SMALL lane (2026-10-09): rail auto-resubmit and the RDA4 `SigningPurpose`

Both former CODE_GAPs are closed; nothing is armed (`palw_receipt_spend_v4` stays `None` on every preset, and no consensus id moved).

**Rail auto-resubmit (RFC-0009 mandatory test 2, "obstruct", wired).** `misaka-palw-remote::resubmit` is a pure state machine
(`ResubmitStateV1` in a JSON file, `decide_resubmit_v1`, `step_resubmit_v1`); `misaka-palw-fp-rail --track <claim> --bond <o> --relay <a,b,…>
[--observe <x,y>] --carrier <tx.borsh> --funding-amount <sompi> --resubmit-state <file> [--resubmit-bound <DAA>]` runs one step per invocation
(from a timer): it polls every relay and observer, decides, and — if a relay's ACK has gone unconfirmed past its bound — sends the SAME bytes
through the next relay, rewriting the state file atomically.

| Property | How it is held | Test |
|---|---|---|
| the same bytes, never a re-sign | the state names the tx id; the id of the bytes about to be sent is recomputed and anything else is `CarrierChanged`; the module holds no key | `a_plan_names_the_bytes_and_refuses_other_bytes`, `a_relay_that_swallows_the_carrier_is_routed_around_with_the_same_bytes_and_asked_once` |
| an ACK is a relay's word | confirmed only by an INDEPENDENT observer (not the relay judged, not one already found swallowing); with none, `Blind` — nothing is sent | `an_acking_relays_own_word_and_a_suspects_word_confirm_nothing`, `a_claim_naming_another_bond_ends_the_sending_and_is_never_ours` |
| one liar cannot move time | the clock is the lower median of the answering nodes' DAA | `one_node_reporting_a_far_future_clock_cannot_make_an_ack_look_overdue` |
| bounded | a swallowing/refusing/tampering/unreachable relay is never asked again; at most `RESUBMIT_MAX_RESENDS_V1` re-sends after a carrier that was SEEN vanished; then `Exhausted` (loud) | `with_every_relay_spent_the_plan_is_exhausted_and_stays_so`, `a_carrier_that_was_seen_and_vanished_is_resent_a_bounded_number_of_times`, `a_relay_that_refuses_or_lies_about_the_id_is_spent_and_the_next_is_tried_in_the_same_step` |
| a sent copy cannot be a second charge | one funding input, one tx id; a node that already holds it answers `AlreadyKnown` (a success) | `a_relay_that_swallows_the_carrier_…` (the re-sent copy is the same tx id), `the_state_file_round_trips_atomically_and_refuses_what_it_is_not` |

Status: **IMPLEMENTED_AND_TESTED** on fake nodes (`misaka-palw-remote` 98 lib tests); the wRPC adapters of the rail are unexercised against a live node
(unchanged from §Status above).

**RDA4 `SigningPurpose`.** `SigningPurpose::PalwReceiptAuthV4 = 8` and `SignerMessageDigest::PalwReceiptAuthV4(Hash64)` (borsh index 8) are
APPENDED — every older discriminant and wire byte is unchanged. The digest is `redeem_auth_id_v4` (total over the authorization), the context
`PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT`, reserved to this purpose (no other purpose may carry it; this purpose may carry no other). **Visible only under
`palw_receipt_spend_v4`**: `SigningPurpose::offered_by(params, daa)` is `Params::palw_receipt_spend_v4_active_at`, and a `kaspa-pq-signer` daemon offers the
purpose only when started with `--palw-receipt-spend-v4` (default off; `--deny-purpose palw-receipt-auth` still wins). A daemon that was not told
the network arms the fence refuses the request BY NAME before any key is touched, and `misaka-palw-fp-rail --redeem-auth-out` prints that refusal. No journal,
by decision: the authorization is position-free, non-revocable except by its expiry, and the chain's branch-scoped spent-quantum set is the double-spend
guard (RFC-0009 §6). The authorization body is built by `kaspa_pq_validator_core::build_redemption_authorization_v4_with` over any `MessageSigner`
(the in-process key's builder is now a thin wrapper of it), so the sidecar and the seed produce the identical authorization.

| Test | What it pins |
|---|---|
| `consensus/core` `signing_purpose_discriminants_are_api_stable`, `the_receipt_authorization_purpose_is_appended_and_its_digest_agrees_with_it` | 0–7 unchanged, 8 appended; digest variant index 8; the purpose/digest/context triple |
| `consensus/core/tests/palw_receipt_spend_v4_fence.rs` | offered nowhere a preset ships it (t12, mainnet, devnet, rc), never at `never()`, and from the height only where armed; the params/schedule ids are the dormant ruleset's |
| `kaspa-pq-signer` `the_receipt_authorization_purpose_is_refused_until_the_fence_is_offered`, `the_redemption_bundle_is_built_through_the_sidecar_and_a_dormant_daemon_refuses` | dormant daemon refuses by name; once offered, the bundle verifies with the chain's own check and equals the in-process authorization; context borrowing refused |


## Lead E2E against live nodes (2026-10-10)

Build: integration `0b73fd33f` (kaspad, misaka) and the miner with the drill flags below; a 9-node salted testnet-12 drill on one Mac
(`tests/hf-onboarding/devnet.sh`, RUN_ID=lead2), two of its nodes as `--rpc`, bond 8 registered by the HF onboarding run, class
`f1c5635c…` (registry Active). The executor is `/usr/bin/false`: everything up to inference runs against the nodes, and no execution
result is fabricated.

| Run | Result |
|---|---|
| public-t12 miner → drill nodes | refused before inference: `consensus_params_id` differs (and, with this change, the genesis) |
| `--verify-headers` from the genesis checkpoint | L1 header chain verified over wRPC; `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED`; STOP without the opt-in, nothing signed |
| `--own-node --checkpoint-trust own-node` | `FULL_NODE`; every gate passed; halted at the executor (exit 1) |
| `--pin <sink>` + opt-in | op 202 state proof: bond proven `Active`, collateral 3,119,045,986,560 sompi; halted at the executor |
| a class the model registry holds `Candidate` | the node refuses the template by name (no new claims) |
| wrong drill salt | refused: another genesis |
| `--palw-drill-ruleset` without a salt | refused at start-up |

Changes made for it (`palw-remote-miner`): `--palw-drill-genesis-salt` (params from `palw_chain_params_v1`, so every signature is under the
drill's domain) and `--palw-drill-ruleset <params id>:<schedule id>` (a drill compresses its fence schedule through kaspad flags this
binary does not re-derive; accepted only with a salt). The node ruleset now carries `getPalwNodeStatus`'s `genesis_hash` — before, the
miner never compared genesis (`genesis: None`) and relied on the params id alone.

Not covered (unchanged gaps): a real executor (the backend is not extracted), signing a winning attempt, submit and the Panel's replay of
a remote miner's capture; L2 below `FULL_NODE` stays a DESIGN_GAP.
