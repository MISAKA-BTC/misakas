# Qwen2.5-1.5B A16 — 2M held-context artifact

This record identifies the real PALW artifact behind testnet-12's wide dense genesis row
(`PALW_T12_GENESIS_HELD_ROWS` in `consensus/core/src/config/params.rs`). It is not a placeholder and
it is not interchangeable with the 512- or 8,192-position artifacts.

| field | value |
| --- | --- |
| catalog model id | `Qwen/Qwen2.5-1.5B/graph-v7@2097152` |
| class id | `74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a` |
| PALW size | `2,868,906,956` bytes |
| PALW SHA-256 | `35d41da6272010035894d76bbd0c17edfb76f20a6b6625063e639ccd06739bf3` |
| inventory root (what the chain registers) | `f63af2c46b3816f6a16c168a130d28ec95b0ca5107da76d4abe24e4d9396e65c80d6d83014c47855e2394af20ce3333a59359fd3eb06090fdc7bfd502f75c7c2` |
| PALW container digest | `b5baca6364135a62bd4512a58c2ca747373019a495505968d884b2c0e52e4ce9322a8af0db90d3ec1001c15b5a89fe0e8008519e55a5f3f865e15562fb8967ae` |
| checkpoint | `Qwen/Qwen2.5-1.5B-Instruct`, `model.safetensors` SHA-256 `dd924a11b4c220f385b51ffa522daea7c9f3d850e31b162bb5661df483c6d3ee` |
| converter revision | `220a0231bdd533fa68a18d69efe1dc00d266dad5` |

The artifact was generated from the official float checkpoint, not by relabelling the 512 artifact:

```bash
qwen25-convert /path/to/Qwen2.5-1.5B-Instruct --a16 --n-ctx 2097152 \
  --out qwen25-1.5b-a16-2m.palwart
```

The converter fully decodes and re-reads the container. Its fixed reference set reported
top-1 agreement `45/57`, top-5 `56/57`, rank correlation `0.8932`, and `FAITHFUL true`.

Before copying a file to any producer or seat, verify both the file hash and the class pairing:

```bash
shasum -a 256 qwen25-1.5b-a16-2m.palwart
palw-certify bind --artifact qwen25-1.5b-a16-2m.palwart --lane fp --out qwen25-2m-fp.cert
```

The certificate must name the class id above and the artifact header's `max_position` must be
`2097152`. Keep the `.palwmanifest` sidecar (`palw-class manifest --network testnet-12 <file>`) next
to the artifact; the genesis card reads the root and digest above from the committed copy,
`consensus/core/src/config/class-manifests/qwen25-1.5b-a16-2m.palwmanifest`.

The class is registered at testnet-12's genesis, so it is never registered again. At launch it admits
no claims: it is a class that needs a measured verification-deadline row, and without one active it
is refused `ClassDeadlineUnmeasured` on every lane (`palw_state_v2.rs`, ADR-0152 §4-quater). A 2M
seat or attempt is also far heavier than an 8k one; see
[testnet12-join-mining.md](testnet12-join-mining.md) for sizing.
