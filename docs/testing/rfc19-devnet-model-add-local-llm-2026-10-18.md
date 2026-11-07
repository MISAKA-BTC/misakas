# RFC19 R-7 / R-8 devnet execution plan — 2026-10-18

Status: **PREPARING — NOT RUN**. No successful inference or model-add observation
is claimed. This plan must be frozen with all class IDs and fixture hashes before
launch. It is separate from edition-0-only fence testing.

## Scope and source

- Base: `origin/pre` = `c68d385d0ca6af176ca35da70f984a8c1e499d5e`.
- Dedicated pre checkout: `/Users/wata/.codex/worktrees/rfc19-pre/MISAKA-testnet`.
- Old binary: release build of pre plus the shared, user-authorized devnet test
  connection, edition 0 only. Initial unchanged-pre build log:
  `/tmp/misaka-codex-rfc19/old-release-build.log`.
- New binary: the identical common source with a devnet-only edition 1 adding exactly one new
  class ID and fixed artifact root at H. Edition 0 IDs and roots must not change.
- Target: owned devnet only. No t12 preset, public node, or VPS changes. At most
  three nodes on the machine; this runner reserves at most two while another
  lane has its private node running. One resident worker per model.

## Fixed intended run coordinates

| Input | Value |
| --- | --- |
| H | 8 accepted DAA |
| margin | 1 DAA |
| node A P2P / JSON wRPC | 127.0.0.1:38170 / 127.0.0.1:38173 |
| node B P2P / JSON wRPC | 127.0.0.1:38180 / 127.0.0.1:38183 |
| old gateway | http://127.0.0.1:38200 |
| new gateway | http://127.0.0.1:38201 |
| prompt, both phases/models | `Reply with one short sentence: what is two plus two?` |
| response limit | 8 generated tokens |
| old genesis expected tier, before/after H | 1000 / 1000 permille |
| new registered expected tier, before/after H | 300 / 1000 permille |

The registry reports catalog membership tier. Applied allocation must be observed
separately at its roll; H itself must not rewrite an existing allocation snapshot.
The RFC17 production-fold test covers that distinction separately.

## Actual model copy and immutable pins

Original (read only, used by another agent):
`/Users/wata/Downloads/genesis-models/work/02-huihui-qwen35-2b/out/`.
Dedicated APFS clone copies (separate files/inodes):
`/tmp/misaka-codex-rfc19/model-source/`. No source file was modified or removed.

- Index: `Huihui-Qwen3.5-2B-abliterated-PALW.palwart`.
- Container root: `136b823b66c0fb9b248dd9d733ceabf511164c91503b3fea7e2cd5f878d7c21ccefb0a3cb19eb02d543e137d720d7b53817121d2155ae04b346eed2122399a0a`.
- TIR inventory root: `a644954878ae42dda2cbcc9de879ccabb6e52db99d96f74d8dda57cedc4b4db411d8d3978fdd211f3fb0c42ac1c1f7235a3d27e3299980b2051295ecf7a18c2e`.
- Context: 32768; text generation only, `qwen35-gdn-hybrid`.
- Segment 0 SHA256: `cc5266c81ef4cb04a398b906ecf7499f3f6d78d940f30b24bd1cc14e51c83fe1`.
- Segment 1 SHA256: `f208ee43a08a52dab99670d0f0881a6bd72aca4028024de85433286aac6c8f97`.

Inspection: `/tmp/misaka-codex-rfc19/model-inspect.json`; full segment/entry
verification: `/tmp/misaka-codex-rfc19/model-verify.json` (must finish successfully).
The container root and inventory root are distinct identities; extracting a legacy
container must never silently substitute the inventory root for the registered
PALWART3 root. The index carries no layout, so the class layout/ID must also be
pinned before execution.

## Outstanding preconditions

The unchanged pre devnet leaves `palw_model_tier_v1` dormant.
`Params::validate_palw_model_tier_v1` explicitly refuses activation, even on
devnet, pending a measured release record. Both existing framed text workers
serve their legacy A16 / Qwen36 containers; there is no shipped generic TIR
PALWART3 framed worker. TIR SDK sniffing accepts only PALWTIR1 / PALWTIRS.
On 2026-10-18 the user explicitly authorized the same devnet-only fixture and
actual PALWART3 execution connection on both binaries. Common source checkout:
`/Users/wata/.codex/worktrees/rfc19-pre/MISAKA-testnet`, branch
`codex/rfc19-devnet-fixture`, common source commit `7560dd6ed` (including the pre-schema build repairs and
zero-share ordinary-registration correction). This ports the existing pricing lane's container binding (87af99d36),
retaining this pre's resource schemas, and adds a feature-required devnet entry.
Public presets and startup refusals remain unchanged. The only old/new source
difference will be `kaspad/src/private_integration/rfc19-edition.txt` (0 versus 1).

The finite control channel requests one to four accepted DAA from the real native
heartbeat miner, retaining actual PoW, mempool admission and block acceptance.
Registrations use the ordinary funded `misaka palw submit-object` command.
Both registrations request zero initial cadence share, as the SDK ordinary TIR
preflight requires without a separate certified family. This does not change
their catalog tier (300/1000); each exact signed registration is preflighted
before export.
Mining pauses between finite commands, including all prompt executions.

R-7 uses the tiny wide128 seed-37 TIR code fixture at two declared contexts (16/32),
not a trained model. R-8 uses the verified real Qwen3.5 weights for two distinct
declared classes (contexts 128/256) in separate processes. The two classes share
the same original weight inventory/container; they differ in their class-bound
layout. They are not represented as two separately trained checkpoints. The
source remains untouched. A private derived PALWTIR1 execution file selects the
existing tiled logits scheme and declares its layout; the existing native
container-binding checker permits exactly that scheme selection when the source
program declares none. The original container and inventory roots stay fixed.
The second runtime file is an APFS clone whose fixed-size layout header is
changed; both actual inventory roots are recomputed and required to match.

Gateways use fresh native sink/DAA anchor files written from the same public
node probes before each phase. Their local commitment/receipt is inspected;
no claim is submitted, and no Gate C, consensus credit, Final or reward is
inferred from local inference. Such claims require their independent native
authentication/settlement paths. Binary and fixture/plan hashes are checked
before launch.

Common compilation: `/tmp/misaka-codex-rfc19/common-check.log`; release build:
`/tmp/misaka-codex-rfc19/common-build.log`. The first release/check attempt was
stopped when shared disk fell below 25GiB, as required. Checks resume only above
that threshold; actual runtime-file creation needs 2.4GB of workspace. No other
lane's files are removed.

Old/new R-7 class IDs and roots, R-8 class IDs, binary hashes and worker hashes
remain **UNSET** until the compiled fixture exporter has run. Do not launch with placeholders.

## Required observations and failure rules

Before H: roll old nodes to the new binary, register only the one new ID, and
compare all nodes' sink/header, PALW state root, registry pairs, catalog edition
and root. Old stays 1000; new is 300. After H: compare the same fields, unchanged
ID/root pairs, edition 1 and new tier 1000; separately respect allocation rolls.

R-8 uses distinct worker/gateway processes for old and new models. At both phases
check `/health.class_id`, `/v1/models`, inspected artifact identity, nonempty
prompt answer, output tokens, and response commitment/receipt against the node's
registered `(class_id, artifact_root)`. Unstarted service, empty answer, class/root
mismatch or fallback is FAIL. Preserve complete JSON observations and process
logs under `/tmp/misaka-codex-rfc19/`; link final verdicts from this document.
