# RFC12 implementation and remaining acceptance, 2026-10-11

This record covers the full RFC12 requirements inventory in the [joint audit](../../design/palw/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md), not only a crash patch. The production retirement policy remains unassigned. All shipped presets retain `palw_dns_retirement = None`; no CLI activation override, approved D/W/concentration values, reward redistribution, or legacy-horizon extension is introduced here. ADR177 remains authoritative: no VM fallback, Torrent/Seeder machinery, or acquisition gate is restored.

## Concrete changes

The SDK `complete_check::VerifiedCompleteCheckV1` and `misaka model onboard verify`
support the complete-check bootstrap consumer. They finish one bounded op-211
inventory, check its ledger/aux roots, bind op-231's attempt/program to that same
snapshot, and derive the program, position limit, commitment, registrant and
kernel parameter root from the rooted rows. A file is accepted only for that
program and artifact; the entire admitted input/leaf domain is executed. The
2-MiB local container/header limit is checked before allocation, beside the
consensus 64-KiB tensor and 90,000-byte post limits. `model onboard complete-post`
prepares a signed tag-109 object for an open, unposted, unexpired attempt;
submission uses the existing public object path. There is no required beacon or
pre-existing Final in this consumer.

These are tooling and snapshot-integrity checks. The caller must authenticate
the served roots against its trusted chain view. The tiny typed program in the
SDK controls is explicitly a fixture, not a trained-model, native mature-work,
operator-independence, sampled-soundness or release result. Production kernel
and Panel=0 activation refusals remain intact. The complete-only private process
profile and actual public registration/work/Final/pruned-join run remain
separate requirements; construction-only process logs do not satisfy them.
This candidate has scoped parse/diff checks; its Rust execution is centralized.

The unpublished helper also accepts `--private-probe-profile=complete-v1`. Its
factory retains the salt's exact genesis, registry, supply, difficulty bits and
canonical economic basis, moves the TIR fence with its mirror to genesis, and
arms only the kernel route, signed registration and complete-only Panel-free
policy in addition to the existing explicit retirement mechanics policy. This
constructor checks the full salted factory fingerprint before opening its owned
database. It checks every production startup/value/dependency rule, except the
three unreleased activation refusals; normal `Params::validate_palw_v2` and
`ConfigBuilder` still refuse the profile. Sampled rewards remain disabled, the
effective target remains 128, and PoW/material/court horizons are retained.
It installs no artifact, capability, class, claim, receipt or Final. A finite
construction run under this profile remains construction evidence until actual
public registration/execution/prosecution/Final/material and pruning are run.

| Requirement | Implementation and actual consumers | Verification scope |
| --- | --- | --- |
| AUTHORITY / HEADS | EVM, PALW, legacy overlay and UTXO imports require the established pruning target. The sidecar imports recheck and pin it through their durable batches. A historic authentic root cannot authorize arbitrary replacement of the live tip. Existing pruned fixtures establish context through `import_pruning_points`; root-negative and selected-child-witness coverage remains. | New target controls are native consensus API tests. Rust execution for the latest guards is centralized in the integration checkout; parse checks alone are not execution evidence. |
| HEADS / cross-ledger crash recovery | All three sidecars persist `stable=false` in the same RocksDB batch as their accepted state. Cached readiness checks the incomplete marker even at the same sink, and native evidence / canonical ETH heads are unavailable until completion. | The central native batch passed the same-sink memo control and actual owned OS kill / cold reopen / identical retry cases for PALW, overlay and EVM. These process tests use public serving/import bytes and existing verified UTXOs; they are not P2P joins or full pruning drills. |
| AUTHORITY / UTXO recovery | UTXO import resolves a typed target/header failure before monetary writes and persists incomplete recovery with its position. It pins the target for copying and transaction validation, releases it before virtual/head publication, and uses nonblocking rechecks to avoid inverse or recursive pruning-lock waits. | `rfc12_utxo_import_target_and_completion_are_generation_bound` checks genuine genesis completion, wrong-target preservation, changed sink / UTXO position refusal and a missing-header typed error. Latest execution remains pending the central native batch. |
| HEADS / import completion | `ConsensusApi::complete_pruning_point_import`, its session wrapper and the actual IBD completion caller check virtual sink, established target, UTXO position, enabled PALW tip / overlay point, and EVM execution commitment / latest / snapshot before one `stable=true` batch. A changed or busy generation stays incomplete. Genesis premine initialization completes its own pinned generation and skips non-genesis sidecars. | Pointer coherence is verified at completion; it does not replace the preceding root verification. The explicit legacy stable-marker setter remains a trusted maintenance / test seam and is no longer used to finish production IBD. |
| MONEY / staged UTXO interleaving | Before virtual UTXO clear/copy, import recomputes the actual locked staging table MuHash and compares the header. An authentic incremental hash cannot authenticate a table changed by a second append stream. Processing uses at most 1,000 rows per chunk and the existing pool; read failures produce a typed refusal before virtual writes. | The native API control starts with real funded t12 genesis, retains its correct stream hash, changes one staged sompi through the public append API, and requires all virtual UTXOs to remain unchanged. No signed model approval or synthetic balance is admitted by this control. The central native batch passed the actual-table corruption refusal. Release-scale elapsed time remains unmeasured. |
| HEADS / durable stream ownership | `PruningImportSessionV1` binds the established point to a monotonically increasing persisted epoch. Begin writes that token and `stable=false` together. Every public session mutation is serialized against begin; clear is once, append precedes UTXO finalize, and completion closes the session in the same batch as `stable=true`. Token-free money/sidecar mutations and a blind true marker refuse after the first epoch. Actual staging PALW and full IBD propagate the token through every sidecar and UTXO chunk. | New controls delay an old append on a separate OS thread and complete the exact real genesis premine once. The actual OS-kill / cold-reopen sidecar drill now checks persisted epoch, a new same-point epoch, rejection of stale root-valid serving bytes and an authorized retry. These extensions are pending central execution, distinct from the previously passed crash control. |
| MONEY / retirement rights | Historic overlay retry verifies every existing bond / reserve / same-point snapshot before any write. Existing lawful later unbond / slash stamps survive when facts agree as of the authorized anchor. Identical append-once reserve rows are reused; conflicting monetary rows refuse. | The central native batch passed the actual signed pre-retirement bond / post-retirement unbond and identical historic retry control before the new target guards. The extension distinguishes an unauthorized historic replacement from an authorized retry. No confiscation, escrow increase or new monetary policy is assigned. |
| BRIDGE / combined fee income | Common `pay_model_leg` resolves real owner and contributor bonds and reserves their full collectible fee shares in the same R ledger as issuance before payouts, positions, market reserve or improvement-pool writes. Both reservations roll back when either recipient has insufficient room. Carrier and EVM buy/sell consumers use this common path; net seller principal is outside income. | Private fold tests cover positive carrier buy/sell, exact journal replay/undo, no manufactured Q/B/F/T slots, second-recipient rollback and two EVM actions where only one fills. The central core batch reported 38 pass and one assertion expecting the wrong typed refusal; the assertion now identifies the actual exhausted contributor. Latest rerun remains pending. These fixtures are not native successful-fill / backing / withdrawal evidence or class economic approval. |
| OPS / wallet | An unavailable, failed or disconnected DNS poll revokes the cached early-spend shortcut. Poll begin and publication bind RPC / network / connection generation under a small mutex; reconnect, rebind, disconnect and network changes invalidate it. Delayed successes from an old connection cannot restore a revoked anchor. Long DAA maturity remains lawful. | Wallet controls cover cached-anchor revocation and an ordered old-success-after-rebind / retired-response interleaving. These are wallet mechanics; actual wRPC transport execution remains distinct. The central wallet batch passes all three processor-owned generation/mature-entry controls, including actual balance, old iterator, priority/stash and lawful long-fallback paths. The central v9 batch also passes generation-bound scan response installation, context network provenance and network/RPC-pinned saved selection. |
| OPS / public import diagnostics | `getPalwNodeStatus` appends a version-6 tail with available / epoch / active point / phase / stable, read under one metadata guard. New JSON clients default older missing fields to unknown; gRPC fields are appended without changing existing tags. The CLI reports incomplete imports before old drawing evidence or unsynced-mining exemptions. The private P2P driver pins this stamp across native, capacity and ETH reads and requires the completed join epoch to survive cold restart. | RPC controls retain the v3/v4/v5 prefix, consume the exact v6 frame and treat old JSON diagnostics as unknown. CLI control checks that known incomplete recovery overrides old draw reports. This is pending central Rust/transport verification; a prefix control is not execution of an old binary. |
| AUTHORITY / OPS / release100k | The [private multiprocess driver](0012-multiprocess-driver.md) owns A/B/JOIN processes and separate initially empty DBs. It forwards ordinary P2P bytes over controlled links, executes pinned finite work fixtures, checks fork/partition healing, real body GC, empty-DB pruned join, forced cold restart, native/wRPC/ETH generations and commitments, actual 100k claims and fixed latency. | Six Python oracle / socket controls pass. They include successful header-only RPC pruning semantics, reject transport failures as GC evidence, and refuse unknown/incomplete import diagnostics. The opt-in unpublished private node helper now supplies a salted, fingerprinted Config construction candidate. Its compile/runtime and finite signed-work plan remain pending; no node/P2P/100k PASS is claimed. |

Implementation commits in the isolated RFC12 lane: `d54b2b17c`, `81a903f1b`, `a38f9fc44`, `a3c63af31`, `1c0bda0ab`, `0416b1e70`, `65e926f60`, `22648fb21`, `b97322ca0`, `f49b88345`, `f72048459`, `a0f98bc51`, `c56d6b922`, `974293a53`, `b8fd7b768`, `b3e724223`, `cc588b0f6`, `c556ecf6d`, `1c6fe7706`. Shared income API dependency is RFC08 commit `7a514e41e`; parent assertion correction is `e9d38ba71`. Integration may contain equivalent commits under different hashes. Do not cherry-pick dependencies or corrections twice.

## Authority callers reviewed

* `VirtualStateProcessor::dns_reorg_outcome`: retirement is resolved at the incumbent sink; the DNS BFT veto retires while the full PALW comparison / missing-data refusal remains.
* `dns_stake_preferred_tip` / sink search: retired or unavailable incumbent DNS does not nominate a stake-preferred candidate.
* Template / populated UTXO / block admission and validator duties: retired DNS participation kinds refuse, while lawful exit and finite historical evidence retain their distinct rules.
* `ConsensusApi::get_dns_confirmation`, active validators, attestation targets and precommit duty: retired authority is unavailable. Bond records remain readable for lawful rights.
* `get_native_settlement_snapshot`, `get_evm_canonical_heads` and public settlement explanation: incomplete pruning generations cannot supply native or ETH confidence labels. Root, policy, generation and prefix checks remain; Latest execution does not imply Safe.
* The RPC deposit-claim queue and template freshness predicate retire the optional DNS Pause gate together. Deposit claim/refund/debit/withdrawal consensus rules still apply.
* Production IBD establishes proof/history context on fresh staging before issuing its session and finishes through the checked completion API. Both public legacy proof/history context calls are serialized against begin and refuse after the DB has issued any session. A successful fresh staging context replacement batches incomplete with its proof descriptor/virtual/chain or target/history/header/retention authority update; metadata guards hide staged cache rows until DB commit. Invalid history retains its no-write preflight and previous stable marker. Warm authorized-target changes use the separate session-invalidating intrusive transition. These additional native controls are pending central execution. Authenticated EVM resync alone does not imply native Safe/Finalized reconstruction.
* Wallet/SDK DNS coinbase maturity consumes only a processor-owned, connection-bound live observation or ordinary long DAA maturity. Static network configuration never receives RPC anchors. Runtime global maturity-policy updates still propagate to each processor; a processor-specific display-policy setting does not change another processor's observed authority.
* Actual scan RPC consumers capture the connection generation before registration or RPC await. Synchronous response installation checks that generation under the rebind/disconnect/network mutex; the guard never crosses an await. Stale empty and nonempty responses return `StaleUtxoObservation` before installing entries. The trusted explicit `extend_from_scan` API remains available. Saved iterator and generator priority/stash selection bind network and connection generation, including a switch away and back to the same network. Contexts also retain their monetary network independently of the processor: newly constructed selection and current-generation imports refuse an old context on a different network. Wrong-network balances are unavailable, pending/stasis/outgoing rights are retained, and returning to the original network or a same-network rebind keeps a fresh lawful recovery path. Empty contexts created before network selection bind only their first monetary import; explicit clear permits reassignment. The central v9 batch passes the ordered wallet control over actual maps, balance, discovery bus and generator; it is not a real wRPC server interleaving.
* Network switches, RPC-generation invalidation and synchronous monetary context mutations now share the same connection-generation mutex. Actual insert/update/remove, scan installation, outgoing registration/cancellation/expiry and coinbase demotion commit before a switch or refuse without writes. Pending/stasis selection retains global rows until the matching generation's local and global promotion/revival commits; a delayed notification cannot remove an unprocessed row from tracking. Direct long-DAA stasis-to-mature jumps are supported. DashMap outgoing read guards are cloned away before gated updates or awaits to avoid an inverse lock wait. A new real OS-thread control stalls an actual insert at the context lock, starts a public network switch and verifies it cannot complete until the insertion commits; stale promotion/revival keeps both ledgers and fresh lawful maturity recovers. The central v9 batch passes the five wallet controls, including this actual OS-thread insertion/switch test, stale promotion/revival retention, direct stasis maturity and owned clear/rebinding checks. This remains distinct from a real wRPC notification or scan-response interleaving.
* Existing mature coinbases are reclassified into pending/stasis after their DNS shortcut disappears. Balance calculation, context UTXO queries, previously constructed iterators and generator priority/stash inputs recheck the current processor. Outputs remain retained and can mature again through the ordinary long DAA horizon. Existing non-coinbase outgoing/compound force-maturity behavior remains.
* `rfc0012_wallet_dns_authority_is_processor_owned_and_rechecks_already_mature_coinbases` is a wallet API regression candidate using actual `UtxoEntryReference`, balance, selection and generator consumers, two processors and the unchanged consensus 600-DAA fallback. It adjusts the processor-local wallet display maturity to 550 DAA to exercise early release without changing shared network policy. The central wallet run passes this actual-entry control after correcting the sweep fixture to use `Fees::None`. Actual wRPC execution remains a separate condition.

The feature-gated `population-v1` construction profile derives a bounded total registry up to 100,000 from explicit salt/size, with distinct valid ML-DSA keys and real collateral carved out of the existing main UTXO. It keeps total premine constant and pins registrations, monetary set, genesis and the DB manifest. The ordinary Args/presets and M-07 comparison stay unchanged; initial loader re-derives the factory and accepts no arbitrary UTXO override. The three-process script compares actual bounded capacity totals and indexed total supply and records owned RSS/DB bytes. New small factory/M-07/tamper controls are central Rust candidates, and no real population bootstrap or pruned transport has passed yet. Empty added capability sets and distinct operator IDs confer no model participation or independence approval. The existing snapshot ceiling remains a resource acceptance boundary.

## Remaining conditions, not approvals

| Group | Required remaining closure |
| --- | --- |
| AUTHORITY | Execute the pinned genuine mature-work multi-process fork / partition / long-range plan and certify attack controls. The driver and private API fixtures do not establish adversarial common-prefix security. |
| SETTLE | Approve D/W and operator/class concentration using actual measured unique mature work and a defensible adversarial/economic argument. Verify all native court / provider horizon extensions and pruned provenance. Registered operator IDs do not prove independent parties; no class has been assigned a guessed conviction floor or 128-bit soundness approval. |
| BRIDGE | A real eligible class must produce actual native carrier/EVM fills, backed fee/principal transfers, refund/withdrawal once and reorg recovery with agreement of both ledger roots. Existing native EVM cases that refuse `ClassNotEligible`, `MARKET_MISSING` or `EXCEEDS_POSITION` are useful negative controls, not this positive acceptance. |
| HEADS | Run actual pruned P2P transport and in-flight IBD crash / restart / finalized-conflict resync, including the complete population snapshot resource bounds. The durable-session implementation now closes the public concurrent append/clear ownership gap. Central execution of its cold-reopen/thread controls and real in-flight P2P stress remain required; pointer checks, locked table verification and session ownership retain distinct roles. |
| MONEY | Confirm every earn/pay crossing and legacy escrow/reserve/fee/exit/liability consumer against the approved transition policy and existing lawful horizons. No 92/8/0 schedule or extension beyond prior accepted terms is silently selected. |
| OPS | Execute the 100k resource/latency gate and actual SDK/wRPC callers with the selected private binary, inspect every preserved log/root and validate the chunked snapshot transport. Public presets remain dormant and startup activation remains refused until release evidence and explicit policy approval exist. |

The additional locked-table MuHash check performs an O(N) import-only scan. Its memory is bounded by the chunk size, but elapsed time / DB-I/O cost at release scale is unmeasured and belongs in the pruned-join resource gate. It is not a per-block fee or a public court-cost result.

The status service caches only the immutable boot consensus fingerprint. Its first calculation
still uses the unchanged `Params::consensus_params_id()` algorithm, which serializes the PALW
bundle and its genesis registrations; subsequent reads reuse that value instead of repeating a
registry-sized allocation. Import epoch, active point, phase, stable state and live authority
continue to come from the current consensus session on every request. This bounds repeated
status reads during a population probe; it does not establish a measured 100k latency gate.
The P2P context similarly caches the immutable params/identity/schedule hashes for handshake and
candidate summaries, checkpoint checks and recovery authorization. Their comparison algorithms
and messages are unchanged. Height-dependent fired/next fork IDs and peer re-judgment still use
the current DAA score, and live participation, headers, roots and import state remain uncached.
Native snapshot consistency checks reuse the processor's existing constructor-pinned ruleset
hash while still checking current generation, policy, execution roots and canonical provenance.
The capacity shadow's `limit=1` bounds the wire output, not computation: its existing full-network
accounting remains O(claims + bonds + locks + rows) before truncation. The construction script
records that actual read cost; no bounded-cost or 100k latency approval follows from the limit.

The complete-check CLI requires independent caller pins from a trusted chain view:
`model onboard complete-post --ledger-root HASH --aux-root HASH --tip-daa DAA ...` and the
complete branch of `model onboard verify` both refuse absent or mismatched pins
before artifact execution or key loading/signing. The current DAA must also be
pinned from that same trusted chain view. The convenience response and complete
op-211 inventory must match those roots and DAA as well as each other. Reports
explicitly identify the caller-pinned snapshot scope; the CLI does not establish
that the caller selected the right chain, operator independence, or sampled
soundness. Ordinary sampled verification retains its existing argument contract.

The SDK complete builder explicitly takes the authenticated current DAA and
refuses a different advertised DAA or a rooted route/attempt from its future.
It uses that authenticated DAA for the 60-DAA post deadline. The production kernel
tick returns early when no claim, seal, demand or other busy ledger row exists;
therefore the ledger-root-committed `header.scalars.daa` can lag and cannot
substitute for an authenticated current chain clock. The new SDK control covers
a stale advertised clock that would otherwise revive an expired open attempt.

## File-derived signed bootstrap preparation

`model onboard bootstrap-prepare` closes the previously harness-only construction
of `ArtifactBoundV1` (104), a real `KernelRouteV1` registration (110/inner 1),
`KernelBoundV1` (106), and `ConformanceCommittedV1` (107). Its SDK boundary reads a
bounded complete-check PALWTIR1 file and the caller's locally retained
`ClassRegisteredTirV1` object. It re-derives the class identity, exact program,
layout/tokenizer declaration, complete artifact inventory, parameter commitments,
canonical v2 kernel plan and exhaustive input/leaf reference. It enforces the
existing complete domain/post/file bounds and caller-selected maxima for the
100-BILI binding reservation and 1-BILI complete-check bond fee before signing.
Carrier transaction fees remain separately checked by ordinary submission.

`artifact-bind` exports a local statement and makes no claim that its retained
native registration object has been included. The three later stages require
`--ledger-root`, `--aux-root`, and `--tip-daa` from one independent trusted caller
chain view. Before executing the file or loading the signing key, they authenticate
every op-211 page as one complete inventory. They require the exact binding to
be mature, unrefuted, owned by that registrant, and not from the future. Kernel
registration runs the production plan, public-court/DA and carrier resource gates
under the rooted policy. Later stages refuse any program/plan/parameter record
substitution or a binding that already exists. Commitment uses caller-supplied
bounded Borsh metadata, preserving its source/implementation/scope/resource
statements, and verifies all chain/file roots and existing lifecycle retry rules.

All four stages only write signed Borsh objects; they do not submit, modify chain
state, inject classes/capabilities/Finals, or authorize public activation. Reports
identify local-file or caller-pinned snapshot scope and retain native registration
inclusion, mature useful work, trained-model provenance and security approval as
unestablished. The three new SDK controls use a seeded typed code fixture for
actual signatures and root/file/clock/cost/refutation/statement refusal. Central
Cargo execution and actual process bootstrap remain separate acceptance evidence.
# Private bootstrap export boundary (candidate; runtime evidence pending)

The feature-only `misaka-rfc12-private-node` accepts
`--private-probe-profile=complete-v1 --private-probe-salt=<private salt>
--private-probe-export-bootstrap=<new owned directory>` together with its normal
private testnet arguments. It exports an actual PALWTIR1 wide128 seed-37 code
fixture, raw native tag 61, signed registration envelope 108, and signed artifact
binding 104. Registration prices come from the exact factory's genesis base
class, and native admission plus the bounded file-derived complete-check domain
are checked before signing. The envelope binds the factory's fired fork digest
at DAA 1 and expires at DAA 32. It is intended for the initial bootstrap, and
does not authenticate the live registration target after retargeting.

The fresh directory is mode 0700; salt-derived private funding and bond seeds
are mode 0600, created without replacing existing files, and omitted from the
public manifest. Ordinary startup, PoW validation, Panel quorum, material windows,
registration inclusion, and complete-check consensus validation remain in force.
This export inserts no state, capability, claim, conformance outcome or Final.
The manifest's inclusion, useful-work, trained-model, 100k and security flags
remain false until separately supported by real process evidence.

# Signed worker and carrier batch boundaries (candidate; central tests pending)

Both automatic class-worker submission sites now wrap their retained raw
declaration only when `palw_signed_registration_v1` is active. The envelope binds
the actual network/genesis, fired fork digest, registrant bond and a checked
200-DAA retry window. Raw retry/inclusion tracking is preserved. Signed
registration carriers cannot be replaced by the proof-priority fee-spend path,
and CLI duplicate checks inspect the wrapped class/root before constructing fees.
Dormant presets continue submitting their original declarations.

The capped CLI submission path checks the whole built batch immediately before
its actual transport callback. New controls build real signed native carriers,
check their payload and deducted fee, and exercise an exact total cap, a
one-sompi excess, and overflow of two individually representable deductions.
Both refusal cases require zero callback invocations. The extreme values are
CLI arithmetic/side-effect controls, not evidence of network supply admission.
These controls do not establish real carrier inclusion or mature useful work.
