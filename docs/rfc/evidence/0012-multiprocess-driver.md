# RFC12 private process/P2P driver

`scripts/misaka-rfc12-multiprocess.py` executes a pinned private plan through actual
wRPC JSON and Ethereum HTTP reads. It owns three processes and distinct initially
empty app directories. Fork and partition traffic uses opaque loopback TCP links:
the production P2P protocol exchanges all headers, bodies, proofs and sidecars.
The driver never copies DBs/state between nodes, rewrites a parameter file, or
permits a dormant fence to open. A compatible private test binary and genuine
signed-work fixture commands are external prerequisites. Current shipping presets
keep retirement `None`; observing them produces **BLOCKED**, not acceptance.

Run with Python 3.9+ (no third-party dependencies):

```sh
python3 scripts/misaka-rfc12-multiprocess.py --plan private-plan.json --out evidence.json
```

The plan must be prepared and fixed **before** the run. Its SHA-256 is recorded.
Required fields:

| Field | Contract |
| --- | --- |
| `ruleset_id`, `policy_id` | Actual private binary fingerprints; unapproved production D/W values must not be guessed. |
| `minimum_depth`, `minimum_work` | Integer depth and decimal integer unique work; both positive, declared before measurement. |
| `minimum_isolated_daa_span` | At least 5,400 genuine DAA per isolated branch. A clock-stopped short fixture cannot pass. |
| `minimum_claims` | At least 100,000 actual state claims, observed through `getPalwCapacityShadow`; declared synthetic counts are not accepted. |
| `maximum_probe_ms` | Fixed latency acceptance bound. It includes actual public state/roots/readiness/capacity reads. |
| `fork_winner`, `partition_winner` | `A` or `B`, fixed before the corresponding run; the harness cannot pick a winner after seeing a result. |
| `nodes` | Exactly `A`, `B`, `JOIN`; each has `argv` (array), distinct loopback `wrpc`/`p2p` addresses, and loopback `eth` HTTP URL. |
| `fixtures` | Arrays of finite executable argv commands for `bootstrap`, `fork`, `partition`, `prune`. They create legitimate signed work and quiesce production before assertions. No shell is used. |
| Timeouts | Optional `command_timeout_s`, `probe_timeout_s`, `scenario_timeout_s`; evidence preserves timeout failures. |

Node argv must include `--nodnsseed`, `--outpeers=0`, and an appdir using the
role's `{A_APPDIR}`, `{B_APPDIR}` or `{JOIN_APPDIR}`. No `--connect`/`--addpeer`
is permitted: only driver-owned links can carry traffic. Fixture argv can use
`{ROOT}`, `{A_WRPC}`, `{B_WRPC}`, `{JOIN_WRPC}`, and (for the pruning phase)
`{OLD_BLOCK}`. A role's optional `grpc` field also supplies `{A_GRPC}` etc.
Ports and private fixture keys belong to the test plan; they are not production
network recommendations. Existing salted drill key/domain protections remain.
The driver supplies no activation argument and no model-acquisition gate.

The run performs isolated, mature fork selection; transport partition/heal;
actual old-block-body garbage collection; an empty-DB pruned P2P join;
and forced owned-child kill plus cold restart. It compares complete L1 header
commitments (UTXO, PALW, overlay, EVM), native generation/policy/readiness and
ETH latest/safe/finalized state roots across nodes. ETH block IDs use the actual
adapter's prescribed first 32 bytes of the native 64-byte block hash; complete
native identifiers are still compared through wRPC. No missing data, null
certificate, empty history, merely high DAA, or heartbeat/floor count substitutes
for native mature-work coverage. Capacity queries use their own connection and
a final generation check, because the expensive query is terminal on a stream.

Artifacts and node logs persist under the reported temporary directory for
review, including failures. Only processes/sockets created by the driver are
killed or closed. `SCENARIOS_PASS` means those measured mechanics passed under
that plan. **`fullRfc12SecurityApproval` remains false**: common-prefix analysis,
class soundness/economics, actor independence, D/W/concentration justification,
full bridge fill/withdrawal/backing matrix, in-flight IBD crash fault injection,
and authorization of live monetary/legacy-horizon policy are separate conditions.
This driver is not yet a measured full private network result.

Six oracle/socket controls are verified with:

```sh
python3 -m unittest discover -s scripts/tests -p test_rfc12_multiprocess.py -v
```

They reject dormant retirement, live DNS authority, stale generations/readiness,
and a one-unit-short u128 work floor. The pruning oracle recognizes successful
header-only RPC replies as missing bodies and refuses to call transport errors
GC evidence; the socket control checks established
transport cut and subsequent reconnection. These six tests are not node,
pruning, P2P-protocol, release100k, or soundness evidence.

The driver requires version-6 `getPalwNodeStatus` import diagnostics. An older
sender is `BLOCKED`, not a stable import. The epoch, active point, phase and marker
must stay identical across native/capacity/ETH observations; all completed native
observations have no active import and a true marker. A fresh pruned JOIN must
complete epoch > 0, and that epoch must persist after the owned forced kill and
cold restart. These are additional coherence checks within the pinned plan; they
do not supply a retirement policy, signed-work fixture or security approval.

## Unpublished private process helper

`misaka-rfc12-private-node` is an opt-in binary of the unpublished integration
package (`--features private-probe`); ordinary `kaspad` has no corresponding Args
flag. Its feature-gated library constructor injects a complete private `Config`
while retaining normal Args validation. It requires an explicit salted testnet-12
genesis, actual proof-of-work validation, controlled loopback peers/endpoints,
no DNS discovery or UPnP, and an absolute owned app directory. A directory must
be empty on first use; cold reopen requires its exact stored private fingerprint.
Ordinary `kaspad` refuses a directory carrying the private probe marker.

Build centrally, after the active shared-target jobs finish:

```sh
cargo build --locked -p kaspa-testing-integration --features private-probe --bin misaka-rfc12-private-node
```

The helper recognizes its own explicit arguments before calling the unchanged
public Args parser: `--private-probe-profile=native-v1`, a nonzero 32-byte
`--private-probe-salt=<64 hex characters>`, and optional
`--print-private-probe-profile` (prints identity without constructing a node or
opening a DB). Every process in one plan must use the same salt/profile. Its
normal arguments include `--testnet --netsuffix=12 --nodnsseed --outpeers=0
--disable-upnp`, explicit `127.0.0.1` P2P/gRPC/wRPC/ETH ports and its role's owned
app directory. Driver-owned links add peers through RPC after startup; argv does
not contain addpeer/connect routes.

`native-v1` has **test** retirement DAA 1, D=1, unique work=1, concentration
limits=1000 permille, and legacy-evidence horizon=5. These values exercise
transport and recovery mechanics. They are printed with the exact ruleset,
policy and genesis IDs before services begin, and are never release policy or
128-bit/common-prefix/economic approval. The Config's normal PALW validation
still runs. Existing post-launch rules are preserved; no synthetic class,
measured cost, conviction floor or eligible native market fill is manufactured.

This construction patch is a compile/runtime candidate. It supplies a compatible
private-process entry point, not the finite signed useful-work/100k plan or a
successful P2P run. Native useful-work, public market/bridge, forced-kill/pruned
join and release-scale acceptance remain pending until the actual logs and
commitments exist.

The construction profile preserves the shipped canonical-work schedule as well.
The salted T12 factory currently reports `canonicalWorkDaa: 0`; the generic
mainnet/testnet/devnet/RC dormant-fence test does not describe T12. The manifest
now exports this actual field rather than inferring it from another preset.
Non-floor attempt evidence requires an active canonical-work fence at the claim's
accepted DAA; floor claims are excluded regardless of their count. Supplying an
artifact or advancing heartbeat DAA alone therefore cannot turn this profile into
the full driver's mature-work fixture. A compatible validated private work profile,
real non-floor execution/Final/material evidence and its pinned fixture commands
remain necessary. Existing native harness admission/trace seams cannot supply that
public-process evidence.


## Finite construction/transport smoke

`scripts/misaka-rfc12-private-smoke.py` supplies a separate finite entry point
once the opt-in helper is built. It fixes and saves a generated loopback plan
before starting A/B/JOIN, owns initially empty DBs, reads real public wRPC genesis
headers/import stamps, and compares indexed main-wallet UTXO rows against the
salted genesis factory. The manifest includes only that public owner's address,
outpoints, amounts and maturity fields; no signing seed is emitted. Ordinary
P2P handshake, socket cut/heal, third-process join, SIGKILL and cold reopen use
the same production services as the full driver. There is no DB copy, synthetic
claim, background producer, timestamp restamp or fabricated native certificate.

```sh
python3 scripts/misaka-rfc12-private-smoke.py \
  --binary /absolute/path/to/misaka-rfc12-private-node \
  --salt <64-hex-nonzero-private-salt> --out /absolute/path/to/evidence.json
```

A successful measured run is named `CONSTRUCTION_TRANSPORT_PASS`. Its result
always keeps `matureUsefulWorkMeasured`, `prunedIbdMeasured`, `release100kMeasured`,
`marketFillsMeasured` and `fullRfc12SecurityApproval` false. It exercises genuine
process/RPC/P2P construction and cold genesis money recovery, and does not satisfy
the full driver's long-range/useful-work/pruning requirements. The driver is a
runtime candidate until the actual helper build and preserved logs are available.

The private constructor also applies the production salt keyring checks directly
to its injected Config: public producer/validator/pay keys and public genesis
bond/fee/stake outpoints are refused even though it intentionally uses no public
salt Args flag. These are shared checks, with production Args validation unchanged.

## Factory-derived private population

The same unpublished helper also accepts `--private-probe-profile=population-v1 --private-probe-bonds=N`, where N is greater than the existing registry and at most 100,000. These arguments belong exclusively to the feature-gated integration binary. Ordinary Args and public constructors have no population override.

The factory derives every added valid ML-DSA-87 registration from the explicit salt and bounded index, creates its collateral output at the existing registry floor and carves that collateral from the salted main output. The total premine remains exactly the original cap. It pins total registration count into monetary outpoint derivation, genesis UTXO commitment, genesis hash, ruleset identity and the owned DB manifest. Initial UTXO loading derives the same complete factory and the existing M-07 hash/commitment comparison remains mandatory. A caller cannot supply arbitrary initial monetary rows. Added workers declare no model capability and supply no beacon or useful work.

After the central feature build, a small construction check is:

```
python3 scripts/misaka-rfc12-private-smoke.py --binary /absolute/path/misaka-rfc12-private-node --salt <64-hex-nonzero> --profile population-v1 --bonds 17 --out /tmp/rfc12-private-population.json
```

A dedicated resource runner may select 100000 and longer explicit setup/probe deadlines, for example `--timeout 3600 --manifest-timeout 3600`. The driver fixes this count before launch, checks the public capacity read's total registry count, indexed coin supply, genesis money and roots across three owned processes, handshake/cut/heal and a cold SIGKILL/reopen. Capacity `limit=1` bounds returned rows; the underlying full-network calculation remains part of the measured cost. It preserves an owned-process RSS snapshot in KiB, owned DB logical bytes, factory timing logs and RPC latency observations. That snapshot is not peak-RSS evidence or a predeclared resource acceptance threshold. This result remains `CONSTRUCTION_TRANSPORT_PASS`; `release100kMeasured`, `prunedIbdMeasured`, `matureUsefulWorkMeasured`, `marketFillsMeasured` and full security approval stay false. No actual population process run is recorded yet.

The native carriage retains one bond public key and an operator identity hash; registration objects' second public key is not a second full key in the state carriage. RFC06's central native fixture observed 72 bonds / 71 candidates at 251,751 full-carriage bytes. A 100k registry with no seals may fit the retained 384 MiB snapshot ceiling, while multiple complete populations add a separate retained-state cost. The new profile permits the registry cost to be measured; it does not raise the bound or label a refusal as successful pruned transport. Complete populations, usable native beacon input, post-Final history compaction, genuine pruned joins and the approved resource/latency criteria retain their separate acceptance requirements.

## Predeclared construction resource observations

The private smoke also observes only its owned manifest/A/B/JOIN children at a
fixed sampling interval, retaining each role's maximum observed RSS and restart
count. The optional `--resource-limits=/absolute/path/limits.json` input is read
before any child starts and included in the pinned plan. No default release limits
are guessed. Without a limits file, `resourceLimitsMet` is unavailable (`null`).

The exact version-1 JSON schema has these fields:

| Field | Required value or unit |
| --- | --- |
| `schemaVersion` | integer `1` |
| `kind` | `rfc12-construction-resource-bounds` |
| `maximumOwnedProcessRssKiB` | selected positive integer, per owned child |
| `maximumOwnedDatabaseLogicalBytes` | selected positive integer, per role after shutdown |
| `maximumProbeMs` | selected positive integer, complete public probe duration |
| `maximumFactorySeconds` | selected positive integer, manifest factory elapsed limit |
| `samplePeriodMs` | selected integer from 100 through 5000 |

An observed excess, missing child/DB measurement or live-child RSS observation
failure refuses resource success. Manifest timeout kills only that factory child;
other owned children close through the existing driver cleanup. Manifest stdout/
stderr, factory elapsed time and failure artifacts persist even if node launch is
never reached. The chosen limits are immutable inside the sampler.

These are monitored acceptance limits, not OS hard quotas. `sampledPeakRssKiB`
does not capture an allocation entirely between samples, the RSS snapshot excludes
shared host/build pressure, and DB bytes are a post-shutdown observation rather
than a compaction peak. `completePeakRssMeasured` and `osHardQuotaApplied` remain
false. Passing selected construction limits does not change any release100k,
pruned IBD, mature-work, market or security flag.

Four resource controls cover strict predeclared schema, actual owned OS-child
replacement, an actual child refusing a bound fixed before launch, and missing/
excessive cost measurements. Together with the six existing oracle/socket controls,
`python3 -m unittest discover -s scripts/tests -p 'test_rfc12_*.py' -v` passes ten
tests. These tests are resource instrumentation controls, not native node or 100k
measurements.
# Finite signed-bootstrap transport driver (runtime pending)

`scripts/misaka-rfc12-private-bootstrap.py` is the finite next boundary after the
constructor-only smoke. It requires the opt-in private helper, the normal CLI,
an explicit private salt, a total carrier-fee cap, and the existing predeclared
resource-limits JSON. It creates the exact complete-v1 export, starts three
separate owned databases with different actual salted seat keys, queues the
signed registration and artifact binding through the normal CLI Borsh wRPC
carrier path, and then connects only ordinary driver-owned P2P links. The A
process uses the shipped floor producer with its real execution and PoW checks;
B and JOIN judge through their normal seat services.

The CLI's `palw submit-object --maximum-total-fee-sompi=<cap>` checks the actual
fee sum after assembling all carriers and before its first broadcast. The driver
also obtains a bounded dry-run preview, and queues the factory-priced objects
before enabling the first production peer. It waits for actual native class and
artifact-binding rows on all three processes, compares both public route roots
at the same sink/clock, kills only its own A process with the OS, reopens its
same DB as an observer under the identical consensus factory, and checks those
persisted identities and binding terms again.

A successful run records `nativeRegistrationBindingMeasured=true` only.
Complete-check, useful-work maturity, pruned IBD, market fills, 100k, and security
approval remain false. No clock, Final, registration or state carriage is
injected. Missing Panel participation, a closed participation gate, expired
envelope, native admission refusal, unavailable funds or a resource limit ends
the run with diagnostics; the driver does not weaken those conditions. Its
three Python controls verify refusal on another factory, actual OS child timeout
cleanup, and preservation of failed-command diagnostics. They are not native
registration or P2P evidence.
