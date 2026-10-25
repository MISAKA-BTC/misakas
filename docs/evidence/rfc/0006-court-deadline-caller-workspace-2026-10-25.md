# RFC-0006 court deadline caller workspace

Status: scoped implementation and focused runtime controls PASS. The worker has not executed Cargo; the root coordinates the actual build and tests.

## Actual allocation boundary

Normal block preparation calls `sweep_court_deadlines`. Its native deadline loop cloned a session, the associated claim and any retained TIR dissection phase before the existing court/claim writers could reserve their own allocations. The abandoned-ladder refresh arm additionally cloned the working session while that first clone was still live. Those caller-owned copies were separate from the writers' later state/index/journal allocations.

The sweep now preflights these copies from the borrowed native deadline index before its first loop iteration. Only entries satisfying the existing strict `deadline < accepting DAA` predicate contribute. Each actual session contributes two typed native clone heaps (working and possible refresh), each existing claim contributes one, and each actual TIR phase contributes its underlying clone heap. No serializer ratio, model limit, new court verdict, source authority or allocation-discovery clone is used. A missing session is left to the original sweep's `MissingSession` path; the quote does not invent a new corruption-to-absence acceptance path.

Admission uses the existing transition construction owner. Its lease survives through no-show/backstop decisions, court removal/refresh, existing payout/slash/rearm writers and journal/output handoff. The extra refresh slot and rows another close removes can overreserve. Requests conservatively remain charged until transition output splitting retires unused construction reservations. Flags off bypasses the quote entirely. The outer evidence and per-iteration restored evidence are existing shared Arc owners and introduce no deep-copy allocation.

The quote covers the selected caller clones only. Existing writers retain their own preadmission duties; other replay/court-validation temporaries, broad auxiliary folds, source assembly and raw transport remain separate. No activation fence, model tier, claim-level selected-verifier approval, signed representation or adjudication rule changes.

## Actual controls

Package `kaspa-consensus-core`, lib filter:

```
palw_state_v2::tests::court_deadline_workspace_v2
```

Two parents use the existing native `licensed_with_court` fixture, then fold a real `CourtDisclosed` object. This produces an actual nonempty ladder transcript before the next turn times out. The initial empty ladder may correctly have zero heap; no fake fixed reservation is introduced to force a quota failure.

1. Measure Q from that borrowed admitted source. At zero and Q−1 the actual sweep returns typed `NativeResource` before changing the session, claim, root or journal; Q admits the workspace itself. A sufficient fresh checker retries the same source through the original challenger-silence outcome and native claim rearm. The construction reservation remains live through that consumer and releases when its builder drops. A capacity refusal is never a responder conviction.
2. At exactly the indexed deadline, a zero checker permits the unchanged not-due sweep. Past it, enabled and flags-off executions have identical state/root and complete journal bytes; the journal is also replayed and reverted with the existing delta APIs.

These are scoped local controls, compiled and executed on the fixed Core binary below. They are not a full native TIR-phase timeout campaign, a service-bound archive/funded settlement campaign, K3/cost evidence or a 15-gate PASS. The TIR `Some` phase quoting branch is source-connected through its existing typed native heap walker; no new `Some` fixture acceptance is claimed here. Native39-to-RFC12 settlement mapping remains unformulated, and no substitute mapping is introduced.


## Terminal verification

The combined four-package lib no-run build passed in 283.15 seconds with all 19 recorded source identities matching. Build time includes dependencies/lock work and is not a court or model cost benchmark. The two new court parents passed **2/2**, harness **0.03 s**.

- Immutable Core: `/tmp/misaka-rfc-deadline-legacy-reorg-kaspa_consensus_core-20261025`.
- SHA-256: `e490c6c76aba8597ab7dc567e924914f094873a533e2ddc0aeb6554f42f66831`.
- Pin registry: `/tmp/misaka-rfc-deadline-legacy-reorg-pins-20261025.json`.
- Focused controller: `/tmp/misaka-rfc-deadline-legacy-reorg-focused-results-20261025.json`.
- Common wave verification: [deadline / legacy / reorg verification](0006-0012-deadline-legacy-reorg-verification-2026-10-25.md).

The broader deadline-name regression initially passed 72/73, exposing an older readiness-disclosure fixture's conflicting expiry/root-standing expectation. The exact same failure was reproduced on the prior immutable Core `/tmp/misaka-rfc-payout-provider-market-kaspa_consensus_core-retry4-20261025`, independently of this change: the fixture creates a demand valid only until 1000 and then at 3141 expects `RootNotCommitted`, although the production reader correctly checks expiry first. Reproduction log: `/tmp/misaka-rfc-readiness-demand-expiry-prior-reproduction-20261025.log`. That broader group is not represented as fully passing here; the root common record tracks its separate fixture correction and rerun. The failure does not change the focused court 2/2 result or introduce a new runtime acceptance claim.

## Final coordinated retry1

All ten new parents and eleven related groups passed after the existing readiness test input correction. Earlier pending/failure checkpoints remain historical. [Final source manifest, pins and 153 distinct passing parents](0006-0012-deadline-legacy-reorg-verification-2026-10-25.md). Scope limits above remain in force.
