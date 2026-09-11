# ADR-0108 — An extension is a manifest the verifier recomputes, and a receipt is evidence, not a vote

* Status: PROPOSED 2026-09-11, at the operator's request ("このような拡張をパーミッションレスで行える
  基盤を作成するADRを作成してから実装して"). **Decisions 1–9 IMPLEMENTED the same day** (§9),
  consensus-inert: no consensus object, acceptance rule, fence, parameter or fingerprint moves. A
  fleet takes it by an ordinary rebuild, and a node that never takes it is not partitioned from one
  that did.
* Builds on: [0046](0046-palw-v2-consensus-object-carriage.md) (an identity is derived, never
  declared), [0054](0054-palw-share-follows-production.md) / [0056](0056-palw-permissionless-class-admission-and-share-economy.md)
  (admission is a deterministic gate, not a vote), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  (a class the build never heard of is served from the registration the chain carries — behind a seal
  the operator arms), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) Decision 5 (a drill's
  evidence is readable by anyone), [0075](0075-certification-is-a-consensus-object.md)
  (certification is two lifecycle objects any transaction can carry), [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)
  Decisions 3 and 5 and its SA-5 (a transformer is named by its manifest, a consumer recomputes, and a
  manifest nobody can fetch is no derivation), [0072](0072-the-ticket-is-the-execution.md) SA-2
  (the fork-id gate: a scheduled height is compared, never a claim about it), and the doctrine that
  consensus changes ship as an activation and never as a re-genesis.
* Answers the operator's 2026-09-11 assessment (§1): "the verification platform is mostly built; the
  extension platform is half built; what is missing is SOURCE → canonical form, shared across the
  kinds of thing a person can bring."
* Amends nothing in consensus. Adds one crate, one CLI verb with five forms, and one document format.
* Supersedes nothing.

## 0. The sentence this ADR is

**A person who made something — with a model, a converter, a drill, or by hand — writes ONE manifest
that says what it is, what it depends on, and what recomputing it must produce; any node's verifier
recomputes it and answers with exactly one of three classifications — *expressible now* (the chain
already has the object; admission is permissionless and the manifest says which object), *a node
extension* (the chain never sees it; a node that lacks the code answers "unverifiable here", never
"valid"), or *a ruleset change* (a fence; the verifier says what fingerprint the arming build would
print and whether arming it is a flag day, and activation stays a coordinated release); and a receipt
of that recomputation is evidence someone else can reproduce, signed by whoever ran it, that no
consensus path reads and no count of which admits anything.** The chain keeps naming identities it can
recompute. The manifest is how a stranger learns what an identity means. Nothing about the generator
of the thing — a person, Claude, GPT, Kimi — is recorded, asked, or trusted.

## 1. What exists, measured on `main` `40ac431b` (2026-09-11)

The operator's assessment, checked against the tree. Each row names the primitive that makes it true.

| What a person brings | The chain's entrance (on `main`) | Verified by | Permissionless? |
|---|---|---|---|
| A class of an existing architecture (weights + profile) | `PalwConsensusObjectV2::ClassRegistered` | `verify_class_admission_v6` — `class_id = H(profile)`, kernels, court cost, canonical job, PWU all recomputed | **chain: yes**; node: the SDK's compile-time lineage table decides what this build can *serve*; ADR-0067's `with_chain_classes_v1` arm is sealed by default (§1.2) |
| A family certification (drill evidence) | `FamilyCertified { evidence }`, `ClassLaneCertified { class_id, lane, profile }` | `certify_e2e_family_v1` / `certify_e2e_free_prompt_lane_v1` — the transition grades the vectors, records only what the grader returns | **yes** (`palw-certify drill\|bind`, `misaka palw submit-object`) |
| A model line, version, proposal, evaluation | `ModelLineFounded`, `ModelVersionPublished`, `ModelProposalPosted`, `ModelEvaluationPosted` | the fold | **yes past `palw_model_lines`**, which every shipped preset leaves `None` |
| An artifact derived from a model's answer | `DerivedArtifactV1 { object, signature }` | `misaka_palw_derive::verify` — grammar, transformer, artifact hash recomputed; consensus reads `kind != 0` and nothing else | **yes on chain**; verifiable only where the transformer's manifest is published (ADR-0078 SA-5, `registry::manifest_is_published`) |
| Court evidence | `CourtOpened` … `CourtAttnRootClaimed` | the court | yes, by construction |
| A new context width inside the ladder | a `ClassRegistered` whose profile carries the width (`n_ctx` is a field of `PalwShapeProfileV3`, so it is inside the id) | the same gate, at the ladder `palw_admission_shape_at_v1` derives from `Params` at the height | **yes** |
| A new kernel, a new court, a new state transition, a width past the ladder | a fence — 29 `palw_*` fields of `Params`, every one `Option<ForkActivation>` (`palw_fences_v1`) | `consensus_params_id` / `consensus_identity_id` / `consensus_schedule_id`, the fork-id gate | **no** — a release, and this week measured what that costs (§1.3) |

Five things the table shows are true at once:

1.1 **IDENTITY / COMMIT / VERIFY / ADMIT exist, per kind, and all four recompute.** A class id is a
hash of its profile; a family id is what the court's grader returns; a transformer id is a hash of its
manifest; a derived artifact carries the hashes a consumer recomputes. Nothing on this list is a
registry a person appends to. That is the property this ADR must keep.

1.2 **What is sealed is sealed for a reason, not for lack of code.** `PalwClassSdk::resolve_chain_registered`
can serve a class the build's tables never heard of, from the registration the chain carries, and
refuses unconditionally until `with_chain_classes_v1()` is called — "arm it deliberately once the
operator accepts interpreted execution" (ADR-0067 Decision 5). The seal exists because an unpaid
panel seat is cheap to buy and a registrant who can seat itself judges its own class. This ADR does
not lift it (Decision 8).

1.3 **A fence is a flag day whether or not it is armed.** Adding a `Params` field moves
`consensus_params_id` for every preset that sets it (ADR-0095's field, once written, moved
testnet-11's printed fingerprint `060e3597…` → `ecbdbc22…` on 2026-09-11); arming it at a genesis
height moves `consensus_identity_id`, which the handshake refuses on; scheduling it at a future height
is compared by the fork-id gate the moment the schedule is announced — the third flag day cut every
un-upgraded node the day the fleet restarted, at DAA ≈ 2,241 against a fence at 2,400. None of this is
a defect: it is what a rule change *is*. It is why no manifest may ever "activate" anything (Decision 6).

1.4 **SOURCE → canonical form is the missing step, and it is missing per kind.** `palw-class inspect`
reads an artifact file; `palw-certify drill` runs a fixture drill; `palw-derive` reads a transformer by
name; a context width is a number an operator passes to `bind --n-ctx`. Each entrance takes its own
input in its own shape, and none of them takes "here is a thing I made, its identity, what it depends
on, and what recomputing it must give" as one document. A person with a new thing has to know which
tool, which flag, and which vocabulary before the first refusal tells them anything.

1.5 **No entrance says which of the three things it is.** `verify_class_admission_v6` refuses a profile
whose kernel this build lacks (`reachable_kernels_v1` finds nothing to price) with the same shape of
answer it gives a profile that is malformed. A person cannot tell "your file is wrong" from "this build
cannot judge this" from "this needs a release". Every refusal names a field — the repository's
discipline holds — but the *tier* is not a field.

## 2. The boundary, stated as three tiers

```text
                     fixed verifier: this build's consensus-core + SDK + derive
                                          │
        ┌─────────────────────────────────┼─────────────────────────────────┐
        │                                 │                                 │
   A. EXPRESSIBLE NOW              B. NODE EXTENSION                C. RULESET CHANGE
   the chain has the object;       the chain never sees it;        a fence; accept/reject or a
   this build recomputes it        a build must carry the code     state root moves
        │                                 │                                 │
   permissionless admission        permissionless on chain,        a coordinated release:
   (existing object, existing      verifiable only where the       ADR-0072 SA-2's gate, ADR-0105
   gate, existing fee)             code is; SA-5 says publish      §7's notice, one host at a time
```

* **A** is a class whose kernels this build prices, a certification this build's court grades, a
  transformer this build ships, a derivation over one, a line or version past its fence, a context
  width the ladder admits.
* **B** is a transformer this build does not ship (the chain admits its id; a consumer without the
  code says *unverifiable here*), a model architecture with no lineage in this build (the chain admits
  the registration if the kernels price; a node without the lineage cannot serve it — ADR-0067's arm
  is exactly this case for one family), a new derived kind. **B is permissionless at the chain and
  reproducible where the code is published; it is never "valid" on a machine that could not run it.**
* **C** is a new kernel, a new court, a width past the ladder, a new object, a changed transition.
  **C is never permissionless, and this ADR says so in the report rather than pretending otherwise.**
  A candidate can be *described* (Decision 6) and *reproduced* (Decision 4); it is activated by a
  release that every peer must carry, because a validator cannot verify semantics it does not have.

The user's instinct — "追加物を無限にpermissionlessにするのではなく、固定された verification language で
表現できる範囲を十分広くする" — is this diagram. The platform's job is to make A wide and B honest, and
to make C's cost visible before anyone pays it.

## 3. Decisions

### Decision 1 — One manifest, canonical, with a derived identity

`PalwExtensionManifestV1` is a JSON document. Its canonical form is RFC 8785 (JCS) as
`misaka_palw_derive::canon_json::canonicalize_json` already implements it — sorted keys, no duplicate
keys (refused before any parser could choose), no lone surrogates, shortest-form numbers. Its id is

```text
extension_id = BLAKE2b-512( key = "misaka-palw/extension-manifest/v1", canonical bytes )
```

the same shape `transformer_id_v1` has. Two people who write the same manifest write the same id; a
manifest whose bytes were reordered by an editor has the same id; a manifest with a field changed has
a different one.

```jsonc
{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "model-class" | "context-profile" | "family-certification" | "lane-certification"
        | "derived-transformer" | "ruleset-candidate",
  "name": "a name the person chose — never an identity",
  "network": "testnet-11",
  "source":   { "digest": "<64 hex>", "note": "what made it; opaque; never checked" },
  "artifact": { "root": "<128 hex>", "bytes": 1795427276, "path": "qwen25-a16.palwart" },
  "requires": { "ruleset_id": "<64 hex consensus_params_id>",
                "fences": { "palw_kary_court": "active" },
                "kernel_ids": ["<128 hex>", ...] },
  "declares": { "object_id": "<128 hex — the identity this kind derives>",
                "capabilities": ["attempt", "fp"],
                "resource_bounds": { "memory_bytes": 2147483648 } },
  "verification": { "vectors": "<path or inline, kind-specific>",
                    "expected": { "artifact_root": "<128 hex>", "family_id": "<128 hex>" } },
  "admission": { "object": "ClassRegistered" }
}
```

**The identity is derived, never declared** (ADR-0046, restated for manifests). `declares.object_id`
is what the person *claims*; the verifier recomputes the kind's identity from the kind's own input —
`H(profile)` for a class, the grader's family id for a certification, `H(manifest bytes)` for a
transformer — and a mismatch is refused **by field name**, never averaged, never taken on trust.
`source.digest` is the only field the verifier never checks: it is the person's own record of what
they started from, and this ADR states in §6 that nothing downstream may ever read it as evidence.

`requires.ruleset_id` is the `consensus_params_id` the manifest was written against. A verifier on
another ruleset does not refuse — it reports the mismatch and verifies anyway — because a manifest
written before a fingerprint moved (§1.3 happened twice this week) is not thereby wrong.

### Decision 2 — Every answer names its tier

`verify_extension_v1` returns `PalwExtensionReportV1`, and the report's `classification` is one of:

```text
Expressible   { admission_object, would_be_refused: Option<field-naming reason> }
NodeExtension { missing: what this build lacks, by name — a transformer, a lineage, a kernel id }
RulesetChange { fences: [(name, requested height, this build's value)],
                would_print: { params_id, identity_id, schedule_id },
                flag_day: bool, reason }
Refused       { field, reason }
```

**There is no bare `PASS`.** A verifier that could not run the thing says `NodeExtension`, and a
report that says `Expressible` says *which* object it would be. `Refused` is for a manifest that is
wrong about itself — an id that does not recompute, a bound that is zero, a vector whose expected root
the recomputation does not produce. A limit of this build is not a verdict on the manifest
(the repository's own rule: a limit is not a verdict).

### Decision 3 — Verification has three depths, and the report says which it reached

```text
Structural   any machine: canonical form, field bounds (§6 SA-1), the id, the kind's own
             identity recomputed from what the manifest carries INLINE (a profile, a
             transformer manifest, a fence list)
Vectors      the declared vectors re-run with this build: a transformer over its DSL vectors,
             a drill's fault vectors through certify_e2e_family_v1, a class profile through the
             admission gate at the named shape
Full         the same computation the chain's transition applies, over the bytes the person holds:
             the artifact file read and its root recomputed (the SDK's pairing), the certification
             evidence graded, the registration object built and the gate asked
```

A machine that holds no 34 GiB artifact stops at Structural or Vectors and the report says
`depth_reached: Vectors`, `depth_requested: Full`, `stopped_at: "artifact.path not readable"`. That is
not a failure; it is the sentence the operator's Mac-versus-GPU case asked for.

### Decision 4 — A receipt is evidence of reproduction, signed by whoever reproduced it — and it is not a vote

`PalwExtensionReceiptV1` is a canonical JSON document carrying the manifest's `extension_id`, the
report (classification, depth, every check by name with `pass | fail | skipped(reason)`, the
recomputed roots), the verifier's `ruleset_id` and crate version, a timestamp the verifier chose, and
optionally an ML-DSA-87 signature by the verifier's key over the receipt's canonical bytes, under the
context `misaka-palw/extension-receipt/v1`. That context is a constant of the extension crate and is
**deliberately not added to the bundle's `signature_contexts_root`**: that root is a consensus set
(`palw_signature_contexts_v2`, armed at genesis), and adding a member moves the fingerprint — a
receipt signs nothing the chain ever verifies, so the chain's set has no reason to know it.
`receipt_id = H(canonical bytes without the signature)`.

What a receipt is for: two receipts from two builds that disagree about the same manifest are a bug
report with a preimage attached. A person who cannot run the Full depth can read receipts from people
who could, and can check that those people signed what they claim.

What a receipt is **not**, stated as a rule the tests hold (§7 I-9): **no code path in `kaspad`,
`kaspa-consensus`, `kaspa-consensus-core` or the SDK reads a receipt; no count of receipts changes any
admission, activation, fence, seat, price or share.** A hundred receipts saying `Expressible` do not
admit a class; the gate does, exactly once, when the object is carried. A hundred receipts saying a
ruleset candidate reproduces do not arm a fence; a release does. This is the difference between
reproducibility and a Sybil vote, and it is the whole reason the chain can afford to let anyone verify.

### Decision 5 — Preflight and submit go through the objects that already exist; nothing new rides

`preflight` = `verify` at depth Full, plus the shape the chain would judge at a height: `Params` at
the node's current DAA (`palw_admission_shape_at_v1`: which court, which ladder, which prompt-ids
form), the object it would build, what it costs (a carrier fee for a certification; a registration's
share and slash value from the network's terms), and the refusal it would meet — **before any fee**.

`submit` builds the existing object and files it:

| kind | object built | how |
|---|---|---|
| `model-class`, `context-profile` | `ClassRegistered` via `PalwClassSdk::build_post_genesis_registration` | needs the registrant's bond key (the same object `kaspad --palw-register-class` files in-process); the CLI signs with its own key and files through `submit-object` |
| `family-certification` | `FamilyCertified` | `submit-object`, unsigned (the fee is the rent, ADR-0075) |
| `lane-certification` | `ClassLaneCertified` | `submit-object` |
| `derived-transformer` | nothing — a transformer is not a chain object; each derivation over it rides per claim (`DerivedArtifactV1`) | `submit` refuses, naming `admission.object: none` |
| `ruleset-candidate` | nothing — a fence is a release | `submit` refuses, naming the tier |

No manifest, receipt, or extension id is written to the chain. The chain keeps naming what it can
recompute (a class id, a family id, a transformer id); the manifest is the document a stranger fetches
to learn what that id means, published wherever the artifact is (Decision 9).

### Decision 6 — A ruleset candidate is described and costed; it is never activated by a manifest

`kind: ruleset-candidate` names fences and heights: `{"palw_heartbeat_transparent": 12000}` or
`{"palw_context_ladder": "genesis"}`. The verifier takes this build's `Params` for the named network,
sets the fences as asked, and reports what the arming build **would print** — `consensus_params_id`,
`consensus_identity_id`, `consensus_schedule_id`, the fence schedule — beside what this build prints;
whether the identity moves (arming at genesis) or only the fingerprint and schedule (a future height);
and whether the build refuses the combination (`validate_palw_v2`, which refuses a fence its ruleset
cannot carry — ADR-0096's decode constraint, ADR-0105's rule without the heartbeat lane). A fence name
this build does not have is reported as such: the candidate needs a build before it needs a height.

That is the whole of what a manifest may do to a rule: say what arming it costs. Activation is ADR-0105
§7's recipe — schedule the height in the preset, rebuild, and give the notice the fork-id gate makes
necessary. A candidate's receipts are reproducibility evidence for the *release discussion*; the
release is what activates.

### Decision 7 — A context width is a class inside the ladder and a ruleset candidate outside it

`kind: context-profile` is `model-class` with the width named: the manifest carries a profile (or a
registered class id and a width), the verifier projects the profile at that width and asks the
admission gate at the network's shape. Inside the ladder the answer is `Expressible { ClassRegistered }`
— the same object, a different id, because `n_ctx` is inside the id. Outside it, the gate's refusal
is translated into `RulesetChange { fences: [palw_context_ladder …] }` with the height field left for
the person to fill, and the report says which wall (the ladder, the court's cost shape, the DA form)
the width hits first. ADR-0097's fit walls and ADR-0103's held context live on branches as of this
writing; this kind maps onto whatever walls the build carries, by asking the gate rather than by
restating them.

### Decision 8 — The chain-class seal stays; the manifest tells the operator it is there

`resolve_chain_registered` stays sealed behind `with_chain_classes_v1()` for ADR-0067 Decision 5's
reason. A `model-class` report says, beside `Expressible`, whether **serving** the class on this build
needs the arm (`serving: { in_build_table: false, needs_chain_classes_arm: true }`), so a person learns
before registering that their class will be registered, judged, and — until an operator arms the seal
— served by nobody. Lifting the seal is an economics decision (paid seats) this ADR does not make.

### Decision 9 — Publication and discovery are off-chain, and the id is what makes that safe

A manifest is published beside its artifact — a file, a URL, a content-addressed store; this ADR does
not choose. What makes that safe is that nothing depends on *where*: the id is a hash of the bytes,
the identities inside it recompute, and a manifest fetched from anywhere is either the one whose id
was quoted or refused. The verifier **never follows a reference on its own** (§6 SA-2): every byte it
reads is a path the person handed it.

## 4. The CLI

```text
misaka palw extension inspect   <manifest>              what it is, its id, its tier by structure alone
misaka palw extension verify    <manifest> [--depth structural|vectors|full] [--receipt-out <file>]
                                                        [--key <source>]  recompute; write a receipt
misaka palw extension preflight <manifest>              verify at Full + the chain's shape at the node's DAA
misaka palw extension submit    <manifest> --key <source> [--yes]
                                                        build the existing object and file it (dry-run unless --yes)
misaka palw extension receipt-verify <receipt> [--manifest <file>]
                                                        the signature, the id, the ruleset it was made on
```

Exit codes distinguish the tiers so a script can branch: expressible-and-would-be-admitted `0`;
refused (the manifest is wrong about itself) `2`; node extension (unverifiable here) `3`; ruleset
change `4`; a depth not reached because the machine lacks the bytes `5`. Every non-zero exit prints the
field or the missing thing by name.

## 5. What this costs

A manifest is small (bounded, §6 SA-1). Structural verification is microseconds. Vectors is what the
vectors cost — a drill's fault vectors through the court are seconds; a transformer over its DSL
vectors is milliseconds. Full is the artifact: the SDK's pairing reads the whole file once (1.7 GiB
dense in seconds, the 34 GiB mmap tier in minutes on a cold disk). Nothing here adds a byte to a block,
a millisecond to a transition, or a field to `Params`.

## 6. Security amendments, stated before the build

* **SA-1 — bounds.** A manifest is at most 1 MiB canonical; at most 4,096 vectors; at most 64 fences;
  every hex field exactly 64 or 128 characters; every path relative and free of `..`. A manifest over a
  bound is refused before it is parsed further. A bound exceeded is no report.
* **SA-2 — the verifier follows nothing.** No URL, no fetch, no "the manifest says the artifact is
  here". Every byte comes from a path the person passed on the command line, and a path the manifest
  names is resolved relative to the manifest's own directory and refused if it escapes it.
* **SA-3 — a receipt names the ruleset it was made on**, and `receipt-verify` refuses to call a receipt
  made on another `ruleset_id` a receipt for this one. A receipt from a devnet is not evidence about
  testnet-11 (the same object can be Expressible on one and RulesetChange on the other).
* **SA-4 — the signature context is its own.** `misaka-palw/extension-receipt/v1` is used for
  receipts and nothing else, and it is not in the chain's context set (Decision 4); a signature under
  another context does not verify, so a receipt cannot be replayed as an attestation, a bond message,
  or anything else ML-DSA signs in this system — and nothing signed for the chain verifies as a receipt.
* **SA-5 — no aggregation, anywhere.** No struct in consensus, node, SDK or CLI holds a *set* of
  receipts. `receipt-verify` takes one. This is the mechanical form of Decision 4.
* **SA-6 — `submit` signs with the operator's key, never one the manifest names.** A manifest carries
  no key material and no signature; a field that looks like one is refused as unknown.
* **SA-7 — `source.digest` is inert.** It is carried for the person and is not an input to any check,
  any id, or any report field other than its own echo. A build that ever reads it as evidence has
  changed this ADR.

## 7. Invariants the tests hold

* **I-1** The same manifest with keys reordered, whitespace changed, or written by another serializer
  has the same canonical bytes and the same `extension_id`; a duplicate key is refused; a changed field
  changes the id.
* **I-2** A manifest whose `declares.object_id` is not what the kind recomputes is `Refused` naming
  `declares.object_id`, for every kind.
* **I-3** Every field bound in SA-1 is enforced at the boundary, with the offending field named, before
  any kind-specific work.
* **I-4** `model-class`: the RC's own genesis rows (the SDK's built-in ledger) verify `Expressible {
  ClassRegistered }` and the report says they are already registered; a profile whose kernels this
  build's vocabulary cannot price is `RulesetChange` naming the kernel (a new kernel is a release,
  ADR-0067); a profile no certified family covers is `Expressible` and the report says *weightless*
  (ADR-0069 Decision 5); an artifact container no lineage of this build sniffs is `NodeExtension`
  naming the lineage; the serving flag says whether the class is in the build's table.
* **I-5** `family-certification`: the shipped attempt-lane drill evidence verifies to the pinned
  family id at depth Vectors; one flipped vector is `Refused` with the grader's own error name.
* **I-6** `derived-transformer`: a manifest naming a transformer this build ships verifies at depth
  Vectors, and one naming the previous published tree's id resolves through
  `PRIOR_SOURCE_TREES_SHA256_HEX`; a name this build lacks is `NodeExtension`; a widened ceiling is a
  different id (ADR-0078 SA-2, re-pinned here).
* **I-7** `ruleset-candidate`: a dormant fence at a future height → params id and schedule id move,
  identity does not, `flag_day: true` with the reason "the fork-id gate compares heights"; the same
  fence at genesis → identity moves; an unknown fence name → the report says a build is needed; a
  combination `validate_palw_v2` refuses is reported with its refusal.
* **I-8** A receipt round-trips: sign, canonicalize, verify; a receipt whose report byte changed after
  signing fails; a receipt made on another ruleset is refused by `receipt-verify` naming `ruleset_id`;
  an unsigned receipt verifies as unsigned and says so.
* **I-9** No aggregation: the workspace has no dependency edge from `kaspad`, `kaspa-consensus`,
  `kaspa-consensus-core`, `misaka-palw-sdk` or `misaka-palw-derive` to the extension crate (`cargo
  tree -i misaka-palw-extension` names only the CLI), so nothing that decides anything can read a
  receipt. Recorded in §9 with the measured output.
* **I-10** The verifier never opens a path outside the manifest's directory, and never a URL.

## 8. What is deliberately not decided, and what is not verified

* **A `service-descriptor` kind.** The operator's list has it; `main` has no chain object for a
  service (ADR-0101's "anyone serves" lives on a branch). Reserved as a name, refused as unknown until
  there is an object for it to name.
* **Registration terms over RPC.** `PalwRegistrationTermsV2` (registered class ids, registered
  artifact roots, chain-certified families) has no RPC on `main`; `kaspad --palw-register-class` reads
  it in-process. `preflight` therefore judges against the network's genesis terms and the fences at the
  node's DAA, and its report carries `chain_terms: "genesis only — the node exposes no terms RPC"`. A
  `getPalwRegistrationTerms` RPC is the natural next step and is not built here.
* **Sharded seats (ADR-0099) and held context (ADR-0103)** are on branches; the `context-profile` kind
  asks the gate this build has, and will ask the walls those ADRs add when they land, without a change
  here.
* **Lifting the chain-class seal** (Decision 8) waits on paid seats.
* **Not verified:** a Full-depth run over the 34 GiB Qwen3.6 tier on a fleet host; a `submit` of a
  `model-class` against a live network (the object-building path is the SDK's, already exercised by
  `kaspad --palw-register-class`; the CLI wiring is exercised in dry-run).

## 9. Number hygiene and implementation record

`docs/adr/README.md` on `main` at `40ac431b` says the next free number is 0108 (0099–0104 and
0106–0107 are resident on other branches; 0105 landed here). This ADR takes **0108**. A concurrent
claimant renumbers the later writer. **The next free number is 0109.**

Implementation, 2026-09-11, on `feat/adr-0108-extension-envelope` — see the commit log for the
measured results (§7's invariants, the crate's tests, the CLI's dry runs, and the `cargo tree -i`
output I-9 requires).
