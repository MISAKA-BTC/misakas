# MISAKA — agent instructions

This file is read automatically by coding agents (Claude Code reads `CLAUDE.md`, Codex reads `AGENTS.md`). The design premise above all ADRs/RFCs is [docs/PRINCIPLES.md](docs/PRINCIPLES.md).

## 暗号学的な計算証明を使わない — 2026-10-15 ユーザー決定(恒久)

**MISAKA は、LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用しない。今後も使用しない。**
ZK などで LLM の計算を証明することは不可能である(ユーザー判断)。

- **禁止:** zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント(PCS)等に基づく succinct argument / validity proof、再帰証明・proof aggregation、その他「計算の正しさを暗号学的に証明する」方式。設計案・RFC・ADR・実装・将来計画・研究項目のいずれでも採用候補に挙げない。
- **MISAKA の検証方式は不可侵原則のとおり:** 確率的に検出する(サンプリング再実行、決定論的な再計算、Freivalds などの代数的スポットチェック)、公開証拠で局所化する、決定論的に裁く(court・再実行による反証)、経済的に抑止する(bond・slash)。合意 credit の認証(`SecurityCertified`)は、抽選された検証者の承認を「正直な割合」の明示的な前提の下で用いる。
- **使ってよい暗号:** ハッシュ、デジタル署名(ML-DSA 等)、Merkle などの commitment と opening、VRF・beacon などの乱数。これらはデータの同一性・作成者の認証・乱数のためであり、計算の正しさの証明ではない。
- **既存文書:** ZK・succinct proof・compact proof を「将来」「研究」として挙げている箇所(例: `docs/design/palw/rfc9-l2-fork-choice.md` 案 C、RFC-0015 §10、`docs/design/palw/rfc-0009-remote-record.md`)は本決定により除外する。過去の記述は履歴として残す。
- **エージェント(Claude Code、Codex など)へ:** 設計・実装・レビュー・提案で上記の暗号学的計算証明を持ち出さない。既存の記述を見つけたら、除外である旨の注記を付ける。
