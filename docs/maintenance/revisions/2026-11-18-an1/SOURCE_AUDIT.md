# AN1 source監査と根拠

## 実際に読んだもの

- 添付`MISAKA_RFC_0056-0058_AuditRevision_20261117(2).zip`の文書一式。RFC-0055の依存条件、0056/57/58の全本文、ADR/PRINCIPLES/AGENTS、現行bookの入口・担当・検査仕様を確認した。
- ユーザーの最新AN1設計説明全8節。入力束縛、一意性、転用防止、帰納法、正直Panel条件、元実装の報告値、費用下界の未取得を参照した。
- 本会話で作成したFULL1/VF1統合補完稿v0.3。新方式へ転用できない費用定理と、維持する最適化・会計の限界を区別した。
- 添付の会話記録テキスト。元資料・過去作業の記録として扱い、現在のnative実装の証拠にしていない。
- Hugging Faceの[Cache strategies](https://huggingface.co/docs/transformers/en/kv_cache)。共通prefixの直接再利用に関する一般背景の一次資料として確認した。これ自体はAN1の費用下界ではない。

## 読み出していないもの

元PALW whitepaper v0.3のPDF/TeX、ユーザーがリンクしたAN1 HTML/ZIP/results/vector、実native repository、実ネットワークの運用記録は今回取得していない。元AN1の11群・7,226レコード等はユーザーからの報告値であり、今回の独立実装の実測値に混ぜない。

既存資料が引用する固定main commitのsource監査は過去記録として保持する。今回それを再fetch・実行して現在mainの状態を確認したとは扱わない。

## 変更の権限と範囲

ユーザーは最新指示でAN1への変更とPanel Court方式の維持、whitepaper/RFCの修正を明示した。この範囲で文書・参照実装を編集した。Groth16案を追加せず、旧AGENTS/PRINCIPLESの計算validity proof不使用方針を維持した。新方式の公開採択・activationを推定していない。

0059/0227の番号は添付一式内で未使用を確認した。未取得remoteや他branchに対する番号予約は保証しない。既存番号・wire tag・署名domainを再利用したと扱わない。

## 証拠の保全

`source-baseline.json`に添付current配下の全ファイルのサイズとSHA-256を記録し、`changes.json`に今回の変更・追加・削除を記録する。元の過去検証、原則、0055、旧ADR等は保全する。旧0056〜0058とwhitepaper補完稿の元bytesは新revisionのsource記録・差分から追跡できる。

文書check・RFC/ADR integrityは、入力時点の失敗と今回の差分を比較する。公式baseline、歴史source-map、過去adoption-mapを更新して失敗を隠さない。
