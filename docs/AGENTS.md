# docs/ を編集するエージェントへ

この指示は docs 配下に適用する。リポジトリ直下の AGENTS.md / CLAUDE.md を置換するものではない。

採用済み設計は原則凍結。設計変数・パラメーター以外を通常変更しない。重大な問題の場合のみ、新規 RFC と新規 ADR の両方、レビュー、設計責任者の明示的採択によって変更する。[PRINCIPLES §11](PRINCIPLES.md#design-freeze) / [正式手続き](governance/design-freeze.md)。

RFC-0003 の画像・動画経路と RFC-0004 のモデル改善は mainnet 後に延期し、当面無効。初期 mainnet の必須実装・有効化対象に戻さず、共有コード・別入口で迂回して開放しない。mainnet 開始後も自動有効化しない。[適用範囲](governance/post-mainnet-scope.md)。

パラメーターの調整は台帳に旧新値・根拠・試験・承認を残す。数値で認証・責任・不変条件を無効化しない。意味不変の文書修正、既存設計への実装・不具合修正・最適化、試験は継続する。

既存の番号・主文書パスを勝手に変更・再利用しない。Draft、採択、実装、検証、有効化を分ける。元資料のログ、digest、実測値、過去のコマンド出力を書き換えない。

編集後は `python3 docs/maintenance/check_docs.py` を実行する。既知の未解決参照を隠すために baseline を一括更新しない。リンク検査は仕様の意味や実際の無効化を保証しない。

現行の閲覧・通常編集先は `rfc-v2/`（20本、0001〜0020）と `adr-v2/`（統合50本）。旧 `rfc/`・`adr/` の本文・番号・パスは互換参照と履歴として保持し、統合後の新規採択記録は別に識別する。旧番号・wire ID・fence名を新番号へ一括置換しない。統合版も PRINCIPLES を上位とし、未採択・延期・不採用を復活させない。対応表とregistryを参照し、変更時は両統合版の `check_integrity.py` も実行する。

2026-11-08 に [RFC-v2-0016〜0020](rfc-v2/README.md) の設計を明示採択した。[ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) は post-consolidation の採択記録であり、ADR-v2 の51冊目ではない。新しい claim / profile の規範入口は [RFC-v2-0020](rfc-v2/0020-verification-gated-final-and-adversarial-models.md) と [公開裁定の方針 v0.2](design/palw/consensus/public-adjudication-policy-v0.2.md)。状態は `ADOPTED_DESIGN_NOT_ACTIVATED` であり、設計採択を実装・測定・activation 完了へ変更しない。旧署名済み claim の意味、採択対象外の設計、過去の統合 snapshot を維持する。

2026-11-08の追加採択は[新RFC-0048](rfc/0048-exec-credit-schedule-and-one-bps.md)と
[ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)に記録する。
通常編集先は20冊を維持し、07はcredit、10はScheduleId・slot・1 BPS、05/18/19/20はcourt容量と成功境界を所有する。
今回の入力基準の保全検査は maintenance/revisions/2026-11-08-exec-1bps/check_revision.py を使う。
以前の2026-11-08改訂フォルダはその時点の検査・採択記録としてbytesを保持する。
新profileの1 BPSを実測・決済保証・DAA換算へ昇格させない。
