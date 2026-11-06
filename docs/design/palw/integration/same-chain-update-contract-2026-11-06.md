# Same-chain update contract (2026-11-06)

This contract is for a later update of one network. It does not change the current separation of testnet-11 and testnet-12. Carriage version stays 22. Any other version is refused by `palw_carriage_version_refusal_v1`, and a stored state is not migrated in place.

`PalwStateV2Store::reindex_if_stale` deletes derived rows, the tip and the pruning snapshot when the schema stamp disagrees, then writes the new stamp in the same synchronous batch. It does not convert the previous rows into the new format.

## Consensus rules

A new rule is activated at its fence. A past claim keeps the price and ruling recorded at its claim origin. A past block is replayed under the rules that block carried. The current tip is not substituted for either of those moments.

## Canonical state and carriage

A reader accepts carriage version 22, in the root order and tail tags that version already pins. An older carriage is refused. A compatible reader for a future version has to be added before that version is written; this document does not add one.

## Local database

Derived rows, the tip and the pruning snapshot may be deleted and rebuilt from the canonical carriage. The rebuild source is that carriage. If the process stops during the deletion, the schema stamp is still the old one because the stamp is written in the same batch as the deletion, so the next start runs the rebuild again.

## Runtime and artifact identity

An old tree id resolves only when the same manifest contract reproduces the same artifact bytes. An old id is not an alias for a different computation.

## Wire and public API

A peer or client that sends a form this build does not know is refused. The public RPC constructors stay. An unknown carriage version is the refusal above, not a best-effort decode.

## Recovery paths

Each later update names which of these it uses. A pruned node may not still hold the source it would rebuild from.

- old-data startup: open a version-22 carriage, or refuse any other version.
- snapshot import: import a snapshot whose carriage version is 22 and whose schema stamp matches this build.
- replay: replay past blocks under the rules those blocks carried.
- reorg: revert by the recorded delta, including the epoch and receipt-epoch variants bound to their own maps.
- pruned recovery: a pruned node uses a verified snapshot when the history it would rebuild from has been pruned.
- resume after interrupt: a rebuild that stopped before its batch committed runs again from the previous schema stamp.
