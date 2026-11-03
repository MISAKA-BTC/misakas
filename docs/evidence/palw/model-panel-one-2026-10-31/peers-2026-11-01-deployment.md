# Peer observation deployment — 2026-11-01

The parent authorized deployment of the reviewed peer observation change independently of
the pending canonical-chain decision. The frozen source was committed as0feaf511e. This
agent changed only census helpers/drop-in, its timer lifecycle, and the approved frontend
candidate on113. It made no node, DB, binary, hold, REST, nginx or publisher change.

The production app preimage13176ce4, index8098d01d, old script2eddb451, service command,
empty drop-in list and active/enabled timer matched the reviewed preflight. Frozen source
and candidate hashes matched. Temporary output at14:41:55Z verified actual runtime/PID/
listener/log binding and both rejection directions without publishing the feed.

All old service/timer/script, public JSON, app and index files were backed up under
`/root/misaka-incident-20261031/peer-census-deploy-20261101/backup`. The original service
and timer fragments and old script remain unchanged. Only the census timer and oneshot
were briefly stopped. Adjacent helpers were installed under `/usr/local/libexec/`, with
new `90-t12-runtime-observation.conf` under the census service's drop-in directory.
Daemon reload and the new oneshot succeeded; the existing timer resumed active/enabled.

At14:43:47Z the feed was available, testnet-12, DAA18935, P2P identityd8292f0c, paramsEDF/
schedule4024 and advertised fence18640. It contained157 observed event groups:146 local
rejections and11 received peer rejections, with zero unparsed records. These groups are
sessions, not157 distinct nodes or evidence of network majority.

The approved app was atomically replaced with SHA862c49fffc9c7526c03d71c08290c911726f07b74d03726367d74f371c9205d6.
Only the index script query changed to `t12-peer-observation-0feaf511e`; index SHA is
87bfd77e973cb3b1b20ad96ea637c53a5e6bf7f350480cc824aca787b9a69f70. External downloads at
14:44:37Z matched both. The exact public app passed20/20 existing and new fixture controls,
including live home/body hydration preservation, plus syntax checking. This was not a
browser-pixel check. The source observations still show a chain split and do not establish
Qwen claim acceptance or Final.

[Deployment record](peers-2026-11-01-deployment.json) contains all exact hashes, backups,
public source facts and boundaries. A guarded manual rollback candidate is saved locally
at `/tmp/misaka-chain-20261031/rollback-peer-census-113.py`; it has not been executed. Copy
it to the incident directory for parent review before use. It verifies installed hashes
and backup hashes, stops only census, removes only this drop-in/helpers, restores app/index/
feed and resumes the originally active timer. It refuses later file drift instead of
overwriting subsequent work. No node restart or hold release belongs to rollback.

After this check, another agent may pause node6/7 for separately authorized tests. The
census should then publish unavailable rather than a false zero; that is expected behavior.
