## 2026-11-27 ユーザー決定（新規profileの合意credit）

RFC-0011・ADR-0011を適用する。新規claimはG14選出1席の完全再実行、選出外にも開かれた資料要求・反証、元期限とCourtの解決を必要とする。独立Gate Cは旧claimの受付時契約に限る。Panel役務は5%、BFTは5%。個別Courtの実回収額の49%枠は選出外を含む適格な立証者へ、残差はburn。下記の旧Gate C必須記述は新規profileではこの決定が置き換える。

# MISAKA — agent instructions

This file is read automatically by coding agents (Claude Code reads `CLAUDE.md`, Codex reads `AGENTS.md`). The design premise above all ADRs/RFCs is [docs/PRINCIPLES.md](docs/PRINCIPLES.md).

## 暗号学的な計算証明を使わない — 2026-10-15 ユーザー決定(恒久)

**MISAKA は、LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用しない。今後も使用しない。**
ZK などで LLM の計算を証明することは不可能である(ユーザー判断)。

- **禁止:** zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント(PCS)等に基づく succinct argument / validity proof、再帰証明・proof aggregation、その他「計算の正しさを暗号学的に証明する」方式。設計案・RFC・ADR・実装・将来計画・研究項目のいずれでも採用候補に挙げない。
- **MISAKA の検証方式は不可侵原則のとおり:** 確率的に検出する(サンプリング再実行、決定論的な再計算、Freivalds などの代数的スポットチェック)、公開証拠で局所化する、決定論的に裁く(court・再実行による反証)、経済的に抑止する(bond・slash)。合意 credit の認証(`SecurityCertified`)は、抽選された検証者の承認を「正直な割合」の明示的な前提の下で用いる。
- **使ってよい暗号:** ハッシュ、デジタル署名(ML-DSA 等)、Merkle などの commitment と opening、VRF・beacon などの乱数。これらはデータの同一性・作成者の認証・乱数のためであり、計算の正しさの証明ではない。
- **エージェント(Claude Code、Codex など)へ:** 設計・実装・レビュー・提案で上記の暗号学的計算証明を持ち出さない。既存の記述を見つけたら、除外である旨の注記を付ける。

## 2026-11-26 採択済み設計

- 全モデルの係数は **1.0**。genesis を含む全モデルを BFT モデル管理の対象とし、新規追加・採用の経路は `BFT_DECISION` に統一する。`KERNEL_AUTO` の自動採用を廃止し、kernel 演算、変換、`BaseCompatible` 技術互換検査を維持する。測定資料は対象へ紐づけて追跡するが、測定証拠の添付・数学的定理・一律合格閾値は票の有効条件にしない（`UNKNOWN` を `PASS` へ改変しない）。新規の `FORK_DESIGNATED` 特別区分および元 genesis の管理免除は設けない。[RFC-0007](docs/rfc/0007-bft-model-admission.md)
- MOE3 の配分は Miner 用 POSITION 購入 50%、LP 用 POSITION 購入 20%、MISAKA 永久 LP 20%、Panel/Gate C 5%、BFT 5%（小数 8 桁 POSITION）。旧 MOE2（30/20/50）や二階層（1.0/0.3）を新規経路へ混入しない。[RFC-0016](docs/rfc/0016-position-market-and-rewards.md)
- 全モデル共通の burn-pause と正規解除を維持する。BMR1 の恒久廃止は用途別 bond（`MINING` と `PANEL/GATE_C` の分離）を区別し、規定の対象資産を 100% burn、Panel/Gate C 検証 bond は一律保護する。個別 Court slash の実回収額は適格者報酬 49%、burn 51% を基本とし、端数と未受給分は RFC の規則に従う。[RFC-0012](docs/rfc/0012-bonds-budgets-and-slashing.md)・[RFC-0017](docs/rfc/0017-model-stop-and-retirement.md)
- MWA1（[RFC-0013](docs/rfc/0013-model-work-accounting.md)）は `PROPOSED_DESIGN_NOT_ACTIVATED`（提案段階）であり、参照検証に留め、未採択の価格・量子・target を公開条件や合意実装に組み込まない。
- G14、Court、独立 Gate C、FOW1、TB1、一回会計を維持する。既受理 claim は受付時の署名・原期限・責任に従う。既存 wire ID・合意規則・公開 fence の変更は個別実装と有効化手続きに従う。[PRINCIPLES](docs/PRINCIPLES.md)

## 文書と開発

- [RFC 20本](docs/rfc/README.md)を実装設計の正本、[ADR 20本](docs/adr/README.md)を判断理由の正本とする。旧仕様への参照は対応表で解決する。採択、実装、検証、公開有効化は個別に記録する。
- 実装は通常経路を受賞から決済まで接続し、短時間試験で確認してから最終総合試験を行う。S3 の DAA 8602 台帳・受賞・決済途中状態と復元世代を保持し、論理量と実ディスク消費を区別する。低容量時は実走を止める。[RFC-0018](docs/rfc/0018-storage-pruning-and-ibd.md)・[RFC-0020](docs/rfc/0020-devnet-release-and-migration.md)
- 公開有効化、QF1 の REVIEW_HOLD 解除、VPS 配備はそれぞれ別途承認を要する。docs の検査は [文書の検査と差替え](docs/maintenance/README.md)に従う。
