# PALW Object-v2 local ingress and availability operations

This runbook describes the Object-v2 path used by the live Header-v4 `testnet-200` network.
`palw_algo4_accept` is enabled in its preset; the local importer remains an explicit operational
surface and starts only when the operator supplies `--palw-enable-algo4`.

## Security boundary

There is no Object-v2 HTTP, gRPC, or wRPC upload method. Initial bytes enter a node only through an
explicitly configured local filesystem spool. Both the spool and peer recovery converge on the same
ordering:

1. require Object-v2 and the 256-KiB consensus cap;
2. recompute canonical chunk metadata and content root;
3. freeze one selected-chain sink and resolve the exact batch/leaf and provider bonds;
4. verify both owner-to-session authorizations, both Receipt-v3 signatures, slots, epochs, and match;
5. atomically insert the content-addressed durable row;
6. publish bytes to the bounded in-memory P2P serving cache.

Malformed, oversized, legacy V1, root-mismatched, side-fork, or semantically invalid bytes fail before
durable insertion or serving. The P2P service is separately gated on negotiated protocol version 103.

## Prepare the spool

Use an absolute path owned by the kaspad account. Directories must be mode `0700`; input files must be
regular, single-link, non-symlink files owned by the same uid with no group/other permission bits
(the provided CLI creates them as `0600`). Kaspad rechecks this boundary on every scan.

```sh
umask 077
install -d -m 0700 /srv/misaka/palw-da
kaspad --testnet --netsuffix=200 --palw-enable-algo4 \
  --palw-da-import-dir=/srv/misaka/palw-da
```

The flag is default-disabled, requires `--palw-enable-algo4`, and is refused by sync-only node
profiles. On an inert network the daemon fails startup rather than running a warning-only importer.

The spool contains `incoming/`, `processing/`, `archive/`, and `quarantine/`. A producer must publish
one job as follows:

1. write and `fsync` `<job>.palwda.tmp` as `0600`;
2. atomically rename it with no-replace semantics to `<job>.palwda` and `fsync` `incoming/`;
3. write and `fsync` `<job>.json.tmp` as `0600`;
4. atomically rename it with no-replace semantics to `<job>.json` **last** and `fsync`
   `incoming/`.

The metadata rename is the ready marker. Do not copy a partially written final filename into the
directory. `misaka palw da enqueue` implements the sequence above with Linux `renameat2(...,
RENAME_NOREPLACE)` or macOS `renamex_np(..., RENAME_EXCL)`; unsupported filesystems/platforms fail
closed. An existing regular file or dangling symlink is never overwritten.

## Qwen lifecycle export to node admission

First create the selected-chain context described by the Qwen runtime. It contains the on-chain
`batch_id`, `leaf_index`, provider bond outpoints, leaf Receipt-v3 expectations, and both owner
authorizations.

```sh
palw-lifecycle export \
  --db lifecycle.sqlite \
  --network NETWORK_HEX128 \
  --escrow ESCROW_HEX128 \
  --node-context node-da-context.json \
  --out receipt-da-object-v2.json

misaka palw da enqueue \
  --artifact receipt-da-object-v2.json \
  --spool-dir /srv/misaka/palw-da
```

The enqueue command accepts only schema `misaka.palw.lifecycle-receipt-v3-node-da-bridge.v2` with
`node_da_object_compatible=true` and `node_admission_required=true`. It decodes
`bridge_object.bytes_hex`, independently recomputes the V2 root/length/chunk count, and writes:

- `incoming/<root>.palwda`: exact canonical object bytes;
- `incoming/<root>.json`: schema `misaka.palw.da-spool-entry.v1`, batch id, leaf index, root, length.

Compatibility is not admission. The Qwen artifact continues to say `consensus_enforced=false`; only
the node log `admitted and archived Object-v2 …` and the matching
`archive/<root>.complete.json` marker indicate successful full node admission.

## Inspect and answer an Object-v2 challenge

`kaspa-pq-validator palw-payload da-inspect` and `da-response` accept both canonical legacy Object-v1
and Header-v4 Object-v2 bytes. They read the version prefix from the exact canonical object and build
the Merkle proof in that version's commitment domain; Object-v2 is not repacked as Object-v1.

```sh
kaspa-pq-validator palw-payload da-inspect \
  --object-file /srv/misaka/palw-da/archive/OBJECT_ROOT.palwda \
  --chunk-index CHUNK_INDEX \
  --proof-out /secure/incident/OBJECT_ROOT.CHUNK_INDEX.proof.borsh

kaspa-pq-validator palw-payload da-response \
  --network-id PALW_NETWORK_DOMAIN_U32 \
  --challenge-id CHALLENGE_ID \
  --provider-bond PROVIDER_BOND_TXID:INDEX \
  --owner-key /secure/provider-owner.seed \
  --object-file /srv/misaka/palw-da/archive/OBJECT_ROOT.palwda \
  --chunk-index CHUNK_INDEX \
  --out /secure/incident/CHALLENGE_ID.response.borsh

kaspa-pq-validator palw-submit \
  --node-wrpc-borsh NODE_WRPC_BORSH \
  --network NETWORK \
  --validator-key /secure/funding.seed \
  --kind da-response \
  --payload-file /secure/incident/CHALLENGE_ID.response.borsh \
  --exclude-funding-outpoint PROVIDER_BOND_TXID:INDEX
```

Require `object_version: 2` from the inspection step and independently match the challenge's object
root and sampled chunk index before allowing owner-key access. Existing files and symlinks are not
overwritten. The CLI validates the completed 0x3b bytes with the same stateless consensus validator
before writing them; selected-chain state still performs the root, challenge, deadline, bond-owner,
and ML-DSA checks during acceptance.

## Crash recovery and failure handling

Claim and archive transitions are restart-idempotent:

- a partial `incoming/` → `processing/` claim is completed from its counterpart and full admission is
  rerun;
- if enqueue previously made only the final object durable, a retry securely opens that owner-only,
  single-link, non-symlink file, verifies identical bytes/inode stability, and publishes metadata;
  mismatched bytes are rejected and final paths are never deleted as cleanup;
- each enqueue globally reclaims exact valid dot-temp prefixes older than ten minutes after the same
  owner/mode/link/open-inode checks. The cleanup examines at most 4,096 directory entries and fails
  closed above that backpressure bound; recent prefixes are left for an in-flight producer;
- a partial archive without a completion marker is rolled back to `processing/` and full admission is
  rerun;
- a completion marker is terminal only when the archived metadata and object still validate exactly;
- existing processing/archive/quarantine destinations are never overwritten; conflicting copies are
  quarantined for inspection;
- every security-relevant rename and marker creation is followed by source/destination directory
  `fsync`.

All daemon directory work is rotating and bounded. Persistent iterators consume at most 64 incoming
entries and 64 processing entries per two-second tick (including irrelevant object/temp names) and
attempt at most 16 jobs. Archive audit consumes at most 32 entries and revalidates at most four object
triplets per tick, then reopens at EOF. Thus finite backlogs cannot starve later names, and a retained
archive is revisited without hashing every object on every scan. Completed archive triplets are not
automatically deleted. Size an operator retention policy for this audit trail; rotate only a whole
`.palwda`/`.json`/`.complete.json` triplet after its on-chain DA retention window has ended, preferably
with the importer stopped, and retain an external immutable copy required by the incident policy. Never
rotate one member of a live triplet in place.

Any malformed/insecure/admission-rejected pair is moved to `quarantine/` with a bounded error marker.
Fix the cause and enqueue a new job; do not edit an archived completion in place.

## Peer recovery, restart rehydration, and retention

When the independent acceptance lever is active, the node periodically reads one fork-coherent
selected-parent snapshot. It atomically replaces the serving cache with canonical V2 bytes whose
obligations remain inside retention, removing stale side-fork roots on reorg/restart.

For a missing object it prioritizes an open challenge's sampled chunk, selects up to eight peers at
protocol 103, and uses bounded request timeouts, exponential backoff, and peer failover. Every 16-KiB
chunk proof is independently verified. All chunks must reconstruct the selected-chain V2 commitment
before full admission and publication. Success, timeout, invalid-response, failover, rehydration, and
GC counters are exposed through the in-process flow-context telemetry snapshot and success/failure is
logged.

Durable GC does not use the smaller 64-object/8-MiB serving view. It captures the complete retained
root set from the bounded fork-local obligation table, scans the object prefix outside the virtual
commit lock, then reacquires the lock and commits one deletion batch only if the selected parent is
unchanged. Each deterministic batch removes at most 4,096 roots; later sweeps continue from the next
stale keys. Missing/corrupt active selected-parent state, iterator error, reorg/generation mismatch, or
batch failure deletes zero rows. This bounds insert-only side-fork/restart accumulation without deleting
a live root merely because it fell outside the serving cache.

The recovery scheduler obtains and republishes availability bytes; it does not hold provider owner
keys and therefore does not sign or submit on-chain 0x3b responses on an operator's behalf.

## Automatic challenge response (off-node)

Automatic challenge discovery and deadline-aware owner-key submission now ship as an off-node tool,
`kaspa-pq-validator palw-da-auto-respond`, keeping the owner key off the node exactly as the security
boundary requires. Discovery uses the read-only `getPalwState` RPC, which was extended to return the
open DA challenges on a queried provider bond (`da_challenges`: challenge id, provider bond, object
root, sampled chunk index, opened/response-deadline DAA). The tool:

1. queries `getPalwState` for each `--provider-bond` the operator owns and reads its open challenges;
2. plans responses with a pure, unit-tested policy — only owned bonds, never a challenge whose deadline
   has already passed, soonest deadline first, capped per cycle (`--max-per-cycle`), urgent-flagged
   within `--safety-margin-daa`;
3. with `--enable-auto-response`, for each due challenge reads the served canonical object bytes from
   `--object-dir` (`<object_root>.palwda`), builds the owner-signed 0x3b response via the same
   `build_signed_da_response` primitive as the manual path, and submits it through `palw-submit`,
   excluding the bond outpoint from fee funding.

Without `--enable-auto-response` it only discovers and prints the plan. The node enforces
funder == provider owner, so the owner key both signs and funds.

```sh
kaspa-pq-validator palw-da-auto-respond \
  --node-wrpc-borsh 127.0.0.1:27210 --network-id PALW_NETWORK_DOMAIN_U32 \
  --owner-key /secure/provider-owner.seed \
  --provider-bond PROVIDER_BOND_TXID:INDEX \
  --object-dir /srv/misaka/palw-da/archive \
  --interval-seconds 15 --enable-auto-response
```

The manual `da-inspect` / `da-response` / timeout payload/submission tooling above remains for incident
handling. Live multi-node withholding/reorg/archive-rotation and long-retention soak remain operational
release requirements; this tooling is exercised in the unbond/slash rehearsal but not yet under a
multi-node adversarial soak.
