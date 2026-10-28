# P2 source branch reuse audit — 2026-10-22

Read-only source inspection of handover head `2a6ea1a4ff934635978144a84f8ec96351d2bdb8` on `g14/door-design`. This is not an integration, measurement or adoption record.

| Branch component | Reuse boundary |
| --- | --- |
| `palw_tir_history_bisection_v1.rs` | Pure history-tile/position localization and A16 kernel terminal are candidate calculation components. Fixed 16-ary padded-tree, two-party first-disagreement semantics differ from the current ragged native all-path geometry. Keep a distinct version; add checked/bounded allocation and original/source/profile/role authentication before any native use. |
| `palw_tir_designated_answers_v1.rs`, registry/tag245/processor changes | Old candidate uses 32-byte identities and additional state/codec/fence rules. All-designated-parties absence is not the approved F02 actual-supply/eligible-duty default. Do not cherry-pick this as the new commonfrontier. |
| `palw_tir_adjudication_slot_v1.rs` | Owner FIFO and limited reoccupation do not establish F03 progress for one outsider under continuous colluding owner refill. Do not replace the approved owner-independent frontier. |
| Added frame-cost fit functions | Constants fitted from ADR-0190 examples and `derived_upper` door rows are assumptions/derived results, not measured native capacity. They cannot establish real-model door <=3,120 or a new fee/service profile. |
| `tests/palw_tir_court_verify_cpu.rs` | Port the ignored fixture verification harness to current APIs with exact source/executable provenance. Separate wall elapsed, process CPU and process peak RSS. Setup and fixture generation are not free; retain setup-only runs. Fixture operations are not real-model all-branch maximum costs. |

The current evidence store already has parameter multiproof/carriage and builder/checker support. P2 must compare serialized **whole evidence** and actual CPU/IO/transfer/retention, not only a reused multiproof's size. Numerical fold checks remain mandatory. Source branch history/position optimization requires a coherent new-version dependency graph, native authority/supply/funding and validation before it can shorten a door.

P0/P1 remain open as recorded in the [completion ledger](../../../design/palw/integration/g14-active-completion-ledger-2026-10-21.md); this audit does not make those gates optional.
