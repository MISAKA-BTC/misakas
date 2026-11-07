# Source-backed complete frontend 28

Implementation source, not a validation or checkpoint-support result. Build, conversion,
fidelity, public-court and native Final/redemption verification remain to be run in the
ordered validation phase. The existing historical failures remain unchanged.

`palw-class compile-complete-frontend28 --declaration SOURCE.json
--declaration-sha256 SHA256 --original-source-dir CHECKPOINT --out FRESH_DIRECTORY`
connects actual HF safetensor/sharded checkpoints or GGUF inputs to the canonical Source28
admission. It does not require a preconstructed Source28 bundle, converted tensor files,
manifest or pipeline Borsh. Its source-backed compiler produces all of those outputs.

The pinned JSON schema is `misaka.palw.source-backed-complete-frontend.v28`:

- `original`: repository, exact 40-hex revision, advertised task, independently pinned
  original task-metadata file and SHA256, sorted source-file extent/SHA256 pins, sorted
  required components with file/stage assignments, the full qualified source-tensor
  inventory (`checkpoint-relative-path::tensor-name`), and each original context maximum.
- `stages`: one entry per actual execution stage, in pipeline order. Each has a source
  checkpoint, its complete tensor-name subset, optional source config file and JSON
  pointer, and one of the compilation paths below. Every selected source shard and config
  must be present in the original file pins. The union of stage inputs must equal the
  independently declared *and acquired* complete checkpoint tensor inventory.
- `pipeline`: declarative stage names, trip rules, maximum trips, token templates, exact
  structural input bindings and output-stage index. It is compiled and validated over
  the actual emitted V2 programs. Maximum trips must equal the original maxima, preventing
  a shortened pipeline from presenting a full-context source declaration.
- `tokenizer_file`: original pinned preprocessing content, never an inferred model name.
- `config_inert`: explicit JSON pointers outside stage-owned config subtrees. Uncovered
  config leaves and nonexistent inert pointers fail. Inside each stage, the existing
  strict config consumer rejects unread keys.
- `block_bytes`, `maximum_file_bytes`, `tensor_workspace_bytes`: explicit positive budgets.

The **declarative frontend** path supplies `frontend_pack` and `frontend_sha256`, plus
`semantics` (`version: 28`, typed external/random inputs and Rows/Final/Logits output).
The existing expression language expands its `program` fields with `version: 2`; those
fields retain original V2 Input references and post StateWrite operations. This new
entry point validates canonical V2 before resolving or streaming any weights. It shares
strict config, tensor-header and packed-descriptor import logic with the V1 frontend.
It does not interpret a V2 source as the transformed V1 court view. The off-chain
`Program::of_v2` and `StageSemanticsV28::of_program` helpers produce this representation.

The **independent canonical** path supplies `canonical_v2`, `canonical_sha256` and
`source_bindings`, with no frontend pack. The canonical program itself owns semantics;
source imports preserve the existing explicit integer/fixed-point/packed-descriptor
contracts. If both canonical and frontend paths are supplied, the generated program
must equal the independently pinned canonical program byte for byte. Neither route
uses an official model/compiler/signer list. Unsupported source formats and semantics
produce named refusals. Known HF architecture dispatch is not needed by either route.

Before converted tensor files are allocated, the adapter checks canonical V2, ranges,
source-input caps, real-PC instance counts and commitment workspace, whole-component
file/stage coverage, complete inventory, pipeline/context bindings, exact source-bundle
framing bound and aggregate converted-weight extent. A bounded writer enforces each
actual tensor's declared extent. It then computes true real-weight PC and stage R from
the emitted bytes, excluding every external/random job input, and calls the same
`FlatStageSourceWireV28::check` and `NativeStageSourceBundleV28::check` used by native
registration. All original source and compiler-input hashes are rechecked before a
successful conversion report is published.

Outputs include actual `tensors/`, `stage-N.tir2`, `pipeline.borsh`, `manifest.borsh`,
`source-bundle.borsh`, `complete.palwtir2`, a per-instance `weight-import.jsonl` with
saturation/descriptor/source-read diagnostics, and `conversion.json`. Failure retains
partial diagnostic files without the successful report. A fresh directory is required
for every attempt. The resulting artifact can enter the existing complete-pipeline
class/declaration/native payload path; the adapter grants no funding or Final authority.

The successful report always records `source_fidelity: NOT_RUN` and no native Final or
A6/A8 credit. Its immutable artifact and source pins are the inputs to the separate
fidelity/execution/court/independent-node cohort stages. Compilation does not substitute
for any of those measurements. Source28's frozen canonical V2 size/type limits remain
unchanged; window37 or a future extended grammar requires its own explicit adapter.
