# Trained local ordinary forward — a867

Source `a867e78e05f8fdfb6586c21c8ddbf64f76dd159b` passed 39 local
regressions and three actual pretrained-model cases. Ordinary forward creates
generated IDs and the existing output commitment without constructing the
evidence computation record. A separately labelled dense execution starts
fresh executor state and borrows the same owned parameters; its generated IDs
and output commitment matched in every case.

| Model | Original maximum context | Input / generated tokens | Parameter load seconds | Ordinary seconds | Dense seconds | Sampled owned-PG RSS maximum |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| HuggingFaceTB/SmolLM2-360M | 8192 | 16 / 1 | 41.921 | 17.107 | 17.906 | 498368512 bytes |
| HuggingFaceTB/SmolLM2-135M | 8192 | 16 / 1 | 16.958 | 7.097 | 7.656 | 227328000 bytes |
| EleutherAI/pythia-70m | 2048 | 16 / 1 | 9.457 | 1.507 | 1.566 | 133267456 bytes |

Whole process wall times were 77.108, 31.821 and 12.676 seconds. Generated IDs
were respectively 30, 30 and 273. Original class, artifact root, registered
tokenizer and job-context pins were enforced before the report. Pythia retained
the adopted i64 Q24 final-output profile. This phase did not convert models or
change their context, parameter values, arithmetic or token-selection rules.

The earlier full working-memory lease was retained unnecessarily after CPU
completion. Two overlapping executor reservations made the 360M comparison
infeasible under the 8 GiB requested-storage cap. The implementation now drops
the actual scratch and executor, acquires a small output lease while the full
lease remains live, then releases the full lease. Both reservations are charged
before execution; late output-reservation denial returns no result and keeps
spent attempt/work/request counters. The original full workspace quote remains
unchanged. Four additional regression cases exercise that handoff, denial,
exact/one-below limits and the subsequent dense reservation.

Actual requested-storage peaks were 5236756548, 3331652795 and 2992009523 bytes.
These are requested-allocation ledger values. macOS physical RSS enforcement
was unsupported; the external process-group monitor samples RSS and can kill
after a sample. Neither these figures nor the 8 GiB ledger certify a physical
memory bound. CPU limits were finite 300/180/120 process seconds, with a separate
900-second wall deadline for each fresh process. All six list/run process groups
were reaped, without live descendants or zombies.

## Records and regression recovery

- [Actual trained3](ordinary-trained3-a867-functional-v4-20261102.json), SHA256 `f452db631aac4b0b60ef447a3cb389ad4eebaab4e5e42cbdd5e21c3b5cd44796`.
- [Initial same-source regression invocation](ordinary39-a867-20261102.json), SHA256 `9c6b24131e701954d15db4e0c4d9c7c719b4e2a9ff7829538ce8765edd4c5b8a`: 34 successes; status remains **INCOMPLETE** because the subsequent A7 build reached the 512 MiB disk stop.
- [A7-only recovery](ordinary39-a867-a7-recovery-20261102.json), SHA256 `d137d220decd36cdc0d2d282a2afda6e33754480ad1187c7edff3d07e6dbc8d9`: five successes, three trained declarations ignored. They are not added as model executions.

The derived 39-case record combines those two physical invocations at the same
source, source snapshot, native inputs and build profiles: Core5, executor5,
ordinary18, metadata6 and caller5. It retains both original manifests, their
individual environments/CXX guards and every build/run origin. It does not
rename the original INCOMPLETE record to PASS. Earlier E0716 and invalid-empty-PRE
fixture failures remain separate raw records; normal-form checks and the test
assertions were preserved when fixing the fixtures.

[The derived record](ordinary-controls-20261102/positive-a0-ordinary39-a867-composed-20261102.json)
has SHA256 `dfb2bbeff448a82a8c2ee65abff7ced22a14344adaab425c3f924f6053a06ba2`.
[Control, source-review and independent-audit snapshots](ordinary-controls-20261102/index.json)
preserve their bytes and original absolute paths.

The successful caller executable has 25455384 bytes and SHA256
`463ecdc3803862291c58e6aeb6b2a2c00c57f42f646871fa9e0396528bb095b7`.
Actual Cargo JSON checked the supported Rust 1.93/Borsh Source profile. Runtime
used the default libtest stack, Core opt0/debug0 and caller/executor opt0/debug2;
no stack override, global Rustflags or C++ compilation was used.

## Qualification boundary

The actual model record explicitly retains **COSMETIC_LINT_PENDING**: Core lint
had passed, executor lint stopped on one `collapsible_if`, and A7/SDK lint had
not run. Only after these model executions, source
`bf3e523445a6de3b70278c5ad5e1a7609419f416` replaced that nested optional-layout
predicate with an equivalent let-chain. All other 4955 source files and all
other bytes of that file were unchanged. The bf3 lint result is separate; the
model executions are not relabelled as bf3 runs.
Subsequent source `7a0ffcc7fb318fdf025d60beeb45f4b411539eec` only normalizes
the platform-infinity constant in a pure CPU-limit test through checked integer
conversion. The CPU-limit implementation and model execution code were not
changed. Independent reviews confirmed these two exact style/test paths and
the other 4954 source-file hashes. Their later lint records remain separate
from the actual a867 model executions.

[Final Clippy record](plain-clippy4-7a0-20261102.json), SHA256
`4908b3792bb7fd9009bfd9a4cff395eba059c3a805a79b9f24a76aa696a698f5`,
passed all four commands and eight selected targets at source7a0: Core library
and no-allocation integration, executor library and two integrations, A7 library
and caller integration, and SDK library. Actual metadata profiles were checked;
no tests or models were executed by lint. Earlier profile-placement and style
failures remain intact. The completed lint does not rewrite the original
runtime record's pending-lint status or change its source qualification.

This is local short-job self-consistency with original maximum-context metadata,
not execution of every position at that maximum. Parameter loading verifies
the owned weight root and independent identities; the saved full-file SHA is
not a publisher or numerical-correctness certification. Dense follows ordinary
and may benefit from warmed parameter-range/OS caches. The timings do not
establish cold/warm speedups or an independent Hugging Face numerical match.

Native registration, standard carrier admission, independent second-node replay
and same-DB recovery were qualified separately at source50fe in the
[registration report](trained-native-registration-50fe-results-20261102.md).
This local API does not install native Assignment/Result/role/funding authority,
provide Node/P certification or grant rewards/consensus credit. Public presets
remain disabled; the separate native-assignment RFC/ADR drafts remain unadopted.
