# RFC0006 native routing execution consumers (2026-10-18)

Status: source implementation checkpoint, UNCOMPILED / UNEXECUTED. No test,
node, benchmark, measurement, Python compile, or build was run. Only source
inspection, rustfmt and diff checks were used. This is not RFC06 completion.

## Prospective block context

The SDK transmits its computed holding identity to Q36. Before execution, the
producer requests a separate opaque runtime authority from its own node. The
getter fixes the selected-parent native snapshot and import generation, and
requires the candidate's direct-parent set, DAA, bits, blue work and blue score
to match the own template context at both ends. The computed AFTER(checkpoint)
root must equal the candidate's `palw_state_root`. The checkpoint header itself
commits its parent's state and is not used as an AFTER(checkpoint) root.

CoreV1's job context deliberately uses `misaka-palw-rc` network bytes. Its
execution anchor binds the actual network/genesis. The authority now derives
the full canonical context from native profile, formula, prompt commitment and
draw rule rather than incorrectly requiring those fixed bytes to equal the
node's network ID. Prompt/tree scratch is reserved before derivation. Legacy
contexts retain their explicit network check.

The actual Q36 per-layer plan retargets every routed reader to its native
declared router. Multiple independent routers remain supported. The backend
retains the native snapshot, immutable row, whole job and plan allocation owner
through execution and material publication. It does not use a new declaration
to reinterpret a historical V1 claim.

## Accepted claim replay

`PalwNativeRoutingClaimAuthorityV2` is a distinct runtime owner with no wire,
Borsh or deserialization constructor. The own getter reads a stable native
checkpoint, fully validated immutable header and import bracket. A current
claim must have its own durable V2 stamp; the declaration's registrant,
artifact, profile and prospective bound are checked against native state.
Unstamped claims keep V1 semantics; unreadable/present invalid stamps do not
fall back. Removed classes and missing native identities are unavailable.

For attempts, the claim's recorded job identity and native canonical full
context are checked. That recorded identity also names accepted riders' own
anchors; it is not replaced by a carrying-header anchor. For free-prompt
claims, the exact context must reproduce the native job pin and accepted class
semantics. The source Arc, row owner, whole-job owner, and plan owner remain
live through replay. Fresh getters may advance to another normal chain point,
but must preserve the same accepted claim, declaration, execution, job and
import generation before a result is consumed.

The main panel's immediate attempt replay and SEAT-R attempt worker now bind
this owner into the real backend before execution/polling and refresh it before
consuming a result. The FP SEAT-R worker prefers an exact captured context
whose native job pin matches, and binds the same actual Q36 plan. FP pipeline
stop probes check every fixed identity field; the final run checks the exact
executed context. Audit and operator-DA workers bind before starting and retain
the original owner until a fresh own read allows their result to be consumed.
Local failures remain no-verdict/unjudged, not an arithmetic conviction.

These are identity and plan consumers. Matching arithmetic does not create
selected-verifier approval, SecurityCertified, credit, or settlement.

## Explicit residuals

- Prospective FP production needs its own pre-claim job/descriptor factory.
  Prospective riders need a separate native lead/index factory. They must not
  reuse the block-attempt factory.
- Some accepted-rider callers still derive the carrying block's anchor. The
  accepted factory refuses that different job; those callers still need to
  select the recorded rider anchor before replay. This guard is not coverage.
- Generic DA capture recovery, fraud-filer and partial-resume workers have not
  all been connected to this new owner. The main-panel/audit/operator changes
  do not establish all-consumer completion.
- A stopped FP job with no capture may lack the exact executed count needed to
  reproduce its native pin before replay. The ceiling context is not silently
  substituted. Missing accepted semantic identity is also unavailable; this
  checkpoint introduces no historical migration or reinterpretation.
- Inherited-prefix FP contexts need their original accepted descriptor: the FP
  pin does not include prefix context version/nullifier/assignment fields.
  Ordinary V2 fixes these fields and its trace scheme canonically; the new
  factory refuses inherited-prefix contexts rather than trusting omitted
  fields from a capture. This is an explicit remaining consumer, not coverage.
- Kimi K3 lacks a grounded actual artifact/runtime contract; see
  [the separate gap record](rfc06-kimi-k3-runtime-gap-2026-10-18.md).
- Genesis `registrant=None` declarations, complete writer/resource coverage,
  physical allocation/cost/100k/pruned-P2P and consolidated real-node acceptance
  remain open. The public dormant activation refusal remains unchanged.
