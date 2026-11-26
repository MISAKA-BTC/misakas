# RFC-0017 — モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **統治する前提 — [PRINCIPLES §9](../PRINCIPLES.md)(凍結前提、2026-10-15 ユーザー決定):** モデルの二階層と、モデル単位の検証の不採用。本 RFC は §9 に従い、§9 を再審しない。規則は [ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) が定める。
>
> **モデル単位の検証を行わない理由(ユーザー):** モデルに対して検証を行うと、その検証において何を信頼するか(誰が・何を根拠に「正しく共有・運用されている」と判断するか)という問題が新たに発生し、それは解決しない。チェーンは、モデルの公開状況・保持者数・取得可能性・共有状況を審査しない。

## 現行仕様 — ユーザー判断 2026-10-15 第 4 回・第 5 回

本 RFC は、モデル資格の審査の RFC から、**モデル階層の RFC** に改めた。判断記録は [ユーザー判断](../history/decisions/2026-10/user-decisions-2026-10-15-adr-rfc16-rfc17.md) §7b・§7c。

ユーザー判断(原文、第 5 回):「genesis/fork モデルは 1 倍で有効、後から追加されるモデルは 0.3 倍で、そこから検証などはなく 0.3 倍のまま固定。つまりモデルに対しての検証がなくなった。」

1. **1.0 の階層。** `ApprovedModels(h)` に含まれるモデルである。`ApprovedModels(h)` は、genesis のモデルと、高さ `h` までに有効化された年次フォーク(ソフト / ハード)が指定した「モデル ID + 固定 artifact root」の集合である。
   * 年次フォークは追加するだけで、既存モデルを上書きしない(ADR-0175)。
   * 指定は政策的決定であり、公開性・独立検証可能性の技術的証明ではない。
2. **0.3 の階層。** 後から登録したモデルは、登録した時点から審査なしで 0.3 に固定する。
   * 昇格・更新・失効・試行手数料・試行上限はない。
   * 1.0 になる経路は年次フォークでの指定だけである。
   * registry の `Active` などの状態は、運用状態として階層とは別に残る。
3. **モデル単位の検証・審査は、すべてのモデルについて廃止した。** 下記「廃止した審査」の段階(Registered / Probation / Eligible / Lapsed)、抽選審査、更新、失効、試行手数料、試行上限、公平な待ち行列は適用しない。
4. **係数は一度だけ掛ける。**
   * `A_m = c_m · S̄_m^1.5`(ADR-0177)。その `P_m` を RFC-0016 V+ の物理ブロック権利と coinbase 配分の両方が使う。
   * 受理ブロックの報酬に重ねて掛けない。
   * 登録 tier 全体の発行上限 `κ` により、多数登録で 0.3 の枠を増やせない。
   * RFC-0016 の entrant reserve と旧 Probation 予算は審査の廃止で無意味になる。`ε = 0` とし、`κ` だけを上限とする。
5. **claim 単位の検証は、すべての階層で維持する**(ユーザー判断、原文:「これは残す。そうじゃないと claim の検証ができない」)。
   * claim ごとの認証は [RFC-0018](0018-dag-parallel-multi-model-consensus.md) の Gate C(`SecurityCertified`)にある。これは、抽選検証者の m-of-k 承認と G14 公開告発 window による、前提に基づく認証である。court と確率的検出も維持する。
   * 未認証の claim は、genesis・年次指定・登録のどの階層でも、fork choice・決済深度・Rule E 参加・settled anchor・weight を進めない。
   * 旧 U-2 の Probation bar は対象が存在しないため削除した。

**実装(休眠、未 build、未試験):**

* fence は `palw_model_tier_v1` で、module は `consensus/core/src/palw_model_tier_v1.rs`(ADR-0179 §実装)。
* fence `palw_model_eligibility_v1` は、tier fence が同じ高さ以下にあるときだけ arm できる。この fence は RFC-0018 の認証 attestation が通る inner 74/75 の A-2 gate として残る。
* tier fence の下では、審査の code はすべて到達しない。全ての資格 read は「対象外」になり、inner 74/75 の審査 object は拒否され、sweep は何もしない。
* 審査の code は、休眠のまま履歴として残す。審査は一度も live になっていない。
* 試験の対応は ADR-0179 §試験 source にある。
* **arm の gate(2026-10-22、lane rfcgap-b):** tier fence の固定拒否を arm-readiness record(`PALW_MODEL_TIER_ARMING_RECORDS_V1`、空)に置き換えた。record は (network, 高さ)、承認された `c`・`κ`、カタログ連鎖の先端 edition と root に一致し、ADR-0179 と RFC-0019 §16 の各条件に証拠 digest を 1 つずつ持つ。新しい年次 edition は先端 root を変えるので edition ごとに record が要る。空なので arm は拒否される。平均期間は 7 日(2026-10-16 決定・10-22 再確認)。

---

## 廃止した審査の履歴

旧資格・抽選審査・更新・失効・試行手数料等は [履歴](../history/rfc/0017-model-eligibility-retired-2026-10-15.md) へ分離した。元本文で既に撤回済みの範囲であり、今回新たに規則を変更したものではない。
