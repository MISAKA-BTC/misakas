# RFC-0029 allocation encoded-length counting 

Status: integrated no-run build and focused/regression runtime PASS.

The actual complete allocation calls `original_controls`, which currently encodes five values solely to read their lengths: the full component tuple, original commitment, header, native config, and owner work rules. Replacement is the same Borsh serializer with its counting writer (`object_length`), preserving exact byte counts and existing pricing formulas while eliminating these temporary byte buffers. No additional reservation would be added to the Source peak.

The connected test compares legacy encoded lengths with counted lengths on real existing allocation fixtures for plain/flat and unconstrained/constrained variants. It then verifies actual control/validation/footprint fields and native plan identity recomputation. It passed within the eight-test allocation module run recorded below. These fixtures do not establish model execution, activation, approval or public-chain campaign completion.

## Read-only simultaneous-role audit of existing Source peak

Source retains a header and RUN-sized window vector, not its temporary verified StepSpace. The common allocation finish creates one decode-view context, one registered binding (raw program bytes, layout, context), and one verified StepSpace. The resolver's first verification temporary drops at the semicolon before its returned binding is used for the second verification. Consequently at most one decoded program/analysis/StepSpace is alive, with two layouts and three contexts when including the retained Source header; the Source's retained window vector is the same RUN role.

The existing Source construction quote explicitly retains one program/analysis/StepSpace peak, one raw program clone, two actual typed layouts, three conservative full-header context envelopes, RUN window vector, and hash writers. This supports the view/binding/verified subset already within the held conservative peak; merely seeing these later objects is not grounds for another reservation. It does not establish coverage of terminal inventory/range sizing, semantic families, plan/hash writers, or arbitrary allocator/runtime internals. Those roles require their own simultaneous-layout audit. The earlier header copy was outside the later Source admission and is handled independently in the preceding wave.

## Actual pinned serializer contract and changed consumer

Cargo.lock pins Borsh 1.5.1. Its `ser/helpers.rs::to_vec` allocates a Vec with initial capacity 1024 and calls `value.serialize(&mut result)`. `object_length` calls the same `value.serialize` with a stack `LengthWriter { len }`; each `write` performs checked `usize::checked_add`, returning the existing I/O OutOfMemory error on overflow. It allocates no byte buffer itself. Thus the five calls in actual `original_controls` now count rather than buffer, with the same `map_err(|e| e.to_string())` mapping and existing u64 conversion. Serializer-internal allocations are not claimed removed.

Both raw and native complete allocation variants share this real consumer. Pricing equations, semantic components, witness limits, frame bytes, rent, fees, resource/allocation IDs, and all hash serialization calls remain unchanged. No new resource reservation or authority is added. The test is included only within the existing allocation tests module; expected filter `allocation_counting_lengths` (one test covering four actual fixture variants). Existing allocation module, native derivation/header/preparation, Source3/Output/funded-candidate regressions will check the connected behavior. Final runtime results are recorded below.

## Final verification

Integrated Core / Consensus / Node no-run build passed in 4m15s. Frozen six-file source identity record: `/tmp/misaka-rfc-counting-import-clock-wave-source-20261025/record.json`. Immutable Core binary: `/tmp/misaka-rfc-counting-import-clock-core-20261025`, SHA256 `cf5826362e5088c402204ac7b066604b780c2c7f7966869cfcd77aa622644fa0`.

- Allocation module **8/8 PASS**, .74s, `/tmp/misaka-rfc-counting-import-clock-allocation-tests-20261025.log`. This includes the new single counting-length test; its four fixture variants are not four independent tests.
- Native derivation **1/1 PASS**, .13s, `/tmp/misaka-rfc-counting-import-clock-derive-regression-20261025.log`.
- Preparation **2/2 PASS**, .73s, `/tmp/misaka-rfc-counting-import-clock-preparation-regression-20261025.log`.
- Header copy **2/2 PASS**, .43s, `/tmp/misaka-rfc-counting-import-clock-header-regression-20261025.log`.
- Source3 **4/4 PASS**, 1.74s, `/tmp/misaka-rfc-counting-import-clock-source3-regression-20261025.log`.
- Output **2/2 PASS**, .22s, `/tmp/misaka-rfc-counting-import-clock-output-regression-20261025.log`.
- Funded candidate **1/1 PASS**, 2.44s, `/tmp/misaka-rfc-counting-import-clock-funded-regression-20261025.log`.

Isolated children are included in parent counts. The results validate exact serializer lengths and the existing connected tiny-fixture native behavior; they do not establish all allocator coverage, actual model capture, selected approval or any campaign/activation gate.
