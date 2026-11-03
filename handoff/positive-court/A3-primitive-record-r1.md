# A3 — primitive execution record integration, r1

**148 selected tests passed, zero failures**, on exact source commit `4b8cc795de8a7016510e8e83d1e750e393c1a028` (tree `21eab9232d611f81c85a6ecf0da5915c12fea80a`). Branch: `codex/a3-primitive-record`; A0 base: `d284f5902`. Historical 129-test evidence at `4afa89506` is unchanged and is not counted again here.

`PanelCapture::encode_execution_record(cap)` connects the retained actual capture directly to A0's primitive codec. It copies `result`, artifact, full-logits root, step count, step Merkle root and committed execution root literally, and retains the generated IDs without conversion. The exact extent is **344 + 4 × N bytes**. The record carries no class, registered program, job, logits values or new derivation. Existing dense persistence, capture, dtypes and commitments are unchanged.

The actual integration path is Panel replay/capture → primitive encode/decode → A1 `PreparedCheck::authenticate_record(&self, ...)` → binding rebuilt from its independently authenticated retained class/job → existing `authenticate_witness` → original-author signed evidence frames → actual A1 arithmetic terminal. The normal case reaches `Matched`; a validly signed wrong Miner output reaches `OutputMismatch` after all required relations.

Fourteen corrupt record variants, a too-small cap and a different authenticated Result are refused as deliveries. After each corrupt record, the same borrowed preparation accepts the original record and original signed witness at attempt 0; a final pass checks all original signed frames to `Matched`. No test replaces the witness, resigns it or substitutes a new attempt. A1's distinct `RecordDeliveryError` is not a witness verdict. These tests exercise retry at the role boundary, not a native A4 attempt counter or rollback.

| Required suite | Passed |
|---|---:|
| common-contract | 41 |
| a1-a3-exec | 37 |
| remote-integration | 14 |
| source-guards | 1 |
| a2-regression | 51 |
| legacy-feature-off | 4 |

The 14 remote tests include the three new actual-capture record tests and the accepted A2 cold acquisition/replay pipeline. A1's four record-only tests additionally cover explicit decoded-ID heap caps, wrong signatures and progress preservation. The five-test preflight on `7ce181a55` is retained separately and excluded from the 148 total. `RecordLimits` reserves newly decoded generated storage; retained class/job/plan and hashing/evaluation workspace need separate accounting and are not a complete process/RSS bound.

Minimal import: A1 helper upstream `8e3c2e481` (local cherry-pick `95aef2a9c`), A3 encoder/tests `7ce181a55`, then A3 stable-API/CI adaptation `4b8cc795d`. The independent expanded-plan commit `05c47201d` is not a prerequisite and is absent from this candidate. Do not import a full workspace snapshot. CI now includes both `positive_record` and `positive_panel_record_r1`.

[Validation metadata](A3-r1-evidence/primitive-record-d284f5902/validation.json) pins all six exact commands, source tree and file hashes, unedited raw logs, build settings and 18 actual test binaries. Rust formatting, CI shell syntax and source diff checks passed. Documentation no-new-regressions passed with 43 known issues and zero new occurrences.

This A3 increment installs no native protected anchor or case/scope authority and does not establish native admission, a real UTXO reservation/lock, fee/payment entitlement, Gate C or consensus credit. Public presets and activation remain unchanged.
