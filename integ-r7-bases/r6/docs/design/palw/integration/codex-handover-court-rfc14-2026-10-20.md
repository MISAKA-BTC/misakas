# Codex への引き継ぎ — RFC-0014 の完成と、追記された court 系 RFC の実装(2026-10-20)

この文書は Codex(および後続の agent)向けの作業指示です。最初に、リポジトリの `CLAUDE.md` / `AGENTS.md` と [docs/PRINCIPLES.md](../../../PRINCIPLES.md) を読んでください。

## 0. 絶対に守ること

- **計算の正しさの暗号学的証明を使わない。** ZK、zk-SNARK / zk-STARK、zkVM、zkML、PCS による succinct argument、再帰証明・proof aggregation は禁止です(PRINCIPLES §8)。hash・署名・Merkle commitment・VRF は、データの同一性・認証・乱数のためにだけ使います。
- **モデル単位の審査をしない**(PRINCIPLES §9)。claim ごとの検証(court、Gate C)は全モデルで維持します。
- **fence を武装しない。** 新しい規則は、すべて休眠の fence の後ろに置きます。どの preset でも `None` のままにします。fence より前の挙動は byte 単位で同一に保ちます。t12 の同一性試験(`palw_the_release_did_not_move`、`palw_activation_pool_is_t12_only`、`testnet12_identity_did_not_move`)を必ず通します。
- **新しい object 種別や header 形式は、自分の fence までは判定しないで通します。** fence の前で bytes を拒否すると、古い release と混在した時点で chain が分裂します。
- **禁止事項:** 仮値、証拠の捏造、validation の回避、未検証の fence の武装、上限の引き上げ、モデルの除外、偽の記録、未検証の公開。
- **測定値の扱い:** 観測した最大値を安全上限と呼びません。observed_max・derived_upper・未被覆の条件を分けて書きます。測定結果は `docs/measurements/`(書き方は同 README)に置きます。
- **稼働中の t12・main・配備に触れない。** 負荷をかける試験は、隔離した devnet か、この Mac の上だけで行います。
- **build は共有の lock を通します:** `/Users/wata/.cache/misaka-heavy-lock.sh cargo ...`。full suite を反射的に流さず、対象を絞った試験 → 最後に 1 回、の順にします。
- **commit message の末尾:** `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`(Codex は自分の帰属行を使ってかまいません)。

## 1. 仕様の正本(先に読む順)

1. [RFC-0026 — court の主契約](../../../rfc/0026-court-policy-master-contract.md)(Draft、current court policy)。時計の表、帰責、責任期間、置換表、未決事項(§9)、受入試験 T01〜T15(§10)。**ほかの RFC と食い違ったら、ここを基準にします。**
2. [RFC-0014](../../../rfc/0014-panel-independent-fraud-prosecution.md) の末尾の 2 節:「改定 — 2026-10-19」(ユーザーが方針として採用)と「整合性修正 — 2026-10-19」。
3. [RFC-0024](../../../rfc/0024-court-door-3200-to-ctx-2m.md)(Draft):A = 最終裁定の 1 位置化、B = 2 段の先読み開示、C = 先行取得と強制開示、付録 C = 逐次 step 搬送の削減。
4. [RFC-0025](../../../rfc/0025-scalable-objective-court-many-models-mass-disputes.md)(Draft):大量の court の容量予約、混雑延長、pruning / IBD。
5. 末尾に注記を足した RFC-0008(R8-WINDOW)、RFC-0009(court signer)、RFC-0023(段階 1 / 2)。
6. 監査報告 [0026-assets/audit-report.md](../../../rfc/0026-assets/audit-report.md)(F01〜F11)。

## 2. branch とコードの所在(2026-10-20 時点)

| branch | head | 中身 |
|---|---|---|
| `release/t12-16000` | `b522cbe4d` | `consensus/core/src/palw_court_capacity_v1.rs`(court 専用の block 予算、予約枠、期限順の割当、新規 claim の受付制限、囮の価格。休眠、値は仮)。読む範囲の解析(`misaka/palw/tir/exec/tests/frame_measure.rs`)と、artifact の必要部分だけを読む経路(`misaka/palw/tir/exec/src/node/partial.rs`)。rothschild の ML-DSA-87 化。`docs/measurements/` |
| `g14/door-design` | `2a6ea1a4f` | ADR-0189〜0191。履歴 tile の二分探索と 1 位置化(`palw_tir_history_bisection_v1.rs`)。指名 custodian の登録簿(tag 245、state field と processor 接続、`palw_tir_designated_answers_v1.rs`)。door の導出(`palw_tir_frame_cost_v1.rs`)。court の検証 CPU の計測試験(`consensus/core/tests/palw_tir_court_verify_cpu.rs`) |
| `codex-free/d2-x3` | `05fcf7b5c` | D2 / X3(ADR-0188)。X3 の固定期限と、開いている DA session による Final の保留。node 試験 31 件が通過。`x3_req_5c`(reorg)は `#[ignore]` で未検証 |
| `pre` | 最新 | RFC-0024〜0026 と、この文書 |

`pre` には `palw_court_capacity_v1.rs` がありません。実装は上の branch から始め、統合はユーザーの承認を得てから行います。

## 3. やること(優先順)

### P0-1:無応答の帰責規則(RFC-0026 §4、監査 F02)

- **問題:** chain 上で「期限まで応答がない」は、Miner が応答しなかった場合と、署名・送信したが収録を妨害された場合とで同じ履歴になり得ます。今の仕様では、どちらでも同じように default になり得ます。
- **作るもの:**
  - (a) 合意状態から決定論的に計算できる「正規の処理機会」の定義と、その記録(court の手番ごとに予約された収録枠と、その枠が実際に提供されたか)。
  - (b) 無応答 default の条件として、4 つ(義務・権限・予約・期限、処理機会が与えられたこと、inclusion の前提、前提が崩れたときの別経路)の判定を実装する。
  - (c) 処理機会が提供されなかったときは、slash せずに期限を補正し、Final を保留する経路。
- **禁止:**
  - ローカルの回線測定、Panel の申告、mempool の観測を、判定の根拠にすること。
  - 有効な応答が来たときだけ時計を進めること。
  - 自己申告の DDoS だけで期限を延ばすこと。
- **試験:** T02(永久沈黙)、T03(偽署名・Panel の架空の完了申告)、T04(通常 tx の flood、有料 court の飽和、不正 codec の flood を分けて測る)。
- **具体的な規則が決まらない場合:** 選択肢と、それぞれが何を保証するかを ADR(Proposed)に書き、ユーザーに選んでもらいます。勝手に値を決めないでください。

### P0-2:外部の検証者 1 人の訴追経路(RFC-0026 §5、F03)

- **問題:** 共謀者が owner 枠(上限 8)をすべて取って合法に動き続けると、まだ完成した証拠を持っていない外部の検証者が、局所化に入れません。
- **作るもの:**
  - original claim の受付時に、許容する敵対的分岐を含む責任容量を予約する。
  - 争点・root・範囲・手番に対する開示を、誰でも使えるようにする(既存の designated custodian / tag 245 の共有開示を土台にする)。
  - 先着した申立人に、claim の訴追経路を独占させない。
- **土台:** `palw_court_capacity_v1` の予約枠(reserve pool)、期限順の割当、新規 claim の受付制限(bounded admission)。
- **試験:** T05(全 owner 枠の共謀占有、継続的な埋め直し、同じ root の重複申立て)。
- **完成するまで:** `OneOutsiderCapacityIncomplete` として武装を拒否する gate を入れます。

### P1-1:RFC-0014 2026-10-19 改定の期限規則

- 通常の申立て窓 `a + 3,120`、異議のある claim だけ係争の hard deadline `a + 10,000`。起点はどちらも original claim の受付時点です。
- 有効な申立てを契機に、`collect_until`・開示義務・資料保持・Final の保留・bond の解放条件を、1 つの原子的な状態更新で動かします。既存の公開 allocation 経路は `material_until = collect_until + reorg_retention`、`bond_exit_after = material_until` と連動しており、clock を延長しない前提で書かれているので、新しい versioned fence で導入します。旧 claim には旧 ruleset を適用します。
- 各手番の応答期限を残し、時計の reset を禁止します。同じ争点の重複は統合します。
- **応答義務に基づく勝敗表**(RFC-0014 10-19 改定 C)をそのまま実装します。応答を果たした Miner は slash しません。Panel が最後の証拠を出さなければ challenger 側の敗北です。既存の敗訴費用の徴収を流用します。
- **混雑延長:** `a + 3,200 + E_i ≤ a + 10,000` の内側だけです(RFC-0026 §2.1)。
- **境界表(F09):** notice、root、各 round、choice、最終 fragment、`collect_until`、hard deadline、default sweep、bond exit の起点・受理区間・等号・同一 block 内の順序を、コードの定数と試験(T07:−1 / 一致 / +1)で固定します。

### P1-2:依存契約の horizon(RFC-0026 §3.3、F07)

- 受付時に、EXEC 依存 worker・provider lease・kernel liability・custody pledge が `a + 10,000` と後続の責任を覆っていることを検査し、覆わない claim は受け付けません。R8-WINDOW は維持します。
- bond の解放は、post-Final の公開告発 window、Gate C、monetary material の保持、同じ bond が負う別 claim の責任の最大値を満たしてからにします。
- **試験:** T09。

### P1-3:court signer(RFC-0026 §6、F04)

- profile ごとに、資料を保持する者・新しい応答を署名する者・投稿を中継する者・不履行の責任者を、型と state に持たせます。
- 現行の署名方式を保ちます(court の義務が終わるまで、利用可能な court signer を必須にする)。委任や追加署名のいらない終了証拠は、別仕様として ADR に提案するだけにして、実装はユーザーの承認後にします。資金移動用の鍵を provider に渡す設計にしないでください。
- 第三者 relay(作成済みの署名応答を誰でも提出できる)を実装します。
- **試験:** T08。

### P1-4:終了理由ごとの効果表(RFC-0026 §4.3、F08)

- 計算不正、応答義務違反、棄却、Panel の放棄、service 前提の崩壊、hard deadline 到達のそれぞれについて、credit・court・担保・証拠・共有 frontier・pruning への効果を実装し、表で試験します。
- 期限だけで Final にしないでください。RFC-0023 の段階 1 と段階 2 を混ぜないでください。
- **試験:** T13。

### P1-5:court 提出経路の容量(RFC-0014 10-19 改定 E・F)

- court の応答用の予約帯域、少額の応答 fee、challenger bond、第三者 relay、期限と収録保証の連動を実装します。`palw_court_capacity_v1` を土台にします。
- **step block の搬送が court 予算に計上されているかを、まず確かめます(未確認)。** 計上されていなければ、court の証拠の bytes を court 専用の予算に計上する変更を入れます。
- 受理前の不正 codec・重複 message には、受信 queue・長さ検査・署名検証の work 上限を設けます。fee だけで済ませないでください。

### P2:RFC-0024 / 0025 の最適化(P0・P1 の後)

- **TIR 公開 frame の局所化の起点を確かめる(未調査):** legacy の held DA と shard court には、告発者が名指しした leaf から始める経路(`open_at_named_leaf`)があります。TIR の公開 frame(door の対象)が step の木を上から降りているなら、名指しから始める経路を足します。door の「逐次 9 回の搬送」がどの段かを、先に特定してください。
- **先読み開示(RFC-0024 B)と 1 位置化(A)の統合:** 本文 B の各 round の数値 fold 検査は残します。hash だけで局所化する案(付録 C.2)は、内部の親子 fold の不正を反証できる証拠ができるまで採りません(F06)。T10 を書いてください。
- **認証経路の multiproof:** fixture で認証経路が 27,156,480 B、重複除去後の operand が 74,720 B でした(`release/t12-16000` 20fff6d90)。
- **証拠量:** SC64 は今の形式では届きません(2M・4 分岐で署名だけ 87,913 B、RFC-0026 §7.1)。現行 profile・16 分岐候補・先読み profile を、1 つの codec 集計(全 bytes を serialize して合計)で比べてください。

## 4. わかっている測定値(`docs/measurements/`、`release/t12-16000`)

- **t12 の受動的な観測:** 1 DAA ≈ 184 秒(中央値)、1 DAA に 1 chain block、1 ブロックの tx bytes の観測最大は 111,949 B。
- **court の検証 CPU**(fixture、M1 Max、1 コア):
  - close は中央値 0.35 ms。最後の段で失敗する偽造も同じ速さ。
  - root の受付 1.36 ms、dissection の最下段 1.11 ms。
  - 1,000 件で合計約 3 秒。
  - 一撃の告発の判定、履歴の最終裁定、並列実行は未測定。実モデルの class も未測定。
- **隔離 devnet での payload の取り込み:** 最後の報告の時点で測定が進行中でした。`docs/measurements/2026-10-19-devnet-payload-carrier.md` があるか確かめ、なければ未完として扱ってください。

## 5. 既知の未完・注意

- `x3_req_5c`(X3 の reorg の node 試験):枝 B が実際の時計より何時間も先に進み、node の 132 秒の時計の制限で拒否されます。制限を緩めずに、node が読める試験用の時計を harness に入れて直してください。
- a2u row 76 の `palw_elastic_final_capacity_v1` を harness が武装しないことによる既存の失敗(2 件)。`--lib` の既存の失敗 8 件。
- R-D2-NODE(filer の KernelWitness と fold の claim-part の経路)は未実装。tag 230 の codec(案 A:per-leaf digest / 案 B:Merkle root)はユーザーの判断待ちです。
- door ≤ 3,120 は、実モデルの class でも、実際の帯域でも、まだ成り立っていません。式から導いた値だけで、2M・12,500 B/DAA では 3,242〜3,592 DAA です。
- 3,120 から 3,200 への変更には、ユーザーの明示的な承認が要ります。

## 6. 完了の定義

- 各項目の実装が休眠の fence の後ろにあり、t12 の同一性試験と、対応する T 番号の試験が出荷と同じ build(release)で通っていること。
- RFC-0026 §8.1 の状態欄(設計だけ / 別 branch 実装 / 統合済み / build 済み / 敵対的試験済み / 有効化済み)を、commit hash つきで更新すること。
- 決まっていない値・規則は、ADR(Proposed)と選択肢として残し、ユーザーに判断を求めること。値を勝手に埋めないこと。
- fence の武装、統合、配備は行わず、ユーザーの承認を待つこと。

## 7. 報告

各段の節目で、commit hash、通った試験、測定値(`docs/measurements/` のファイル名)、未完の条件を短く報告してください。
