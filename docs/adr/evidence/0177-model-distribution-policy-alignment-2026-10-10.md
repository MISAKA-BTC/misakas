# ADR177 モデル入手不介入・model-bond配分の文書整合記録 — 2026-10-10後続改定

この記録は、ユーザーの新方針と「MISAKA Torrent・Seeder報酬の概念も廃止する」という追記を、
`pre`のdocsへ反映した範囲を示す。[ADR-0177](../0177-model-bond-allocation-without-availability-consensus.md)を正とする。

## 判定と範囲

**文書検証: PASS。** 既存203 Markdownと新ADR177を改定し、共通優先注記201文書、追加ローカルリンク268件、
新規範と旧Final/activation条件の撤回、fence整合、`git diff --check`を確認した。
作業開始時からのコード・Params・fingerprint等の非文書変更は0件で、branch/HEADも維持した。

結果・対象一覧・改定前後digest・撤回した旧sectionを
[JSON記録](0177-model-distribution-policy-alignment-2026-10-10.json)へ保存する。
検証対象は追加したローカルリンク、Markdown fence、旧Final/activation述語の撤回、
新規範の接続、全共通前提文書の優先banner、`git diff --check`、作業開始時からの非文書変更の有無である。
既存の欠落リンク/未配置ADRは修復対象にせず、追加リンクだけを検査する。

## 規範的変更

* MISAKA Torrentの採用/統合、専用Bonded Seeder・Seeder報酬/固定15%配分を廃止する。
* PoR/Full Fetch/lease/TRDC/FPR・全モデル強制公開費用、配布起因資格/weight停止を撤回する。
* 固定Model ID/root/weights/specと個別claimの有限認証証拠/courtを維持し、model取得の強制へ転用しない。
* 重複しないモデル別miner拘束元本からcoinbaseを配分し、network/model/個別bond予算、Q/B/R/F capと共通DAA時計を維持する。資本だけでは報酬を払わない。
* 公開による外部資本獲得の圧倒的経済優位を評価目標とする。同額資本から所有者独立性を識別できるとはしない。

RFC14 §16と前段のFinal/実装段階/activation条件、ADR173 D9–D11、RFC15のadmission/Final/§8.5、
RFC06/08/09/10/11/12/13、ADR62/67/166/seat-root173/175/176と関連実装spec・索引を改定する。
他のRFC/ADR・主要docsには、旧方向よりADR177を優先する共通注記を付す。

## 未完了事項と履歴

**経済優位・倍率式・配分会計実装・条件付きG14 E2E・Panel=0 activationは未完了である。**
この文書チェックは、公開/閉鎖の敵対的シミュレーション、実ネットワーク計測や合意試験のPASSではない。
全所有者拒否時のモデル取得保証は撤回し、モデル未取得時の実効検出確率p=0も未解決として経済評価へ含める。
model operandを含む許可court unit・累積scope、`f`・元本snapshot・期間/端数/未使用枠と移行は別途確定する。

従来の[Seeder文書チェック](../../rfc/evidence/0014-independent-bonded-seeders-audit-2026-10-10.md)・
[FPR文書チェック](../../rfc/evidence/0014-forced-public-retrieval-alignment-2026-10-10.md)は撤回前の履歴であり、
現行方針の完成証拠へ流用しない。旧claim、runtime、既存の試験/測定/activation記録は保持する。
モデル配布Seederと、既存P2P node meshやAMMの市場seed拠出者は異なるscopeであり、後者の履歴は改変しない。
この依頼でコード・Params・fingerprint・稼働設定を変更しない。以前の依頼によるdirty変更も維持する。
