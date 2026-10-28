# Essential next native escrow connection — read-only proposal

Rechecked current native-funded-coordinator-next.md, funded-preflight, real UTXO loop, script validator and commit boundary. No repository/source/build/RPC/node/transaction changes. RFC-0035 A authorizes independent actual escrow; this memo does not select a price/profile or activate it.

## The smallest substantive next implementation

Connect one actual accepted TFO carrier, with its native-resolved input entries and full transaction identity, into a **native core funded-original candidate check** inside the actual pre-object builder. The check must reach registered source, source graph, numerical geometry and parameter cover, then report concrete Missing when native profiles/full branches/costs are unavailable. Keep the funded installer CLOSED. This materially connects native authority and source derivation; another caller-DTO-to-opaque-preflight wrapper does not.

Concrete changes to prepare after measurement/source freeze:

1. `utxo_validation.rs::calculate_utxo_state`: only after native validation, capture TFO transaction/actual entries/calculated_fee together with exact accepting point and branch parent identity. Keep raw fee F unchanged and reuse F−mandatoryBurn for rent. Capture only actual accepted set, not every decoded peer transaction. Use retained native allocation ownership for copied entries/body or borrow/share the actual block-store owner; an unmetered deep clone is not acceptable.
2. `processor.rs` carries those native-owned facts into the SAME object-fold/acceptance operation. Resolve by full body hash plus txid (not txid alone: sigscript/sigop/mass are excluded). The capture's private constructor must remain at the actual native validation boundary. Do not accept public supplied `PopulatedTransaction` entries as authenticated facts.
3. `palw_accept.rs` TFO dedicated refusal becomes a call into the dedicated native candidate check, followed by continued refusal/Missing. Do not insert literal inner TFP1 into the old extras map/FP extractor, and do not return ordinary FP or funded state on that path. An ordinary script-valid carrier can remain UTXO accepted with its fallback output, even when candidate funding fails.
4. A new private child of `palw_state_v2` should use the actual TransitionBuilder pre-object state, ctx and resolved native extras. Reuse `palw_tir_public_fold_v1::{original_plan_v1,prepare_original_v1}` rather than duplicate policy arithmetic. Source/registered class/layout/current producer key/class-registration-derived demand must come from that exact state. Reuse native source graph / numerical geometry / parameter cover constructors. The public result is Invalid / Missing / Unavailable diagnostics, not an authority/complete/paid bit.
5. The actual paired-object native resource checkpoint (`objects.rs::apply_object_with_pair_v1`, checkpoint before inner writes) contains the new core work and any future claim/frame/funding writes. Test rollback root/reservations/frames and resource failures through the actual entry, not only pure helper tests.

### Important authority nuance

`ValidatedTransaction` alone does not record whether the invocation used Full or SkipScriptChecks. `calculate_utxo_state` currently chooses SkipScriptChecks on the selected-parent leg; `tx_validation_in_utxo_context.rs` explicitly skips script execution under that flag. Thus an adapter must not relabel every generic ValidatedTransaction as freshly Full-script-authenticated. Preserve/resolve native validation provenance and recheck the exact TFO inputs through the existing ordinary script-policy path where necessary; do not invent a peer-supplied “signature checked” boolean. This memo does not assert an upstream signature vulnerability: it identifies a missing provenance boundary for the new stronger claim.

A meaningful next native test uses a real MLDSA ordinary input and real UTXO view → Full native validator → actual candidate check → registered graph derivation → explicit MissingNativeProfile/CanonicalBranches, leaving FP/state unchanged and ordinary fallback spendable by its payer. Bad input/signature never reaches candidate; bad inner consent produces funded-object refusal without rewriting the ordinary transaction fee or freezing fallback. This is a native integration test, not successful escrow admission.

## What true actual-escrow completion additionally requires

### Rooted lock (not currently installed)

The principal is a concrete UTXO output, not deposit exposure or carrier fee. A new append-only namespace/rooted row must bind original claim/execution/source/full graph/profile, payer's new consent, acceptance interval/material horizon, current escrow outpoint, exact current principal amount, immutable settlement rights and per-purpose spending/remaining accounting. Row creation checks the exact actual output exists unspent in the composed native view at the final same-state-root admission point. It must participate in existing delta root/carriage/load validation/undo/pruning retention; a separate local balance map is insufficient.

New funding is distinct from spending an already locked escrow. Same-mergeset own-output provisional exclusion cannot become a permanent lock when the full original fails; conversely installing a lock only after the whole UTXO walk must not miss an earlier same-mergeset spend. This requires an accepted-order transactional candidate/escrow state view or joint-plan validation capable of declining the funded object when its own output was already spent. It must never lock a pre-existing third-party output because an invalid TFO names it. Ordinary payer fallback is preserved when admission fails.

### Qualified spend (not currently connected)

A third-party relay may exercise only an exact deterministic native financial effect against a known current rooted escrow row/outpoint. The native plan must bind full transaction bytes and EVERY actual input entry/index, actual parent root/block and accepting point, immutable contract/operation, current role authority or court-finalized permission, exact payee/change scripts and values, actual fee allocation and once-only row debit. All ordinary payer/relay inputs retain normal signature/maturity/mass validation. A transaction changing signature script, mass, another input, output recipient, financial scope or point cannot reuse it.

The per-input seam is `check_scripts_sequential` / `check_scripts_par_iter` around the individual TxScriptEngine invocation. Any exception to payer P2PKH authorization must be a private opaque native authorization specific to that locked input and exact successful joint transition, not a bool, generic SkipScriptChecks or cached ordinary signature success. Unknown script versions and OP_TRUE do not create custody; the current script engine accepts unknown future versions, so those scripts cannot secure escrow.

Principal and fees are separate conserved quantities. Total real inputs−outputs must equal actual carrier fee; all mandatory burn/rent/native charges share one budget, and the escrow output principal cannot be added as fee credit. Exact debt/recipient/remainder/burn rules must be natively derived from the adopted profile. If those policies are unavailable, return Missing; no example fee or fabricated paid amount can authorize a spend.

The final qualified spend must fail UTXO acceptance if its joint transition cannot happen. The current post-UTXO PALW object-drop path is unusable for a payout that already happened. Validate before merging its UTXO diff, then prove the exact plan was consumed by the final same-block PALW delta. Store that rooted delta/root and UTXO diff/multiset/acceptance/status in `commit_utxo_state`'s one batch. Cached apply/revert must validate old/new exact values before advancing UTXO diff; node-local tip cannot be the authority for a branch candidate.

## Why this does not yet unblock funded install

Native full branch grammar, authenticated fee/capital/retention/settlement profiles, complete runtime/windows and actual financial rights are still absent. Existing source+numerical geometry and the signed FundingConsent only bind identities; they do not supply adopted prices or a full original. Current extra `OriginalFundingScopeV1` remains a DTO and current private preflight is not a root-authenticated spending capability. Do not satisfy missing policy by adding another wrapper.

Required eventual positive tests: same-mergeset lock survival; failed funding fallback remains spendable; real UTXO escrow amount/row match; relay spends only eligible effects without payer online; alternate recipients/other claims/other sides/duplicate operation fail; ordinary fee payer signature still required; half-commit/crash and reorg restore both UTXO and remaining escrow once; unknown resource holds instead of slash/refund. These require a genuine complete coordinator and native adopted policy, not fixture booleans.
