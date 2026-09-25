# Session context and domain addenda

Every discovery agent's prompt was, in this order: the [base prompt](discovery-base.md), the
session context below, the addendum for its domain, and its unit (files and focus, listed in
[units.md](units.md)). These are the exact strings sent, extracted from the workflow script.

---

## Session context from the audit coordinator (applies to every unit)

**Audit target.** Commit 3d2bd6dc5d77d37396d1b73c4c13526090923b92 (main, 2026-09-26 00:52 +0900). The working tree is /home/user/misakas. The branch may carry extra commits that only add files under docs/security/audits/ — those are the audit's own output, not audited code. Before starting, run:
  git rev-parse HEAD
  git diff --stat 3d2bd6dc5d77d37396d1b73c4c13526090923b92 HEAD -- . ':(exclude)docs/security/audits'
The second command must print nothing. Record both results in commands_executed. If it prints anything, stop and report that in limitations.
Rust sources, Cargo.toml and Cargo.lock at this commit are byte-identical to the testnet-12 release commit 0e8ec984e.

**Reachability is graded at runtime, against the shipped parameter sets.** Read fences and values from the constructors in consensus/core/src/config/params.rs — palw_t12_shipped_params() (testnet-12, the live network; palw_t12_arm_every_rule_from_genesis arms every rule at DAA 0 except bond maturity at DAA 1,000 and six dormant rules: palw_inactivity_leak, palw_frontier_provenance, palw_beacon_fold, palw_shard_licensing, palw_fp_decode_rules, palw_fp_decode_constraint) and mainnet_shipped_params() (mainnet preset, not final). A prior internal audit filed a false CRITICAL by reading a preset constant that the shipped constructor overrides, and others were filed from #[cfg(test)] fixtures. For every finding, state in 'reachability' whether it is reachable on testnet-12 as shipped, on the mainnet preset, or only in tests / dormant code — and how you established that.

**Publicly known issues (disclosed in docs/t12-launch-2026-09-25.md §2 before this review).** (1) heartbeat-transparency double spend within merge_depth by an unbonded heartbeat miner [CRITICAL, fix WIP]; (2) panel-seed grinding: the panel seed is the anchor block's identifier, so the anchor producer can redraw the panel [CRITICAL, fence WIP]; (3) DNS finality in Bootstrap (stake reorg gate not in force); (4) post-Final Valid locks filling a seat's 500‰ budget; (5) V04 licences held back by claim-id order; (6) V01/V03/V05/V07 readiness/carrier recovery; (7) scheduled: registry resilience, bridge audit BR-1..BR-7, position fixes (sink binding, quoteSell gross). Do not spend effort re-deriving these. If a finding has the same root cause, set known_issue_ref to e.g. "launch note §2.2" and keep it short. If you show a known issue is broader or more severe than disclosed, or that its planned fix is insufficient, that IS a finding.

**Building and running tests.** The machine has 4 cores and a shared cargo target dir; other reviewers run concurrently. Test binaries for kaspa-consensus-core may already be built. Rules:
- Prefer static analysis. Run cargo only when it materially changes a conclusion, and only targeted: e.g. cargo test -p kaspa-consensus-core --test <file_stem>  or  cargo test -p kaspa-consensus-core --lib <filter>. Never build the whole workspace, never run cargo clean / cargo update, never change Cargo.toml or Cargo.lock.
- If cargo waits on the build-directory lock for more than ~10 minutes, or a build would clearly take longer than ~25 minutes, stop and continue statically; say so in limitations.
- To test a hypothesis you MAY create a temporary NEW integration-test file named <crate_dir>/tests/zz_audit_<unit_id_lowercase>_<n>.rs, run it with --test, then DELETE it before you finish. Put its full source and the relevant output into the finding's evidence field. Never edit any existing file in the repository. Never write under docs/security/audits/.
- Before returning, run git status --porcelain and make sure you left no files behind.

**Big files.** consensus/core/src/palw_state_v2.rs is ~64k lines and config/params.rs ~26k lines. Use grep / rg to find the functions you need and read those regions; do not try to read them end to end.

**Independence.** You are one of several reviewers. You cannot see other reviewers' work and they cannot see yours. Stay centred on your unit, but follow a call path wherever it leads.

**Output discipline.**
- At most 10 findings. Merge instances of one root cause into one finding. Order by severity.
- A finding is a violated security invariant with a concrete code path. Hardening ideas, style, missing docs and design-level worries go into architectural_concerns, not findings.
- status: use CONFIRMED only if you actually demonstrated it (a test you ran, or a rigorous code-level argument you wrote out in evidence); ACCEPTED BY DESIGN only if a document explicitly accepts the behaviour AND the code matches that document; otherwise UNCONFIRMED. A Critical/High you could not demonstrate must be UNCONFIRMED. (FALSE POSITIVE is assigned by the verifiers, not by you.)
- Give file paths relative to the repository root, and the line numbers at this commit.
- The id field of a finding is a placeholder you set to <unit_id>-<n>; the coordinator assigns final IDs.
- If your unit finds nothing, return an empty findings list and say precisely what you checked. Do not declare the system secure.

---

## Domain addendum: Consensus
Focus on: determinism; state transition; activation boundaries; fork conditions; reorg behaviour; serialization / canonicalization; integer overflow / rounding; duplicate receipt / double execution; replay; genesis / parameter pinning; unknown version handling; old-node / new-node interaction. The question for every rule is: can two honest nodes that see the same blocks reach different state, or can a block that violates the rule be accepted (or a valid one rejected) by part of the network?

## Domain addendum: PALW and economics
Focus on: job uniqueness; receipt uniqueness; double payment; court state machine; bond accounting; slashing; model registry; position / store accounting; gas / metering; held context; prompt canonicalization; model output commitment; malicious producer; malicious panel; malicious model owner. For every payment path ask: can the same work, receipt or claim be paid twice, paid without being verified, or paid more than the rule allows? For every state machine ask: is there a transition the rule does not intend (skipped window, re-entered state, a Final that can be re-opened or a slash that can be avoided)? Supply must be conserved: issuance only where the schedule says, burns and slashes debited exactly once.

## Domain addendum: Cryptography and post-quantum
Focus on: signature verification bypass (length / encoding checks, empty keys, wrong algorithm id, batch or cached verification); domain separation between every signed or hashed object type (a signature or hash valid for one object must not be valid for another; contexts, prefixes, length framing); ambiguous encodings that let two different objects hash the same; randomness and seeds used in consensus (grinding, bias, withholding, local RNG or clock reaching consensus); key derivation and address hashing. Treat the underlying primitives (ML-DSA-87, BLAKE2b, SHA3) as correct and list their proof as an area for human review; look at how MISAKA uses them.

## Domain addendum: P2P, RPC and operator
Focus on: remotely triggerable panic / crash (unwrap, expect, indexing, arithmetic overflow in debug-vs-release, unbounded recursion) on data from peers or RPC clients; unbounded allocation or CPU from attacker-sized inputs; messages or RPC calls that reach consensus or wallet state without the checks the P2P path applies; authentication and default bind addresses of every listener; ban / rate-limit logic; handshake and network-isolation checks; secret handling (key files, permissions, logs, env); validator / signer privilege boundaries; node-duty bugs that could make an honest operator's node commit a slashable offence.

## Domain addendum: Release and supply chain
Focus on: whether a released binary can be tied to this commit (pinning of toolchain, actions, containers, git dependencies, build scripts that fetch at build time); CI workflows that run untrusted code with secrets (pull_request_target, workflow_run, write tokens); artifact signing and verification gaps; whether the release identity check (release.json vs what kaspad prints) can pass while the binary runs a different ruleset; dependency advisories policy (deny.toml exceptions); anything that lets two builds of the same commit differ.

## Domain addendum: Architecture and threat-model review
You are not primarily hunting bugs. You check the project's own threat model and security claims against the code, and map what is missing. Read docs/palw-rc-threat-model.md, docs/architecture/overview.md, SECURITY.md, docs/palw-registry-map.md and the ADRs they cite as governing. For each stated trust boundary or security claim, find where (if anywhere) the code enforces it. Then list attacker classes and assets from the adversarial assumptions above that the threat model does not address (docs/mainnet-readiness.md already notes there is no network/RPC threat model). Anything you find that is a concrete vulnerability goes into findings with the same evidence rules.

## Domain addendum: Prior-audit closure verification
Enumerate EVERY finding graded Critical or High (or CRITICAL/HIGH, or the Japanese 重大/高) in the documents assigned to you. For each: locate the fix in code at this commit; locate the regression test and read whether it actually asserts the fixed behaviour (not a tautology, not a #[cfg(test)] fixture that bypasses the shipped params); run the regression test if it is cheap (targeted cargo test); and determine from palw_t12_shipped_params() / mainnet_shipped_params() whether the fix is ARMED on testnet-12 and on the mainnet preset, or implemented behind a dormant fence. Documentation that says "fixed" is a claim, not evidence. If a fix is missing, regressed, not armed, or its test does not test it, also file a finding for it.
