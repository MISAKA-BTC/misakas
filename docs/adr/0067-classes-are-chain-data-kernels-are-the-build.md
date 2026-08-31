# ADR-0067 — Classes are chain data; only kernels are the build

Status: **PROPOSED.** Nothing here is implemented. Decisions 1–4 name the work; Decision 5 names
the fence it ships behind. Builds on ADR-0049 (the adjudication contract — whose admission
carriage is the load-bearing half of this design and is ALREADY LIVE), ADR-0054 (share follows
production — the economics that make permissionless classes survivable), and ADR-0034 (capability
declarations — the opt-in that makes them safe to serve). Consistent with the standing doctrine
that consensus changes ship by activation, never by re-genesis.

## The goal, stated as a test

MISAKA is meant to be a permissionless chain. Today it fails one specific permissionless test:
**a model cannot join the network without a central party cutting a release and every node
updating.** The class catalog (`canonical_classes_v1`) is compiled into the binary, and
registration, production and adjudication all read it — so "add a model" is, operationally,
"open a PR against this repository, wait for a maintainer to merge it, wait for a release, wait
for the fleet to upgrade." Whoever controls `main` controls the model set. That is a permission,
held by a central party, over the chain's defining activity.

The test this ADR wants the chain to pass: **a stranger with weights, a converter and a bond can
put a new class on the chain, and nodes that never heard of it can validate, serve and adjudicate
it — with no release in the critical path.**

## What is already true (measured, not planned)

The investigation that produced this ADR found the design half-built, which is what makes it an
ADR rather than a research program:

1. **Admission is already data-driven.** `ClassRegistered` carries a
   `PalwClassAdmissionCarriageV2` — the FULL `PalwShapeProfileV3` (the graph) and the canonical
   job — and `verify_class_admission_v2` validates the class FROM THE WIRE: shape validation,
   the coverage gate, ladder depth, the court-cost ceiling, the PWU recount. `class_id` must
   equal `profile.shape_profile_id()`, so the identity is derived from the carried data, not
   looked up. A node that has never seen a class in any table can already refuse or accept its
   registration completely. (ADR-0049 Decision H put the carriage on the object; the gate
   enforcing it from the wire is live on testnet-11 today.)

2. **The coverage gate's reference set is the KERNEL table, not the class table.**
   `verify_catalog_coverage_v1` checks the profile's kernel ids against
   `catalogued_kernel_ids_v1()` — the set of kernels this build's adjudicator can compute. A
   profile that references only catalogued kernels passes on any node, whether or not that node's
   class table has a row for it. The boundary between "data" and "build" already runs through the
   kernel set at the one place consensus checks it.

3. **Panel service is already opt-in per class.** Sortition draws seats only from bonds whose
   ADR-0034 capability declaration names the claim's `runtime_class_id`
   (`eligible_seats_v3` filters on it). Nobody is ever drawn to judge a class they did not
   declare, so a permissionless class cannot conscript anyone.

4. **The economics of a permissionless registry are already decided.** ADR-0054: registration
   requires an active bond and a signature over the class-and-share (spam pays rent), an entrant
   starts at the minimum grantable share (1‰), share grows only by filling the budget that share
   admits, and silence returns it to the floor. A garbage class that certifies is still garbage —
   and decays to nothing without anyone adjudicating its quality.

5. **The execution arithmetic is cross-architecture deterministic on the real class.** Measured
   2026-08-31: the same prompt on the same 1.7 GiB artifact produced identical output ids, all
   four leg roots, execution root, CU and claim id on arm64 and x86-64. The kernel set is the
   thing that makes this true; nothing in this ADR touches it.

## The one thing that is not true

**Execution resolves through the build.** `PalwClassSdk::resolve(class_id, artifact_root)` walks
the compiled lineage impls, each of which matches against the compiled class table, and
constructs an engine for the rows it knows. A class the table lacks answers
`"this node cannot serve the registered class"` — from a node that just VALIDATED that class's
registration from the wire. The chain can admit what the node cannot run. Closing that one gap is
this ADR.

## Decision 1 — the chain state is the class catalog; the compiled table is genesis bootstrap plus cache

`resolve` gains a second arm: when the compiled table has no row for `(class_id, artifact_root)`,
the SDK reads the class's registration from chain state — the carriage's profile IS the row. The
compiled table keeps exactly two jobs: it seeds the genesis classes (which exist before any chain
state does), and it serves as a verified cache for classes it happens to know. Authority moves to
the chain; the table stops being a gate.

The known-weights rule, the sibling filters and `registration_candidate` read the same
chain-derived ledger, so "which classes exist" has one source. The rule that a class IS its graph
is unchanged — it is what makes this safe, because a chain-carried profile hashes to the same
`class_id` on every node or it is a different class.

## Decision 2 — execution from the registered profile

Each container family gains an engine mode constructed from a `PalwShapeProfileV3` rather than
from a compiled row: the profile's pre/attn/post node tables become the op sequence, the
geometry sizes the buffers, and the artifact supplies the weights. ADR-0049's canonical IR is the
substrate — the op sequence is already generated from IR for the catalogued classes; this
decision makes the interpreter the ONLY path for chain-registered classes, and the compiled
specializations an optimization for the rows the build knows.

Two properties fall out structurally rather than by discipline:

* **ADR-0049 Decision F becomes unviolatable for interpreted classes.** Decision F requires the
  profile to name every narrowing the engine performs; the A16 v1 class was refused on the
  free-prompt lane precisely because its profile omitted a requant its engine performed. An
  engine BUILT FROM the profile cannot perform a narrowing the profile does not name — the
  correspondence check becomes the constructor.
* **A registered class needs no per-class code review**, because there is no per-class code. The
  review surface collapses to the interpreter and the kernel set, both of which are the build's
  and both of which every class shares.

Scope, stated plainly: this covers any model expressible in the catalogued kernel set — new
checkpoints, new sizes, new context widths, new quantization maps of the families the kernels
serve. It does not cover new architectures; see Decision 3.

## Decision 3 — the kernel set is the consensus surface, and it is irreducible

Adjudication means a seat recomputes the same arithmetic and compares. Executable semantics
cannot ship as data — a "portable" op language would just be a VM whose instruction set is the
same boundary renamed, with a determinism proof owed for every op on every architecture. So the
honest boundary is the one the coverage gate already enforces: **kernels are code, and a new
kernel is a consensus change**, shipped the way this chain ships consensus changes — by release
and activation, never by re-genesis.

What this decision buys is the sharp statement of what a release is FOR. Today a release gates
every model. After this ADR a release gates only:

| change | needs a release? |
|---|---|
| new checkpoint / size / n_ctx / quant map of a served family | **no** — register it |
| new model family whose graph the kernel set expresses | **no** — register it |
| new kernel (new attention variant, new activation, new routing) | yes — consensus change |
| interpreter or gate changes | yes — they are the build |

A maintainer keeps authority over the ARITHMETIC the chain can adjudicate. A maintainer loses
authority over WHICH MODELS use it. That is the intended shape of a permissionless compute chain:
the rules are governed, the participants are not.

## Decision 4 — distribution and service stay off-chain, and the chain keeps pinning only identity

`ClassRegistered` continues to carry no URL. The chain pins WHAT (class id, artifact root); WHERE
is Hugging Face, a mirror, a torrent — anyone's problem and everyone's option, made safe by the
bit-reproducible conversion (two honest converters land on the same root, so any source that
serves the pinned bytes is as good as any other).

Seat service likewise stays a market: a chain-registered class still needs seats that chose to
download its artifact and declare its capability (ADR-0034), and a panel still needs the quorum's
worth of them. The protocol's job ends at making service POSSIBLE without a release; making it
HAPPEN is the registrant's distribution work. This is the same division ADR-0054 drew for
economics — the chain prices participation, it does not recruit participants.

## Decision 5 — the interpreter ships behind a fence, and the fence's arming condition is a fuzz gate

A profile interpreter is a larger attack surface than a compiled row: the adversarial input space
is now "every profile the admission gate accepts" rather than "the rows a reviewer merged". The
existing wire-enforced bounds already contain the blast radius — the court-cost ceiling caps what
adjudicating any admitted class can cost, the PWU recount caps what it can claim to be worth, the
ladder-depth check caps the dispute tree, and a bond must sign the registration — but "bounded"
is not "verified".

So, per this repository's fence discipline (ADR-0066's shape): the chain-registered-class arm of
`resolve` ships DORMANT behind a top-level fence, and the arming condition is stated now, before
any code exists to argue with it:

* a profile-space fuzzer that drives arbitrary gate-accepted profiles through the interpreter and
  the court's close path, run to saturation with zero panics, zero non-determinism between two
  architectures, and zero closes over the ceiling;
* the interpreter's output proven bit-identical to the compiled engines on every class the build
  also carries (the compiled rows become the interpreter's reference vectors — differential
  testing the catalog gives us for free);
* one full lattice walked on a devnet for a class that exists ONLY as chain data: registered from
  the wire, served by declared seats, a claim committed, replayed, licensed and finalized, and a
  receipt block minted — with no row for it in any binary.

## What this does not do, said plainly

* **It does not remove releases from new arithmetic.** A new architecture still waits on a kernel,
  and a kernel is a consensus change. Permissionless has a floor, and the floor is "the network
  can recompute you".
* **It does not make quality permissionless — nothing does.** The gate proves adjudicability, not
  worth. ADR-0054's economics are the quality mechanism: a class nobody mines returns its share
  to the floor and its registration becomes rent paid to nothing.
* **It does not solve seat incentives, and it leans on the problem harder.** Panel seats are
  currently unpaid (the R-7 finding stands: nothing in the transition pays a
  `PalwPanelSeatV2`). A stranger's class needs five declared seats, and today those seats serve
  from goodwill. Fixing seat payment is its own ADR and becomes MORE urgent the moment classes
  are permissionless; this ADR knowingly increases demand for volunteer adjudication and does not
  supply it.
* **It does not change any consensus rule today.** Everything consensus-side that this design
  needs (the carriage, the wire-enforced gate, the kernel-set coverage check, capability-filtered
  sortition, ADR-0054's economics) is already live. The work is node-side: the SDK's resolve arm,
  the interpreter mode, the fuzz gate, the fence. Fingerprints do not move.

## The order of work

1. The interpreter mode for the dense (A16) container, differential-tested against the compiled
   rows the build already carries.
2. The `resolve` chain-state arm behind the fence, plus the ledger unification of Decision 1.
3. The fuzz gate of Decision 5, run to its stated saturation.
4. The devnet lattice walk with a chain-only class.
5. Arm on a testnet re-launch or activation; the mmap (Qwen3.6) container follows the same path
   second, because its interpreter is a larger piece and the dense family proves the seam.
