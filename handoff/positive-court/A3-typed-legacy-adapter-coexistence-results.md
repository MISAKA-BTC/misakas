# A3 — simultaneous owner/material backing correction

Source `922878cc940c6e2e0d1034bd3eabd84cc36d311c`, exact A0 base `79c3f9bca`. The corrected candidate supersedes
source `01b5fed24` and its historical hold evidence `ea7e51a42`; those raw logs
and their post-review warning remain unchanged. Import source `01b5fed24` plus
`922878cc9` onto that base (the intervening evidence-only commit adds no code).

**42 selected tests passed, zero failures**, on this exact source. Strict exec
Clippy (`--lib --no-deps -- -D warnings`) passed. Historical 40/63/148 results
are not added. Full exact commands, source/tree/file/binary hashes and raw logs
are in [validation metadata](A3-r1-evidence/typed-adapter-coexistence-root79/validation.json).

| Scope | Passed |
|---|---:|
| Typed adapter integration and attacks | 8 |
| Typed capture / canonical context | 9 |
| A1 fullrelation | 11 |
| r1 Panel | 7 |
| r1 primitive record | 4 |
| Budget / native-capacity units | 3 |

A0's independent review found that the former `all_claims` quote omitted the
live material slot, while the allocation guard omitted nonzero external stage
reservations. Neither separate check bounded their simultaneous lifetime. The
new constructor requires and persists `StageReservations`. A single checked
inventory now adds index, metadata, cumulative exported claims, current scratch,
all four persistent external owner envelopes, and all three live-material
components. External source/evaluator buffers may be independent of the live
ones, so they are added without an alias assumption. A1's evaluator workspace
already includes its target indices/results; those are counted once within that
component. The public cold quote and constructor cap combined backing before
identity encoding/hashing and index allocation.

The all-claims preflight includes every future header/lane plus all previously
exported claims and live stages before charging/reserving the outer Vec. Later
declarations can only increase the envelope of each same external owner; separate
owners must be pre-summed by the caller. Zero declarations cannot withdraw a
constructor reservation. Dropping or failing a material releases its live slot
only, never external reservations or cumulative work.

The new integration regression uses **all four nonzero external reservations**.
At one byte below the combined quote, cold claims alone and live material alone
both fit, but their coexistence refuses before claim/header charging. After
material Drop the same cold export fits, with spent work unchanged. At the exact
combined quote the same material and complete claims coexist successfully. A
separate unit catches the independent-buffer counterexample (four 100-byte
source/evaluator buffers require 400, rather than component maxima of 200) and
checked overflow. The earlier zero-external live-slot regression remains.

[Adapter design and ownership contract](A3-typed-legacy-adapter.md) retains the
original identity/legacy mapping and range boundaries. New local fullrelation
checks still use a separately declared range-admissible class, actual source
authentication/direct evaluation/A1 checker, and original Miner signatures. The
original sparse declaration remains unverified when its static range condition
fails; no program, context, dtype, old hash/root or range guard is rewritten.

These are logical backing reservations, not measured RSS/allocator/cryptographic
workspace or an LLM benchmark. Extra caller-owned direct binding/preimage/path
exports and explicit copies need separate owner reservations. This prototype
assigns no public typed record/wire/domain/ID and establishes no native
registration/admission, funding/locks/fees, Gate C, consensus credit or activation.
Root79's i32-final limitation remains. Other-role native integration and wide
profiles remain separate.
