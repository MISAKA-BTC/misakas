# RFC-0007/0008 paid-trap checked base lookup — 2026-10-25

## Actual consumer boundary

The paid-trap reveal and check-fee consumers used the structural Option-only
`paid_claim_authority_for_base_unchecked_v2`. A present malformed base index or missing/malformed
primary authority became None. Actual reveal admission and folding could then select the legacy
mesh path; fee settlement could select the ordinary non-paid path. This is a fail-closed boundary
for damaged/local state, not evidence of a valid-chain payment exploit: normal native acceptance
and restore already constrain these rows.

The new retained checked lookup distinguishes exact base-index absence from corruption and
native Resource. It checks index version/kind/owner, primary context identity, authority and
enrollment versions/base identity, and agreement with the exact enrollment index. Both decoded
authorities retain their own reservations while compared. It neither reconstructs the ledger
nor grants selected approval. The base index has a fixed scalar schema, so its structural
preflight cannot expand attacker-controlled collection lengths; malformed framing is Decode,
while actual point-read/decode admission failures preserve Resource.

Actual paid-trap target selection, reveal folding, fee-hold selection, conviction recording and
the convicted-claim branch of held-fee release now use the checked lookup. Genuine absence
still selects the old non-paid behavior; the dormant fence returns before reading paid rows.
Historical Option-only accessors remain compatibility readers. The existing String-returning
reveal admission wrapper now fails closed through checked selection; this legacy acceptance
interface still stringifies errors, without parsing their text. Core fold and settlement paths
preserve `NativeResource` through the existing typed projection-error mapper.

Fee settlement preflights each matching check-fee target before any settlement write. The
existing fee-pattern helper inspects four events by index in constant time; the batch pass is
linear and allocates no cache/map. Conviction batches likewise preflight all targets before an
earlier conviction can write or charge a signer. Existing held-row malformed filtering,
receipt/lock projections and other liability consumers are separate scopes; this change does
not claim their complete closure. No policy, wire format, row tag or activation rule changes.

## Verification status

The actual core binary with the current production changes passed the existing paid-trap
regression group **14/14** in 0.04 seconds
(`/tmp/misaka-rfc-paid-trap-regression-tests-20261025.log`) and the existing paid-claim reader
group, excluding the new leaf, **4/4** in 0.63 seconds
(`/tmp/misaka-rfc-paid-trap-reader-regression-tests-20261025.log`). These existing regressions
used the same production source before the fixture-only correction below.

The corrected new filter passed **4/4** in 0.74 seconds, including its isolated quota child
(1 case, 0.06 seconds; included within the four, not an additional distinct test), in
`/tmp/misaka-rfc-paid-trap-checked-tests-retry1-20261025.log`. The coordinated retry build took
3m20s; that compilation duration is not an execution cost measurement. The parent coordinated
Cargo against the frozen shared source. No independent Cargo invocation was started. Static
Rust parser checks and scoped `git diff --check` also passed.

The first coordinated core build compiled successfully in 3m21s. Its new test filter passed
three cases and failed the isolated quota case
(`/tmp/misaka-rfc-paid-trap-checked-tests-20261025.log`): the fixture constructed its transition
builder after arming zero quota and hit the existing transition-construction capacity refusal
before reaching the intended paid-trap consumer. The fixture now admits its builder before
arming zero quota, then exercises the actual subsequent checked lookup/fold refusal. Production
construction admission is unchanged; no capacity bypass was added. The corrected runtime
retry passed as recorded above; the initial failed run is preserved as a failure and is not
counted as another four-case pass.

New core filter:
`palw_state_v2::tests::paid_claim_reader_v1::paid_trap_checked_v1` (four tests), using the existing
real native Program/paid-authority fixture rather than a partial authority prefix. Controls
cover exact retained authority/fee-hold and no ledger reconstruction; malformed index,
missing/malformed primary, wrong version/base/owner and missing enrollment index; actual
reveal admission/fold, fee settlement and conviction refusal without writes; later corrupt
conviction preventing an earlier partial write; genuine absence and dormant mesh equivalence;
and isolated zero-quota typed Resource before mutation. The isolated child is included within
its parent test count. Existing `paid_claim_trap_v1` and `paid_claim_reader_v1` filters remain
relevant regression groups.

These are small mechanism controls, not a four-adapter or multi-process campaign. K3 formal
inputs remain uncreated, campaign gates remain UNKNOWN, and whole-model physical workspace/
cost bounds are not established. Public activation remains None; normal bootstrap refusals,
model tiers and selected-verifier claim authentication remain unchanged.


## Convicted held-fee sweep continuation

A persisted held-fee row already identifies a paid obligation. The convicted-claim sweep formerly
accepted checked lookup's genuine base-index absence and treated it as `clean=false`, allowing
full fee release. It now requires a present authority in that branch: missing base index,
corruption and typed Resource refuse before any collected due row is deleted or paid. Generic
claim lookup absence and the inactive fence keep their prior semantics. Due-row discovery
already completes before release writes; this change preserves that order and adds no policy
or row/wire change. Other malformed held-row/receipt projections remain separate scope.

The coordinated full
`palw_state_v2::tests::paid_claim_reader_v1::paid_trap_checked_v1` filter passed **7/7** in 1.07
seconds, including the three additional actual-sweep tests and both isolated quota children
(each 1 case, 0.06 seconds; included within the seven, not additional distinct cases). Log:
`/tmp/misaka-rfc-paid-trap-held-sweep-tests-20261025.log`. The build took 3m19s; that compilation
duration is not an execution cost measurement. These tests use the real full paid authority, with explicit local terminal,
held-fee and receipt rows for sweep mechanics. They do not claim native court/receipt campaign
acceptance. Controls drive the actual sweep for Clean forfeiture versus incomplete-receipt
full release and repeat-once behavior, missing base-index refusal, a later corrupt due claim
preventing an earlier valid release, and an admitted-builder zero-quota Resource refusal.
Recipients use distinct payout payloads so a forfeited fee cannot be hidden in another total.
The isolated quota child is included in its parent test count.

The earlier four-case pass and 14/4 existing regressions above predate this production sweep
change. A concurrent consensus build used that older core library; it is not runtime acceptance
of this continuation. The final seven-case run above uses the frozen sweep source and supersedes
the earlier four-case filter for that source generation; these overlapping runs are not eleven
distinct tests. The older concurrent consensus run is not relabelled as sweep acceptance.
