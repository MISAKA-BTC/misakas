# 経済パラメータ台帳 — (値, evidence, 再較正トリガ) の三つ組

- **Authority:** ADR-0046 Decision §3 —
  「各パラメータは『値 + 実測 evidence への参照 + 再較正トリガ条件』を三つ組で
  `docs/econ-parameters-frozen.md`(新設)に記録して初めて凍結とみなす。
  evidence の無い値は preset に書かれていても『未較正』と label する。」
  本ファイルがその台帳の実体であり、ADR-0046 DoD item 1 を閉じる。
- **Date:** 2026-07-27
- **Scope:** kaspa/palw workspace の **shipped preset に実際に乗っている**経済パラメータのみ。
  提案値・設計メモ・未実装の案は載せない(載せると「未較正だが台帳にある」状態が生まれ、
  台帳の意味が消えるため)。
- **単位:** `SOMPI_PER_KASPA = 100_000_000`(`consensus/core/src/constants.rs:45`)。
  本文の「MSK」は `10^8 sompi` を 1 とする表示単位。
- **時間単位:** 全 PALW preset で `palw_epoch_length_daa = 100`
  (`consensus/core/src/config/params.rs:1403,1520,1800,1837`)、10 BPS ⇒ 1 epoch ≈ 10 秒。
  epoch 単位のパラメータの壁時計換算はすべてこの前提に依存する。

---

## 0. この台帳が **していない** こと(先に読むこと)

この台帳は「凍結の**形式**」を用意し、現に preset に乗っている値を可視化するだけである。
以下は本ファイルを書いても**閉じない**。コードを書いて閉じられる種類のものではない。

1. **L1 スパム経済(stamp ramp)の凍結** — 多機(multi-machine)flood キャンペーンの実測が要る。
   単一機の G6 harness は自分で `thresholds: None` と申告している(§3)。**外部。**
2. **L3 報酬経済(replica premium π / `palw_compute_work_scale`)の較正** — ADR-0045 の
   fraud 配線が前提で、その配線自体が未実装。**外部・依存先未着手。**
3. **独立レビュー / 第二オペレータ / 長期 soak** — いずれも本リポジトリの外側の事象。
4. **mainnet 値の決定** — 現在の値はすべて testnet スケール。mainnet activation は
   ADR-0046 の 3 レーン全部を通し直す必要がある。

したがって **2026-07-27 時点で status = FROZEN のパラメータは 1 つも無い。**
台帳が空の FROZEN 欄を持つのは失敗ではなく、正しい現状の記録である。

---

## 1. 凍結ステータスの語彙(3 値のみ)

| status | 意味 | 必要条件 |
| --- | --- | --- |
| **FROZEN** | 実測 evidence に裏打ちされた値。変更は再較正 evidence とセットでしか許されない | 再現可能な実測 evidence が存在し、その evidence 自身が外部条件(多機再実測・独立レビュー等)を要求していないこと |
| **暫定** | 導出の筋は明示されているが**実測ではない**。安全側に倒しただけの値 | 導出式が本ファイルに書かれていること + 再較正トリガが定義されていること |
| **未較正** | 較正 evidence が無い。preset に数字が書かれていても「調整済み」を意味しない | — |

「preset に非ゼロの数字が書いてある」ことは **どの status の根拠にもならない**。

---

## 2. 台帳

| # | パラメータ | 現在値 | コード位置 | evidence | 再較正トリガ | status |
| --- | --- | --- | --- | --- | --- | --- |
| E1 | `min_provider_bond_sompi` | `10 * SOMPI_PER_KASPA` = 1,000,000,000 sompi (10 MSK) | `consensus/core/src/palw.rs:4386` | live provider payout 79,299,440 sompi/leaf(`scripts/palw-shared-testnet/DA-REAL-MINT-RUNBOOK.md:17-19,:243-246`)⇒ κ ≈ 12.6 | 報酬スケール変更(coinbase 減衰表 / leaf 単価)、`max_view_batches` の再価格、mainnet activation | **暫定** |
| E2 | `min_leaf_bond_sompi` | **0(VACUOUS)** | `consensus/core/src/palw.rs:4378` | なし | re-genesis での leaf bond 価格決定(`max_view_batches` とのトレードオフ含む) | **未較正** |
| E3 | `provider_unbond_floor_epochs` | 6 epoch(≈60 s @ 100 DAA/epoch, 10 BPS) | `consensus/core/src/palw.rs:4393` | なし(`audit_window_epochs = 6` に一致させた設計上の選択のみ) | audit window / DA retention horizon の変更、mainnet activation | **未較正** |
| E4 | `palw_compute_work_scale` | **0**(全 7 preset) | `consensus/core/src/config/params.rs:1401,1518,1589,1651,1726,1798,1835`(assert: `:2049,:2085`) | なし | ADR-0045 fraud 配線の着地(ADR-0046 Decision 1 L3) | **未較正** |
| E5 | replica premium **π** | 10,000 bps に hard-pin(= neutral) | `consensus/src/pipeline/virtual_processor/utxo_validation.rs:1157-1160`;governed range 5,000..30,000 は `consensus/core/src/palw_premium.rs:68-69` | なし | 同上(ADR-0045 fraud 配線) | **未較正** |
| E6 | anti-spam stamp ramp `PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE` | `window_daa: 26_440, replicas_per_hash: 4, burst: 8, base_stamp_bits: 12, max_stamp_bits: 19` | `consensus/core/src/palw_antispam.rs:55-56`(doc `:52-54`) | **EVIDENCE-PARTIAL**(§3 — 単一機のみ) | 多機 flood キャンペーン(ADR-0043 と同一)の閾値凍結、独立レビュー | **未較正** |
| E7 | DA policy `PalwDaPolicyV1::STRICT_TESTNET` | `min_beacon_burial_daa: 100, retention_daa: 2_000, response_window_daa: 200, samples_per_provider: 1, max_challenges_per_bond_per_epoch: 4` | `consensus/core/src/palw/da.rs:709-715` | なし | DA 応答コストの実測(challenge/response 手数料)、retention horizon の mainnet 化 | **未較正** |
| E8 | slash 量 | **量パラメータが存在しない** — bond output-0 の全額没収 | `PalwProviderBondMutation::Slash(TransactionOutpoint, u64)`(`consensus/core/src/palw.rs:1990-1994`);適用点 `consensus/src/pipeline/virtual_processor/processor.rs:2581,:2752,:4390,:4407`、`.../utxo_validation.rs:528` | なし | 部分 slash を導入する設計変更、または bond 床(E1)の変更 | **未較正** |
| E9 | DA challenger bond 額 | **専用パラメータが存在しない** — challenger は既存の active provider bond を指す outpoint を出すだけ | `consensus/core/src/palw/search_snapshot.rs:1517`(`challenger_bond: TransactionOutpoint`)、所有検査 `:1802` | なし | challenger 固有の担保を導入する設計変更 | **未較正**(該当パラメータ無し) |

---

## 2.1 各行の注記

### E1 — `min_provider_bond_sompi` = 10 MSK(唯一の「暫定」)

ADR-0046 Decision 1 の L2 規律は `slash ≥ κ × (1 攻撃で得られる期待報酬)`、κ ≥ 3。
slash は全額没収(E8)なので slash 量 = bond 額 = 1,000,000,000 sompi。
2026-07-26 の live DA-real mint で観測された 1 leaf あたり provider payout は 79,299,440 sompi
(`DA-REAL-MINT-RUNBOOK.md:18-19`、settlement PASS で exact-SPK 一致を確認、同 `:243-246`)。

    κ = 1_000_000_000 / 79_299_440 ≈ 12.6   (≥ 3 を大きく満たす)

これは**攻撃利得の上界からの導出**であって、攻撃者の実コスト測定ではない。よって **暫定**。
FROZEN に昇格する条件は「報酬スケールが確定し、かつ κ を実際の攻撃シナリオ(複数 leaf を
同時に取る場合の期待利得)で再計算した evidence が残ること」。

値の由来自体はさらに弱い: コード注記(`palw.rs:4381-4385`)が明言するとおり、
この 10 MSK は DNS testnet の `min_bond_amount_sompi` を**そのまま鏡写しにした testnet スケール値**で、
κ ≈ 12 は事後に確認した性質であって設計入力ではない。mainnet activation は再価格が必須。

### E2 — `min_leaf_bond_sompi` = 0 は **VACUOUS**、しかも staging が継承する

`is_consistent_for_activation`(`palw.rs:4264-4277`)は `max_view_batches`、`max_batch_leaves`、
`max_leaf_chunk_leaves`、`min_provider_bond_sompi`、`provider_unbond_floor_epochs` の
非ゼロ性を検査するが、**`min_leaf_bond_sompi` は意図的に検査していない**。
その理由は `palw.rs:4279-4299` に 21 行の DELIBERATE OMISSION として記録されている(要約すれば
「無意味な数字で assertion を満たすと、可視な activation blocker が不可視になる」)。

結果として leaf bond 床は**存在するが常に 0** であり、`PalwBatchManifestV1::admission_valid` の
leaf 側担保要求は現状 **何も要求していない**。これは open で documented な ECON finding である。

さらに配布面: 出荷済み全 preset が `PalwBatchAdmissionParams::INERT` を持ち
(`config/params.rs:1413,1530,1810,1847`)、`STAGING_MAINNET_PALW_PARAMS` は
`..MAINNET_PARAMS`(`config/params.rs:1732`)経由でそれを継承する。
すなわち **ADR-0048 の staging-mainnet リハーサル網は `min_leaf_bond_sompi = 0` で起動する。**
staging を「mainnet 形状の予行」と読むときは、この 1 点だけは mainnet 形状ではない。

### E3 — `provider_unbond_floor_epochs` = 6

非ゼロ性は `is_consistent_for_activation` が強制する(`palw.rs:4276`)。
**magnitude には測定が無い。** 6 は `audit_window_epochs = 6` に合わせた選択で、
「slash しうる audit window が閉じる前に bond が退出できない」という順序性だけを保証する。
DA exit gate が live obligation 中の担保支出を独立に阻むため、この値は現状
「二重の安全弁のうち測っていない方」である。したがって **未較正**。

### E4 / E5 — weight = 0、π = neutral は「現状維持の明示的決定」

ADR-0046 Decision 1 L3 が明示するとおり、両者は ADR-0045 の fraud 配線が前提で、
それまでは 0 / neutral を維持する。`palw_premium_at_window` は `_epoch` 引数を**無視**して
定数 `PALW_PREMIUM_BPS_ONE` を返す(`utxo_validation.rs:1157-1160`)ので、
governed range 5,000..30,000(`palw_premium.rs:68-69`)は現在 dead clamp である。
「決定済み」であることと「較正済み」であることは別で、ここでは前者のみが真。**未較正。**

### E6 — stamp ramp は候補であって凍結値ではない

`PUBLIC_REGENESIS_CANDIDATE` の doc 自身(`palw_antispam.rs:52-54`)が
「operators must calibrate the 12..19 bit floor/ramp under the G6 header-flood benchmark
before activating it in a preset」と書いている。
`STAGING_MAINNET_PALW_PARAMS` はこれを載せた最初の preset(`config/params.rs:1727`)なので、
**staging は未較正の ramp で走る**。それは意図された配置(staging が L1 再実測の場)だが、
「staging に載っている = 較正済み」と読んではならない。evidence の現状は §3。

### E7 — DA policy は preset で再価格**できない**

`PalwDaPolicyV1::STRICT_TESTNET` は `Params` のフィールドではなく、consensus seam に
直接ハードコードされている:
`consensus/src/pipeline/virtual_processor/processor.rs:2502,:2552,:3362`、
`.../utxo_validation.rs:475,:517`、`consensus/src/consensus/palw_da.rs:1017`。
よって mainnet 化にあたっては「値を較正する」だけでなく
「per-preset にする(= パラメータ化する)」という構造変更が先に要る。
これは較正以前の未着手項目として記録しておく。

### E8 — slash は全額没収であり、量の自由度が無い

`Slash(TransactionOutpoint, u64)` の第 2 引数は DAA score であって金額ではない。
`effective_provider_bond_status`(`palw.rs:2005-2013`)が `slashed_at_daa_score` を見て
`Slashed` を返し、bond output-0 の UTXO は除去される。部分 slash は表現できない。
したがって L2 規律の左辺(`slash`)は E1 と同一量に固定されており、
**E1 を較正することが slash を較正すること**である。独立した slash パラメータは存在しない。

---

## 3. G6 単一機実測 — **EVIDENCE-PARTIAL**

E6(stamp ramp)に関連する唯一の実測データ。**閾値凍結には使えない。**

- **測定内容(ADR-0043、2026-07-27、Apple M1 Max):** 1,000-sibling flood 下の per-header 書き込み。
  total ops p99 **1,037 → 16**、reachability ops p99 **1,023 → 2**、data writes p99 **→ 1**
  (max 79 / 65 / 64 は 64-閾値交差時の単発 re-tile)。
  出典: `docs/adr/0043-g6-sibling-flood-bounding.md:132-134`。
- **これが示すこと:** ADR-0043 (A) の bounded allocator により、per-header の reachability 書き込みが
  **amortized O(1)** になったこと。gate は Measurement → **Bounded**。
- **これが示さないこと(重要):** harness 自身が
  - `thresholds: None`(`consensus/src/pipeline/header_processor/g6_measurement.rs:573`)
  - `public_value_activation: "StopShip until multi-machine remeasurement freezes thresholds and independent review lands"`(同 `:572`)

  と自己申告している。harness の doc(`:543-544`)も
  「no pass/fail latency or DB-operation threshold: the JSON evidence is an input to calibration and
  independent review, not an activation decision」と明記し、`#[ignore]` 付きの手動 harness である
  (`:546-547`)。
- **測定環境:** **単一機のみ**(M1 Max 1 台)。serial/concurrent flood の多機実測と long-soak は未実施。
- **結論:** 12..19 bit の ramp を凍結する根拠には **なっていない**。多機再実測 + 独立レビューが
  済むまで E6 は **未較正** のまま。

---

## 4. 台帳の運用

1. **値を変えるときは同一コミットで本ファイルを更新する。** これは規則であって願望ではない:
   `consensus/core/src/config/params.rs` の test module に drift alarm
   `shipped_economic_constants_match_the_frozen_ledger` があり、E1/E2/E3/E4 の magnitude を
   staging preset 上で pin している。値を変えるとこのテストが落ち、
   メッセージが本ファイルの更新を要求する。
2. **drift alarm は新しい consensus 規則ではない。** 既存の activation preflight
   (`is_consistent_for_activation` / `palw_activated_presets_bound_the_view`)は
   **非ゼロ性しか見ない**ので、`10 MSK → 1 MSK` や `6 → 1 epoch` は全テスト green のまま通る。
   alarm はその隙間だけを塞ぐ。
3. **status を上げるには evidence 列を先に埋める。** 「暫定 → FROZEN」も
   「未較正 → 暫定」も、evidence 列と再較正トリガ列が両方埋まってから status を書き換える。
   逆順(先に FROZEN と書いて後で evidence を探す)は ADR-0046 Decision §3 違反。
4. **外部条件は本ファイルでは閉じられない。** §0 の 4 項目は、コミットではなく
   実測・レビュー・第三者の参加によってのみ閉じる。

## 5. 検証できなかった記述

- ADR-0046 Context(`:24`)が引く **certificate tx fee 293,437 sompi** と
  **DA challenge/response 各 ~360k sompi** は、両リポジトリ全文検索で ADR-0046 本文以外に
  出典が見つからなかった。よって本台帳の evidence 列には採用していない
  (E1 の κ 導出には、runbook で裏の取れる payout 79,299,440 sompi のみを使用)。
  これらを evidence として使うには、live run の生ログ側に数値を残す必要がある。
