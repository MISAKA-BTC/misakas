# testnet-11 — history

**This page is a record, not an entry point.** testnet-11 (Relaunch 5f, 2026-09-03 → 2026-09-25)
was the public network before [testnet-12](../t12-launch-2026-09-25.md). Current `main` no longer
builds a testnet-11 node: to run one, build commit **`1f98d3bf4`** (the last testnet-11 `main`) and
select it with `--testnet --netsuffix=11` or `misaka --network testnet-11`.

What follows was the top of the repository README until 2026-09-25, moved here unedited apart from
link paths and two headings, so the flag days, fingerprint changes and rollout instructions stay
readable and linkable. "Above" and "below" in the text refer to that README. Measured hashes, DAA
scores, peers and commands describe testnet-11 at the time and are not current operator defaults.

> [!NOTE]
> **testnet-11 (previous network; status 2026-09-22).** Relaunch 5f. **Current `main` no longer
> builds a testnet-11 node:** this release moves testnet-11's identity (the fifth certified family,
> state v21/v22), so a `main` build is refused by testnet-11 peers. To keep running testnet-11, build
> commit **`1f98d3bf4`** (the last testnet-11 `main`), select it explicitly with
> `--testnet --netsuffix=11` or `misaka --network testnet-11`, and verify fingerprint
> **`79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640`** (verified on 2026-09-22). Its fence schedule is `1150, 1900, 2150, 2400, 3500, 4000, 6900,
> 7100, 7101, 7200, 7300, 7301, 7780, 7800, 8100, 8160, 8500, 8600, 8700, 2125000`. The held
> regime and the audit's deep fixes are at DAA 7,100; the DAA 7,101
> flag day carries ADR-0130's operator lottery and 5-DAA spans, the model registry, the economic
> payout, the work target and the short challenge window; ADR-0133's Verification V2 (S1) is at
> 7,200; ADR-0130's span-short is at 7,300; ADR-0134's compute-overlay retirement is at 7,301; the
> 7,000 release prints `ae1d6162…`.
> The network produces PALW blocks at a frozen 120-second cadence. Testnet-10 and older relaunches
> are not supported entry points. ADR-0123's epoch-budget release is implemented but remains
> dormant on every shipped preset (`palw_epoch_budget_release: None`).
>
> **Rebuild before DAA 7,100.** At DAA 7,100 the held regime and the audit's deep fixes arrive (the
> deployed release scheduled them at 7,000 and is refused from 7,100, a height it does not name);
> from DAA 7,101, together:
> * **Panels are paid** (ADR-0124): 20 % of a `Final` claim's reward goes to the seats whose `Valid`
>   receipts the chain credited, a drawn seat holds three times the claim's exposure, and a claim is
>   paid for the compute it certifies.
> * **The execution lane opens at one block a second** (ADR-0125): round blocks carry transactions
>   between the 120-second PALW blocks. They add no confirmations: a payment is as final as the
>   settled PALW anchors after it (ADR-0127/0129) — `misaka palw settlement --daa <d> --min-depth <n>`
>   waits for them, and `misaka wallet utxo list` shows each output's depth.
> * **Validators receive 20 % of a block's subsidy instead of 30 %** (ADR-0126); the tenth is escrowed
>   for the block's PALW claim and paid at `Final`.
> * **A block buys one unit of work from any model** (ADR-0137): a block draws against one
>   network-wide work target `W` — `CCU/W`, the class's counted compute against the block's own
>   floor — and a class's *share* becomes a result the readers report, not an input to the draw. No
>   class share, class target, epoch budget or seat price is read past this height.
>
> **Two rules are NOT on this day**, and the reason is a measurement rather than a schedule.
> ADR-0132 S's single lottery and ADR-0138's anchor clock arm together, and arming them would have
> stopped the chain's clock: testnet-11 has no `bits`-priced producer — 60 of its last 60
> selected-chain blocks are the model lane — so past the anchor clock only a heartbeat can advance
> the DAA score, and the rule that admitted heartbeats measured them against a parent every new
> block replaces. A chain producing faster than the interval suppressed the lane entirely, and the
> registry drill reproduced it: the clock frozen at its own flag day, the miner running, nothing
> minted. [ADR-0142](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md)
> is the rule that fixes it — built and drilled, armed nowhere — and these two follow it on a later
> day. Nothing else in the upgrade depends on them.
>
> * **The execution lane's gas scales with the lane** (ADR-0139): each distinct permitted round a
>   chain block merges adds 3,000,000 gas to what that block may accept, under a 390,000,000
>   ceiling — so "one block a second" is one second of transactions, not one second of scheduling
>   against a 120-second gas budget.
> * **The permissionless model registry opens** (ADR-0135): anyone may register a model class, its
>   profile is derived from its graph, seats prove they hold its artifact, and a class walks a
>   lifecycle that holds it back when its panel cannot verify it. A claim is challengeable for 120
>   DAA instead of 1,200 (ADR-0132 §7.6).
> * **Validators vote BFT by bonded stake** (ADR-0128): an anchor is DNS-final when more than two thirds
>   of the counted stake has attested and precommitted to it, and the DNS stake reorg gate refuses
>   chains that abandon it. Validators restart on this build and precommit without new flags
>   ([docs/validator-runbook.md](../validator-runbook.md)). PALW production and settlement do not
>   depend on validators; the gate is a veto layered on top.
>
> **From DAA 7,200**, two more, both of ADR-0133: a receipt may attest the segments of a job rather
> than the whole of it (Verification V2, S1), and a seat's possession proof becomes a multiproof over
> sixteen leaves drawn from the whole artifact instead of one leaf of a contiguous window.
>
> These are consensus rules: a node on the 7,000 release keeps peering below 7,100 and is refused
> from 7,100; its own 6,900 and 7,000 are never reached by it.
>
> **Before it was armed**, this bundle was audited twice against the code rather than the design:
> a pre-arming security audit ([docs/palw-audit-2026-09-18-6001.md](../palw-audit-2026-09-18-6001.md),
> two Critical and six High findings, all fixed) and a DAA-clock audit
> ([docs/palw-daa-clock-audit-2026-09-18.md](../palw-daa-clock-audit-2026-09-18.md), which is why
> ADR-0138 exists and what it deliberately does not close). The release report
> ([docs/palw-release-6001-verdict-2026-09-18.md](../palw-release-6001-verdict-2026-09-18.md))
> records the gates on the frozen candidate, the three holes the bundle's own fixes opened, the
> measured cost of a block at the 390,000,000 gas ceiling, and the drill that armed the verdict.

<details>
<summary>Historical Relaunch 5f rollout log and crossed fences</summary>

> **Status (recorded 2026-09-12).** The live public network is **`testnet-11`** — the PALW release candidate,
> Relaunch 5f. Explorer at **[misakascan.com](https://misakascan.com)**, web wallet at
> **[wallet.misakascan.com](https://wallet.misakascan.com)**. Current network identity: consensus
> fingerprint **`ae1d6162…`** (the release that schedules ADR-0120's fence at DAA 6,900 and held
> context's and the audit's deep fixes at DAA 7,000 — moved to 6,000 on 2026-09-17 and to 7,100 on 2026-09-18, see above — on
> top of 3,500 and 4,000; the 3,500 + 4,000
> build prints `4300409b…`, a build with 3,500 alone `02c7282b…`, the builds before it `ecbdbc22…`,
> and `891a1a14`…`a5f1bdf7` print `060e3597…`) — and genesis
> **`ad30b5cb…`** (three execution classes and the 347M MSK community allocation in genesis).
>
> **Rebuild before DAA 6,900 — one build covers 6,900 and 7,000.** (As recorded on 2026-09-12; the
> 7,000 heights moved to 6,000 on 2026-09-17 and to 7,100 on 2026-09-18.) From DAA 6,900 a model's store opens
> only once 1,000,000 MSK is locked into it instead of 100,000 (ADR-0120; pledges made before stay and
> count toward it, and a store already open stays open). From DAA 7,000 classes that hold their context
> off the chain become usable (held context, ADR-0118/0119/0121), and the rest of the pre-mainnet
> audit's fixes apply (`palw_audit_2026_09_11_deep`: court and panel hardening). Both are consensus
> rules: a node without them keeps peering until their height and forks off at it; the `4300409b…`
> build forks off at 6,900.
>
> **In force since 2026-09-12: DAA 3,500 and 4,000.** From DAA 3,500 the model store's
> owner fee on every join and leave is 5 % instead of 1 % (ADR-0114; the 5 % burn is unchanged). From
> DAA 4,000 the pre-mainnet audit's fixes apply (`palw_audit_2026_09_11`: a class-activation crossing
> can no longer halt the chain, a lifecycle payload a build cannot decode no longer invalidates its
> block, a certified quantum spent by conflicting receipts is paid once rather than once per receipt,
> and a court move the fold refuses no longer uses up the block's court slot), and an attempt's draw is one forward — its job
> has no decode calls (ADR-0117, `palw_prefill_draw`). Both are consensus rules: a node without them
> keeps peering until their height and forks off at it. Rebuild from `main`, restart, keep your
> appdir, and check the startup lines below — a node that already rebuilt for 3,500 alone
> (`02c7282b…`) must rebuild again before 4,000.
>
> **Build from `main` at `891a1a14` or later.** The network crossed a fourth fence — ADR-0095's
> model benefits at **DAA 2400** — on 2026-09-09, and every node built from an earlier `main` has
> been on its own arm since DAA ≈2,241: refused at the handshake (by its own older gate), its
> virtual DAA far behind the explorer, `Seeder mesh overlap: NONE` in `misaka node doctor`, and its
> address listed under *Refused at handshake* on the explorer's
> [Peers page](https://misakascan.com/#/peers). **The fingerprint did not change** across that fence
> — until 2026-09-11 it left ADR-0095's fence out — so it could not tell you which side you were on;
> the build above prints, on the line after it,
> `Consensus fence schedule: 1150, 1900, 2150, 2400, 2125000`, and that line is the check. Do **not**
> run `7f4dded4` (the first build with that fence): it cannot sync from an empty datadir. To rejoin,
> rebuild, move your `datadir` aside (keep the rest of the appdir) and resync — the steps are in
> [the node-operator doc](../testnet11-node-operator.md). Validators: restart yours once the node
> is synced. DNS finality stalled from DAA 2,246 while most bonded stake attested on the dead arm; it
> recovered on the network's chain on 2026-09-10, and the EVM bridge carried its first deposit the
> same day.
>
> The chain was re-minted 2026-09-03 (Relaunch 5f) — no flag day since has re-minted the genesis.
> Four fences are scheduled on it: ADR-0083's at **DAA 1150**, the DA court, private prompts and the
> model market at **DAA 1900**, ADR-0084 U-08's refutation ladder at **DAA 2150**, and ADR-0095's
> model benefits at **DAA 2400** — all four crossed. A build missing any of them is refused, and the
> refusal arrives when the first node carrying the new fence connects, not at its height.
>
> A node with state older than 5f must wipe its appdir and resync; a node on an older ruleset is
> refused at handshake. The values to trust are the two your own node prints on startup, not the
> ones on this page.
> `testnet-10` has been **stopped**; its parameter set still exists so historical data can be read,
> but nothing operates it and its public entry point is closed.
>
> PQ-only consensus and the DNS-finality reward overlay are **active from genesis on every defined
> network** (`pq_activation_daa_score = 0`, `dns_activation_daa_score = 0`). The `mainnet` parameter
> set is **defined but NOT launched or endorsed for production** — do not run `--mainnet` expecting a
> live or supported network.

</details>

## What testnet-11 was

testnet-11 runs **PALW ConsensusV2** (ADR-0042): blocks are won by a lottery over *verified LLM
inference* rather than by hashing alone, and the work a block claims is settled by other nodes
re-deriving it. Three things follow that a hash chain does not have.

**A block's work is a claim, and claims are judged.** A producer publishes an execution and the
material behind it; a panel of bonded seats re-runs the job and files signed receipts; a licensed
claim can still be disputed, and a dispute is settled by bisecting to a single arithmetic step and
adjudicating it. Weight is credited only once that lattice turns over — `safe_weight` moving off
zero is the network working, not a formality.

**Producing needs a bond.** Attempts name a bond the chain holds, so an unregistered node can sync,
serve and verify, but not produce. The genesis registry seats the initial set.

**The cadence is frozen at 120 s per block** (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`), refused at
parameter construction if anything tries to change it. Inference takes real time, and a block
interval shorter than the work it certifies is a chain that certifies nothing.

Three execution classes ship in the genesis:

| class | model | share | needs a model file? |
|---|---|---|---|
| **PALW-BASE-0** (the floor) | deterministic integer model, pure Rust in this tree | 22‰ | **no** — no GPU, no download |
| **QWEN25-A16** | Qwen2.5-1.5B-Instruct, A16 static-PTQ conversion | 489‰ | yes (use the class-bound artifact) |
| **QWEN36** | Qwen3.6-abliterated-35B-A3B (Q4_K_M), hybrid runtime | 489‰ | yes (~34 GiB, download or convert) |

Running or verifying a node needs none of them for the floor; producing in a model class needs that
class's artifact. Both model artifacts derive deterministically from public weights on
[Hugging Face — Misakachain/Qwen3.6-35B-A3B-PALW-runtime](https://huggingface.co/Misakachain/Qwen3.6-35B-A3B-PALW-runtime),
and every panel seat re-derives the same bytes or the class does not license
(see [docs/palw-public-testnet-classes-runbook.md](../palw-public-testnet-classes-runbook.md)).

Since ADR-0058 a block does not need to win tip selection for its work to count: the whole
mergeset — reds included, which at 120 s cadence and `ghostdag_k = 1` is every block of every
class slower than the floor — creates claims, is verified, is paid, and moves the per-class
difficulty and share. A slow class starves no more.

## Joining testnet-11 (as of 2026-09-22)

The recommended operator path is the ADR-0122 CLI. It verifies the node, identity, model, key,
funds, Bond registry, artifact, panel capability and fee output before writing
`~/.misaka/mining.toml`:

```bash
misaka --network testnet-11 mining setup
misaka --network testnet-11 mining start --print-command
misaka --network testnet-11 mining start
```

For an existing Bond, inspect the exact outpoint without a key or class id:

```bash
misaka --network testnet-11 bond status --bond <txid>:<index>
```

`REGISTERED` and sufficient sustained collateral are separate results. Never re-run
`--palw-register-bond` for an already registered key; collateral cannot be topped up and the
append-only registry refuses a second Bond from the same key.

The network is permissionless. DNS seeding is live (`seeder1.misakascan.com`), so a fresh node
needs **no flags beyond the network selection**:

```bash
cargo build --release -p kaspad
./target/release/kaspad --testnet --netsuffix=11 --utxoindex
```

The log must show this fingerprint and, on the next line, this fence schedule, or you are on the
wrong ruleset:

```
Consensus params fingerprint: 79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640 (network testnet-11)
Consensus fence schedule: 1150, 1900, 2150, 2400, 3500, 4000, 6900, 7100, 7101, 7200, 7300, 7301, 7780, 7800, 8100, 8160, 8500, 8600, 8700, 2125000 (schedule id …)
```

> **The fingerprint moved on 2026-09-20 (execution span-short at 7,300).** The identity did not:
> a node on `137b9c50…` still peers until DAA 7,300. One without `7300` in the schedule is refused
> from that height. The 8,000 clock build printed `137b9c50…`; ADR-0150's readiness-R2 build printed
> `c3a5e91d…`.

A build from `891a1a14` up to `a5f1bdf7` prints `060e3597cd2950bc…` on the first line and the same
second line. It runs the same ruleset: until 2026-09-11 the fingerprint left ADR-0095's fence at 2400
out (the build that added it never wrote it), and writing it moved the value without moving any rule.
The two stay peers — the handshake logs `schedules a FUTURE fence differently` between them rather
than refusing — but rebuilding is how the first line becomes a check again. The second line is the
one that names heights: a build without `3500` in it forks off at 3,500, one without `4000` at 4,000,
one without `6000` at 7,100 (the 7,000 release, `ae1d6162…`, which names 6,900 and 7,000 instead),
one without `6001` at 7,101, one without `7300` at 7,300, one without `6201` at 7,301. One case the second line cannot show:
`4000`, `6000` and `6001` each carry more
than one fence, and a build with only some of them (at 4,000, the audit's
alone prints `09efd285…`) shows the same line and parts silently at that height — the first
line is the check.

and the genesis the network builds on is
`ad30b5cb965ad305…` (the node prints it in any `Genesis mismatch` warning). If your log shows
**`Genesis mismatch … local: d25a80b9…`**, your datadir holds a RETIRED pre-relaunch chain —
most likely synced from a stale node that was still answering on the network name (observed live
2026-08-28). The current network refuses that history at handshake, so the node sits peerless
forever. Recovery: stop the node, delete the app dir (`~/.kaspa-pq/misaka-testnet-11` or your
`--appdir`), rebuild from current `main`, and resync — the real chain re-downloads in minutes.

If DNS seeding is blocked where you run, resolve a seeder once and pass the result for that
invocation (the peer flag currently accepts IP addresses, not hostnames):
`SEEDER_IP=$(dig +short A seeder1.misakascan.com | tail -n1); kaspad ... --addpeer="$SEEDER_IP:26311"`.
Do not copy that resolved IP into permanent configuration; the address behind a seeder is
operational state and can change or be withdrawn.

| I want to… | read |
|---|---|
| run a node / verify the chain | [docs/testnet11-node-operator.md](../testnet11-node-operator.md) |
| join as a PALW verifier / panel seat | [docs/testnet11-verification-participation-ja.md](../testnet11-verification-participation-ja.md) |
| produce blocks (floor class, no model needed) | [docs/testnet11-join-mining.md](../testnet11-join-mining.md) |
| produce or verify with the LLM classes | [docs/palw-public-testnet-classes-runbook.md](../palw-public-testnet-classes-runbook.md) |
