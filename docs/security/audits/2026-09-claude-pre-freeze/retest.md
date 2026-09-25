# Retest — phase 8

**Status: not started.** Nothing on this page has been retested yet. A finding is not "retested"
until this page says so, with evidence, for its ID.

## Who retests

The retest runs in a **new session** that took no part in discovery or verification. It receives
only the [retest prompt](prompts/retest.md), the findings files, [remediation.md](remediation.md)
and a checkout of the fix commit. It does not receive the earlier sessions' transcripts. A session
that found a bug and then judges its own fix is influenced by its own earlier reasoning, so its
verdict does not count as a retest.

Once an independent human auditor is engaged, their retest replaces this one. It does not add to it.

## Record (fill in when the retest runs)

| field | value |
|---|---|
| Retest date | |
| Fix commit | |
| Reviewer | Claude (Anthropic), new session |
| Model identifier | |
| Session reference | |
| Retest prompt blob | `git hash-object docs/security/audits/2026-09-claude-pre-freeze/prompts/retest.md` = |

## Results

| ID | severity | remediation status (maintainers) | retest status | evidence |
|---|---|---|---|---|
| | | | | |

Retest statuses: FIXED · FIXED, NOT ARMED · PARTIALLY FIXED · NOT FIXED · REGRESSED · CANNOT RETEST.
New issues that the fix introduced are listed below as `MSK-26A-RT-<NN>`.

## New findings from the retest

None recorded yet.
