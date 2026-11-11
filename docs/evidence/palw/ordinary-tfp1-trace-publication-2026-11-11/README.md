# Ordinary TFP1 Trace publication and native metadata — 2026-11-11

This continues the [ordinary replay adapter](../legacy-tfp1-public-replay-2026-11-11/README.md)
and [TFO3 native pairing](../tfo3-native-pairing-2026-11-11/README.md).
It is implementation of the adopted vertical plan, not public activation,
accepted TFP1, selected-panel approval, Final or main push.

## Connected material consumers

The ordinary Trace publisher reads the original retained capture through the
existing TirBackendV1 bounded capture reader. It checks the exact FP claim,
signed companion carrier, Original ID, registered Source, native prompt form
and literal prompt. Each advance admits and polls one native slice, authenticates
the StepLeaf against Source, then writes at most one existing provider unit.
Every poll, including preparation and failure, consumes its admitted work.
Every write requires its own operational admission. The provider owner must
reserve the full conservative retained wire bound before capture opens and
hold partial files independently of the cursor. The capture's admitted length
is pinned; subsequent reads cannot expand it. Retrying a failed write repeats
charged work. The fixed Trace index is written only after all leaves and the
exact signed raw-byte total agree. Trace completion does not publish the
all-scope manifest or assert availability of other materials.

The ordinary ExecutionRules codec is separate from the existing wide codec.
Its 512-byte body contains eight native identities: program graph, primitive
set, tiled logits scheme, generic context trace scheme, CU ruleset, full FP
job, execution context and prompt commitment form. The full FP job commits
selector and stop configuration. It retains the native CU ID as subject;
its new content root authenticates this complete descriptor. This is data
identity, not a proof of model computation or an enabled relation plan.
The decoded program is stream-hashed against the registered class's literal
program bytes; layout identity is also stream-hashed without a duplicate
serialization buffer. The caller still accounts for that work.

Cold InitialState and Dependencies reuse their existing 128-byte and 68-byte
preimages byte-for-byte. New helpers derive them from native FP, Source and
registered class instead of a candidate OriginalCommit. The fresh V3/V4 path
rejects tails and inherited prefix state. The TFO3 companion check now derives
these roots independently: a caller and candidate agreeing on invented roots
cannot pass. Other dependency forms need their own explicit material relation.
Ordinary replay fetches and compares all three exact descriptors and bodies
before opening independent replay, and includes them in its aggregate body
byte cap. Existing wide rules and signed meanings are unchanged.

## Evidence and limits

Implementation commit: `f888eae3a`. The fixed candidate passed three tests:
ordinary capture/provider/replay (0.40s), existing wide replay (0.78s), and
native TFO3 pairing/material checks (0.43s). Remote build took 2m34s; native
test build took 3m04s. See [summary](summary.json), [source hashes](source-snapshot.sha256),
[remote test](remote-build-and-test.log), [wide regression](wide-regression.log)
and [native test](native-pairing-test.log). The local fixture
uses a tiny native execution, real companion signature, bounded capture
reader, existing filesystem provider and independent legacy replay. It does
not exercise a production export permit or public network acceptance.

The [consumer map](remaining-connections.md) distinguishes the required
production ownership from the local adapter. In particular, the daemon's
phase-12 prepared-index permit does not authorize these provider exports.
A dedicated paid export/retention path must be connected to the actual
native paired-claim reader. Full manifest production, cold artifact transport,
full FP retention, all protected cases and the complete quote/admission
profile remain required. No public polling, deployment or transaction was
performed for this change.

The subsequent user-adopted [HF acquisition change](../../../rfc/0052-hugging-face-model-acquisition-and-claim-materials.md) removes mandatory model-weight redistribution from the new HF profile. This publisher handles claim-specific Trace, so that work remains applicable.
