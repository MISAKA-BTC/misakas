# Isolated devnet load measurement preparation (not executed)

Prepared tooling based on the candidate branch and the 2026-10-19 report. This document does not record an executed devnet measurement; build and run evidence must be recorded separately.

## Prerequisites to verify before execution

1. A reviewed tooling port of historical ML-DSA commit 67bc4d4ca plus measurement.rs is the required sender source. Its runtime capabilities must report MLDSA87, devnet_fanout=true and trace_version=1. Source availability is not binary verification or a completed run; pin the actual built binary and source manifest.
2. The old sender had two self outputs hardcoded; existing `misaka wallet utxo` only lists/consolidates. The reviewed measurement mode adds bounded per-depth batches with at most 32 outputs per split tx. The reviewed sender tooling candidate must supply real bounded fanout, or repeated zero-payload two-way split rounds waiting for actual maturity each round. Do NOT report one premine outpoint as many spendable inputs. A fanout tool must derive output count within existing mass/size limits, sign ML-DSA, calculate actual contextual mass/fees, retain txids, and wait for RPC-confirmed spendable outpoints.
3. The legacy sender logged failures but successful submissions only as aggregate rate. The measurement mode adds durable submit_attempt then submit result records. The reviewed sender tooling candidate must supply JSONL per-attempt events with monotonic+UTC time, full txid, payload size, actual serialized bytes, actual mass, RPC success/error and target pacing sequence. No seed/private key logging. A mempool observation is NOT an RPC submission timestamp or exact sent count.

## Binaries and isolation

Need source-pinned release `kaspad`, `misaka` (package misaka-cli), ML-DSA `rothschild`. Existing drill can use public devnet seed bonds 0..3 and public main-wallet premine output 40. All keys generated/copied into a fresh 0700 directory; file mode 0600. No t12/node appdir or external key import. Unique loopback ports reserved before launch: P2P base, wRPC-borsh base, additional node-0 JSON and gRPC ports. Retain exact argv, SHA256 of binaries and source commit/status. No fence/preset/cap changes. Use existing drill producer options; node0 must enable gRPC (remove drill's --nogrpc), and explicit JSON listener. No DNS/upnp/external peers. Verify RPC network=devnet AND all connected peers loopback before funding.

## Run sequence

- Build once under shared lock. Start 4 nodes, capture all stdout/stderr; verify all alive and DAA advancing.
- Fund ONLY measurement payer from public devnet premine; retain actual funding RPC response.
- Pre-split and wait. For a 600-second 1 tx/s run require at least ceil(rate*duration)=600 distinct confirmed, eligible initial UTXOs; this intentionally avoids relying on recycled outputs or a guessed maturity time. Record actual outpoint/amount/script/DAA/coinbase flag; keep enough per-output value to cover existing ML-DSA payload fee and change. If mass prevents 600 in one transaction, bounded batches/rounds; never raise caps.
- Run one persistent sender process for the complete fixed-rate run, not 3-second duty pulses or restarted every block. No fractional TPS claim: current CLI tps is integer. Retain zero-payload funding/split preparation separately from the confirmed-pool measured run. Do not start timer before readiness.
- Observer captures raw getBlockDagInfo, getMempoolEntries, getBlocks(includeTransactions) and getVirtualChainFromBlock(includeAcceptedTransactionIds). Store full hashes and full tx bodies; getBlocks scans DAG, not just sink. Selected-chain acceptance event records accepting block DAA separately from carrying-header DAA. Retain removed-chain lists to handle reorg; do not erase earlier observations.
- Stop sender gracefully, record exit status, continue drain observation until pending submitted IDs are accepted, rejected, or timeout; label right-censored transactions.

## Honest outputs

Report intended count/rate versus actual RPC successes/rate, unique mempool first-seen count, selected-chain accepted IDs, censored/failed submits, DAA progress and elapsed wall time. Pair individual txids. Record own exact serialized bytes and mass from sender; JSON string length is NOT serialized bytes. Record measured payload and background block composition separately. No p99 from inadequate sample, no inference of service guarantees/door<=3120/court CPU from this carrier test. Saturation is supported only if achieved offered bytes exceed observed available service and persistent queue/backlog occurs; otherwise mark NOT SATURATED. No heartbeat percentage inferred from admission drops.

Observer scaffold: observe.py. It does not create funds, start nodes or execute a sender. It deliberately reports exact submitted count as unknown; successful-submit tracing and fanout prerequisites must be verified on the reviewed pinned sender before a qualified load result. Synthetic fixtures in test_audit.py/test_driver.py are explicitly separate from actual measurement evidence.

## Exact reviewed measurement mode contract

`--network devnet --rpcserver 127.0.0.1:PORT --private-key-file KEY --payload-size 17408 --tps 1 --initial-utxos 600 --measurement-duration-secs 600 --measurement-funding-timeout-secs 3600 --measurement-log PATH`. Duration and funding timeout are each 1..3600 seconds; TPS 1..100; initial UTXOs 1..10000. The driver enforces these before touching RPC or launching a sender. Keys are ML-DSA-87. No --unleashed or alternate recipient.

Initial funding must be either an eligible pool of >=target_count independent own UTXOs or one sufficiently funded eligible root. For 600 the current sender requires at least 600 MSK operational output reserve plus actual split fees in that root (and actual dust/fee rules may require more); this is a sender experiment reserve, not a protocol capital recommendation. Sender refuses underfunding/mass overflow and waits for all observed outputs of each dependency depth. DAG and UTXO calls are separate observations, not an atomic proof of chain state. No new maturity rule is introduced.

Every RPC attempt is durably logged before submit. Matching returned txid means rpc_outcome=accepted. Generic RPC error, timeout or mismatched returned id means unknown, not rejected; the current sender emits no typed rejected result. Unknown stops the run but can still appear in mempool/selected chain; audit retains both facts. measurement_ended.success=false is not an admission rejection.

serialized_bytes has serialization=borsh_transaction_v1: exact signed Transaction DTO Borsh length, not P2P frame/block-wire length. Do not compare it with old 111,927-byte observations unless encoding identity is established. mass=max(compute_mass,storage_mass,transient_mass), and each dimension is retained. Accepting-block DAA is separate from carrying-header and first-mempool-observation DAA.

Pure offline checks: `python3 -m unittest discover -s scripts/drills/payload_load -p 'test_*.py' -v`. All fixtures are hypothetical and produce no real measurement. The driver attaches to an already isolated devnet. The separate managed launcher defaults to preflight only; its explicit --run mode starts the four-node devnet and funds the measurement key from public local-devnet funds.

## Candidate launcher and observation completeness

launch.py is a prepared managed launcher, not an executed run. Default mode validates pinned binaries/capabilities/unused ports and prints a plan without making a key directory, funding or starting nodes. --run uses fresh0700 workdir, four owned processes only, loopback P2P/borsh and node0 gRPC+JSON, existing floor-only/lane0,2,10 options, public devnet seed keys and a new measurement seed. The 1000MSK default is experiment setup funding from the public local premine, not a protocol capital requirement or recommendation. Existing sender actual-mass checks choose feasible split geometry; no fixed split count/depth/confirmation duration is promised. Actual trace depth/output matches and actual initial pool are the evidence.

Audit separates sender_trace_complete and observer_complete. Empty observer, missing start/end/schema, RPC error, absent APIs, no DAA progress or failure to bracket load interval makes acceptance/censor totals unknown, even if matching-RPC success counts are known. Sender throughput uses measurement_ended.elapsed_ms minus measurement_started.elapsed_ms (monotonic); t_unix_ms only pairs cross-process observations. Missing/nonpositive monotonic duration yields unknown rate; large clock shifts make observation interval pairing unknown. Initial-pool target_count is distinct from observed send attempts/successes and actual elapsed duration, and is not assumed to be achieved offered load.

## Streaming evidence storage and audit limits

The default observer file is `observer.jsonl.gz`. Each original JSON RPC record is streamed directly through gzip (level 1), flushed per record and closed with its footer when observation ends. Compression changes disk representation only; it does not filter transaction data or change counters. `observer-ready.json` signals process readiness so the driver never attempts to read a live, unfinished gzip stream. The final audit reads observer records once as an iterator; only the much smaller sender trace is loaded as a list. Incomplete gzip, invalid JSON, missing observer end or RPC gaps produce unknown observation totals or an explicit failed audit, not a zero-success measurement. Gzip CPU/IO competes for resources on the same machine and is part of the experiment context.

The current sender trace does **not** preserve every raw submitted signed transaction or its full input UTXO entries. It records actual DTO byte/mass measurements and identities. Observer RPC records can preserve signed transactions visible in mempool/blocks, but an unobserved rejected/unknown attempt may have no independently replayable transaction body/input snapshot. These are limits of the current build, not completed evidence coverage. Borsh DTO length remains distinct from network/P2P bytes and is not directly comparable with old block-byte observations.
