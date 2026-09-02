# Derived artifacts (ADR-0078): what was made from it is committed; the thing never rides

This page is the operator's and the consumer's half of ADR-0078. The chain's half is the ADR.

## The shape, in one screen

```text
POST /v1/chat/completions  { messages, "derive": "scene" }          ← the person asks for a kind
   │
   ▼  one inference (ADR-0077 R0): the answer, the commitment, the claim id
   │
   ├─ grammar `scene/v1` canonicalizes the answer      → canonical DSL   (dsl_hash)
   ├─ transformer `scene/glb/v1` builds the artifact    → GLB bytes       (artifact_hash, artifact_bytes)
   └─ DerivedArtifactV1 { claim_id, output_root, grammar_id, transformer_id, kind, dsl_hash,
                          artifact_hash, artifact_bytes, executor_pubkey } + signature
   │
   ▼  the response carries all three: the DSL as text, the artifact inline (≤ 4 MiB) or by
      `GET /v1/artifacts/<derived-id>`, and the object (borsh, hex) with its signature if the
      gateway holds the key
   │
   ▼  `misaka palw submit-object --object <stem>.derived-object.borsh --yes`
      → one lifecycle transaction (0x4b), a few hundred bytes, beside the claim
```

The chain accepts the object only if the claim exists on this chain, the object's `output_root`
is the claim's, the signer is the claim's executor bond key, `(claim, transformer)` is new, and
the claim holds fewer than `PALW_DERIVED_MAX_PER_CLAIM` (4) derivations. It credits no weight, no
payment, no exposure. It never accepts the DSL or the artifact, chunked or whole.

## Kinds this build ships (Decision 8) and their ids (Decision 9)

`palw-derive list` prints the registered grammars and transformers with their ids. The kind
table's ids are fixed in `kaspa_consensus_core::palw_derived_v1::kind`; the chain interprets none
of them beyond `kind != 0`.

| kind | id | grammar | transformer | artifact | max DSL | max artifact | max work |
|---|---|---|---|---|---|---|---|
| scene | 1 | `scene/v1` | `scene/glb/v1` | `.glb` (positions, vertex colours, no normals) | 256 KiB | 2 MiB | 65,536 mesh vertices |
| image | 2 | `image/v1` | `image/png/v1` | `.png` (RGBA8, filter 0, stored deflate) | 4 MiB | 32 MiB | 4,194,304 raster-pixel |
| cad | 3 | `cad/v1` | `cad/stl/v1` | `.stl` (binary; STEP is a later writer, booleans are refused by name) | 64 KiB | 1 MiB | 4,000,000 exact-predicate |
| code | 4 | `code/v1` | `code/evm/v1` | `.mcod` build + test log (the in-tree EVM is the first named toolchain) | 4 MiB | 16 MiB | 7,710,000,000 evm-gas |
| map | 5 | `map/v1` | `map/mmap/v1` | `.mmap` | 4 MiB | 16 MiB | 23,330,816 cell-visit |
| music | 6 | `music/v1` | `music/smf/v1` | `.mid` (SMF format 1, no running status) | 4 MiB | 16 MiB | 65,536 midi-note |
| simulation | 7 | `simulation/v1` | `simulation/trace/v1` | `.msim` step-hash chain + summary + final state | 4 MiB | 64 MiB | 100,000 simulation-step |
| contract | 22 | `code/v1` | `contract/evm/v1` | `.mcod` | 4 MiB | 16 MiB | 7,710,000,000 evm-gas |

The three ceilings are ADR-0078 SA-2's, and they are manifest fields: they are in `transformer_id`'s
preimage, so loosening one is a NEW transformer and the derivations made under the old one stay
checkable against the old id. `palw-derive list` prints them; `palw-derive manifest --transformer
<name|id>` prints the whole manifest with its exact preimage.

### `code` and `contract`: where the initcode actually runs (ADR-0078 SA-1, ADR-0079 Decision 12)

Both rows execute a program a model wrote, so neither runs in the process that asked for it.

* **The in-tree EVM runs in a separate confined process.** `code/evm/v1` and `contract/evm/v1`
  frame the job — the deploy data, each test's calldata, value and gas limit, and the *digest* of
  the run manifest — and spawn `palw-evm-runner` (shipped beside `palw-derive`, or named by
  `MISAKA_PALW_EVM_RUNNER`) through the host's confinement backend, in an ephemeral tree destroyed
  after the run, `env_clear`ed, under a resident ceiling of 1 GiB and a deadline derived from the
  gas the answer itself declares. The runner reads one `MEVJ` frame on stdin, writes one `MEVR`
  frame on stdout, and holds no prompt, no claim, no key and no test name: it returns facts
  (success, output, gas), and the transformer turns them into the verdict log. **There is no
  in-process fallback** — a missing runner is a refusal, which is what "the row does not ship
  without it" means. A killed, over-ceiling or denied run is `no object` (Decision 2's
  parse-failure arm), never a different artifact: the backend cannot change a number, only refuse
  one (ADR-0079 S4).
* **The gas ceiling and the state fixture are part of `transformer_id`.** The fixture is an empty
  world plus one funded deployer at nonce 0 under a zero block environment; its digest and every
  gas ceiling are hashed into the *run manifest*, whose digest rides in the transformer manifest's
  writer name (`misaka-code-build/2/canonical-v1+evm-run/<ceiling>/<digest>`) and in the `MCOD`
  header. The runner refuses any job that names a run manifest other than the one it was built
  with, so a stale runner beside a new library cannot execute under someone else's ceiling.
* **External toolchains** for `code` (rustc → wasm32, solc, clang → wasm32) are manifests run by
  the hermetic runner in `misaka-palw-derive::kinds::code`; none is registered — an external
  toolchain is named only when its two-architecture drill passes on the fleet (Decision 11). Its
  runner now enforces ADR-0079 Decision 12: it refuses on a host whose confinement backend is
  `none` (no backend, no socket denial, no run), and refuses when a bond or wallet key is
  reachable in the process's environment or in the directories the caller names — *the build's
  output is never executed on a host that holds a key*.

Running the crate's tests therefore builds two binaries: `cargo test -p misaka-palw-derive`.
`cargo test -p misaka-palw-derive --lib` does not build binaries, and every EVM test then fails
naming the absent runner — that is the gate holding, not a broken test.

## The bounds, and what refuses what (SA-2, SA-3, SA-5)

```text
answer bytes ─▶ max_dsl_bytes ─▶ grammar ─▶ max_steps ─▶ transformer ─▶ max_artifact_bytes ─▶ object
               (the KIND, on the        (declared_work,     (the layer, on the bytes that
                byte count, before       before the run)     came back, before any object
                its own parser)                              names them)
```

* **`max_dsl_bytes` is the kind's wall**, checked on the byte count before its own parser — a JSON
  parser is an allocator driven by its input. It is not enforced a second time in `derive_with`,
  and that is deliberate: each kind pins the WORDS of its refusal in its corpus golden, and a wall
  in the layer would run first and replace them all with one sentence. What the layer does instead
  is refuse to let a transformer ship without that wall: `palw-derive drill` and
  `derive::tests::every_transformer_refuses_an_answer_over_its_declared_dsl_ceiling` feed every
  registered transformer an answer one byte over its declared ceiling and require a refusal that
  names the ceiling.
* **`max_steps` and `max_artifact_bytes` are checked by the layer too**, as backstops behind each
  kind's own prediction: the step ceiling before the run for a transformer that implements
  `Transformer::declared_work`, the artifact ceiling on the bytes that came back. They fire only
  when a kind's own accounting was wrong, which is when a backstop is worth having.
* **A zero ceiling is refused by name** (`check_declared_bounds`): "declares none" must not read as
  "no limit".
* **SA-3, the inputs a transformation names by hash** (Decision 10): `NamedInput` holds one upload
  for the job's life and wipes it on drop; `check_offered_named_input` answers "may I read this
  many bytes" before anything is buffered; `check_named_inputs` requires every declared hash to
  resolve to bytes that hash to it, and refuses anything held that the DSL did not name. Every
  shipped kind declares `max_inputs 0`, so every upload is refused today — a transformation kind
  states its two numbers in `bounds::named_input_limits`.
* **SA-5**: `derive_with` refuses to derive at all when no manifest is published in this tree at
  the `transformer_id` the manifest hashes to. A consumer who cannot fetch the manifest cannot
  verify, and an unverifiable statement should not be storable.

## The gateway (Decision 6)

Request fields, beside the OpenAI ones:

| field | meaning |
|---|---|
| `"derive": "<kind or transformer>"` | derive after the inference; absent = the answer is the product |
| `"serve_dsl": true` | elect this claim's DSL into the data-availability obligation (default off) |

Gateway flags: `--derive-seed <file>` (the bond key's 32-byte seed — the gateway then signs the
object itself; without it the object is left unsigned for the rail), `--artifact-inline-max <bytes>`
(default 4 MiB).

Response: `misaka.derivation` — `status` (`derived` or `refused` with the grammar's reason; a
refusal changes nothing about the claim, X4), the ids and hashes, `dsl`, `artifact`
(`inline_base64` or `url`), `object_borsh_hex`, `signature_hex`. Beside it, for X6:
`misaka.output_token_ids`, `misaka.job_context_hash`, `misaka.family`, `misaka.fp_claim_id`,
`misaka.executor_pubkey`.

Outbox files per job: `<stem>.dsl`, `<stem>.artifact.<ext>`, `<stem>.derived-unsigned.borsh`,
`<stem>.derived-object.borsh` (when signed), `<stem>.derived.json`, `artifacts/<derived-id>.<ext>`,
and with `serve_dsl` the `FPD1` payload `<stem>.dsl-payload.fpd1`.

## The rail and the CLI

```bash
# sign a derivation the gateway left unsigned (same bond key as the claim, its own context)
misaka-palw-fp-rail --derive-artifact <outbox>/fp-job-XXXX --bond-key-seed <seed>
# or, for the signer sidecar: the digest under SigningPurpose::PalwDerivedArtifactV1
misaka-palw-fp-rail --derive-artifact <outbox>/fp-job-XXXX --print-derived-message

# carry it (one lifecycle transaction; dry-run without --yes)
misaka palw submit-object --key-file <funding seed> --object <outbox>/fp-job-XXXX.derived-object.borsh --yes

# the DSL under the DA election, staged beside the claim's material so the node serves it on request
misaka palw fp-submit --tx <rail tx> --material-out <node>/palw-retention --capture <material.bin> \
    --dsl-payload <outbox>/fp-job-XXXX.dsl-payload.fpd1 --yes
```

## Verification (Decision 5, X6) — anyone holding the answer

```bash
palw-derive verify --object <derived-object.borsh> --answer <the DSL or the raw answer> \
    [--artifact <file>] \
    [--output-token-ids <ids.json> --job-context-hash <hex> --family qwen25-a16|qwen36]
```

The tool re-runs the grammar and the transformer and compares `dsl_hash`, `artifact_hash` and
`artifact_bytes`; with the ids, the job's context hash and the family it recomputes the claim's
`output_root`. A mismatch exits 2 — a publicly demonstrable false object. Nothing on chain
convicts the executor for it (Decision 5 says so plainly); what it costs is the executor's name
on a provenance anyone can show is wrong.

## The two-architecture drill (X3)

```bash
palw-derive drill                                           # goldens + bounds, on this host
palw-derive drill --report arm64.json                       # on an Apple host
palw-derive drill --report x86.json                         # on an Intel/AMD host, same build
palw-derive drill --check x86.json                          # on either: byte-identical or exit 3
```

The corpus lives in `misaka-palw-derive/corpus/<kind>/`, with each kind's `golden.json` pinned by
its tests. The drill checks three things and says which failed:

| check | what it compares | exit |
|---|---|---|
| the goldens | every corpus sample, derived or refused, against its kind's `golden.json` (a sample named `-refused-` must refuse, and the pinned message says which wall it hit) | 4 |
| the bounds | an answer one byte over each transformer's declared `max_dsl_bytes`, generated rather than stored, which must be refused with a message naming the ceiling | 5 |
| `--check` | `rows`, `refused` (message for message) and the source-tree hash of two reports | 3 |

The goldens are the half that makes a second architecture checkable without shipping a file
between hosts: run the drill there and the pins either hold or they do not. Locally on Apple
silicon the second architecture is Rosetta:
`cargo test -p misaka-palw-derive --target x86_64-apple-darwin`.
