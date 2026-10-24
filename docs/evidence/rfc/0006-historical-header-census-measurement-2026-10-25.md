# RFC-0006 historical-header census: cold measurement and fallible reads

Status: core census controls passed 5/5; actual DB measurement controls passed 4/4 with an isolated quota child. This is the historical census consumer, not every historical-header transport/cache consumer or an archive/pruned campaign. Activation remains unarmed; K3 inputs remain uncreated and the 15 campaign gates remain UNKNOWN.

The actual consensus census previously called `headers_store.get_header`, then Borsh length measurement, and converted every store/measurement failure with `.ok()` into an unavailable entry. A cold read could construct and cache the entire header, its parent vectors, and opaque PALW commitment merely to measure its carriage size. Corrupt rows and storage faults were observationally indistinguishable from a pruned/missing header.

The new `DbHeadersStore::historical_header_borsh_len_v2` borrows an existing cached header if available. For a cold current-format row it reads the actual pinned bincode row, counts compressed parent runs and individual hashes through allocation-free visitors, borrows the commitment byte slice, and computes the exact Borsh size. It constructs only an empty-dynamic-field Header on the stack for fixed-field length measurement. That exhaustive construction also makes added Header fields a compiler error rather than a silent schema omission. It does not build parent vectors, clone the commitment, serialize a second header buffer, or populate the full-header cache. Bincode fixed-width encoding matches the existing persisted format; trailing/truncated data is rejected. This read-only measurement does not authenticate a hash, root, claim, or computation and does not change accepted consensus bytes.

The legacy fallback retains the existing Header2 conversion semantics, including merging identical adjacent parent lists. An allocation-free shape pass counts actual levels/hashes and validates the conversion's level limit. Before the owning legacy decode/conversion it reserves both concrete header layouts, Arc bookkeeping, old/new outer-vector slots and hash storage, with typed geometric growth envelopes. The ticket lives through conversion and Borsh measurement. A current-row decode error never falls back to legacy. Legacy namespaces remain subject to the existing database-open rules; this does not enable an old database or introduce a migration.

`palw_exec_origin_coverage_checked_v2` takes a fallible callback. Only a genuine `KeyNotFound` becomes `Ok(None)` and contributes to unavailable. Capacity, malformed-row, Borsh measurement and database errors abort the complete observation; a partial census is not published. The existing infallible convenience API remains compatible. The real consensus endpoint uses the checked reader while retaining the existing native state/sink/clock/import bracket. That endpoint still reports a String error; this change does not claim a new typed public RPC error protocol.

Scope limits: the pinned RocksDB/read machinery, small DB-key/prefix scratch, underlying source state and other concurrent owners require their existing separate accounting. The cold current-row scanner removes header materialization for this census only. Other full-header decoders/caches/transports, serializer scratch, whole-node RSS, CPU/disk cost, all-model costs, and archive/pruned availability at campaign scale remain open.

Executed controls use the actual Header serializer, real DbHeadersStore and production census loop:

- Exact Borsh parity across versions 0/1/2, 0/1/2/31/255 parent runs, boundary-sized commitments, maximum BlueWork; reject truncation and trailing bytes.
- Cache hit versus cold current row; cold measurement leaves the full-header cache empty; legacy repeated levels preserve existing conversion size; missing key differs from corrupt current row with a valid fallback.
- Isolated zero-logical-quota child: current cold measurement succeeds without an owning header, while legacy conversion returns LocalResourceUnavailable before decode.
- Fallible census reads abort on capacity/corruption/storage failure, stop further callbacks, and do not reinterpret a partial prefix as an unavailable-history observation.

The real DB/header suite passed **4/4 in 0.08s**, including its isolated zero-quota child (1/1 in 0.01s, included rather than added to the count). The coordinated build took 3m52s. Log: `/tmp/misaka-rfc-historical-header-measure-tests-retry2-20261025.log`; binary: `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_consensus-53c349c8ce803ba3`. K3 and real campaign requirements remain separate.

The actual core test binary ran the production census loop controls: **5/5 PASS in 0.06s**, including the isolated list-quota boundary/lifetime child cases and the new fallible-read abort control. The coordinated build took 7m05s including lock wait. Log: `/tmp/misaka-rfc-historical-census-typed-tests-20261025.log`. This proves the scoped core behavior, not live archive coverage.

The first shared consensus test build read dependency metadata before the separate TypedKernelV4 fixture added its direct validator-core test dependency, then saw that fixture's later module registration. It stopped at unresolved `kaspa_pq_validator_core` before header runtime. Log: `/tmp/misaka-rfc-historical-header-measure-tests-20261025.log`. The fresh header invocation resolves the saved manifest; no runtime success is claimed for the failed attempt.

The second shared consensus compile exposed three missing borrows in the separate TypedKernelV4 fixture's BTreeMap lookups (`E0308`), corrected before the successful header run. Log: `/tmp/misaka-rfc-historical-header-measure-tests-retry1-20261025.log`. The header production source did not change between these two attempts.

Final node library/tests/private-node metadata check passed (`/tmp/misaka-rfc-provider-rider-header-node-check-20261025.log`, 8m52s including lock wait). This compiles the actual consensus census API and writer integration; it is not live archive/pruned availability evidence. Later routing-origin/queue changes are outside the header run's behavior scope; the header source remains unchanged.

Implementation source commit: `e320f02d9`.
