# Public metadata feed publisher

The 113 audit found a fixed `llm-jobs.json` reset file from 2026-09-25, with no jobs publisher
service, timer or cron. The old manual header dump defaults to the pruning point and is not
used here. This publisher uses only public RPC headers and the verified metadata exporter.
It does not read material, answers, models, outboxes or keys. It never infers acceptance,
approval, Final or SecurityCertified; the frontend must join the actual node claim state.

`jobs-publisher.mjs` requires Node 20 with `--experimental-websocket` (already available on113).
No npm/pip dependencies are needed. Its exporter subprocess receives only these arguments:

```sh
/path/to/verified/misaka-palw-jobs-export \
  --blocks /var/lib/misakascan-jobs/headers.next.json \
  --retention /dev/null --network testnet-12 \
  --out /var/lib/misakascan-jobs/export.next.json
```

The initial selected-parent lookback is 24; subsequent runs validate the saved cursor and
rewind 12 selected parents to include parallel blocks. A cursor no longer on the selected chain, or an explicitly missing header/full block, falls
back to a bounded recent window. Other cursor RPC errors stop the run and preserve both files. Each run permits at most 6 pages, 64 RPC calls and 60 seconds by
default; limits have hard maximums. Header-only pages are bounded, and only non-floor native
Attempt headers go into the exporter dump. No full-chain pruning-point walk is used.

If the page limit is reached, only complete pages are exported, `source.complete=false` is
published, and the cursor records their last selected-chain point. A later run continues.
No cursor moves past data already published. Sync is checked before header collection and
again just before publication; loss of sync preserves the old feed and cursor. Existing model rows are merged by block hash
and capped at 512, so an old Final's header metadata survives a fresh page visit after it
leaves the live window. Presence in this feed does not mean the node admitted that claim.
The `fp_rows` projection is also metadata-only; legacy non-metadata data requires migration.

A lexical JSON scan converts unsafe integer tokens to decimal strings before `JSON.parse`.
Quoted strings and escaped contents are skipped. This avoids rounding nonce/u64 fields
through JavaScript Number; the Rust exporter already accepts decimal strings in `u64of`.
The JSON/message size and numeric-token length are bounded.

A nonzero exporter result, missing rows, hash mismatch, invalid schema, source RPC failure,
network/genesis mismatch or run-budget failure preserves the prior feed and cursor.
New output is metadata-whitelisted: prompt/output text, token IDs, `accepted` and `phase`
are not published. Model header identities are matched to the exporter rows before publication.
The file is staged and renamed atomically; cursor persistence follows publication. If cursor
persistence alone fails, the valid feed stays published and the next run may safely rescan.
A single-process lock prevents overlap; a dead process's stale lock is recoverable. Recovery
is itself serialized and rereads the lock owner, so a competing recovery cannot delete a
new owner's lock. An abandoned recovery guard requires operator inspection rather than
unsafe automatic removal.

## Validation and its limits

```sh
node --check contrib/misakascan-t12/jobs-publisher.mjs
node --test contrib/misakascan-t12/test/jobs-publisher.test.mjs
```

Eleven fixture tests cover a real public481352 header through a stub metadata exporter, failure
preservation, lossless u64 transport, page/cursor progress, retired cursor recovery, RPC count
and deadline limits, initial/mid-run sync refusal, strict outpoint normalization and overlap
protection. The exporter stub tests coordinator behavior; it does not establish
header hash validity or claim admission. The oversized-nonce case intentionally alters the
old header and tests transport only. The actual immutable Rust exporter must separately pass
the saved481352 CLI smoke before deployment. The actual immutable29 exporter smoke is saved
in `docs/evidence/palw/model-panel-one-2026-10-31/jobs-publisher-exporter-481352-cli-smoke.json`.
Its raw output is also passed unchanged into a publisher fixture. The tool emits its outpoint
as `(128hex, u32)`; the publisher accepts only that strict form or `128hex:u32`, rejects
out-of-range/noncanonical indices, and publishes the single colon form used by the UI join.
Native live RPC rows already use parsed bond fields; feed-only rows need this normalization.
Tests never read private data.

The metadata exporter is a different executable from a running node. Its29 source contains
future18850 consensus configuration; running this tool does not activate that fence on any
node. Header hashing consumes exact header fields, the class is decoded from the signed
Attempt, and the claim ID hashes the unchanged unsigned Attempt bytes. Job-anchor identity
uses network name plus the unchanged T12 genesis. These derivations do not evaluate the
future fork schedule, admit the claim, or authenticate model computation. The real CLI
matches the independently derived saved block/class/claim with `hash_mismatches=0`; the
unknown Qwen8K profile has null prompt fields. It supplies no acceptance/Final evidence.

A read-only negative integration run against the public95 RPC at2026-11-01T14:54:16Z
observed `isSynced=false`, DAA18930 and EDF/4024. It invoked no exporter and preserved SHA-
identical temporary local feed/cursor files. No production feed or service was changed.
Production deployment remains blocked until the parent verifies a suitable synced source.

## Parent-reviewed deployment (not performed by adding these files)

1. Verify a synced source, the immutable exporter revision/SHA and its actual481352 smoke
   output. Do not enable the timer merely because the metadata executable was built.
2. Back up exact public `llm-jobs.json`, SHA/stat, scheduler inventory and any existing files
   of the new names to `/root/misaka-incident-20261031/explorer-jobs-publisher-<UTC>/`.
3. Install this JS as `/usr/local/libexec/misakascan-jobs-publisher.mjs`, and install the two
   templates from `systemd/`. Put the verified exporter path in
   `/etc/misakascan-jobs-export.env`, following its `.example`. Never use an unchecked target path.
4. Review the service's exact paths, then `systemctl daemon-reload` and run the oneshot once.
   The existing tunnel is `127.0.0.1:38314 -> relay95:26314`. No node, filler, REST or nginx
   restart/config change is needed. No legacy exporter/textify command should be enabled.
5. Verify the public JSON, generated time, source DAA, metadata identities, and unchanged
   negative481352 admission in the UI. Enable the timer and observe another successful run,
   cursor progress and preserved history. Empty *new* model input must not erase existing rows.

Rollback: disable/stop only `misakascan-jobs-export.timer` and its oneshot, atomically restore
the backed-up public JSON, and remove/disable only these new unit/driver/config files before
reloading systemd. Keep workdir/audit records and immutable binaries. Existing nodes and
other services remain running.
