# Methodology — 2026-09 Claude-assisted pre-freeze security review

## 1. What this review is, and what it is not

This is an **AI-assisted security review**: the discovery, verification and write-up were done by
Claude (Anthropic) agents, run by the MISAKA maintainers against one fixed commit
([scope.md](scope.md)). It is a **pre-audit**. Its purpose is to find and remove Critical and High
issues before the code goes to an independent human audit.

It is **not** an independent third-party audit, and it must not be called one. An independent
external audit needs a reviewer outside the project who owns the scope, the execution, the
judgement and the signature. None of that is true here. The project ran the tool, chose the scope,
and publishes the result.

It is **not a proof of security**. An empty section means the agents found nothing they could
support with evidence. It does not mean that nothing is there. [README.md](README.md) §Limitations
lists what this method is known to miss.

## 2. Phases

| phase | what happens | output | status in this document set |
|---|---|---|---|
| 0 | Fix the audit commit and the identity it runs | [scope.md](scope.md) | done |
| 1 | Architecture and threat-model review | [threat-model-review.md](threat-model-review.md) | done |
| 2 | Consensus discovery | [consensus-findings.md](consensus-findings.md) | done |
| 3 | PALW and economics discovery | [palw-findings.md](palw-findings.md) | done |
| 4 | Cryptography and PQ discovery | [crypto-findings.md](crypto-findings.md) | done |
| 5 | P2P, RPC and operator discovery | [network-findings.md](network-findings.md) | done |
| 6 | Release and supply-chain discovery | [release-findings.md](release-findings.md) | done |
| 6a | Prior-audit closure check | [prior-audit-closure.md](prior-audit-closure.md) | done |
| 6b | Independent verification of every candidate finding | status column of each findings file, [poc/](poc/) | done |
| 7 | Maintainers fix, each fix with a regression test | [remediation.md](remediation.md) | **not started** |
| 8 | Retest in a **new** session that did not take part in phases 1–6b | [retest.md](retest.md) | **not started** |
| 9 | Freeze the final result | `final-report.md` | **not started** |

Phases 7–9 are deliberately not done in the session that found the issues. A session that found a
bug, fixed it and then checks its own fix is influenced by its own earlier judgement. The retest
must be done by a session that sees only the finding records, the fix commits and the
[retest prompt](prompts/retest.md).

## 3. Independence structure

Agents inside one review share a model, so they can share blind spots. The structure below is meant
to stop one agent's *conclusion* from becoming another agent's *premise*:

1. **Discovery units are blind to each other.** The code was cut into 24 units
   ([prompts/units.md](prompts/units.md)). Each unit was reviewed by one fresh agent that received
   the [base prompt](prompts/discovery-base.md), the
   [session context and one domain addendum](prompts/domain-addenda.md), and its own file list. It
   could not see any other unit's work.
2. **Domains ran as separate workflows.** Group A (Consensus and Crypto), group B (PALW and
   economics) and group C (network, RPC, operator, release, threat model, prior-audit closure) ran
   as three workflows in parallel, with no shared state beyond the read-only repository.
3. **Gap units.** After each group, a completeness critic received each unit's scope statement and
   only the **titles** of its findings. It looked at the repository and proposed up to three
   unreviewed high-risk areas. Those were reviewed by fresh agents in the same way as the other units.
4. **Verification is done by agents that did not do discovery.** Each candidate finding was handed
   to new agents as a **claim** only: a file holding the title, affected code, the invariant, the
   preconditions, the attack sequence, the expected and actual behaviour, the claimed impact and
   the claimed reachability. They did **not** receive the finder's evidence, proof of concept,
   severity, confidence or status, and were told not to read any other review material on the
   machine. They had to reproduce the finding from the code themselves, or refute it.
   Verification of a group started as soon as that group's discovery ended. Each later group's
   candidates were de-duplicated against the clusters already formed, so a root cause found in two
   groups was verified once.
5. **Retest (phase 8) is a different session.** See [retest.md](retest.md).

## 4. Evidence rules

These are in the base prompt, and the verifiers apply them again:

- Implementation and executable tests are authoritative. ADRs, specifications, READMEs, earlier
  audit reports, release notes and comments are statements of *intended* behaviour, and are never
  accepted as evidence that the code does something.
- An earlier audit finding counts as closed only when the fix is in the code at the audit commit
  **and** a regression test asserts it **and** the fix is armed on the network in question. See
  [prior-audit-closure.md](prior-audit-closure.md).
- **Reachability is graded at runtime** against the shipped parameter constructors
  (`palw_t12_shipped_params()`, `mainnet_shipped_params()`), never against preset constants or
  `#[cfg(test)]` fixtures. An earlier internal audit filed a false CRITICAL by reading a preset
  constant that the shipped constructor overrides.
- **Critical or High needs proof.** The finding must give a concrete code path and either a
  reproducible test or proof of concept, or a rigorous code-level argument showing why reproduction
  is impractical. Without one of these it is recorded as UNCONFIRMED, not as Critical or High.
- Discovery does not modify production code. Proofs of concept are new test files, kept in
  [poc/](poc/). They are not added to crate test suites. Adding regression tests is the maintainers'
  job in phase 7.

## 5. Status

| status | meaning | how it is assigned |
|---|---|---|
| **CONFIRMED** | The behaviour was demonstrated at the audit commit | A verifier reproduced it with a proof of concept that ran, or wrote out a rigorous code-level argument, and no verifier refuted it with evidence |
| **UNCONFIRMED** | Plausible from the code, but not demonstrated | Neither confirmed nor refuted. This includes every Critical or High claim that could not be proven |
| **FALSE POSITIVE** | The claim is wrong | A verifier showed concrete evidence against it: a guard the finder missed, an unreachable precondition on the shipped parameters, or a misread of the code |
| **ACCEPTED BY DESIGN** | The behaviour is intended | A document explicitly accepts it (e.g. [SECURITY.md](../../../../SECURITY.md) §1–2), **and** the code does exactly what that document says |

A finding with the same root cause as a publicly known issue ([scope.md](scope.md) §5) carries a
**Known issue** reference in addition to its status.

## 6. Severity

Severity is graded by **impact, if the finding is reached**. Reachability is recorded separately
as *testnet-12 as shipped*, *mainnet preset* or *dormant / tests only*. A Critical that only a
dormant fence reaches is still Critical, and is labelled dormant, because arming the fence would
ship it.

| severity | impact |
|---|---|
| **Critical** | Consensus split between honest nodes; acceptance of an invalid block or state transition; unauthorised issuance, theft or permanent loss of funds or bonds; network-wide halt. An attacker can reach it without privileged access or collusion beyond what the protocol assumes it tolerates |
| **High** | The same classes of impact, but needing significant preconditions (colluding stake or seats, a narrow timing window, a specific class or store) or with bounded loss; a remote crash of any node by an unauthenticated peer or RPC client; slashing of an honest party |
| **Medium** | A bounded accounting error; a denial of service that needs sustained attacker resources; griefing of honest participants; a documented security control that is not enforced, where exploitation needs local access |
| **Low** | Defence in depth; an edge case that is hard to exploit or has low impact |
| **Informational** | No direct security impact. Recorded because a later change could make it one |

**Confidence** (High / Medium / Low) is the verifiers' confidence that the status and severity are
correct. It is not the likelihood of exploitation.

## 7. Verification protocol (phase 6b)

Every candidate from discovery was de-duplicated across units: findings with the same root cause
were merged, and the merged record keeps every unit that found it. Each candidate then went to
verifiers who had not taken part in discovery:

| claimed severity | verifiers |
|---|---|
| Critical, High | a **reproducer** (writes and runs a proof of concept) **and** a **refuter** (tries to show the claim is unreachable on the shipped parameters, guarded elsewhere, or misread). If they disagree, a **judge** reads both reports and the code, and decides |
| Medium | a reproducer, who must also try to refute |
| Low, Informational | one verifier |

The final severity is the verifiers' own assessment, not the finder's. When a finder's severity
and the final severity differ, the findings file shows both.

## 8. Proofs of concept

Each proof of concept in [poc/](poc/) is a Rust integration test that applies to the audit commit:

```sh
git checkout 3d2bd6dc5d77d37396d1b73c4c13526090923b92
cp docs/security/audits/2026-09-claude-pre-freeze/poc/<ID>.rs <crate>/tests/audit_poc_<id>.rs
cargo test -p <crate> --test audit_poc_<id> -- --nocapture
```

The header comment of each file names the crate, the command and the expected output. A proof of
concept passes when it **demonstrates the vulnerable behaviour**. After a fix it should fail, or be
inverted into the regression test.

## 9. Finding IDs

`MSK-26A-<DOMAIN>-<NN>`: `26A` is this review (2026, first). `<DOMAIN>` is `CONS`, `PALW`, `CRYP`,
`NET`, `REL` or `ARCH`. IDs are assigned in the order that de-duplication produced the clusters.
The number says nothing about severity. IDs are never reused. A FALSE POSITIVE keeps its ID, so a
later review can see that the claim was already examined.

## 10. GitHub code scanning (SARIF)

[findings.sarif](findings.sarif) contains **only** findings that are CONFIRMED, have a file and
line, and were reproduced by a proof of concept or a written code-level proof. No UNCONFIRMED,
FALSE POSITIVE or architectural item is exported. The file is SARIF 2.1.0 and can be uploaded with
`github/codeql-action/upload-sarif`.
