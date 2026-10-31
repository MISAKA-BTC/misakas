# A3 — PALWTRC2 frozen-candidate results

**18 selected tests passed, zero failures**, on exact source **`c667259a6a8554de881e8b57f1b783737a98c17a`**,
branch `codex/a3-wide-capture-v2`. Strict exec library Clippy
(`--no-deps -- -D warnings`) passed; documentation no-new-regressions passed
with 43 known occurrences and zero new ones. Historical 49/42/A1/other-role results
are not added or treated as Root-current whole-system success.

| Same-source scope | Passed |
|---|---:|
| New bounded capture2 / identity / resource / refusal-order | 7 |
| Existing PALWTIRC codec | 4 |
| Existing explicit wide run | 7 |

Import sequence after run source 79b8804d9/evidence ee6299930: **c14d47b7d →
2faef030d → c667259a6**. c14 and 2fa were source-only/review stages; no Cargo was
run on those commits. Two independent reviewers found an early work-order issue,
2fa fixed it, and c667 added A0's direct regression. A0's independent review
reported no other blocker. Source was fixed before the granted limited tests.
No mutable compile/preflight/test failure occurred in this increment.

[Exact commands, source/tree/file/binary/static-archive/log hashes](A3-r1-evidence/wide-capture-v2-c667259a6/validation.json)
record this 18-test run. The existing A3 cache and existing static RocksDB archive
were used with two jobs, debug0, no incremental compilation and Core opt0. New
targets, C++ rebuilds, Root/A1 cache use and other-cache cleanup were not requested.
The deliberate catch_unwind panic diagnostic from the earlier wide V1-refusal
regression remains an expected rejection with its outer suite passing.

[API/version/ownership contract](A3-wide-capture-v2.md) separates the new
**PALWTRC2** five-field body (unchanged binding DTO version 1; i64 rows) from old
PALWTIRC/i32 bytes. New decode is bounded before each peer-controlled Vec,
including native row/leaf headers, metadata, eight-byte lanes, leaf payloads and
total payload. It has no generic BorshDeserialize entrypoint. Both input frame
and decoded output coexist in its operation cap; EOF and cross-magic are strict.
The wire oracle is an independent top-level five-field Borsh shadow plus literal
magic/version/extreme bytes, sharing the unchanged nested binding/leaf DTOs;
it is not claimed as a full immutable file-hash golden.

The producer constructor copies a borrowed already-completed wide run's original
rows, IDs and roots and moves the pre-reserved optional dense leaf Vec, without
inference/replay. Its operation quote adds the original run reservation, fresh
copies, the moved Vec's actual outer/payload capacities once, metadata scratch
and declared external owners. Empty fold-only or full N dense data is accepted;
partial dense data is refused. The old DTO/encoder and its default decode budget
remain unchanged, and all four old codec regressions pass.

Authentication borrows independently pre-admitted opaque V2 geometry and still
requires the independent literal expected binding/root set. The token alone
does not contain expected roots. Class/artifact/full context/layout/program/count/
roots remain pinned; a streaming program-byte comparison avoids generic program
decoding or constructing a new StepSpace. Metadata/prompt scratch, temporary
lane reads/hash writer, saved dense hashes and simultaneous dense/row/tile Merkle
workspaces are admitted before private allocation/hashing. The last regression
shows that a decodable malformed row width with zero work budget refuses at
minimum visits first, and with sufficient work refuses at row width.

Coherently recomputed wrong-selection/OOV IDs, an out-of-range raw I8 leaf and
row-versus-dense disagreement can authenticate **as data identity only** when
they match independently selected test roots. Original expected roots reject
substitutions. Canonical geometry tests recompute their changed roots too, so
root mismatch does not mask version/width/count/order/coordinate rejection.
There is no selector check, arithmetic acceptance, Miner conviction or Court
outcome in this API.

Tests also cover every hostile vector-length location, prefix truncation,
trailing bytes, header amplification, spare moved capacity, complete additive
build/decode/encode/auth peaks and exact/one-below ceilings. Caller test objects
use a conservative declared 64 MiB external envelope. These are **logical caps,
not actual host leases or physical RSS measurements**. Retained original frames/
runs, independently admitted space/metadata, parameters/plans and prior exports
must be included by the caller when not already counted in an operation.

Hello2/cell/IPC remains the next separate increment. No backend/default public
route, native registration/admission, funding/fees, Gate C, credit or activation
is added. Actual trained-cohort reproduction at original maximum context stays a
later SDK/native-wide task; these local fixtures do not count as that outcome.
