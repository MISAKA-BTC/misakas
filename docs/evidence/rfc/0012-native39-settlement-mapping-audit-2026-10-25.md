# RFC-0012 / native39 settlement mapping audit

Status: read-only source and contract audit at `da8edb76b`. No implementation, policy adoption, test run or campaign PASS is recorded by this note. The node/Core build in progress is unrelated execution evidence.

## Existing contract

[RFC-0012 R12-SETTLE](../../rfc/0012-palw-only-consensus-and-native-evm-settlement.md#r12-settle--palwsettlementpolicyv1-parameters-and-invariants), lines 489–503, requires unique `(canonical work id, quantum index)` from the transition-carrying PALW delta, excludes carrier copies/heartbeats/floor/unspent FP grants, retracts convicted work and imposes the complete retention/retirement/consumer-liability horizon. Its user-policy amendment at lines 598–607 requires claim-level SecurityCertified credit; Final alone is insufficient. These general requirements are present.

[The native39 operating contract](../../design/palw/tir/launch/native39-operations.md), section 4, specifies a `'V'` mining-claim Gate-C subject, SecurityCertified receipt issuance, and one-use `PNR9` receipt-quantum spending. A private job's Final grants neither weight nor coinbase entitlement. Section 1 keeps activation dormant and refuses ordinary arming. Section 5 explicitly retires model-level examination. This audit does not revive the older model-conformance path.

The existing native39 sources define the following facts, but do not define their complete translation into RFC-0012 settlement facts:

| Required settlement component | Existing native39 fact | Remaining mapping boundary |
| --- | --- | --- |
| Subject / unique identity | `NativeMiningClaimRecordV39.id`, receipt id, one-use quantum bitmap; Gate-C subject `'V'` | The RFC-0012 evidence reader currently expects a `PalwClaimStateV2` claim id, not this aux-only mining subject. No explicit v39-to-settlement unique identity contract was found. |
| Work weight | Canonical native inference work, `NativeMiningPriceV39.credited_mac` / `quantum_mac` / `quanta`; a PNR9 spend draws a budget-clipped F grant | RFC-0012 W is measured in canonical PWU. Neither receipt quantum count, MAC count nor granted Final weight is explicitly declared to equal W. This audit chooses none of them. |
| Anchor | Mining claim `admitted_daa`, Master-established time, and the actual PNR9 carrying block | No exact settlement anchor/accepted blue-score selection for this subject was found. Registration, certification, receipt issuance and spend are different events. |
| Operator / class | Claim/receipt `producer`, `operator`, `model`, `master` | Operator identity exists; concentration-class identity for the settlement W denominator is not explicitly connected to the v39 model/Master fields. |
| Retention / maturity | `palw_native_mining_liability_until_v39` derives certification lifetime + receipt lifetime + prosecution tail; component jobs retain material | The exact RFC-0012 maturity clock must combine these with accepted/M-maturity/retirement and the transition's anchor. No explicit complete v39 settlement formula was found. |
| Void / retraction | Mining conviction removes the receipt, revokes recorded granted safe weight, closes the budget and marks the claim closed; kind 115 is convicted and 116 tracks granted weight | The existing RFC-0012 extractor retracts legacy `Claim Final→Voided` ids. It does not translate this v39 conviction and previously spent quantum identities into its fact retractions. |

## Actual source boundary

`consensus/core/src/palw_native_mining_v39.rs:856` defines the mining claim; `:907` defines its receipt; `:71` derives the liability endpoint. `palw_native_mining_fold_v39.rs:652–718` validates and spends a certified PNR9 quantum, adds the budget-clipped grant to `safe_weight` and journals receipt/granted rows. `palw_native_mining_court_fold_v39.rs:350–394` retracts that granted weight and receipt on conviction. These are actual separate consumers, not missing native39 implementation as a whole.

`consensus/core/src/palw_native_settlement_v1.rs:124–139` defines the settlement evidence as legacy Attempt Finals, FP spends, legacy void ids and certification ids. Its extractor at `:175–187` recognizes certification transitions, but candidate conversion at `:360–362` requires `rules.state.claim(id)`; native39 mining claims reside in aux rows instead. Its legacy Attempt conversion at `:367–406` derives PWU, accepted anchor/operator and maturity from `PalwClaimStateV2`. There is no native39 branch in that extractor/converter. Thus a `'V'` certification id cannot silently become useful settlement work merely because it appeared in table 136.

The actual native39 fixture `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/model_conformance/flat_native_v9/funded11/owning39/master/node_e2e.rs:458–482` already drives node-drawn mining-claim approvals and receipt issuance. Its subject is not a legacy native Attempt, so it is not by itself a positive actual EVM Safe/Finalized fixture. Older nonBASE EXEC tests separately retain trace/admission/conformance seams and cannot substitute for current accepted claim-level work by their names.

## Conclusion and limits

The reviewed current RFC-0012, RFC-0002 and native39 operating/source contracts supply general certification/conservation rules and several required component identities, but an explicit complete native39→RFC-0012 fact mapping was not found. The production settlement consumer currently has no such branch. Implementing one by choosing receipt quanta, MACs, F grant, anchor or maturity semantics would make an unapproved policy decision. This audit therefore records the concrete missing contract/consumer boundary without adding one.

No BASE relabelling, planted frontier/fact, model review, new public policy, arming value or made-up K3 input is used. Actual nonBASE approved-work→matured evidence→native EVM Safe/Finalized remains open; K3, the 15-gate campaign and historical-signature replay remain UNKNOWN/unexecuted.
