# RFC-0029 owner accepted-material publication marker

The production owner now records a finished-reference hash only after its authenticated Unit or complete Ids has been published into that OwnerCase. Previously it inserted the hash before the fallible accepted-material read. A read/allocation error returned from the planning callback while leaving the hash present, so later ticks skipped the same accepted reference permanently. An absent result remains unharvested. Already harvested references still skip the read; Unit requests remain deduplicated.

The new `palw_tir_owner_harvest_v2` helper is called by the actual owner turn's accepted-slot loop. Neither the loader's String API nor its error taxonomy changes. Semantic errors and local resource errors both retain the reference for another read, matching the owner's existing pending read disposition. This does not mark a claim false, set case.done, authenticate new material, or change a native fence, policy, fee, wire format or approval rule.

## Verification

Initial parent-coordinated four-package build passed in4m48s with20 frozen source identities matching. Both new tests then failed during material fixture construction, before the quota/helper path, at Source binding verification (`job context shape`): the fixture allowed only4 total context tokens for3 prompt +2 generated tokens. Native context validation correctly requires P+D <= max_context_tokens; layout execution geometry separately uses P+D-1=4 positions. The test-only correction sets context capacity to5 and leaves layout/production validation unchanged. Initial failure log: `/tmp/misaka-rfc-head-harvest-gdn-harvest-tests-20261025.log`.

Corrected retry1 passed: coordinated build35.50s, all20 frozen source identities matched (`/tmp/misaka-rfc-head-harvest-gdn-wave-source-retry1-20261025/record.json`). Only Node bytes changed; Core/Consensus/Base0 artifacts retained their prior SHA. Immutable Node `/tmp/misaka-rfc-head-harvest-gdn-node-retry1-20261025`, SHA256 `ec38110866d91bcefa647df9eca3c0ee4cf9ba94c01e8fe2b2f8ec9ca1c84752`: `tir_owner_harvest_` **2/2 PASS**,0.06s harness /4.13s wall, quota child0.01s included. Log: `/tmp/misaka-rfc-head-harvest-gdn-harvest-tests-retry1-20261025.log`.

Related same-production controls passed on the initial Node artifact (before the fixture-only correction): source worker3/3 (0.16s), owner replay2/2 (0.00s), Cone4/4 (0.66s). Logs: `/tmp/misaka-rfc-head-harvest-gdn-source-worker-regression-20261025.log`, `/tmp/misaka-rfc-head-harvest-gdn-owner-replay-regression-20261025.log`, `/tmp/misaka-rfc-head-harvest-gdn-cone-regression-20261025.log`. These are separate artifact generations; the initial new-test fixture failures remain failures, not production-path quota results. Child test counts are inclusive. No Cargo was run by this lane; static `git diff --check` and new-leaf Rust parsing/formatting also passed.

Filter: `tir_owner_harvest_` (two outer controls; isolated quota child is included in its parent, not an additional test).

- Actual OwnerCase publication of a publicly authenticated StepNode Unit and outer-EOF Ids, request deduplication, marker-after-publication, no extra loader call for a consumed reference, and unchanged done/demand fields.
- Same production helper with a real native reservation refusal in its loader callback: no marker/Unit/Ids publication or done mutation, release the blocking reservation, retry the same reference successfully, and skip any further read. None and semantic error also leave the reference unharvested.

The material fixture uses public program/layout/header constructors, Source root authentication and the real Ids assembler. Its declared roots authenticate material identity only. It asserts no accepted claim, honest model computation, paid custody, selected-verifier approval or live campaign. The quota callback tests the actual helper's commit/retry boundary; it does not inject an accepted body or claim that Core's physical body/decoder allocator was dynamically exhausted at a particular internal stage.

## Remaining boundaries

The existing accepted-material loader still has raw Source construction, material decoding and String allocator refusals. This change does not complete their typed admission or retained-output ownership. Existing Unit Arc/Vec, Ids replacement and harvested-set allocations are unchanged and are not newly quoted here. Restart continues to rebuild local cases from native accepted state. K3 inputs remain unavailable and RFC-0012 campaign gates remain UNKNOWN; public activation remains unarmed.
