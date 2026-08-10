# MISAKA Audit-Fee v0.2 設計書 — 検証も仕事である

**副題:** audit fee の base-coin 無上限 mint を止め、検証仕事を emission の同一比例分配に統合する

**版:** v0.2 Draft
**日付:** 2026-08-11
**親文書:** `docs/misaka-compute-token-program-design-v0.1.md`（§6.3 の MUST-revisit を本書が解決する）
**対象ソース:** branch `token-program-phase-a`（PR #62）— 本書は設計のみ。実装は PR #62 の後続 PR（→ §6）
**関連:** ADR-0024 §6（audit fee の存在理由）、`vlt.rs` の `audit_fee_sompi` 較正コメント、2026-08-10 devnet 実走の `not_verified` 飢餓観測

> 規範語 MUST / MUST NOT / SHOULD / MAY は親文書と同じ用法。数値「候補」は testnet 計測後に凍結。

---

## 0. 結論

epoch 予算 `R(E)` の分配対象を「実行された仕事」から「**検証済みにするために行われた全仕事**」へ広げる。

```text
v0.1:  reward_i(E) = ⌊ R(E) · X_i(E) / X(E) ⌋            X = Σ 実行 x_j
v0.2:  reward_i(E) = ⌊ R(E) · work_i(E) / W_work(E) ⌋
         work_i(E)   = X_i^exec(E) + X_i^audit(E)
         X_i^audit(E) = Σ_{i の counted verdict v} x_j(cert(v))
         W_work(E)    = Σ_i work_i(E)
```

- **1 µRTE の仕事は、実行でも再実行（検証）でも同じ 1 µRTE として払う。** committee が c 人なら 1 job の総仕事は `(1+c)·x_j` であり、報酬もその比で自然に割れる。分配比率のパラメータ（bps ノブ）は**存在しない**（→ §2.2 で却下）。
- **base-coin audit fee（`audit_fee_sompi` の consensus 副作用 mint）は `tkn_activation_daa_score` で停止する（MUST）。** fence 以下では v0.1 挙動が byte-identical に続く。fence 以上では base coin の新規発行経路は coinbase schedule のみに戻る。
- verdict の仕事は **TOK を鋳るが票は鋳らない**（MUST NOT）。`C_i(E)`/`W_i(E)` に `X^audit` は一切入らない（→ §2.4）。
- 供給不変条件は強化される: fence 以上で `base coin 発行 = coinbase schedule`、`TOK 発行 ≤ Σ R(E)`。**無上限 mint 経路はゼロになる。**

---

## 1. v0.1 の何が壊れるか

親文書 §6.3 が保留した問題を、実測を踏まえて確定する。

1. **無上限 base-coin インフレ。** counted verdict ごとに 0.5 KAS が新規発行される。emission が certificate 量に金銭動機を与えた瞬間、この経路は「計算活動に比例して増える、schedule 外の通貨発行」になる。cert 1 件 ≈ 最大 `committee × 0.5 KAS`。
2. **定額 fee とジョブサイズの誤価格。** 検証の実費は replay 計算 ∝ `x_j` なのに fee は定額。fee > 実費となる小ジョブ帯が構造的に存在し、そこが **tiny-job pump**（小ジョブ量産 → 抽選された verifier 群への低コスト mint）になる。sortition が自己収益化を薄めても、網全体のインフレとしては残る。
3. **検証経済は load-bearing である（実測）。** 2026-08-10 の devnet 実走で、verdict が流れない間 certificate は `not_verified` に滞留し `X=0` が続いた。検証が経済的に成立しない網では emission そのものが止まる — audit の対価設計は emission 設計の一部であって付録ではない。

---

## 2. 設計

### 2.1 統一仕事量 emission

`stage_vlt_credits` の credit walk は既に certificate ごとの counted verdicts（抽選 committee 員の、署名検証済みの verdict）を解決している。v0.2 はその walk の出力に **audit 側の per-validator 集計**を加え、settlement が実行・検証を一本の比例分配で払う。

```text
epoch E の finalized 行（v0.2）:
  exec:  (validator_id, X_i^exec)   … v0.1 と同一（§3.2 の x_j 総和）
  audit: (validator_id, X_i^audit)  … i の counted verdict が判じた cert の x_j 総和

settlement:
  work_i = X_i^exec + X_i^audit
  reward_i = mul_div_floor(R(E), work_i, W_work)      … 既存の 256-bit 経路をそのまま使う
```

- verdict の重みは **判じた certificate の `x_j`**。verdict の向き（Confirmed/Refuted）にも、その cert が最終的に credit されたかにも依存しない（MUST）。refutation にも同額を払うのは §7 の griefing 均衡（「refute は正当な仕事」）を v0.1 から引き継ぐため。counted（= 抽選 committee 内・署名有効）だけが対象なので、verdict スパムは sortition が上限する。
- executor 自己検証は既存規則どおり不可能（InvalidCertificate）。
- 端数・`min_network_compute` floor・冪等・pin 済み行のみ読む、は v0.1 §5 の性質をそのまま継承（floor の比較対象は `W_work` ではなく **`X^exec` 総和のまま**とする（MUST）— floor は「網に実計算があるか」の判定であり、検証は実行に随伴するため二重に数えない）。

### 2.2 却下した代替案

| 案 | 却下理由 |
|---|---|
| §6.3(1) per-epoch audit 予算 cap（sompi 建て維持） | インフレは有界化できるが、繁忙 epoch で verdict 単価が replay 実費を割り、検証供給が止まる（v0.1 の飢餓を予算内に移し替えただけ）。二通貨の較正問題も残る |
| §6.3(2) `R(E)` を bps で exec/audit に固定分割 | committee サイズや confirmations 較正を動かすたびに bps が誤価格化する。c=5 で公平な分割は audit ≈ 83% であり、「audit_share=20%」のような直感値はほぼ常に誤り。統一仕事量なら自動で正しい比になる |
| verdict 単価を x_j 比例の sompi 建てにする | 誤価格は直るがインフレ経路が残る。§1(1) が主敵である以上不採用 |

### 2.3 base-coin audit fee の停止

- `compute_audit_fee_outputs` は sink DAA が `tkn_activation_daa_score` 以上のとき空を返す（MUST）。fence 未満は v0.1 と byte-identical。
- 停止と TOK 側の給付開始は**同じ fence** で切り替わる。二重払い期間も無給付期間も存在しない（settlement は fence 以上の epoch にのみ audit 重みを含める。fence を跨ぐ epoch は「その epoch の anchor DAA が fence 以上か」で片側に決める — MUST、決定は per-epoch で全ノード同一）。
- 新しい fence は**導入しない**。TOK が存在しない網に audit-TOK は定義できず、TOK が存在する網で base-coin mint を続ける理由もない — 一つの fence で両方が切り替わるのが唯一整合的な線。

### 2.4 票との絶縁（継続）

`X^audit` は `C_i(E)`・`W_i(E)`・`VltVotingSnapshot` のどこにも入らない（MUST NOT）。理由は二つ:
1. §8.1 の quorum 交差論証は「実行された計算」を分母に較正されている。検証は同一 job の**再計算**であり、票に数えれば一つの物理計算が最大 `1+c` 票分に膨れる。
2. 貨幣は無限に発行されても票は `λ·B_i` cap の内側という v0.1 の分離原則（親 §0 成立条件 1）を、audit 側でも守る。

---

## 3. 経済分析

| # | 論点 | v0.2 の帰結 |
|---|---|---|
| 1 | インフレ | base coin: fence 以上で新規経路ゼロ。TOK: `Σ R(E)` が全役割込みの上限（**構成的に有界**） |
| 2 | tiny-job pump | 消滅。verdict 重み ∝ 判じた job の `x_j` なので、小ジョブは小報酬 |
| 3 | 量産攻撃 | 総 pie 固定 → cert/verdict をいくら増やしても**ゼロサム再分配**にしかならない |
| 4 | 検証の採算 | µRTE 単価が実行と同一なので、GPU 時間の機会費用が対称。verifier のなり手が構造的に消える誤価格は起きない |
| 5 | executor 取り分の希薄化 | 意図された帰結: c=5 なら executor は約 1/(1+c) = 1/6。**R(E) の意味が「実行への報酬」から「検証済み計算の総費用」へ変わる** — R0 較正（親 §12 #2、TBD）はこの定義で行うこと（SHOULD） |
| 6 | 繁忙 epoch の希薄化 | 実行・検証が同率で薄まる（対称）。どちらか一方だけが飢える v0.1/cap 案の非対称は消える |
| 7 | bootstrap（TOK 無価格期） | 検証対価が無価格資産になる。初期 PoW と同型の受容されたトレードとし、fence 時期は運用判断（親 §10）。sompi 併給の移行窓は複雑性に見合わず不採用 |
| 8 | 共謀 | 相互検証で pie の取り分は増やせるが発行総量は不変。取り分増は実計算（replay）の実費を伴う — まさに mining |
| 9 | slashing | 不変。ContradictoryVerification は bond（base coin）を燃やす。TOK は没収対象にしない（可換性維持、親 §8.10） |

---

## 4. パラメータ

新規パラメータは**なし**。`audit_fee_sompi` は fence 以下の挙動のために残置（deprecated 注記を付す、MUST）。`TokenParams` は不変 — したがって **consensus fingerprint は動かない**（params 形状・wire 形式に変更がないため。動くのは store 層のみ）。

---

## 5. 実装スケッチ（後続 PR の作業台帳）

1. `vlt.rs`: `VltEpochCredits` に `audit: Vec<(Hash64, u128)>` を追加（borsh 層変更）→ `VLT_CREDITS_SCHEMA_VERSION = 4`（既存の再導出機構がそのまま面倒を見る。旧行は破棄・チェーンから再計算）
2. credit walk（`vlt_epoch_snapshot` 系）: certificate 解決時に counted verdicts の `(verifier_id, x_j)` を epoch 集計へ加算
3. `token.rs`: `emission_rewards` を `work = exec + audit` の統合入力で呼ぶ薄い前段（`merge_work(exec, audit)`）+ 単体テスト（比例・floor 対象が exec のみ・票非混入）
4. `processor.rs`: `settle_token_emission` が v0.2 行を読む。`compute_audit_fee_outputs` とその 2 呼び出し元に fence gate
5. `TokenEmissionSettlement` に `audit_paid: u128` を追加（監査可視化。borsh 変更 → `TOKEN_LEDGER_SCHEMA_VERSION = 2`）。settle ログと `getTokenEmissionInfo` に露出
6. harness 拡張: verify に (a) fence 以上で audit-fee UTXO が**一つも**生まれない assert、(b) verifier 口座（executor でないノード）への TOK 貸記 assert、(c) settle 行の `audit_paid` クロスノード一致
7. devnet 実走 → PASS → PR（base: PR #62 の head）

## 6. 段階

本書は設計のみ。実装は **PR #62 マージ後の後続 PR**（§5 の順で 1 本、devnet PASS を添えて）。fence は既存 `tkn_activation` を共用するため、activation 手順（親 §10）に変更なし。

## 7. 未決

| # | 事項 |
|---|---|
| 1 | R0 較正の再定義（§3 #5: 「検証込み総費用」として） — 親 §12 #2 と同じ凍結タイミング |
| 2 | committee 絶対数が大きい網での audit 支配（c ≫ 1 で executor 取り分 → 0）。c は consensus 較正値なので §8.9（親）の再較正と同時に見る |
| 3 | verdict の遅着（cert の epoch より後の epoch に counted される場合の帰属）— walk の既存解決規則に従う。実装時に固定し本書へ追記 |
