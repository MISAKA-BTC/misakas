# RFC-0006 / RFC-0012 historical-header census consumer — 2026-10-24

This closes a validation gap in an existing campaign consumer, not historical header
or signature verification itself. The actual 15-gate campaign has not run. K3 formal
specification, manifest, weights and signed inputs remain unavailable (user: not
created). Activation remains unchanged.

## Existing path and defect

The native [census](../../../consensus/core/src/palw_exec_origin_coverage_v1.rs) counts
eligible legacy Attempt claims, readable original header sizes and unavailable headers.
The private [V2 writer](../../../kaspad/src/private_integration/exec_origin_coverage_v1.rs)
requests up to 256 named rows per failure list, retains complete totals, and binds the
result to its own native sink, DAA and import epoch. The campaign already reads those
files in a public RPC bracket and compares archive/pruned generations.

However, [archive_coverage](../../../scripts/drills/rfc12_acceptance_judgement.py)
previously consumed zero failure totals without checking their partition of `eligible`,
exact native u64 bounds, named list framing or header size consistency. Empty eligible
history also yielded an OBSERVED predicate without exercising historical header reads.

## Change and limits

The [census validator](../../../scripts/drills/rfc06_exec_origin_coverage.py) is connected
to that existing consumer. It checks all native counter widths; `within_cap + oversized
+ unavailable = eligible`; maximum/size consistency; the private writer's truncated
list lengths; canonical JSON Hash64 fields; and distinct claim identities across both
lists. Missing or malformed census data is NotObservable/BLOCKED. Both roles must use
the pinned native cap `1_599_931` bytes (`100_000 × 16 − 69`) and count the same eligible
claims. A cross-language source check derives the native cap from its two chunk
constants, slot-mass expression and origin subtraction; numeric or formula drift fails
the control rather than silently updating the tooling rule. A valid but empty history is
also NotObservable/BLOCKED, after checking cross-role consistency.

Pruned absence remains a local observation and is never promoted to global absence.
The archive must still report no oversized or unavailable eligible headers. No header
bytes, carrier signatures or approvals are revalidated by these checks. This is not a
historical signature replay, migration authorization, soundness measurement or release
approval. The 256-row ceiling is the existing private V2 writer contract, not a new
consensus parameter. Public wire and chain rules remain unchanged.

## Verification

```text
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc06_exec_origin_coverage.py' -v
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_acceptance_judgement.py' -v
```

Ten new controls and all 36 existing acceptance-judgement controls pass. The controls
exercise the actual `coverage_envelope`/`archive_coverage` consumers with counterexamples:
empty history, matching arbitrary caps on both roles, native-cap drift, impossible
partitions, boolean/float/negative/overflow counters, truncated
lists, malformed/repeated claim hashes, contradictory sizes/caps, and malformed pruned
reports. Their dictionaries are oracle test inputs, not node observations or campaign
acceptance evidence. Historical signatures and the remaining real campaign gates stay
UNKNOWN/unexecuted where evidence is absent.

## Harness source closure

The separate 15-gate [campaign](../../../scripts/drills/rfc_integration_campaign.py)
now includes the previously introduced `rfc12_owned_fixture.py` dependency in its
`SOURCE_LEAVES` pins. An AST check over all actual local imports detects omissions;
existing explicitly loaded dynamic sources remain separately listed. The original
frozen file pins and expected SHA values are not rewritten: changed helper bytes
refuse both revalidation and recapture against their original expected hash. Archived
manifests and predeclared inputs are not upgraded in place by this change.

The new historical-header validator belongs to the separate acceptance driver import
closure, not the 15-gate campaign imports. That driver executes from the live checkout
and `phase_identity` requires its plan's commit to equal HEAD and rejects tracked dirty
files before launching the fixture. The helper must therefore be committed with its
consumer for this implementation's evidence to name a complete source revision. There
is no standalone Python package copy/save path in that driver. No full runtime source
integrity or build attestation claim is added here.

Additional controls:

```text
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc_integration_campaign.py' -v
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_acceptance_driver.py' -v
```

All 16 campaign controls (including two new source-pin controls) and four existing
acceptance-driver controls passed. These remain harness tests, not a campaign run.
