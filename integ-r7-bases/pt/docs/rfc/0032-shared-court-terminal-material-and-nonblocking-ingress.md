# RFC-0032 — 共有courtの終端materialと非独占提出

Status: **Draft / Review Required**。利用者はA（認証済みmaterialと終端intent）の詳細仕様化・隔離実装を承認済み（2026-10-21）。採択・本番規則の置換・公開wire・有効化は行わない。判断案は[ADR-0199](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md)。既存の[共有frontier契約](0029-owner-independent-shared-tir-query-data.md)と[正本](0026-court-policy-master-contract.md)の未完を埋めるための提案。

## 1. 解く問題と確認済み範囲

全owner枠が共謀者でも外部検証者一人が進めるF03では、最初の不完全な証拠bodyがroot/path共通slotを占有して他の提出者を止めてはならない。現行の数値fragment adapterはcomplete bundleだけを検査し、未完成prefixの合意保存・独占を導入していない。このままではmulti-block提出は未実装である。

[依存検証](../evidence/palw/g14-integration-candidate-2026-10-20/terminal-dependencies.md)では既存GQA fixtureの一つのnamed leafの二履歴・24終端でparameter opening/carriageを実際に使用し、個別欠落48ケースは拒否した。共有DATAのStep/Rows/Tile/Trace/Idsだけで終端証拠を再構成できるという前提は置けない。prompt/decodeはその24終端では未使用で、全original・全modelの依存を測定した結果ではない。

## 2. 比較する方式

| 方式 | 未完成bodyの扱い | 追加で閉じる必要があるもの |
| --- | --- | --- |
| A: 認証済みmaterialと終端intent | 任意filerのbody prefixを共有終端slotの所有権にしない。原始materialを認証・保持し、適格な外部filerのintentに対して既存builderで終端を構成する | parameter/promptを含む閉じた依存集合、担当者・保持契約・搬送予約、構築予算、決定論的canonical化、全合法分岐費用と現実の原資 |
| B: filer別の独立証拠stream | filer間でbody進行を分離し、完了証拠のみroot/path結果へ統合する | 無限のfiler/streamに対する有限の有料受付・記憶上限、共謀者の再充填下でも一人が完了できる予約・公平性の実証 |

利用者の選択に従い、Aで詳細仕様化・隔離実装を進める。Aも現段階ではF03を保証しない。Bを単にActive bond/fee/先着slot数で解決済みと扱わない。Aを採る場合でも、既存のcomplete-bundle入力を無条件の有料・公平提出保証に昇格させない。

## 3. Aの認証・canonical化境界

- 合意状態の実original、登録program/layout、artifact root、prompt commitment、native rules/profileを起点にする。受信者指定のrootやcallerのchecked flagを信用しない。
- parameter materialの識別はartifact identityと既存inventory座標。受理時にleaf数・座標・tensor/layer/row piece・dtype/長さを登録programから照合し、既存Merkle openingを検査する。rootが合うだけで別inventory形状を許さない。
- promptは実nativeのFlat/Merkle形に従いoriginalへ結び付ける。生成IDとpromptを混同しない。既存の無効なモデル改善・画像動画経路を追加入口として有効化しない。
- 同じ認証済みmaterialはfilerの所有にしない。同じ論理座標への異なる有効なencodingを新報酬・別slot・timestamp resetにしない。断片のhashだけでmaterial認証済みとしない。
- `PalwTirEvidenceStoreV1` adapterは保持済みの認証済み入力だけを読み、任意のlocal cache/network storeへ黙ってfallbackしない。欠落は明示的refusal/未供給であり、数値有罪・default・slashへ変換しない。
- 既存builderのcanonical証拠と同じbyte列・native local outcomeを生成する。構築は全投入資源を事前にadmitし、失敗で状態を部分更新しない。intent署名は依頼者の認証であり計算の正しさの証明ではない。

## 4. 供給・原資・期限は別の必須条件

Aの原始materialも、最初の不完全な送信が唯一slotを占有するなら同じ問題を再発させる。canonical materialの供給担当者と代替配送、必要fragment集合、実供給・配送・収録、回復義務を既存F02契約へ結び付ける必要がある。producer以外のmaterial保持者へ義務を新設した扱いにしない。既存契約を用いるなら、その署名authority、実支払原資、保持horizon、履行可能性を実データから照合する。契約が不足すればoriginal受付を成功扱いにしない。

費用はoriginalの全許容分岐について、material fetch/authentication/retention、root/round/intent、builder再実行、multiproof結合、署名・hash・拒否/重複ingress、snapshot/restart/IO、未決責任archiveまで積算する。共有重複排除は実装と費用証拠がある箇所だけに適用する。未没収bondをPanel売上としない。最低Account Bondが運用資本を保証するとも扱わない。

最初のnoticeは承認済み隔離Aの `a <= d <= a+3,120`、hardは元originalの `a+10,000` を維持する。後続intent/material/再提出でresetしない。全前提の実成立後に必要処理がhard内へ収まること、未供給ならno-slashとなることを検証する。hard到達やTTLだけで担保返還・証拠削除しない。

## 5. 検証と進行順

1. 固定した既存fixtureで、原始materialだけから既存builderへのadapterを作る。正直/偽数値履歴、全兄弟、parameter/prompt/decode依存、欠落・改変・別original/別artifactを検査する。元fixtureを通るよう差し替えない。
2. 実native shape preflightと原子writer、独立namespace、保持/復元/journal/undo/reorgを揃える。旧wire tagとX3の意味を保持する。
3. 全originalのmaterial依存・資源・担当者・予約・資金を一括で照合し、受付・最遅notice・回復・終了効果を接続する。単一leafの試験を全分岐保証にしない。
4. 実nodeで全owner共謀・不完了/再充填・外部一人・restart/reorg/pruned IBDを含むT05を実施する。全T01–T15/R-D2、実費用、実model doorを別途満たすまで武装を拒否する。

未決の単価・受付率・配送時間・保持期間・actor契約は仮値で埋めない。本書だけで上限引上げ、model排除、モデル審査、暗号学的計算証明、本番規則の採択を行わない。

## 6. 隔離borrowed material viewの実装範囲

`SharedTirBuilderStoreV1`は保持済みoriginal/sourceへ束縛した認証済みunitと、登録programで再検査したparameter openingだけを読む。必要materialがなければNone/構築拒否とし、fixture execution・disk・networkへfallbackしない。元sourceに束縛した両端のStepLeaf openingから連続runのcanonical siblingsを導くnative変換を追加し、全leaf hash treeの隠れた読み出しを避ける。新しい計算証明ではなく、既存Merkle commitment上のデータ変換である。

[実装・検証記録](../evidence/palw/g14-integration-candidate-2026-10-20/material-builder.md)では既存GQA fixtureの正直/偽数値履歴・24終端で従来builderとのbyte一致と欠落拒否を検査する。parameter集合の試験供給には直接builderのread-setを用いるため、全originalの先読み・依存閉包を実装済みとはしない。

このcheckpoint（7de51563d）ではborrowed read-only viewであり、prompt・row-pin、永続化・再起動、materialの供給/保持/原資・全分岐予約、実際の処理機会と公平なmulti-block提出は未完。clone・multiproof/range組立・builder outputを含む完全な資源/費用profileも別途必要である。本節はAの限定的な隔離実装記録であり、F03/T05や本番規則の採択・有効化を示さない。

## 7. Promptとrow-pinの隔離projection

保持済みoriginalのwork rulesからprompt形式を解決し、Flatの全listまたはMerkleの供給済みtileを既存commitmentへ認証する。形式の差替え、改変、個数違い、重複/逆順tileは拒否する。未供給tileを取得済みにはしない。

row-pinは認証済み全IDと、committed token/要求laneの二つのTraceTileから既存native形式へ構成する。whole rowsやlocal cacheへfallbackしない。これらはidentityを保持するデータ変換であり、裁定結果・計算credit・供給証明ではない。

[検証範囲と限界](../evidence/palw/g14-integration-candidate-2026-10-20/prompt-row-material.md)。永続化、promptのbounded wire入口、全originalの依存閉包・供給/保持/原資・予約、完全な費用profile、F02/F03と実node T05は引き続き未完である。
