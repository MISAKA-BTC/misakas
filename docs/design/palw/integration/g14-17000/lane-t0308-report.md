# lane T0308 — T03 forgery rejection / T08 offline miner, distinct signer, relay (2026-10-26)

Branch `claude/g14-t0308` (from `2d7f6023f`). The slice is SPINE's: the Panel kernel route (`Mode::Panel`, every seat colluding), the class card 1 registers in `canonical::world`, and the single-prompt decode job. No cryptographic proof of computation is used. Signatures only authenticate who said what. A response is judged against the claim's own committed values. Unavailable material is never treated as a computation-error conviction. Model tiers and per-claim lottery approval are unchanged. Wire tags, existing IDs and existing DB rows keep their meaning.

## Defect found and fixed (T08 / P1-3: an old signature replayed as a new response)
- The kernel route object's signature binds only the network, the signer and the object bytes (`palw_kernel_route_message_v1`). Any third party can re-carry an old signed `Respond` in a new carrier.
- A rejected response costs its signer `dismissed_proof_fee` (`KernelLedgerV1::respond`). So replaying the same signed junk burned that signer's fee once per replay.
- Fix in the fold (`palw_kernel_route_fold_v1.rs`): new aux table **161** `PALW_KERNEL_REJECTED_RESPONSE_TABLE_V1`.
  - Key: H(signer, claim, stage, position, bytes). Row: the DAA of the first rejection (u64).
  - A key that is already present drops the object before the ledger classifies or charges it.
  - The key leaves out the demand's filing DAA, so an old signature is also not "new" for a later demand.
  - A valid response needs no marker: the position stays served, and a repeat is refused with `already served` / `no open demand`.
- The scanner arm was added to `wire_kernel_aux.rs`, and to the generator `gen_rfc06_native_wire.py` with identical output.
- Residual: markers are not garbage-collected. Their growth is bounded by rejected responses, and each one paid a fee. Table 161 must not collide with any table number the parallel lanes add.

## Tests (`consensus/src/.../g14_kernel_route_e2e/canonical/t0308.rs`; native: mempool → template → fold, ML-DSA-87, replay node `assert_same`)
- `t0308_t03_slice_forged_signatures_fake_completion_and_fake_responses_have_no_effect`
  - (a) Panel completion receipts with forged signatures.
  - (b) A response under the producer's bond, signed with another key.
  - Both (a) and (b) are refused. The claim row and every bond's money are unchanged.
  - (c) A third party's correctly signed response with junk bytes: it is rejected, only its signer pays the fee, the demand stays open, and the producer is not relieved.
  - (d) Replay of (c): dropped, with no second fee. This is a negative control for the fix.
  - Positive controls: the real receipts complete the Panel, and the silent producer defaults at the deadline with only the fixed penalty.
- `t0308_t08_slice_offline_miner_distinct_signer_and_third_party_relay_answer_in_time`
  - The miner is offline. A distinct bond (not the producer, not a seat, not the demander) signs, and an unrelated relay pays for the carrier.
  - Service lands before the deadline. Neither the signer nor the relay is charged or credited.
  - A replay of the same signature is refused and moves nothing.
  - No default happens. The claim reaches Final and is paid once.
- `t0308_t08_slice_a_late_relayed_response_does_not_undo_the_default`
  - A correct response relayed after the deadline serves nothing, and the default stands.
- The fixture's separate keys only show distinct signers. They are not evidence of operator independence.

### Results
- Command: `buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline --no-fail-fast -p kaspa-consensus-core -p kaspa-consensus --lib -- t0308 aux_ t03_forged t08_miner g14_canonical_panel`
- consensus-core: 18/18 pass (aux registry included).
- consensus: 11 passed, 5 failed, exit 101.
  - All 3 new tests pass, as do the existing campaign tests `t03_forged…` and `t08_miner_offline…`.
  - The 5 failures are existing Panel conviction asserts at `canonical.rs:676` and `:1158` ("the colluding seats are not charged"). This is the seat-charge issue SPINE already reported in §5, which the coordinator has to decide. It is unrelated to this change, which only touches rejected responses.
- `check_rfc06_native_wire_idempotent.py`: the generator output matches the hand-edited `wire_kernel_aux.rs`.

## Authority (current duty) on this route
- A kernel route object verifies only under the **current key** of an **Active** V2 bond (processor preflight).
- `Respond` is accepted from any registered bond. The relay is not an actor.
- The F02 `palw_court_current_signer_key_v2` (rotation-aware) is the shared-original court's authority. Delegation to a designated court signer (`K::DelegateDuty`) is not needed on this route, because the route already accepts a distinct signer.
- The campaign test `t08_distinct_court_signer_under_current_duty_authority_and_relay` is blocked waiting for that delegation. On this slice, `t0308_t08_…` covers it instead.

## Proposed connection to the spine (spine.rs not edited)
- Add after S3c: `Stage::Run`-equivalent stages "T03 forged/fake objects before effect" and "T08 offline miner + distinct signer + relay + no replay".
  - Calling these 3 tests' bodies from the spine needs them exposed as `pub(super) async fn` (a small change to t0308.rs).
- To run them on an IBD / pruned-import node as well, use the `start_node` path from `canonical` (not yet done).

## Remaining work
- The response deadline of the adaptive semantic duties on the shared original (F02/F03 tag 178) and its replay protection. That path uses the separate `SignedSharedDataResponseV1` (bound to request_id/query_daa).
- Reorg across the replay marker (S4) and on a cold node.
- Central commit: none. The changes are in the fold, the aux scanner and the generator. The coordinator should decide whether they count as central.
