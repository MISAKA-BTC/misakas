# Joining Testnet-12 as a PALW producer

**testnet-12 launched on 2026-09-25/26 JST** from release commit `0e8ec984e` (`rcore/int-3`). On
2026-09-26 the public fleet moved to the node-only update `8a0810992` (same identity; see launch note
§0) — **build `8a0810992`**. The identity values below were checked against that release. Read
[`t12-launch-2026-09-25.md`](t12-launch-2026-09-25.md) first: it lists what the release contains, the
known issues (two CRITICAL ones are fixed by post-launch activation fences) and when a payment may be
treated as final.

The collateral figures in §4–§5 were re-checked against the release on 2026-09-26: the release's own
reservation functions evaluated on the testnet-12 parameters, and the per-claim `escrowSompi` /
`reservedSompi` and each bond's `reserved_exposure` read off the live chain. Two things the node
itself prints are **not** the rule: its derived default collateral and its "may then hold forever"
warning are a legacy weight-only formula (§5). `misaka bond status`'s `exposure_ceiling` and
`reserved_exposure` are the live answer. The 2M row's figure below has not been re-checked. The faucet is still
**TBD**: it has no testnet-12 funding yet (the operator decides that).

The deployment record, which covers the genesis, the collateral and why, is
[`testnet-12-regenesis-2026-09-23.md`](testnet-12-regenesis-2026-09-23.md).

Blocks are produced by PALW ConsensusV2 at a fixed 120-second cadence. On testnet-12 one DAA step is
one execution span (`span_daa = 1`), so 1,000 DAA is about 33 hours. `kaspa-pq-miner` and
`misaminer` cannot build the attempt envelope. Production runs inside `kaspad --palw-produce`.

## 0. What is different from testnet-11

* **A new chain.** The genesis is `a27f8f44…`. A testnet-11 bond does not exist here. A key file
  can be reused, but its bond has to be registered again on testnet-12.
* **Its genesis outputs have their own txid.** The premine sits on `5e0d5f1b…` (not the
  `misaka-premine` sentinel every other network and the private testnet-12 instances use), so no
  spend signed on another chain is valid here. The indices are the usual ones (genesis bonds 0–7,
  fee floats 41–48); a command line that named `<sentinel>:<index>` must name the new txid.
* **Collateral backs the whole fraud gain.** Every claim reserves its escrow plus its weight against
  the bond, and at the current subsidy that is about 3,200.95 MSK per floor claim (§5). One bond holds only
  as many claims at once as its collateral covers.
* **`--palw-register-bond` works on testnet-12 again.** testnet-12 requires an operator-possession
  signature from DAA 0. Before `5a559459` the node's registration carried one signature, so every
  carrier was mined and its bond was dropped. It now carries both signatures.
* **Model classes pass through a lifecycle** before they pay (§8). A class you register yourself
  starts in `Candidate` and waits for an admission audit.
* **The CLI defaults.** Every `misaka` command — the operator commands (`mining`, `verifier`,
  `doctor`, the status snapshot) and `bond`, `wallet`, `key` and the rest alike — uses testnet-12
  when you name no network. `--network`, `MISAKA_NETWORK` and `~/.misaka/config.toml` still win, in
  that order (§3).

## 1. Build

Use commit `8a0810992` — the node-only update of the launch commit `0e8ec984e` that the public fleet
runs since 2026-09-26 (kaspad sha256 of the fleet's x86_64 Linux release build
`07cba17406c486e0e31d4d46e107b2998cf70229125e804513f59099051c903b`). A node on `0e8ec984e` that
runs a producer can fork itself off the network (launch note §0): replace it.

```bash
git checkout 8a0810992
```

The docs inside that checkout are the older copies; keep following this page on `main`.

```bash
cargo build --release -p kaspad -p misaka-cli
# only if you will serve a model class:
cargo build --release --bin qwen25-convert --bin palw-class
```

The binary from `misaka-cli` is `target/release/misaka`.

## 2. Keys

```bash
misaka --network testnet-12 key gen --out ~/.misaka/miner.seed      # 0600, refuses to overwrite
misaka --network testnet-12 key pubkey --key-file ~/.misaka/miner.seed
misaka --network testnet-12 key address --key-file ~/.misaka/miner.seed
```

`key pubkey` prints only public values: the ML-DSA-87 verification key, its P2PKH payload, the
funding address and the validator id. Compare it with
`misaka bond status --bond <outpoint> --output json` (`registered_pubkey`) to confirm that a key
file is the one a bond was registered under.

**One key is one operator, and it can register one bond for the life of the chain.** testnet-12 arms
`palw_operator_id_unique` from DAA 0. Retiring a bond does not free its key, and collateral cannot be
topped up. For more capacity, create a new key and register a new bond.

## 3. Start a node

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default
```

The node bootstraps from the built-in DNS seeders. `--addpeer` takes IP addresses, not hostnames.

| purpose | port |
|---|---:|
| P2P | 26311 |
| node gRPC | 26210 |
| wRPC Borsh | 27210 |
| wRPC JSON | 28210 |

Check the startup log for these lines:

```text
Consensus params fingerprint: b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f (network testnet-12)
Consensus fence schedule: 1000 (schedule id 93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd)
```

A datadir from the first testnet-12 deployment (genesis `a8cabac4…`) is refused at startup with a
genesis mismatch. Move it aside. Do not point the new node at it.

`misaka` already defaults to testnet-12 for every command. If you also run commands against another
network from the same shell, name it each time, or pin testnet-12 once so a stray setting cannot
move you:

```bash
export MISAKA_NETWORK=testnet-12        # or: misaka --network testnet-12 config init
```

The examples below still pass `--network testnet-12` explicitly.

## 4. Funds

**Faucet: TBD.** A testnet-12 faucet needs funding from the testnet-12 premine, and that is the
operator's decision. The setup wizard has a faucet hint for testnet-11 only, so on testnet-12 it
prints none. Once someone has funds, `misaka --network testnet-12 wallet send --key-file <k> --to
<addr> --amount <MSK> --yes` moves them.

**testnet-12 runs the mainnet-assumed bonds** (decided 2026-09-24): a producer bond needs at least
**13,000 MSK** (the producer floor), a panel seat at least **130,000 MSK** (ten producer floors), and
a DNS-finality validator bond at least **20,000,000 MSK** (below).

What each role needs, from §5:

| role | bond collateral | also |
|---|---|---|
| panel seat on the floor only (`misaka verifier`) | **at least 130,000 MSK** (the seat floor); a seat is drawn only with about 640.17 MSK of room under its 500‰ ceiling for each floor or 8k panel it is bound to | a fee float at the key's address (≥ 0.1 MSK) |
| floor producer | **at least 13,000 MSK** (the producer floor), and **about 6,402 MSK for each floor claim held at once**: 13,000 MSK holds 2, 100,000 MSK holds 15 | the fee float |
| `Qwen2.5 graph-v7@8192` producer | **at least 13,000 MSK**, and **about 6,451 MSK for each 8k claim held at once** — pass `--palw-bond-collateral` (§5: the node's default for this class is unfundable) | the artifact, and memory (§6) |
| `Qwen2.5 graph-v7@2097152` producer | **about 125,888 MSK for each claim held at once** | about 11.6 GiB per attempt, and a week of CPU per attempt on a fleet host |

The fee float pays the lifecycle carriers: registrations, receipts, readiness proofs and court
answers. Change returns to the same address, so one funding lasts for many carriers. The float must
be a different output from the bond collateral.

**DNS-finality validators** (`misaka validator`) stake a separate bond of **at least 20,000,000 MSK**.
DNS finality activates only once **at least 6 validators** are active with **at least 120,000,000
MSK** of active stake between them; until then no anchor is DNS-confirmed and a coinbase matures on
the 600-DAA fallback alone (about 20 hours). The unbonding period is 10,083 blocks, **about 14 days
and 6 minutes** at the 120-second cadence (14 days of evidence window plus the reorg horizon).

## 5. Register a bond

### Inspect first

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
misaka --network testnet-12 bond status --key-file ~/.misaka/miner.seed
```

* `registry: REGISTERED`: a formally registered PALW bond.
* `registry: NOT REGISTERED`: an ordinary, reserved or locked UTXO, not a registry record.
* `UNDERSIZED`: registered, but below the node's whole-lifetime sizing for the inspected class.

Do not register when `bond status` already says `REGISTERED`.

### How much collateral

The chain reserves each claim's full fraud gain against its producer's bond, from the block that
accepts the claim:

```text
reserved per claim = escrow + weight
escrow             = the accepting block's subsidy x worker carve 720 permille
                   = 444,562,014,000 sompi x 0.72 = 3,200.84650080 MSK (block-one subsidy)
weight             = 0.10752660 MSK (floor) / 24.71600230 MSK (8k)
room               = collateral x 500 permille - everything the bond already backs
```

A claim therefore reserves 3,200.95 MSK (floor) or 3,225.56 MSK (8k), and a bond holds
`collateral × 0.5 ÷ (escrow + weight)` of them at once: one floor claim per ≈ 6,401.91 MSK of
collateral, one 8k claim per ≈ 6,451.13 MSK, and never below the 13,000 MSK producer floor.
13,000 MSK holds 2, 31,191 MSK holds 4, 100,000 MSK holds 15. The escrow scales with the subsidy.

**A full bond waits; it does not wedge.** When the room is full the producer holds with `the bond's
exposure ceiling leaves no room for another claim` until room comes back. The escrow part is released
early, at licence, when every seat of a panel that was never redrawn returned `Valid` (not on the 2M
class); otherwise the whole reservation stays until the claim is `Final` or voided. That is why a
fleet bond (939,063 MSK) carried 190 floor claims at once on 2026-09-26 with only ≈ 212,557 MSK
reserved: most of them were licensed. Registration and each declared class also reserve small fixed
amounts on the bond, and a seat's panel duties share the same room.

**Pass `--palw-bond-collateral` explicitly (in sompi).** Without it the node locks its own derived
default — `palw_v2_collateral_for_claim_lifetime_v1`, a weight-only formula from the devnet economy
that R-core+ did not replace. On the release it is 3,119,145,986,560 sompi (≈ 31,191 MSK, 4 floor
claims at once) for the floor, and **≈ 2,000,332,625 MSK for the 8k class** (`--palw-producer-class`
set to the 8k id), which no one can fund. When a named amount is below that figure the node warns
that the bond "may then hold forever"; on testnet-12 the warning is wrong — such a bond holds claims
while it has room and waits for a release when it does not. `bond status`'s `UNDERSIZED` and
`whole_claim_lifetime_collateral_sompi` read the same legacy figure.

### The wizard (recommended)

```bash
misaka --network testnet-12 mining setup
```

The ADR-0122 wizard checks the node, the network, the model, the key, funds, the bond registry, the
artifact, capability and the fee output, then writes `~/.misaka/mining.toml`. Running it again
resumes from chain and local state. It registers through `kaspad --palw-register-bond`, so it gets
the operator-possession fix. It does not pass `--palw-bond-collateral`. It registers the node's
derived default and asks for funds to match: ≈ 31,191 MSK for the floor, and an unfundable
≈ 2,000,332,625 MSK when `--model` names the 8k class. To choose the amount — and always for a model
class — use the manual form.

### Manual, one shot

```bash
kaspad --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default \
  --palw-register-bond \
  --palw-producer-key=~/.misaka/miner.seed \
  --palw-bond-collateral=<sompi>
```

* `--palw-producer-pay-address` defaults to the key's own funding address. The collateral output pays
  to it, and the funding UTXO has to be there.
* Funding must be a mature, non-coinbase output that holds the collateral plus the carrier fee and
  change. Registration spends one input.
* `--palw-producer-class=<id>` sizes the default for a model class instead of the floor — for the 8k
  class that default is unfundable, so name the collateral whenever you pass it.
* The node prints `[palw-panel] registered bond <txid>:0 …` and **keeps running**: only its
  registration worker stops. Keep that outpoint, stop the process yourself before starting the
  producer (one process per bond), and check `misaka bond status --key-file …` shows it `REGISTERED`.

### Declare what the bond judges

A new registration declares no capability, and a bond that has declared nothing is never drawn onto
a panel. The declaration is a carrier of its own, and it needs **a second output at the key's
address**: the registration's change (`<carrier>:1`) is the node's fee float, which the node reserves
as soon as it persists it, and the CLI does not spend a reserved output. A key funded with one
transfer has nothing to pay the declaration with — send a separate ≈ 1 MSK first. For the floor, the draw requires the declaration. For a registry model class it requires a
fresh readiness (possession) proof, which the running panel submits by itself once it holds the
artifact.

```bash
misaka --network testnet-12 --output json model list   # the row with "base": true carries the floor's class_id
misaka --network testnet-12 bond capability --key-file ~/.misaka/miner.seed \
  --bond <txid>:<index> --class-id <floor-id> --declare <floor-id>[,<model-id>] --yes
# or, for a model class (declares it and names the artifact the verifier must load):
misaka --network testnet-12 palw panel join --key-file ~/.misaka/miner.seed \
  --bond <txid>:<index> --class <class-id> --artifact /path/to/artifact.palwart --yes
```

`mining setup` and `verifier setup` do this step for you. The declared set is replaced, not merged.

A seat is also drawn only when its free collateral covers the lock a `Valid` signature takes on that
claim (route matrix #3, `5a559459`). A bond below the lock is skipped, not drawn and then failed.

## 6. Start

### The floor

```bash
misaka --network testnet-12 mining start --print-command
misaka --network testnet-12 mining start
```

Equivalent manual shape:

```bash
kaspad --testnet --netsuffix=12 --appdir=~/.t12 \
  --listen=0.0.0.0:26311 --rpclisten-borsh=default --utxoindex \
  --palw-produce \
  --palw-producer-key=~/.misaka/miner.seed \
  --palw-producer-bond=<registered-bond-txid>:<index> \
  --palw-fee-outpoint=<mature-non-bond-txid>:<index>
```

For the floor, leave out `--palw-producer-class` and `--palw-class-artifact`. The floor is minted from
a seed on every node.

**The seat duties and the execution lane need no flag** (since 2026-09-25). A node that holds
`--palw-producer-key` and `--palw-producer-bond` always runs the panel's seat duties (receipts,
replays, readiness proofs, data-availability answers, the automatic filers and, with
`--palw-fee-outpoint`, the carriers) and the execution-lane round blocks that the chain's permits
grant this bond. No flag turns them off. `--palw-produce` is the only choice: whether this node also
opens attempt claims. The node prints one line at startup, `PALW duties (on by construction; …)`, that
names each duty and, for one that is idle, why. `--palw-panel` and `--palw-round-lane` are still
accepted. They do nothing and log one warning each.

Without `--palw-fee-outpoint` the seat duties still run, but the seat carries nothing on chain: no
data-availability answers, no filers, no carriers. The startup line then reads `panel seat duties ON
as bond … (receipts only: …)`. The fee outpoint is your consent to spend, so it stays a flag.

If the key file does not load or the bond does not parse, the node logs a second line after the
services start: `PALW duties NOT as planned: …`. That line names the duty that is not running.
`getPalwNodeStatus.panelRunning` also tells you whether the panel actually started.

**Run each bond in exactly one process at a time, and keep that process's app dir.** Because the
duties have no off switch, every `kaspad` started with a bond's key and outpoint runs that bond's
duties. This includes the execution lane's round blocks. Two such processes at the same moment sign
the same round permit over two different blocks, and the chain slashes the whole bond
(`RoundPermitEquivocated`). The following all count as a second process:

* a standby node or a copy of the node on another host;
* a `kaspad --palw-register-class …` run started next to the running node. It does not exit once
  the class lands. Register through the running node instead: use `misaka model add`, or restart
  that node with the flag (see `docs/palw-add-a-model-runbook.md` §5);
* the same node started again in a fresh app dir while the old one still runs.

The app dir also holds the record of the last round this bond signed (`palw-round-last-signed`).
If you wipe it and re-sync, the node can sign an unmerged permit a second time. Move or restore the
app dir instead of deleting it. The node prints this rule at startup (`PALW bond …: run it in exactly
ONE process …`). It prints it as a warning on a `--palw-register-class` run.

### The 8k model row (`Qwen/Qwen2.5-1.5B/graph-v7@8192`, class `ebf44d0a…`)

1. **Get the artifact.** Convert it from `Qwen/Qwen2.5-1.5B-Instruct` (`model.safetensors` SHA-256
   `dd924a11b4c220f385b51ffa522daea7c9f3d850e31b162bb5661df483c6d3ee`):

   ```bash
   qwen25-convert /path/to/Qwen2.5-1.5B-Instruct --a16 --n-ctx 8192 --out qwen25-1.5b-a16-8k.palwart
   palw-class manifest --network testnet-12 qwen25-1.5b-a16-8k.palwart          # writes the .palwmanifest sidecar
   palw-class manifest --network testnet-12 --check qwen25-1.5b-a16-8k.palwart  # exit 1 on a mismatch
   ```

   Expect 1,799,359,436 bytes and inventory root `88096dc1…`. The genesis card reads the same values
   from its committed sidecar. Keep the sidecar next to the artifact.
2. **Memory.** Size every node on the host with `--palw-host-memory-share=<bytes>` (or
   `--palw-host-memory-budget` ÷ `--palw-host-node-count`).
   * **An 8k seat needs at least 3.5 GiB (`--palw-host-memory-share=3758096384`).** A full-seat
     replay needs ≈ 3.37 GiB (artifact 1.68 + trace scratch 1.67 GiB). A seat with a 3.0 GiB share
     logs `readiness … no proof — a replay needs 3.37 GiB as full-seat …` and never becomes ready.
     Nothing else tells you, and a class that cannot reach 7 ready seats never leaves `Prefetching`.
   * An 8k producer peaked at 3.67 GiB RSS in the drill, which ran it with a 5 GiB share
     (`5368709120`).
   * `misaka mining start` does not pass this flag. Put it in `[advanced] extra_kaspad_args` in
     `~/.misaka/mining.toml`.
3. **Start.** Use `misaka mining setup --model <class-id> --artifact <path>`, then `mining start`, or
   add `--palw-producer-class=<class-id> --palw-class-artifact=/path/qwen25-1.5b-a16-8k.palwart` to the
   manual shape. Add `--palw-verify-class-manifest` to re-derive every sidecar at startup and refuse
   to start on a mismatch. That costs about 135 s per artifact at 2M, and less at 8k.
4. **A class that is registered after your build.** The genesis rows are in this build's tables.
   Serving a class that was registered permissionlessly and that this build has never heard of needs
   the chain-registered-class arm. On testnet-12 it is **always on** (the permissionless model registry
   is in force from genesis there), so pass no flag for it. `--palw-chain-classes` is deprecated on
   testnet-12, and `=false` no longer turns it off. testnet-11 keeps the arm off unless the operator
   passes `--palw-chain-classes`. With the arm, the node executes from the registered profile, and the
   panel's readiness proofs resolve through the chain's registration. A class is served only if its
   artifact is loaded.

**A producer refuses to start for a class it cannot produce.** For example, when no loaded artifact
matches the registered root, the node prints `--palw-producer-class: … This node will not start as a
producer.` to stdout and to the log, and exits 1. Fix the artifact, or drop the flag and produce for
the floor.

## 7. Observe

```bash
misaka --network testnet-12 mining status --watch 5
misaka --network testnet-12 doctor
misaka --network testnet-12 work list
misaka --network testnet-12 rewards
misaka --network testnet-12 palw registry                 # every class's lifecycle state, profile, ready seats
misaka --network testnet-12 palw panel list --class <id>  # bonded / ready / selected seats, receipts
misaka --network testnet-12 model readiness <class-id>    # each seat's proof age, collateral, why not ready
misaka --network testnet-12 palw round-lane               # the execution lane: stage table, permits, schedule
misaka --network testnet-12 dashboard --listen 127.0.0.1:8791
```

### The lane-mix alarm

Every testnet-12 node runs the lane watch, whether it produces or not. Once a minute it counts the
last 600 selected-chain blocks by lane: attempt (algo 6), receipt (7), execution (9), round (10) and
heartbeat (8). The walk follows selected parents only. Round blocks are never selected parents, so in
practice the work it counts is attempts and receipts. When there is none, it logs:

```text
[palw-lane-watch] no PALW work block in the last N selected-chain blocks (DAA a..b) — M of them are
heartbeats: the chain is running on its clock alone. Check that producers are running for a class that
can produce (the floor always can), that panel seats are up, and each producer's `holding:` line
```

The line is an ERROR, and it repeats as `still: …` every 10 minutes. It is not raised until the
window holds 30 blocks. `PALW work is back on the chain: …` clears it. The same figures are in
`getPalwNodeStatus` (v3: `laneWindowBlocks`, `laneWorkBlocks`, `laneHeartbeatBlocks`,
`laneLastWorkDaa`, `laneMix`, `laneAlarm`).

**What it means:** blocks are arriving and the DAA is moving, but no PALW work is on the selected
chain. The first testnet-12 deployment ran like that for 1,230 blocks while every liveness signal
looked green, because the heartbeat lane exists to keep the clock moving when nothing else does.
A new chain cannot confirm it is doing work until a floor producer is up with seats to judge it.

A producer that has held for 30 minutes logs `NOT PRODUCING for N min — holding: <reason>` at ERROR.
The reason is the part to read.

### `E-MODEL-NOT-ADMITTING`

`mining status` / `doctor`: **"Not mining: the chain admits no new claim of this class now."** The
chain's own class gate refused. The producer checks this gate before it spends an inference, so it
does not mine claims its chain would refuse. The class is in a lifecycle state that takes no claims:
`Candidate`, `Registered`, `Prefetching` or `Held`. Or it is in `Probation`, `ActiveLimited` or
`Active` but its ready seats have no room to verify another claim. The finding's `current:` line
carries the chain's detail. To see which case you are in:

* `misaka --network testnet-12 palw registry` shows the state and the ready seats.
* A `Candidate` waits for the next admission audit (§8).
* A `Prefetching` class waits for 7 ready seats. Check each seat with `model readiness`. The usual
  cause is a memory share below the replay (§6).

The finding's own hint says `misaka panel list`. The command is `misaka palw panel list`.

## 8. The model lifecycle

The floor (BASE-0) is always `Active`. A model class moves through these states at each span
boundary:

| state | admits claims | leaves when |
|---|---|---|
| `Candidate` (a class someone **registered**, i.e. bought) | no | the **admission audit** seats a jury: `seat_count` (5) operators drawn from the floor's population, not the class's, and a majority (3) of them hold the class → `Prefetching` |
| `Prefetching` (the **genesis** rows start here) | no | ≥ `required_ready_seats` (7) seats of distinct operators hold a fresh possession proof and enough free collateral → `Probation` |
| `Probation` | 50 ‰ of its derived admission | 10 probe claims reach `Final` with none failing → `ActiveLimited` |
| `ActiveLimited` | 100 ‰ | 3 stable lifecycle steps → `Active` |
| `Active` | 1,000 ‰ | — |
| `Held` | no | ready seats are back to enough and the class is not overloaded → `Probation` |

A class that drops below 5 ready seats (no panel can be drawn), or that overloads its receipt window,
goes to `Held`.

* **The admission audit runs once every 100 DAA on testnet-12** (`Params::palw_admission_audit_period_daa`
  = `PALW_ADMISSION_AUDIT_PERIOD_DAA_STANDARD`; with `span_daa = 1` that is every 100th span), which
  is about 3 hours 20 minutes at the 120-second cadence. ADR-0147's own default was one epoch
  (1,000 DAA, about 33 hours); 100 DAA is the standard since 2026-09-23. A class registered just
  after an audit waits up to that period, even if its seats were ready within minutes. That is by
  design (ADR-0147): a lottery that could be re-drawn every span would be passed by waiting. There is
  no flag to shorten it.
* **The genesis rows** (8k `ebf44d0a…` and 2M `74c67e63…`) skip `Candidate`. They open at
  `Prefetching` and reach `Probation` on any span once 7 seats are ready. The registry starts
  counting at DAA 30 (its readiness grace). With 8 genesis cards, the 8k row needs 7 of them, or
  third-party bonds, running 8k seats with a ≥ 3.5 GiB share.
* A class in `Candidate` or `Prefetching` can still produce a block. The block is valid and stands,
  but its claim is refused as work: `class … is Candidate under the model registry and admits no new
  claims`. It earns nothing and carries no weight. The producer's gate now stops this before the
  inference.
* **Held hybrid rows (Qwen3.6-35B-A3B graph-v7)** register and certify, but every attempt fails at
  prefill position 15 until the held map is fixed. See the deployment record.

Registering a model is covered in [`palw-add-a-model-runbook.md`](palw-add-a-model-runbook.md).

## 9. Stop safely

```bash
misaka --network testnet-12 mining stop --drain
```

Drain stops new work and keeps the process up for the claims, panels and court duties that are still
open. **On testnet-12 a producer that disappears is charged.** Past `palw_audit_2026_09_23` two voids
forfeit weight + escrow (≈ 3,200.85 MSK per floor claim), the same amount a proven fraud forfeits:

* a `ProducerWithholding` void, where the data-availability court confirms the default;
* the second `ReceiptTimeout`, where two independently drawn panels could not conclude.

This happens whether the producer withheld on purpose or its node was simply down.
`BindTimeout` and `NoCapablePanel` are not charged. `--force` is for emergencies only and can abandon
these duties.

## 10. Epoch budgets

testnet-12 arms ADR-0123's `palw_epoch_budget_release` from DAA 0. A class whose epoch budget is spent
borrows the epoch slots that other classes are not filling. On testnet-11 this is still dormant. A
class registered mid-epoch has budget 0 until the next boundary. `kaspad --palw-dump-classes` logs
every class's share and budget.
