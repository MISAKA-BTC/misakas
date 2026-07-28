# The LLM side of PALW — running Qwen3.6, issuing a receipt, verifying it

PALW's audited-compute lane (algo-4) is about attesting that a *pinned* language-model computation
ran. This page is the entry point for that half of the system: where the runtime lives, how to issue
a `ComputeReceipt`, how to verify one, and — stated first, because it is the part most easily
oversold — what a receipt does and does not prove.

Nothing here is required to run a node or mine. It is the provider side.

---

## 1. What proves what — read this before anything else

A receipt is a **self-attestation that a pinned computation ran**, checkable by a second process
against pinned artifacts. It is **not**:

- a trustless proof of physical GPU execution,
- a TEE attestation,
- a zero-knowledge proof,
- evidence about the model's safety, alignment or output quality.

The runtime's own README says this in the same words, and the design makes no TEE/ZK claims. A
single self-attested receipt is exactly that: self-attested. The trust it carries comes from the
pins (a specific llama.cpp commit, a specific GGUF digest, a specific tokenizer) plus an independent
verifier re-checking every binding — not from cryptographic impossibility of lying.

**Production receipt issuance is deliberately fail-closed** in the runtime, and several protocol
requirements are still in progress. Treat what follows as a reproducible experiment, not a
production service.

---

## 2. Where the code is

| Repository | Role |
|---|---|
| **[LLM-Validation](https://github.com/MISAKA-BTC/LLM-Validation)** | The proof-of-compute runtime. Runs the pinned Qwen3.6-35B-A3B via a patched llama.cpp with a read-only graph observer, issues `ComputeReceiptV1`, and verifies receipts in a separate process. **Start here** — it carries the authoritative Quickstart. |
| **this repository** | The chain. Consumes PALW work on the algo-4 lane; carries the consensus rules, the batch/DA/certificate machinery, and the node. |

They are separate programs with separate model backends — see §7.

---

## 3. The pinned artifacts

The runtime does **not** ship or redistribute weights. It fetches them at pinned digests
(`config/runtime-pins.sh` in the runtime repo):

| Artifact | Source | Pin |
|---|---|---|
| Model (GGUF) | `registry.ollama.ai` → `huihui_ai/Qwen3.6-abliterated`, tag `35b-Claude-4.7` | sha256 `1dc494614bee8a3b…a671b`, 23,938,321,728 bytes, `Q4_K_M` |
| Base metadata | Hugging Face `huihui-ai/Huihui-Qwen3.6-35B-A3B-Claude-4.7-Opus-abliterated` | rev `ac18882735d037f6074a7630eb68d85db8234c25` |
| Inference runtime | `ggml-org/llama.cpp` | commit `12127defda4f41b7679cb2477a4b0d65ee6a0c8f` + `palw-observer` patch (sha256 `d155a88b…9e02c`) |

The model is **abliterated** — its refusal behaviour was reduced upstream. The runtime attests
*that* a pinned computation ran; it makes no claim about what the model will say. The upstream model
is unmodified: this is a wrapper, not a fine-tune, and not a new model.

---

## 4. Issuing a receipt

Full procedure, prerequisites and hardware notes are in the runtime repo. The shape of it:

```sh
# clone the runtime repo first — `runtime-palw/` is NOT in this repository — then fetch the
# pinned llama.cpp and the 24 GB GGUF, both digest-checked:
./scripts/install.sh && ./scripts/verify-install.sh
```

```sh
# build the two binaries (--locked is mandatory for reproducibility)
cargo build --release --locked --manifest-path runtime-palw/Cargo.toml \
  --bin palw-metal-receipt --bin palw-verify-bundle
```

**Do not pass `+1.81.0`.** Earlier revisions of this page did; the crate declares
`rust-version = "1.85"`, and its manifest says in as many words that the old 1.81 declaration was
unsatisfiable for any `--features ml-dsa` build "and therefore dishonest". Cargo refuses outright,
so the command simply fails. Use the default toolchain, or install 1.85.0.

These two binaries are auto-discovered from `src/bin/`, not declared in `[[bin]]` — and they are two
of **seven**. The other five (`palw-worker`, `palw-integer-receipt`, `palw-verify-integer`,
`palw-lifecycle`, `palw-verify-search`) cover the chain-facing path; `palw-lifecycle export
--node-context` is what produces the artifact `misaka palw da enqueue` consumes.

```sh
# create a 32-byte audit key ONCE, outside the output directory, 0600, never overwritten
AUDIT_KEY="$HOME/.config/misaka-palw/audit-keys/local-audit.key"
install -d -m 700 "$(dirname "$AUDIT_KEY")"
test ! -e "$AUDIT_KEY"
(umask 077 && openssl rand 32 > "$AUDIT_KEY")

runtime-palw/target/release/palw-metal-receipt \
  --prompt-stdin --audit-key-file "$AUDIT_KEY" \
  --output-dir "receipts/manual-$(date +%Y%m%d-%H%M%S)" \
  --n-predict 2 < /path/to/prompt.txt
```

`--prompt-stdin`, `--audit-key-file` and `--output-dir` are all required, and there is deliberately
no option that takes the prompt on argv. A zero, all-zero, or wrong-permission audit key is rejected
before inference, not after.

A successful run writes five files sharing the receipt ID as basename:

| File | Mode | Contents |
|---|---|---|
| `<id>.palw` | 0644 | signed canonical public envelope |
| `<id>.json` | 0644 | strict `misaka.palw.public-receipt.v2` metadata (unknown fields rejected) |
| `<id>.palw.bundle` | 0600 | private verification material, XChaCha20-Poly1305 under the audit key |
| `palw-state.sqlite3` | 0600 | schema-v4 durable state |
| `<id>.complete` | 0644 | `misaka.palw.receipt-set.v2` completion marker |

**The marker is not authentication.** It binds the receipt ID, bundle ID and the public JSON's
SHA-256 so a crash cannot leave a half-published set — but it is neither a keyed MAC nor a network
signature, and anyone who can write the directory can recompute it. Never treat a marker alone as
evidence of authenticity, authorization or maturity.

Job IDs, nonces, salts and signing keys are generated per run from the OS CSPRNG, so **your receipt
ID will not match any published example**. Those identities bind the set together; they are not
registered network credentials.

---

## 5. Verifying a receipt

Verification runs as a **separate process** against the artifacts on disk:

```sh
ID=... # the .palw basename
runtime-palw/target/release/palw-verify-bundle \
  --receipt "$OUT/$ID.palw" \
  --bundle  "$OUT/$ID.palw.bundle" \
  --public-json "$OUT/$ID.json" \
  --audit-key-file "$AUDIT_KEY" \
  --state-db "$OUT/palw-state.sqlite3"
```

It checks exact filenames, directory ownership/mode/link-count/inode, bounded path races, the v2
marker, every public JSON field, the bundle's authenticated encryption, the signatures, the
assignment, the manifests, the observer evidence and openings, and the existing DB acceptance.

Success prints `status=local_restored` and `trust_scope=embedded_local_snapshot`.

**Read that trust scope literally.** Local mode proves continuity with the registry snapshot that
was encrypted at issuance. It does **not** independently fetch a network authority, and it cannot
see a later revocation. A production verifier must supply the current network ID, the
scheduler/signer registry and the approved manifest hashes through the library's external-trust API,
from a separate channel.

---

## 6. A reference receipt

`receipts/final-v7/8e2dd34b…` in the runtime repo, issued and verified on Apple Silicon/Metal:

| Field | Value |
|---|---|
| canonical compute units | `41692` |
| CU ruleset | `43a5feef177b389f…70ce` (semantic v3) |
| evidence level | `gemm_traced` |
| GEMM trace root | `78f6f15a768bcfa9…d84f7` |
| decode tokens | `2` |
| model bytes | `23938321728` |
| llama.cpp commit | `12127defda4f41b7679cb2477a4b0d65ee6a0c8f` |

The CU figure is committed under a **graph-independent semantic ruleset**, and each GEMM is bound to
a real Metal kernel dispatch. That binding is honestly labelled: it records the kernel and its launch
geometry, which is not the same as a CUDA-style accumulator proof.

---

## 7. Two different model backends — do not conflate them

| | `LLM-Validation` | `misaka-palw` crate (this repo) |
|---|---|---|
| Inference | patched **llama.cpp** + read-only graph observer | **candle**, GGUF |
| Purpose | issue and verify `ComputeReceiptV1` | the k=2 replica dispatch / conformance gates behind the frozen `VerifiableInferenceBackend` |
| Build | its own binaries | optional, off by default: `--features qwen-backend` / `qwen-metal` / `qwen-cuda` |
| In a default node build | absent | absent |

A default `kaspad` build compiles neither. See
[testnet-participation.md](testnet-participation.md#2-supported-platforms-and-hardware) for the
node-side feature flags and what they do not accelerate.

---

## 8. How this reaches the chain — current state

Honestly: **it does not yet** — though a code path does exist as far as the node's local spool
(`palw-lifecycle export --node-context` → `misaka palw da enqueue`). What does not exist is
*acceptance*, and the reason is not the one this page used to give.

PALW has three independent levers: **land** (the code ships), **accept** (`palw_algo4_accept`), and
**weight** (`palw_compute_work_scale > 0`). Only the first is released. `palw_algo4_accept` ships
`false` on **all six presets**, and while it is false an algo-4 header is rejected in
`check_pow_algo_id` — before GHOSTDAG, before reachability, before any header-stage store write.
Its release condition is defined once as gate-class semantics in ADR-0040 §7.1.1, not flipped per
network.

On the networks operated today,

- `testnet-10` has PALW inert (`palw_activation_daa_score = u64::MAX`) — the lane does not exist
  there at all, and there is no provider role on it;
- `testnet-200` has PALW genesis-active and its peer allowlist is open, but the acceptance lever is
  closed and `palw_compute_work_scale = 0`. Measured output is ~2.6 BPS of algo-3 against the 2 + 8
  design — the hash lane on target, the PALW lane contributing nothing.

The DNS-finality floor is a genuine *second* gate — `testnet-200` inherits `PRODUCTION_DNS_PARAMS`
via `..MAINNET_PARAMS`, so it wants `min_active_validators = 3` bonded at 20,000,000 MSK each and
one is bonded today. But it is not the binding one, and an earlier version of this page was wrong to
call it "the whole reason": **clearing the validator floor would not open the lane.** Note also that
the 20,000,000 MSK figure is the *validator* bond; the PALW **provider** bond floor is
`min_provider_bond_sompi = 10 MSK` with a 6-epoch exit delay — a different object entirely.

You do not have to take the validator count on faith — it is a chain-derived fact you can read off
any node yourself:

```sh
misaka mtp validators --rpc 127.0.0.1:27220 --network testnet-200
# 1 bond(s) on testnet-200 at daa 202879, 0 slashed
```

(`27220` is one deployment's port, not a code default — the default is `127.0.0.1:27210`, and some
nodes bind borsh elsewhere again. Pass the port your node actually uses.) The command pages the bond
registry to exhaustion with the point of view pinned, and reports
`stored_status` and `effective_status` separately because they routinely disagree — on `testnet-10`
every one of 28 bonds is stored `pending` while being effectively `active` (27) or `unbonding` (1),
so a single collapsed status field would be actively misleading.

What `validators` does **not** report is per-epoch attestation: no RPC says which validator signed
which epoch. That has to be indexed out of blocks, and `misaka mtp attestations` now does it —
walking the stake-attestation-shard subnetwork and decoding each payload:

```sh
misaka mtp attestations --rpc 127.0.0.1:27220 --network testnet-200 --max-blocks 20000
# 0 row(s) = 0 distinct (validator, epoch) from 0 validator(s) in 0 shard tx(s) over 20080 block(s)
```

Zero, across every block that node retains in the scanned range — the halt is visible from two
independent directions, the bond floor and the absence of attestation traffic. Run the same command
against `testnet-10` and it returns thousands of rows, which is what a lane that is actually turning
looks like.

Two cautions the command prints for itself, because both are easy to get wrong. **Absence is not
proof**: a pruned node holds nothing below its pruning point, so an empty range means "none retained
here", not "nobody attested". And **rows are not participations**: a DAG puts one shard transaction
in several blocks, so dedup on `(validator_id, att_epoch)` before you compute a rate — on
`testnet-10` today, 2,562 rows collapse to 1,194 distinct pairs.

So a receipt issued today is a reproducible local artifact. Nothing on-chain has accepted one, and
no provider has been paid. When that changes it will be because the validator set cleared, and this
section should be the first thing updated.

---

## 9. Where to go next

| Topic | Document |
|---|---|
| Runtime quickstart, hardware, full runbook | [LLM-Validation](https://github.com/MISAKA-BTC/LLM-Validation) |
| Joining a network, node/miner/validator | [testnet-participation.md](testnet-participation.md) |
| The audited-compute lane's consensus rules | [adr/0039 onward](adr/) |
| Why the lane is currently closed | [adr/0042-permissionless-snapshot-auth-completion.md](adr/0042-permissionless-snapshot-auth-completion.md), [adr/0048-header-v4-staging-mainnet.md](adr/0048-header-v4-staging-mainnet.md) |
