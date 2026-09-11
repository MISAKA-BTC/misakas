# The extension envelope — bringing something to a MISAKA network

*The operator's guide to ADR-0108. The ADR is the reasoning; this is the procedure, with the
example manifests in [`extension-manifests/`](extension-manifests/) and the output this build
actually prints.*

You made something — a model class, a context width, a drill's evidence, a converter — and you want
it on a MISAKA network, or you want to check somebody else's. Write **one manifest**, hand it to a
verifier, and the verifier answers with exactly one of four things:

| tier | what it means | exit |
|---|---|---|
| **expressible now** | the chain already has the object for it; admission is permissionless and the report says which object | `0` |
| **a node extension** | the chain never sees it; this build lacks the code, so the honest answer is *unverifiable here* — never "valid" | `21` |
| **a ruleset change** | a fence: the report says what the arming build would print and whether arming it is a flag day; activation is a release | `22` |
| **refused** | the manifest is wrong about itself — an id that does not recompute, a zero bound, a vector whose expected root the recomputation does not produce — **or** it is expressible and the chain would still refuse it (already registered) | `20` |

A fifth outcome is not a verdict: `23` means the depth you asked for was not reached because this
machine lacks the bytes (see [Depths](#depths)). **There is no bare "pass".**

```text
                     fixed verifier: this build's consensus-core + SDK + derive
                                          │
        ┌─────────────────────────────────┼─────────────────────────────────┐
        │                                 │                                 │
   A. EXPRESSIBLE NOW              B. NODE EXTENSION                C. RULESET CHANGE
   the chain has the object;       the chain never sees it;        a fence; accept/reject or a
   this build recomputes it        a build must carry the code     state root moves
```

## The five commands

```text
misaka palw extension inspect        <manifest>                       what it is, its id, its tier by structure alone
misaka palw extension verify         <manifest> [--depth structural|vectors|full]
                                                [--receipt-out <file>] [--key-file <path>]
misaka palw extension preflight      <manifest>                       verify at Full + the chain's shape at the node's DAA
misaka palw extension submit         <manifest> --key-file <path> [--bond <txid:index>] [--yes]
misaka palw extension receipt-verify <receipt> [--manifest <file>]
```

`--depth` defaults to `vectors`. `--output json` (or `--json`) prints the report as JSON — the same
document a receipt carries. `inspect` exits by **tier only** (`0` for expressible, even when the
report says the chain would refuse it); `verify`, `preflight` and `submit` exit by **admission**, so
a script that wants "would this be taken?" should branch on `verify`.

The key is named the way every keyed `misaka` command names it — a 0600 seed file or stdin, never
an argument and never the environment (ADR-0063 SA-1). A manifest carries **no key material**: a
field whose name looks like a key or a signature is refused as unknown before the document is read
(ADR-0108 SA-6), and `submit` signs with your key, never one the manifest names.

## The manifest

One JSON document. Its canonical form is RFC 8785 (sorted keys, no duplicate keys, no floats) and
its identity is a hash of those canonical bytes:

```text
extension_id = BLAKE2b-512( key = "misaka-palw/extension-manifest/v1", len_le64(canonical bytes) ‖ canonical bytes )
```

Two people who write the same manifest get the same id; an editor that reorders the keys does not
change it; a changed field does. The `model-class` example below is 795 canonical bytes and
`68041ae3ce95b7ba…`.

```jsonc
{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "model-class" | "context-profile" | "family-certification" | "lane-certification"
        | "derived-transformer" | "ruleset-candidate",
  "name": "a name you chose — never an identity",
  "network": "testnet-11",
  "source":   { "digest": "<64 hex>", "note": "what made it" },   // carried; read by nothing
  "artifact": { "root": "<128 hex>", "bytes": 1795427276, "path": "qwen25-1.5b.palwart" },
  "requires": { "ruleset_id": "<64 hex>", "fences": { "palw_kary_court": "active" },
                "kernel_ids": ["<128 hex>", ...] },
  "declares": { "object_id": "<128 hex — the identity this kind derives>",
                "capabilities": ["attempt", "fp"], "resource_bounds": { "memory_bytes": 6442450944 } },
  "verification": { ...kind-specific... },
  "admission": { "object": "ClassRegistered" }
}
```

Four rules worth knowing before you write one:

* **The identity is derived, never declared.** `declares.object_id` is what you *claim*; the verifier
  recomputes it from the kind's own input — `H(profile)` for a class, the grader's family id for a
  certification, `H(manifest bytes)` for a transformer — and a mismatch is refused **by field name**.
* **`source.digest` is inert** (SA-7). It is your record of what you started from, echoed by
  `inspect` and an input to nothing.
* **`requires.ruleset_id` is reported, not enforced.** A manifest written before a fingerprint moved
  is not thereby wrong, so the verifier says the two ids and verifies anyway.
* **The verifier follows nothing** (SA-2). No URL, no fetch. Every byte comes from a path you passed
  or from a path the manifest names *relative to its own directory*; `..`, an absolute path, a URL,
  and a symlink that resolves outside that directory are all refused by field name.

### Limits (SA-1)

At most 1 MiB of canonical bytes, 4,096 vectors, 64 fences; every hex field exactly 64 or 128
characters; every path relative and free of `..`. **A bound exceeded is no report** — the document
is refused at the boundary, before any kind looks at it. (The byte bound is the outer one: 4,097
two-hash vectors are over 1 MiB before they are over 4,096, and the refusal says so.)

## Depths

```text
structural   any machine: canonical form, the bounds, the id, and the kind's identity recomputed
             from what the manifest carries inline
vectors      the declared vectors re-run with this build: a transformer over its DSL vectors, a
             drill's fault vectors through the grader, a class profile through the admission gate
full         the computation the chain's transition applies, over the bytes you hold: the artifact
             read and its root recomputed, the certification graded, the registration object built
```

A machine that does not hold the weights stops early and **says so** rather than failing:

```text
  tier       expressible now as ClassRegistered
  depth      reached vectors of full requested — stopped: artifact.path not readable
```

that is exit `23`. The verdict above it still stands; what is missing is the bytes, not the answer.

## One worked example per kind

Every example is in [`extension-manifests/`](extension-manifests/) and is run by
`misaka-palw-extension/tests/doc_examples.rs`, so a stale example is a failing test rather than a
document nobody executes. Run them from that directory.

### 1. `model-class` — a class of an architecture the chain already prices

[`model-class.json`](extension-manifests/model-class.json) names the floor by its ledger row.

```console
$ misaka --network testnet-11 palw extension verify model-class.json --depth full
  tier       expressible as ClassRegistered, but would be refused: the class is already registered
             under this exact root in testnet-11's genesis: a second registration is DuplicateClass
  serving    in this build's table: true; needs the chain-class arm to be served: false (ADR-0067 D5)
  depth      reached full of full requested
```

Exit `20`, and that is correct: the floor is in genesis, so a second registration is
`DuplicateClass`. The floor is *derived* — every node mints its artifact from the pinned seed — so
Full depth recomputes its root with no file on disk.

Two things this kind tells you that no other entrance does:

* **`serving`** (ADR-0108 Decision 8): whether this build's own table carries the class. A class the
  table does not carry is registered, judged and priced by the chain exactly the same, and served by
  nobody until an operator arms `with_chain_classes_v1()` (ADR-0067 Decision 5). You learn that
  *before* you register.
* **weightless** (ADR-0069 Decision 5): a class no end-to-end certified family covers may register —
  at share 0, earning nothing until some build certifies a backend for it. The report says
  `expressible now as ClassRegistered (weightless)`; it is not a refusal.

A kernel this build cannot adjudicate is **not** refused as a bad manifest — it is a ruleset change,
naming the kernel: *"kernel 5741…4552 is outside this build's vocabulary — a new kernel is a release
(ADR-0067)"*. An artifact container no lineage of this build sniffs is a **node extension** naming
the lineage.

### 2. `context-profile` — a new width of a class that exists

A width is a class: `n_ctx` is a field of the profile and the profile's hash is the class id.
[`context-profile.json`](extension-manifests/context-profile.json) projects the dense A16 row at
`n_ctx` 24 with the shipped ladder function and points at the published 1.5B artifact.

```console
$ misaka --network testnet-11 palw extension verify context-profile.json --depth full
  tier       expressible now as ClassRegistered
  serving    in this build's table: false; needs the chain-class arm to be served: true (ADR-0067 D5)
  depth      reached full of full requested
    [FAIL] registration.new_weights — this root is already registered under another class id …
    artifact_root_form: the inventory root under this profile — the form a court-capable row registers
    pwu_per_inference: 1589424
```

Measured on the published `qwen25-1.5b-a16.palwart` (1,795,427,276 bytes): **43.5 s, 5.8 GB peak
RSS**. Without the file beside the manifest the same command stops at `vectors` and exits `23`.

Two notes:

* **Which root to write.** A court-capable row registers its *inventory root* under the profile, not
  the artifact digest; the chain's arm accepts either, and the verifier recomputes both and says
  which one matched. Get it wrong and Full depth refuses with both values printed — that is the
  cheapest way to learn the right one.
* **`registration.new_weights` is a warning, not a refusal.** The weights here are the ones the
  graph-v5@512 row already registered (the inventory root does not depend on `n_ctx`). Nothing in
  the `ClassRegistered` transition refuses a second class over registered weights; it is the SDK's
  candidate rule that never builds one for a node itself (the 2026-08-28 mispairing). So the check
  is named and the tier is left alone — a limit of one path is not a verdict on your manifest.

Outside the ladder, a width is a **ruleset change**: the report names `palw_context_ladder` and says
which wall the width hit first.

### 3. `family-certification` — a drill's evidence

Make the object first, with the drill this build ships:

```console
$ palw-certify drill --family base0 --lane attempt --out base0-attempt.borsh
wrote base0-attempt.borsh: FamilyCertified, attempt lane, family PALW-BASE-0 (43fb5f37…), 14 fault vectors, 10 kernels, 754377 bytes
```

[`family-certification.json`](extension-manifests/family-certification.json) points at it.

```console
$ misaka --network testnet-11 palw extension verify family-certification.json --depth vectors
  tier       expressible now as FamilyCertified
  checks     7 pass, 0 fail, 2 skipped
    [skip] carriage.single — 754377 bytes is above one carrier's 100000: it rides as 8 ObjectChunk
           carriers (ADR-0075 Decision 14) — `submit` cuts them
    [pass] grader
  recomputed
    family_id       43fb5f37…      (what the GRADER returned, not what the evidence claims)
    family_digest   b62721aa…
    rent_sompi      8750000        carriage.rent_per_chunk_sompi 20000000 × 8 chunks
```

At `--depth structural` nothing is graded and the report stops there: the family id is only ever what
the court's grader returns (ADR-0075 Decision 2). Flip one vector and the refusal carries the
grader's own words. Without the file, the answer is a **node extension** — "not readable on this
machine, so nothing here can be graded" — never a pass.

The chain's own refusals are checked too, in the transition's order: more than 32 vectors
(`TooManyDrillVectors`) is refused by field, and a digest the chain already certified would be
refused (`FamilyAlreadyCertified`) — visible only against live terms, since the genesis carries none.

### 4. `lane-certification` — binding a class's lane to a certified family

[`lane-certification.json`](extension-manifests/lane-certification.json) binds the free-prompt lane of
the graph-v5@512 row, from `palw-certify bind --model-id Qwen/Qwen2.5-1.5B/graph-v5@512 --lane fp`.

```console
$ misaka --network testnet-11 palw extension verify lane-certification.json --depth vectors
  tier       expressible as ClassLaneCertified, but would be refused: NoCertifiedFamilyCovers: no
             family the chain certified for the free-prompt lane covers the class's 10 kernels
             (judged at the genesis state, which certifies none) — file `palw-certify drill
             --family a16-v5 --lane fp` as a family-certification first
    [pass] chain.class_registered
    [FAIL] chain.covering_family
    covering_family  PALW-QWEN25-A16-V5      (the family THIS BUILD can drill for it)
```

Exit `20`, and the refusal is the procedure: **a lane binds to a family the chain certified**, and
the compile-time set this build ships does not count. So the order is always (1) file the
`FamilyCertified` for that family and lane, (2) file the `ClassLaneCertified`. The report names the
exact drill command for step 1.

The attempt lane has one more rule, and the report states it the same way: it seats a class that
registered *weightless*, so a class already holding a share is refused `ClassAlreadyWeighted`. Where
more than one rule would refuse, the report names the first one the transition would hit and the
ones behind it ("— and after that, …").

### 5. `derived-transformer` — a converter, and what it must reproduce

[`derived-transformer.json`](extension-manifests/derived-transformer.json) names a transformer this
build ships and gives one DSL vector with the hashes recomputing it must produce.

```console
$ misaka --network testnet-11 palw extension verify derived-transformer.json --depth vectors --receipt-out music.receipt.json
  tier       expressible now — no chain object rides for this kind
    transformer_id  4f47c578…        resolved_tree current        vectors_run 1
```

Nothing rides for a transformer: each *derivation* over it rides per claim as a `DerivedArtifactV1`
(ADR-0078), so `submit` refuses and names the tier. The id is the hash of the transformer's manifest,
so **a widened ceiling is a different transformer** (ADR-0078 SA-2) and the declared id no longer
recomputes; a zero ceiling is refused by its own field. An id from the *previous* published source
tree still resolves (`PRIOR_SOURCE_TREES_SHA256_HEX`) and the report says `resolved_tree prior …`, so
derivations a live chain already carries stay checkable. A name this build does not ship is a **node
extension**: publish the manifest (SA-5) or verify on a build that has it.

### 6. `ruleset-candidate` — what arming a fence would cost

[`ruleset-candidate.json`](extension-manifests/ruleset-candidate.json) asks for
`palw_heartbeat_transparent` at height 9,000,000.

```console
$ misaka --network testnet-11 palw extension verify ruleset-candidate.json
  tier       ruleset change (flag day) — the fork-id gate (ADR-0072 SA-2) compares heights the moment
             the schedule is announced; identity unchanged, params id and schedule id move
  fence      palw_heartbeat_transparent: requested 9000000, this build absent
  arming build would print  params 08bf8214…  identity 12e975ef…  schedule 4f3a79d2…
  this build prints         params ecbdbc22…
  fence schedule            [1150, 1900, 2150, 2400, 2125000, 9000000]
```

Exit `22`, always: **a candidate is never "expressible"**, even when it names values this build
already carries. The verifier takes this build's `Params`, sets the fences as asked, and prints what
the arming build *would* print beside what this build prints:

* a **future height** moves `consensus_params_id` and `consensus_schedule_id`, leaves
  `consensus_identity_id` — and is still a flag day, because the fork-id gate compares heights the
  moment the schedule is announced (this is what cut un-upgraded nodes at DAA ≈2,241 against a fence
  at 2,400);
* **genesis** moves the identity — refused at the handshake by every un-upgraded peer;
* a fence this build does not have says so: *the candidate needs a build before it needs a height*;
* a combination the build refuses (`validate_palw_v2`) is reported with its refusal verbatim.

Activation is ADR-0105 §7's recipe — schedule the height in the preset, rebuild, give the notice. No
manifest activates anything.

## Preflight and submit

`preflight` is `verify --depth full` plus the shape the chain would judge at the node's **current
DAA**, the object it would build, what it costs, and the refusal it would meet — before any fee. It
needs a node (`--rpc`); without one it exits `4` with the connection error.

`submit` builds the object that already exists and files it through the same carrier path
`palw submit-object` uses, **dry-run unless `--yes`**:

| kind | what is filed |
|---|---|
| `model-class`, `context-profile` | a `ClassRegistered` built by the SDK and signed with your key, which must be the **bond's** — pass `--bond <txid:index>`. Built twice (once to learn the object, once with the signature over it), exactly as `kaspad --palw-register-class` does, and written beside the manifest as `<name>.class-registered.borsh` |
| `family-certification`, `lane-certification` | the object file itself; over one carrier's 100,000 bytes it is cut into `ObjectChunk`s beside it and submitted in index order (ADR-0075 Decision 14) |
| `derived-transformer` | nothing — refused, naming `admission.object: none` |
| `ruleset-candidate` | nothing — refused, naming the tier (exit `22`) |

No manifest, receipt or extension id is ever written to the chain. The chain keeps naming what it can
recompute; the manifest is the document a stranger fetches to learn what that id means.

## Receipts — what they are, and what they are not

`verify --receipt-out <file>` writes a `misaka-palw/extension-receipt/v1`: the manifest's id, the
whole report, the verifier's crate version and `ruleset_id`, a timestamp, and — with `--key-file` —
an ML-DSA-87 signature over the receipt's canonical bytes under the context
`misaka-palw/extension-receipt/v1`.

```console
$ misaka --network testnet-11 palw extension receipt-verify music.receipt.json --manifest derived-transformer.json
receipt id    22de6e38…
extension id  6fbaefdc…  (matches the manifest)
ruleset       ecbdbc22…  on testnet-11  (this build's ruleset for that network)
signature     signed under misaka-palw/extension-receipt/v1 by 58203686…
```

**A receipt is evidence you can reproduce. It is not a vote.** No code path in `kaspad`,
`kaspa-consensus`, `kaspa-consensus-core` or the SDK reads one; no count of receipts changes any
admission, activation, fence, seat, price or share. A hundred receipts saying *expressible* do not
admit a class — the gate does, once, when the object is carried. That is the difference between
reproducibility and a Sybil vote, and it is why the chain can afford to let anyone verify.

Four properties the tool holds up:

* the **id is over the unsigned bytes**, so signing does not move it, and two verifiers signing one
  report produce the same receipt id;
* a **report byte changed after signing** fails verification, naming `signature_hex`;
* a receipt **made on another ruleset** is refused naming `verifier.ruleset_id` (SA-3) — a devnet
  receipt is not evidence about testnet-11, because the same object can be expressible on one and a
  ruleset change on the other;
* the context is the receipt's **own** (SA-4) and is deliberately not a member of the chain's
  signature-context set, so nothing signed for the chain verifies as a receipt and no receipt
  replays as a bond message, an attestation or anything else ML-DSA signs here. `receipt-verify`
  takes **one** receipt: no type anywhere holds a set of them (SA-5).

An unsigned receipt verifies as unsigned and says what that means: *"the report is whoever handed you
this file says it is — nobody vouches for it"*.

## What this does not do (ADR-0108 §8)

* **No `service-descriptor` kind.** `main` has no chain object for a service, so the name is reserved
  and refused as unknown until there is one.
* **No registration-terms RPC.** `PalwRegistrationTermsV2` has no RPC on `main`, so `preflight`
  judges against the network's **genesis** terms and every report says so:
  `terms  genesis only — the node exposes no terms RPC`. A live chain may hold classes, shares and
  certified families the genesis does not — which is why a lane binding's refusal is phrased "judged
  at the genesis state". A `getPalwRegistrationTerms` RPC is the natural next step; the library
  already takes live terms (`PalwExtensionEnvV1::chain_terms`) when a caller can supply them.
* **The chain-class seal stays** (ADR-0067 Decision 5). The report tells you it is there;
  lifting it is an economics decision about paid seats that this ADR does not make.
* **One rule no offline verifier can check**: whether the registrant's bond affords the
  registration's exposure (`RegistrationExposureUnaffordable`). It is reported as a skipped check
  rather than left silent.
