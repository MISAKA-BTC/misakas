# Qwen3.6-35B-A3B — 2M held-context artifact

This is the hybrid twin of [`qwen25-a16-2m-held-artifact.md`](qwen25-a16-2m-held-artifact.md).
It is not interchangeable with the shipped 512-position `qwen36.palwq36`.

| field | value |
| --- | --- |
| catalog model id | `Qwen3.6-35B-A3B/graph-v7@2097152` |
| graph | ADR-0103 graph-v7 (graph-v6 under the held composition) |
| `n_ctx` / `max_position` | `2,097,152` |
| converter default it replaces | `--context 512` (`Qwen3.6-35B-A3B/graph-v3`) |

The 512-token class stays registered and runnable. A 2M claim needs this class, a 2M
artifact, and testnet-11's held-context fence. Pairing the 512 file with graph-v7, or the
2M file with graph-v3, is refused by shape.

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
palw-class inspect --artifact qwen36-2m.palwq36
```

Register only after the held-context fence, naming the catalog id so the 512 siblings are
not ambiguous:

```text
--palw-register-class Qwen3.6-35B-A3B/graph-v7@2097152
--palw-class-artifact /path/to/qwen36-2m.palwq36
```
