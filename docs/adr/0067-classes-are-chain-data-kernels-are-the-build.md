# ADR-0067 — Classes are chain data; only kernels are the build

Status: **Decisions 1–3 and 5 LANDED for the dense (A16) container (2026-08-31), fenced and
dormant; Decision 4 needed no code; Decision 6 including its cache bound LANDED (2026-09-01), as did
the pruning-point sidecar and the cross-architecture clause. The mmap (Qwen3.6) interpreter's blocker —
a measured defect in the class's registered graph — is CLOSED by the corrected `graph-v2` row
(2026-09-01); the interpreter itself and the operational arming remain PROPOSED.** See "What landed" at
the foot of this document. Builds on ADR-0049 (the adjudication contract — whose admission
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

## Decision 6 — four storage tiers, and node storage is never consensus state

Permissionless registration multiplies MODELS; it must not multiply every node's DISK. The failure
to refuse is mechanical: N registered classes at tens of GiB each, times a fleet that treats
"registered" as "must hold", is a fleet whose storage grows with strangers' decisions — and the
chain never asked for that. The chain's own design already refuses it (the registration carries
kilobytes and no URL); this decision makes the node side match, as four tiers with different
obligations:

| tier | holds | size | who must have it |
|---|---|---|---|
| ① chain state | class id, profile, canonical job, artifact root, admission facts | KB | every node — it IS consensus |
| ② validation artifact | a real-weights SLICE of the model plus its expected rows (the converter's `--layers N` shape), enough to prove THIS build's kernels reproduce THIS class's arithmetic bit-for-bit | MB | a node DECIDING whether to serve |
| ③ full model artifact | the weights the work runs on | GB–tens of GB | only nodes that CHOSE the class |
| ④ node cache | whichever ③s this operator serves, under an operator-set byte bound with demand eviction | bounded | nobody — it is local policy |

**Registration and possession are different acts, and nothing may couple them.** Registering a
class obligates no node to fetch anything; a node that never heard of your model validates your
registration from tier ① alone (the wire-enforced gate needs nothing else). Sortition already
draws seats only from bonds that DECLARED the class (ADR-0034), so a node that ignores a class is
never conscripted into judging it. "Every node holds every model" is not a degraded mode this
design tolerates — it is a bug in an operator's configuration.

Two honest boundaries, stated so the tiers do not over-promise:

* **Tier ② de-risks the fetch; it does not license the declaration.** A seat's duty is a REPLAY of
  the full job on the full weights, so a capability declaration backed by only the validation
  slice is the documented Incapable trap: the seat draws, cannot serve, files `Incapable`, and
  enough such seats make the claim's quorums unreachable — the declaration would kill the very
  claims it advertised for. Declaration remains "I hold ③ and can serve it"; tier ② is what lets
  an operator learn, for megabytes, whether fetching gigabytes would even be compatible — the
  plan compiles, the kernels land on the slice's expected rows — before spending the bandwidth.
* **Eviction must retract what it invalidates.** A cache that drops ③ while the bond's capability
  declaration stands re-creates the same trap on a timer. Declarations already carry a liveness
  window (`is_live_at`); an evicting node stops renewing the class's declaration FIRST and evicts
  after the window lapses, so sortition stops drawing it before the artifact is gone.

This is also where the economics point back at R-7: model count grows demand for serving and for
seats, and the supply is a MARKET — operators choosing classes worth their disk and their
compute — not a protocol obligation. A permissionless registry with conscripted storage would be
neither permissionless nor operable; a registry where holding is chosen needs holding to be worth
choosing, which is one more reason seat payment (R-7) is the next economic ADR this one leans on.

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
5. The validation-artifact format for the dense container (a `--layers N` slice plus expected
   rows), and the operator cache bound with declaration-first eviction (Decision 6).
6. Arm on a testnet re-launch or activation; the mmap (Qwen3.6) container follows the same path
   second, because its interpreter is a larger piece and the dense family proves the seam.


## What landed (2026-08-31)

* **The interpreter** (`A16Engine::plan_from_profile` / `forward_token_planned`): execution from
  the registered declaration, one committed row per declared node, refusals naming the node. The
  differential against the compiled engine holds logits, every table row and the cache state
  bit-identical — and CAUGHT A REAL DEFECT before any claim existed: the compiled engine emitted
  its SwiGLU trace rows out of the declared order, so the free-prompt capture was committing the
  silu row at the up-projection's slot; a court bisecting there would have convicted an honest
  producer. The engine now emits in the declared order.
* **The fence and the arm** (`PalwClassSdk::with_chain_classes_v1` /
  `resolve_chain_registered`): sealed by default, armed only by the greppable constructor;
  refusal order — fence, profile-hashes-to-id, canonical-names-the-class, artifact-held
  (Decision 6's "registration does not obligate possession"), plan-compiles.
* **PART of the fuzz gate** (`fuzz_a16_profiles_v1`, `palw-a16-profile-fuzz`): seeded, clockless,
  gate→plan→double-execution under `catch_unwind`. Its first 400 iterations found a
  gate-and-plan-accepted profile whose rewired refs fed a kv-width row to the q-rope and panicked
  the head slicing; the fix made the PLAN width-sound (every consumer's input width checked
  statically), eliminating the class. Saturation: 50,000 iterations, 6,207 executing, zero
  panics, zero nondeterminism.

  **The court's side, which the first version of this gate did not have.** Every gate-accepted
  profile now also has its worst close derived (`derive_court_cost_v1` — the gate's own function)
  and measured against `PALW_RC_COURT_MAX_CLOSE_BYTES`, with `closes_over_ceiling` and
  `max_close_bytes_seen` as columns, because a class the gate admits whose disputes cannot be
  carried executes, certifies and can never be policed — a defect of the adjudication half that
  no amount of executing can see. Measured at 400 iterations: 46 costed, worst close 20,257 bytes
  against a ceiling of 81,920, zero over.

  **The differential now runs over the classes this build carries**, not only synthetic
  geometries: every A16-family catalog row's real profile is compiled and the planner's answer
  pinned, and each servable row is compared row-for-row against the compiled engine at a runnable
  geometry. It pins the v1 rows' divergence as the finding it is — the compiled engine performs a
  narrowing their graph does not name, so an interpreter executing their declaration commits one
  pre row where the engine commits two, which is exactly why Decision F refuses them on this lane.

  Still honest about one clause: **"zero non-determinism between two architectures" is measured as
  two runs in one process.** The cross-architecture evidence this repo has is the manual
  arm64/x86-64 comparison of one real class (identical roots, CU and claim id), not a fuzz
  property — closing that means running the same seeded corpus on two machines and diffing the
  tallies, which is CI work, not code.
* **The chain-only lattice walk**: a class in NO table — wire-shaped registration with the
  carriage riding — applied, served through the armed arm, committed, bound (the duty naming the
  replay lane), licensed, Final, and the producer-built receipt-spend envelope admitted in full.
* **The registration index** (`PalwClassCarriages`, store prefix 226): consensus state retains a
  class's economic facts and drops the carriage, so the node keeps what the wire delivered —
  verbatim bytes, written where the lifecycle filter accepts the object (the path IBD replays),
  existence-gated against current state at every read. Exposed as
  `palw_registered_class_carriage_v1` through the consensus API and session.
* **The node half of the fence** (`--palw-chain-classes`): the panel's every duty resolve and the
  producer's class resolve route through `resolve_or_chain` — tables first, then (armed) the
  chain's own registration, and "the chain never registered it" never reads better than "this
  build cannot serve it".

* **The validation tier** (`palw-slice-kat emit|verify`): a validation artifact is an ordinary
  dense artifact converted with `--layers N` (real weights) plus a KAT binding a fixed token
  schedule to the bit-exact digest of every logits row. `verify` replays it on the candidate's
  machine and exits non-zero on a mismatch, naming the divergence; the same file carries the full
  model's identity facts machine-readably. A passing KAT licenses the FETCH, never a capability
  declaration — the tool says so on every success.

## The three remaining clauses, built (2026-09-01)

Three of the four items the section above listed as not landed are now code rather than prose.

* **The cache bound (Decision 6's ④).** `--palw-class-cache-bytes` is the number that makes
  "registration obligates no node to hold anything" true rather than aspirational. Artifacts load
  in the order the operator listed them — their priority, stated by them — and loading stops at the
  bound, measured from each file's size BEFORE it is read, because a bound that notices after
  loading is a bound that OOMs the node it was set to protect. What did not fit is NAMED in the
  log: a node quietly holding nine of ten artifacts would declare nine classes and look, to its
  operator, like it served ten. Declaration-first eviction then arrives structurally instead of as
  a rule somebody must remember — a class this node does not hold is one it cannot resolve, does
  not declare, and is never drawn to judge. Zero means unbounded, which is every node's behaviour
  before this and is right for one class on a dedicated box. Shared by the producer and the panel
  through one SDK entry point, so the two cannot drift.

* **The pruning-point sidecar.** `PruningPointPalwStateMessage` gained a repeated
  `PalwClassCarriageEntry` (additive field 3), the server fills it from the registered-carriage
  store, and the IBD client adopts each entry after the state it rides with. This is the
  unattended-fleet answer to the gap `--palw-class-carriage` covered by hand: a node syncing from
  a pruning point now receives the class carriages of classes registered before that point instead
  of needing an operator to carry them across.

* **The cross-architecture half of Decision 5.** Stated as a CI job over two machines, it is now a
  value in the artifact instead: the fuzz corpus digests its 400 seeded plans and their outcomes
  into `CORPUS_DIGEST_400`, pinned at
  `90939894923247d3e1eb18478b0495744e9ff0416bb9a20d16d01f0c411ff5eb`. Any machine running the suite
  asserts the same digest, so a divergence is a test failure on the machine that diverges rather
  than a report somebody has to compare by hand.

## Why the fourth is not merely unbuilt (2026-09-01)

The mmap (Qwen3.6) interpreter was staged second on the rollout above because it is the larger
piece. Building it turned up something the sequencing did not anticipate: **it cannot be built
correctly against the graph this class has registered, because that graph misdescribes the engine.**
Three of them are unambiguous, and all are measured mechanically by
`misaka-palw-base0/tests/qwen36_profile_conformance.rs` rather than argued from a reading:

* **The shared expert's gate is two different tensors under one name.** The engine reads
  `blk.N.ffn_shared_gate.weight` as the mixture's SCALAR gate — one output, through a sigmoid,
  applied to the whole shared row. The registered profile declares a node of that name **512
  outputs wide**, which is the shared expert's own gate projection. The artifact can supply
  exactly **one**. The engine already hit this collision and fixed its own side — `Qwen36Engine::expert`
  carries the scar, naming the expert's base `ffn_shared_expert` precisely so its gate does not land
  on the scalar's name — but the IR was never moved with it, so the chain's description of this
  class still carries the pre-fix naming. `ffn_shared_up.weight`, `ffn_shared_down.weight`,
  `ffn_shared_scalar.weight` and `ffn_shared_apply.a16` are declared and exist in no artifact.

* **An operand that chooses which experts run is named by no node.** `ffn_router_up.a16` is the
  router softmax's widening, read per layer. The engine's own comment measures the cost of getting
  it wrong: up to a factor of sixty-four in temperature, "enough to make the router select nearly
  uniformly or nearly one-hot" — it decides WHICH eight experts execute. ADR-0049 Decision F asks a
  profile to name every narrowing; this one does not merely narrow, it steers, and the profile does
  not name it.

A third measurement puts those two in proportion, because reporting them alone would understate
the gap. Counting the whole surface — quant params that ride with a declared weight excluded as
bookkeeping, global names kept whole — the registered profile names **53 operands**, an artifact of
this lineage carries **76**, and the two do not nest: **27 operands the engine reads are named by no
node**, and **14 names the profile declares are carried by no artifact**. Many of those are
defensible as FUSION — a node that stands for the eight chosen experts cannot name eight tensors,
and `ffn_gate_exps.routed` is a name for a computation rather than for bytes. That defence is also
the problem: a fused name resolves to bytes only under a rule, and no such rule is written anywhere.
An interpreter cannot follow a graph whose names it cannot resolve, and today the resolution lives
only in the compiled engine's hardcoded order — which is exactly the arrangement ADR-0067 exists to
end.

Neither of the two is breaking the live network today, and the reason is itself the point: this container
commits no step leg (`qwen36_roots_v1` says so in place — its `execution_root` is a composite over
the tiled logits trace), so nothing ever resolves these names against an artifact. They are inert
labels until an interpreter makes them the program. That is precisely the condition under which the
dense family's identical defect survived: the A16 differential found the SwiGLU rows out of declared
order only once something executed the declaration.

**The rule that was missing is now written, and it closes almost all of the gap.** The fusion was
never the problem — one node standing for "the eight chosen experts' gate projections" is a name for
a computation, not for a tensor anyone stored. The problem was that the mapping from a fused name to
the bytes it reads lived only inside the compiled engine's hardcoded order, which is precisely the
dependency this ADR exists to remove. `RESOLUTION` in the conformance test is that mapping, derived
by reading the engine's arms against the IR they claim to mirror and held to a real artifact: every
target must be a store an artifact of this lineage carries. With it, the 14 declared names no
artifact can deliver fall to **1**, and the 27 engine reads no node names fall to **0**.

**The one name left is not a name at all — it is a third finding.** The profile declares a
`VCacheWrite` node (`blk.N.attn_v_cache.a16`) that narrows the V projection as it enters the cache.
`Qwen36Engine::full_arm` pushes V in RAW — `cache.values[li].push(v)` — with no requant and no such
store in any artifact. K is normed and rotated before its cache write and the graph declares both;
the V path was written as though it were symmetric. This one cannot be closed by renaming: ADR-0030
gives a step leg one committed row per declared node, so a node with no computation behind it is a
slot that can never be filled, and every step leg of the class would be short by exactly one row.

**The corrected row now exists (2026-09-01, same day).** `qwen36_profile_v2` builds the corrected
tables: the shared-expert names the engine reads, the router widening named at the node that reads
it, the phantom V-cache node deleted with its `VCacheWrite` role moved onto the V projection — the
computation that actually feeds the cache. The GDN arm's own 24 nodes are COPIED from v1 at compile
time rather than transcribed, so they cannot drift; the attention arm's renumbering after the
deletion is hand-derived and machine-checked (`structural_diff_v1_v2` holds v2 to v1 node by node,
excusing exactly the three corrections and requiring every step reference past the deletion to
shift by exactly one). Against the same artifact measurement that convicted v1, v2's residue is
**zero in both directions** with a resolution table reduced to the structural fusions alone, and
all three findings are asserted closed (`v2_leaves_no_residue`, `v2_closes_the_three_findings`).
The v2 class id is pinned (`069b9482…`, a different class by construction), and the ledger carries
`graph-v2` rows for all three lineage members. **Registered on no chain yet** — registration is an
operator action with a bond and a fee, and it stays one. The interpreter now has a graph it can
follow; building it is the remaining piece of this clause.

**The fix cannot be an edit.** `shape_profile_id` is the borsh of the whole profile, node tables
included, and the hybrid's id is pinned in-tree as a live chain fact — "the shipped hybrid class id
moved — every registered QWEN36 claim is now unreachable". Correcting the naming produces a
different class, so it ships the way `Qwen/Qwen2.5-1.5B/graph-v2` did: a new row registered
alongside, with the genesis-registered class left exactly as the chain committed to it. The
interpreter follows that row rather than preceding it.

## What an adversarial audit found afterwards (2026-08-31)

Eight lenses over the implementation, each finding faced by two skeptics whose job was to refute
it: 28 raised, 10 survived, plus 4 from a completeness critic. What that bought, worst first:

* **A seat certified work it never priced.** The replay arm compared roots and nothing else, while
  a claim's `pwu` comes from the job shape its payload DECLARES and its `execution_root` rides
  that payload verbatim — so a producer could declare a hundred-thousand-token job, serve a
  one-token material whose roots were honestly that material's, and collect the quanta. Block
  weight in this lane would have been purchasable with recycled collateral instead of inference,
  which is the one property the lane exists to establish. Fixed: `PalwSeatDutyV2` carries the
  claim's `pwu`/`quanta` and the seat re-prices what it actually ran.
* **Every honest free-prompt claim would have defaulted its own producer.** The worker stamped
  `misaka-palw-rc` into its job context (the FLOOR's baked-in constant) while a seat derives that
  context under the node's own network name — two different context hashes, so no replay could
  ever match, and a quorum of `Unavailable` burns the producer's reserve for work performed
  correctly. Fixed: the network id is the operator's, required, with no default.
* **A gossiped empty prompt panicked the panel task**, taking down every duty that seat held.
* **Two gate-accepted profiles reached a panic or the wrong parameters** through the planner.
* **`--palw-chain-classes` armed the panel and not the producer.**
* Plus the reorg/status gates on the carriage read, and the honest corrections in this section.

**The pruned-sync gap is closed by the third door.** A node that joins by a pruned sync receives
the class table wholesale and no carriage rows, so it would refuse to serve classes the chain
registered — safe, and still a gap, because on such a fleet only nodes that watched a registration
go by could judge its class, and judging decides quorums. The two doors this ADR first named (the
pruning-point sidecar; a p2p request) are both protocol work. The third needs none:
`--palw-class-carriage <class-id>:<file>` adopts a declaration from anywhere, because a carriage
is **self-authenticating against chain state** — the chain must currently hold that class unfrozen,
the profile must hash to the class id (which is what a class id MEANS), and the canonical job must
name the same class. A supplier who satisfies all three has handed over exactly the bytes the
accept path would have written; anything else is refused, so a hostile source wastes only its own
bandwidth. The sidecar remains the better answer for an unattended fleet and is still unbuilt.
