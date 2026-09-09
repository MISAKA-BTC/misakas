# The components manifest (`misaka/components/v1`)

**A manifest row is a pointer with a digest, and never a place a component is trusted from
without the digest matching.** Every component a person needs — the node, the CLI, the family
workers, the gateway, the rail, the class artifacts, and on the Studio's side the runtime, the
shell and the engines it stages — is one row of one JSON file, `components.json`, that both
repositories publish with their releases and both check (ADR-0096 Decision 10). The row says
where the bytes are and what they hash to; an installer that finds the file somewhere else
(beside the executable, in `engines/`, on `PATH`, in the models directory) verifies it against
the same digest before it runs it, and a file that does not match is not that component, whatever
its name. The defect this closes is §1.3 of the ADR: four hand-kept tables in the Studio said
what to install, one of them named a binary this tree deleted on 2026-09-02, and nothing in
either repository's CI knew.

Written by `scripts/misaka-components-manifest.py` (the node half), validated by the same script
in `--validate` and verified on disk by `--check`. The script is standard-library Python 3, so it
runs unchanged on the three release runners and on an operator's machine.

## The file

```text
{
  "schema":      "misaka/components/v1",
  "release":     "<tag>",                 the release this manifest belongs to
  "network":     "testnet-11",            the network the release is cut for
  "node_manifest": { "url": …, "sha256": … },   Studio manifests only: the node manifest it was built against
  "components":  [ <row>, … ]             sorted by id
}
```

Canonical form: keys sorted, two-space indent, one trailing newline, rows sorted by `id`. A
manifest is named by other manifests by its sha256, so a reordered copy with the same meaning is
a different file; the validator refuses an unsorted one for that reason.

## The row

| field | type | required | meaning |
|---|---|---|---|
| `id` | `[a-z0-9][a-z0-9.-]*`, unique in the manifest | yes | the component's name as an installer knows it; for a binary its file name without `.exe`, for an artifact its file's stem |
| `kind` | `node` \| `cli` \| `worker` \| `gateway` \| `rail` \| `engine` \| `artifact` \| `tokenizer-table` \| `runtime` \| `shell` | yes | what the row is; a consumer dispatches on it and refuses a kind it does not know |
| `version` | string | yes | the release tag for a binary row; for an artifact row, the registration that pinned its root (e.g. `relaunch-5f`) — the root is the identity, `version` is for people |
| `platform` | Rust target triple, or `any` | yes | `any` is REQUIRED for `artifact` and `tokenizer-table` rows and REFUSED for every other kind: a binary for no platform is a row nobody can install |
| `url` | `https://…` or `hf://<repo>/<path>` | yes | where the bytes are. `hf://` resolves to `https://huggingface.co/<repo>/resolve/main/<path>` |
| `sha256` | 64 lowercase hex | yes | of the component's OWN bytes — the file that will be executed or mapped |
| `size` | integer | yes | of the same bytes; the cheap check before the expensive one |
| `requires` | list of ids | yes (may be empty) | ids that must be installed for this row to be useful: an artifact requires its worker. Every id must be a row of this manifest, or of the node manifest a Studio manifest names |
| `member` | path inside the archive | when `url` is an archive | the node release publishes one zip per platform, not loose binaries, so a binary row points at the zip and names its member; `sha256`/`size` stay the member's own bytes |
| `archive_sha256`, `archive_size` | hex64, integer | with `member` | the zip's digest, so a downloader can verify the transport before extracting and verify the member after |
| `class_id` | 128 lowercase hex | `artifact` | the class the artifact pairs with (`palw-class inspect` prints it; the Studio's `PalwClassSpec.class_id_hex` is the same value) |
| `artifact_root` | 128 lowercase hex | `artifact` | the INVENTORY root the chain pins (`getPalwProducerFacts.artifactRoot`; `qwen36-run --root-only` for the hybrid tier) — not the file's sha256, which is a different digest of the same bytes |
| `tokenizer_commitment` | 128 lowercase hex | `tokenizer-table` | what the artifact header's `tokenizer_commitment` names (`Base0ArtifactV1::tokenizer_commitment_of`); a table row is how a v6 class serves it beside the artifact (ADR-0096 Decision 8) |
| `model_id`, `convert_command`, `notes` | string | no | the chain's model id string; the command that reproduces the artifact from the public weights; anything else |

Unknown keys are refused, top level and row. A new key is a schema change — a second table in
this document, or `misaka/components/v2` — not a row that quietly carries more than the checker
reads.

## Producers

* **This repository's deploy workflow** (`.github/workflows/deploy.yaml`), one manifest per
  platform job, uploaded beside the zip as `components-<triple>.json`. The step runs the script
  over the binaries the job produced that are components at all — today `kaspad` and `misaka`;
  the family workers (`palw-a16-fp-worker`, `palw-qwen36-fp-worker`), the gateway and the rail
  become rows the day the workflow builds them, and the script refuses every other name the
  release builds (`rothschild`, `kaspa-wallet`, the PQ tools, the bridge) rather than guess a
  kind. Rows point at the platform zip with `member`, because that is what the release publishes.
  The platform is read from the compiler (`rustc -vV`'s host) on the two jobs that pass no
  `--target`, and is the explicit musl triple on Linux. Artifact rows are merged from a file
  (`--artifacts`) when one is given; the workflow gives none yet, so the artifact rows of a
  network live in the Studio's offline table until this tree checks such a file in.
* **The Studio's release workflow** writes its own manifest — `misaka-studiod` (`runtime`), the
  shell (`shell`), the engines it stages (`engine`) — and NAMES the node manifest it was built
  against by `node_manifest: { url, sha256 }`. Two halves, one schema, and the reference is the
  seam: a Studio release is pinned to exactly one node release's rows.

## Consumers

* The Studio's `GET /api/v1/components`: per row, the installed version, the manifest version,
  whether the sha256 was verified, and where the file was found (`beside the executable`,
  `engines/`, `PATH`, `models_dir`); install and update go through the existing download
  manager, which already verifies sha256 and resumes. `misaka-studiod --check` prints the same
  table. `PalwClassSpec.artifact` becomes the OFFLINE copy of the artifact rows, pinned to the
  same sha256 by a test.
* The cross-repository check: the Studio's CI fetches the node manifest its own manifest names
  (offline, the pinned copy) and asserts that every `engine`, `worker`, `gateway` and `node` id
  the Studio can spawn is a row in it (ADR-0096 invariant 10). A binary this tree stops building
  fails the Studio's build, not a person's evening.
* Anyone, by hand: `scripts/misaka-components-manifest.py --check components-<triple>.json
  <dir>` over an extracted release verifies every row's file against its digest and exits
  non-zero naming the first mismatch; rows of platform `any` whose file is not in the directory
  are listed as skipped, never counted as verified, and a run that verified nothing fails.

## Example — `aarch64-apple-darwin`, release `testnet-main-e65ccf20`

The full shape. Angle-bracketed values are what the writer fills from the bytes it hashes; the
two artifact rows carry their real values, copied on 2026-09-10 from the Studio's class table
(`crates/misaka-studio-core/src/palw.rs`, `TESTNET11_CLASSES`, MISAKA-Studio `7096533`), which
is the table this manifest replaces as the source of truth. The rows the deploy workflow writes
TODAY are `kaspad` and `misaka`; the four other binary rows appear when the workflow builds them,
and until then an artifacts file whose `requires` names a worker is refused by the validator —
which is the cross-repository check in miniature.

```json
{
  "components": [
    {
      "archive_sha256": "<sha256 of rusty-kaspa-testnet-main-e65ccf20-osx.zip>",
      "archive_size": "<bytes>",
      "id": "kaspad",
      "kind": "node",
      "member": "bin/kaspad",
      "platform": "aarch64-apple-darwin",
      "requires": [],
      "sha256": "<sha256 of bin/kaspad>",
      "size": "<bytes>",
      "url": "https://github.com/MISAKA-BTC/misakas/releases/download/testnet-main-e65ccf20/rusty-kaspa-testnet-main-e65ccf20-osx.zip",
      "version": "testnet-main-e65ccf20"
    },
    {
      "archive_sha256": "<sha256 of rusty-kaspa-testnet-main-e65ccf20-osx.zip>",
      "archive_size": "<bytes>",
      "id": "misaka",
      "kind": "cli",
      "member": "bin/misaka",
      "platform": "aarch64-apple-darwin",
      "requires": [],
      "sha256": "<sha256 of bin/misaka>",
      "size": "<bytes>",
      "url": "https://github.com/MISAKA-BTC/misakas/releases/download/testnet-main-e65ccf20/rusty-kaspa-testnet-main-e65ccf20-osx.zip",
      "version": "testnet-main-e65ccf20"
    },
    {
      "archive_sha256": "<sha256 of rusty-kaspa-testnet-main-e65ccf20-osx.zip>",
      "archive_size": "<bytes>",
      "id": "misaka-palw-fp-rail",
      "kind": "rail",
      "member": "bin/misaka-palw-fp-rail",
      "platform": "aarch64-apple-darwin",
      "requires": [],
      "sha256": "<sha256 of bin/misaka-palw-fp-rail>",
      "size": "<bytes>",
      "url": "https://github.com/MISAKA-BTC/misakas/releases/download/testnet-main-e65ccf20/rusty-kaspa-testnet-main-e65ccf20-osx.zip",
      "version": "testnet-main-e65ccf20"
    },
    {
      "archive_sha256": "<sha256 of rusty-kaspa-testnet-main-e65ccf20-osx.zip>",
      "archive_size": "<bytes>",
      "id": "misaka-palw-gateway",
      "kind": "gateway",
      "member": "bin/misaka-palw-gateway",
      "platform": "aarch64-apple-darwin",
      "requires": [],
      "sha256": "<sha256 of bin/misaka-palw-gateway>",
      "size": "<bytes>",
      "url": "https://github.com/MISAKA-BTC/misakas/releases/download/testnet-main-e65ccf20/rusty-kaspa-testnet-main-e65ccf20-osx.zip",
      "version": "testnet-main-e65ccf20"
    },
    {
      "archive_sha256": "<sha256 of rusty-kaspa-testnet-main-e65ccf20-osx.zip>",
      "archive_size": "<bytes>",
      "id": "palw-a16-fp-worker",
      "kind": "worker",
      "member": "bin/palw-a16-fp-worker",
      "platform": "aarch64-apple-darwin",
      "requires": [],
      "sha256": "<sha256 of bin/palw-a16-fp-worker>",
      "size": "<bytes>",
      "url": "https://github.com/MISAKA-BTC/misakas/releases/download/testnet-main-e65ccf20/rusty-kaspa-testnet-main-e65ccf20-osx.zip",
      "version": "testnet-main-e65ccf20"
    },
    {
      "archive_sha256": "<sha256 of rusty-kaspa-testnet-main-e65ccf20-osx.zip>",
      "archive_size": "<bytes>",
      "id": "palw-qwen36-fp-worker",
      "kind": "worker",
      "member": "bin/palw-qwen36-fp-worker",
      "platform": "aarch64-apple-darwin",
      "requires": [],
      "sha256": "<sha256 of bin/palw-qwen36-fp-worker>",
      "size": "<bytes>",
      "url": "https://github.com/MISAKA-BTC/misakas/releases/download/testnet-main-e65ccf20/rusty-kaspa-testnet-main-e65ccf20-osx.zip",
      "version": "testnet-main-e65ccf20"
    },
    {
      "artifact_root": "1a7457f100d9fb0f3406d882b4b5bcd7e2ebcccd54edc5268a08c3a85bc6c8d3adacdf345cde3cb72ffe8ed7fe7a2f729d10f00821f94b1e8562e4e217b72708",
      "class_id": "4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7",
      "convert_command": "qwen25-convert /path/to/Qwen2.5-1.5B-Instruct --a16 --out qwen25-1.5b-a16.palwart",
      "id": "qwen25-1.5b-a16",
      "kind": "artifact",
      "model_id": "Qwen/Qwen2.5-1.5B/graph-v5@512",
      "platform": "any",
      "requires": ["palw-a16-fp-worker"],
      "sha256": "a8c4e53e5b30dd0d4dc6ef791e0513890a07a2b3a22d045e612536bba1240b1f",
      "size": 1795427276,
      "url": "hf://Misakachain/Qwen2.5-1.5B-PALW-A16-runtime/palw-runtime/qwen25-1.5b-a16.palwart",
      "version": "relaunch-5f"
    },
    {
      "artifact_root": "f4aad4fd543928eb2d3a737555b09da9bf685fc515c0f8d4520988efcffacf0813d1b727537f0d03d349253aa11ef427e4047c2166b69fd7edb46a4a9984b368",
      "class_id": "5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e",
      "convert_command": "qwen36-convert --url <gguf url> --header header.bin --out qwen36.palwq36 --context 512",
      "id": "qwen36",
      "kind": "artifact",
      "model_id": "Qwen3.6-35B-A3B/graph-v3",
      "platform": "any",
      "requires": ["palw-qwen36-fp-worker"],
      "sha256": "7a944595a4256ab0aa4ca8b59f39fea268654b3630e54fb354cf1fa7658cf08c",
      "size": 36492831232,
      "url": "hf://Misakachain/Qwen3.6-35B-A3B-PALW-runtime/qwen36.palwq36",
      "version": "relaunch-5f"
    }
  ],
  "network": "testnet-11",
  "release": "testnet-main-e65ccf20",
  "schema": "misaka/components/v1"
}
```

The floor class (`PALW-BASE-0`) has no row: its artifact is derived from a seed on every node, so
there is nothing to point at and nothing to download. A tokenizer-table row has the binary rows'
shape with `platform: "any"` and `tokenizer_commitment` in place of `class_id`/`artifact_root`;
none is listed because no shipped class serves one yet (ADR-0096 Decision 8 is behind a fence
that is `None` on every preset), and a row with an invented commitment would be exactly the kind
of pointer this file exists to forbid.

A Studio manifest differs only at the top:

```json
{
  "components": [
    { "id": "misaka-studio", "kind": "shell", "platform": "aarch64-apple-darwin", "version": "v0.2.0",
      "url": "https://github.com/…/releases/download/v0.2.0/MISAKA-Studio_0.2.0_aarch64.dmg",
      "sha256": "<…>", "size": "<…>", "requires": ["misaka-studiod"] },
    { "id": "misaka-studiod", "kind": "runtime", "platform": "aarch64-apple-darwin", "version": "v0.2.0",
      "url": "https://github.com/…/releases/download/v0.2.0/misaka-studiod-aarch64-apple-darwin",
      "sha256": "<…>", "size": "<…>", "requires": ["kaspad", "misaka"] }
  ],
  "network": "testnet-11",
  "node_manifest": {
    "url": "https://github.com/MISAKA-BTC/misakas/releases/download/testnet-main-e65ccf20/components-aarch64-apple-darwin.json",
    "sha256": "<sha256 of that file as published>"
  },
  "release": "v0.2.0",
  "schema": "misaka/components/v1"
}
```

`requires: ["kaspad", "misaka"]` resolves through `node_manifest`; without that reference the
validator refuses the row, because a requirement nobody can look up is a requirement nobody can
install.

## What this file does not do

* It does not sign anything. The digest binds a row to bytes; what binds the manifest to a
  release is that the release published it, over TLS, from the repository's own release page.
  A signed manifest is a later schema, not a quiet field in this one.
* It does not say a component is compatible with a chain. `network` names the release line;
  the fingerprint a node prints at start (`Consensus params fingerprint: …`) is the only thing
  that says which chain a binary follows, and `docs/testnet11-join-mining.md` is where that is
  read.
* It does not carry the artifact bytes or say who may download them. `hf://` rows point at the
  public repositories the Studio's table already named, and the license lives with the weights.

Related: [ADR-0096](adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
(Decision 10, invariants 10–11), [docs/model-requests.md](model-requests.md) (how a new
artifact row comes to exist), `scripts/misaka-components-manifest.py --self-test`.
