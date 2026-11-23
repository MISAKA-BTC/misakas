# MISAKA — agent instructions

This file is read automatically by coding agents (Claude Code reads `CLAUDE.md`, Codex reads `AGENTS.md`). The design premise above all ADRs/RFCs is [docs/PRINCIPLES.md](docs/PRINCIPLES.md).

## 暗号学的な計算証明を使わない — 2026-10-15 ユーザー決定(恒久)

**MISAKA は、LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用しない。今後も使用しない。**
ZK などで LLM の計算を証明することは不可能である(ユーザー判断)。

- **禁止:** zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント(PCS)等に基づく succinct argument / validity proof、再帰証明・proof aggregation、その他「計算の正しさを暗号学的に証明する」方式。設計案・RFC・ADR・実装・将来計画・研究項目のいずれでも採用候補に挙げない。
- **MISAKA の検証方式は不可侵原則のとおり:** 確率的に検出する(サンプリング再実行、決定論的な再計算、Freivalds などの代数的スポットチェック)、公開証拠で局所化する、決定論的に裁く(court・再実行による反証)、経済的に抑止する(bond・slash)。合意 credit の認証(`SecurityCertified`)は、抽選された検証者の承認を「正直な割合」の明示的な前提の下で用いる。
- **使ってよい暗号:** ハッシュ、デジタル署名(ML-DSA 等)、Merkle などの commitment と opening、VRF・beacon などの乱数。これらはデータの同一性・作成者の認証・乱数のためであり、計算の正しさの証明ではない。
- **既存文書:** ZK・succinct proof・compact proof を「将来」「研究」として挙げている箇所(例: `docs/design/palw/consensus/rfc9-l2-fork-choice.md` 案 C、RFC-0015 §10、`docs/evidence/palw/implementation-records/rfc-0009-remote-record.md`)は本決定により除外する。過去の記述は履歴として残す。
- **エージェント(Claude Code、Codex など)へ:** 設計・実装・レビュー・提案で上記の暗号学的計算証明を持ち出さない。既存の記述を見つけたら、除外である旨の注記を付ける。

## 凍結前提: モデルの二階層、モデル単位の検証は不採用 — 2026-10-15 ユーザー決定

- genesis と年次フォーク(年 1 回程度のソフト/ハードフォーク)で指定されたモデルは **1.0**。
- 後から登録したモデルは、**登録した時点から審査なしで 0.3 に固定**。昇格・更新・失効・試行手数料はない。**1.0 になる経路は年次フォークでの指定だけ**。
- **モデル単位の検証・審査は不採用。** モデルに対して検証を行うと、その検証において何を信頼するかの問題が発生し、解決しないため。モデルの公開状況・保持者数・取得可能性をチェーンで審査する仕組みを提案・実装しない。
- 係数は配分重みに一度だけ(`A_m = c_m · S̄_m^1.5`)。登録モデル全体の発行上限で多数登録の迂回を防ぐ。
- **claim 単位の検証(court、Gate C / `SecurityCertified`、claim ごとの抽選検証者認証)は全モデルで維持する。** 未認証 claim はどの階層でも fork choice・決済を進めない。
- 詳細は `docs/PRINCIPLES.md` §9。これは凍結した前提で、エージェントは再審・代替案の提案をしない。

## 2026-11-11 ユーザーの後続限定改定

[PRINCIPLES §9](docs/PRINCIPLES.md)と[RFC-0053](docs/rfc/0053-additional-model-burn-pause.md)／[ADR-0221](docs/adr/0221-additional-model-burn-pause.md)を適用する。追加登録モデルだけに、誰でもburnで停止、登録者だけがburnで解除する制度を採択。genesis・年次fork指定モデルは対象外。取得可否証明・第三者検査・多数決・定期可用性認定は廃止し、旧一般規則はこの可逆な稼働停止に限り明示改定する。モデル品質審査、class取消、tier変更は行わない。claimの反証・責任・原期限を維持し、設計採択を公開有効化としない。

## 2026-11-21 ユーザー改定 — 新規採掘の MAS1

新規採掘に限り [RFC-0065](docs/rfc/0065-model-admission-screening-and-bft-stop.md) を優先する。登録・既存係数・市場、旧 claim の責任と原期限を維持する。新規採掘の検査・停止は MAS1、Qwen3 の実行形式は [RFC-0066](docs/rfc/0066-qwen3-fow1-execution-and-readout.md) に従う。公開有効化は別途決定する。

## 2026-11-23 ユーザー明示改定 — MAD2二経路モデル採用

[RFC0067](docs/rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](docs/adr/0235-model-admission-kernel-auto-and-bft-decision.md)をモデル採用の後続正本とする。ユーザーはkernel自動追加とBFT YES/NOによる追加の二経路を指示した。BFT経路では数学的定理・費用/品質の証拠提出を必要とせず、共通技術互換と正規quorumで採用を判断する。旧「BFTは停止専用」「UNKNOWNは全経路で採掘不可」はAUTO側と旧profileの規則に限定する。未実装kernel、個別claimの検証、独立Gate C、停止、一回会計を免除しない。計算validity proofは追加しない。この承認済みの文書改定について再確認を求めず、実投票・runtime・公開activationの完了には読み替えない。

同日の追加指示により、元genesisだけをBFT管理（採用・変更・停止・解除）の対象外とする。追加モデルは採用経路を問わずBFT管理対象。BFTはforkモデルも追加でき、Optionなし・Miner=Rを継承するが停止対象とする。後付けGENESIS指定、rule停止、aliasで権限境界を迂回しない。個別claimの正当性検証と永久LP等の既存資産保護は維持する。
