# A0 — 担当分担と統合手順

契約の入口は [COMMON-CONTRACT.md](COMMON-CONTRACT.md)。各担当はA0の共通commitをbaseとする別branch・別workspaceで進める。A0以外は共通schema・module登録・署名domain・activationを直接変更せず、必要な差分をA0へ返す。

| 担当 | 主な所有範囲 | 最初の受入基準 |
| --- | --- | --- |
| A0 共通仕様・統合 | positive-contract crate、workspace/module登録、契約・番号・統合記録・切替 | 同じcodec/vector/APIを参照でき、native未接続は拒否 |
| A1 Node検査 | 専用checker module、CoveragePlan/進捗/費用、既存局所検査の副作用なし呼出し | モデル未保有Nodeが全必須範囲を導出。省略・別モデル・別入力を拒否 |
| A2 Miner | `palw_kernel_roles.rs`内の新入口、専用資料応答/provider module | 結果のみcommit。証拠生成APIなしで進行し、全Panel上限と委任を守る |
| A3 Panel | `palw_kernel_verifier.rs`の新入口、専用builder、remote pursuit専用module | 一度の再実行から証拠構築・固定・分割提出。Cleanは合格ではない |
| A4 Court基盤 | 新versionの受付/進捗/fold/分割搬送、共通G14予約・期限 | 旧Fraudと共存。Panel拒否とMiner反証を分離し、仮の受付成功なし |
| A5 信用・会計 | security certification、全信用reader、既存hold/返金/精算、依存 | Positive未完了は全consumerで無信用。提供済み資料料金を成功報酬から分離 |
| A6 保存・復旧 | 専用schema/journal/undo、row stream、pruning/IBD | 進捗・予算・料金・責任が同時に復元。保存Passedだけを信用しない |
| A7 独立検証 | 独立期待値・攻撃fixture・差分・実プロセス・負荷計測 | 実装担当の自己申告を合算せず、統合candidateそのものを検査 |

共有ファイルの編集責任者は一人。各担当は専用moduleへ処理を置き、module登録・共通型・旧新切替はA0へ依頼する。A1・A3・A7は別担当を維持する。

## 結合順序

1. A1+A3+A4: 小さいTIR/EXACT。偽中間値、最後の工程欠落、別class/入力/branch差替えまで拒否する。
2. A2+A3+A4: warmのMiner要求ゼロとcoldの適格原資料応答。ACK隠し・重複/overlap・鍵変更・provider・Panel放棄を試す。
3. A4+A5: 実裁定を全consumerと既存money engineへ接続。支払/返金の一回限り適用と原資保存を確認する。
4. A4+A5+A6+A7: restart、再送、Panel交代、reorg、IBD、pruningを通す。どの予算・責任も片側だけ残さない。

A5/A6は共通契約から先行実装する。模擬イベントの単体試験は必要だが、実Court接続完了とはしない。A7は初期からMiner/Panel/Node別費用、raw bytes・CPU・RSS・保存・cold/warmを採る。実測されたjob形状と最大文脈を混同しない。

## 提出形式

```text
対象commitとbase契約revision:
変更した責務・ファイル:
接続したAPI:
実行した試験とraw log:
未接続・未検証:
残っている仮実装:
他担当へ必要な変更:
有効化設定: 無効 / 変更なし
```

未接続箇所に仮の成功を置かない。A0は共通契約の変更を先にcommitし、小さい差分を順次統合する。担当ごとの別commitの成功結果を足し合わせて全体完成にはしない。統合candidateで旧G14回帰・新規受入試験・資源/資金/保存の原子的失敗試験を再実行する。

## 以前の試作の扱い

元checkout `/Users/wata/Downloads/MISAKA-testnet` の試作は未統合の参考資料。[凍結したsource hash一覧](a0-workspace.json) を保存した。A0 workspaceへ実装を一括コピーしていない。

特に、私設4KiB chunk rootと整数fee ledgerはG14の実inventory/原資予約ではない。署名済み結果の任意claim IDで旧Gate Cを読まない。以前の型/domainを新しい共通契約へ黙ってaliasしない。A1〜A6は必要な処理だけを各担当で移植し、共通crateへ明示的に接続する。

このチャットではA0 workspaceのみ作成した。A1〜A7のユーザー所有チャットやworkspaceは自動作成していない。
