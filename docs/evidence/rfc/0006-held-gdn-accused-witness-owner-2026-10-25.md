# RFC-0006 held-GDN accused witness-copy owner — 2026-10-25

## Actual consumer

After retaining authenticated producer answers and making the independently admitted complete
answer batch, the actual Base0 transition/convolution builders called
`gdn_checkpoint_witness_from_disclosures_v2`. That native helper allocates an outer
`Vec<PalwGdnStateChunkWitnessV2>`, copies every selected chunk byte vector and sibling path, and
copies the first anchor opening. None of the earlier answer/leg/output owners covered this
second independently allocated accused witness.

A new borrowed scan checks the existing native demand count, StateChunk variants, exact
checkpoint/chunk identities and equal anchors before copying. Its checked request is the actual
answer count times `size_of::<PalwGdnStateChunkWitnessV2>`, plus the destination lengths of all
chunk byte vectors and Hash64 sibling vectors, plus the first anchor opening's Hash64 siblings
and the leaf's native heap coverage. Vec clone destination lengths are counted; spare source
capacity and repeated copies of the shared anchor are not invented. Unknown/overflow native
heap coverage is Resource. Exhaustive output/opening/chunk destructuring requires review if
those native types acquire fields. Actual output capacities are checked against the request.
This is an explicit typed copy request, not an encoded-byte multiplier or a physical RSS bound.

Admission precedes the native witness constructor. The immutable witness owner can build a
witness-bearing native refutation while retaining that same guard. Additive v4 transition and
convolution APIs reuse the existing native assembly and return this guarded result. The actual
held filer now calls v4, keeps the guard across the real native court, and admits the signed
output inside its consuming handoff callback. The output acquires its independent retained
ticket before that callback returns and the input guard retires. Error/NoFaultFound paths drop
the witness-bearing input before its ticket. Intermediate carrier mapping moves without cloning.

The generic callback return type does not by itself guarantee retained output ownership. That
obligation is met by the concrete node callback's existing `action_owner::accusation`, not an
assumption about arbitrary generic users. Legacy raw/v3 interfaces keep their earlier ownership
scope; the actual held caller selects the new v4 route. No Core, wire, policy, court-authentication
or selected-verifier rule changes.

## Verification status

The parent's actual node build passed in 6m18s. The new node filter passed **2/2 in 9.01
seconds** in `/tmp/misaka-rfc-gdn-witness-owned-node-tests-20261025.log`. Its children passed in
5.63 seconds (both real transition/convolution callers) and 3.24 seconds (direct copy control);
they are included in their parent counts, not extra distinct cases. Build duration is not a
model cost measurement. The complete held-filer regression passed **24/24 in 132.45 seconds**
in `/tmp/misaka-rfc-gdn-witness-full-held-tests-20261025.log`, including these same two new cases
and the earlier 22. This overlapping run is not added as new distinct cases. Concurrent consensus
and external Cursor compilation affected elapsed time; no performance change or cost benchmark
is inferred from this duration.

Pinned actual node binary: `/tmp/misaka-rfc-witness-court-cone-floor-node-20261025`, SHA-256
`15588ac14c3fd269b1e3bf8272f09c1119bda176a02016b0346db969672f321b`.
No Cargo invocation was started by this worker. Both new leaves were individually formatted,
existing modified files passed parser-only checks, and scoped `git diff --check` passed. Docs
checker passed with 43 baseline missing references and zero new regressions, not a clean strict
audit.

New node filter: `palw_gdn_held_filer::tests::witness_owner_tests` (two parent cases with two
isolated children included within their parents). One child uses real borrowed authenticated
answers, exhausts quota before the witness copy, distinguishes foreign-unit shape from local
Resource, and checks source preservation. After retry it compares the native witness and measures
the exact outstanding copy ticket through isolated logical-ledger reservation probes. It checks
independent output-ticket survival and value-before-ticket destruction on an error handoff.

The other child drives both actual v4 transition and convolution builders. After leg scratch
retires it requires exactly the witness-copy request to remain outstanding, runs each real native
court, and requires that same guard to survive court-scratch retirement. It observes overlap at
the actual signer, returns an independently retained signed output, retires it, then also drives
the actual retained filer to a terminal-checked accusation for both cases. This verifies the
connected normal caller and ownership lifetime; it does not simulate a court acceptance or
replace native checking with a mapper fixture.

There is deliberately no claim that an actual step quota was selectively exhausted at this small
witness copy after permitting a larger preceding leg/batch workspace. The direct borrowed-copy
quota control and the connected real normal caller/lifetime controls are separate evidence.
An earlier leg refusal is not counted as a successful witness-copy refusal test. Children and
overlapping full held-filer regression groups will not be summed as distinct cases.

## Explicit remaining scope

Only the accused witness-copy role inside the returned refutation is admitted here. The full
refutation's required/input nested vectors, tile copies, predecessor replay and its witness,
step/checkpoint opening scratch, prefix state construction, authentication scratch, generic
case-book/history maps and PerDecodeCall served-chunk/frontier reconstruction remain independent
resource boundaries. No prior ticket retroactively covers those allocations. Typed native court
Resource handling remains in place; existing backend String refusal boundaries are not classified
from their text.

Formal K3 specification/manifest/weights/signed inputs remain uncreated and all fifteen campaign
gates remain UNKNOWN. These tiny mechanisms are not all-model costs or an accepted multi-process
court campaign. Public activation stays None, ordinary selected-source/V6 bootstrap refusal and
model tiers remain, and selected-verifier claim authentication is unchanged.
