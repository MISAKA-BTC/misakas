# 公開T12・PublicOneHonest 初期設定候補

状態: **DRAFT — 実装・検証用候補。未採択・未測定・未有効化。**

作成日: 2026-11-12。コード調査基準: `bda55c38b`。ユーザーから「未確定なので具体案を作成する」と依頼されたことを記録する。これは以下の候補値の承認ではない。

目的は、Studioの同一claimが **quote → 署名 → canonical受理 → 独立Panel検証 → Final → 当選権利の一回消費 → 対応ブロック受理** まで進む実装候補を固定することである。設定案の作成、旧quoteの改善、ローカルfixtureの成功を、この達成の代わりにしない。

契約の意味に関わる部分は [RFC-0054](../../../rfc/0054-public-t12-original-admission-profile.md) / [ADR-0222](../../../adr/0222-public-t12-original-admission-profile.md) の未採択レビュー案に分離した。数値案の作成を採択と扱わない。

## 1. 推奨する決め方

以下の「候補値」で全経路を実装し、同じ候補を通し検証する。公開有効化は別判断とする。処理容量の実測値がまだない項目は、測定・確定方法を先に固定し、架空の throughput を設定しない。未確定の公開値を、共通plannerの引数・呼出し・原子的保存まで未実装にしておく理由にはしない。

採択する場合の対象は **実装・検証用候補** である。公開設定には、最終的な全数値、binary/policy digest、全経路試験、移行条件をそろえた別の記録が必要。[設定台帳](../../../governance/parameter-register.md)、[凍結規程](../../../governance/design-freeze.md)、[通し実装方針](public-one-honest-vertical-implementation-plan-2026-11-10.md)に従う。

## 2. 変更しない条件

- 必要な完全再実行承認は最低 **1件**。各claimの独立選出Panelに能動的な正直者が少なくとも一人いるという前提を維持する。人数や署名数だけでこの前提を保証したとしない。
- 原受理を `a` とし、noticeは **a+3,120 DAA**、hardは **a+10,000 DAA**。取得・再試行・reorg・交代で時計をリセットしない。DAAを秒へ固定換算しない。
- モデル計算の暗号学的な正しさの証明、モデルの取得成功証明、モデル単位の審査は導入しない。HF固定版とroot/hashの照合はデータ同一性確認に限る。
- Minerによるモデル重みの独自配信・モデル用消失訂正・Nodeのモデル可用性サンプリングを必須にしない。claim固有の入力・trace・openingと局所courtは維持する。
- 追加モデルのburn停止・登録者burn解除、genesis/年次fork指定の除外、1.0/0.3の二階層は[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)とPRINCIPLESを維持する。停止/解除のburn額は本候補のoriginal資金とは別であり、ここでは変更しない。
- `Final` は承認に加え、未解決caseなし・必要な検査機会・資金/期限/依存条件を満たすこと。公開URL、他者の取得receipt、ローカル再計算一致だけでは成功させない。

## 3. Panelと受付量の候補

| 論理パラメーター | 推奨候補 | 根拠・扱い |
| --- | ---: | --- |
| 抽選位置数 | 75 | 旧75を新profileでも**明示的に選ぶ提案**。旧41件承認は引き継がない。人数の新しい最適値を測定したという意味ではない |
| 選出された異なる主体の保護枠上限 | 75 | 既存の正規化identityで重複をまとめる。未知の正直者を取得済み者だけに絞らない。重複主体を独立した複数人と数えない |
| 第三者の同時保護枠 | 4主体/claim | 新規の試験候補。Panel内の一人の保護枠と別勘定。4枠を超える第三者の同時参加は本候補の正常完了保証に含めない。適格な既存caseを追い出さず、抽選や署名を迂回させない |
| 防御容量 | 全合法caseの応答を合算して導出 | 「防御1件分」で全申立人への応答を代用しない。相互排他的な枝以外は全量を予約する |
| 新profileの同時進行original | 初期T12では1件、ネットワーク全体 | 新profileの初回容量評価を限定する運用候補。旧profileのclaimや既存責任は止めない。性能達成値ではない。2件目は空きができるまで明示拒否する |

抽選の母集団・seed・復元抽選・正規化の既存の意味は変えない。上記の保護枠は全scope・全合法分岐を含む量であり、「一人一回しか異議を出せない」という新制限ではない。既存の適格な反証を第三者枠の満杯だけで捨てる実装にしない。caseの保護枠と、既存経路から受け付ける決定的な反証は区別する。4枠の配分/競合処理に既存規範から決められない部分があれば、公開化前にRFC/ADRの審査対象とする。

75人分で期限内に収まらない場合、正直者の保護を削って成功させない。グラフの正当な共有化、実装改善、処理容量またはこの候補の再審を行う。同時1件にしても、1件内の最大合法攻撃の検証は省略しない。

受付枠を返すのは、成功Final・Refuted・未検証Abortのいずれかがcanonicalに記録され、当該originalの未解決caseと未終了のservice義務がなく、対応する予約の解放が同じ遷移で完了した時点とする案。単なる期限到達やmempoolの削除では返さない。枠の再利用後もtrace保持、journal/undo、未決済資金、生成権の保持資源は別勘定で計上し続ける。この有限の解放処理が未実装なら、同時1件という数値だけを入れて「最初の1件の後に永久停止」させない。

## 4. 期限配分の候補

すべて原受理DAAからの相対量。以下は**工程設計用の試験候補**で、期限内完了の測定結果ではない。モデル取得の時間枠はPanel自身の実行予算であり、Nodeによる取得認定を作らない。

| 初回の工程 | 候補 | 最遅到達点 |
| --- | ---: | ---: |
| draw準備 | 32 DAA | a+32 |
| claim固有資料の初期供給 | 512 DAA以内 | a+512 |
| Panelのcold取得 | 512 DAA | a+1,024 |
| Panelの全体検査 | 1,024 DAA | a+2,048 |
| 最初の計算noticeの包含 | 256 DAA | a+2,304 ≤ a+3,120 |
| 初回資料欠落の検出目標 | cold取得枠の終わりまで | a+1,024 |
| 初回資料要求の包含目標 | 検出/準備後256 DAA | a+1,280 ≤ a+3,120 |

`FirstNoticeDemand` の具体値は `draw_ready_by=a+32`、`material_ready_by=a+512`、`cold_acquisition_daa=512`、`full_inspection_daa=1024`、`notice_inclusion_daa=256`、`missing_material_detected_by=a+1024`、`material_request_inclusion_daa=256` とする候補。canonical包含の実サービス枠を予約できなければ受理しない。

**a+1,280は初回目標で、資料要求権の打切りではない。** 原noticeまで適格な要求が可能な契約を維持する。後から正規の欠落を認識する主体も保護できるよう、最遅合法時点までの要求・repair・後続検査の機会を全グラフに予約する。初期実装の一つの早いPMR枠だけで代用しない。具体的な予約方式は、各適格主体について256 DAA以下の間隔の要求機会をnotice末まで確保する案とし、実包含条件・最遅提出条件・失われた供給枠の回復を明記して検証する。これを既存の単一`InitialMaterialRequestPlanV1`が実装済みとは扱わない。必要な機会が失われた場合は成功不可・NoSlashHold/未検証終結とし、正直者のdefaultや有罪を推定しない。

| 最後の合法notice（a+3,120）の後 | 候補 | 最遅到達点 |
| --- | ---: | ---: |
| 資料供給 | 1,536 DAA | a+4,656 |
| 全体検査 | 1,536 DAA | a+6,192 |
| 不一致の局所化 | 1,024 DAA | a+7,216 |
| 証拠作成・提出包含 | 512 DAA | a+7,728 |
| 局所裁定・回復 | 2,048 DAA | a+9,776 < a+10,000 |

後半合計は6,656 DAA、hardまで224 DAAを残す。この224を新しい延長権にしない。因果関係を持つ各packetの実予約、選出主体全員と第三者・防御の最悪競合で検査する。工程の合計だけが収まっても予約成功にはしない。**資料・undoの保持**下限は `max(既存の必要horizon, a+12,000)` を候補とし、追加2,000 DAAはreorg余裕の試験値であって測定済み保証ではない。訴追・裁定の原期限は3,120/10,000 DAAのままで、保存期間を理由に延長しない。資金holdは実際の残存責任・適格精算に従い、保持期限との同一視も自動延長もしない。

## 5. 入力・探索上限の候補

対象claimの[既存診断](public-one-honest-implementation-ledger.md)は5,154葉、270 numerical roots、3,620,160 edges、802,170座標、1,080 root-memberships。builder最大は1,043 elements、130 terms、64 targets、323 source reads。これらは単一候補の下限であり、全profileの最悪値ではない。

| 項目 | 実装・検証用候補 | 意味 |
| --- | ---: | --- |
| original trace葉 | 8,192 | 旧4,096を新profileだけで置換する候補 |
| numerical roots | 512 | 旧64を新profileだけで置換する候補 |
| cover総edges | 4,194,304 | 旧262,144では対象が通らない。超過は受付拒否 |
| cover総座標 | 1,048,576 | 対象の802,170より大きい。既存の有限上限を明示継承 |
| root-memberships | 1,048,576 | 既存の有限上限を明示継承。別の資源次元と相殺しない |
| 一局所再実行のelements / terms | 2,048 / 256 | 丸めた試験上限。builderと独立checkerに同じ規則を適用 |
| 一局所再実行のtargets / source reads | 128 / 512 | 全処理の実回数を計上。自己申告値は使わない |
| original cover作業領域 | 1 GiB | 512 MiB論理枠での探索診断に余裕を置く候補。Node全体のRSS保証ではない |
| raw tokenizer | 8 MiB | 登録用候補上限。2,104,556 bytesの実tokenizerと8 MiB paddingの既存検査を入口にする。最大語彙等の同時最大検査は追加で必要 |
| claim固有資料の一意byte合計 | 64 MiB | 初期候補。prompt/input/program/trace/opening等の正確な導出量と各scope上限を両方検査。モデル重みを含めない |
| claim資料のPart本文 | 最大16 KiB | 新profileの搬送分割候補。署名・認証・外側carrierのbytesは別途全量を計上。既存TKR1のwire分割を黙って変更しない |
| quote有効期間 | 最大64 DAA | 受付で親・価格・予約・期限を再検査。元claimのanchor/TTLを延長する値ではない |

64 MiB等は新たな対応モデル集合の保証ではない。上限外のclaimを黙って小さい別claimへ置換しない。対象claimも新しいmanifestで正確な量を再導出する。[cold replay記録](../../../evidence/palw/hf-cold-model-replay-2026-11-11/README.md)のRAM見積り2,480,262,208 bytesは**Panel側**であり、Nodeの局所検証の1 GiB枠と混同しない。[tokenizer検査記録](../../../evidence/palw/tokenizer-registration-linux-fixed-2026-11-11/README.md)の54.60–56.79秒も、最大語彙・実DAG・混合負荷の最悪時間ではない。

manifest本体は既存codecの2,104 bytesを維持する。10 scopeの本文上限は次を候補とし、合計64 MiBの条件も同時に適用する。これらはモデル重みの配信枠ではない。

| scope | 本文の候補上限 |
| --- | ---: |
| Artifact | HF固定参照descriptorとして1 KiB以内、かつ既存descriptor codec上限以内 |
| Program | 64 KiB |
| Prompt | 16 KiB、かつ既存の外側carrier上限以内 |
| InputTokens | 32 KiB（raw ID列で8,192 token以内） |
| Tokenizer | 8 MiB |
| ExecutionRules / InitialState / Dependencies | 現行ordinary cold codecの512 / 128 / 68 bytes |
| Trace | indexを含む本文48 MiB |
| Output | 32 KiB（raw ID列で8,192 token以内） |

毎claimに各scope最大値を常に売る案ではない。実効scope上限は[初期供給のnative導出](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/initial_supply.rs)から得た正確な必要本文量（予約用の下限は1 byte）とし、上表を超えれば拒否する。profile IDはこの導出規則と全体上限を、claimの署名は導出結果を束縛する。callerの申告だけで枠を縮めない。Merkle opening・trace搬送envelope・署名・再送等の総wire量と保持量は別途導出して全て予約する。本文合計64 MiBを総通信量64 MiBの保証として使わない。

署名entryとnative側で完全一致させる`chunk_bytes`の候補規則も固定する。Merkle InputTokensは既存32 token tileの**128 bytes**。Traceは`max(136, widest_committed_leaf_value_bytes)`。残る単一本文scopeは`max(1, native_body_bytes)`とし、HF descriptorはboundedなcanonical encodingの実長からこの値を決めてからresolveする。既存codec ID・root算式は維持し、空資料の適否は既存codecで検査する。予約下限1 byteを理由にpayloadへpaddingしない。値のu32変換・全体上限も検査する。これらのmanifest chunkは搬送Partの16 KiBとは別で、例えばTokenizerの単一HTTP本文chunkが8 MiBでも、一つのon-chain draftへ8 MiBを入れる許可にはならない。

64 MiBは一式の資料上限であり、一つのBatch draftの上限ではない。既存の256 Parts × 16 KiB＝4 MiB/一draftという制約は維持する。各scope/rangeを認証可能な既存形式で分割し、全draftの費用・依存・同時保持を計上する。4 MiBを超える分割不能な認証単位は拒否し、root/codecをその場で変更しない。HTTP上の全資料取得とNodeが扱う局所資料の範囲も混同しない。

## 6. 処理容量の確定方法

旧`34,000 bytes / 13 work / 1 io / 1 control`を新profileへ流用しない。旧算式でも対象の数値枝は2,734 workを必要とし、さらに資料parse・署名・journal・rollback等がある。

1. 本候補の全入力上限から、各routeの最大合法packetと完全な因果グラフを導出する。`ServiceCost`の各次元と変換式を版に固定する。workをCPU秒と同一視しない。
2. 最低要件として採用するNodeと独立Panelの機材・帯域・disk条件を記録し、同じrelease binaryで最大合法混合負荷を測る。複数登録者のTKR1、BCT2、通常tx、Final/生成、rollback、restartを含む。
3. 初期の予約済みcourt枠には、混合負荷で確認できた処理能力の**25%以内**を割り当てる案とする。これは四次元の独立最大値を組み合わせる規則ではない。実際に同時に処理できた共同領域の内側で、有限の整数ベクトルを一つ選ぶ。通常tx等と同じ資源を二重予約しない。
4. その固定ベクトルで本候補の最大合法グラフを期限内に予約できることを検査する。最大atomic packetすら収まらない場合は、課金だけを増やして通さない。合法な分割または候補の再審が必要。
5. 実装後の測定から得たbytes/work/io/controls、同時候補数、RAM/DB、quote/replay最大pass数を台帳へ記録して採択する。未測定の現時点では公開presetのこれらの値は**設定しない**。

25%と同時original 1件は試験開始時の保守的な配分案であり、安全性の証明ではない。残り75%があらゆる負荷を処理できると主張しない。再編・候補再演で消費する処理も測定・会計に含める。

測定開始用の具体的なservice book候補は下表とする。`W_max`/`I_max`は、上記入力上限で許される**全route**の最大atomic packetに必要なnative work/I/Oをコードから導出した値で、旧13 workではない。未導出なら表をruntime設定へ変換しない。

| 比較ケース | bytes/DAA | work/DAA | io/DAA | controls/DAA |
| --- | ---: | ---: | ---: | ---: |
| S1 | 34,000 | W_max | I_max | 1 |
| S2 | 68,000 | 2×W_max | 2×I_max | 2 |
| S3 | 102,000 | 3×W_max | 3×I_max | 3 |

これは旧34,000-byte単位を比較軸として明示使用する提案であり、旧料金・work・成立証拠の流用ではない。各vectorについて全グラフの予約と混合負荷を検査し、既存blockのmass/サイズや他laneの必要容量を超えるものは除外する。全要件に合格する最小のvectorを採用候補にする。全て失敗ならS3を無条件に採択せず、失敗箇所と再提案を記録する。固定された公開容量をRPC callerや各Nodeが自己測定で随時変更する設計ではない。

全routeの最大atomic wire量を`B_max`として、`B_max <= bytes/DAA`も必須とする。work/I/Oが収まるだけでは合格しない。通常txのmass、署名数、peak heap、永続化量等の独立した上限も全て満たす必要がある。

## 7. 料金・資金の比較用候補

以下も未採択の試験価格。`1 BILI = 100,000,000 sompi`として表示し、native会計は整数sompiを用いる。旧固定総額0.816 BILI、旧signer数に基づく資本、以前承認された別登録txの支払額は流用しない。

| 項目 | 候補 | 根拠・注意 |
| --- | ---: | --- |
| original基本fee | 1,000,000 sompi（0.01 BILI） | 比較用の工程管理価格。計算安全性や攻撃抑止の十分額と主張しない |
| 実carrier搬送 | 50 sompi/byte | [既存G14値](../../../../consensus/core/src/palw_g14_values_v1.rs)を新profileの試験価格として明示継承。本文だけでなく必要な全wire bytesを計上 |
| native service work | 1 sompi/work | 新規の比較用価格。対象binaryの版付きwork算式に対する価格で、実CPU秒単価ではない |
| native I/O / control | 1,000 / 10,000 sompi | 新規の比較用価格。実原価・最大攻撃価値の検証は別途必要 |
| Node保存 | `ceil(retained_byte_DAA / 1,024,000)` sompi | 1 KiBを1,000 DAA保持するごとに1 sompiという比較用価格。必要なjournal/undo/archive分を含む保持量から導出 |
| 保護資本の最低額 | 100,000,000 sompi（1 BILI） | 実資本は`max(この下限, nativeで導出した全責任の必要資本)`。必要資本が未導出のとき1 BILIで代用しない |

元originalに予約するnative service費を全合法グラフから算出して`F_service`、Node保存費を`R_node`、必要資本を`C_native`とすると、候補式は次のとおり。

```
F_original = 1,000,000 + F_service
C = max(100,000,000, C_native)
required_escrow = F_original + R_node + C
signed_fee_budget >= fee_used + F_original + R_node + F_future_carriers
```

各加算・積算はchecked arithmetic、切上げは明示する。`F_service`のwork/io/controlと`R_node`の保持費は別次元で、同じ費用を再加算しない。`F_future_carriers`は50 sompi/wire-byteの搬送費と、将来carrierが負担する保持費を各一度だけ含む。既使用分`fee_used`も署名fee上限から引く。将来のcarrier費を原escrowの必要額へ同じ額として二重加算しない。[現在のfunding実装](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/funding.rs)の区別を維持する。ただし、将来carrierを誰がいつ立替え、どの既存適格精算で回収するかが未接続なら、金額の表示だけで全工程資金済みにはしない。

現行の保存価格にはflat byte単位の経路がある。上記byte-DAA価格、work/io/control価格は本候補で提案する新しい算式で、quote・署名・実debit・精算への変換は未接続である。現行関数へ率だけ差し替えて実装済みとしない。旧claimの価格・精算額は変更しない。

quoteは通常wallet手数料、実burn、escrow出力、bond予約、将来carrier費の各額を別々に表示する。escrow出力を取引feeと数えず、funded資本と既存bond exposureも混同しない。支払先・burn比率・適格精算・返還の状態遷移は既存契約を勝手に変えない。新profileで一意に対応できない精算項目は、実装前に明示的な設計審査事項として残す。上記価格が採択されても、個々のwallet署名・実支払を承認した意味にはしない。

## 8. 署名scopeとnative導出の候補

数値だけでなく、未確定だった署名の意味をレビューするための**契約案**。既存署名やwireを再解釈しない。状態遷移・署名意味に新規の決定が必要なら[凍結規程](../../../governance/design-freeze.md)に従いRFC/ADRへ分離して採択する。

- `network`: Nodeのcanonicalなnetwork/genesis domainとFPのdomainを照合した値。
- `ruleset`: 全設定、codec、費用算式、期限算式、資金/精算規則のcanonical encodingをdomain付きでhashした新profile ID。任意のテストhashや旧profile IDを代入しない。
- `branch`: 元FPの署名済み`anchor_block`に束縛する案。quote/実受付の双方でそのanchorの適格な祖先関係と元FPのTTLを検査する。同じanchorを共有するforkへの再包含は、現在のcanonical prefixから全条件を再検査し、旧forkの受理・burnを持ち越さない。これは最終包含blockの予言ではない。
- `other_relative_graph`: 初回資料供給、notice、検査・提出・回復等の**実際に導出した非Batch義務**から算出する。ユーザーに任意のhashを選ばせるパラメーターではない。
- fee/capital/retention/settlementの4 profile IDも、それぞれの完全な規則のdigestから算出する。IDだけを指定して未実装の規則を満たした扱いにしない。

quoteは親root・親識別子・二つのprospective clock・policy ID・必要資金・予定期限を返す。将来の実txidや`AcceptedOrigin`は作らない。実受付は実carrier/UTXO/順序/包含点を使って同じplannerを再実行する。64 DAA内でも条件が変われば再quoteを要求する。

## 9. 接続作業と採択後の検証

最初の実装単位は引き続き **受付入口 → 共通条件 → 資金・予約 → 原子的保存 → 受付結果** とする。

1. 旧op248の意味を維持し、新版request/responseへprompt・manifest・HF固定参照を接続する。quoteと署名受付に同じnative plannerを使う。
2. 完了済みTKR1とimmutable class semanticsを読む。hash/サイズだけの宣言をtokenizerの局所裁定route完成としない。初回供給・全体検査・全合法caseのgraphを、Batch予約と二重計上せず合成する。
3. 実UTXO署名・payer consent・資金出力と、同じ候補bookでの全工程予約を照合する。全候補のordered prefixで選別と最終foldを一致させる。
4. 書込み前のprefixから専用frame/history/service/escrowを準備し、既存の同じcheckpointで保存する。新profileの資金割当・escrow・予約・claimの一部だけを残さない。PALW受付が拒否されても通常UTXO取引と通常feeが残り得る既存仕様は区別し、その場合はpayerが回収できる通常出力とする。全UTXO取消を実装済みと主張しない。
5. 同じclaimの実txid、包含点、保存行を読み戻す。続いて独立選出Panelの全体承認・異議処理・Final、当選権利の一回消費と対応ブロック受理を確認する。

実装候補の通し接続後にbuild・短い回帰・統合を実行し、同じbinaryで最大合法混合負荷を測る。試験には原子的失敗、署名/資金不足、予約不足、境界DAA、stale quote、tokenizer不一致、唯一正直者の反証、資料不供給、虚偽case、停止中の成功禁止、restart/reorg/IBD、二重権利消費を含める。全件拒否・全Abortを正常経路の達成としない。

公開有効化の前に、全候補値の採否、測定済み容量、最終fee/capital policy、独立レビュー、移行高さ/params/schedule ID、旧ノードの扱いを記録する。実行中のclaimは受理時のprofileを保持し、途中で新しい費用・時計へ切り替えない。移行高さは現在の公開状態を確認してから決め、過去ログのDAAを流用しない。mainへの統合と公開有効化は別の操作である。

## 10. この案で確定していないこと

この文書は、公開容量、最大合法負荷での完了、必要資本の経済的十分性、署名scope契約の採択、全経路の実装・型検査、公開quote/受付/Final/生成の成功を証明しない。これらを完了と表示しない。現在未接続のplannerを完成させる責任は実装側にあり、ユーザーに内部hashや未実装関数の値を決めてもらうことで代用しない。
