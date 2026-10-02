# Qwen3.6-35B-A3B — 2M held-context artifact

This is the hybrid twin of [`qwen25-a16-2m-held-artifact.md`](qwen25-a16-2m-held-artifact.md).
It is not interchangeable with the 512-position `qwen36.palwq36`.

**Status on testnet-12:** there is no hybrid genesis row. A held hybrid row is registered by
transaction (the model registry, context ladder and held-context fences are armed from DAA 0), but
on the current build every held hybrid attempt fails at prefill position 15
(`ConvIsNotTheGeometrys`: the graph-v7 held map gathers a convolution window the Qwen3.6-35B-A3B
engine does not hold), so a registered row produces no blocks until a new map version ships. See
`PALW_T12_HYBRID_N_CTX` in `consensus/core/src/config/params.rs` and
[palw-add-a-model-runbook.md](palw-add-a-model-runbook.md) §0.

| field | value |
| --- | --- |
| catalog model id | `Qwen3.6-35B-A3B/graph-v7@2097152` |
| class id | `b4b891afe49a59f570283233e95f67b3f6887a9d4483686ead64a7ce025d1e6a4a5c4a3eae69cd9339f0ec2e9ecd12904b91e8bb52bb446509448c13e459fac3` |
| graph | ADR-0103 graph-v7 (graph-v6 under the held composition) |
| `n_ctx` / `max_position` | `2,097,152` |
| converter default it replaces | `--context 512` (`Qwen3.6-35B-A3B/graph-v3`) |

A 2M claim needs this class and a 2M artifact. Do not rely on the pairing check to catch a 512 file:
the hybrid pairing does not compare context widths, so the 512 file's sidecar also lists the
`graph-v7@2097152` row (`consensus/core/src/config/class-manifests/qwen36-35b-a3b-512.palwmanifest`),
and its rotary table covers only 512 positions. Convert at the width you register.

Build the artifact from the same GGUF the 512 conversion used — do not relabel the 512 file:

```bash
qwen36-convert --url <gguf url> --header header.bin \
  --out qwen36-2m.palwq36 --context 2097152
```

or locally:

```bash
qwen36-convert --gguf /path/to/Qwen3.6-abliterated-35B-A3B-Q4_K_M.gguf \
  --out qwen36-2m.palwq36 --context 2097152
```

`--context 2097152` is this family's rotary-table ceiling (`d_head` 256 → 128 pairs × 2M =
`RopeTableV1::MAX_TABLE_ENTRIES`). The rope table alone is about 2 GiB; the mapped weights
stay ~34 GiB.

Before a producer or seat loads the file:

```bash
shasum -a 256 qwen36-2m.palwq36
palw-class inspect --network testnet-12 qwen36-2m.palwq36
```

Register naming the catalog id so the 512 siblings are not ambiguous. Add these to the node that already runs the bond (or use `misaka model add`). Never
add them to a second `kaspad` started beside that node: it would run the same bond's round lane and
double-sign its permits (`docs/palw-add-a-model-runbook.md` §5).

```text
--palw-register-class Qwen3.6-35B-A3B/graph-v7@2097152
--palw-class-artifact /path/to/qwen36-2m.palwq36
```
