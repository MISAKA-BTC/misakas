# PALW behavior-preserving refactoring plan

Status: In progress. User changed the active task from G14 feature work to completion of the refactoring described in two attached reviews. Base `292a6d7db`; branch `codex/palw-behavior-preserving-refactor`, reusing the clean attached integration worktree. Original checkout modifications are not part of this work. Prior G14 commits remain on their original branch; this task adds no G14 feature or monetary rule.

## Full requested scope and acceptance

| Area | Required result | Current evidence/status |
| --- | --- | --- |
| 0 Baseline | Map canonical, historical, dormant and deferred paths; record starting test results and identity inputs | Canonical/history/dormant/deferred route matrix recorded below; preserved core/node executable hashes and before/after checks retained. Remaining rule/profile coverage follows each later extraction |
| 1 Court send state | One typed owner for pending/due/moved, preserving deadline order, duplicate suppression, submitted vs accepted and retry/recovery | Completed for shared node-local court send state: private pending/deadline/debounce owner; 55/55 source projections; 461 panel tests and three representative real-service cases pass. Actor capability narrowing continues under area 5 |
| 2 Carrier and fee ownership | One build/submit/change-outpoint persistence/funding/inflight boundary; unchanged success/failure/restart behavior | Implemented: typed funding/count owner, prepared/submitted tokens and one change recorder at 16 old success sites. 463 panel tests, three representative actual-service cases, t12 identity 9 and activation 5 pass |
| 3 Consensus preparation | Acceptance and full transition share pre-object work; historical ordering, roots, deltas and native resource errors preserved | Completed for the pre-object scope: 38 existing tests match before/after, 29 integration and 3 transformer-pin tests pass. Other refactoring areas remain separate obligations |
| 4 Rules and views | Strengthen existing canonical rule resolver; separate block facts/current rules/claim acceptance rules, preserving accepted_daa semantics | Initial borrowed views implemented and selected core tests pass. Canonical resolver and caller migration remain in progress; no parallel resolver |
| 5 Runtime/validator/backend responsibilities | Central worker becomes orchestration; validation dispatch and capability interfaces depend on narrow inputs, ordered side effects retained | Validator implemented as exhaustive family dispatch with explicit immutable inputs; consensus lib/tests type check passes; 463 panel cases, 3 representative service cases, t12 identity 9 and activation 5 pass. Worker/backend work remains pending |
| 6 Compatibility paths | Classify normal/history/dormant/deferred/unused entrances; isolate misleading nonproduction sync APIs, retain necessary old wire and replay | Route matrix and standalone test-only isolation complete: consensus lib/tests check, three walker fixtures and no-caller/canonical-resolver guard pass. Preserve historical adapters while the remaining boundaries are narrowed |

Completion requires all areas, not just the first extraction or shorter files. Verify acceptance/refusal/exclusion, bytes/IDs/state roots/deltas, collateral/reservation/release/slash/reward/weight, activation boundaries, claim-time rules, restart/reorg/pruning and submit/retry behavior at each changed boundary. Preserve independent reference implementations and journal/atomicity. Build/test commands use the existing heavy lock. No production arming, pre merge/push, deployment, new bond/reward/fee/deadline/cap or activation change is authorized by this structural task.

## Consensus preparation finding

The two pre-object regions differ operationally in artifact ownership migration placement: acceptance places it before class retarget; full application places it after vertex/mesh sweeps. The pre-object entry also rearms material retirements after the shared region, whereas full application rearms later after its object/work processing. No claim that these orders commute has been established. The refactor preserves both historic positions explicitly rather than silently selecting one as canonical.

`palw_state_v2/block_preparation.rs` now owns all common ordered statements. An internal migration-order enum records the two old placements. Entry-specific rearm stays at its original call site. A normalized statement-sequence comparison projects both enum choices and verifies equality to each old region; this is structural evidence, not a replacement for runtime tests. The existing fold source-reading API includes the new child so guards still see all production code.

## Fixed compatibility constraints

Keep wire enum ordinal/discriminant order, legacy entry behavior and codec bytes; Final and SecurityCertified remain distinct. Do not move or edit `misaka/palw/derive/src` because transformer identity hashes paths and source content. Preserve its pin and independent reference implementations. No crypto computation proofs or model-level review are introduced; the permanent two-tier premise and deferred media/improvement paths remain fixed.

Evidence: [baseline and preparation sequence audit](../../../evidence/palw/refactor-2026-10-21/baseline.json). Source reviews were static and their numerical size counts are not treated as fresh measurements of this checkout. The two review attachments are task inputs, not instructions to import their external URLs or unexecuted claims as verified facts.

## Preparation checkpoint evidence

The unchanged pre-refactor executable was preserved with its recorded SHA-256. Migration/model-line, reward-sweep, claim pre-object judgment and retained/legacy canonical-byte tests pass under both binaries with the same 38 unique test names/results. Fresh integration tests cover carrier refunds (5), deadline re-date (8), burn/cap acceptance vs fold (2), t12 identity (9) and activation (5). Transformer pin tests pass 3. Total current selected passes: 70; repeated pre-object/baseline subsets are not added to that total. These are scoped existing assertions, not a claim to compare every possible state/root/delta.

Remaining work is still the full court/fee ownership, rule/view and validator/runtime/backend boundary, compatibility-route classification and complete regression/recovery matrix. No endpoint or frozen semantic rule has been removed or normalized.

## Route classification from the current checkout

This is a source inventory, not an activation or deployment decision. Inspect the preset's fence schedule at the target block; a route's existence does not enable it on any network.

| Classification | Entrance and owner | Rule/time contract and preservation requirement |
| --- | --- | --- |
| Canonical admission/rehearsal | `processor/palw_accept.rs`, `processor/palw_validate.rs`, pre-object retained base | The existing `palw_transition_extras_for(point)` resolver and object-specific overlay select block rules. Native resource refusals remain distinct from invalid objects. Admission does not install the full transition |
| Canonical live and IBD fold | `processor/palw.rs` full/retained v7 wrappers | Uses the same resolver; attaches accepted carrier/refund/EVM and mergeset facts at the existing call sites. Keep journal, delta and material-retirement order |
| Canonical reorg | `consensus/src/processes/palw_state_walk.rs` and processor/store integration | Revert/apply stored deltas in the existing chain order. Do not regenerate historical changes from today's rules |
| Canonical pruning import | Processor pruning-point carriage loading and checked witness/root import | Imports the checked carriage for its pruning point; the unused standalone walker is not the import engine |
| Historical compatibility | Core v2–v6 entry wrappers, old object codec/tag variants and retained-state adapters | Keep their explicit defaults and old bytes. They converge on the shared engine but are not permission to fold current blocks with default extras |
| Dormant test/exploration walker | `consensus/src/processes/palw_state_v2_sync.rs` | Whole-workspace Rust search finds no construction or step-type use outside that module; other matches are module declarations, comments and an existing no-caller source guard. It deliberately lacks current extras. Now compiled only under `cfg(test)`; its no-caller guard also pins that boundary. Its three existing transactional/restart/reorg fixtures pass; do not connect it to production |
| Canonical node court attempts | `PalwPanelService::worker`, reporter/replay/fraud/held books, public-response actors, priority lane | One local pending/deadline/debounce owner is being installed. Native accepted views remain authoritative. Preserve eager queue-attempt stamps where the old code used them, submission stamps, refusal retry and recovery |
| Deferred | RFC-0003 image/video and RFC-0004 improvement paths | The post-mainnet deferral remains; shared interfaces and registry cleanup cannot turn these on |
| Identity-sensitive | `misaka/palw/derive/src`, transformer pin, independent reference implementation | No move, content change, wire ordinal or identity-input change in this task |

The block/claim distinction remains mandatory: `canonical_work_daa` and other claim-origin fences stay as heights where consumers compare the claim's `accepted_daa`. This inventory does not replace them with a current-block boolean. Existing model coefficients, deadlines, collateral, reservations and payouts are unchanged.

## Carrier boundary findings for the next extraction

The main fee chain and the tick's spendable funding are distinct: a held tick deliberately has no spendable funding, but must not erase its persistent tip. Existing readiness/receipt lanes also have their own funding/inflight state and scan throttle. A shared owner must retain that distinction, the current replacement path's in-flight count, and each lane's recovery reads.

Submission and bookkeeping have an order contract. The old sites clone the selected change output before submission, then perform site-specific success logs/flags, persist the next outpoint, replace funding and increment inflight. A common submission result can carry the change to the original bookkeeping point; moving persistence ahead of those site-specific effects would be a separate change. A persistence write error currently warns and continues, so this structural task must not silently introduce a new durability/refusal rule. Builder and mempool refusal paths likewise retain their existing retry/drop decisions. The runtime extraction now implements that order. Built/submitted tokens carry the exact old change snapshot; only successful submission constructs the latter token. The caller consumes it at its old success bookkeeping point. Main, readiness and receipt funding remain separate, with checked increment, bootstrap saturating increment and explicit replacement/lane count updates preserved. The standalone bond-registration transaction has a collateral output and no runtime inflight book; it remains an explicitly separate path.

## Court ownership and dormant-walker checkpoint

The typed court owner preserves old stable EDF, session-round order, duplicate handling, due-date insertion vs tightening, queue-attempt/submission stamps and the unsent/due cleanup. Native accepted state and filer recovery remain the authority; a local stamp never settles a claim. The raw-parts adapter exists only under `cfg(test)` to retain independent pre-refactor fixture assertions.

Before: 455 panel tests pass and four existing source guards fail. After: all 461 tests in that same non-service scope pass, including two new ownership/retry scenarios. No previously passing test disappears or regresses. Four old guards were updated for an already-existing capture-arm branch, receipt-lane funding exclusion/site and explicit collusion drill; this changes no protocol upper bound or ordinary licence rule. Three representative real-service cases match before/after; the attempted whole-panel baseline was interrupted and is not called a full pass. Walker tests pass 3, its source guard 1, t12 identity 9 and activation 5.

The 55 outbox-facing production function bodies match the reviewed mechanical projection. A patch reconstructs all 36 changed source files byte for byte from `1b83142a2`. See [the court checkpoint validation](../../../evidence/palw/refactor-2026-10-21/court-phase-validation.json) and [test comparison](../../../evidence/palw/refactor-2026-10-21/court-test-comparison.json). Full refactoring is still in progress: carrier ownership, typed rules/views, validator dispatch and backend capabilities remain.


## Carrier ownership checkpoint

`CarrierChainV1` owns the persistent main funding and inflight count. `CarrierBookMutV1` lends only the selected funding/count pair to a send site, including a held tick or independent lane. `PreparedCarrierV1` retains transaction identity and the pre-submission change snapshot; submission wrappers retain the original overlay, direct or replacement transport. `SubmittedCarrierV1` can be constructed only after that transport returns success. It means submission, not chain acceptance, and does not alter claim credit.

All 461 previously passing non-service panel tests remain and pass; two new scenarios cover held/refused funding preservation and persistence-before-memory order. The 16-site source contract inventory matches transport, persistence selection and counter operations, and six low-level transport/pricing/funding methods remain unchanged apart from formatting. Eleven lifecycle build sites share preparation. This inventory is targeted structural evidence, not exhaustive execution equivalence. See [carrier test comparison](../../../evidence/palw/refactor-2026-10-21/carrier-test-comparison.json) and [source contracts](../../../evidence/palw/refactor-2026-10-21/carrier-contract-audit.json).

Full refactoring remains in progress: typed rule views, validator dispatch and narrower backend/runtime responsibilities are still required. No pre integration, arming or deployment was performed.

Final carrier checks: representative actual-service cases 3/3; t12 identity 9/9; activation 5/5. [Validation and limits](../../../evidence/palw/refactor-2026-10-21/carrier-phase-validation.json). Source-only whitespace check is clean; raw logs and patch are retained verbatim, including their original whitespace.


## Initial block/claim input views checkpoint

The existing extras remain the legacy compatibility input and the processor's existing `palw_transition_extras_for(point)` remains the production resolver. New borrowed views neither resolve a fork nor read a current tip. `PalwBlockFactsV1` exposes accepted input facts; `PalwRulesAtBlockV1` evaluates the block-scoped rules; `PalwRulesForClaimV1` delegates pricing, independence and canonical weight to the existing claim-origin expressions. No serialized type, fence or default is changed.

Shared pre-object preparation now receives facts and rules separately. Round fees are facts; the execution-lane configuration is a rule. Fold reads use the bound block view for fresh-attempt work rules, while the builder's canonical claim weight uses the claim view. The new boundary scenario checks claim origins immediately before, at and after a fence while the reading block moves across it. All 12 pricing tests and the same 38 preparation/history tests pass; all 29 selected integration tests also pass (including t12 identity 9 and activation 5).

This is an initial caller migration, not completion of area 4. Raw extras still serve legacy and unconverted consumers. At the initial-view checkpoint validator family extraction was only a draft. The following validator stage implements it separately; initial-view test counts do not validate that later stage. Backend capability and worker responsibility work remain pending.

Initial-view [validation and limits](../../../evidence/palw/refactor-2026-10-21/input-views-validation.json): the 11 original pricing tests still pass and one origin-boundary scenario is added. The unchanged 38 preparation/history tests and 29 integration tests pass, giving 79 unique selected after-tests. The preserved pricing baseline predates the preparation extraction; it is not labeled an adjacent-checkpoint binary.


## Validator responsibility checkpoint

The entry retains the original no-bundle response and per-object receipt-path precheck before an exhaustive dispatch. Ten private family methods receive only the immutable state, parameters, block point and Panel parameters each actually uses. The 133 original arms cover 141 distinct variants. All guarded/fallback arms stay in their family's original order, all error strings and inner calls are copied, and the 16 continues that targeted the outer object loop now return success from that object's helper; the entry then proceeds to the next object as before. No inner-loop continue is rewritten.

The source projection checks all 133 arms, exact family routing/arguments and the unchanged entry precheck and loop tail. `cargo check --release -p kaspa-consensus --lib --tests` passes. Source guards' inventory includes every new family file. One existing attribution guard now ends the class-registration arm at the next arm in that family instead of the former physical neighbour in the monolithic match; its attribution checks and ordering assertions are retained. Fresh node tests pass: the same 463 non-service Panel cases, three representative actual-service cases, t12 identity 9 and activation 5. The attribution guard passes in a separately labeled extracted-function harness; a full core-crate test run is not claimed. Backend registry/capabilities, remaining worker orchestration and further canonical rule/view migration remain unfinished.

Validator [validation and limits](../../../evidence/palw/refactor-2026-10-21/validator-validation.json), [all-arm projection](../../../evidence/palw/refactor-2026-10-21/validator-split-projection.json) and [Panel comparison](../../../evidence/palw/refactor-2026-10-21/validator-panel-comparison.json). Full task status remains In progress.


## Backend operation-view checkpoint

The existing registry remains the only factory and the original compatibility trait and old tests are byte-unchanged. The current checkout has 106 trait methods (the incoming review's 104 was a historical source count, not a fresh measurement). Five borrowed operation views expose execution, replay, material, court evidence and resource operations. They forward 96 distinct existing methods in 97 methods (the authenticated token-table getter is shared by execution and scoped replay); ten configuration, downcast and drill hooks stay on the compatibility trait. Constructing a view does not assert backend support, certify a model, change a refusal or start work.

Eight helper boundaries now accept their needed operation view: resource composition, canonical FP prompt derivation, the two scoped FP material verifiers, attempt material admission, held challenger evidence, committed attempt answer extraction and retained interval binding. The 43 parameter/argument substitutions invert to the original seven node files byte for byte. The helpers keep their original order and values; their interface no longer exposes the entire backend.

Core lib/tests and node lib/tests type checks pass. Two resident new test bodies run successfully in a separately labeled native harness against the actual new operation-view source and the byte-unchanged compatibility trait: configured instance/default replay retention, and unsupported operations staying refused/absent without a substitute execution. This is not a full core-crate test run. Combined node runtime regression is deferred until the remaining worker/rule changes settle, to avoid relabeling the preceding validator's results as tests of this checkpoint. See [validation and limits](../../../evidence/palw/refactor-2026-10-21/backend-ops-validation.json) and [source audit](../../../evidence/palw/refactor-2026-10-21/backend-ops-source-audit.json).

Compound consumers, configuration and drill edges retain their explicit compatibility role. Worker orchestration and further canonical resolver/view migration remain in progress. No new financial or activation policy is included.
