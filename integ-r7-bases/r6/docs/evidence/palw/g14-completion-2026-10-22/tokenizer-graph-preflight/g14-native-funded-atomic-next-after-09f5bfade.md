# RFC35 native funded admission: actual next seams after 09f5bfade

Read-only current source review. No runtime activation, code edit, build, fee/profile/cap choice. Existing candidate success is only ordinary UTXO validation; no funded-admission capability exists.

## Current code stops before admission by construction

- `utxo_validation.rs:427 palw_funded_original_candidate_from_native_v1` calls read-only core diagnosis. `calculate_utxo_state:445` supplies None; production does not pay extra candidate verification cost. Opt-in internal accepted loop:621 can borrow actual ValidatedTransaction entries but cannot install a row.
- `palw_tir_public_fold_v1.rs:24 palw_funded_original_candidate_v1` authenticates native input/output arithmetic, consent/current producer/class/layout and returns Missing source construction meter/full grammar/profiles/ordered coordinator. There is no success variant.
- `palw_court_funding_preflight_v1.rs:120 preflight_funding_transaction_v1` consumes PopulatedTransaction (owns entries Vec), checks checkpoint, exact full tx including signature_script/mass + actual entries digest, prepare_funding, once fee purposes. Its expected scope/requirements remain caller DTOs. Do not feed consent profile IDs/zero amounts as native requirements. Borrowing actual native entries is preferable to a second deep copy; changing this helper's ownership alone is not atomic funding.
- TFO dedicated rejection remains `processor/palw_accept.rs::palw_v2_objects_of_block`; no inner TFP extractor emission permitted.

## Earliest authoritative snapshot and ordering

`processor.rs` candidate branch around2330 derives same-branch `palw_state`, actual header/current accepting point and locked bond/burn views, then calculate_utxo_state at2359. This is valid branch provenance, NOT final ordered original-prefix state. The walk groups each merged block: it validates that block's independent transactions in parallel against selected-parent UTXO + mergeset_diff, then accepted loop adds each actual ValidatedTransaction into diff/muhash. Full flags or selected-parent cached checks must remain distinguished.

Core PALW object application occurs later (around2470) after the entire UTXO walk. Therefore adding a rooted escrow row only in that later core fold is insufficient: an accepted child mergeset block can spend the new ordinary fallback outpoint before that lock is created. Conversely marking every structurally decoded TFO output prospective-locked before complete original acceptance incorrectly freezes ordinary payer fallback for Missing/Invalid originals.

Necessary positive coordinator ordering: actual validated carrier -> complete native original PLAN on exactly the ordered state prefix -> prospective locked outpoint overlay before next dependent mergeset block -> core state/journal writes staged -> joint DB commit. Within-block independence permits retaining existing parallel UTXO validation; synchronization is required at each merged-block boundary before descendant validation, and original conflicts across same-block carriers need deterministic accepted order or atomic joint processing. Existing accepted loop must not claim current parent state alone is this ordered state.

## Required inputs and no-fake-completion rule

Native planner must receive actual tx and same native-populated entries/fee, accepting point+selected parent+root, current registered class/layout/semantics/tokenizer inventory, current producer authority, actual native court/demand/rules, and selected service/fee/capital/retention/settlement profile BODIES. It must itself derive complete source/numerical/semantic/F02/termination grammar and exact obligations. Caller scope DTO, signed profile IDs and local graph budget are not authority. Existing prepare_original_v1 validates OLD TFP1 frame allocation and pledge, but cannot certify the additional full shared canonical grammar/native funding profiles.

No valid independent patch can currently emit a positive funded row from existing APIs: those authoritative profile/grammar outputs do not exist. A fresh wrapper calling existing prepare with a claimed Complete bool would manufacture acceptance.

## Limited independent implementation unit that is genuinely useful now

1. Implement source/geometry native allocation owner, as /tmp/g14-source-graph-native-preflight-next.md specifies. This permits the actual prospective planner to execute existing original preparation on native inputs without unmetered expansion. Local graphbudget patch already handles only V/E expansion counts and is not this owner. Tests: local resource refusal BEFORE program/layout clone/graph expansion; typed capacity vs malformed separation; no paid/admitted result.
2. Extract the existing OLD-TFP1 FP projection into one core helper from `palw_fp_objects_v3.rs` (consumed-prefix view remains supplied by the same native extractor policy). Dedicated TFO planner can reuse it internally without emitting a legacy object. Keep all version/fence/prefix/constraint/tokenizer stateless checks in the native policy path. Test all existing projection bytes/claim/jobpin/tails unchanged and TFO never appears in legacy extraction.
3. Implement private paired core writer only AFTER a native-complete plan type exists. It must use `objects.rs:35 try_checkpoint` (native resource-backed) before any ordinary FP reservation; prepare+reserve_for_claim+install_original+escrow row are one scope. Current check_paired_original/prepare_paired uses old extras map and must not be repurposed to mean TFO funded. Dedicated map/namespace and key/row ordinal coordination are needed. No standalone row seed while incomplete.

Unit3 cannot honestly be completed first. A dormant new row/authority DTO with no real native-complete producer adds schema risk without reducing current missing positive path. Prefer units1/2 followed by actual planner semantics, not another generic preflight wrapper.

## Atomic and reorg boundary to target once complete planning exists

Core `objects.rs:35` checkpoint restores `state` and journal entry length together (`transition_builder.rs:8962/8970`), including derived caches. This is appropriate to roll back FP/frame/escrow primary rows as ONE object. It does not revert accepted ordinary UTXO, and RFC35 permits this: failed original leaves a normal payer fallback.

Native `UtxoProcessingContext.mergeset_diff`/multiset and prospective escrow lock overlay must be checkpointed together around candidate transition. On failed original restore only speculative lock/core state, retain the successfully validated ordinary carrier. On any fatal native block failure, all candidate core/UTXO/lock staged changes are discarded. Need checkpoint ownership preflight for cloned maps/diff, not unmetered clones.

`processor.rs:3251 commit_utxo_state` creates one WriteBatch; tracked PALW delta at3330 and UTXO diff/multiset/acceptance stores at3480 are written together via `db.write(batch)` ~3500. Escrow primary row should travel in that PALW delta; prospective locks must be derived/rebuildable from rows and actual outpoints, never a node-local tip cache or independent db write. Core StagedCourtSeed/Move inverse journal can provide exact rollback if fresh namespace load/undo/retype guards are implemented. Parent selected-chain PALW re-derive/undo loop must reconstruct the same locks.

## Required native tests for the future positive unit

- Real Full signature carrier, complete native original succeeds: actual own outpoint matches payer keyed fallback and exact amount, single rooted row, ordinary FP+frame+escrow state root/journal jointly installed. Duplicate same carrier/row and source/profile/parent/entry mutations refuse.
- Same mergeset later child spend: refused only after original successful admission; Missing/Invalid candidate leaves fallback spendable (existing regression is baseline).
- Fault injection after FP reserve/frame insert/row insert: FP/core/lock overlay restore, ordinary carrier output remains; no alias release for other claim/opposite side.
- Real DB batch/reorg: original block detach removes escrow lock and restores parent root+UTXO; reaccept reinstates exactly once. Header-only/local tip change never changes authority.
- Qualified settlement spend is separately incomplete: no general script-skip flag. Only exact currently rooted locked outpoint + verified court settlement effects may authorize constrained destinations/amounts; payer ordinary signature must not freely withdraw active principal. Real in-out fee budget must not pay old burn/rent and new carriage twice. New funding creation and later qualified spend require different admission paths.
