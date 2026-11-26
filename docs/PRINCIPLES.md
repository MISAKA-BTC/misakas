# MISAKA 設計原則

2026-11-26のユーザー指示により、RFCを20本の実装設計、ADRを対応する20本の判断理由へ統合し、モデル追加をBFT承認だけに統一する。本書は設計上の優先規則であり、公開activationや実装の完成を宣言しない。

## 1. 設計と実行

指定された不変モデル・入力・数値規則で有用推論を実行する。実行同一性、計算正当性、衝突特性、実計算費用、有用性を別の性質として記述する。正しい高速化を算術不正と扱わない。[RFC-0002](rfc/0002-tensor-ir-kernels-and-runtime.md)、[RFC-0004](rfc/0004-fow1-execution-and-ticket.md)、[RFC-0005](rfc/0005-qf1-qwen3-execution-profile.md)。

## 2. 計算の検証

LLM・モデル計算の暗号学的validity proofを採用しない。ZK、SNARK/STARK、zkVM/zkML、PCS系succinct argument、GKRによる計算validity、再帰証明・proof aggregationを用いない。

Panelによる再実行、必要な範囲の確率的検査、公開資料、局所反証、Nodeの決定論的Court、用途別bond・個別slashを用いる。ハッシュ・署名・Merkle opening・乱数の認証は利用できるが、全計算や費用の証明と称さない。

## 3. モデル追加はBFTのみ

新規モデルの正式追加は、共通技術互換と有効なBFT YES定足数に基づく。kernel自動解析のPASS、timeout、別経路のfallbackでは追加しない。BFT採否に数学的証明・品質証明・費用測定の提出を必須としない。未評価はUNKNOWNとして保持する。

未知kernel、未定義codec、資料identityの不一致、有界checkerの欠落をYESで免除しない。モデル追加時に市場も原子的に作る。BFTモデル承認は個別claimの検証を代替しない。[RFC-0007](rfc/0007-bft-model-admission.md)。

## 4. 全モデル共通の管理・経済

MOE3では全モデルが係数1.0、一意なPOSITION市場、同じBFT管理を持つ。genesisの差はcreator不在であり、停止免除や市場免除を設けない。稼働後にgenesis originを付ける追加経路、特別なfork経済分類を設けない。

coinbase CはMiner用POSITION購入50%、LP用POSITION購入20%、MISAKA永久LP20%、Panel/Gate C合計MISAKA5%、BFT MISAKA5%。整数残差は規定式に従う。通常売買はburn5%、正規creatorがあれば1%、いなければ0%。[RFC-0016](rfc/0016-position-market-and-rewards.md)。

## 5. 停止・廃止・個人責任

全モデルに可逆burn-pauseを適用する。正規解除には登録者の認証又は専用BFT承認と、所定の解除burnを要する。creator不在のgenesisの解除は専用BFT経路を用いる。

BFTはモデル採用・専用停止・専用解除・規定恒久廃止を扱う。台帳finality・fork choice・reorg vetoの権限を持たない。

恒久廃止は対象のactual reserve、登録責任lot、MINING lot、未確定mining報酬を規定scopeで100%burn/失効する。全Panel/Gate C検証lotと正当な検証報酬を一括没収しない。個人不正はCourtで別に扱い、実回収MISAKAの49%を有資格の発見・立証者合計へ、残りをburnする。二重配分・自己slash報酬を認めない。[RFC-0012](rfc/0012-bonds-budgets-and-slashing.md)、[RFC-0017](rfc/0017-model-stop-and-retirement.md)。

## 6. claimから台帳へ

claim Final、独立Gate C認証、正規Ticket当選、受賞block受理、報酬決済を区別する。元claimの全関係、元期限、資料取得・包含、正直参加者、選出前固定を明示する。BFT YESや未告発を成功Final・SecurityCertifiedの代替にしない。[RFC-0008](rfc/0008-g14-panel-and-verification.md)、[RFC-0009](rfc/0009-court-and-claim-final.md)、[RFC-0011](rfc/0011-gate-c-and-consensus-credit.md)、[RFC-0015](rfc/0015-dag-blocks-and-settlement.md)。

## 7. 仕事・資源・保存

一つの仕事からのTicket・用途別credit・支払は一回だけ。認証前の成功効果、別aliasの二重仕事、未回収slashからの報酬を認めない。共通搬送容量、保護Court容量、bond予算、RAM/disk/資料保持を有限にする。

pruningでは未解決責任・未消費権利・資料義務・reorg範囲・署名/曝露journalを保持する。固定資料へのID付加や全trace書込みをLLM費用下界と同一視しない。[RFC-0006](rfc/0006-tb1-randomness-and-release.md)、[RFC-0010](rfc/0010-materials-and-funded-delivery.md)、[RFC-0014](rfc/0014-exec-schedule-and-capacity.md)、[RFC-0018](rfc/0018-storage-pruning-and-ibd.md)。

## 8. 提案・延期・不採用

MWA1の共通meter・仕事比例原資・EXEC量と600秒frontier目標は提案であり、文書整理では採択へ昇格させない。新認証state root形式も提案のまま。画像・動画採掘、モデル改善はmainnet後へ延期・当面無効。PALW汎用VMとkernelのみの自動モデル採用は使用しない。

## 9. 文書と移行

新規設計の規範入口は本書と[20本のRFC](rfc/README.md)。RFCは数式・形式・状態遷移・制約・受入条件の正本、ADRは背景・選択・理由・代替案・帰結の正本とする。spec/designはcodec・技術補助、evidence/auditは記録とする。旧文書の記述が本書と衝突する範囲は新規profileに適用しない。

旧71RFC・235ADR本文の履歴・複製をこのdocsへ残さない。[RFC対応表](rfc/MAPPING.md)と[ADR対応表](adr/MAPPING.md)は参照解決だけに用いる。既存wire tag・署名domain・DB版・claim IDの変更はこの再編に含まれない。

既受理claimの署名・元期限・責任は受付時規則を維持する。設計変更は実装・検証・公開有効化と別に管理し、具体的なactivationは対象ネットワークで明示する。[RFC-0020](rfc/0020-devnet-release-and-migration.md)。
