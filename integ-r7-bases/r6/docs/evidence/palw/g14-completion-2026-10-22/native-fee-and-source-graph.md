# Native carrier fee conservation and source material graph — 2026-10-22

Status: **isolated implementation; selected verification passed; G14 incomplete**. No production preset, price, deadline, wire tag, rooted schema or activation was changed.

## Native fee correction

A released/slashed bond spend can also carry a lifecycle ObjectChunk opening a pending group. Previously its actual fee `F` was separately tested against mandatory bond burn `B` and object rent `R`. With `B=R>0` and `F=R`, both individual tests passed although `F<B+R`; collectible fees were clipped to zero, so only `F` could actually be destroyed. The object gate could therefore regard the same money as satisfying both obligations.

The native accepted-transaction loop now retains its existing raw actual-fee map and separately records `F-B` from that same validated transaction and the same bond-burn obligations. Priced PALW gates read the remaining amount. The carrier's ordinary transaction may remain accepted while an underpaid PALW object is refused, preserving the existing separation. Missing priced entries still supply zero; the existing unpriced sentinel/fence selection remains. No caller `paid` flag, new money, escrow balance or fee schedule is introduced.

Bridge finality classification also uses the exact collectible amount added to total fees, rather than including destroyed bond/rent money. This preserves the existing bridge and activation conditions while keeping finality fees a subset of collectible fees.

The new regression uses a typed retiring/slashed bond fixture, real payer MLDSA signatures, the native UTXO validator against a fixture UtxoCollection with full script validation and bond filter, actual lifecycle codec/extractor, production fee recorder/selector and PALW acceptance. Its six inner cases are one runner test: underfunded combined obligations, exact combined payment, zero burn, an explicitly unpriced helper selection, and bridge/nonbridge collectible residuals. This is not a mined released-bond lifecycle. In particular the unpriced case selects the helper sentinel explicitly; the processor in that fixture has its rent fence active.

The separately selected existing carrier test uses temporary TestConsensus nodes, test-only parameters and skipped PoW. It creates funding coinbases, signs real transactions, builds/inserts blocks and reads PALW state and reward results for rent equality, one-short payment and a disabled rent fence. It supplies native pipeline evidence, not a live-devnet saturation, model latency or profitability measurement.

## Native source-only graph

The private-owned v3 component derives all seven source query families from the original native frame/source. It derives logical request, role-specific Begin/Part/Finish and fresh-response nodes, sharing FullIds streams across ID queries. All allowed queries are enumerated; there is no peer-selected subset constructor or graph deserializer.

Allocation precedence is separate from runtime completion. A response may use authenticated public material completed by another signer. A short declared body leaves unused maximum tail slots, rather than waiting for nonexistent parts. The native part-cursor rule is reused. Role-slot counts remain capacity bounds, not actual Ready/current-authority consent.

Clock-free logical IDs bind source/resource/rules/profile/grammar. Actual allocation scope separately determines absolute packet IDs. Projection checks reject missing/extra packet IDs, changed retained packet spelling, another acceptance's book and windows outside the original service interval. Reservation and repair clone, verify the complete source projection and commit only on success. Native transport bounds are conservative uppers; ragged geometry is checked against an independent per-query stage count rather than assuming the upper is exact.

This type explicitly covers **SourceMaterialOnly**. Packet costs and detailed windows still require native derivation/calibration. Numerical court branches, parameters, semantic duties, role consent, capital, payments and physical costs are not closed by this source-only graph. No new rooted row or original installer is added. Future persisted graph validation must rederive from the native original rather than trust stored fields.

## Verification

[Exact commands, source hashes, reconstruction and executable metadata](native-fee-source-verification.json) record the verification phases. Changed tracked sources reconstruct from the base commit and preserved patch; added sources are preserved separately. Results are reported after the corresponding commands finish.

The first native regression [failed its fixture funding assertion](native-fee-source-test-native-bond-rent.log) before validating the spend: the existing generic bond fixture did not cover the existing chunk rent. Its original test source and executable remain pinned. The repair derives the fixture outpoint value from the existing `R` and maximum tested fee, retaining at least its original value; the actual fixture UTXO and typed bond use the same amount. No price, global limit, production gate or assertion was relaxed. [Repair commands and exact source](native-fee-source-repair-verification.json) record the separate rebuild. [Two existing native tests](native-fee-source-existing-tests.json) passed on the first pinned executable; reruns are not additional unique tests.

The completed selection passed **36 unique tests**: 33 core tests (source graph 4, fee 2, funding 4, funded envelope 4, legacy envelope 5, transformer identity 9 and activation 5) and the three native tests below. Repeated native runs are counted once. Both the initial core/consensus test typecheck and the repaired consensus test typecheck passed; release executables were rebuilt and pinned with hashes. Runtime sources match the initial run; only the regression fixture changed for the repair.

| Native test | Result | Evidence |
| --- | --- | --- |
| Combined bond burn and rent, six inner cases | 1 passed | [log](native-fee-source-repair-bond-rent.log) |
| Actual UTXO carrier fee through block processing | 1 passed | [log](native-fee-source-repair-funded-carrier.log) |
| Refused chunk consumes no grading slot | 1 passed | [log](native-fee-source-repair-grading-slots.log) |

These are selected tests, not a new whole-repository or whole-G14 campaign. [Native wire generator check](native-wire-idempotent.log) is recorded separately.

[Document check](docs-check.log) remains FAIL with the same 43 baseline occurrences and five imported missing references; this checkpoint adds no authored missing reference. Native wire generation is idempotent (exit 0).

## Remaining financial integration

The real escrow adapter must prepare a deterministic transition plan against the same selected-chain state/accepting point **before** UTXO acceptance, then apply only accepted transaction plans atomically with PALW state. Current PALW object refusal occurs after ordinary carrier UTXO acceptance; appending an escrow debit there could leave an actual spend without the corresponding accounting, or the reverse. Qualified native per-input authorization must bind the exact transaction/entries and contract; txid alone excludes signature-script/sigop/mass fields and is insufficient as a reusable authorization cache key. Ordinary input validation and standard signature caches must remain separate.

No actual escrow lock, qualified third-party payment/refund/change, full original duty graph, F02 supplied opportunity/default, complete D2 C1 path, monetary pruning, physical profile, whole T01–T15 campaign or real-model door <=3,120 is completed by this checkpoint. Earlier pruning repair remains separately pinned evidence.
