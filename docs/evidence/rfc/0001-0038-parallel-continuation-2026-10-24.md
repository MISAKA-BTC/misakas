# RFC-0001–0038 parallel implementation continuation — 2026-10-24

The user requested parallel implementation across RFC-0001 through RFC-0038. Three workers covered
0001–0012, 0013–0025 and 0026–0038; the integrating worker finished the existing GDN resource-owner
change. These are four concrete continuations, not an exhaustive completion or acceptance of all 38 RFCs.

## Implemented boundaries

- **RFC-0006 SDK:** an explicit router-binding declaration can be signed offline as the existing native
  carrier and passed to the existing wallet submission command. See the [SDK record](0006-router-binding-sdk-2026-10-24.md).
- **RFC-0006/0008 GDN:** signed step accusations reserve their object/Arc, opening copies and bounded
  production signature before construction. The case book and service tick share the same immutable
  owner. An independent outbox copy obtains its own typed payload reservation before deep Clone.
  Ticket bookkeeping nodes are also reserved before allocation. The outbox keeps these tickets while
  its existing carrier loop owns the batch and while unsent copies remain. Duplicate keys conservatively
  keep their tickets until the last copy leaves. Dropping a due/submission stamp does not release data
  tickets. Failed enqueue admission leaves accusation send counts/debounce unchanged for retry.
- **RFC-0023:** checkpoint construction admits its typed maps, nested copies and material encoding peak
  before construction, returns a retained owner, and serializes court sessions by reference. The borrowed
  source state is an input boundary; serialized checkpoint contents and consensus rules are unchanged.
- **RFC-0037:** a spill visitor checks local byte/count/field bounds before allocation and delivers one
  row at a time. Local capacity refusal and malformed framing have distinct I/O error kinds. See the
  [RFC implementation note](../../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md).

## Verification and limits

The GDN owner's actual filer source was compiled against the matching Cargo-built consensus/base0
libraries. The resource-retry regression passed, including pre-construction refusal, shared Arc lifetime,
independent copy lifetime and release after drop (57.76 seconds). The existing separately dependent
false-Valid handoff test was omitted from that standalone harness, as in the previous continuation.
The outbox's actual source and actual EDF function passed four test functions, including an isolated
quota subprocess for refusal, in-flight batch retention, duplicate keys, retries, cancellation recovery and final release.
These are focused source harnesses, not a full-node integration campaign.

The full private kaspad library and all its test targets passed metadata/type checking after combining
worker changes. The SDK library, binary and test targets also passed metadata/type checking; its three
focused runtime tests passed. The final private kaspad metadata/type check completed in 22.63 seconds.

The final actual core unit binary passed **56/56 RFC23 tests** (1.29 seconds, one test thread,
`RUST_MIN_STACK=67108864`) and **6/6 spill-reader tests** (0.01 seconds). The test-only command override
`profile.test.package.kaspa-consensus-core.opt-level=0` reduced code generation cost without changing
repository profiles. The first checkpoint run exposed an existing fixture that expected the stage1
retirement-floor correction while disabling stage1. The final fixture holds stage1 active on both sides
and tests only stage2 `None` versus an unreached activation. Production legacy semantics were preserved;
the existing F05 regression also passed. An intermediate missing test constant import was corrected
before these final runs. Failed/intermediate builds are not counted as passing verification.

Local reproduction logs are `/tmp/misaka-parallel-filers-tests.log`,
`/tmp/misaka-parallel-outbox-tests.log`, `/tmp/misaka-parallel-kaspad-check.log` and
`/tmp/misaka-parallel-final-kaspad-check.log`, `/tmp/misaka-rfc23-final-20261024.log` and
`/tmp/misaka-rfc37-spill-tests-final-20261024.log`. The standalone harnesses use actual source, without
replacing court verdicts or EDF ordering with mocks. Their scope does not cover the full service graph.

GDN's legacy raw enqueues, reservation objects, outbox frame Vec, EDF scratch, due/submitted maps,
carrier encodings, held filer, earlier evidence construction and all other consumers still need their
own complete construction/retention admission. This payload change does not close those boundaries.
The RFC37 visitor is not yet connected to typed import or NativeDemand and grants no verification token.
Physical RSS, whole-model costs, restart/reorg/prune campaigns and production acceptance are unproven here.

## Frozen conditions

`PALW_T12_FULL_ACTIVATION_DAA` remains `None`. No public activation or remote-node deployment is
performed. Selected-source and V6 paid bootstrap refusal remain in place. RFC-0003/0004 remain deferred
until after mainnet; RFC-0017 model screening remains abolished. Model tiers and selected-validator
claim authentication are unchanged. No cryptographic model-computation correctness proof is introduced.
K3 formal inputs remain uncreated; no fixture is promoted to K3 or full campaign acceptance evidence.
