# PALW-AN1 検証記録

日付: 2026-11-18（Asia/Tokyo）  
仕様状態: **DRAFT_NOT_ACTIVATED**

本記録は、今回作成したAN1の独立参照実装・反例によるコード査読、および添付RFC一式に対する文書回帰検査を分離して報告する。ユーザーがリンクした元AN1 HTML／ZIP／results／vectorは未取得であり、その実装の追試ではない。nativeの本番実装・有効化、学習済み本番LLM、ネットワークの安全性、LLM評価費用の下界を確認した記録でもない。

## 1. 判定の一覧

| 検証対象 | 結果 | 保証の範囲 |
| --- | --- | --- |
| 独立参照実装のテスト | **15群・229 assertions、PASS** | 小型の決定論的モデル、入力・重み・trace・Court・抽象状態の接続 |
| 独立した反例・回帰査読 | **52 checks PASS、失敗0、例外なし** | 正規生成器を迂回した悪意あるRoot、strict整数型、過大leaf、資料hold、同batch Final等 |
| 既存docs checkerの比較 | **FAILを維持、報告された新規失敗増分0** | 失敗メッセージとリンクキーのCounter比較 |
| 既存RFC-v2 integrityの比較 | **FAILを維持、7件の既存失敗から増分0** | 旧source bytesに関するFAIL行のCounter比較 |
| 既存ADR-v2 integrityの比較 | **FAILを維持、86件の既存errorから増分0** | 過去source／付録／metadata等のerrorのCounter比較 |
| 本番native・実ネットワーク・LLM費用定理 | **未検証／未完成** | fixtureや文書検査を実装・定理の代替にしない |

229 assertions、52 checks、RFCの受入要件の行数は、異なる分類で重複もあるため合算しない。「新規失敗増分0」は、既存の文書検査全体がPASSになったという意味ではない。

## 2. 独立参照実装

実装・profile・実行結果は[参照実装README](reference_impl/README.md)、[profile.json](reference_impl/profile.json)、[results.json](reference_impl/results.json)、[公開vector](reference_impl/test_vectors.json)に保存している。

| 項目 | 今回の独立実装 |
| --- | ---: |
| vocabulary | 259 |
| 内部次元 | 2 |
| 重み数 | 1,060 |
| 生成長 | 2 token固定 |
| 基準traceの論理レコード数 | 6,153 |
| trace木のpadded leaf数 | 8,192 |
| trace木のhash呼出数 | 16,383 |
| job token上限 | 8 |
| record byte上限 | 512 |

整数はchecked signed 64 bit、除算はfloor、浮動小数点は使用しない。実モデルに対するattentionや生成kernelの互換性を示す数値ではない。元AN1説明の7,226レコードや11群という報告値を、この独立実装の実測値へ読み替えていない。

### 2.1 実行結果を結び付けるコードhash

| 対象 | SHA-256 |
| --- | --- |
| `an1_reference.py` | `e5cca83dbbfc16cd39ccc23000573e84504a6a092d61af971570f193841cec4e` |
| `check_an1.py` | `3ee1ed855c215eba6dd0f38eae4b7dffa1386c219bf125d48075b56b79888161` |
| 独立postfix査読script | `428afd8ded32eab62dff02483ede50e5510755ab6b8d463fc2565a9881e71fa4` |

参照実装側の結果JSONは、実行した実装とテストscriptのhashを持つ。52 checksの結果は[独立査読結果](validation/independent_code_review_results.json)に、査読の手順と反例は[独立コード査読記録](validation/independent_code_review.txt)に保存した。[独立scriptの元source](validation/independent_code_review_script.source.txt)は当時のbytesを保存した記録であり、scratch上の配置に依存するpathを含む。

配布用の[相対パス版script](review_evidence/postfix_check.py)を別に作成し、再実行した[52 checksの結果](review_evidence/portable_postfix_results.json)も保存した。[manifest](review_evidence/manifest.json)は参照先・bytecode抑制・結果出力先だけの変更、検査関数のAST一致、全52行の結果一致を記録する。配布用scriptのSHA-256は`22527dd42191c4f229b07c58e946eeeaaee359f6649946bc918dd16e32daa6bf`である。再実行は既存結果を残し、別名の結果を出力する。

### 2.2 15群の内訳

| 群 | assertions |
| --- | ---: |
| actual input binding／canonical encoding | 17 |
| registration／Original／Panel順序 | 14 |
| 全再実行とcanonical Root | 12 |
| 有効な局所関係のNode Court | 26 |
| 認証された算術・構造反証 | 14 |
| canonical整数型・境界の回帰 | 64 |
| 無効証拠と重みindex認証 | 10 |
| 出力とpaddingの結合 | 5 |
| 自己申告属性でなく実資料Rootの照合 | 4 |
| 有界資料noticeとFinal前holdの回帰 | 12 |
| 一人正直Panelとnegative優先 | 6 |
| 未解決可用性と現在のFinal gate | 7 |
| slot／creditの原子性とrollbackモデル | 11 |
| 有用jobと採掘試行の識別 | 5 |
| strict targetと費用保証の境界 | 22 |

## 3. 独立査読で発見した二つの反例と修正

**整数型の不一致。** 初回実装は、native受付が整数countと等値のfloatを受け入れる一方、Courtはstrict整数を要求した。認証された誤計算traceでもCourtがInvalidEvidenceとなり、虚偽承認とclosure fixtureの下でFinalが成功する例があった。修正後はfloat、bool、数値文字列、負数、u64 overflowを元claimとnative contextの両方で拒否する。canonical整数countの誤traceは有効なComputationRefutedになることを確認した。

**有界Courtへ入らない過大leaf。** 初回は、512 byte上限を超える513 byteのleafを含むMerkle Rootを作ると、Panelが反証と報告しても、Nodeが証拠上限で拒否するため成功停止へ接続されなかった。修正後の独立検査では、正規Tree constructorを迂回するraw hash生成器から同種の攻撃Rootを構成した。Panelは元claim・期待Root・期待indexを持つ資料reportを返し、適格な資料noticeを同batchのFinalより先にnative holdへ接続する。四人の承認と真のclosure fixtureがあってもFinalはfalse、creditはゼロとなった。

無資格actor、別OriginalId、float scope indexのnoticeは拒否し、pending集合を変えない。元Rootへ認証された違反、認証できないInvalidEvidence、資料未充足を分離する。単なる不正packetをMinerの有罪証拠や無条件holdへ変えない。

**資料caseの客観的な応答・解除・終結手順は、この参照実装では未実装である。** 今回確認したのは適格noticeによる成功停止との接続であり、実ネットワークのDA、虚偽未着の判定、完全な資料case活性を証明したものではない。初回の失敗記録を修正後PASSで書き換えず、別時点のコードhashと反例として独立査読記録に保持している。

初回の実装本体は保存していないため、修正前の版をこの配布物だけで再実行できるとは主張しない。[初回反例のscript](review_evidence/initial_counterexamples.py)、[当時の観測結果](review_evidence/initial_counterexamples_results.json)、実装hashが証跡として残る。

全Panelが悪意で、検査・closure fixtureも真と偽装され、正直な反証が届かない場合に不正claimが通る境界テストも含めている。この結果を隠して無条件安全性を主張しない。

## 4. 文書checkerのbaselineとpost比較

baselineは今回の添付RFC ZIPから展開した元snapshotに対して、2026-11-18 06:38:03 UTC（15:38:03 JST）に実行した記録である。先行作業で作成したディレクトリ名に別方式名が含まれるが、AN1の採用方式を表さない。元ログ・JSONを[baseline](validation/baseline/summary.json)へbyte単位で保持して複写し、[copy manifest](validation/baseline-copy-manifest.json)へサイズとSHA-256を記録した。元snapshot、旧revision、公式baseline、三つのchecker本体は変更していない。

postは更新treeに対して実行した同じ三つのcheckerの出力である。[比較結果](validation/document_check_delta.json)は、単なる件数比較ではなく、各エラー文字列またはリンクキーの**Counter（重複発生回数を保持する集合）**を比較する。既存エラーが一つ消え新規エラーが一つ増えた場合も新規増分として検出する。

| 比較対象 | baseline | post | 新規増分 | 消滅数 |
| --- | ---: | ---: | ---: | ---: |
| docsのfailure entries | 947 | 947 | 0 | 0 |
| 公式baseline未登録のlink issue発生回数 | 930 | 930 | 0 | 0 |
| RFC-v2のFAIL行 | 7 | 7 | 0 | 0 |
| ADR-v2のerror entries | 86 | 86 | 0 | 0 |

docsの947 failure entriesは、906のlink issue記録と41のparameter source欠落記録からなる。linkの記録には`xN`という複数回発生があるため、発生回数は930になる。公式checkerの「new link issue」は**そのcheckerに元々登録されたbaselineに対して未登録**という意味であり、今回AN1が新設した不備数ではない。

既知issueの集計は43、存在しないdocs先への参照は971、見出し欠損は2で、baselineとpostで同じである。既知issueの43件はcheckerが公開する集計値を保存しており、個別キーを出さない集計から完全な差分検証をしたとは主張しない。新規未登録linkについては、出力に含まれるsource／target／fragmentのキーと重複回数を比較している。

RFC-v2の7件は、RFC-0009／0011／0014／0023／0026／0037／0041について、過去source bytesとの不一致が元inputから残るものである。ADR-v2の86件も、過去source・付録・status宣言・metadata等の不一致が元inputから残っている。これらの過去台帳をAN1に合わせて書き換えてFAILを消していない。

ローカルリンク検査は、仕様の数学的健全性、全consumerの実装、モデルの有用性、資料取得やCourt包含を保証しない。外部URLやパッケージ外runtimeコードもcheckerの検証対象外である。

## 5. 再実行

更新済み`current`ディレクトリで、参照実装を次のように実行する。

~~~bash
python3 docs/maintenance/revisions/2026-11-18-an1/reference_impl/check_an1.py
python3 docs/maintenance/revisions/2026-11-18-an1/review_evidence/postfix_check.py
python3 docs/maintenance/revisions/2026-11-18-an1/check_revision.py
~~~

文書チェックは、同梱のbaselineを上書きせず、postだけを更新して実行する。既存失敗が残る三つの公式checkerはexit 1を返す。失敗を無視してPASSと表示せず、出力を保持して差分を確認する。

~~~bash
python3 docs/maintenance/check_docs.py --json-out docs/maintenance/revisions/2026-11-18-an1/validation/post/check_docs.json > docs/maintenance/revisions/2026-11-18-an1/validation/post/check_docs.log 2>&1
python3 docs/rfc-v2/check_integrity.py > docs/maintenance/revisions/2026-11-18-an1/validation/post/rfc_v2_integrity.log 2>&1
python3 docs/adr-v2/check_integrity.py --json docs/maintenance/revisions/2026-11-18-an1/validation/post/adr_v2_integrity.json > docs/maintenance/revisions/2026-11-18-an1/validation/post/adr_v2_integrity.log 2>&1
python3 docs/maintenance/revisions/2026-11-18-an1/validation/compare_checks.py
~~~

[compare_checks.py](validation/compare_checks.py)は既存結果を読むだけで、公式checkerや旧baselineを変更しない。結果の`NO_NEW_REPORTED_FAILURES`とexit 0は、「報告された失敗の新規増分がなく、比較に使ったchecker本体がbaseline時と同じ」という意味であり、公式checker全体のPASSではない。文書をさらに編集した場合は、三つのcheckerと比較を再実行して新しい結果を保存する。

## 6. 未実施・未証明の境界

- 実owner署名、鍵管理、beaconの認証・予測不能性、後続Panel選出pool・抽選イベントの真正性。
- ネットワークのcold資料取得、保護されたCourt包含、資料caseの客観的解消、全合法要求下の正常完了。
- native DBの並行性、全consumer・RPC・import/repair・全carrierへの実接続、実際のreorg・IBD・pruning。
- 学習済み本番LLM、全opcode・全shapeのtrace展開完全性、最低Nodeの最悪負荷、全carrier合計1 BPSの実測。
- 最適化、cache、batch、外注、入力選別を許したLLM評価費用下界、通常LLMと同額以上のGPU・電力・金銭費用。
- DAG common-prefix、stable freeze、収束、settlement、全forkにわたる一回性。

Panel＋Courtの条件付き正当性は、能動的な正直検査者と実効的な資料・異議機会を前提とする。テストでその境界をfixtureとして与えたことは、現実のネットワークにその条件が成立する証明ではない。

## 7. 白書成果物の確認

改訂統合稿は全18章・13ページであり、同じ原稿からPDF、編集用TeX、MathMLを含む自己完結HTMLを生成した。PDF中の60種類の数式の描画失敗はゼロ。全13ページを描画して目視し、Final・確率の式と密な表はページ単位でも確認した。文字・表の重なり、ページ外への欠落、数式の判読不良は見つからなかった。[PDF QA](whitepaper/pdf_qa.json)はPDFと原稿のhashを記録する。

PDFはReportLabから生成したもので、TeX source自体の直接コンパイルは未実施である。元whitepaper v0.3の原PDF・TeXは未取得のため、前回補完稿からの改訂として記録している。
