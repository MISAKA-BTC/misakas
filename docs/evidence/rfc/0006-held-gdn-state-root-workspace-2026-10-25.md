# RFC-0006 held-GDN recomputed state-root workspace — 2026-10-25

## Actual consumer

The actual held predecessor callback recomputed its own state and hashed its chunks before
the [predecessor witness request](0006-held-gdn-predecessor-witness-owner-2026-10-25.md) existed.
`seat_state_here_v1` called the existing state-root helper without a separate hashing-workspace
ticket. Full chunk hashes, held top/slice hashes, fold levels and Writers therefore preceded
the witness admission. This continuation covers that hashing stage only.

The additive typed `base0_fp_recompute_state_at_covered_root_retained_v3` uses the same native
covered-position conversion, private input checks, teacher-forced walk and class serializer as
the raw entry. After serialization it constructs `gdn_state_root_request_v3`, then calls its
`root_v3`. The actual held predecessor callback now uses this typed path. Resource remains
Resource through Core, Base0's typed root/replay boundary, the native callback and the existing
Resource-prioritizing final map. Dynamic semantic replay reasons still use the existing semantic
side channel; Resource is never formatted into that String. Raw/String recompute entry points
remain unchanged.

The native root function is not used to quote its own cost. Requesting uses only actual borrowed
chunks, native held layout/address/count and checked typed Writer/array spans. `root_v3` admits
the workspace first and calls the same existing `palw_state_chunks_root_for_map_v1` exactly once.
That root's serialization, map domains, enumeration and fold order are not reimplemented.
The returned Hash64 is a scalar. This does not establish computation correctness or replace
claim-level selected-verifier approval.

## Quote and ownership

The native Base0 held wrapper checks chunk byte caps before obtaining layout and checking chunk
count. The new typed route retains that semantic order. Native layout/count/address errors stay
semantic refusals; native u32 representations are not reclassified as quota. Only checked
request arithmetic and actual admission failure are Resource. The new root consumer quotes
held maps; another map explicitly withholds as unsupported rather than taking an unreserved
fallback. Historical raw routes retain their own map support.

The request first admits its native layout's layer Vecs from the profile's actual attention
and recurrence counts. A recurrence layer count is a conservative metadata bound even at a
checkpoint whose layout carries no recurrence. No full-prefix multiplier is used. The request
declares that layout before its metadata lease, so values retire before their ticket, including
construction refusal. The source chunks are borrowed; this ticket does not own them.

Workspace includes actual full leaf count M, top slice count S, maximum actual slice block count
B, native Hash64 slots and sequential fold-level peaks `B+ceil(B/2)` / `S+ceil(S/2)`. Result-based
leaf collection receives the conservative typed push-growth/old-new envelope. The leaf-hashing
phase and folding phase are compared as phases rather than multiplying by selected witnesses.
The fold-role sum is conservative, not minimal allocation or RSS.

Exactly one additional native scratch layout is accounted for while the request layout lives.
The existing root function first completes `palw_state_chunk_leaves_for_map_v1`; that helper's
layout retires when it returns its hash Vec. It then calls `palw_state_root_from_leaves_for_map_v1`,
which constructs a separate layout and lends it to `palw_state_root_from_hashes_v4`. Those two
native scratch layouts are sequential, not simultaneously live. The M hashes remain across
the second phase. The request layout has its own independent metadata ticket throughout both.

The chunk Writer span follows actual code: initial256 bytes, map hash, slice u32, block u32,
length u32 and a bulk actual chunk. It accounts for the old/new allocation when a large bulk
extension exceeds the initial capacity. Top-leaf Writer remains the actual256-byte allocation.
All arithmetic is checked and all sizes are native types, not encoded-size multipliers or
physical allocator overhead estimates. Nested native hashing arrays retire inside the root
call before its workspace ticket drops; the request layout retires before its separate ticket.

## Verification status

The parent's coordinated Core/Consensus/Node private-EVM no-run build **passed in4m26s**.
Frozen20-file source snapshot:
`/tmp/misaka-rfc-root-preparation-canonicality-wave-source-20261025`.
Node pin: `/tmp/misaka-rfc-root-preparation-canonicality-node-20261025`, SHA256
`82fe81cddc23f23dd814afcfedd92f5e974eba53a732cf8dab9c25cdccd9cf7b`.
Build log: `/tmp/misaka-rfc-root-preparation-canonicality-wave-build-20261025.log`.
Build elapsed includes dependencies and is not a cost measurement.

The focused `state_root_tests` filter **passed2/2 in3.43s** on that pin. Its isolated
child runs passed1/1 in1.25s and1/1 in2.08s, respectively; those children are included
in the two parent tests, not additional independent test counts. Runtime log:
`/tmp/misaka-rfc-root-preparation-canonicality-node-tests-20261025.log`.
The same-pin full held regression **passed34/34 in61.87s**, including the new two
and the previous32; these overlapping runs are not added together as independent tests.
Its log is `/tmp/misaka-rfc-root-preparation-canonicality-held-regression-20261025.log`.

This worker has not invoked Cargo. New leaves were formatted individually, existing modified
files passed parser-only checks, and scoped `git diff --check` passed. Static parsing is not
runtime acceptance.

New node filter: `palw_gdn_held_filer::tests::state_root_tests`, two parent tests, each with an
isolated child included. The source is the actual profile384/artifact64 native prefill draw,
second recurrence checkpoint32 and honest predecessor16 used by the earlier predecessor record.
That earlier fixture acceptance is not a result for this new workspace path.

The direct test obtains actual serialized predecessor chunks and the legacy native root first.
It successfully quotes native metadata, then occupies remaining logical quota and requires
`root_v3`'s workspace admission itself to refuse. It checks unchanged source, the existing
byte-cap-before-layout refusal, wrong-count semantic refusal, successful retry to the same
native scalar root and complete scratch/metadata ticket retirement. Quoting never calls the
old root function; the test's legacy comparison is separate from that quote.

The real stage test delegates every forward and serialization to genuine backend kernels.
Immediately after serialization, the wrapper obtains the new request's actual metadata and
workspace amounts without hashing a root, retires that test quote, then uses a real quota lease
to leave exactly metadata-sized room. Actual caller metadata can fit, while its following
nonzero root workspace cannot. This is a quota experiment after serialization, not stage counters
alone and not a leg/answer/rank/prefix refusal substituted for the target. The test requires16
successful native forwards, one completed actual serializer, Resource at the actual held caller,
unchanged candidate/demands/answers/clock and no signature. Releasing the blocker and using fresh
same-job kernels must reach native court and Accuse/sign once. This is scoped native mechanism
coverage, not a campaign.

## Remaining scope

Prefix execution, cache allocation, recurrence-state projection copies, the serializer, its
output chunks' ownership/admission, and callback leaf clone precede this request and remain
open. The other raw recompute/discovery consumers are not switched implicitly. Transition
input-coordinate/preimage/run/path/binding construction, canonical-start rank, other authentication
scratch, book/history maps and PerDecodeCall served-chunk/frontier reconstruction also remain
separate. This is not a whole-predecessor, whole-transition, arbitrary-prefix, all-model cost
or physical RSS bound.

Formal K3 specification/manifest/weights/signed inputs and the formal native39 settlement mapping
remain unprovided. No policy is invented from those missing inputs; all fifteen campaign gates
remain UNKNOWN. Public activation stays None, ordinary selected-source/V6 bootstrap refusal
remains, and the frozen model tiers and selected-verifier claim approval are preserved.
