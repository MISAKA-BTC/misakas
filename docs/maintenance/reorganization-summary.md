# RFC20・ADR20の責務分離

日付：2026-11-26。前回のRFC20版を基に、ADRを235本から20本へ整理し、設計と判断理由を双方向に移した。元の添付docs(6).zipからのRFC71本→20本の整理も含む。

## 文書の役割

| 文書 | 正本として記す内容 |
| --- | --- |
| RFC | 適用範囲、型、数式、データ形式、状態遷移、原資、期限、制約、拒否条件、移行、受入条件 |
| ADR | 背景、簡潔な選択、判断理由、比較した案、費用・欠点・帰結 |

20分野で同じ番号を対応させ、全RFC・ADRを相互リンクした。保証の仮定と検査可能な安全条件はRFCに残す。設計の具体性を削って理由だけにしない。

## 主な内容の移動

- 担保の予約分解、損失下限、期間枠、返金・保持、account最低担保案をRFC-0012へ集約した。
- Positive assignmentのfield表、独立job導出、資金・署名・役割・nonce・一回消費と、限定relayの署名・cursor・custody・復元契約をRFC-0010へ集約した。
- 基盤署名・identity、kernelの算術、prefix依存、Court容量、Gate Cのconsumer、EVM境界、UTXO commitment、RPCの型・原因保持を担当RFCへ移した。
- Qwenの具体構成・scale・生成方式の選択理由、Gate Cを設ける理由、MWA1の共通targetを選ぶ理由、relayの代替案をADRへ移した。
- BFTだけのモデル追加、全モデル共通MOE3、個別claimのPanel/Court/Gate C、計算の暗号学的validity proof不採用を保持した。
- MWA1、新認証state map、account最低担保案、直列witness時計案は提案のまま。採択済みprefix exportと未割当wire候補も区別した。

## 旧本文の除去

旧71RFC・235ADRの本文、補助directoryに残っていた6件の草案・転送コピーを配布物に残さない。archive、別履歴ZIP、v1/v2の並行文書体系を作らない。完全な旧pathの小さい対応表と削除一覧だけを持つ。重複していた旧ADR0026・0173・0193・0210は完全pathで解決する。

前回RFC20版に含まれたRFC/ADR本文の合計は4,256,854 bytes、今回は331,687 bytesで、92.21%減少した。補助codec・運用資料・生の検証記録は別用途として保持する。この数値はZIP全体の削減率ではない。

## 検査

| 検査 | 結果 |
| --- | --- |
| RFC・ADRの連番、件数、相互参照 | 各20本、重複なし |
| 旧参照の完全path対応 | RFC71件、ADR235件 |
| 現行RFC/ADR・governance・主要入口のローカル参照 | 欠落・見出しエラー0件 |
| Markdown参照解析 | 16件成功、終了コード0 |
| QF1参照codec・位置・hash・Ticket | 57件成功、終了コード0 |
| QF1／MOE3の数値・参照データ | 15項目一致 |
| 前回配布版との全体リンク比較 | 新規欠落0件。既存1,288件から1,236件へ減少 |

全docsをリンク正常とする結果ではない。古い試験ログなど同梱されていない対象への参照が1,236件残る。旧allowlistに基づく全体checkerは終了コード1となる。今回の差分は[source-link-comparison.json](source-link-comparison.json)、現行文書は[design-docs-validation.json](design-docs-validation.json)で判定する。存在しない証跡を補完・実証済みとは扱わない。

これは文書整理と参照codecの検査であり、実LLM、native Court、BFT投票、devnet、計算費用下界、公開ネットワークを検証したものではない。

## 適用

作業ブランチで未コミット変更を保持し、ZIP内のdocsと比較して追加・更新・削除を適用する。**上書き展開だけでは旧文書が残る**ため、[削除一覧](removal-manifest.json)も反映する。runtimeのenum、wire tag、domain、DB schema、旧claim IDを文書番号で一括置換しない。

```bash
python3 docs/maintenance/generate_indexes.py
python3 docs/maintenance/check_design_docs.py
python3 -m unittest discover -s docs/maintenance -p test_doc_tools.py
python3 docs/design/palw/models/qf1/check_qf1_reference.py
```

公開設定、fence、既存台帳、実支払いの変更は含まない。実装の完了状態をRFC/ADR本文へ列挙しない。
