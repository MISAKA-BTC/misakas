# Original Assignment funding-relay profile — 2026-11-03

Based on `7da72773c`, under the paired user adoption of RFC-0045/ADR-0213
and RFC-0046/ADR-0214. This stage preserves original selection DATA. It does
not establish actual native R/A/C acceptance, physical custody or payments.
All shipped Positive/Assignment/relay presets remain None.

## Source changes

The retained original Assignment material now contains a fixed, exhaustive
`AssignmentFundingRelayProfileV2`. Cooperative encodes as tag0 (one byte).
RequiresRelay encodes as tag1 followed by the original fired scope, relay
ruleset and complete finite policy/fee/physical preimages (433 bytes total).
Unknown tags, substituted preimages and inconsistent original fired scope are
rejected. The material identity binds the complete profile; rewriting it to
cooperative cannot preserve the original rooted material reference.

New-intake selection explicitly distinguishes absent/future/Never relay
configuration from an invalid armed relay configuration. The latter is an
error, rather than permission to downgrade. Native raw intake DATA retains this
selection. Continuing contracts must resolve their original authenticated
material, not call the new-intake selector again under later Params.

NativeHeap and NativeWire cover the added field and every fixed variant. No
r1 projection, old wire tag, aux0–20 meaning, public parameter value, network
preset or existing ruleset recipe changes in this stage. This unreleased
internal Open-material layout has not been used for successful native A intake.

## Validation

`original-profile-typecheck-first.json` passed contract/Core/Consensus library
and all test-target typechecking. `original-profile-library-clippy-second.json`
and `original-profile-tests-clippy-first.json` passed strict Clippy on the
current source. `original-profile-core-runtime-first.json` executed all 93 Core cases with zero
failures on the current frozen source. The artifact audit pins the exact source
map and all 11 executable hashes/profiles before the next source change. This
is this stage’s unique count; earlier stages and repeated runs are not added.
The all-target typecheck preceded one equivalent exhaustive-match rewrite in
the config helper; current strict Clippy and runtime followed that rewrite.
Documentation checks found zero new issues (43 retained baseline references).

## Remaining connections

Original-material continuation routing, actual fee allocation before coinbase,
R custody/current state slots, accepted raw framing/DB storage, F/R2 binding,
all-branch resources, original source/Remaining/Case factory, P charge/release,
positive and nonpositive accounting sessions, actual payment and restart/reorg/
IBD tests remain unfinished. This profile DATA does not replace those owners.
