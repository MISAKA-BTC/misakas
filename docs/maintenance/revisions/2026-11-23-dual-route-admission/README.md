# 2026-11-23 MAD2二経路モデル採用の改訂記録

状態: ADOPTED_DESIGN_NOT_ACTIVATED。ユーザーの明示指示による文書変更。

## 対象

入力: misakas-pre (2).zip。
基準commit（ZIP comment）: 7dddad7298d3a45e5ba07b6047cb32863c7540ce。
入力SHA-256: eccf17e60a485caed0c5936fd16a07b7ac3c3e7064f9c20d6259e76b1e77eb3e。

## 変更

- [RFC0067](../../../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)と[ADR0235](../../../adr/0235-model-admission-kernel-auto-and-bft-decision.md)を追加。
- [移行契約](../../../governance/model-admission-dual-route-transition.md)と[機械可読policy](../../../design/palw/models/mad2-admission-policy.json)を追加。
- RFC0062/0065/0066の採用条件をAUTO又はBFT YESへ変更し、current entry、原則、関連ADR、registryと索引へ反映。
- BFT側はG-COLL定理・G-COST/G-USEの証拠提出を不要にし、共通技術互換・署名・quorum・停止と個別claimの検証を維持。
- [必要性監査](../../../design/palw/consensus/verification-responsibilities-and-simplification-2026-11-23.md)に、PoWとの差、各機構の役割、実装の不足と共通化案を記録。

## 追加指示の反映

元genesisはBFTのモデル採用・変更・停止・解除の対象外。追加モデルは自動/BFTのどちらで採用してもBFT管理対象。BFTはforkモデルを追加でき、Optionなし・Miner=Rを適用するがBFT停止対象とする。分類・不変origin・共有rule停止のgenesis除外・fork原子登録をRFC0067へ追記し、RFC0063/0064/0065、関連ADR、経済policyに整合させた。元genesis以外へ停止免除を作らず、個別claimの検証と既存永久LP保護を維持する。

## 保全

変更した既存ファイルの元bytesはbefore/に.txtとして保存する。change-manifest.jsonは入力に対するhashと変更範囲を示す。統合RFC/ADR内の歴史payload、実測ログ、Rust/実行スクリプト、公開fence、配備、実支払いは変更しない。文書用の索引生成器と本改訂の検査器は文書整合性のための変更である。

## 検査

入力には既存の欠落参照と歴史digest不一致がある。既存baselineを消したり一括更新してPASSにしない。validation/のbeforeとafterを比較し、今回の新規リンク不備、本文・機械可読policyの不整合、歴史payloadの改変、runtime変更がないかを別に確認する。

check_revision.pyは本改訂の保存原本・文書policy・変更範囲を確認する。暗号検証・native実装・3ピア試験・Qwen困難性の証明ではない。

現在のADR-v2編集本文に付けた優先注記に合わせ、変更した冊子のauthored_section_sha256を更新した。変更前の登録hash・実bytesのhashと新hashはeditorial-checksum-updates.jsonに記録した。このうち0021・0022・0050の編集本文hashは入力時点でも不一致だった。歴史source manifest、履歴本文、既存リンクbaselineは更新していない。

実行:

    python3 docs/maintenance/check_docs.py
    python3 docs/rfc-v2/check_integrity.py
    python3 docs/adr-v2/check_integrity.py
    python3 docs/maintenance/revisions/2026-11-23-dual-route-admission/check_revision.py

最終結果はvalidation/summary.jsonを参照する。今回行っていないものは、Qwen推論、BFT実投票、nativeビルド、ブロック生成、公開activationである。

## 作業ブランチへの取り込み

この配布物は入力ZIPの基準commitからの文書改訂であり、Mac上の最新作業ブランチ全体を置き換えるものではない。新しい実装commitがある作業先では文書差分をレビューして取り込み、実装ファイル・最新の実測・稼働設定を上書きしない。配布に同梱するMAD2_changes.patchは元ZIP基準である。適用前に `git apply --check MAD2_changes.patch` で確認し、競合する文書は新しい実装記録を残して統合する。チェック失敗を強制上書きで回避しない。
