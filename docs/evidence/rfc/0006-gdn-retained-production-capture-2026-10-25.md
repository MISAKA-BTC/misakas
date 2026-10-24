# RFC-0006 GDN production capture ownership — 2026-10-25

## Concrete consumer repair

The source-plan's former autonomous-dispatch gap had already been partly connected: the panel
service discovers own native claims, builds GDN evidence, calls the deterministic court, signs
only a guilty verdict and publishes through the court outbox. The remaining boundary repaired
here precedes those evidence owners. Both `gdn_step_filer_tick_v1` and the held filer's
`ServiceGdnHostV1::material` used the bare `gdn_accuser_view_v2` decoder/replay and returned an
unowned capture projection. The served-pool selector also decoded and copied opaque captures
before retaining those allocations.

The additive `gdn_accuser_view_retained_v3` admits the dense or folded Borsh decode before
construction, validates the binding and the registered backend's exact profile, and compares
execution/class identity against the own accepted claim view. Prompt IDs must match the binding.
The shared service selector preserves the accepted-block attempt job check; its free-prompt path
retains the canonical fixed-field payload decode and checks its derived executed context against
the capture. Foreign or malformed pool entries are skipped; resource failures withhold the
consumer explicitly and never become evidence of producer misconduct. Folded magic never falls
through to a dense decoder after malformed bytes.

The service owns its capture and prompt copies through the consumer handoff. The step and held
production consumers now require retained decoded material; the held host trait also carries
that owner through replay, evidence assembly, court checks and signing. There is no bare decoder
fallback in either production consumer. The original legacy backend API remains for historical
callers and fixtures; this change does not claim closure of all other backend methods.

## Folded replay admission

The folded accuser path forces dense materialization before it can probe step tiles. The normal
backend Producer resource quote chooses Fold retention on held classes and therefore cannot
stand in for this allocation. The new path derives the actual DenseTiles sink under the existing
materialization and class-ladder caps. It admits the decoded capture projections, registered
profile cache/checkpoint/trace/dense-capture terms, typed trace/chunk/vector descriptors, and
actual artifact parameter/weight inventory scratch before entering that replay. Arithmetic
uses checked sums; saturated components or unsupported geometry refuse as typed Resource.

The construction ticket remains alive while the existing replay and binding comparison run.
The resulting material acquires an independent exact retained ticket before the temporary
construction ticket is released. These are conservative logical construction requests, not
physical allocator/RSS closure, measured costs or approved production resource limits. The full
artifact scratch term can make capacity refusal conservative; it does not authorize a smaller
unbounded replay. Legacy map IDs/root interpretation and existing unsupported-map refusals stay
unchanged.

## Verification status

The actual base0 integration target
`cargo test -p misaka-palw-base0 --test gdn_capture_owner_v3` passed **3/3** on 2026-10-25
(0.08 seconds runtime), including two isolated child-process runs. The coordinated build took
8m01s including dependency compilation and shared-target lock waits; that duration is not a
model execution cost measurement. The runtime log is
`/tmp/misaka-rfc-gdn-capture-owner-tests-retry1-20261025.log`.

These tests use actual tiny Q36 dense and held/folded captures and explicitly assert their actual
Dense/Folded retention variants. A held recurrence finding reaches the existing deterministic
court. Controls check foreign execution/class/prompt, malformed bytes and real materialization
cap refusal. An isolated child tests zero-quota Resource refusal for both actual codecs; another
checks retention of the returned folded owner's quota until its drop. The child runs are part
of these three tests, not additional distinct cases.

The initial library compilation failed because the wrapper declared `Result<(), E>` while
`verify_binding_v1` returns three roots. The narrow `.map(|_| ())` correction preserves all
binding checks; the original error is retained in
`/tmp/misaka-rfc-body-owner-store-tests-retry1-20261025.log`. Earlier cancelled queued jobs are
not runtime passes. No independent Cargo job was started; the parent coordinates the shared
source-generation build.

The fresh actual node unit binary subsequently passed the focused changed-consumer controls:

- ServiceGdnHost: **1/1**, 3.00 seconds runtime,
  `/tmp/misaka-rfc-gdn-node-host-tests-20261025.log`. This checks retained capture material,
  own prompt/binding, foreign execution/prompt refusal and history-dependent withholding.
- Held court: **2/2**, 3.38 seconds,
  `/tmp/misaka-rfc-gdn-node-held-court-tests-20261025.log`. Actual small native folds void the
  forged claim and reduce producer collateral for answered conviction and silent default.
- Held tick: **2/2**, 3.40 seconds,
  `/tmp/misaka-rfc-gdn-node-held-tick-tests-20261025.log`. The retained host contract reaches
  demand, concrete accusation/court terminal, silent default and honest no-demand controls.

These are five node tests in addition to the three base0 tests; the isolated children above
remain included within the base0 count. The node build took 4m27s including dependencies and is
not an execution cost measurement. Node lib/tests/private metadata validation also passed in
33.69 seconds, recorded in `/tmp/misaka-rfc-native-owners-node-check-retry5-20261025.log`.

Two intermediate node compilations exposed callers that still attempted to mutate retained
owners. The production step filer tried to assign `owned.max_leaves` (E0594,
`/tmp/misaka-rfc-native-owners-node-check-retry3-20261025.log`); it now retains immutable owned
material and overrides only the borrowed view's scalar ladder with the fold's actual class
limit before probing. The ServiceGdnHost test tried to mutate its retained prompt (E0596,
`/tmp/misaka-rfc-native-owners-node-check-retry4-20261025.log`); it now replaces that fixture
with an independently retained foreign prompt. Neither correction adds unrestricted mutation
or changes a core API. Failed compilation attempts are not runtime passes.

These tests are small mechanism controls. Native multi-process production-court campaigns,
independent real sources, K3 weights/manifest/execution inputs, physical workspace/cost
calibration, 100k and pruned-P2P acceptance remain separate unexecuted requirements. Public
activation remains `None`, normal arming refusals remain, model tiers are unchanged, and matching
local replay never substitutes for selected-verifier claim approval.

Implementation source commit: `596d166dc` (native body/GDN/prospective routing consumers). This identifies the implemented source scope, not completion of RFC-0006 or a campaign gate.
