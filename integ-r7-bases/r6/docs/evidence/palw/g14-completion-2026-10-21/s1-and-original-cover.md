# S1 discharge and original numerical dependency checkpoint — 2026-10-21

Status: **isolated implementation and selected verification; G14 incomplete/unarmed**. No pre push or deployment.

## Implemented

RFC-0034/ADR-0201 S1 now has a private native transition and separate rooted provenance (key25/row31). Creation captures the actual active prefix removed in the same court-termination checkpoint. Subsequent chunk qualification records only the native wrapper's actual accepting point within the original signed assembly window. Complete timely material must pass the original signed declaration and native v5 on the same state before a one-time positive fulfillment record can remove **that declaration's nominal deposit exposure**. Original raw archive/ending cause, actual collateral/debits, court stake, opposite side and other holds remain. This is neither an actual refund nor a Panel payment.

Unclocked stored material never acquires an invented past timestamp. Equal bytes can be genuinely resubmitted through a new on-time native event and receive that current point, without altering the old bytes/cause. Unknown automatic qualification, late +1, changed bytes, already-qualified duplicates and repeated discharge are refused. Public paid archive dispatch is still closed. The unresolved whole-service book and other duties still prevent global exit/pruning.

Wire time/work/OOM failures retain `NativeResource`; malformed/truncated material remains a semantic refusal. Loaded proof assembly and decode shape receive memory admission before construction. Forward checking reuses the existing native-v5 verifier. Signature-message quotes cover the actual digest-vector clone and Borsh serializer capacity with the existing encoded-value method. Actual slice/Vec message bytes match at count1/2/32 for both sides/verdicts; old signature domains, wire fields, deadlines and caps are unchanged.

The parameter dependency explorer now shares one verified binding/inventory/range-analysis prelude across an entire original. The opaque public source reconstructs its native binding from the actual registered program/layout/header and enumerates every eligible numerical leaf, returning the canonical root-to-parameter relation and union. Original leaf scans, total exploration edges, total visited coordinates and repeated root memberships have separate explicit budgets; failure returns no partial success. Named and original-wide results agree, and root count agrees with the independent existing numerical topology formula.

These are numerical parameter dependencies only. Source/semantic/prompt/other witness families, all resource/capital/fee/authority contracts, acquisition/retention and atomic paid original admission remain required. Prelude/scratch/output allocation and physical costs need their own native preflight/profile; exploration counters are not CPU time, paid fees, profit or door measurements.

## Verification

[Release build and exact source hashes/executables](s1-cover-final-build.json), [raw build log](s1-cover-final-build.log), [tracked patch](s1-cover-final-tracked.patch). A subsequent test-helper-only move to `tests/common/` avoids Cargo treating it as an independent target; [fresh layout and identity executable hashes](s1-cover-final-layout.json), [fresh 24-test log](s1-cover-final-layout-and-identities.log). Runtime source hashes match before/after this move. [Reconstruction audit](s1-cover-source-reconstruction.json) reproduces all 20 build-phase source hashes from the parent commit, tracked patch and preserved added files.

The final selection contains **84 unique passes / 0 failures / 0 ignored**:

| Selection | Passes | Overlap |
| --- | --- | --- |
| [Native frame](s1-cover-final-frame.log) | 42 | Includes 7 native archive tests |
| [Archive filter](s1-cover-final-archive.log) | 11 | 7 above overlap; 2 resource/message tests and 2 existing capacity/archive tests add 4 |
| [Causal reservations](s1-cover-final-causal.log) | 6 | Disjoint |
| [D2 routes/root codec](s1-cover-final-d2.log) | 8 | Disjoint |
| [Named parameter dependencies](s1-cover-final-parameter.log) | 6 | Same six rerun after helper move; count once |
| [Original parameter dependencies](s1-cover-final-original-parameter.log) | 4 | Same four rerun after helper move; count once |
| [t12 identity + activation](s1-cover-final-layout-and-identities.log) | 9 + 5 | Fresh current-runtime non-test library |

Native signed-v5 internal fixture cases and adversarial totals/terminal paths are inner assertions, not additional runner passes. The direct D2-only intermediate rustc runs used a preserved library from `510039f9d`; they are not additional final passes. First and second added-fixture runs compiled but failed 4/6 tests before valid range/site setup. Their raw logs, exact test sources, libraries and commands remain in the `parameter-expanded-*` artifacts; no validator/range/cap was relaxed. The third intermediate run passed6. The first S1 candidate passed its selected9+6 but preceded resource/re-observation corrections and is preserved separately.

[Core all-tests type check](s1-cover-final-core-tests-check.log) and [non-test consensus library check](s1-cover-final-consensus-check.log) both exit0 with warnings. The source-derived native/wire inventory required explicit field-order review pins for three pre-existing bounded row decoders. [Two-run generator audit](s1-final-generator-audit.json) reports zero changed output on either run, no new opaque type. Source/scripts whitespace check passes; raw logs/patch evidence retain original bytes.

## Remaining scope

No whole RFC-0026 T01–T15 item is completed by this selection. Actual public funded ingress, whole legal-branch graph and actor contracts, accepting-block real supply/delivery/default, full D2 C1/native draw/signature/fees, dependencies, end-effect monetary collection/refund, global qualified pruning, P2 and real-model door <=3,120, saturation/concurrency/reorg/pruned-IBD node campaigns remain.

The user selected RFC-0035/ADR-0202 A for detailed isolated implementation of real-UTXO-backed operational escrow. That implementation is **not present** in this checkpoint. Current nominal deposits/rent burn are not prefunded service money. No prices, draw cap, holding period, network premise or production rule was adopted by that structural choice. New Account Bond implementation and measured Panel profit/heartbeat remain absent.

[Document check](s1-cover-final-docs-check.log) remains FAIL/exit1 for the same 5 imported missing references plus 43 known baseline occurrences. No new authored missing reference or stale index remains. Imported raw assets and the baseline were not rewritten.
