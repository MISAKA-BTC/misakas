# 2026-11-20 — モデル登録時の試行別認証出力

## 今回追加した設計

ユーザーの「モデル登録時に、別の採掘試行の出力が天文学的な確率でしか重複しない構成を要求する」という指示に基づき、[RFC-0060](../../../rfc/0060-model-registration-and-attempt-output-binding.md)と[ADR-0228](../../../adr/0228-model-registration-and-attempt-output-binding.md)を追加した。提案系列はPALW-MOR1、状態は**DRAFT_NOT_ACTIVATED**である。

登録対象を、モデル資料・実入力adapter・規範的実行・完全trace・出力束縛規則へ接続する。正式出力は`(y, s, rho)`とし、自然文yの一致は許す。rhoは完全な登録文脈、実beacon、試行ID、実入力／回答／traceの三Rootへ結び付けた256bitの識別値である。

登録する構成そのものから、古典ランダムオラクルモデルの下で衝突確率`min(1, Q(Q-1)/2^257)`を導く。Qは非公開・破棄・不正Rootを含む相異なる入力への総照会上限であり、採択claim数ではない。同じ試行をMinerとPanelが再計算すれば、同じ正しい認証出力を得る。

## AN1との接続

- AN1の完全C、実token入力、二段階freeze、Ticket式、Panel完全再実行、モデル非保有Nodeの局所Court、一回会計を保持する。
- Ryは回答yのみ、Rtauはreceipt生成前のコア実行に対するRootとし、rho後処理を自身のRootへ戻す循環を作らない。
- MOR1構造要件は対象profile共通の技術的適合条件とし、permissionless登録、burn停止・解除、不変IDを保持する。
- 非重複性を通常LLM費用の下界、本人の実行、偽Rootの正当性、資料取得、DAG安全性の証明として扱わない。
- 新しい不変な登録／profileの草案であり、既存claim・署名・実行規則へ遡及適用しない。native実装、採択、activationは未実施である。

## 収録物

- [変更範囲](CHANGES.md)
- [入力資料と監査境界](SOURCE_AUDIT.md)
- [検証結果](VALIDATION.md)
- [receipt構成の参照検査](check_mor1_reference.py) — native wireやQwen実行の実装ではない。

現行の閲覧構造20 RFC／50 ADRを維持し、0060/0228を両registryの統合後追加文書として登録した。関連する現行冊には適用範囲付き参照を追加する。RFC-v2では現行著述部末尾の履歴節より前、ADR-v2では既存本文・履歴ブロックの後へ置き、各checkerが保護する原文範囲を維持する。旧文書・原文hash・過去の測定結果を改訂しない。
