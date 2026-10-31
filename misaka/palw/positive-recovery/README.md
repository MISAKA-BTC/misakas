# A6 Positive Court recovery

Node-facing checked recovery and synchronous RocksDB transactions. See
[implementation](../../../docs/design/palw/positive-court/A6-RECOVERY.md)
and [native connection contract](../../../docs/design/palw/positive-court/A6-NATIVE-CONNECTION.md).

The API persists real archive/head/clock/related-row/undo transactions. It does
not provide native UTXO funding authority, a new consensus/P2P wire format, or
credit authorization. Native projections must hold the existing exclusive
chain-state writer guard, validate current branch/generation/clock and prepare
row deltas without applying side effects before the single commit.

`prepare_record` and `prepare_clock` consume an old runtime. `commit_prepared`
checks a current native clock source before projecting checked facts. A failed
or ambiguous store operation never returns the old runtime as usable state.
Use `restore_and_commit` for startup publication; `restore_selected` is only a
read-only, rechecked preview. Only a bounded, fully replayed import token can
select an imported history. Pruning keeps the current archive and native rows.

Whole-history replay/encode is intentionally bounded and explicitly quoted.
The quote is not a native performance profile or an RSS/disk guarantee.
