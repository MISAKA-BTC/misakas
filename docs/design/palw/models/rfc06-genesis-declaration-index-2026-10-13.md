# RFC06/12 genesis declaration index — 2026-10-13

The launch genesis registers three native classes, including two with admission carriages. The existing genesis
transition installed their economic state but did not populate the node's class-carriage index (prefix 226).
`palw_class_carriages_for_sync_v1` enumerates current classes and serves that index, so a source node could publish
zero declarations even while holding both registered classes. This is a native consumer connection defect, not
evidence that a 100k population or pruned P2P transport has passed acceptance.

The repair stages only accepted `ClassRegistered` objects with `admission: Some` from the genesis object list.
It verifies the accepted artifact root/registration DAA, the profile-derived class ID and canonical job's profile ID,
then encodes that accepted carriage. The internal `PalwUtxoCommitV2` carries these records into the same RocksDB batch
as the native UTXO data/valid marker and PALW genesis delta/tip. All derivation completes before that batch begins.
The class-store writer does not publish uncommitted positive cache rows, and its write guard remains held through
the database commit. Its record layout, schema version, public parameters and consensus/wire roots are unchanged.
The binary-derived `BASE0` registration with `admission: None` acquires no fabricated declaration.

Three controls are added or extended: an abandoned class-index batch remains absent through fresh caches; rejected
duplicate-bond genesis leaves no class-carriage rows alongside its existing no-UTXO-valid/no-PALW-delta/tip checks;
and a fresh launch genesis must serve its two exact admission carriages through the actual sync API, decode them
to the accepted profile IDs and roots, and retain them in a new consensus instance with fresh caches. This is a
fresh-cache database recovery control, not an OS crash or independent-process P2P test.

Source/fixture work passed formatting and diff checks. Compilation/execution is pending. These changes were made
after the root's running consensus build (`94934`) entered its Rust compile phase; that binary must not be reported
as containing this repair. A later incremental build and terminal test results are required before assigning PASS.
Existing RFC06 tracked-cache hooks outside genesis are retained.
