# RFC-0054 — 公開T12 original受付profileの具体案

Status: **Draft / NOT_ADOPTED_NOT_ACTIVATED**
Date: 2026-11-12
Decision proposal: [ADR-0222](../adr/0222-public-t12-original-admission-profile.md)
Detailed candidate: [公開T12初期設定候補](../design/palw/integration/public-t12-profile-candidate-2026-11-12.md)

## 1. 問題と提案の範囲

ユーザーの「未確定なので具体案を作成する」に対する設計レビュー用の提案。依頼を採択と解釈しない。採択済み設計への通常の接続・不具合修正は継続し、このDraftを公開受付の新しい許可条件には使わない。

コード調査基準は `bda55c38b`、数値案の記録は `2b9d30e67`。[実装台帳](../design/palw/integration/public-one-honest-implementation-ledger.md)にあるとおり、TFO3の公開受付は拒否状態で、保護付き受付のconstructorを公開本経路へ接続できていない。署名scope、完全なサービスgraphと資金の対応を未確定のまま受付を許すと、署名していない負担・未予約の義務・異なるforkの資金持越しが起こり得る。これは現在の公開ネットで実際に損失が発生したという報告ではない。

本提案は新profileの署名scope、保存価格の時間単位、同時受付枠の占有・解放を明示する。数値だけの調整では契約の意味を確定できないため、[凍結規程](../governance/design-freeze.md)の新RFC・ADRによるレビュー対象として分離する。現行設計の置換はまだ行わず、原則・旧claim・旧wireを維持する。

## 2. 守る条件

[RFC-0053](0053-additional-model-burn-pause.md)を維持する。モデル取得証明・登録時取得審査・定期可用性認定・多数決裁定を導入しない。Minerにモデル重みの全量配信を課さない。HF固定参照とroot/hashは同一性の照合に使い、claim固有の入力・trace・Merkle opening・Node局所courtを残す。追加モデルのburn停止と登録者burn解除、genesis/年次fork指定の除外、モデルの1.0/0.3を変更しない。

各選出Panelに能動的な正直者が少なくとも一人いる前提、最低1件の完全再実行承認、未解決caseがないこと、必要な反証機会を維持する。ローカル再実行一致や資料URLだけでFinalを認めない。原受理aからnotice a+3,120 DAA、hard a+10,000 DAAを維持する。暗号学的な計算正当性の証明は使用しない。

## 3. 提案する契約

### 3.1 署名とquote

新profileのnetworkはcanonical genesis/network domainに一致させる。branch scopeは元FPが署名したanchor_blockとする案を採る。quoteと実受付の双方で適格な祖先関係と元TTLを検査し、同じanchorを持つ別forkへの再包含も現在のcanonical prefixから全条件を再実行する。旧forkの受理・burn・予約を持ち越さない。最終包含blockを予測して署名scopeに代入しない。

rulesetは全設定・codec・費用・期限・資金・精算規則のcanonical encodingを新domainでhashする。fee/capital/retention/settlementの4 IDも対応する完全な規則から導出する。other_relative_graphは実際の非Batch義務からコードで導出し、RPC callerが任意hashを選べる項目にはしない。exact encoding、domain tag、wire割当は実装レビューで記録し、旧IDの意味を変更しない。

新版quoteは認証済み親の識別子/root、prospective DAA/blue clock、policy ID、必要資金・予約・期限を返す。将来の実txidやAcceptedOriginを捏造しない。quoteの最大64 DAAは候補上限で、元TTLを延長しない。実受付は実carrier・署名・UTXO・prefix・包含点で同じplannerを再実行する。旧op248 v1の意味を維持し、新版の必須入力を旧版へ落として受理しない。

### 3.2 費用と保持

候補額は詳細案§7のとおり。基本fee 0.01 BILI、搬送50 sompi/全wire byte、work 1 sompi、I/O 1,000 sompi、control 10,000 sompi、資本下限1 BILIを比較用に提案する。実資本はnativeで導出する全責任必要額以上とし、未導出を1 BILIで代用しない。

新しい保存価格の候補は `ceil(retained_byte_DAA / 1,024,000)` sompi。時間を含まない既存flat byte価格とは別の算式として版を分ける。journal/undo/archiveを含む保持量を導出し、同じ責務を二度課金しない。保持下限候補はmax(既存必要horizon, a+12,000)で、訴追期限・資金holdの自動延長にはしない。

原escrowは基本fee・予約済みservice費・Node保存費・資本を区別して導出する（詳細案の `F_original = 基本fee + F_service`）。現行funding実装のlocal変数 `original_fee` はNode保存費を含むため、この変数へ保存費を再加算する意味ではない。署名fee budgetは既使用分と将来carrier費も含めて検査するが、将来carrier費を原escrowへ二重計上しない。将来carrierの実立替主体・適格精算・不足時処理が未接続なら「全工程資金済み」と表示しない。既存の支払先・burn比率・返還規則は変更しない。一意に対応できない精算規則は採択前レビューの残件とし、価格表だけで承認されたものにしない。

### 3.3 枠と保護機会

初期T12の新profileはネットワーク全体で進行original 1件を候補とする。canonical prefixから占有を導き、同じblockの2件目も同じ順序で判定する。SuccessFinal/Refuted/Abortだけで即解放せず、未解決case・残サービス義務がなく、対応する予約解除と同じ原子的遷移で進行枠を解放する。保持・undo・escrow・未使用生成権の残存資源は別に会計する。既存claimや他profileを停止しない。再起動・reorgでも占有を再現し、1件目終了後に次が受理できることを検査する。

Panel抽選75位置、正規化された選出主体を最大75枠、第三者は同時4枠を候補とする。全合法caseの防御義務を合算し、1件分で代用しない。4枠が満杯という理由だけで既存規則上の適格な決定的反証を拒否しない。第三者枠の競合割当・解放のexactな順序は別途レビューして規則へ束縛する必要があり、満杯時の挙動を実装者任せにしたまま有効化しない。

初回の資料要求包含a+1,280は目標であり権利打切りではない。原noticeまで適格な要求とrepair・後続検査を保護する。256 DAA以下の間隔で要求機会を用意する案とし、既存の単一の早い要求枠で代用しない。最遅提出・包含・後続因果graphの予約を具体化することが残る。機会が失われた場合は成功不可とし、正直者のdefaultを推定しない。時間表の合計だけで反証可能性を保証したと扱わない。

## 4. 実装の完了条件と容量の決定

実装単位は **受付入口 → 共通条件 → 資金・全工程予約 → 原子的保存 → 同じclaimの受付結果** とする。FpTokenizerは受理済みTKR1とimmutable semanticsからrouteを構成する。候補quote、実署名、ordered prefix、実foldを同じ規則につなぐ。claim/frame/history/service/escrowは既存checkpointで一括確定・undoする。PALW拒否時に通常UTXO取引と通常feeが残り得る既存仕様は維持し、profile割当の原子性と区別する。

探索・資料上限と工程期限は[詳細案§4〜6](../design/palw/integration/public-t12-profile-candidate-2026-11-12.md)を参照する。処理容量は全routeの最大atomic bytes/work/I/Oと全合法graphを導出した上で、同じbinaryの最大合法混合負荷で決める。比較候補S1/S2/S3や25%の配分案は測定済み性能ではない。全候補が失敗すれば公開設定を作らない。未測定値があることは共通plannerの接続を止める理由にはしない。

通し接続後にbuild・回帰・統合を行うという既存の実装順を維持する。正常受付と、資金/予約不足、境界DAA、stale quote、署名/root不一致、途中失敗、唯一正直者の反証、資料不供給、burn停止、restart/reorg/IBDを検査する。続いて同一claimの独立Panel承認・対話・Final・当選権利の一回消費・対応block受理を確認する。拒否だけの試験や通常blockの生成は達成に数えない。

## 5. 代替案・移行・残件

旧固定料金・旧13 work・テストhashをそのまま新profileに流用する案は、現在のgraph費用や署名意味を満たす根拠がないため採らない。全経路の接続前に補助処理だけを増やすことも完了条件にしない。新しいモデル取得認定はユーザー決定に反するため導入しない。

採択候補は実装・検証用の契約と数値案であり、公開有効化や個々のwallet支払ではない。採択前に署名encoding、精算対応、第三者枠競合、最遅資料要求の予約をレビューする。公開有効化にはさらに測定済み容量、全値、binary/policy digest、移行高さ/schedule、旧ノード対応、通し試験と独立レビューが必要。旧claimは受理時profileを保持し、途中で価格・時計を変えない。main統合だけで有効化しない。失敗時は新profileを未有効化のまま修正し、既受理claimの責任を消去しない。

本書は既存RFCの本文をまだsupersedeしない。採択時にはRFC-v2-0005/0012/0016〜0020の新profileに限る補足として対象節を記録し、旧snapshotは保持する。RFC-0053とPRINCIPLESを変更する提案ではない。
