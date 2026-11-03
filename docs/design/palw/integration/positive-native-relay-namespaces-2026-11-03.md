# RFC-0046 native relay namespace inventory — 2026-11-03

Implementation inventory under accepted [RFC-0046](../../../rfc/0046-positive-native-funded-relay.md) / [ADR-0214](../../../adr/0214-positive-native-funded-relay.md). It does not activate a network, migrate old funds or establish native R custody. Fixed contract/native codec, field-inventory and logical-allocation tests pass; the actual fee/custody/binding owners are still missing.

## Wire and fixed rows

Global lifecycle object remains176. Append-only local operation14 is a separate bounded body and needs its own actual fired fence before decoding. It is not added to the existing0–13 Assignment decoder. Body14 is `(2u16, 1u16, 14u8, FundingRelayTermsV2, purpose_signature[4627], relay_signature[4627])`. Terms have maxima1087 bytes and body10346 bytes; target-specific terms lengths are895/1015/955/1087 bytes. Existing Result/Case/consent/projection/transaction identities do not change.

Aux151 retains local kinds0–18 and appends:

| kind | exact key after `(2u16,1u16,kind)` | fixed value DATA | maximum value bytes including the5-byte header |
| --- | --- | --- | --- |
|19 FundingRelayAuthorization | funding outpoint68 + purpose1 | full terms, payer key, purpose signature, actual-origin/material/quote/charge references, full original metadata policy/fees/physical/quote |10043 |
|20 FundingRelayCustody | funding outpoint68 + purpose1 | terms ID, relay signature, material/reservation references, retention clock, optional exact binding |6031 |

Both keys are74 bytes, below the existing142-byte ceiling; each value is below16384. Purpose tags are PosterA0 / CaseC1 / ProverJoin2. Relay template tag0 is `NativeOp12NoCaseNoReceiptsV1`. Target tags0–3 retain the accepted field order. These tags are independent of global object numbers and old aux136 certification prefixes.

Original metadata preimages are retained in row19 so a later policy change or historical raw pruning cannot silently reprice a live package. Structural validation recomputes the stored quote using those original preimages and the original R acceptance clock. Its usage remains DATA: native must independently derive actual construction/staging/read work. A matching quote, origin reference or stored signature is not actual fee allocation, custody or accepted ancestry.

The pair reconstructs and hashes the exact full literal body, including both signatures. Substituted terms, signature bytes, quote/profile, purpose, outpoint or continuation links fail its checks. The structural restore reader requires both rows and rejects another purpose for the same exact outpoint. It authenticates the purpose consent only; the actual native backend must still validate the ordinary relay signature against the actual registered key/expected entry, origin lineage, fee burn, real custody and binding history.

Binding is absent at R registration. Only actual F/S acceptance may install its actual origin, original funding outpoint/contract, predecessor and account-slot reservation. A DTO with these fields supplies no such capability. No registration deadline automatically removes a current package or unlocks its account.

The raw-material key uses suffix `0x4c` under `DatabaseStorePrefixes::PalwStateV2Tip`, followed by the 64-byte relay material digest. The UTXO commit stages the literal lifecycle payload at that key when the final delta and actual admitted carrier supply a matching new registration. Accepted origin is scoped to the canonical rows and acceptance record. Cold replay still requires an authenticated branch and custody reader.

## Data identity domains

All identity hashes use canonical Borsh streamed into keyed BLAKE2b-512. The purpose signature uses the separate context below. These hashes/signatures authenticate data and authors, never model computation correctness.

| domain/context | exact preimage/purpose |
| --- | --- |
| `misaka/positive/funding-relay-terms/v2/r1` | full fixed terms; excludes signatures, R txid, accepted origin and local lease |
| `misaka-positive/funding-relay-register/v2/r1` | purpose signature over terms ID |
| `misaka/positive/funding-relay-material/v2/r1` | full literal body14, including both signatures |
| `misaka/positive/funding-relay-policy/v2/r1` | complete finite metadata policy |
| `misaka/positive/funding-relay-physical/v2/r1` | complete finite physical profile |
| `misaka/positive/funding-relay-fees/v2/r1` | complete finite fee schedule |
| `misaka/positive/funding-relay-quote/v2/r1` | complete quoted identities, usage, retention and fee |
| `misaka/positive/funding-relay-ruleset/v2/r1` | zero-extended32-byte actual fired-Params ID and metadata policy ID |
| `misaka/positive/funding-relay-template/v2/r1` | fixed native template recipe below |

The template recipe is219 canonical bytes: contract version/revision, template tag, input index0, tx version0, sequence0, sigop1, lock0, lifecycle subnetwork, gas0, exact77-byte payload with a zero Assignment placeholder, Assignment byte range8..72, sighash0x82, ordinary transaction signature context and the explicit P2PKH entry-script recipe. Outpoint/amount/key are parameters authenticated by each actual normal signature and R terms. The independent Python vector pins template ID `96c791c8e6fe04de8adc2f126d1d7b00b0fd5e187ca458168aa1f029eb35cb38d745439eda286425f6969ceea8e58ab91562f80b350dd3451e399ece722c8eac`. That digest is not a computation certificate or a future UTXO.

The ordinary transaction signature remains `kaspa-pq-v2/tx/mldsa87`; NONE|ANYONECANPAY remains0x82. Current-input amount/script/outpoint, sequence/sigop, version/lock/subnetwork/gas and full payload remain authenticated. Outputs and foreign fee inputs still need actual native money qualification and normal Full scripts.

## Audit scope and remaining owners

`relay-row-collision-audit-20261103.json` and `relay-domain-collision-audit-20261103.json` in the integration handoff record1,450 visible local/remote refs and the exact searched implementation scopes. No conflicting match was found. These checks do not establish public activation or a running network's state.

The separate Some-only Params selection commits full metadata policy/fee/physical preimages and template identity. All published presets must remain None. Current validators still refuse activation while the native fee/custody/binding/restore owners are missing. Original Assignment profile routing, pre-coinbase collectible-fee allocation, live R custody reservation, prior selected-parent F lookup, conditional R2 continuation and terminal actual settlement remain required. Structural pair/codec success and a declared logical allocation lease do not satisfy those owners or an enforced OS quota.
