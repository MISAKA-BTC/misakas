# RFC-0012: reserve settlement across actual native branches and cold readers

Status: coordinated native control PASS. No campaign gate is
promoted. K3 inputs remain uncreated. Public activation and monetary policy are
unchanged.

The existing native `pos_v2_reserve_drip_pays_finalized_epoch` fixture already
builds two actual DNS bonds, a signed honest attestation, genuine signed
conflicting attestations, their accepted funded slashing evidence, the reserve
accrual and actual coinbase drip. Its full body now lives in a shared test leaf;
the existing public filter still runs that same native flow. The new mode uses
a real on-disk DB and also independently checks the reserve share as integer
`principal * 4000 / 10000`, instead of trusting the production split helper alone.

The new filter is
`rfc12_reserve_drip_reorg_and_cold_restart_conserve_actual_coinbase_utxos`.
It observes the actual original drip output through the bounded acceptance-read
API, closes all native DB handles, and starts a separate process to reopen and
compare every retained per-block reserve value and every actual virtual UTXO.
The writer independently reopens and compares the same snapshot.

An independent native node replays the exact accepted common prefix before the
drip, then builds a heavier alternative branch with distinct real miner data.
Both branches use the same existing private DNS fixture config. Native template
and validation construct the alternative coinbase; no balances, UTXOs, frontier,
accepted transaction IDs or bond history are planted. Importing this branch must
actually change the selected sink. The test requires that the original and
alternative drip outpoints together represent exactly one spendable reserve
payment, then repeats the cold reader/writer handoff and checks later actual
coinbases do not pay that obligation again.

The fixture's inherited small DNS epochs, skip-PoW and simulated timestamps are
explicit. The cap is inherited `u64::MAX`, participation gets the entire validator
pool and the honest validator is sole included peer for its epoch. It isolates
this reserve leg; it does not measure a production rate limit, all reward legs,
physical resource quota, retirement-fence campaign or full independent live-node
accrual/drip trace. `rfc12_retirement_reconciliation.reserve_balance` remains
NotObservable until such a trace exists. This fixture is no K3/useful-work,
OS-crash, actual pruning/IBD or EVM runtime evidence.

The first coordinated test compile stopped at two fixture-only API type errors:
`TransactionOutpoint` has no `Ord` implementation, and `ConnBuilder` takes a
`PathBuf`, not the environment's `OsString`. Sorting now explicitly uses the
transaction ID and output index; the DB path conversion is explicit. No native
rule changed. Failed log:
`/tmp/misaka-rfc-reserve-drip-reorg-tests-20261025.log`.

The next build passed (4m03s), and the first real cold reader passed (0.07s),
but the parent then stopped at an over-strong fixture timing assertion:
while delivering the not-yet-heavier alternative branch, its first body still
had `StatusUTXOPendingVerification`. Native validation reaches that branch only
when the sink walk selects it. The fixture now requires every alternative body
to be `UTXOValid` after the actual heavier sink is selected. It does not accept
pending bodies as final success. Unwinding also exposed a test DB destruction
order bug after reopening replaced the tracked Arc; the guard is now retained
in the caller until all callee DB owners have closed, including unwind. The
failed run/SIGABRT log remains
`/tmp/misaka-rfc-reserve-drip-reorg-tests-retry1-20261025.log`.


A subsequent compile (`...tests-retry2-20261025.log`) caught one leftover tuple
pattern after the ordered `PersistentDb` wrapper was introduced. The explicit
field access was corrected; this compile failure is no runtime evidence.

The final coordinated build passes in **4m00s**. The new native parent passes
**1/1, 2.34s**, including two different-process cold readers **0.06/0.02s**:
`/tmp/misaka-rfc-reserve-drip-reorg-tests-retry3-20261025.log`.
The actual live drip sum after native reorg is **91,836,150 sompi**, exactly
matching this fixture's independently computed accrual. The old outpoint is
absent and the alternative outpoint holds that amount; later real coinbases
carry no additional payment to this sole included validator.

The final consensus binary was copied before the long-pruning attempt to
`/tmp/misaka-rfc08-typed-pruning-aa5825416-run1-20261025`, SHA-256
`83a1b5cc8d1ad898b5e201d1a69958ded60de20d9aa8ad0687d9914b4216d6d9`.
Against that same executable, existing native controls pass separately:

- Original reserve-drip flow: **1/1, 1.32s**,
  `/tmp/misaka-rfc-reserve-drip-existing-regression-20261025.log`.
- Accepted slashing evidence through a native branch reorg: **1/1, 0.65s**,
  `/tmp/misaka-rfc-reserve-slash-reorg-regression-20261025.log`.
- Two accepted slashes in one block: **1/1, 0.65s**,
  `/tmp/misaka-rfc-reserve-multi-slash-regression-20261025.log`.

The original and new modes share the actual accrual/drip setup; their common
coverage is not counted as two independent implementations. These results do
not establish a live retirement-campaign trace or all rewards across all models.
