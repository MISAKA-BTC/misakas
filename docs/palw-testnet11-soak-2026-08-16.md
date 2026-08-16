# PALW testnet-11 fleet soak — 稼働記録 (2026-08-16 開始)

公開テストネット Track A ゲート4。TN11 (公開testnet形状: T=120s, PALW-4 worker flavor,
fingerprint `62781823…`) を **live t10 fleet 3 host上の隔離チェーン**として起動し、
複数miner・実難易度・多日soakにかける。

## 構成

| host | 役割 | ノードcap | miner cap | 備考 |
|---|---|---|---|---|
| ibm 169.58.39.220 | node + miner (build host) | 6G | 3500M | star中心 (A/Cがdial) |
| C 5.104.81.23 | node + **先行miner** | 6G | 3500M | ufw deny→dialする側 |
| A 160.16.131.119 | node + miner | 5G | 3500M | ubuntu + sudo systemd |
| B 95.111.236.186 | **不参加** | — | — | disk 100%満杯 (下記) |

- binaries: commit **7c8afbf** の `git archive` からibmでビルド (kaspad `ceea673e…`,
  misaminer `e9372a5c…`, 3 host sha一致)。**working treeを配布しない** — 並行セッションの
  in-flight編集 (palw_carriage.rs) を検出したため、tested-commit原則で隔離した。
- x86でconsensus-coreスイート実行: **572/573** — 唯一のFAILは
  `palw_reference::tests::sqrt_matches_hardware_exactly` (別レーンのSoftFloat参照実装が
  x86 hardware sqrtと不一致。PoW/soak経路はpalw_reference不使用のため無関係。task chip起票済)。
- worker/GGUF: gate-2で校正済みの `~/palw-class/palw-worker` (2bd857f8…) +
  **永続化したGGUF** `~/palw-class/Qwen3.5-2B-Q4_K_M.gguf` (/tmp reboot消失対策済)。
- 資源封じ込め: 全unit systemd transient + MemoryMax + MemorySwapMax=0 —
  **死ぬのは実験側、本番kaspadは常に無傷** (gate-2の教訓の恒久化)。

## 起動時に踏んだ事実

1. **algo-5時代のfleetテストノード残骸が3 host全部で4日間生存していた**
   (appdir `.palw-fleet`, port 37711, unsaferpc 0.0.0.0公開)。「テストノード停止済」の
   記録は誤り。appdir確認の上で3台とも停止 — 37711解放とBの資源回復を兼ねる。
2. C は ufw default-deny — 着信不可、**Cが常にdialする側** (旧h3と同じ)。
   トポロジは ibm を中心にした star (C→ibm, A→ibm)。
3. 3ノードとも起動ログの `Consensus params fingerprint` = pin値 `62781823…` 一致。

## soakが検証するもの

- x86 classでの **T=120実難易度の相互replay** (miner多重化、fork/merge、orphan/unorphan)
- worker長時間安定性 (数千推論/日/host)、メモリ挙動 (cap内で完走するか)
- 0x200ccccc genesis bits の launch挙動 (gate-3の「収束難易度近傍スタート」推奨の実地確認)
- ※ pruning-proof cliff (既知の限界①) は T=120 では 10800ブロック≈15日先 — このsoakの
  範囲外 (devnetの短周期で別途試験可能)

## bring-up 実績 (2026-08-16 20:10-20:35 JST)

1. 3ノード起動、全hostの起動ログ指紋 = pin `62781823…` 一致。
2. **C単独miner先行**: TN11初ブロック誕生 (genesis bits 0x200ccccc、C実測~13.6s/attempt)。
   3ブロック採掘 → **A・ibmが実難易度のPALW replayで3/3受理 (RELAY-VERIFIED)** —
   これが cross-host T=120 実難易度検証の初成立。
3. その後 A・ibm のminerを起動 → **3-miner soak状態**。1h毎の自己記録watch
   (`palw-soak-watch` transient unit → `~/.palw-soak/status.log`) を3hostに配備。
4. canary確認: live t10 kaspadは3 hostとも無傷 (A 1d18h / C 2d11h 連続稼働;
   ibmのlive t10は本作業開始前 ~17:07 に第三者要因で再起動されていた — 本soakと無関係)。
   Cの soak kaspad.log の panic 4行は初回ポート衝突 (旧fleet残骸) 時のもので、
   再起動後は0。

## 操作

```bash
# 状態 (どのhostでも)
ssh <host> 'bash ~/palw-soak/misaka-palw-soak-status.sh'
# 停止
ssh <host> 'sudo systemctl stop palw-soak-miner palw-soak-node'   # Aはsudo付き, root hostは素
# 再開 (ノード→minerの順)
ssh <host> 'MEM_MAX=6G BIN=~/palw-soak/kaspad PEERS="…" bash ~/palw-soak/misaka-palw-soak-node.sh'
ssh <host> 'RIG=<name> BIN=~/palw-soak/misaminer bash ~/palw-soak/misaka-palw-soak-miner.sh'
# 時系列ログ (1h毎の自己記録)
ssh <host> 'tail ~/.palw-soak/status.log'
```

## B (95.111.236.186) の状態 — 運用者判断待ち

disk **100%** (473MB free / 193G)。内訳: `/root/kpq-testnet-t10` 36G (live t10 —
触るな) / **`/root/palw` 15G (旧PALW артефакты — 掃除候補)** / `/var/lib` 49G
(docker) / swap 6.4-7.0G 常用・load 12前後。soak参加はこの解消後。
