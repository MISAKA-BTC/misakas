# Codex への作業指示 — Panel=1 有効化のための実測（2026-10-26）

作成: Claude（G14 統合の統括、branch `claude/g14-panel1-17000`）。依頼者: ユーザー。この文書は実測の依頼であり、値の採択・record の pin・fence の武装・配備を宣言しない。

## 0. 目的と期限

- **goal（ユーザー、2026-10-26）**: G14 を完了し、Panel=1 の kernel route で claim が Final まで行けるようにして、testnet-12 で **DAA 17,000** の fence で有効化する。
- **進め方（ユーザー指示「垂直統合的に」）**: 1 つの slice（Panel=1 の kernel route、genesis class 1 つ、job 形 1 つ）について G14 の全項目を端から端まで閉じ、その slice だけを武装する。他の family は G-01 (b) により報酬なしのまま、後で同じ背骨で広げる。slice の class は Claude 側の lane SPINE（branch `claude/g14-spine`）が決める。決まるまでは、既存の kernel route E2E（`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/`）が使っている class を対象にし、決まったら差し替える。
- **期限**: 公開 t12 の DAA は 2026-10-26 時点で 15,847。実測ペースは中央値 184 s/DAA（`docs/measurements/2026-10-19-t12-passive-daa-blocks.md`）なので、17,000 まで約 2.4 日。record の pin・跨ぎ drill・配備がこの後に続くため、**§2 の P0 項目は 24 時間以内**に結果を返す。間に合わない項目は、途中結果と残りの所要時間を返す（推定を実測と書かない）。

## 1. 担当

Codex には次の 3 つを依頼する。

1. **実測**: §2 の項目を実行し、生データ・環境・binary の sha256 を残す。
2. **独立レビューの材料**: 検証方式の独立レビューに渡す、同一 claim の比較（§2 M-SHADOW）と攻撃費用の表を作る。レビュー自体は別の reviewer が行う。
3. **承認材料**: §3 の承認項目について、ユーザーが決められるように選択肢・根拠・実測を一枚にまとめる。決定はしない。

値の決定は Claude 側の lane VAL（branch `claude/g14-values`）が目安として行い、実測が出たら置き換える。Codex は値を決めず、実測と、その値を式に入れた結果だけを返す。

## 2. 実測項目（優先順）

### 共通ルール

- 手順と記録の形式は [mac-studio-values-and-measurements-2026-10-22.md](../mac-studio-values-and-measurements-2026-10-22.md) §1 に従う。各結果に `sysctl hw.model hw.memsize hw.ncpu`、source commit、binary sha256、feature、command、exit、反復回数、中央値と p95、生ログの場所を書く。**この機械は最低要件ハードウェアではない**と明記する。
- **負荷を記録する。** 前回の M-C（`origin/studio/values-p1-measure`）は 1 分平均負荷 51〜61 の下で測っており、そのままでは採用できない。計測前後の `uptime` を残し、負荷 5 超なら `under_load=true` として結果を分ける。Claude 側の lane が `~/Downloads/MISAKA-wt-b/buildslot.sh` で build しているので、計測は buildslot の空き時間に行い、計測中は build slot を 1 つ占有して他の build を止める（buildslot で `sleep` を包むなど、他 lane の build を kill しない）。
- 計測 harness の変更は試験側だけにする。product・consensus のコードを変えない。
- 公開 t12 の node・VPS は停止・入替・DB 操作・ssh をしない。公開 wRPC の読み取り（`wss://wallet.misakascan.com/t12/wrpc/json` の `getBlockDagInfo` など）は可。公開ネットワークへ local node を参加させる計測は、ユーザーの承認を得てから。
- 計算の正しさの暗号学的証明は使わない（AGENTS.md）。モデル二階層を変えない。

### P0（slice の武装に直接要る。24 時間以内）

| ID | 何を測るか | 方法 | 使う値・record |
|---|---|---|---|
| M-X3 | V10: `B_cold_fetch`（material の cold 取得）、`B_check`、`B_localize`（二分探索 round 数 × round 時間）、`B_carrier`（carrier の包含遅延）、`B_reorg_slack`、`c_retain` | slice の class の最大 profile で court の全 round を回し、DAA に換算する（184 s/DAA と block 容量 111,949 B の実測を使う）。**どの項も 0 にしない**。 | V10、`palw_claim_hard_deadline_v1` の arming record |
| M-C | court 操作クラスごとの検証時間・evidence bytes・working set（前回の op 一覧に、kernel route の Panel=1 承認、post-commit challenge、constraint receipt、public fault proof、外部者 ingress を追加） | `consensus/core/tests/palw_tir_court_verify_cpu.rs` の `MEASURE_OP` を拡張。peak RSS は `/usr/bin/time -l`、N=0 との差を setup 分として分ける。**無負荷で再測定**。 | V1・V2・V4・V11、`PALW_RFC0016_MEASURED_RECORDS_V1`、`PALW_RFC0608_MEASURED_RECORDS_V1` |
| M-CAP | V11: court capacity の `ρ_court` と session weight。最大規模・同時 court の資源境界（同時 session 数を増やしたときの CPU・RSS・IO・carrier bytes、詰まる点） | M-C の op 費用 × 同時数で予測し、実際に同時 court を node で回して実測と比べる。外部者用の予約枠 R=⌊cap/2⌋ が実測で守れるか。 | V11、`palw_court_capacity_v1` の arming record（G-07） |
| M-SHADOW | 同一 claim の比較: 同じ claim を Panel route（従来）と kernel route（Panel=1）の両方で検証し、判定・Final 時期・費用が一致するか。不一致は全件を生データで残す | slice の class で、正直 claim と各種の嘘（偽出力、状態偽装、重複 work、乱数 withholding）を両経路に流す。 | RFC-0011 §15.7 の shadow comparison、kernel route の arming record |
| M-COST | slice の class の実モデル・長文脈での計算／転送／紛争費用（claim 1 件あたりの検証 CPU、取得 bytes、court 1 件の費用、DA 保持 bytes/DAA） | 実 artifact で実行。手元に無い場合は fixture class で測り、実モデルとの差を明記する（実モデルの値として扱わない）。長文脈は 9B-8k と、slice が対象にする最長 context。 | P1-5、M-DA、R4・R5 |

### P1（G14 全体の閉鎖に要る。P0 の後）

| ID | 何を測るか | 方法 | 使う値 |
|---|---|---|---|
| M-P15 | P1-5 容量の較正: original 1 件あたりの tx・UTXO・fee・codec・署名検証・hash・heap・IO・保持 bytes、restart と拒否の費用。全 original の同時実行での上限 | 実 node（local devnet）で original を同時に増やし、各資源の実測曲線を取る。0 費用や上限引き上げを仮定しない。 | P1-5、V2、V5 |
| M-L | liveness: 拒否された court／H-1 carrier 1 回あたりの CPU（組み立て部分 0.72 ns/B は実測済み、裁定部分が未計測）、object ごとの全 state 複製（claim 行数 1k〜1M） | `origin/liveness/court`（`f74ca87d3`）、`origin/liveness/lifecycle`（`7abbaaed3`） | mempool policy の上限 |
| M-DA | class ごとの DA 取得量（material bytes/claim）と保持 bytes/DAA | 既存の fixture class と実 artifact | R4・V5 |
| M-STO | 30 GB ストレージの M1〜M9（`docs/design/palw/storage/capacity.md` §8） | devnet と縮小 cap。disk の空きが 30 GB を切るなら中止して報告 | 容量予算 |

### P2（外部入力。実運用が要る）

| ID | 何を測るか | 方法 |
|---|---|---|
| M-OPS | RFC-0027: 実運用の node の運用費・利益・heartbeat、Account Bond に必要な資本 | 公開 t12 の観測（公開 wRPC の読み取りと、公開されている block・coinbase のデータ）で取れる範囲を先に集計する。自前の node を公開ネットワークで運用して測る部分は、ユーザーの承認を得てから。シミュレーションや仮定価格の感度分析を実運用の測定と書かない。 |

## 3. 承認が要る項目（Codex は材料を作る。決定はユーザー）

| 項目 | 何を決めるか | 必要な材料 |
|---|---|---|
| beacon の k/D | 本番 beacon policy（`approved_panel_beacon_policies_v1` は空）、抽選承認の m/k | f=0.25（2026-10-22 ユーザー決定）で検出 90〜99%（2026-10-17 ユーザー決定）を満たす組を式で示し、選出・交代の実測（M-SHADOW と同じ run）を付ける |
| 前提 fence 3 本の policy | `palw_bond_budget_v1`、`palw_model_bond_allocation_v1`、`palw_material_income_v3` | 各 policy の候補値、M-P15・M-COST の実測、経済監査（攻撃者の最大利益と正直な参加者の収支） |
| 検証方式の独立レビュー | reviewer と範囲 | M-SHADOW の結果表、攻撃種別ごとの検出率・費用、前提（正直な割合）の一覧 |

## 4. 返し方

- branch: Codex 側の作業 branch で、実測 harness（試験側のみ）と evidence を commit する。Claude の lane の branch や共有 branch へ push しない。
- evidence: `docs/evidence/palw/g14-17000/measurements/<ID>/` に、結果表（`README.md`）・生ログ・環境・command を置く。
- record 候補: arming record が要求する requirement ごとに evidence 文書を作り、その sha256 を `docs/evidence/palw/g14-17000/records-candidates.md` に並べる（network: testnet-12 の genesis hash、activation_daa: 17000、source_commit、evidence digest、未充足の requirement と理由）。**code の record 一覧への pin は Claude 側（lane ACT の手順）で行う**ので、Codex は pin しない。実行していない requirement は空欄にして「未充足」と書く。
- 最後に `docs/evidence/palw/g14-17000/measurements/SUMMARY.md`: 完了した実測、負荷下の結果、未実行と理由、残りの所要時間、§3 の承認材料の場所。
- docs を編集したら `python3 docs/maintenance/check_docs.py` を実行する。
- 結果の commit SHA と SUMMARY の path をユーザーへ返す（ユーザーが Claude 側へ渡す）。
