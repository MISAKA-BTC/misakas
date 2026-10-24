# RFC-0008 current-generation provider answer fence — 2026-10-24

## Scope and behavior

`DurableMonetaryMaterialProviderV1::answer_material_current_v4` previously omitted
its current generation lookup for the preferred consent when no kernel context
archive was configured. Retained legacy held bytes could therefore pass through
that current API without a current accepted generation. The preferred consent
now needs the same current root/consent/provider/exact-scope authority as every
fallback. The exact-current API keeps its requested consent.

The common generation gate rechecks that authority after response assembly,
while the returned answer owner still holds its allocation ticket. A failed
postcheck drops the owner and returns the error; no response escapes under that
stale generation. This brackets assembly, not a transaction with future network
submission or acceptance.

A failed initial authority lookup can still select another retained candidate.
Once current authority is positive, any replay, metadata, allocation or response
assembly error refuses the whole read instead of trying another consent. This
is a conservative availability change from the earlier broad error fallback.
It preserves the assembly error without parsing its text or mistaking a local
allocation refusal for an absent generation. The existing authority API itself
returns `String`: a resource refusal inside that initial authority lookup cannot
be distinguished from other authority failures and still participates in the
preexisting candidate search. This change does not resolve that API limitation.

The historical `answer_material_owned_v1` behavior and public `String` signatures
are unchanged. There are no core, wire, policy, activation or economics changes.

## Evidence

The new `monetary_material_current_tests.rs` exercises the production common gate
with a real typed kernel provider store admitted through the public provider
path, persisted under two consents, then cold reopened. Current native authority
outcomes are injected into the same gate; kernel context replay and an actual
native reorg are not simulated as accepted chain evidence. The fixture is not
K3, a multi-host provider campaign or proof of current chain acceptance.

- No current authority: both preferred and fallback are checked and response
  construction is never called; the legacy owned reader remains usable.
- Preferred rejected, fallback current: fallback must pass its own checks before
  and after actual retained-store response construction. Exact read cannot use it.
- Authority changes during assembly: the postcheck rejects the actual response
  owner, does not try another consent, and releases its native reservation.
- An isolated child process enables a real logical native capacity limit after
  opening the store. A live response owner prevents a full-capacity reservation;
  postcheck rejection releases it. A full-capacity competing lease causes actual
  store answer allocation to fail; the error is preserved without fallback.

`cargo test -p misaka-palw-remote --lib current_generation_tests -- --nocapture`
passed all 3 cases (plus the isolated child case); compilation took 18.23 seconds.
The resulting library test binary's complete `monetary_material` group passed
14 cases in 0.38 seconds, including existing replacement observation, generation
retention, finality/provenance and cold-GC tests.

The existing core test binary also passed the unchanged
`provider_generation_follows_its_own_replacements_cold_route_and_rollback` and
`provider_capacity_refusal_is_atomic_and_gc_requires_exact_finalized_native_generation`
regressions. The core binary was reused, not rebuilt for this remote-only change.

Local run logs are `/tmp/misaka-rfc08-current-generation-tests-20261024.log`,
`/tmp/misaka-rfc08-current-generation-all-tests-20261024.log`,
`/tmp/misaka-rfc08-existing-generation-test-20261024.log` and
`/tmp/misaka-rfc08-existing-capacity-test-20261024.log`.
