# ADR-0169 — RFC8 uses the existing EXEC lane; the former algo-11 design is retired

Status: SUPERSEDED DIRECTION, 2026-10-08. Stable filename retained for existing links.

The former algo-11 design body has been deleted. Its previously reported test results, suite totals and
unrun cases remain in the [v0 test record](../design/palw/rfc-0008-v0-test-record.md). Those results describe the
`rfc8/claim-backed-blocks` branch of 2026-10-03/04; they do not establish current main's implementation or
validate the replacement EXEC design. No old fence is activated by this change.

[Revised RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) and
[EXEC integration spec v1](../design/palw/rfc-0008-implementation-spec.md) govern new RFC8 work:

- REAL anchors an ordinary or long useful-work claim under the current REAL qualification rules.
- EXEC_TX and EXEC_SLICE share the existing chain-independent EXEC class; neither becomes a selected parent,
  contributes raw/PALW fork-choice weight or advances the DAA/clock.
- TxPermit and WorkSlice authorization/accounting are separate. Slice work settles once at root claim Final;
  root-prefix and slice ranges cannot duplicate credit, rewards or execution-schedule credit.
- Current main's heartbeat, BASE-0, 120-second cadence and liveness structure remain the baseline.
- ADR-0173/RFC14 public prosecution, evidence retention and liability apply to every slice/boundary.
  Panel=0 still requires RFC14 and RFC15's separate completion/activation gates.
- New root/slice statements bind [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
  common post-commit policy. Each slice's evidence is fixed before its corresponding future source window;
  a seed exposed at root-open cannot test later freely chosen statements. Carriers never multiply beacon sources.

This is a design/documentation update. The replacement runtime has not been implemented or activated by it.
