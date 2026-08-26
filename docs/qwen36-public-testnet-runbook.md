# Qwen3.6-35B-A3B on the public testnet — what an operator does

**What this is.** `testnet-11` (PALW ConsensusV2) now registers **two** execution classes at
genesis: `PALW-BASE-0/rc`, the derived liveness floor every node can run from a seed, and
**`PALW-QWEN36`**, a 35B hybrid model converted from
`Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf`. This is the operator-side path for the second
one: how to obtain the artifact, how to prove it is the right one, and what a node does with it.

Every number below is measured on the reference host (M4 Pro, 24 GiB), not estimated.

---

## 0. What you do and do not need it for

| | floor (`PALW-BASE-0/rc`) | `PALW-QWEN36` |
|---|---|---|
| validate the chain | yes, no files | **yes, no files** |
| produce blocks for it | yes, no files | needs the artifact |
| serve a court seat for it | yes, no files | needs the artifact |

**A node without the artifact is a full node.** It validates every block, including blocks produced
for the Qwen3.6 class, because validation checks commitments and signatures rather than re-running
inference. What it cannot do is *produce* for that class or answer a dispute about it. That is the
whole cost of not having 34 GiB free.

The class set is part of the ruleset id, so it is a property of the BINARY, not of what you have on
disk: a node with the weights and a node without them fingerprint identically and peer normally.

---

## 1. Get the artifact

Two routes end at the same 64-byte root. Take either.

### 1a. Convert it yourself (trust nothing)

```bash
cargo build --release -p misaka-palw-base0 --bin qwen36-convert
./target/release/qwen36-convert \
  --url https://huggingface.co/Misakachain/Qwen3.6-35B-A3B-PALW-runtime/resolve/main/Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf \
  --header /path/to/first-48MiB-of-that-file.bin \
  --out qwen36.palwq36 --context 512
```

`--url` streams each tensor by HTTP range, so the 22 GiB GGUF never lands on disk; pass `--gguf
FILE` instead if you already have it. `--header` is the file's first 48 MiB (the GGUF metadata and
tensor directory), which the converter needs before it can ask for ranges:

```bash
curl -r 0-50331647 -o header.bin <the same URL>
```

**Cost:** peak memory is one layer's tensors (a few GiB), peak disk is the output. The run is
single-pass — every tensor is read once, quantized, and the f32 reference is run through it for
calibration in the same pass.

### 1b. Download the converted artifact

Pull `qwen36.palwq36` from the same HuggingFace repository. **Verify it before use** (§2); an
unverified artifact is a file, not a class.

---

## 2. Prove it is the right artifact

```bash
./target/release/qwen36-run --artifact qwen36.palwq36 --root-only
```

It must print exactly:

```
c970d69327bf65d6b2502a8e53a021739f2579c2274754790869320352a92c7a4a8deb5da08e27e90f09d5ae9b4f7e44c983304af7bba8127d28e2d85996b236
```

That is `PALW_RC_GENESIS_QWEN36_ARTIFACT_ROOT` — the value testnet-11's genesis registers. The root
covers the shape, every parameter table, the rotary table and every weight byte, each under a
length-prefixed tagged name.

**The node checks this itself** at startup and refuses a mismatch, so this step is for your own
diagnosis, not a substitute for it. A file with the wrong root is not this class however it is
named.

**Cost:** one pass over the mapping — **104 s** warm on the reference host (33.27 GiB), about a
minute more cold. It runs once at startup, not per block.

**Reproducibility.** The conversion is deterministic: the same GGUF produces the same bytes. Nothing
on the path samples, times or hashes iteration order — the weights quantize per row, the f32
reference is single-threaded, and the fast kernels parallelize over OUTPUT channels only (each
output is a pure function of its index, so the thread count changes who computes it and never what).
`the_fast_kernel_is_bit_identical_to_the_reference` pins that property, and two full conversions of
the same input hash identically.

---

## 3. Run a node that can produce for it

```bash
kaspad --netsuffix=11 --palw-class-artifact=/path/to/qwen36.palwq36
```

Repeatable — pass the floor's artifact too if you carry one. The flag is also
`KASPAD_PALW_CLASS_ARTIFACT`. On startup the node maps the file, computes its root, and matches it
against the class the chain registered; a mismatch is refused **by root**, with the flag named in
the message.

To produce for it rather than for the floor:

```bash
kaspad --netsuffix=11 --palw-class-artifact=/path/to/qwen36.palwq36 \
       --palw-producer-class=<the 128-hex class id> --palw-producer-bond=<outpoint> ...
```

`--palw-dump-classes` prints the classes the chain registers and their ids.

**Cost per block:** the canonical job is **7 prefill + 2 decode** tokens — ten forward passes of a
35B hybrid, **8.0 s** on the reference host, producing 8.9 MB of committed material. Memory is the
mapping (33 GiB, paged) plus working space; the reference host runs it in 24 GiB of RAM because the
artifact is memory-mapped and the MoE touches 9 of 256 experts per token.

---

## 4. What the chain will and will not let this class do

* **`n_ctx` is 8.** A claim may declare a job of at most 8 cached positions. That bound is not the
  runtime's — the artifact's rotary table covers 512 and the engine serves it — it is what the
  court can afford to adjudicate: the class's worst close is 73,636 bytes against the 81,920 a
  lifecycle transaction can carry, and the recurrence's replay evidence is what the context
  multiplies. A larger context returns when the recurrence's replay is checkpoint-anchored.
* **The canonical job is (7, 2)** and `pwu_per_inference` is its counted step-leaf count. A
  registration declaring anything else is refused.
* **The class carries 1‰ of the cadence** on entry — the minimum grantable share, donated from the
  floor by the transition's largest-remainder rule. It is not the floor's replacement; it is a
  second class beside it.
* **Disputes are real.** Every kernel this class reaches is in the court's catalog, its step space
  is projected from the engine's own order, and a decode-token close rides the tiled logits scheme
  (two tile openings, not 248,320 lanes). The class was admitted BECAUSE those numbers fit, and the
  boot gate re-checks them against the ruleset's ceilings on every start.

---

## 5. Joining an already-running chain

Everything above describes the genesis form: testnet-11 registers the class in its own genesis, so
joining is a matter of running the binary. A class arriving on a chain that is **already running**
takes the post-genesis path instead — `--palw-register-class`, which needs an active bond, its key
and a funded fee outpoint, and which carries the profile and canonical job in the transaction for
`verify_class_admission_v2` to check. That path exists and is tested; it is not what this network
uses for this class.

**The fingerprint moved.** Registering a second class changes the ruleset id, so this build and any
earlier testnet-11 build are different networks and refuse each other at the handshake. Joining
needs the re-mint that the day's consensus changes already forced — see the launch record.
