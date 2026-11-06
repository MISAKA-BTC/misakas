# PALW holding lifetimes (2026-11-06)

Dropping a returned holding does not by itself release the artifact. `TirLineageV1` also keeps the loaded entry in its internal `held` set, and that entry owns an `Arc` of the artifact. The lineage can be shared by more than one SDK, so ending one SDK does not end the artifact either. Unload, when it is added, is a local stop. It is not expiry of a chain registration.

| Owner | What it holds | When it is released |
|---|---|---|
| catalog info | class id, profile, registered root | with the process; not a chain expiry |
| loaded artifact | bytes behind the lineage `held` entry and any returned holding | when every `Arc` owner is gone, including `held` |
| running job | worker slot and memory lease | after the job confirms stop or the drain finishes; a waiting cancel does not take this slot |
| retained evidence | court and claim material the node still serves | when the retention rule for that evidence ends |
| cache | local decoded rows | on schema rebuild or process exit |
| reservation | a source's connection or job share | when the owning guard drops, including unwind |

Waiting cancellation and stopping a job that has started are different contracts. A waiter that cancels before it is granted a slot leaves the queue and does not check a worker out. After `acquire_unless_cancelled` returns, the slot stays with the job until that job returns.
