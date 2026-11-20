# MOE1検証記録

日付: 2026-11-20（Asia/Tokyo）。対象は設計文書の改訂と再配布であり、native/EVM実装・ネットワーク有効化・経済均衡・PoW費用下界の検証ではない。

## 判定

**新しい経済条件の文書改訂、旧原本の保全、整数会計の検査は完了した。従来の全体checkerがすべてPASSになったわけではない。** 改訂直前のTB1/FOW1 snapshotと診断を比較し、新たなリンク欠損を残していない。今回の新設計による旧sourceとの不一致は、元原本を保存した明示変更として記録した。

## 文書の範囲

- 直前版の既存2,702ファイルは、全て元のパスに存在する。
- 既存71ファイルを改訂し、変更前原本71件を `before/*.txt` へexact bytesで保存した。
- 残り2,631ファイルは元snapshotのSHA-256と一致する。
- 原ADR9本の全面改訂、ADR0162の適用追補、RFC-v2-0011とADR-v2-0021/0023の著述本文、関連入口・索引・registryを更新した。
- 新RFC0063、ADR0231、共通経済移行契約を追加した。現在の番号文書はRFC64本・ADR227本・RFC-v2 20本・ADR-v2 50本。
- 変更した既存ファイル集合は、各担当の事前対象・root統合対象・索引生成対象の和集合と一致し、対象外の変更・削除はない。

[基準manifest](baseline-manifest.json)と[変更前後のdigest・原本位置](changes.json)を同梱する。改訂範囲の再検査は[check_revision.py](check_revision.py)、結果は[revision-check.json](revision-check.json)。

## 経済規範の確認

独立した読み取りレビューで、新RFC0063・ADR0231・RFC-v2-0011、原ADR9本・ADR-v2-0021/0023、および主要隣接差分を確認した。追加登録と一意市場の原子生成、core市場なし、coreのR全額Miner、追加モデル50%買上げ、通常5%/3%/残額、内部buyback追加fee0、直接transfer、同期EVM journal、membership、winddownの間に、確認範囲で未解消の必須訂正はない。

市場の存在をFOW1採掘適格性や売却時の実資金保証と混同しない。旧20% Panel poolは例示planであり、全profile共通の新規則にしていない。証拠は[独立レビュー](economic-final-independent-review.json)と[統合範囲監査](economic-scope-audit.json)。

## 整数会計

[検査プログラム](check_accounting.py)で、0〜10,000および32/64bit境界の計10,005金額を扱い、合計50,025ケースで保存則を確認した。

| 確認事項 | 結果 |
| --- | --- |
| 追加モデルB=floor(R/2)、D=R−B | B+D=R |
| GENESIS/FORK_DESIGNATED | Miner=R、買上げ・Panel・owner等のR控除0 |
| 旧20%残余poolの例 | buyback+Panel+Miner=R |
| 通常AMM | burn+owner+net=gross。netは残差 |
| 1,000 BILI例 | 追加500/100/400、core1,000、通常売買50/30/920 |
| 内部buyback / 直接移転 | 追加AMM burn・owner leg0 |

これは文書化した式の算術検査であり、本番native整数型、overflow実装、AMM曲線、資金移動、EVM revert、攻撃耐性の実行試験ではない。GPU費用、経済利益又はPoW成立を証明するものでもない。

## 歴史・証拠の保全

[独立保全checker](check_preservation.py)は、改行や空白を正規化せずraw bytesを比較した。[結果](preservation-check.json)は次の全件PASS。

| 保全対象 | 確認数 |
| --- | ---: |
| RFC-v2の既存原条文suffix | 15 |
| ADR-v2のADR_SOURCE block | 204 |
| 既存TB1/FOW1囲みblock | 146 |
| TB1/FOW1単独numbering marker | 1 |
| history/evidence/audit既存ファイル | 1,446 |
| 過去revisions既存ファイル | 269 |
| 旧checkerソース | 3 |
| source-manifest類 | 11 |
| 既知リンク問題baseline | 1 |

## 従来checkerの結果を維持して開示

| 従来checker | 改訂直前の診断数 | 改訂後の診断数 | 差分 |
| --- | ---: | ---: | --- |
| check_docs | 947 | 947 | 新たなリンク等の診断なし |
| RFC integrity | 7 | 7 | 新たな診断なし |
| ADR integrity | 92 | 150 | 明示改訂による旧source/著述checksum等の比較差分58 |

58件は、旧規範のbytes・行範囲・Statusと、今回改訂した現在本文を旧checkerが比較することによる診断である。旧本文自体の喪失を意味するかどうかは、別の保全検査で確認した。新しい現行規範を取り込むために旧manifest、基準値、checkerコードを上書きし、FAILを隠す処理はしていない。

改訂直前にも存在した947/7/92件の診断を、この経済改訂で解消したとは主張しない。現在の全体checkerはFAILを返す。新しい診断の分類と全ての旧・新ログは[比較記録](checker-comparison.json)、`checker-baseline/`、`checker-after/`に収録した。説明不能な新規診断は0件。

ADR0088からの既存参照を維持するため、改訂ADR0087の権利節に旧fragmentの互換anchorを置いた。過去の非譲渡規則を復活させる変更ではない。

## 有効化までに必要な作業

設計状態はADOPTED_DESIGN_NOT_ACTIVATEDである。原子的モデル・市場登録、native/EVM共通journal、transfer/allowance/membership、実reserveと買上げ会計、各claimの経済snapshot、旧cohortとcore市場のwinddown、core検証役務の別原資確保を実装・検査し、明示activationを行う必要がある。過去版の実装成功・測定記録を新規則の完成実績として転用しない。
