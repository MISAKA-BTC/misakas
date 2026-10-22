# RFC-0012 empty observation regression

Base: `6a128eb65775c9cf1a26f975b7028c2d4a8b1187` (`origin/pre`).

The offline acceptance judge returned `OBSERVED` for `agreement([])`,
`finalized_history({})`, and `finalized_history({'A': []})`. These inputs
contain no process observation and cannot establish the predicates.

`require_real` now rejects an empty observation collection with `NotObservable`.
The history predicate also rejects a missing role map. Both become `BLOCKED`
through the existing judgement mapping. A regression covers an empty role
alongside a populated role as well as the three reproduced cases.

Validation on 2026-10-23:

```text
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc*.py'
Ran 188 tests
OK
```

`git diff --check` succeeds. The documentation checker initially found two
incorrect relative links in the remaining-work document at the base:
`rfc06-kimi-k3-runtime-gap-2026-10-18.md` and
`rfc06-native-routing-execution-context-2026-10-18.md`. Both documents exist one
directory above; the links now point there. The baseline is unchanged.

These are offline harness unit tests, not real-process acceptance evidence.
The 15-gate campaign, 100k, pruned IBD, OS crash, actual reward/coinbase,
provider replacement, model-wide cost boundaries, and historical signatures
remain unverified by this change. No UNKNOWN gate is promoted to PASS.
`PALW_T12_FULL_ACTIVATION_DAA` remains `None`; no preset, policy or source
economics input is changed.
