# misaka-palw-bridge — node-side desktop-coordinator bridge

The server half of the palw-gateway coordinator protocol v1 (see the gateway repo,
`palw-gateway/README.md` "PALW coordinator protocol"): the daemon a
`palw-gateway --palw-coordinator http://host:26621/palw/v1` points at. It coordinates
A-commits and B replicas across REAL distinct providers and decides matches with the
node's own k=2 predicate.

## What makes it the *node-side* bridge (vs the gateway's dev loopback)

1. **The match is the real one.** Replica matching builds two
   `misaka_palw::palw::ReplicaMatchKey`s and runs
   `misaka_palw::palw_replica::run_replica_k2` — the same eight-field exact-match
   predicate (design §7.5) the consensus lane mints leaves with, using the same
   domain-separated constructors (`job_set_commitment`, `gemm_trace_root`,
   `operation_schedule_commitment`, `MIL_PALW_*` domains). The qi35-serve class maps:
   - `job_set_commitment` ← class label ‖ job_id ‖ max_new ‖ prompt ids (LE)
   - `output_commitment` ← `Hash64_k(output-domain, decoded output_root)` (equality-preserving
     re-keying of the gateway's output-ids root; byte-parity with a consensus leaf's
     `output_commitment(salt, ids)` needs the beacon salt — a consensus seam)
   - `canonical_gemm_trace_root` ← the engine's ROUTE root (the class's canonical
     execution-trace commitment: MoE routing decisions)
   - `operation_schedule_commitment` ← KV root ‖ recurrent-STATE root
   So two replicas match only if they agree on the output AND the execution structure.
2. **Independence is enforced at the protocol layer.** A job is never offered to its own
   submitter. There is no `allow_self_replica` here and no flag to add one.
3. **Class strictness.** Submissions and results without `runtime_roots` are rejected up
   front (the gateway captures them from the engine's `ROOTS route= kv= state=` line,
   emitted under `QI35_SERVE_ROOTS=1`).
4. **Durable, tamper-evident state.** Every mutation is one JSONL event with
   `root = Hash64_k(journal-domain, prev_root ‖ event_json)`. Boot replays and verifies;
   an altered line refuses to load; a torn final line (crash mid-append) is truncated
   away. The head root is the audit digest a future consensus seam anchors on-chain.

## Protocol (under `/palw/v1`)

```
POST /palw/v1/jobs                        JobSubmission     → {accepted:true}   (idempotent)
POST /palw/v1/verdicts                    {job_ids:[…]}     → {verdicts:[{job_id,verdict}]}
GET  /palw/v1/assignments?provider_id=X                     → {assignments:[…]} (claim-on-fetch)
POST /palw/v1/assignments/{job}/decline   {provider_id,reason} → {declined:true}
POST /palw/v1/replica-results             ReplicaResult     → {recorded:true, matched:bool}
GET  /palw/v1/status                                        → journal head/seq, phases, providers
GET  /health
```

Verdict semantics: `replica_matched` is delivered exactly once (the delivery itself is a
journal event), then `certified`. A `mismatch` is an UNRESOLVED DISPUTE between two
replicas — the consensus lane resolves disputes with sampled audits; this bridge only
surfaces them. A replica result missing roots, late, or from a non-holder never brands
the job: the claim lapses/requeues instead.

## Run

```
misaka-palw-bridge --listen 127.0.0.1:26621 --data-dir /var/lib/palw-bridge \
  [--auth-token T] [--assignment-deadline-ms 120000]
```

## Verified end-to-end (2026-07-31, one M-series host)

Two palw-gateway instances with two REAL qi35 35B engine processes:
prov-a answered a `palw_mint` chat turn (7.3 s) and committed output + route/kv/state
roots → this bridge offered the job only to prov-b → prov-b's own engine ran a full
replay → both keys agreed field-for-field → `run_replica_k2` = Matched → verdicts flowed
back → prov-a's turn reached `certified`, journal seq 4, and a bridge restart replayed
to the identical head root. The independent-process root agreement also demonstrates the
engine's execution commitments are invariant to prefill chunking (A used a system-prefix
snapshot; B replayed from zero).

## Not here (consensus seams, stated honestly)

Beacons and beacon-derived output salts, provider bonds and slashing, DA retention,
auditor sampling and dispute resolution, rewards/maturity, and on-chain anchoring of the
journal head root. The bridge is the coordination surface those attach to, not a
substitute for them.
