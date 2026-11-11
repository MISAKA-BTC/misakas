# TFO3 complete protected profile integration

## What existing schedules provide

- `consensus/core/src/palw_funded_original_coordinator_v1.rs:265`,
  `palw_funded_original_schedule_v1`, resolves the existing lane VAL G14
  funded estimate when the conviction fence is active and its Batch
  estimate when the Batch fence is active. It has no complete protected
  original policy. `PalwFundedOriginalScheduleV1` already holds finite
  fee, capital, retention, settlement, geometry, admission and transport
  schedule inputs, but its existing identities and profile must stay intact.
- `consensus/core/src/palw_g14_values_v1.rs:74,207,222,239,270` provides
  one 34,000-byte/work-13/io-1 packet per DAA estimate, storage schedule and
  bounded constructors. The source's service-inclusion premise remains
  unadopted. These values do not describe full protected verification or
  establish deadlines from observed successful inclusions.
- `consensus/core/src/palw_funded_original_admission_v1.rs`,
  `funded_batch_base_candidate_v1`, and
  `consensus/core/src/palw_funded_original_coordinator_fold_v1.rs:491`,
  `prepare_funded_batch_book_v1`, already bind the actual shared
  CourtServiceStateV2/profile and prepare a private book before mutation.
  The existing entry consumes a fully prepared legacy Frame; using it
  as the new planner entry would retain the serial-time gate.

## Smallest native integration sequence

1. Define a separate, optional versioned complete native profile with
   checked finite fields and a complete scope fingerprint in Params and
   transition extras. Existing shipped defaults stay dormant. The
   immutable ruleset identity must commit to the selected profile, all
   executable manifest codecs/limits, resources and economics. Reuse
   existing storage/rent/escrow arithmetic only where the complete new
   profile expressly uses those same rules.
2. Select the new branch at the common quote and original preparation
   entry `palw_tir_public_fold_v1.rs:498`, `original_plan_mode_v4`, before
   legacy frame/time allocation. Build from the authenticated parent
   (actual bound container, class, layout, earlier tokenizer registration),
   exact immutable TFP1 facts and native profile. Fully resolve PublicOriginal
   ChainScope, prompt metadata and native cold state/dependency semantics.
   Keep op248's current unsigned TFP1 semantics, with any new companion
   inputs versioned. An immutable identity or a non-booked estimate is
   not a complete quote.
3. Extend `PlainBatchOriginalProjectionV2` and the new protected compiler
   with real numerical, output/stop, Selection, tokenizer, manifest
   request/response, DA/default, recovery and defense graph nodes. Give
   each selected rank, finite outsider and defense opportunity independent
   routes in the same shared book. Book worst-case ready and missing-material
   paths before the notice deadline and the subsequent work before hard.
   Use `CompiledBatchAdmissionV1::reserve_protected_v2` for sum capacity,
   actual draft TTL, all retention and checked aggregate fee terms. The
   complete original resource graph identity remains independent of
   signatures and any not-yet-created CaseId.
4. Actual TFO3 uses the same planner with its literal accepted carrier,
   actual transaction and selected parent. Resolve declared rent, actual
   fee, correct escrow output and signer, and the native original link
   within the existing per-original paired checkpoint. Preserve current
   missing-profile refusal until all profile inputs and transitions exist.
   The final writer seam is
   `palw_funded_original_coordinator_fold_v1.rs:904`,
   `install_original_admission_v1`, which already checks exact
   `service_before`, hard clock and same-block FP acceptance, installs
   Frame, `install_prepared_batch_frontier_v1` and funded escrow.
   Add protected book, companion admission/link and downstream holds
   under the same checkpoint, without detached end-of-block rows.
5. Restore/reorg must recheck exact origin, both immutable carrier
   identities, profile and all related native rows. For local payment and
   finality, a successful quote alone establishes none of court service,
   observed supply, verified Terminal or independently selected approval.

## Implementation gaps

- The protected core kernel and relative TFP1/companion binding exist, but
  there is no production complete policy or selected profile branch.
- Source-only Selection envelope and terminal bounds are available. An
  accepted, protected case-specific notice, exact role/range Opens,
  material Finish, atomic booked DecodeToken terminal and repair remain
  missing. First-notice exclusivity in the legacy context is not protected
  capacity for a new profile.
- Authenticated native draw producer, finite outsider rights and distinct
  defense authority are not connected. Claimant rank is separate from
  supplier role. Missing authenticated supply must drive existing
  objective default/recovery or unverified Abort, never fabricate proof
  of unavailability or slash a blocked honest requester.
- The pre-notice draw/cold acquisition/full inspection/notice inclusion
  stages and post-notice DA/default/aggregate settlement need actual
  booking/observation. BCT2 terminal still depends on a whole-prefix
  checkpoint whose size may grow; complete policy needs a bounded
  native mutation/rollback footprint.
- Current wide provider/consumer materials do not provide ordinary TFP1
  trace/prompt processing by themselves. Prompt text sidecar plus exact
  text/registered raw tokenizer/native token-ID relation are separate from
  the retained raw-to-rendering table. Final eligibility still needs
  the new authenticated claim-level approval and the normal native
  readiness/liability/economic rules.

## Measurement/profile gaps

- Old numerical estimate has 2 elements, 3 terms, 1 target request and
  1 source read. Same Studio candidate local maxima over 270 numerical
  roots were 1,043, 130, 64 and 323 respectively. See
  `docs/design/palw/integration/public-one-honest-implementation-ledger.md:112–116`.
  One synthetic 117-packet case in an empty book does not measure
  protected aggregate capacity.
- On the exact Studio original, measure literal prompt preprocessing,
  raw tokenizer acquisition and validation, all native paths' work,
  IO/retained bytes/byte-DAA/RAM and rollback or archive writes.
  Validate same-input model/codecs and exact provider transport before
  any candidate is declared compatible.
- Measure all selected and permitted outsider/defense cases concurrently,
  including hostile races, malformed repeats, resource refusals, and
  actual block inclusion/physical source bounds. Verify every case stays
  within original hard and recovery windows on the real shared-book
  profile. Derive values on the qualified runtime and lowest admitted
  resources, not from a DAA-to-wall-time assumption.
- Existing tokenizer Linux/resource/RocksDB evidence measures TKR1
  registration paths. It does not qualify cold full replay, service
  defaults, positive approval, complete TFP1 acceptance or public activation.
