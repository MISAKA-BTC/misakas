# ADR-0079: A pure function needs no permissions — the sandbox is for the host, and the chain never takes its word for it

**Status:** PROPOSED (2026-09-02). Written against a proposal that ADR-0078 is not enough to run a
local LLM in practice, and that ten provenance layers plus five security ADRs are missing beneath
it. The reading is right about the shape and wrong about the inventory: **nine of the ten
provenance layers already exist in this tree under other names** (§7 says where, field by field),
one is refused because it contradicts a decision this lineage has already paid for, and the five
security ADRs are **one** ADR — this one — because they are one rule.

**Builds on:** ADR-0053 (one execution family: pure Rust in the tree, integer arithmetic — a float
path is a path two honest hosts disagree on), ADR-0026 (borrow the architecture, refuse the
tolerant proof model), ADR-0067 (classes are chain data, kernels are the build; Decision 5's
interpreter fence; Decision 6's four storage tiers and "the registration carries kilobytes and no
URL"), ADR-0069 (E2E adjudicability is the price of weight), ADR-0072 Decision 8 (every field
inside the priced bytes is pinned, or it is the challenge), ADR-0075 (certification is a consensus
object), ADR-0077 (the gateway, the worker, the seat's interval openings), ADR-0078 (the
transformer discipline, the kind table, the four modes), and `SECURITY.md`'s existing operator
posture (loopback by default; a public bind is an explicit, acknowledged act).

**Refuses, by name, and §7 gives each its reason:** a `security_policy_hash` anywhere inside the
priced bytes (ADR-0072 Decision 8), a multi-engine runtime registry (ADR-0053), a tolerance-based
receipt verifier (ADR-0026), a job scheduler between the user and the worker (ADR-0074), and a
chain-side model distribution network (ADR-0067 Decision 6).

---

> **Security amendment appended (2026-09-02)** — see the last section (SA-1…SA-8), corrections found reading the ADR against the tree: the memory ceiling must not be `RLIMIT_AS` (the hybrid maps 33 GiB); the signer trusts the supervisor's channel, not the gateway's bytes; the DA opening server authenticates; `PATH` leaves the allowlist; Decision 7 and ADR-0077 Decision 6 are one rule; nothing logs a prompt.

## 1. The finding: the provenance half is built, the host half is not

The proposal's chain — model → runtime → input → execution → receipt → output root → artifact — is
this lineage's chain. It is worth writing down exactly where each link already lives, because the
cost of not writing it down is a second spelling of a field that already exists, and this
repository has paid that bill before (`derived-sets-need-one-spelling`).

| the proposal's layer | what already carries it | where |
|---|---|---|
| ① Model Registry | the class **is** the registry row: `class_id == shape_profile_id() == H(profile)`, and the profile is the architecture, the geometry, the quantization map and the context length. Admission is permissionless and on-chain. | ADR-0056, ADR-0067 Decisions 1–2, ADR-0075 |
| ② Model Artifact Hash | `artifact_root` in the registration; the worker recomputes SHA-256 **from the bytes** on every load; the registered-class path admits a holding only when its *computed* digest equals the registered root | `misaka-palw-worker/src/main.rs::pinned_model_path_v2`, `misaka-palw-sdk/src/sdk.rs::resolve_chain_registered` step 4 |
| ③ Runtime / Engine Registry | `PalwRuntimeManifestV2` — worker self-hash, patchset root, `libm_arithmetic_digest`, golden-vector root — plus ADR-0075 Decision 5's kernel-coverage binding. There is **one** engine by decision, so this registry names a build, not a choice of engine | `misaka-palw-worker/src/main.rs::runtime_manifest_v2`, ADR-0053, ADR-0067 |
| ④ Input canonicalization | the canonical chat template (frozen, its identity part of the class profile), the canonical job context, the prompt commitment, and segment-wise control tokens | ADR-0044 Decision 10, ADR-0077 Decision 6 |
| ⑤ Inference Receipt | `execution_commitment_v3` over the whole attempt, the claim, the receipt block | ADR-0072, ADR-0073, ADR-0074 |
| ⑥ Output root | `output_commitment_v2` over exactly the emitted ids | ADR-0044, ADR-0078 Decision 2 |
| ⑦ Deterministic verification | checkpoint-anchored replay, the seat's `k` sampled intervals compared **exactly**, and the court's bisection to one leaf | ADR-0069, ADR-0077 Decision 8 |
| ⑧ Resource / cost metering | leaves and pwu — an integer count of executed work, never a clock and never a watt | ADR-0045, ADR-0071, ADR-0078 Decision 10 |
| ⑨ Capability / hardware profile | the four storage tiers, and "registration and possession are different acts, and nothing may couple them" | ADR-0067 Decision 6 |
| ⑩ Artifact store / gateway | the gateway returns the answer and the artifact to the user; the chain holds the derivation; the DA obligation covers the trace, not the thing | ADR-0077 Decisions 3–4, ADR-0078 Decisions 1 and 6 |

So the provenance half needs no new ADR. What it needs — and what §6 schedules — is that the
mapping above be *readable*, so that the next reader does not propose it again.

**The half that is genuinely missing is the host's.** Every mechanism in that table protects the
*chain* from a lying executor. Not one of them protects the *operator's machine* from the three
things a practical local-LLM node now touches:

* **a prompt from a stranger.** ADR-0077's gateway is a public entrance that parses attacker-chosen
  text and hands it to a model. Its default bind is loopback (`127.0.0.1:8790`), but nothing in the
  binary refuses a public bind, and nothing rate-limits one.
* **an artifact from a stranger.** ADR-0056 admission is permissionless; ADR-0067 Decision 5's
  interpreter arm exists so a node can serve a class whose graph no binary in the fleet has a row
  for. Its fence is written as a correctness fence. It is also a security fence and was never
  labelled one.
* **a toolchain over model-written source.** ADR-0078 Decision 11 names pinned external toolchains
  (rustc → wasm32, solc, clang) for the `code` and `contract` kinds, and the `code` row's artifact
  includes **a test log** — which is to say, running a program a model wrote. That is the largest
  new privilege in the lineage and it arrives without a confinement rule.

And one gap that is not speculative: the supervisor spawns the worker with
`Command::new(&state.cfg.worker)` and **no `env_clear()`** (`misaka-palw-agent/src/agent.rs:374`).
Whatever is in the operator's environment when the node starts — `SSH_AUTH_SOCK`, a cloud token, a
wallet path, an exchange API key — is in the model process's environment too. There is also no
memory ceiling on that process, on a fleet that has already recorded an 8.4 GB single-minute burst
in a producer (`t11-producer-memory-growth`).

## 2. The line: determinism already writes the permission list; the OS should enforce it

A PALW execution is a pure function of its committed inputs. That is not a security posture — it is
the precondition for the court being able to try it at all (ADR-0069). Read as a capability list,
it says:

```text
   what the arithmetic forbids            why                        who enforces it today
   ─────────────────────────────────────────────────────────────────────────────────────────
   a clock                          two hosts disagree               the court (wrong root → bond)
   a network read                   two hosts disagree               the court
   randomness                       two hosts disagree               the court
   reading the environment          two hosts disagree               the court
   writing anywhere but the outbox  nothing needs it                 nobody
   ─────────────────────────────────────────────────────────────────────────────────────────
   ⇒ the deny-by-default list is not authored by a policy. It is READ OFF the determinism
     rules already in force — and the OS is not currently asked to enforce any of it.
```

This gives the ADR its shape, and its two refusals:

* **The sandbox is not a consensus mechanism.** A host that deviates is convicted by the court, by
  arithmetic, with its bond. Confinement adds nothing to that and must never be asked to. The
  reason to confine is the *other* direction: a bug in a GGUF parser, a hostile profile, or a
  compiler running model-written source should not reach the operator's keys, their shell agent,
  or the internet.
* **A confinement claim on the chain is a vote, not a verdict.** The chain cannot observe whether a
  host ran sandboxed; it can only record what the host says. This lineage's standing sentence
  applies unchanged: *a court that cannot compute the verdict is a vote, and votes are what this
  lineage refuses* (ADR-0078 §8). So the security posture is enforced locally, reported honestly,
  and committed nowhere.

```text
       the stranger's bytes                       the operator's machine
   ┌──────────────────────────┐            ┌──────────────────────────────────┐
   │ prompt (public HTTP)     │────────────▶│ gateway   no keys, bounded, loopback-by-default
   │ class profile (chain)    │            │    │                             │
   │ model artifact (operator)│            │    ▼  one framed pipe            │
   │ DSL → toolchain (0078)   │            │ supervisor  no keys, one slot, rlimits
   └──────────────────────────┘            │    │                             │
                                           │    ▼  per-job process            │
        the court's arithmetic ────────────▶│ worker    no env, no net, no fs but two dirs
        (what a job MAY do)                 │    │                             │
                                           │    ▼  unsigned commitment → outbox
                                           │ signer sidecar   the ONLY key holder,
                                           │                  one message shape
                                           └──────────────────────────────────┘
```

Everything in that picture except the two dashed rules is already the shipped shape. This ADR makes
it a refusal instead of a habit, and closes the four places where the habit is not yet code.

## 3. Decisions

### Phase A — the doctrine (consensus-inert by construction)

**Decision 1 — the capability set is the arithmetic's, and it is deny-by-default because the
arithmetic already is.** The confinement profile for any process that executes a class, a
transformer, or a toolchain is derived from the determinism rules, not authored beside them: no
network, no clock beyond a monotonic deadline the supervisor owns, no randomness source, no
environment beyond a named allowlist, no filesystem beyond a read-only artifact path and a
write-only outbox path. A capability that the arithmetic does not need is not granted "for now";
its absence is a property the court already relies on. Adding a capability to the execution path is
therefore a change to the execution family, with the family's own gates — not a configuration knob.

**Decision 2 — no security field enters the priced bytes, in any lane, ever.** ADR-0072 Decision 8
settled the general form: *a field the producer chooses freely and no rule pins is a nonce by
another name*, and the review reproduced it — sweeping one free field over ONE execution gave 4,096
distinct tickets and 4,096 distinct Layer-0 tags. A `security_policy_hash` chosen by the executor is
exactly that field, with an extra defect of its own: it is unfalsifiable, because a host can run
wide open and commit whichever hash it likes. So `execution_commitment_v3`, the attempt envelope,
`PalwFreePromptCommitmentV3`, the claim, the certification objects and `DerivedArtifactV1` gain no
security field, no policy id, no attestation, and no confinement flag. The exhaustive
field-classification test from ADR-0072 Decision 8 is the enforcement: every field of the struct
must be chain-equality, execution replay, derived, or the one position field, and a security field
is none of those — so it does not compile.

**Decision 3 — the security posture is off the consensus path, and a test proves it cannot fork the
fleet.** Two nodes with different confinement backends — one Linux with a full sandbox, one macOS
with a partial one, one with confinement disabled entirely — compute **identical roots** or the job
**fails**. A denied syscall is a `JobFailed` with the denial named, never a different number. This
is the invariant that lets the sandbox ship at all: a security control that can change an
arithmetic result is a fork risk, and this lineage has already lost a fleet to a gate that measured
the wrong side (`silent-health-gates-measure-the-wrong-side`).

### Phase B — the processes (privilege separation as a refusal)

**Decision 4 — no process that parses a stranger's bytes holds a key, and no process that holds a
key parses a stranger's bytes.** The shipped shape already obeys this and states it in three
different doc comments; this decision makes it one rule with one enforcement point:

| process | parses | holds | enforcement |
|---|---|---|---|
| `misaka-palw-gateway` | public HTTP text | the executor **public** key only — "the ML-DSA signature belongs to the signer sidecar" | refuses to boot if a signing secret is reachable in its own view |
| `misaka-palw-agent` (a process supervisor, **not** an LLM agent) | one framed Borsh request on a `0600` unix socket | nothing — "It holds no validator keys" | asserted at boot |
| `misaka-palw-worker` | a job frame and a pinned artifact | nothing | Decision 5's confinement |
| the DA opening server (ADR-0077 Decision 8) | opening requests from any seat | the capture, read-only | shape-checked before anything is read (`check_opening_request_shape`) |
| the signer sidecar | one message shape | the ML-DSA-87 secret | Decision 8 |

The counterexample is on record: the `.113` public node ran the public entrance and the seat in one
process, and when it wedged, both went (`t11-5d-public-node-hang-analysis`). Role separation was
already the operational conclusion. This makes it a startup refusal rather than a runbook line.

**Decision 5 — the worker starts with nothing and is confined by the platform's own mechanism.**
Two parts, both cheap, one portable and one not:

* **Portable, and required everywhere:** the supervisor spawns the worker with `env_clear()` plus an
  explicit allowlist — `MISAKA_PALW_GGUF`, `MISAKA_PALW_GOLDEN`, the class-artifact paths, `PATH`,
  and the locale pins the determinism rules already require — and with an explicit working
  directory that is neither the operator's home nor the node's datadir. The allowlist is a constant
  in the tree, not a config file, so adding to it is a reviewed act. This closes
  `agent.rs:374` and costs one line plus a test.
* **Per platform, and named honestly when absent:** Linux gets `seccomp` (no `socket`, no `connect`,
  no `execve` after setup) and `Landlock` (read-only on the artifact and golden paths, write-only on
  the outbox, nothing else); macOS gets a `sandbox-exec` profile with the same shape; any platform
  without a backend runs with the environment discipline alone and **prints which backend is in
  force at boot**, in the same line that prints the class and the manifest. A node whose backend is
  `none` may still mine — the court does not care — but Decision 10 refuses to let it be a public
  entrance.

**Decision 6 — every job has a memory ceiling and a wall-clock deadline, and exceeding either is a
failed job, never a dead node.** The supervisor already kills on timeout; it gains `RLIMIT_AS` (and
a cgroup where one is available) at `PALW_WORKER_MAX_RSS_BYTES`, defaulting to a value the operator
sets from their own hardware and the class's declared footprint. The reason is liveness, not
confidentiality: an OOM killer that reaps the node because a model process spiked is an availability
attack that costs the attacker one prompt, and this fleet has already measured an 8.4 GB burst
inside one minute. A refused job is a `JobFailed`; the node keeps its seat, its peers and its tip.

### Phase C — the inputs (what a stranger may cause)

**Decision 7 — untrusted text cannot become a control token, and that is the *whole* of "prompt
injection" the protocol may promise.** The boundary is lexical and already pinned: the canonical
template runs the tokenizer with `parse_special = false`, so a user's text cannot smuggle the
model's own control tokens, and a ChatML-style profile with segment-wise special tokenization is a
future class profile rather than an edit of this one. This ADR states the other half, because a
promise left unstated gets read as a promise made: **the protocol does not and cannot guarantee
that a model ignores instructions inside its own context.** Obedience is not a consensus property,
and no filter makes it one. The defence is structural instead, and it is Decision 8's.

**Decision 8 — a model's output is data on every path, and there is no path on which it becomes a
command.** Today this is true because the model has no tools: a worker returns token ids. The rule
is written now, before ADR-0078's `agent` row makes it interesting:

* An answer, a DSL, or a task graph is **bytes**. Nothing in the tree executes, fetches, or shells
  out on the strength of model output.
* ADR-0078 Decision 10's planning mode produces a canonical task graph **as an artifact**. Executing
  one is not a derivation and is not this lineage's business; if an application executes it, each
  step is an ordinary program under the operator's own authority, and each inference inside it is
  its own claim (ADR-0077 R0).
* The signer signs **one message shape** — a claim id or a lifecycle object id it re-derives itself
  from the object it was handed. A signer that will sign arbitrary bytes is a key the gateway holds
  by proxy, and this is the sentence that forbids building one.

**Decision 9 — model and runtime integrity is a full read, permanently, and the cheaper check is
named as a defect class.** `pinned_model_path_v2` recomputes SHA-256 over the whole artifact for
every job process, and the audit finding it replaced is the reason: a `(path, size, mtime)`-keyed
cache in the process working directory let anyone who could write that file pass any same-sized
model — on the consensus PoW path, where the consequence is a node that silently forks itself. This
ADR promotes that from a fixed bug to a standing rule: **no artifact identity may ever be derived
from metadata.** Size, mtime, filename, a sidecar `.json`, or a previous run's answer are all the
same defect. The full read *is* the check, and the registered-class path's step 4 — a *computed*
digest equal to the registered root — is the same rule for classes this build has no row for.

**Decision 10 — the public entrance is bounded, acknowledged, and never the seat.** Extending
`SECURITY.md`'s existing pattern rather than inventing a second one:

* Default bind stays loopback. A non-loopback `--listen` **fails at startup** unless
  `MISAKA_PALW_ALLOW_PUBLIC_GATEWAY=1` is set, and the failure message names the intended pattern —
  an authenticating reverse proxy in front of a loopback-bound gateway.
* No wildcard CORS, and no secret-shaped field in any response DTO, both already the house rule.
* Bounds, all of them already-existing knobs made mandatory rather than default: request body,
  prompt tokens, `max_decode_cap`, one job slot, and a per-source request rate. Exceeding a bound is
  a 4xx, not a queue.
* A gateway bound publicly on a host whose confinement backend is `none` (Decision 5) refuses to
  start. That is the one place where the missing backend is fatal, and it is fatal because it is the
  only place where a stranger chooses the input.

**Decision 11 — a stranger's *graph* stays behind ADR-0067 Decision 5's fence, and that fence is
hereby also a security fence.** Its arming conditions are already written (a profile-space fuzzer to
saturation with zero panics, bit-identity against the compiled engines, one full lattice on a
devnet). This ADR adds two, for the reason ADR-0067 gives itself — *"bounded" is not "verified"*:
the fuzzer's corpus must include profiles built to exhaust memory and to recurse, and the
interpreted path must run under Decision 6's ceiling with the ceiling proven to bind. Arming remains
an operator act, and the arming line says what it is: *you are about to interpret declarations
written by strangers*.

**Decision 12 — an external toolchain is the largest privilege in the lineage and gets the
narrowest cage.** For ADR-0078's `code` and `contract` kinds:

* The in-tree EVM (`evm/v1`) is the first named toolchain because it needs none of this — it is
  in-tree, integer, and already adjudicated.
* Any external toolchain (rustc, solc, clang) runs under Decision 5's confinement **plus** an
  ephemeral tree destroyed after the run, no network (already ADR-0078 Decision 11's manifest
  requirement, now enforced rather than declared), `SOURCE_DATE_EPOCH`, the manifest's environment
  whitelist and nothing else.
* **The build's output is never executed on a host that holds a bond key or a wallet key.** A `code`
  row's test log is the execution of a program a model wrote; it runs on a disposable host or in the
  same confinement with no writable state that outlives it, or the row's transformer does not ship.
  This is a completion condition for ADR-0078's Q-05, not advice.

### Phase D — what the operator can see

**Decision 13 — the posture is a local report, printed by the node, signed by nobody.**
`misaka node security-report` prints, from live state and not from config: the confinement backend
actually in force per process, the environment allowlist as the child actually received it, every
listening socket with its bind address and whether the acknowledgement variable was required, which
processes hold key material, the artifact roots verified at load with their computed digests, and
the interpreter fence's state. It reports `none` honestly where a backend is missing. It is a
sibling of `misaka node liveness` (0 / 11 WEDGED / 12 STALLED), reuses its exit-code discipline, and
is what an operator pastes into an issue. It is **not** a chain object, it earns nothing, and
Decision 2 is why.

## 4. What this costs, stated before it is measured

* **Chain bytes, state, consensus surface: zero.** By Decision 2 this ADR adds no field, no object
  and no transition arm. It is not a ruleset move and does not touch an identity.
* **Latency.** `env_clear` and an allowlist: nothing measurable. `seccomp` + `Landlock` install: one
  syscall batch at worker start, microseconds against a multi-second inference. The `RLIMIT_AS`
  call: nothing. The full-read model gate is unchanged — it already costs a 1.2 GB read per job
  process, and the persistent-agent path already amortizes it.
* **Operator friction, and it is deliberate.** Three things that work today stop working: a public
  `--listen` without the acknowledgement variable, a public gateway on a host with no confinement
  backend, and a worker that was reading something out of the ambient environment. Each fails at
  startup with the fix in the message.
* **Platform coverage.** Linux gets the full backend. macOS gets a partial one. Windows gets the
  environment discipline and an honest `none`, which is consistent with the tree's existing Windows
  posture and is the reason Decision 10 gates the *public entrance* rather than mining.
* **What this does not buy.** Nothing here makes a dishonest executor honest, and nothing here is
  visible to a peer. A node that lies about its posture is exactly as convictable as before —
  through its roots — and exactly as unconvictable for its posture, which is the honest state of a
  property no court can compute.

## 5. Invariants the tests must hold

```
S1  No field of any priced, committed or certified struct carries a security policy, posture,
    attestation or confinement flag. The exhaustive field-classification test (ADR-0072 D8)
    refuses to compile one.
S2  A worker child process receives exactly the allowlisted environment: the spawn is env_clear'd
    and the test asserts the delivered set equals the constant, not merely contains it.
S3  On a platform with a backend, a worker's attempt to open a socket, read outside the artifact
    and golden paths, or write outside the outbox is denied by the OS; the drill exercises each
    and asserts the denial, not the absence of a crash.
S4  Roots are identical with the confinement enabled and disabled, on the same inputs; a denied
    operation yields JobFailed with the denial named and never a different number.  (Decision 3)
S5  No process that parses network or public input holds key material: the gateway refuses to boot
    if a signing secret is reachable in its view; the supervisor asserts it holds none.
S6  A non-loopback gateway bind fails at startup without MISAKA_PALW_ALLOW_PUBLIC_GATEWAY=1, and
    fails unconditionally when the confinement backend is `none`.
S7  Untrusted prompt text never tokenizes to a control token (parse_special = false), pinned by
    the existing template test, with a corpus that includes every special-token literal.
S8  Artifact identity is never derived from metadata: the model gate recomputes the digest from
    bytes on every load, and a tree-level guard test fails if any size/mtime/path-keyed digest
    cache is reintroduced anywhere.
S9  A job exceeding PALW_WORKER_MAX_RSS_BYTES or its deadline is a JobFailed; the node keeps its
    tip, its peers and its seat, and the test asserts the node's liveness after the kill.
S10 Nothing in the tree executes, fetches, or shells out on the strength of model output; the
    signer signs only a re-derived id and rejects arbitrary bytes.
S11 An external toolchain runs with no network, an ephemeral tree and the manifest's whitelist;
    its outputs are never executed on a host holding a bond or wallet key.
S12 `misaka node security-report` reports the backend actually in force — a test that disables the
    backend asserts the report says `none` rather than the configured value.
```

## 6. Order of work

| unit | content | done when |
|---|---|---|
| R-01 | `env_clear` + the allowlist constant + the working-directory pin in the supervisor's spawn | S2 green; the boot line prints the delivered set |
| R-02 | `RLIMIT_AS` / cgroup ceiling and the `JobFailed` path | S9 green, including a node-liveness assertion after a deliberate overshoot |
| R-03 | the gateway's bind guard, bounds, and the key-reachability boot refusal | S5, S6 green |
| R-04 | Linux `seccomp` + `Landlock` backend, macOS `sandbox-exec` backend, `none` reported honestly | S3, S4, S12 green on x86_64 Linux and arm64 macOS |
| R-05 | the metadata-cache guard test and the special-token corpus | S7, S8 green |
| R-06 | `misaka node security-report` | prints from live state; a disabled backend is reported as `none` |
| R-07 | the signer's one-shape rule, written down and tested | S10 green |
| R-08 | ADR-0078 Q-05's confinement gate for external toolchains | S11 green; the `code` row does not ship without it |
| R-09 | §7's mapping table folded into `docs/` as the answer to "where is the model registry" | a reader finds the field, not a proposal to add it |

**Done when** a node can be a public LLM entrance on a host that also holds a bond, and the operator
can state — from a report the node prints, not from a promise — that the model process has no
network, no environment, no filesystem beyond two directories, no key material, and a ceiling; while
the chain holds not one byte about any of it.

## 7. Disposition of the proposal, item by item

The proposal is answered in full, including the parts that are refused, because a refusal without a
reason gets re-proposed.

| # | proposed | disposition |
|---|---|---|
| 1 | Model Registry (`ModelDescriptorV1`) | **already exists** — the class profile is the descriptor and `class_id = H(profile)`. A second descriptor would be a second spelling of a derived set. |
| 2 | Model Artifact Hash | **already exists** — `artifact_root`, full-read verified. Decision 9 makes the "cheaper check" refusal permanent. |
| 3 | Runtime/Engine Registry over llama.cpp / vLLM / MLX / TensorRT | **refused as proposed; the aligned form exists.** ADR-0053 decided one execution family: pure Rust, in the tree, integer — because a float path is a path two honest hosts disagree on, and a llama.cpp worker cannot produce a commitment at all (it has no step legs). The registry that exists names a *build* (`PalwRuntimeManifestV2` + kernel coverage), not a choice among engines. Other engines are non-consensus conveniences and can never sign a commitment. |
| 4 | Prompt/Input canonicalization | **already exists** — the frozen template, the canonical job, `parse_special = false`. Decision 7 states its limit. |
| 5 | Inference Receipt | **already exists** — `execution_commitment_v3` and the claim. |
| 6 | Output Root | **already exists** — `output_commitment_v2`; ADR-0078 is the layer above it. |
| 7A | Deterministic verification, exact | **this is the model in force** — bit-exact replay, exact row comparison, bisection to one leaf. |
| 7B | Receipt-attestation verification with tolerance ("more realistic") | **refused.** ADR-0026 took this decision against a measured competitor's RBO/p95 tolerance: a tolerant verifier cannot convict, and a check that cannot convict is a vote. The seat's sampled interval is a *licence*, not a verdict, and it compares exactly. |
| 8 | Resource metering in GPU-seconds, watts, wall clock | **refused in that unit; the aligned form exists.** Work is metered in leaves and pwu — an integer count of executed operations — because a clock is not adjudicable and ADR-0078 Decision 10 already forbids one in a cost model. Token counts are carried; seconds and watts are operator telemetry, never chain facts. |
| 9 | Capability / hardware profile | **already exists** — ADR-0067's storage tiers, and possession as an operator act. |
| 10 | Artifact store / gateway distributing models | **refused for models; exists for outputs.** ADR-0067 Decision 6: the registration carries kilobytes and no URL, precisely so a fleet's disk does not grow with strangers' decisions. Model bytes are obtained by the operator out of band and admitted only by digest. The output side is ADR-0077's gateway and ADR-0078 Decision 6. |
| — | a Scheduler assigning jobs to workers | **refused.** Jobs are self-originated and the attempt is a claim the chain draws (ADR-0074); there is no orderer, and inserting one would create a party that chooses who earns. The user chooses an executor; the chain chooses the ticket. |
| S1 | Sandbox / capability control | **adopted** — Decisions 1, 5, 6. |
| S2 | Secrets isolation | **adopted** — Decisions 4, 5, 8; the shipped shape (no keys in the supervisor, the signature in a sidecar) becomes a refusal. |
| S3 | Tool / agent permission system | **adopted in the only form that is honest today** — Decision 8. The model has no tools; the rule that its output is never a command is written before ADR-0078's `agent` row makes it tempting. A policy engine is deferred (§8) until something exists to police. |
| S4 | Network egress control | **adopted** — Decisions 5 and 10, extending `SECURITY.md`'s existing loopback/acknowledgement pattern rather than adding a second one. |
| S5 | Integrity & Security Receipt (`SecurityPolicyHash` committed) | **integrity half already exists (Decision 9); the security half is refused as a commitment and adopted as a local report (Decisions 2, 13).** A posture the chain cannot verify, committed into bytes that price a lottery, is both an unfalsifiable claim and — by ADR-0072 Decision 8, reproduced with 4,096 tickets from one execution — a grinding surface. |

## 8. What is deliberately not decided

* **TEE / remote attestation** (SGX, SEV-SNP, Apple's attestation). It would convert "the host says
  it was confined" into "a vendor says it was confined", which is a different trust root and not a
  verdict the court computes. It is also the standing answer for anyone who wants the posture to
  bear weight, so it deserves its own ADR and its own argument rather than a paragraph here.
* **A policy-engine DSL between the model and its tools.** Decision 8 says the model has no tools;
  a policy language with nothing to police is a surface, not a control. It enters with the first
  thing that actually executes on model output, and not before.
* **Multi-tenant gateway isolation** (one gateway, many paying users, per-user quotas and
  confidentiality between them). ADR-0077 Decision 16's `PanelDa` privacy mode is the prompt half;
  tenancy is an operator product decision and this ADR takes none of it.
* **Key management.** ADR-0015's remote signer / HSM protocol already owns this; Decision 8 only
  says what the signer may sign.
* **Windows confinement.** No backend is named; `none` is reported honestly and Decision 10 gates
  the one role where that matters.
* **Supply chain of the build itself** (reproducible builds of `kaspad` and the worker, signed
  releases). Real, adjacent, and separable: the worker's self-hash is in the runtime manifest today,
  which is what the *class* needs; what a *downloader* needs is a release-signing ADR.

## 9. Names introduced

| name | kind | meaning |
|---|---|---|
| `MISAKA_PALW_ALLOW_PUBLIC_GATEWAY` | env, operator | acknowledges a non-loopback gateway bind; absent ⇒ startup refusal (Decision 10) |
| `PALW_WORKER_MAX_RSS_BYTES` | constant + operator override | per-job address-space ceiling; exceeding it is `JobFailed` (Decision 6) |
| `PALW_WORKER_ENV_ALLOWLIST` | constant, in-tree | the exact environment a worker child receives (Decision 5) |
| `misaka node security-report` | command | the local posture report (Decision 13) |
| confinement backend `linux-seccomp-landlock` / `macos-sandbox-exec` / `none` | reported string | what is actually in force, never what was configured (Decisions 5, 13; S12) |

No consensus constant, no state field, no object, no version bump. That is the point of Decision 2.

## Security amendment (2026-09-02) — corrections found reading the ADR against the tree

**SA-1 — The memory ceiling measures the right thing.** Decision 6 names `RLIMIT_AS`. The hybrid
maps a 33 GiB artifact (Relaunch 5e: 5m40s to map), so an address-space limit at any "RSS-shaped"
value kills the worker at startup. On Linux the ceiling is cgroup v2 `memory.max` (or
`RLIMIT_DATA` plus a supervisor RSS poll where cgroups are unavailable); on macOS a supervisor RSS
watchdog with kill; the constant is named for what it measures (`PALW_WORKER_MAX_RESIDENT_BYTES`).
Mapped file pages are not the process's to be charged for twice.

**SA-2 — The signer trusts the supervisor's channel, not the gateway's bytes.** Decision 8 has the
signer re-derive the id; it must also refuse a commitment whose roots differ from the worker result
frame the supervisor attaches to the request. Otherwise a compromised gateway obtains signatures on
fabricated commitments, and an honest court then slashes the operator's bond for them. Residual,
stated: a compromised worker can still fabricate — the court doing its job — and the loss is bounded
by the exposure ceiling (ADR-0077 SA-1).

**SA-3 — The DA opening server authenticates** (ADR-0077 SA-2): bonded requesters, bounded bytes,
a per-bond rate; Decision 4's table row gains that column.

**SA-4 — `PATH` leaves the allowlist.** The supervisor spawns the worker by absolute path; a `PATH`
the child inherits is an execution vector on every platform without an `execve` denial (macOS
partial, Windows none).

**SA-5 — Decision 9 for a persistent runtime**: verify at map time, re-verify on file-identity
change, open read-only, fault → `JobFailed` (ADR-0077 SA-6).

**SA-6 — Decision 7 and ADR-0077 Decision 6 are one rule.** The segment-wise template is
gateway-side and consensus-inert (`TEMPLATE_ID_V1` lives in `misaka-palw-gateway`; consensus sees
ids); Decision 7's "a future class profile rather than an edit of this one" is corrected to that.
The security property is the same in both forms — untrusted text never yields a control id — and
S7's corpus pins it.

**SA-7 — Nothing logs a prompt.** Gateway, supervisor, worker and seat log no prompt text or ids by
default; `security-report` prints paths and posture, never key material or prompts.

**SA-8 — Decision 10's per-source rate is not the bound.** Sources share addresses behind proxies;
the binding limits are one job slot, a bounded in-flight queue, and a daily public-job budget tied
to exposure (ADR-0077 SA-1).
