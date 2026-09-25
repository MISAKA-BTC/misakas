# Retest prompt (phase 8)

Give this prompt to a **new** Claude session that took no part in phases 1–6b of this review.
Give it only: this prompt, the findings files, [remediation.md](../remediation.md) with the fix
commits filled in, and a checkout of the fix commit. Do not give it the transcripts of the
discovery or verification sessions, and do not tell it which findings you expect to be fixed.

Record in [retest.md](../retest.md): the date, the model, the session reference, the fix commit,
and this prompt's git blob hash (`git hash-object docs/security/audits/2026-09-claude-pre-freeze/prompts/retest.md`).

---

You are retesting the remediation of an AI-assisted security review of a production-candidate
Layer-1 blockchain implementation.

Repository: MISAKA-BTC/misakas

Original audit commit: 3d2bd6dc5d77d37396d1b73c4c13526090923b92

Fix commit to retest: <FILL IN: full SHA>

This is an AI-assisted retest, not a proof of security. You did not take part in the original
review. Do not assume that the original findings, their severities or their proofs of concept are
correct, and do not assume that the fixes work.

## Inputs

- `docs/security/audits/2026-09-claude-pre-freeze/*-findings.md`: the findings as they stood at the
  original audit commit.
- `docs/security/audits/2026-09-claude-pre-freeze/remediation.md`: for each finding, the fix commit
  or commits, the regression test and the maintainers' claimed status.
- `docs/security/audits/2026-09-claude-pre-freeze/poc/`: the proofs of concept, written against the
  original commit.

## Evidence rules

Implementation and executable tests are authoritative. The remediation table, commit messages,
ADRs and comments are claims, not evidence.

A finding counts as FIXED only when all of these hold:

1. The proof of concept, or an equivalent test you write, demonstrates the behaviour at the original
   commit and no longer demonstrates it at the fix commit. Run both.
2. The regression test named in the remediation table exists at the fix commit, passes, and would
   fail if the fix were reverted. Check this by reverting the fix locally, not by reading the test.
3. The fix is **armed** on the network it is claimed for. Read it at runtime from
   `palw_t12_shipped_params()` / `mainnet_shipped_params()` (or their successors), not from a
   preset constant. A fix behind a dormant fence is "FIXED, NOT ARMED".
4. The fix did not move the consensus identity silently. If the consensus params fingerprint
   changed, the change must be pinned in-tree and announced as a fence.

Check the whole class of the bug, not only the reported instance. If the fix covers one call site
and a sibling call site has the same flaw, the finding is PARTIALLY FIXED and the sibling is a new
finding.

## Also check

- **Regressions.** Review the diff between the original audit commit and the fix commit for new
  vulnerabilities, with the same evidence rules and the same severity rubric as
  `methodology.md` §6. Report any new issue as a new finding with ID `MSK-26A-RT-<NN>`.
- **UNCONFIRMED findings.** Try once more to confirm or refute each one at the fix commit.
- **Findings marked ACCEPTED BY DESIGN or WONT FIX.** Check that the stated rationale matches the
  code.

## Output

For every finding ID, give one status:

- FIXED
- FIXED, NOT ARMED
- PARTIALLY FIXED
- NOT FIXED
- REGRESSED
- CANNOT RETEST (say exactly why)

Give the evidence behind each status: the commands you ran, with their output, the proof of concept
result at both commits, and the regression-test revert check.

Then give:

1. the scope you actually retested
2. what you did not retest
3. every command and test you ran
4. failed or skipped tests
5. new findings, by severity
6. the limits of this retest

Do not declare the system secure because every listed finding is fixed.
