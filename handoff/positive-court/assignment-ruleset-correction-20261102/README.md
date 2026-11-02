# Correct Assignment rule scope without changing the r1 Result recipe — 2026-11-02

## Concrete integration defect found

`67dd5f28b` introduced a separate assignment-ruleset hash. Existing
NativeRowBody::Result::validate_shape requires the EXACT unchanged r1
native::ruleset_id(fired_fork, positive_policy). Therefore that candidate could
not enter the existing Result row. The previous nine passing policy/fence
cases did not test this integration and do not establish r1 row compatibility.
No native Assignment or new-scope Case was ever admitted; public presets were
None. The earlier commands/results remain untouched.

## Correction

The separate helper/domain is withdrawn rather than assigning its bytes a
new meaning. PositiveAssignmentFenceV2::ruleset_id now uses the existing r1
recipe. ResultRow, FundingPlanBasis, literal r1 Result/Service/consent hashes,
wire schemas and validation are unchanged.

Params::palw_positive_assignment_rule_scope_v2_at resolves a read-only scope
at the ACTUAL native admission DAA: clone Params, keep already-fired heights,
set future heights to never through the exhaustive existing visitor, normalize
future companion values, then hash the resulting complete params. This commits
the full active Assignment policy. The fork fingerprint is padded from its
32-byte identity to the existing 64-byte Digest convention; native block/tx
hashes are never truncated. The outer ruleset recipe remains r1. Persist this
original scope for the Assignment; do not recompute old contracts under later
rules. The resulting DATA is not accepted-prefix/funding/admission authority.

This is deliberately different from the advertised ForkIdV1 (which hashes a
height list only) and the genesis-only handshake identity. Neither alone
binds the full policy at an arbitrary native admission height. Neither public
fork-id encoding nor the existing consensus identity/schedule is redefined.

## Actual regressions

- Six shipped preset fingerprint triples still match the actual pre-fence
  baseline. Future/never selection and Positive-policy mismatch refusals pass.
- Changing the active Assignment policy changes the actual fired-rule scope
  even when the advertised fired-height digest stays identical.
- Adding a future NativeMining policy changes raw diagnostics but not the
  earlier fired scope; after its real height it participates in that scope.
- The new assigned scope passes the unchanged r1 Result row shape check.
  The exact retired domain recipe is reconstructed and rejected. Dummy
  signature/origin bytes in this shape fixture provide no native acceptance.

On one unchanged source map, all six contract and five Core fence/row cases
pass, along with strict Core/Contract library and selected test Clippy.
final-source-validation.json pins the records and actual test executables.
The Core build takes 10m01s and the five-case runtime 0.02s; these are not
native/LLM service measurements. Native writer, physical leases, row/history,
funds, Case/P/consumer/recovery and activation integration remain incomplete.
