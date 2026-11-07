# Financial next unit: actual residual-principal refund, with hard prerequisites

Read-only audit, 2026-10-22. No repo changes/build/native execution. ADR0202 A authorizes isolated actual-UTXO escrow development, not an adopted tariff, return deadline or new unconsented obligation.

## Conclusion

The concrete missing financial transition is **consume the contract's current actual escrow outpoint once, pay eligible unused residual principal to the same payer's signed native refund address, and remove/update its rooted escrow lock in the same branch transition**. Its arithmetic needs no invented rate. However it cannot currently be implemented as a reachable successful transition: the repository has no installed RFC35 rooted funded-account/current-outpoint record, no native state-UTXO lock, no native qualified-input spend authorization, no native-derived adopted settlement-policy body/refund eligibility, and no fully closed admitted original that could create those facts. A fake funded row/eligibility bool or DTO-only spend plan would not repair this. Do not add another diagnostic wrapper and call it settlement.

The immediate dependency remains the real native funding/lock installation coordinator, whose admitted original must be fully checked before the same transaction's output enters funded state. Ordinary unsuccessful TFO carrier fallback is already real native payer-signed UTXO spend (covered by existing regression); it is not this escrow refund and must remain available when no funded row was installed.

## What S1 actually changes today

- `palw_court_archive_verify_v1.rs::TransitionBuilder::apply_native_archived_chunk_v1` (~107): private actual native chunk point, original signed window, same source/side, actual complete mask, retained native v5 recheck. It writes rooted ArchiveDeclarationProgress.completed atomically with actual append/re-observation. Unknown historical prefix stays unqualified; other side/claim stays held. Public paid archived intake remains closed.
- `palw_court_archive_discharge_v1.rs::ArchiveDeclarationProgressV1::{check_context,check_loaded_context,check_v5}` (~52/93/102): provenance and native close validation. `PalwChainStateV2::qualified_archive_deposit_v1` (~152) checks actual completed progress/context, not a peer paid flag.
- `palw_court_end_archive_v1.rs::{court_archive_pending_exposure_v1,court_archive_holds_bond_v1,archive_pending_nominal_v1}` (~191-203): subtracts only that completed declared group's nominal deposit from conservative archived bond exposure. This can release that particular bond headroom/hold predicate. It transfers no money, credits no panel, refunds no UTXO, changes no actual escrow principal, and does not settle opposite-side/other-claim work.
- Existing `palw_escrow_funding_v2` is the old reward/capacity escrow under ADR0160, not RFC35's independently funded operating escrow. Neither its reward nor old nominal court deposit/burn may seed a new operating balance.

## Existing funding checks are preparation, not principal

- `palw_court_original_escrow_v1.rs::{FundingTermsV1,FundingConsentV1}` (~39/111) bind original/source/relative graph/profile IDs, acceptance interval/material horizon, actual output index/amount, payer key identity, same-payer keyed native refund address, nonce. `verify_signature_v1` and `check_actual_output_v1` check actual MLDSA consent and exact ordinary payer fallback script/amount. No per-operation recipient/amount/refund-policy body is stored by those profile hashes.
- `inspect_transaction_v1` (~193) derives txid/outpoint from the actual final transaction, not a peer supplied outpoint.
- `prepare_funding_v1` (~243) compares a future coordinator's expected scope/required amount/horizon. Its OriginalFundingScope DTO does not authenticate that scope.
- `palw_court_funding_preflight_v1::{FundingCheckpointV1,TransactionBoundFundingPreflightV1,preflight_funding_transaction_v1}` (~21/64/120) bind full transaction including signature script/mass, actual populated entries, selected parent/root/accepting point/network/scope and checked fee obligations. Populated entries become authentic UTXO provenance only when supplied by actual native composed-view validation. No balance/lock/spend writer exists here.
- Current native funded candidate reaches actual Source validation but remains Missing for full grammar/profiles/ordered coordinator. It still installs neither a funded row nor a script skip/qualified spend path.

## Exact future refund transition contract

Inputs must come from actual native branch state and real validated spending transaction:

1. Rooted, native-created funded account for exact original claim/source/allocation/relative graph/profile and accepted FundingConsent; current outpoint, actual remaining balance and payer native keyed refund address; original consent/current authority snapshots and once event history. Row absent => ordinary fallback rules only, no escrow authority.
2. Actual composed UTXO entry at that current outpoint, matching rooted amount/script; real transaction containing that exact input once. Native current-parent checkpoint/root and ordered prefix must match. Full transaction/input/output binding includes mass and signature scripts; txid equality alone is insufficient.
3. Actual contract settlement-policy body whose identity matches consent. It must authorize the precise refund condition. A policy hash or hard/material DAA alone is not a permission. S1 is one positive discharge input, not an all-obligation release certificate. Every remaining reservation/claim/side/retention/repair cause must be explicitly preserved.
4. Actual original payer's signature for ordinary recovery, OR a precise native contract authorization for third-party relay already covered by signed policy. Generic input-validation skip is forbidden. When third-party relay is supported, it may only realize deterministic signed refund output; no payer's future private key requirement and no relay-selected recipient/amount.
5. Actual refund output script must be the same payer's keyed native MLDSA address signed in original consent. Never the unkeyed payer_key_id, another key, unknown script version, OP_TRUE or arbitrary marker. Owner-specific fee/output policy is native derived, not caller declaration.

Checked conservation for this refund family:

`actual escrow input E + actual relay ordinary inputs R = exact payer refund P + contract-permitted escrow change C + actual relay ordinary outputs O + actual total input-minus-output fee F`.

Existing native fee-burn convention allocates `F = distinct explicit burns B + miner collectible K` (plus any separately specified native fee allocation, if later actually adopted), so B is a subset of F, never another added sink.

Count native burns inside F exactly once; do not add B again to the UTXO equation. Preserve existing raw fee meaning; subtract all distinct obligations before any collectible/refund reimbursement. The escrow debit must exactly equal the contract-approved E-funded portions; ordinary relay fee cannot be charged arbitrarily to E. With unknown reimbursable fee ceiling/policy, refuse qualified refund rather than select a rate. The zero-fee/payer-paid variant is not automatically adopted merely because arithmetic balances.

Once-only state transition: check current outpoint/version and refund event absent; consume that exact outpoint and record operation once; update balance/current actual change outpoint, or terminal refunded account as specified by native policy. New change outpoint derives from final actual txid/index. No second spender, duplicated event, stale branch/root/outpoint, retyped row or another claim may reuse principal. Partial refund cannot release still-reserved funds or erase S1 proof/remaining holds.

## Exact atomic native seam

`consensus/src/pipeline/virtual_processor/processor.rs`:

- `PalwUtxoCommitV2` (~334) carries rooted PALW root/delta and native delta lease.
- Before actual UTXO input removal, a future native pre-UTXO coordinator must identify any contract-locked input using this selected-parent PALW snapshot and same ordered mergeset composed UTXO view. A qualified authorization is exact-input/transaction/contract scoped; ordinary inputs still run ordinary script checks; authorization must not populate ordinary signature-success cache.
- Fold must consume every prepared authorization and produce exact old/new funded-account plus once records matched to the same actual UTXO inputs/outputs. Any failure restores the existing object/FP checkpoint and staged reservations; ordinary nonfunded TFO object failure retains its payer fallback, while a qualified escrow spend cannot be accepted then silently object-dropped.
- `commit_utxo_state` (~3251) already stages `insert_delta_batch_tracked_v1` (~3323) and UTXO diff/multiset/AcceptanceData (~3476) into one `WriteBatch`. Put future escrow old/new changes into this existing rooted PALW delta; do not write an independent balance cache/store first. Existing pruning/PALW lock order is retained through commit.
- Undo/reorg applies exact reverse PALW delta and matching UTXO diff; no local virtual-tip lookup can replace the selected-parent/root contract. Retained once/provenance may not be pruned while needed to validate either direction/IBD.

## Meaningful acceptance tests once prerequisites exist

Actual funded original native admission and lock then actual qualified same-payer refund (not a fabricated seeded row); ordinary fallback after rejected original; third-party deterministic relay only with real signed policy; duplicate input/current outpoint/event; wrong payer key/unkeyed-address trap; wrong balance/output/change/fee allocation; S1 one side complete while opposite/other claim/retention cause remains; native/script/checkpoint failure leaves both state and UTXO unchanged; same-block dependent output/spend; reorg and reverse/replay restore balance, outpoint, once and holds. Unknown rate/profile/policy should cause refusal before mutation. No rate/cap choice is needed to write these invariants, but positive qualified refund needs the actual installed contract and adopted policy body.

## Recommended order

Do not add a financial success path now. Continue actual canonical-family closure + authoritative native rules, then native original escrow/UTXO lock installation in existing batch. Once real principal and policy are available, implement the above exact-current-outpoint refund transition (or a contract-authorized fixed-amount payment using actual body, if that policy is adopted). S1 progress can be consumed as one proven obligation-release cause then, never converted into money by itself. This is a dependency explanation, not a claim of an implemented new financial transition.
