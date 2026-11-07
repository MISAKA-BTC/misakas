# RFC-0026 F04 — court signer and third-party relay

Status: implementation audit / incomplete T08, 2026-10-20. [RFC-0026](../../../rfc/0026-court-policy-master-contract.md) and the [handover P1-3](codex-handover-court-rfc14-2026-10-20.md) govern. No delegation, new signature method, funding-key transfer, fence or activation is introduced.

## Existing accepted-control path

`PalwTirPublicControlV1::signing_message_v1` binds network, claim, allocation/source identities, target, expected turn, literal signer, declared rent and action. `validate_native_signature_v1` resolves the signer's registered bond public key. Native `apply_control_inner_v1` verifies that real ML-DSA signature before `apply_verified_control_v1` checks the action-specific role, source/turn and paid-resource rules. A different tx carrier does not change the signed duty's answerer.

The lifecycle extractor reads the signed object from each carrier payload. Its bond-registration/input binding checks apply to their own object families; they do not turn a TIR control's relayer into its literal court signer. Virtual processor rent handling remains a separate carrier fee requirement. Passing extraction is not UTXO validation or proof that fee funding exists.

## Roles and limits

| Role | Current TIR custody/control representation | Limit |
| --- | --- | --- |
| Material holder | Original producer pledge plus exact credited custody grants and prepared source descriptors | A source descriptor does not prove continued availability or authorize another key to answer |
| Fresh response signer | `control.signer`, native bond key and the action's literal answerer/owner role | Adaptive new demands require fresh signatures from the authorized signer |
| Relayer | Outer lifecycle tx sender, with unchanged signed payload | Must fund a valid carrier and fees; cannot alter source, turn, response or network |
| Liable party | Original/credited qualified custody signers and native default/fraud rules | Relayer identity does not transfer collateral or make a missing service opportunity slashable |

Current control signing uses the registered bond key. This audit does not authorize transferring a producer's fund-movement key to a provider. A separate provider answers only within its own actual custody role, using its own required key. Availability of the authorized court signer for the whole duty remains a profile requirement; custody of bytes alone is insufficient.

## Real-signature presigned-response test scope

The `court_relay_presigned_custody_response_uses_literal_signer_not_carrier_inputs` case starts from the existing verified-half original/acquisition world. It substitutes a real registered ML-DSA key for the response signer, signs Begin/Part/Finish before relay, and drops that signing key before any relay operation. Each exact signed object is extracted from two different illustrative carrier-input identities and passed through real `apply_control_v1` signature verification and native response processing. Different carriers must reach the same root; a wrong network must refuse without state mutation; Finish must authenticate the original committed leaf body.

Only the response controls use real cryptographic signature verification; the original fixture retains its earlier verified-half assumptions. The illustrative carrier inputs are unfunded and are not submitted to mempool/block UTXO acceptance. This isolates already-signed payload relay, not full node T08, live miner-PC shutdown, complete original signature/credit admission, carrier fee payment or new adaptive responses after the signer becomes unavailable. No new unsigned terminal evidence rule is inferred from it.

## Remaining closure

Complete TIR-original T08 still needs funded third-party carriers through its mempool, block acceptance and native fold, original custody/credit admission with real signatures, source-holder and signer availability over all turns, signed dependency horizon, failure/recovery behavior and restart/reorg. The all-owner outsider path, physical inclusion supply and default attribution remain separate unfinished P0 conditions. A funded relay of complete evidence cannot substitute for obtaining the evidence under collusion.

## Funded legacy-node relay scenario

`lg14a_a_newcomer_funds_and_relays_another_bonds_signed_answer` extends the existing isolated LG14-A node harness. The honest legacy claim is actually produced, bound and licensed, the newcomer is registered on funded carriers, and a real signed reservation and binding demand create the producer's disclosure duty. The existing response builder is split into preparation and submission, preserving its old producer-paid wrapper. The new case signs the answer as the producer first, then uses only newcomer-funded outer carriers through mempool validation, the node's template and the subsequent native block fold.

Assertions cover unchanged signed payload in carrying blocks, newcomer funding rather than producer funding, unchanged producer collateral/funding, duty discharge and the independent filer's authenticated binding read. This is the legacy MaterialDisclosedV2 path, not TIR original/TFP1 acceptance or full T08. It uses the documented LG14-A harness differences (isolated test genesis/cards, skipped PoW and directly configured dormant legacy fence); no network preset or live fence is armed. It does not represent an external process PC shutdown or guarantee fresh adaptive signatures after signer loss.

The funded legacy test passed, as did the existing producer-paid non-seat pursuit case on the same freshly built node test executable. Its exact evidence scope and logs are in the [release evidence record](../../../evidence/palw/g14-completion-2026-10-20/README.md). Thus funded legacy third-party carriage has direct node evidence; TIR original/TFP1 and complete T08 remain open.

## Real UTXO and full-IBD extension

The same funded legacy-node case now queries `get_virtual_utxo_entry` directly for the producer's original output, the relayer's consumed input and its new change. It compares amount/script of remaining outputs to the expected funding chain, rather than relying only on the harness map. A fresh `TestConsensus` node receives every selected-chain block from genesis via the normal block acceptance path. Both the consensus state root and the same funding/duty predicates are checked there. A new verifier, starting with no learned binding, reads the accepted relay evidence again.

This extension covers full block replay inside the existing in-process node harness. It is not process shutdown/reopen, network sync, pruned snapshot import, the proposed service ledger's IBD or TIR original/TFP1 acceptance. It must not be substituted for those remaining T08/T14 requirements. Earlier evidence sections that did not assert direct UTXO/full IBD describe the earlier commit's narrower test.
