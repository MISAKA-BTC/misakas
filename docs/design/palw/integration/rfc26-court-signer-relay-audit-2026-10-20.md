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

T08 still needs actual funded third-party carriers through mempool, block acceptance and native fold, original custody/credit admission with real signatures, source-holder and signer availability over all turns, signed dependency horizon, failure/recovery behavior and restart/reorg. The all-owner outsider path, physical inclusion supply and default attribution remain separate unfinished P0 conditions. A funded relay of complete evidence cannot substitute for obtaining the evidence under collusion.
