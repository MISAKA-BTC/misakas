# testnet-12 regenesis — deployment record, 2026-09-23

> **状態（2026-09-23 更新）: 起動は保留中。** 初回配備の直後に 2 つの欠陥が見つかり、運用者の指示で再配備を止めて設計から直している。
>
> 1. **担保の単位** — genesis の carve が「クラス自身の declared leaves」単位、runtime の予約は「floor 正規化後の導出値」で 44.25× 差。全 producer が `holding: the bond's exposure ceiling leaves no room for another claim` で `produced=0`。担保 60,088.18 → 516,429.80 MSK/seat（`2bd134ec`）。
> 2. **class root** — pin した値が operand-inventory root ではなく flat `artifact_digest`（t11 と同じ事故の再発）。全 seat が `holds no artifact whose registered root form is b5baca63…`。
>
> 対処（項目 1〜5 完了、6 進行中）: inventory root の streaming 化（`216641a4`）／`ArtifactDigest`・`InventoryRoot`・`ClassId` の型分離＋`.palwmanifest`＋`palw-class manifest`（`ea7ad7df`）／runtime が sidecar を読み `--palw-verify-class-manifest` で fail-closed（`26030bc1`、`2f588a58`）／genesis の手書き定数を廃止し commit した manifest を `const fn` で読む（`077d4c7f`、root `b5baca63…`→`f63af2c4…`）／per-host メモリ予算 `--palw-host-memory-budget` / `--palw-host-node-count`（`a0350833`）／phase 別メモリ分解＋60 s 周期行（`b9b5ed07`、`e40dfcd2`）／1-seat acceptance が暴いた OOM 経路 3 つの修正（`427a1b8d`、`e40dfcd2`）。
>
> **genesis ブロックは不変**（root は PALW bundle の `genesis_objects` にあり header/premine に入らない）。params fingerprint は `fb8f378d…` → `c746f07c…`（担保）→ `30848c6b…`（root）→ **`bcfbf2a3…`**（t11 で genesis 有効だった `palw_unavailable_abstains` が t12 で dormant に退行していたのを武装、`t12_arms_every_fence_t11_armed` が台帳として常駐）。fence schedule は `1000` の 1 本のまま。
>
> **engine 側（`feat/kv-codec` を `5451aac2` で merge、fingerprint 不変）**: K/V は実測で全要素が ±32,767 内（`clamp16` の帰結）なので **A16-KV-i16 は無損失**の再パックとして出荷既定に。i8 は 89% が再量子化＝クラス変更で名指し拒否。2M attempt の working set 14.84 → 7.84 GiB、fleet 上では +file 2.67 +rope 1.07 で ~11.6 GiB — 等分 share には収まらず paged KV（未着手）が要る。役割別 resource profile（producer/full/partial を 1 導出から）、node-local 予約台帳（RAII・拒否は保持者を名指し）、S1 partial seat の prefix 再実行、telemetry（RPC/CLI）。seat ホストの非対称トポロジ（producer 1 + panel 3）向けに `--palw-host-memory-share`（`4e05f85d`）で per-process share を明示可能。merge 後の全スイート緑（base0 475 / consensus-core 2,582 / kaspad 122 / sdk 35 / rpc-core 199）。
>
> 現在のフリート: 5.104.81.23 の 4 seat は**停止**（unit は残置、script は宣言予算に置換済）、.113 と ibm の public node は旧 fingerprint `c746f07c` で稼働中（heartbeat のみ）、seeder 4 本稼働。**再起動は項目 6（1 seat → 2 → 4 の acceptance と PSS 線形性）完了後、drill → 4 ホスト配備 → seeder 切替の順。** 以下は初回配備時点の記録。

Build `de857a71` (`kaspad v1.1.0-de857a71`), a release build of `feat/testnet-12-regenesis` — **初回配備時のビルド。設計修正後の tip は上の状態欄の commit 群を参照。**

## The network

| | |
|---|---|
| genesis hash | `a8cabac47b96fe30d9675ce08a355f62e6d57aac84865b0d6295c024c6d8ff61fb2d93943952e8da4c36ff87ea7f17f6c8de643fa7b02ecf7512892d590777dd` |
| params fingerprint | `fb8f378df6373455e0c14d08c835184b86717ae3fc983039f9ded633be7fa38d` — **初回配備時。現在は `bcfbf2a3874c4630cc45f6e1d375f2b261873143bdc942ac6ba78ca16e0c104d`**（上の状態欄） |
| fence schedule | **`1000`** — one height, ADR-0065 D1's bond-maturity window (t11's was `1150, 1900, 2150, 2400, 3500, 4000, 6900, 7100, 7101, 7200, 7301, 8000, 2125000`) |
| rule manifest | `… palw_work_target=1 palw_independence=1` (digest `9def81a1…`) |
| premine | 33 outputs, exactly 10B MSK: 8 collateral + 8 fee floats + **16 community (758M)** + main wallet |
| bond collateral | 60,088.18407600 MSK a seat (8 seats = 0.0048 % of the cap) — **初回配備時。現在は 516,429.79663480 MSK a seat（8 seats = 0.0413 %）**、runtime が予約する単位で積み直したもの（`2bd134ec`） |
| classes | BASE-0 floor + held Qwen3.6 `e108e736…`@512 + held Qwen2.5 `74c67e63…`@2,097,152, **each at the minimum grantable share** |
| P2P port | 26311 — unchanged from t11, by the operator's decision |

## Hosts

| host | unit | appdir | listen | bond / fee |
|---|---|---|---|---|
| 169.58.232.113 | `misaka-t12-node` | `.t12` | 0.0.0.0:26311 | 6 / 47 |
| 169.58.39.220 (ibm) | `misaka-t12-node1` | `.t12b` | 0.0.0.0:26321 | 1 / 42 |
| 5.104.81.23 | `misaka-t12-seat2..5` | `.t12`, `.t12b`, `.t12c`, `.t12e` | 127.0.0.1:26311/21/31/41 | 2..5 / 43..46 |
| 95.111.236.186 | seeder only | — | — | — |

Seeders, all four: `misaka-dnsseeder-t12` (`--network-id testnet-12`), binary `1174b965…`.

**The t11 units, launch scripts and datadirs are all still in place.** Rollback is: stop the t12 unit,
`systemctl enable --now` the t11 one.

## What the switch did NOT change, and why that matters

Every PALW flag on every host carried across untouched — `--palw-producer-class=74c67e63…`, the 2M
class artifact, `--palw-producer-key=/etc/misaka/t12/t12-bond-N.key`, `--palw-producer-bond=…:N`,
`--palw-fee-outpoint=…:4N`, and all ports. testnet-12 re-uses the same eight genesis bond cards, the
same premine sentinel txid and indices, and registers the very class the fleet was already mining.
Only the network's name, its genesis and its datadir changed. `the_fleets_premine_outpoints_are_unchanged`
pins the bond↔float pairing (1→42 … 6→47) so a future change cannot break the submitters silently.

## Three things this deployment caught

1. **The old DNS seeder advertises the wrong port on testnet-12.** Measured, not assumed:
   `misaka-dnsseeder --network-id testnet-11` health-checks anchors on `:26311`, and the same binary
   with `testnet-12` checks `:26411` — the `None | Some(_)` fallback in the pre-regenesis
   `NetworkId::default_p2p_port`. Switching the seeders' flag without replacing the binary would have
   pointed every new user at a dead port. The rebuilt seeder answers `:26311` for both.
2. **Stale August `testnet-12` datadirs, with consensus data.** `/root/.t12*` existed on 5.104.81.23
   (64M + 48M + 53M) and 95.111.236.186 (87M) from the internal relaunches the suffix was minted for
   in August. Moved aside as `.aug2026-regenesis-bak-<ts>`, never deleted. Starting on one would have
   meant either a genesis-mismatch refusal or, worse, a silent resume of a dead chain.
3. **A plain `SIGTERM` can leave kaspad hung after it has closed its listeners.** Observed in the
   drill: the node logged `SIGTERM - shutting down…`, stopped its P2P/gRPC/wRPC servers, and then sat
   in state `S` for 19 minutes ignoring both SIGTERM and SIGINT — a process that looks alive and
   serves nothing. The live units are already protected (`KillSignal=SIGINT` + `TimeoutStopSec`, which
   escalates to SIGKILL), and the switch confirmed it: **no hung t11 node was left on any host.** A
   bare `kill` in a script is not protected.

## Drill evidence

Same binary as shipped. `consensus/tests/palw_t12_liveness.rs` and
`consensus/core/tests/t12_economic_safety_drill.rs` cover the rule-level half; this is the live half.

* **boot** — both nodes loaded the genesis, peered, and `[palw-heartbeat-miner] heartbeat #1 … the
  clock ticked` inside 35 s. "**bondless** heartbeat lane (ADR-0060), fee-only" is ADR-0151 D3 in the
  node's own words: the lane that carries this chain's clock takes no bond, so no collateral figure
  can stop it.
* **restart** on the same datadir — ticks continued, 0 errors.
* **IBD** — a third node with no datadir accepted 8 blocks and started minting, 0 errors.
* **partition** — with its only peer gone, the survivor kept ticking (76 beats alone). This is the
  liveness property the collateral reduction rests on, observed under partition.
* **join as a user would** — a fresh node with no `--addpeer` and no `--connect` queried the four DNS
  seeders, got addresses from seeder1 and seeder3 (seeder2/seeder4 are not delegated in public DNS),
  connected to **both** public nodes (`169.58.232.113:26311` and, once peer exchange gave it the port,
  `169.58.39.220:26321`), accepted blocks via relay and tracked the tip (work 21 → 23), with 0 network
  mismatches. **That is the goal's endpoint: a user needs nothing but this build and `--netsuffix=12`.**

**Still owed** (ADR-0151 §4): a forced reorg, and the class-at-cap states on a fleet that is actually
producing model blocks. The arithmetic for every registered class is covered by the drill test; the
reachable-state search over a live reorg is not.

## Operational notes

* **External t11 users are now refused, by design.** ibm logged
  `handshake failed … Network mismatch - local: misaka-testnet-12, remote: misaka-testnet-11` from
  three Japanese IPs within minutes of the switch. The network-id gate is working; those participants
  need this build. **An announcement is the operator's to make.**
* Only `169.58.232.113:26311` is reachable at the default P2P port. ibm's node listens on 26321, so a
  client that picks it from a seeder answer and dials the default port fails and must retry. This is
  the topology t11 had, not a regression, but it means one entry point.
* `/root/misakas-stale-consensus-diagnosis` is still running a t11 fixture node on 5.104.81.23 on its
  own ports and datadir. It belongs to another session's work and was left alone.

## Fleet state at the end of the switch

| host | node | seeder | blocks | peers | fatal |
|---|---|---|---|---|---|
| 169.58.232.113 | active | active | 26 | 3 | 0 |
| 169.58.39.220 (ibm) | active | active | 40 | 10 | 0 |
| 5.104.81.23 | 4 seats active | active | — | — | 0 |
| 95.111.236.186 | (seeder only) | active | — | — | — |
