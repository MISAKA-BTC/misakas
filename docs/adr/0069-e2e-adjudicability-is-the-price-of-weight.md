# ADR-0069 — End-to-end adjudicability is the price of weight

Status: **PROPOSED (2026-09-01).** Written against the ADR-0068 launch audit, whose central
finding was that Relaunch 5's genesis share table hands **97.8% of cadence** (QWEN36 489‰ +
QWEN25-A16 489‰) to families that carry no court responder — `supports_court()` is `false` for
both, `bisect_prefix_state`/`refutation_for_index` take the trait's defaults, and a producer that
never runs the model cannot be convicted (audit F2/F6/F8). Builds on ADR-0039 (a class is
weightless until its kernel catalog closes), ADR-0049 (the adjudication contract), ADR-0067
(classes are chain data; only kernels are the build), and ADR-0054/0056 (share follows production;
permissionless admission). Consistent with the standing doctrine that consensus changes ship by
activation, never by re-genesis.

## 1. The goal, stated as a test

PALW's one claim is that its blocks are paid for by *actual* LLM inference. The claim is only as
strong as the chain's ability to **convict** a producer who did not do the work. Today that ability
is asserted, not enforced:

* `supports_court()` is consulted in exactly two node-local places — `kaspad/src/palw_producer.rs:601`
  (it prints a warning and keeps producing) and `kaspad/src/palw_panel.rs:1785` (it buckets a stall
  reason). It appears in **no** consensus rule. A family can answer `true` or `false` and the chain
  pays it the same.
* The static conformance battery (`misaka-palw-sdk`'s `check_lineage_v1`) proves the *profile* is a
  walkable step space whose every kernel the adjudicator re-executes. It never instantiates a
  backend, so it cannot see that the backend cannot actually **play** the court.

So the chain grants fork-choice weight to families it cannot prosecute. The test this ADR wants the
chain to pass:

> **No family carries fork-choice weight unless this build can take that family's own backend,
> on a real anchor, all the way through a dispute — `execute`, `verify_material`, `bisect_prefix_state`,
> `refutation_for_index`, close — and have the close convict a planted fault and acquit the honest run.**

Weightless registration stays permissionless. **Weight** is the thing that must be earned by being
certifiably convictable.

## 2. What is already true (measured, not planned)

The design is half-built, which is what makes this an ADR and not a research program.

* **The static half exists and is unforgeable by construction.** `verify_catalog_coverage_v1` /
  `verify_profile_coverage_v1` (`consensus/core/src/palw_catalog_coverage.rs`) prove per-kernel and
  per-node-shape adjudicability, and their certificate is constructible *only* through the sealed
  constructor (`PalwReachableKernelSetV1`'s private `_sealed` field) — so "we checked coverage" and
  "a certificate exists" are one fact. The result is committed as the bundle's `court_catalog_root`
  — this build's adjudicable primitive set as one hash (`palw_catalog_coverage.rs:173`). This is the
  exact mechanism ADR-0067 uses to make adjudication a property of the build that two honest nodes
  compute identically.
* **BASE-0 already carries the end-to-end drill.** `misaka-palw-base0/src/backend.rs`:
  `execute_with_injected_fault(job, prompt, leaf)` (:189) produces *guilty* material; then for the
  honest and the guilty run alike, `refutation_for_index` opens (:528, :568) and
  `bisect_prefix_state` before and after the fault leaf shows the two executions agree *before* and
  differ *at* it (:575-579). This is precisely the certification vector — for one family.
* **The gap is three-fold.** (a) Nothing requires that drill of anyone. (b) For QWEN36 and
  QWEN25-A16 the court methods are the trait defaults (`consensus/core/src/palw_backend.rs:176,193,205`
  return `None`/`false`/`Err`); `bisect_prefix_state` is implemented in exactly one place in the
  whole tree (BASE-0, `backend.rs:239`). (c) Admission cannot even *express* weightlessness:
  `granted_share_table_v2` refuses a zero grant by construction — `min_grantable_share_permille` is
  at least 1 (`palw_class_admission_v2.rs:38-42`) — so "register it weightless" is, in today's code,
  a fiction: every registered class already gets ≥ 1‰.

## 3. Decision

### Decision 1 — Two adjudicability properties, named apart

**Static adjudicability** — the profile is a walkable step space whose every reachable kernel and
every node shape the build re-executes. Already defined and already gated (§2, first bullet).

**End-to-end adjudicability** — a real backend, given a real anchor, plays a real dispute to a
conviction. Not defined anywhere today.

Weight requires **both**. Registration and liveness require only the static one (ADR-0039's
"admissible for liveness, weightless").

### Decision 2 — E2E certification is a build fact, committed like the catalog

Mirror `court_catalog_root` exactly. Introduce `court_e2e_root` (name provisional): the hash of the
set of **family descriptors** this build has certified end-to-end, where a certificate is
constructible *only* by running the drill of Decision 3 to a conviction — a sealed constructor, the
same pattern as `PalwReachableKernelSetV1`. Two honest nodes agree on the root because both compute
it from the same build.

This keeps ADR-0067 intact: the **class** (its profile and artifact) remains chain data; the
**build** remains the authority on adjudication — previously over kernels, now over the whole court
turn. The `court_e2e_root` rides the bundle beside `court_catalog_root`.

### Decision 3 — The certification drill, defined

For a family `F` and its canonical job, `F` certifies iff, for a **covering leaf set** `L`:

1. `execute` yields honest material and its four roots.
2. `execute_with_injected_fault(job, prompt, ℓ)` yields guilty material for every `ℓ ∈ L`.
3. For the honest run and every guilty run, `refutation_for_index(material, ℓ)` opens (`Ok`), and
   `bisect_prefix_state(material, i)` is a true prefix commitment — two runs that agree through
   index `i` return the same state at `i`, and two that first differ at `ℓ` return equal states for
   `i ≤ ℓ` and unequal for `i = ℓ+1`.
4. The **actual court** resolves it: `adjudicate_court_close_v2` → `check_step_refutation_v1`
   convicts each guilty run at its planted leaf and acquits the honest run. Not a re-implementation
   of the court — the drill drives the shipped adjudicator.
5. Everything step 4 reads is *available*: the retained material carries the per-step tiles and the
   binding, **or** the checkpoint leg re-captures them (`Base0CheckpointCaptureV1::push_chunks`
   against `next_geometry`), and either way the close fits under the carriage close ceiling
   (tiled, never flat — a Qwen-scale logits row is ≈ 993 KiB against an 80 KiB ceiling).

The passing vectors from a certification run **are** the family's regression test and its
certification evidence — the same artifact serves both, as BASE-0's drill already does.

`L` must be a *covering* set: it must include a leaf in every table the profile declares
(`pre` / `gdn` / `attn` / `post`) and at least one prefill and one decode position. A fault at a
leaf `L` omits is a step the family could diverge on unconvicted, so a smaller `L` is a smaller
guarantee and the certificate must record the `L` it vouched for (as coverage records the reachable
set it vouched for, not the catalog snapshot that covered it — `palw_catalog_coverage.rs:129`).

### Decision 4 — The graph must not lie about the engine (ADR-0049 Decision F, enforced)

A family certifies only if the narrowings its engine actually executes equal the narrowings its
graph declares. This needs, per engine, a `plan()` and a `check_graph` — the counterpart BASE-0 has
in `base0_check_graph_v1` and A16 lacks (`A16Engine` has no `plan()`; its `pre` table performs an
**undeclared requant** that lifts the embedding onto the A16 stream — `misaka-palw-base0/src/legs.rs:113-119`).
The drill of Decision 3 would catch an undeclared step anyway (the bisection diverges at it), but a
graph check names it at build time instead of at a conviction, and turns "the class misdescribes its
engine" from a live-fire discovery into a compile error.

### Decision 5 — The admission gate grants weight only to certified families

Two changes, both local:

1. **Make weightlessness expressible.** Permit a genuine zero-share admitted state — a `share = 0`
   grant, or a `liveness_only` flag on the registration — removing the `min_grantable ≥ 1` fiction
   for uncertified families. A weightless class still produces blocks, is still gossiped and stored,
   still contributes to liveness; it simply earns no cadence and cannot dilute the certified share
   table.
2. **Gate the weight-bearing grant.** `verify_class_admission_v2` / `granted_share_table_v2` refuse
   a nonzero grant unless the registered `PalwRegistrationTermsV2.family` is present in this build's
   `court_e2e_root` set. The judging material is already on chain — `family` is a registration term
   — so this is a set-membership check, not a new object.

### Decision 6 — Permissionlessness and the doctrine are preserved

Anyone still registers any model with a bond (ADR-0054/0056). Certification is **not** a maintainer's
signature; it is a mechanical property of a build, so a third party who ships weights, an adjudicable
backend, and the passing vectors certifies a family without asking anyone — exactly as ADR-0067
intends. Turning a family weight-bearing later is an **activation** (the `court_e2e_root` moves the
way `court_catalog_root` does), never a re-genesis (standing doctrine).

## 4. What this fixes, and what it costs

**Fixes.** Audit F6/F8 directly — the unconvictable 97.8% can no longer hold weight, so a producer
that never runs the model earns nothing whether or not anyone disputes it. It also drains the pool
two other findings drank from: the clone-flood (F4) that moved cadence onto model-free classes gains
nothing if those classes can't hold weight without certifying a real backend, and the round-0
silence acquittal (F2) stops paying, because silence on an uncertified family's dispute defends a
weightless claim.

**Costs.** The drill is CI-time and already exists for one family. The root is one hash on the
bundle. The admission change is a membership test plus a zero-share state. The real work is
per-family — making QWEN36 and A16 actually certify — and that work is already scoped by ADR-0067's
A16 items (the checkpoint leg, the rung methods, the graph check) and by the manual that accompanies
this ADR (`docs/misaka-palw-model-adjudicability-guide-v0.1-ja.md`).

## 5. Considered and rejected

* **Trust `supports_court()`.** Rejected: it is node-local and lie-able, and the audit shows it is
  already `false` where it matters while cadence flows regardless. A build fact committed into the
  ruleset root is the only form two honest nodes cannot disagree about.
* **Forbid registration of uncertifiable families outright.** Rejected: it breaks ADR-0039's
  deliberate "admissible for liveness, weightless" state and the permissionless test — a family must
  be able to exist and produce while its adjudication is still being built. The gate belongs on
  *weight*, not on *existence*.
* **Certify off-chain with a signed attestation.** Rejected: it reintroduces the central party
  ADR-0067 exists to remove. The build, not a signer, is the authority.
* **Charge the accuser for an unanswerable dispute (the pre-audit behaviour).** Rejected already by
  ADR-0067's `rearm_after_unanswered_opening`; noted here because it is the failure this gate
  replaces — the old chain made prosecuting an uncconvictable class a guaranteed loss, which is why
  the classes shipped unprosecuted.

## 6. Invariants to verify at each step

1. **No unearned weight.** For every Active class, `share > 0 ⇒ family ∈ court_e2e_root`. A test
   over the shipped genesis table and over a synthetic post-genesis registration.
2. **The covering set is covering.** The drill fails if `L` omits any declared table or omits a
   prefill or a decode position — asserted as a *difference* (a class that passes with a full `L`
   must fail with an `L` missing one table), so a collapsed `L` cannot pass silently.
3. **E2E ⊆ catalog.** `court_e2e_root`'s families are a subset of what `court_catalog_root` covers —
   you cannot be end-to-end certified for a kernel you cannot even catalog.
4. **Weightless is still chain data.** A `share = 0` class round-trips through carriage, IBD and
   pruning byte-identically to a weight-bearing one; only the grant differs.
5. **Genesis honours the gate.** Relaunch 5's share table assigns weight only to families in the
   shipped `court_e2e_root` — which, until A16 certifies, is `{BASE-0}`. QWEN36 and QWEN25-A16
   register weightless until their certification lands, then take weight by activation.

## What landed

Nothing yet — this is PROPOSED. The tracking items are: `court_e2e_root` on the bundle and its
sealed certificate (Decision 2); the drill harness generalized out of BASE-0's tests into a
build-level gate (Decision 3); per-engine `plan()`/`check_graph`, starting with A16 (Decision 4);
the zero-share admitted state and the weight gate in `verify_class_admission_v2` (Decision 5); and
the Relaunch 5 genesis table reduced to `{BASE-0}` weight-bearing until the model families certify
(Decision 6, invariant 5).
