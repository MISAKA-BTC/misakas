# MISAKA — agent instructions

This file is read automatically by coding agents (Claude Code reads `CLAUDE.md`, Codex reads `AGENTS.md`). The design premise above all ADRs/RFCs is [docs/PRINCIPLES.md](docs/PRINCIPLES.md).

## 暗号学的な計算証明を使わない — 2026-10-15 ユーザー決定(恒久)

**MISAKA は、LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用しない。今後も使用しない。**
ZK などで LLM の計算を証明することは不可能である(ユーザー判断)。

- **禁止:** zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント(PCS)等に基づく succinct argument / validity proof、再帰証明・proof aggregation、その他「計算の正しさを暗号学的に証明する」方式。設計案・RFC・ADR・実装・将来計画・研究項目のいずれでも採用候補に挙げない。
- **MISAKA の検証方式は不可侵原則のとおり:** 確率的に検出する(サンプリング再実行、決定論的な再計算、Freivalds などの代数的スポットチェック)、公開証拠で局所化する、決定論的に裁く(court・再実行による反証)、経済的に抑止する(bond・slash)。合意 credit の認証(`SecurityCertified`)は、抽選された検証者の承認を「正直な割合」の明示的な前提の下で用いる。
- **使ってよい暗号:** ハッシュ、デジタル署名(ML-DSA 等)、Merkle などの commitment と opening、VRF・beacon などの乱数。これらはデータの同一性・作成者の認証・乱数のためであり、計算の正しさの証明ではない。
- **エージェント(Claude Code、Codex など)へ:** 設計・実装・レビュー・提案で上記の暗号学的計算証明を持ち出さない。既存の記述を見つけたら、除外である旨の注記を付ける。

## MISAKA の作業規約(Codex 317033a56 の版、統合時に併記)

### 恒久決定（2026-10-15、ユーザー）

MISAKA は LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用せず、今後も使用しない。
この決定は設計案・RFC・ADR・実装・将来計画・研究項目のすべてに適用する。

- zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント（PCS）等に基づく succinct argument / validity proof、再帰証明・proof aggregation を採用候補に挙げない。
- 検証は確率的検出（サンプリング再実行・決定論的再計算・代数的スポットチェック）、公開証拠での局所化、決定論的裁定、経済的抑止で行う。
- 合意 credit の認証には、抽選された検証者の承認を、正直な割合の明示的な前提の下で用いる。検出率・参加率・取得可能性を仮定なしに保証したと扱わない。 楽観的な Final または一致した local replay は、その承認を代替しない。
- ハッシュ・署名・Merkle commitment・VRF/beacon はデータの同一性・認証・乱数に使用できる。計算の正しさの証明として使用しない。
- GKR や producer の証明 transcript を使う計算 validity argument へ迂回しない。代数的スポットチェックは直接の確率的検査として扱い、その検出範囲・費用・前提を明示する。
- 旧 wire tag や履歴の読み取り互換性を保存する場合も、禁止した方式を新規生成・受理して計算 credit の根拠にしない。既存 ID の意味を新しい方式へ黙って変更しない。

詳細と優先順位は [PRINCIPLES の恒久原則](docs/PRINCIPLES.md#2-計算の検証) に従う。

## MISAKA instructions (English, Codex workspace-rfc02 version, kept at merge)

### Permanent user decision — 2026-10-15

MISAKA does not use, and will never use, cryptographic proofs of the correctness of LLM or model computation. Do not propose or implement zero-knowledge proofs, zk-SNARKs, zk-STARKs, zkVM, zkML, PCS-based succinct arguments or validity proofs, recursive proofs, or proof aggregation for this purpose. This restriction applies to RFCs, ADRs, designs, implementation, future plans, and research items.

Use probabilistic detection through sampled deterministic re-execution and algebraic spot checks, public evidence localization, deterministic adjudication, and economic deterrence. Consensus credit authentication uses approvals from randomly selected validators under an explicitly stated honest-fraction assumption. An optimistic Final or matching local replay does not replace that approval.

Hashes, signatures, Merkle commitments, and VRF/beacons may establish data identity, authentication, and randomness. They do not prove computation correctness. See [the governing principles](docs/PRINCIPLES.md#2-計算の検証).

---

<!-- English version (Codex, 2026-10-15) -->
## MISAKA Working Rules (English)

## Permanent Decision: Model Computation Verification

User decision, 2026-10-15: MISAKA does not use, and will not use,
cryptographic proofs of correctness of LLM or model computation.

- Do not introduce or propose zero-knowledge proofs, zk-SNARKs, zk-STARKs,
  zkVMs, zkML, polynomial-commitment-based succinct arguments or validity
  proofs, recursive proofs, or proof aggregation. This applies to designs,
  RFCs, ADRs, implementations, roadmaps, and research candidates.
- Use probabilistic detection (sampled re-execution, deterministic
  recomputation, and algebraic spot checks), localization using public
  evidence, deterministic adjudication, and economic deterrence.
- Authenticate consensus credit through approval by selected verifiers,
  stating the assumed honest proportion explicitly.
- Hashes, signatures, Merkle commitments, and VRF/beacons may authenticate
  data identity or provide randomness. They do not establish correctness of
  model computation. A local adjudication witness is not a succinct
  cryptographic validity proof.

See [docs/PRINCIPLES.md](docs/PRINCIPLES.md), especially section 8. Do not
reinterpret legacy terminology as permission to violate this decision.

## 2026-11-26 採択済み設計

- 全モデルの係数は **1.0**。genesis を含む全モデルを BFT モデル管理の対象とし、新規追加・採用の経路は BFT 承認に統一する。KERNEL_AUTO の自動採用を廃止し、kernel 演算、変換、技術互換検査を維持する。BFT 票に数学的定理や費用・品質の証拠提出は必須としない。[RFC-0007](docs/rfc/0007-bft-model-admission.md)
- MOE3 の配分は Miner 用 POSITION 購入 50%、LP 用 POSITION 購入 20%、MISAKA 永久 LP 20%、Panel/Gate C 5%、BFT 5%。[RFC-0016](docs/rfc/0016-position-market-and-rewards.md)
- 全モデル共通の burn-pause と正規解除を維持する。BMR1 の恒久廃止は用途別 bond を区別し、規定の対象資産を 100% burn する。個別 Court slash の実回収額は適格者報酬 49%、burn 51% を基本とし、端数と未受給分は RFC の規則に従う。[RFC-0017](docs/rfc/0017-model-stop-and-retirement.md)
- G14、Court、独立 Gate C、FOW1、TB1、一回会計を維持する。既受理 claim は受付時の署名・原期限・責任に従う。既存 wire ID・合意規則・公開 fence の変更は個別実装と有効化手続きに従う。[PRINCIPLES](docs/PRINCIPLES.md)

## 文書と開発

- [RFC 20本](docs/rfc/README.md)を実装設計の正本、[ADR 20本](docs/adr/README.md)を判断理由の正本とする。旧仕様への参照は対応表で解決する。採択、実装、検証、公開有効化は個別に記録する。
- 実装は通常経路を受賞から決済まで接続し、短時間試験で確認してから最終総合試験を行う。S3 の DAA 8602 台帳・受賞・決済途中状態と復元世代を保持し、論理量と実ディスク消費を区別する。低容量時は実走を止める。[RFC-0018](docs/rfc/0018-storage-pruning-and-ibd.md)・[RFC-0020](docs/rfc/0020-devnet-release-and-migration.md)
- 公開有効化、QF1 の REVIEW_HOLD 解除、VPS 配備はそれぞれ別途承認を要する。docs の検査は [文書の検査と差替え](docs/maintenance/README.md)に従う。
