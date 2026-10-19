# RFC-0024 — ctx 2M・大きいモデルでも door ≤ 3,200 DAA に収める court 設計(単一位置裁定・先読み開示・先行取得)

- **Status:** Draft / Review Required。実装完了・採択済み・有効化可能という宣言ではない。
- **作成日:** 2026-10-19。ユーザー指示により RFC-0024 として登録。
- **対となる ADR(草案):** [ADR-DRAFT-palw-court-door-3200](0024-assets/ADR-DRAFT-palw-court-door-3200.md)。正式な ADR 番号は採択時に割り当てる。
- **設計目標:** モデルの大きさ(≤10B は model size の制限であり ctx の制限ではない)と ctx(最大 2,097,152)に関係なく、すべての合法 job について TIR の公開 door を **≤ 3,200 DAA** に収める。producer・Panel・custodian の協力を安全性の前提とせず、適格な外部検証者 1 人による訴追を維持する。
- **採る変更:** A=最終 history 裁定の 1 位置化、B=最大 2 段の先読み開示、C=認証済み material の先行取得と強制開示の併用。
- **非変更:** モデルの算術、検出 policy、正しい署名・root の検証、計算不正と DA/default の区別、担保・責任保持、Gate C、客観的 exact court。ZK・SNARK/STARK・zkVM・PCS によるモデル計算の validity proof は導入しない([PRINCIPLES](../PRINCIPLES.md) §8)。
- **上限値:** 従来の `maximum_door_daa = 3,120` から 3,200 への変更は**明示的な承認対象**であり、本 RFC の登録だけでは gate を変えない。
- **有効化:** 新経路は休眠。wire tag、fingerprint、DAA 16,000、稼働 t12 の設定は本書で割り当て・変更しない。
- **Related:** ADR-0189(door 候補)、ADR-0190(bounded rounds)、ADR-0191(dispute-local decomposition)、[g14 実測 evidence](../evidence/palw/g14-measure-2026-10-18/)。

> **提案する決定:** court の判定をノード上に残したまま、裁定に必要な原本の範囲を小さくし、適応的な要求を前もって網羅し、待機の依存を減らす。3,200 を満たしたことにするために既存タイマーを切り詰めたり、オフチェーンの好条件を最悪時の保証に読み替えたりしない。

## 最新 branch との照合(2026-10-19 時点、g14/door-design)

本文は `misakas-pre` の ZIP を基に書かれており、ADR-0191 系の最新実装を監査していない(§0.1)。登録時点で branch `g14/door-design` にある内容は次のとおり。いずれも休眠・fence 未武装で、door の値は**式による derived_upper であり実測ではない**。

| 項目 | commit | 状態 |
|---|---|---|
| history tile の b=16 二分探索(⌈log16 tiles⌉ 往復) | 16555f312 | 純関数・試験通過 |
| A に相当する tile 内 1 位置への追加往復(⌈log16 tiles⌉+1 往復) | 16458bbb1 | 純関数・試験通過。終端 ≈12 KB は**見込み**で認証証拠込みの上限ではない(本文 §4 の条件どおり) |
| 指名 custodian 回答の登録簿(tag 245)を state field 化・processor 接続・block 試験 | ec2c4ae94 / c718fa12e / 5d7905bdf | 休眠 |
| door 導出表 | bb4daa746 / b6e8a3d16 | `docs/evidence/palw/g14-measure-2026-10-18/door-bisection-derived-1019.md` |

1 位置化後の derived_upper(DAA):

| | 32K, 125,000 B/DAA | 2M, 125,000 B/DAA | 32K, 12,500 B/DAA | 2M, 12,500 B/DAA |
|---|---:|---:|---:|---:|
| 最終裁定の読み込みを含めない | 1,519 | 1,703 | 3,058 | 3,242 |
| 1 位置の最終裁定を含める | 1,635 | 1,819 | 3,408 | 3,592 |

125,000 B/DAA は楽観的な上限で感度分析用、12,500 はその 1/10 の感度値であり、どちらも実測ではない。低帯域時の主因は 17,408 B の step block の搬送が段ごとに重なることで、B(先読み開示)による段数削減と C(先行取得)が、本文 §8 の `S ≥ X − 4`(2M)を満たせるかが採否を決める。

- **有効化:** 新経路は休眠。wire tag、fingerprint、DAA 16,000、稼働 t12 の設定は本書で割り当て・変更しない。

> **提案する決定:** court の判定をノード上に残したまま、裁定に必要な原本の範囲を小さくし、適応的な要求を前もって網羅し、待機の依存を減らす。3,200 を満たしたことにするために既存タイマーを切り詰めたり、オフチェーンの好条件を最悪時の保証に読み替えたりしない。

## 0. 根拠・対象版・未確認事項

### 0.1 添付資料の位置づけ

本書は、添付 `misakas-pre(2).zip`、会話中に提示された door の数値、および本書で新たに提案する規則を区別する。ソースの行番号・抜粋・SHA-256 は [source-audit.md](0024-assets/source-audit.md) を参照する。以下の `[Sxx]` は同ファイルの出典 ID、[U01](0024-assets/source-audit.md#u01) はユーザー提示値を指す。

ZIP の SHA-256:

```text
136f95326e9831f7778b9f8e73028c12b1f8f8ae00468b22770f8e54bec4c512
```

ZIP 内では `ADR-0191`、`b6e8a3d16`、`PALW_TIR_FRAME_COST_RECORDS_V1` を確認できなかった。したがって、投稿にある `g14/door-design`、tag 245 の最新経路、1 位置化の実装、ADR-0191 の正式な door 式を監査済みとはしない。統合時に当該 branch と必ず照合する。`PALW_G14_ARMING_RECORDS_V1` が空であることは ZIP のコードで確認した。[S07](0024-assets/source-audit.md#s07)

### 0.2 既存実装から確認できること

| 事項 | 確認した根拠 | 本提案への影響 |
|---|---|---|
| TIR の history dissection は reduction の部分値を Sum/Max で exact に fold し、最後は 1 history tile を再計算する | [S02](0024-assets/source-audit.md#s02)[S03](0024-assets/source-audit.md#s03) | A/B はこの算術を再利用する |
| history row の通常の読み元は、完成済み HistTile またはその行の元の commit point で、任意には選べない | [S04](0024-assets/source-audit.md#s04) | K/V 行を小さく運ぶには canonical operand resolver の版分けが必要 |
| step leaf は `values_le` 全体を含めて hash される | [S05](0024-assets/source-audit.md#s05) | 巨大な leaf の一部だけを抜いても、そのままでは認証できない |
| original frame は preparation、acquisition、owner work、terminal 等の SUM と絶対期限を持つ | [S06](0024-assets/source-audit.md#s06) | 時間を消したり、並列化で料金まで消したりしない |
| custodian の小分け作業は、承認済み WorkDebit から得た permit の後に実行する | [S08](0024-assets/source-audit.md#s08) | 大きな遅延は単純なネット転送だけでなく、許可と収録の依存でも起き得る |
| annex の任意提供＋オンチェーン強制開示＋無応答 default という経路は既存文書にある | [S09](0024-assets/source-audit.md#s09) | C は既存責任モデルを再利用・拡張する |
| owner 上限や複数 custodian、再取得、コピー、carrier framing の予算が存在する | [S10](0024-assets/source-audit.md#s10) | 1 回の生データ量と最大時間・総費用を混同しない |

### 0.3 本書で証明したとはしない事項

32K/2M の 3,200 達成、約 12 KB の完全 witness、最悪時の実効帯域、出荷 binary での span、実ノード E2E、D2/X3、最新 tag 230/245 の互換性は未確認である。本書はその実装契約と受入条件を定義する。

現行受入条件として投稿にある **3,120 DAA** と、今回要求された **3,200 DAA** は別の値である。3,200 を新上限とするには明示的なパラメータ承認と関連期限の再導出が必要であり、それまで現行の 3,120 gate は維持する。わずか 80 DAA の差であっても黙って置き換えない。[U01](0024-assets/source-audit.md#u01)[S11](0024-assets/source-audit.md#s11)

## 1. 問題と設計目標

投稿にある感度分析は次のとおり。いずれも実測ではない。[U01](0024-assets/source-audit.md#u01)

| 帯域（B/DAA） | 32K・terminal 読込なし | 32K・あり | 2M・なし | 2M・あり |
|---:|---:|---:|---:|---:|
| 125,000 | 1,473 | 1,857 | 1,657 | 2,041 |
| 12,500 | 3,012 | 6,048 | 3,196 | 6,232 |

低帯域時の terminal 増分は両方とも `3,036 DAA`。しかし、135,600 B を 12,500 B/DAA で 1 回転送するだけなら約 10.85 DAA である。**3,036 を全て raw tile の転送時間とは断定できず、これを単純に `12 / 135.6` 倍して新 door を求めてはならない。** 正式式に入っている物理転送・許可・再試行・収録・検証・保持の係数を分解する。

本 RFC の成功とは、平均的なノードや協力的な producer が速いことではない。対応する全 fault route と最悪の合法的な遅延の組合せについて、有界な実行契約の下で、同一始点からの door の導出上限が承認済み上限以下になることである。

## 2. 脅威モデルと保証の正確な範囲

### 2.1 維持する保証

不一致を発見した適格な外部 bonded verifier 1 人が、producer の秘密状態や Panel の協力なしに、認証された material から局所化し、以下のいずれかまで進められる。[S01](0024-assets/source-audit.md#s01)

1. **計算不正:** authenticated inputs と既存算術による exact court で具体的な矛盾を示し、対応する conviction・claim 無効化・責任追及へ至る。
2. **開示義務違反:** 所定の義務者が、受理された有効な要求に期限内で答えなかった事実を、accepted chain の状態から DA/default と判定する。

DA/default は「隠された計算が誤っていた証明」ではない。計算不正用の FalseValid 責任を、資料不提供だけから自動適用しない。計算不正と開示義務違反の責任者・罰・返金は既存規則に従い区別する。

### 2.2 明示する前提

- 外部検証者は対象モデルを取得し、既存の適格性・担保・実行資源条件を満たす。完全非公開モデルの検出確率を本書で保証しない。
- base chain は合意規則に従って進み、所定の有界な収録機会がある。全ブロック生成主体が永久に検閲する場合まで、1 人で期限内収録できるとはしない。
- network delay、帯域、CPU/IO、同時負荷、許容 reorg/restart は、承認された service envelope の範囲内。無制限の通信断や無限回の reorg を有限 DAA に収めるとはしない。
- 検出後の決定論的訴追と、検出自体の確率は別である。A/B/C は検査対象の抽選を予見可能にしたり、検出率を作り上げたりしない。

### 2.3 最適化で削らないもの

original model/class/job/claim の同一性、input/output/state への拘束、全 reduction の被覆、最初の不一致の局所化、既存の interval/overflow 規則、無応答時の客観的手続き、責任期間中の担保・証拠、Gate C と Final の hold を維持する。[S01](0024-assets/source-audit.md#s01)–[S06](0024-assets/source-audit.md#s06)[S12](0024-assets/source-audit.md#s12)

## 3. 提案する全体経路

```text
original claim / class / execution / trace を固定
  ├─ C: material manifest・取得 index・保持義務を固定
  ├─ C: producer は生成時から保存、verifier は任意に先行取得
  └─ paid preparation / Ready / original deadline は既存責任と接続
               ↓
不一致の検出と「最初の不一致」への局所化
               ↓
B: canonical な最大 2 段の開示を 1 response phase で公開
   ├─ 全候補が認証済み → verifier が 2 段の選択をまとめて提出
   └─ 資料欠損 → 同じ絶対期限内の強制開示 / DA default
               ↓
A: history tile 内の 1 位置 [p, p+1) まで局所化
               ↓
認証済み Q・K[p]・V[p] と root の reduction 条件で exact 再計算
               ↓
計算不正 conviction / 開示義務 default / 正当な棄却
```

上記の「phase」は 1 ブロックを意味しない。大きい response は複数 carrier に分割できるが、その bytes・収録・組立・検証はすべて door に計上する。partial arrival で応答時計をリセットしない。

## 4. A — 最後の裁定を 1 位置に絞る

### 4.1 対象

まず、既存の exact な history reduction を持ち、局所 operand の由来を決定できる TIR/attention profile に適用する。一般の GDN state 更新、Float32 reduction、logits、tokenizer、全 kernel をこの変更だけで被覆したと扱わない。

現行関数をそのまま残し、versioned な `PositionDissectionV2` と `PositionCloseV2` を並置する。以下の名称は本 RFC の提案名であり、ZIP に実装済みという意味ではない。

### 4.2 局所化単位

history tile `[a, a+t)` が選ばれた後、`t ≤ 16` なら同じ canonical 16 分岐を 1 段適用して `[p,p+1)` を選ぶ。端の不完全 tile も実在する位置だけを候補にし、重複・欠損・範囲外位置を拒否する。

tile→position の段は B の末尾 bundle に含めてよい。**論理的な検証段は残すが、独立した response 往復を必ず追加する必要はない。** tile が既に 1 位置なら追加段を作らない。

### 4.3 算術

root で固定された reduction の種類、要求 element の順序、totals を保持する。court は既存の demanded-range evaluator を `[p,p+1)` に限定して使い、当該位置の部分値を再計算する。[S02](0024-assets/source-audit.md#s02)[S03](0024-assets/source-audit.md#s03)

attention では、たとえば `max(score)`、`sum(exp(score - m*))`、`sum(exp(score - m*) × V)` の関係を、既存の root total `m*` と数値規則のまま扱う。**1 位置だけで softmax を正規化し直す実装は禁止する。** 新しい softmax、量子化、丸め順序、FP の再結合は導入しない。

「Q/K/V の行を運ぶ」は最小例である。実際の primitive が追加の scale、mask、position、parameter、state 等を読むなら、その認証済み値も canonical witness に含める。

### 4.4 行レベル認証 — 本変更の重要な条件

ZIP の `hist_row` は完成済み HistTile を読む場合があり、その leaf は payload 全体を hash する。したがって、単に payload から K/V の 1 行だけ抜いて既存 leaf hash に対する proof と呼んではならない。[S04](0024-assets/source-audit.md#s04)[S05](0024-assets/source-audit.md#s05)

**最小変更候補は、元の row append の commit-point leaf を開く経路である。** 新しい `PositionOperandResolverV2` は `hist_row_node_v1` 等で導出される当該行の writer と座標から、必要な既存 step leaf の集合を決定する。Q と K/V の writer leaf は original execution root に結び付け、disputed leaf より前の leaf であることを確認する。任意の mirror が作った別 root に差し替えない。

この resolver の採用には以下をすべて要求する。

- 元の writer row と HistTile コピーの同一性について、既存の first-divergence / HistTile concatenation 裁定と合わせた被覆証明を持つ。
- 不一致が HistTile 自体にある場合を、attention の 1 位置検査だけで飛ばさない。
- 正しい実行では新旧 resolver の verdict が一致し、改ざんの場所を移しても反証可能性を失わない。
- 既存 leaf を丸ごと開いた場合の必要 bytes を計測する。writer leaf 自体が大きければ「1 行だから小さい」と見なさない。
- 「任意の後続 leaf を選び、別の先行値を持ち込む」経路を認めず、認証経路と first-divergence の前提を検査する。

この同値性・被覆・サイズ条件が証明できなければ、当該 profile は **`PositionWitnessBudgetIncomplete` として新 gate を通さない**。行サブルート追加による commitment 形式の変更を、実装上の都合で暗黙に行わない。必要なら別の明示的な設計審査と互換性規則を追加する。

### 4.5 Witness の構造とサイズ

概念構造は次のとおり。既存型との具体的な対応と wire tag は統合レビューで確定する。

```text
PositionCloseV2 {
  version, network_rules_id,
  claim_id, allocation_id, session_id,
  class_id, execution_root, trace_root,
  disputed_leaf, site_id, root_claim_digest,
  position, query_position, layer, head_or_group,
  selected_partial, root_totals_digest,
  canonical_step_preimages, canonical_step_multiproof,
  canonical_artifact_openings, required_input_openings,
  submitted_by, signature
}
```

重複した context、既に accepted state にある root/totals は canonical な参照にできる。ただし、pruning/IBD 後も検証に必要な bytes を取得できる状態参照に限る。ノード個別のキャッシュ命中を合意条件にしない。

`B_position` は Q/K/V の本文だけでなく、すべての座標・型・root・opening・署名・codec framing と必要な関連 carrier の合計。投稿の **約 12 KB は見込み**であり、上限ではない。正確な byte 上限と parser/CPU 上限を登録 profile から導き、超過する正当 witness を「不正」として slash しない。

## 5. B — 最大 2 段の先読み開示で適応的な往復を減らす

### 5.1 方針

単に現在の child と「選ばれるはずの次の child」だけを先送りしても、相手は別の child を選べるため最悪往復数は減らない。

そこで `LookaheadDisclosureV2` は、現在の disputed range の **次の最大 2 段で到達し得る全候補**を canonical な frontier として開示する。元の分岐は 16 のまま。深さ 2 の frontier は最大 256 個で、内部の親部分値は frontier から既存の Sum/Max で復元できるため省略できる。

### 5.2 2 種類の frontier を混同しない

- **commitment tree の局所化:** leaf/subtree の hash と opening を使う。hash は値の由来を認証する。
- **attention/history の数値的な dissection:** reduction の部分値そのものを開示し、子→親の exact fold を検査する。hash だけを 256 個出しても数値的な整合性検査の代わりにはならない。

B の時間削減に算入するのは、対象 phase の必要情報が全候補について実際に開示・検証された場合だけである。[S02](0024-assets/source-audit.md#s02)[S03](0024-assets/source-audit.md#s03)[S09](0024-assets/source-audit.md#s09)

### 5.3 検証規則

`LookaheadDisclosureV2` は session、現在の round、親 range、root claim、schema に拘束する。

1. 現行の `palw_attn_child_ranges_v1` の切り方を再帰適用し、depth と frontier の座標列を導く。浮動小数の log で木の形を決めない。[S13](0024-assets/source-audit.md#s13)
2. 各要素の型・個数・proven interval、各階層の Sum/Max を検査する。中間階層を省略しても、その境界・interval チェックは省略しない。
3. 全 frontier が親の主張に fold することを確認する。足りない child、重複、別 root、別 head/layer/position、範囲の gap/overlap は拒否する。
4. 検証済み bundle に対し verifier が `ChoicePathV2`（最大 2 個の子番号）を 1 回提出する。各親子関係と途中の主張を accepted state から再構成する。
5. 不正な要求や不正な bundle の拒否は、それだけで計算不正 conviction ではない。正規の obligation と期限は残り、有効な応答がなければ既存の適格 default へ進む。

### 5.4 得られる論理段数

以下は `32K = 32,768`、`2M = 2,097,152`、tile=16、arity=16 の幾何からの計算であり、DAA の実測ではない。単位の違う 2,000,000 positions 等では recurrence から再計算する。

| context | tile 数 | 既存 tile 探索 | A の位置段を加えた論理段 | depth=2 bundle の response phase 上限 |
|---|---:|---:|---:|---:|
| 32K | 2,048 | 3 | 4 | 2 |
| 2M | 131,072 | 5 | 6 | 3 |

これは **history dissection 部分だけ**の表で、step-tree の第一差分探索、root claim、material 取得、検証者の replay、terminal の収録等は別に計上する。response phase ごとの verifier choice も残る。4→2、6→3 を door 全体の半減とは言わない。

### 5.5 bytes 増大への制約

数値 frontier の要求値の合計個数を `r` とすると、完全な depth=2 の値本文は最大 `256 × r × 16 B`（i128）である。既存の generic ceiling `r = 4,096` では本文だけで 16 MiB になり、小さな 1 carrier には入らない。[S02](0024-assets/source-audit.md#s02)

したがって、depth=2 は無条件に有利ではない。profile は事前に次を導出する。

```text
saved_wait = 2 段を別々に応答した場合の待機上限の減少
added_cost = 追加 frontier bytes の運搬・検証・組立・IO・許可・reorg 費用の時間上限

深さ 2 を時間最適化として選ぶ条件: saved_wait > added_cost
```

`r`、最大 body、carrier 分割数、parser/CPU/RAM 上限、response window を layout/site と固定 cost record から決定する。本文が大きいのに hash 数だけで見積もらない。

depth=1/2 の選択は **事前に拘束された deterministic profile schedule** とする。各ノードの live 帯域や cache によって合意の分岐・deadline を変えない。depth=1 の fallback を許す場合はその最悪経路も 3,200 以下でなければならない。足りない場合は profile 未対応として gate を閉じる。

### 5.6 先読みと paid permit

新しい bundle は既存の支払義務を外さない。複数 carrier の提出、CPU 評価、capture の読み込みについて必要な permit は paid envelope から取得する。

既存 WorkDebit の列を単に省略するのではなく、同一の合計 debit を原子的に扱える transport batching と、合意上必須の逐次依存を区別する。範囲許可やまとめ払いの新しい意味を導入する変更は、本 RFC の暗黙の付随変更とせず、独立した仕様・レビュー・試験を要求する。[S08](0024-assets/source-audit.md#s08)

## 6. C — 原本の先行取得と、非協力時の強制開示

### 6.1 manifest と index

original claim に対応する `CourtMaterialManifestV2` を拘束し、次を含める。

```text
version / schema / network_rules_id
claim_id / allocation_id / class_id / execution_root / trace_root
material_source_id / authenticated_index_digest
position/head/layer → canonical source request の導出規則
custody obligation の参照
maximum material bytes / response bytes / work / concurrent service
material_until_daa / bond_exit_after_daa
```

manifest の hash は **material が取得可能という証明ではない**。責任の対象と値の由来を固定するものとする。

producer は推論中に既存の canonical material と取得 index を保存・整列し、execution root 確定後に同じ material を配布する。mirror や verifier は任意に先行取得できる。ただし、検査乱数の決定順序は維持し、未来の challenge を予知する必要がある全候補の巨大な証拠生成を通常処理に課さない。

B の数値 frontier は dispute の root totals に依存し得る。キャッシュする場合は root totals digest と site を含めて key を作り、別の root claim の集計を流用しない。

### 6.2 2 経路

**任意取得の速い経路:** 認証済み bytes をオフチェーンで取得し、検証者が対応する既存の on-chain opening または bounded filing を提出する。第三者から取得した bytes でも root が一致すれば資料として使えるが、第三者の署名だけを court の正しさの根拠にしない。

**強制経路:** 任意取得が失敗したら、既存 original obligation に対して on-chain の有効な要求を受理する。指名された義務者、request、deadline、最大サイズは accepted state で確定する。投稿の tag 245 経路との対応は最新 branch の schema を確認して接続し、tag を推測で割り当てない。

最悪 door は強制経路で評価する。「事前に取れているはず」を理由にこの経路を削除しない。

### 6.3 起点をずらして時間を隠さない

ZIP の frame では `accepted_daa` から preparation/open/stream/collect が連続し、Final の hold と接続している。[S06](0024-assets/source-audit.md#s06)[S12](0024-assets/source-audit.md#s12)

本 RFC は、door の始点を単に Ready 後へ移して 3,200 達成と表示することを禁止する。最小限、次を別々に報告する。

- `D_frame = collect_until_daa - accepted_daa`。
- `D_prosecution`＝不一致の検出または訴追開始から terminal の収録まで。起点を明示する。
- `T_publish_before_acceptance` と、その bytes・CPU・保存費用。

最新 ADR-0191 の door 定義と対応させるまでは、最も都合のよい短い区間だけを door と呼ばない。C で前倒しできても、original acceptance 後に残る冷起動・再取得・replay は最悪予算に残す。

### 6.4 timeout と責任

任意の HTTP timeout、ローカル disk error、mirror の「持っていない」は slash 根拠にしない。default は次がそろった場合だけ成立する。

1. 対象 original claim と義務者が拘束された、有効かつ支払い済みの要求が accepted chain に存在する。
2. 義務者に必要なサイズ・計算・収録機会を含む正規の応答窓が与えられた。
3. canonical deadline 後も有効な完了応答が chain にない。
4. reorg、重複要求、既に履行済みの応答、無効な要求を規則どおりに処理した。

本 RFC は、複数 custodian への「失敗したら次の人へ新しい full window」という無制限の逐次回しを導入しない。既存の指名・再指名回数の上限を予算に入れる。同じ原本の再要求で original door を再開しない。

### 6.5 保持・共用・費用

同じ root の material はノード内で共用してよい。ただし保持責任・他 claim の lease・reorg での再参照・課金対象は消さない。最終 lease の責任期間が不可逆に過ぎるまで削除しない。

事前公開は費用をゼロにするのではなく、作業時期と重複を変える。producer/custodian/verifier の CPU、wire、disk、保持、WorkDebit、料金と capital は SUM で会計し、実際に並列な工程の時間だけを critical path で計る。[S06](0024-assets/source-audit.md#s06)[S08](0024-assets/source-audit.md#s08)[S10](0024-assets/source-audit.md#s10)

## 7. 合意状態と codec の契約

概念上の状態は `Prepared/Ready → Open → AwaitDisclosure → AwaitChoice → PositionClose → Resolved`。material 要求の待ちと DA default は、同一 original deadline に拘束された既存 obligation として接続する。新 enum を既存 wire の末尾に無断追加しない。

必須規則:

- claim、allocation、source、session、round、root claim、ruleset で全 message を domain-separate する。
- canonical な byte 順序・整数幅・child 順序・proof の重複排除・長さ上限を定義する。
- 長さと上限を allocation 前に検査する。無効 bytes を大量に解釈する無料 CPU 経路を作らない。
- timeout ぎりぎりの無効 message と partial response は deadline を延長しない。
- 有効な応答が早着しても、正直な verifier に必要な replay/選択/収録時間を奪わない。
- 対応する version、request class、proof schema が全ノードで同じでなければ拒否する。
- tag 230 の codec 案 A/B、tag 245、D2/X3 の状態差分と再生規則との整合を統合 gate にする。

## 8. door の算出契約

### 8.1 時間と費用を別にする

料金の SUM はすべての主体の作業を含む。一方、door は依存関係と共有資源の競合を反映した最大完了時間である。

```text
D_new = max(D_honest_close,
            D_latest_legal_responses,
            D_default_at_last_obligation,
            D_restart_path,
            D_reorg_reacquire_path,
            D_admitted_load_path,
            D_other_covered_fault_routes)
```

各 path は preparation、公開、要求、開示、choice、permit、検証者 replay、terminal、収録、recovery から成る。初期の全員無応答だけではなく、**最後まで期限ぎりぎりで正常応答し、最後の要求だけ隠す経路**を含める。

### 8.2 各段の上限

最小の監査項目は以下。

| 分類 | 記録する値 |
|---|---|
| wire | body、opening、署名、metadata、carrier framing、分割数、再送数 |
| CPU | decode/hash/fold/exact evaluation、正直な verifier の replay、拒否経路の上限 |
| IO | disk 読書き、capture reopen、cache miss、実コピーと再取得 |
| 待機 | request/response/choice/WorkDebit の依存、inclusion margin、queue span |
| 回復 | reorg 深さと回数、restart 回数、再認証、再提出 |
| 同時性 | admitted load、共有帯域・CPU・disk、owner/custodian の競合 |

`bytes / bandwidth` は該当する 1 項だけを表す。メモリコピーの bytes をネット転送として数えたり、同じ再送を 2 回計上したりしない。減算する重複は根拠を記録する。

上限計算は整数・切上げ・checked arithmetic とし、overflow や未知の入力は拒否する。実測の平均や p50 だけで最悪上限を置かない。実測は有界な service envelope を支える証拠であって、任意の Internet 状態に対する数学的な帯域保証ではない。

### 8.3 3,200 の達成に必要な削減量

旧 terminal 読込を含まない値を基準にする。各 profile で、B/C 等による **検証済みの前段削減**を `S`、A/B/C が追加する末尾位置段・新 terminal・増加 bytes・新 metadata・追加 recovery 等の、旧基準に含まれない時間を `X` と定義する。項目は重複しない ledger で管理する。

```text
32K: D_new = 3,012 - S_32K + X_32K
  → S_32K ≥ max(0, X_32K - 188)

2M:  D_new = 3,196 - S_2M + X_2M
  → S_2M  ≥ max(0, X_2M - 4)
```

これは旧の全体式を新型へ正式に移す前の差分計画に使う必要条件であり、それ自体は実装の達成証明ではない。元の 3,120 上限を維持する場合は、32K が `S ≥ max(0, X - 108)`、2M が `S ≥ max(0, X + 76)` となる。0 で切り上げる前の不足額がそれぞれ 80 増える。

仮に `X` が以下の値だった場合の算術例:

| 未測定の仮定 X | 32K で必要な S | 2M で必要な S |
|---:|---:|---:|
| 128 DAA | 0 DAA | 124 DAA |
| 256 DAA | 68 DAA | 252 DAA |
| 384 DAA | 196 DAA | 380 DAA |

**128/256/384 は測定値でも提案 deadline でもない。** 「2M では前段から数百 DAA を根拠付きで減らせれば目標に届く場合がある」ことを示す計算例に限る。実際の可否は B の増分 bytes と C の強制経路を含む正式な bound による。

### 8.4 絶対期限と最終段の予約

original acceptance 時に `collect_until_daa`、必要な terminal の最終収録余地、default 処理、保持と bond exit を整合させる。新しい challenge ごとに時計を 0 からやり直さない。[S06](0024-assets/source-audit.md#s06)[S12](0024-assets/source-audit.md#s12)

終盤の段を単純に `min(now + round_window, collect_until)` で切って、正直な responder の必要時間を失わせる実装は禁止する。各段は残りの必要時間を予約したうえで開始し、そもそも合法的な最悪進行が収まらない profile は admission / activation を拒否する。

ZIP の public conviction は `accepted_daa ≤ now < collect_until_daa` で受理する。したがって、この経路で `collect_until_daa = accepted_daa + 3,200` とする場合、terminal がちょうど `+3,200` で収録されるのは遅い。最終許容収録位置は `+3,199` であり、上限計算にはこの排他的境界の余裕を含める。DA/default の境界と同一ブロック内の処理順も、最新 fold の規則と照合する。[S12](0024-assets/source-audit.md#s12)

「入らなかったら timeout で落とせば 3,200」は達成ではない。正直な protocol participant が所定資源で期限を守れることを示す。

## 9. 記録と arming gate

`PALW_G14_ARMING_RECORDS_V1` の空 record 拒否を外さずに、審査済み evidence を pin する。投稿の `PALW_TIR_FRAME_COST_RECORDS_V1` は最新 branch の型を確認して統合する。存在しない型を本 ZIP の既存 API として実装指示しない。[S07](0024-assets/source-audit.md#s07)

必要記録:

```text
source_commit / dependency commits / binary SHA-256 / build flags
network genesis / ruleset fingerprint / proposed activation DAA
class / artifact root / layout / semantics / context / site coverage
A/B/C の version と profile schedule
B_position の body/proof/signature/total と評価 work
各 bundle の frontier 値数 / body / carriers / fold work
effective service envelope と使用機材・負荷・帯域制限
全 path の段別 bound / observed span / D_frame / D_prosecution
reorg / restart / IBD / pruning / sustained-attack の evidence
予算・保持価格・Δ の 5 項との対応
independent review / approval / hashes of logs and results
```

UNKNOWN、未実測、未被覆 fault route、未承認の料金・期限を含む record は arming 不可。測定値や hash の仮置きで gate を通さない。出荷 binary の hash が変わった場合、影響分析と必要な再測定・再 pin を行う。

## 10. 安全性・完結性のレビュー義務

| 義務 | 示す内容 |
|---|---|
| A-Soundness | 正しい attention/reduction と正しい原本に対し、1 位置経路が誤って有罪にしない |
| A-Completeness | false parent が有効に fold するなら、少なくとも 1 child が false。全 reduction を維持して 1 位置へ至る |
| A-Source | writer-row route が original commitment・HistTile 整合性・first divergence を保ち、別の Q/K/V への付替えを許さない |
| B-Simulation | 新しい depth=2 の開示・選択を展開すると、同じ root と range を持つ旧 2 段の合法遷移になる |
| B-Resources | 全候補を実際に運んだ bytes/work を数え、未開示 branch への新規往復を隠さない |
| C-Availability | 任意取得に依存せず、有効要求の履行または客観的 DA/default まで進める |
| One-outsider | owner 枠・queue・colluding reservations・費用で正直な 1 人の訴追を排除できない |
| Deadline | 最遅合法応答・最後の default・承認範囲の回復でも、必要な各段と terminal が期限内 |
| Retention | 係争と責任期間の間、root だけでなく反証を作る bytes が restart/reorg/IBD 後にも残る |
| Credit | pending な具体的不正や未解決義務を無視して Final・認証・払い出し・bond 解放へ逃がさない |

`One-outsider` は「owner が 1 人で動く fixture」だけでは満たさない。ZIP には original frame 当たり owner 数の上限があるため、共謀者による占有や正直な filing の starvation を含む試験が必要である。[S10](0024-assets/source-audit.md#s10)

未知の coverage を「attention は 1 位置になった」で埋めない。重い HistTile-copy fault、別 kernel、decode/input/output/state fault が既存経路に残る場合、その経路も同じ profile 全体の door gate を通す。

## 11. 必須試験

### 11.1 単体・性質試験

| ID | 条件と期待結果 |
|---|---|
| A-01 | 任意の端 tile 長 1〜16、head/layer/position で新旧の正直な結果が一致 |
| A-02 | Q/K/V の任意の必要 lane、mask/scale 等を改ざんし、対応する具体的不正を検出 |
| A-03 | wrong root、wrong source、別 claim、別 position、同値 bytes の型違い、重複/欠損 proof を拒否 |
| A-04 | root max/sum/weighted-sum の各種 false total を局所化し、1 位置 softmax の誤実装を検出 |
| A-05 | HistTile コピーだけを改ざん／writer だけを改ざん／両者と下流を整合させて改ざん。被覆漏れなし |
| A-06 | 完全 witness の bytes・parser・CPU が profile 上限内。超過は未対応/拒否であり無実の slash ではない |
| B-01 | depth=2 bundle を旧 2 段に展開した遷移と同値。非 16 冪、端 tile、1 位置を含む |
| B-02 | 中間 child を明示しない frontier でも全階層の fold/interval を検査 |
| B-03 | 256 候補中の任意 1 候補の欠損、不正値、重複、gap、overlap、異なる親を拒否 |
| B-04 | fragment の最遅到着・欠損・順序違い・同じ fragment 再送で deadline が延びない |
| C-01 | mirror 全停止、producer/Panel/custodian 全非協力で on-chain 有効要求→適格 default |
| C-02 | 有効原本を出し続ける共謀者に対し、外部 1 人が exact conviction まで到達 |
| C-03 | ローカル timeout、無効要求、資料の取得済み状態だけで誤 slash しない |
| G-01 | 既存 permit、料金、lock、class/source binding の迂回を拒否 |

### 11.2 実ノード E2E

32K と 2M を別々に、出荷候補 release binary で実行する。profile を増やす場合も個別の coverage と最大値を記録する。

必須シナリオは、正常系、正直な cold verifier、誤った計算への全員共謀、資料完全不提供、最後だけ不提供、最大に近い合法遅延、無効・巨大メッセージ、連続 false claim、colluding owner 枠の占有、queue 飽和、再起動、reorg、IBD、pruned import、fence 前後である。

帯域 125,000 と 12,500 B/DAA は再現条件として使えるが、出荷 envelope の保証値に自動採用しない。公称帯域だけでなく、競合と carrier/permit 制約を含めた実効値を測る。許容範囲外の障害は別記し、有限時間保証の前提と混同しない。

合格には、導出上限 ≤ 承認済み上限、全実測 span ≤ 対応する導出上限、正直な claim の不当 default なし、担保・credit・保持の整合、全ノードの同一結論を要求する。

## 12. 実装手順と変更箇所

| 段階 | 作業 | 確認する現行箇所 |
|---|---|---|
| 0 | 最新 door branch、ADR-0191、tag 245/230、D2/X3 を ZIP と照合し、正式式と版を固定 | 添付にない branch。未確認のまま arming しない |
| 1 | A の 1 位置 phase、canonical writer-row resolver、完全 witness builder と反例試験 | `palw_tir_dissect_v1.rs`、`palw_tir_court_v1.rs`、`palw_tir_step_v1.rs` |
| 2 | B の depth=2 disclosure/choice、codec 上限、展開同値性、増分 bytes と時間比較 | `palw_attn_dissect.rs`、TIR dissection / public source / owned state |
| 3 | C の manifest/index、先行保存と取得、強制経路、永続 journal・保持 | `palw_tir_public_*`、`kaspad/src/palw_tir_public_custodian.rs`、既存 acquisition driver |
| 4 | A/B/C を SUM・期限・料金・capacity・profile admission に接続 | `palw_tir_public_allocation_v1.rs`、`palw_tir_public_owned_v1.rs` |
| 5 | 出荷 binary で測定、全 fault route と E2E、D2/X3 の回復試験を完了 | node / processor / drill / evidence |
| 6 | 新 RFC/ADR 採択、台帳・Spec 更新、record pin、fence-crossing drill | G14 gate、最新 frame-cost gate、release manifest |

carrier の wire 番号、state version、delta codec、API 名の正式割当は段階 0 の照合後に行う。説明用の新型名をそのまま既存 API と誤認しない。

## 13. 移行・有効化・失敗時対応

- fence 前は byte 単位で旧動作を維持する。open session は開始時の ruleset で完了させ、途中で depth や deadline を変更しない。
- 新旧 witness の受理条件を明確にする。旧経路の fallback が残るなら、その最悪値も含めて新 profile の上限を評価する。
- 新規受付を閉じても既に発生した責任・必要証拠・lock は解放しない。
- 一部 profile だけ合格しても「全 fence が有効化可能」と報告しない。2M の未達を 32K の成功で覆わない。
- 3,200 未達なら、未達の phase・route・資源を報告し、追加最適化、対象 profile の延期、または明示的な別上限の承認へ進む。10,000 へ自動変更しない。
- DAA 16,000 に間に合わせるために未測定 record を pin しない。最新の network DAA を読み取り確認し、fence を跨ぐ余裕がない場合は高さの延期を別途承認する。
- t12 への配備、main の更新、実際の fence 設定は本 RFC の成果物作成に含めない。

## 14. 受入チェックリスト

```text
[ ] 最新 ADR-0191 / door branch / tag 245 / tag 230 / D2/X3 と整合
[ ] 3,120→3,200 の変更と、関連 liability/retention/Final への影響を明示承認
[ ] A: 行の original authentication と HistTile-copy fault の被覆を証明
[ ] A: 完全 witness の最大 bytes/work を実測・導出
[ ] B: depth=2 を旧遷移へ展開でき、全候補を開示・検証
[ ] B: 増加 bytes/work を含めても、最悪 wait 削減が有効
[ ] C: オフチェーン取得なしでも on-chain 履行または適格 default
[ ] door の始点を変えず、preparation/冷起動/再取得/permit を含む
[ ] 32K と 2M の全 fault route で導出上限 ≤ 承認済み上限
[ ] 全員共謀・外部 1 人・capacity/owner 競合を含む node E2E
[ ] restart/reorg/IBD/pruning 後も同じ裁定と責任保持
[ ] Δ の 5 項、保持・court・転送の価格、必要 capital を実測・承認
[ ] 出荷 binary の evidence と独立レビューを record に pin
[ ] RFC と ADR の明示採択、Spec/台帳/状態の更新
[ ] fence-crossing drill と配備の別途承認
```

## 15. 不採用・本 RFC に含めない案

| 案 | 採らない理由 |
|---|---|
| door をただ 10,000 にする | 本 RFC の目標を満たさず、原因と資源上限を残す。別承認の選択肢としては排除しない |
| root だけを公開し、非協力時は諦める | material の取得・客観的 default が閉じない |
| 1 位置の Q/K/V 本文だけ運ぶ | original commitment の opening と reduction 条件が欠ける |
| 2 段目の選ばれそうな branch だけ先送りする | 非協力相手が別 branch を選べるため最悪往復数を減らさない |
| 全 frontier は hash だけなので安いとする | 数値 fold の確認に必要な partials を数えていない |
| off-chain cache がある場合の値で arming する | cold/withholding/reorg の最悪経路を保証しない |
| 巨大な準備を Ready 前に動かして door を短く表示する | 始点を変えただけなら E2E 改善の証拠にならない |
| timer を短くして応答不能な正直な主体を default にする | 客観的手続きと正直な実行可能性を損なう |
| signature、多数決、計算 validity proof で exact court を置き換える | 本提案の保証と現行 PRINCIPLES に適合しない |

## 付録 A. 外部参考の位置づけ

外部の一次資料として、OP Stack の Fault Dispute Game specification（single-step / preimage の扱い）および Arbitrum の BoLD overview（有界な時間と 1 人の正直な参加者を扱う説明）を参照した。これらは設計の一般的な比較材料に限る。**MISAKA の実装・door 式・閾値・採用済み規則の根拠には用いていない。** 他 protocol の時計、VM、trusted role、例外措置を移植しない。URL は source-audit の外部参考節に記録する。

## 付録 B. 同梱の計算補助

`door_budget.py` は、既存の 16 分岐 recurrence、A/B の論理段数、§8.3 の必要削減量、明示入力された path 上限の加算を検査する補助ツールである。**最新 ADR-0191 の正式式でも、出荷 node での性能測定器でも、arming の認可器でもない。** テンプレートの UNKNOWN は UNKNOWN のまま残し、測定済みと装わない。

## 付録 C. 逐次 step block 搬送の削減を優先する(2026-10-19 追記、ユーザー指示)

**優先順位:** ML-DSA の大きさの変更より先に、court の「段ごとに 17,408 B の step block を逐次搬送する」構造を減らす。ML-DSA-87(署名 4,627 B + 公開鍵 2,592 B ≈ 7.2 KB/carrier tx)は 2 番目の要因であり、本付録の対象外。

### C.1 現在の値(2M、12,500 B/DAA、式による derived_upper。実測ではない)

| 項 | DAA |
|---|---:|
| 逐次 step 搬送 (K+1)·span = 9 × 234 | 2,106 |
| 探索往復 (16+2) × 46 | 828 |
| 最終裁定(1 位置、≈12 KB 見込み) | 350 |
| ID・提出・open 窓など | 190 |
| **合計** | **3,474** |

3,120 まで 354 DAA(約 10.2%)の削減が要る。混雑・reorg への余裕を見込み、**単発 door の設計目標は 2,500〜2,700 DAA** とする(承認済みの上限ではなく設計目標)。

**span = 234 の内訳(要確認、第一段階):** 17,408 B ÷ 12,500 B/DAA ≈ 1.4 DAA なので、234 DAA は純粋な bytes の転送ではない。現行の fit `span(B) = 44 + ⌈B / 920⌉` は ADR-0190 の 2 点から外挿したもので、1/10 の感度ではバイト項を ×10 して 44 + ⌈174,080 / 920⌉ = 234 としている。つまり実効は「1 DAA あたり約 920 B(感度時 92 B)」で、取り込み待ち・確認深度・安全窓・frame の分割などが含まれていると考えられる。**この内訳を分解するまで、bytes 削減による door の改善量は正確に計算できない。**

逐次搬送回数を減らした場合の条件付き試算(234 DAA/回がそのまま消え、他の項が増えない場合。先読みで増える bytes・往復は含まない):

| 逐次 step 搬送 | 搬送 DAA | door 合計 |
|---:|---:|---:|
| 9(現状) | 2,106 | 3,474 |
| 8 | 1,872 | 3,240 |
| 7 | 1,638 | 3,006 |
| 6 | 1,404 | 2,772 |
| 5 | 1,170 | 2,538 |

### C.2 設計案(組み合わせ)

1. **commitment による局所化:** 各段で子 16 個の部分値を全部開示せず、子ごとの commitment(例:32 B × 16 = 512 B)で不一致の子を特定する。
2. **実データは最後の区間だけ:** 不一致が特定された区間についてだけ、17,408 B の部分値と裁定に必要な operand を開示する。
3. **2 段先読み(本文 B):** 先読みするのは大きな部分値ではなく、可能な限り小さな commitment に限る。全分岐の部分値を先送りすると総 bytes が増え、逐次待ちの削減を打ち消し得る。
4. **認証経路の集約:** 同じ root から複数の値を開くときは Merkle multiproof で共通経路を共有する。fixture の実測では認証経路 27,156,480 B に対し、重複除去後の operand は 74,720 B(`release/t12-16000` 20fff6d90)。1,000 件同時の court での disk I/O・検証 CPU の詰まりを避ける。

これは ZK・SNARK を導入する話ではない(PRINCIPLES §8)。既存の interactive dissection を、hash による commitment と opening で効率化するもので、optimistic rollup の二分探索型 fraud proof と同じ考え方である。

**成立条件(どれか一つでも欠ければ採用しない):**
- 既存の trace commitment が、各段の子の状態を認証できること。
- 双方が提出する子の commitment と元の root との整合性を、ノードが決定論的に検証できること。整合しない側は、その時点で負ける。
- 最後に開示する実データを、元の commitment に結びつけて検証できること。
- **分岐点:** 現在の 16 個の部分値が「比較のため」だけのものか、「各段の fold の計算を客観的に検査するため」に必須かを確かめる。前者なら hash 化できる。後者なら、その検査を最後の局所裁定へ移せるか(最終区間で、子の部分値から親の値への fold を 1 回だけ検査すれば足りるか)を検討する。

### C.3 同時発生と証拠サイズ

- 単発の door だけでなく、同時に N 件の court が走るときの `max_i door_i(2M, N) ≤ 3,120` を受入条件とする。court の容量は `palw_court_capacity_v1` が扱う(court 専用の block 予算、予約枠、期限順の割当、新規 claim の受付制限)。ただし値は仮で、step block の搬送がその予算に計上されているかは未確認。
- court 1 件の証拠サイズの最悪値を、固定上限として証明する。door が小さくても、証拠量が大きければ 1,000 件同時発生時の帯域問題は残るため。

### C.4 進める順番

1. `span = 234` の内訳を分解する(通信待ち・確認深度・安全余裕・bytes)。
2. hash による局所化と 2 段先読みを、2M・正直な相手・悪意ある相手の最悪時の往復数と bytes で比較する。
3. 認証経路をまとめ、検証速度とメモリを測る。
4. 隔離環境で 1・10・100・1,000 件の同時 court と reorg を再現し、どの claim も締切を超えないことを試験する。

### C.5 目標案

| 項目 | 目標案 |
|---|---|
| 2M の単発 door | 2,500〜2,700 DAA |
| 不一致の局所化 | 小さい commitment が中心 |
| 大きな step block | 最終的に必要な区間だけ |
| court の証拠サイズ | 最悪時の固定上限を証明 |
| 1,000 件同時の court | どの claim も締切超過ゼロを試験 |
| chain 外の認証経路 | 共通経路を重複排除(multiproof) |
| IBD | 10,000 DAA ごとの checkpoint と安全な pruning を検証 |

## 整合性修正 — 2026-10-19(RFC 全件整合性監査)

- door ≤ 3,200 は**対象 profile の性能目標**で、主契約(RFC-0014 2026-10-19 改定の 3,120 / 10,000)とは別に管理する([RFC-0026](0026-court-policy-master-contract.md) §2.1)。付録 C.3 の「同時 N 件で max door ≤ 3,120」は、この主契約の申立て窓ではなく性能の受入条件として読む。
- **本文 B の各 round の数値 fold 検査を現行の規範として維持する。付録 C.2 の commitment(hash)中心の局所化は未採用の研究・設計候補。** 親 100・子 40 と 50 がすべて正しく認証されていても `40 + 50 ≠ 100` の集約不正は残り、最終 leaf だけの検査ではこれを見落とす。採用するなら内部の親子 fold 不正を局所的に反証できる証拠と完結性の証明が先(F06、RFC-0026 §7.2)。
- 最終裁定の約 12 KB は 1 位置の見込みで、court 全体の証拠量ではない(RFC-0026 §7.1)。
- 土台に挙げた `palw_court_capacity_v1` は別 branch(`release/t12-16000`)の休眠実装で、`pre` には未統合(F10、RFC-0026 §8.1)。
