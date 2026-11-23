# docs/ を編集するエージェントへ

この指示は docs 配下に適用する。リポジトリ直下の AGENTS.md / CLAUDE.md を置換するものではない。

採用済み設計は原則凍結。設計変数・パラメーター以外を通常変更しない。重大な問題の場合のみ、新規 RFC と新規 ADR の両方、レビュー、設計責任者の明示的採択によって変更する。[PRINCIPLES §11](PRINCIPLES.md#design-freeze) / [正式手続き](governance/design-freeze.md)。

RFC-0003 の画像・動画経路と RFC-0004 のモデル改善は mainnet 後に延期し、当面無効。初期 mainnet の必須実装・有効化対象に戻さず、共有コード・別入口で迂回して開放しない。mainnet 開始後も自動有効化しない。[適用範囲](governance/post-mainnet-scope.md)。

パラメーターの調整は台帳に旧新値・根拠・試験・承認を残す。数値で認証・責任・不変条件を無効化しない。意味不変の文書修正、既存設計への実装・不具合修正・最適化、試験は継続する。

既存の番号・主文書パスを勝手に変更・再利用しない。Draft、採択、実装、検証、有効化を分ける。元資料のログ、digest、実測値、過去のコマンド出力を書き換えない。

編集後は `python3 docs/maintenance/check_docs.py` を実行する。既知の未解決参照を隠すために baseline を一括更新しない。リンク検査は仕様の意味や実際の無効化を保証しない。

ADRとRFCの閲覧・通常編集先は [ADR](adr/README.md) と [RFC](rfc/README.md)。既存の番号、主文書パス、wire ID、fence名を維持する。統合版の記録は整理直前のGitコミットを参照する。[保守手順](maintenance/README.md)。

2026-11-08 のclaim設計は [ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) と [公開裁定の方針 v0.2](design/palw/consensus/public-adjudication-policy-v0.2.md) を参照する。状態は `ADOPTED_DESIGN_NOT_ACTIVATED`。旧署名済みclaimの意味と採択対象外の設計はその版の記録に従う。

2026-11-08の追加採択は[新RFC-0048](rfc/0048-exec-credit-schedule-and-one-bps.md)と
[ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)に記録する。
EXEC credit、ScheduleId、slot、1 BPS、court容量と成功境界は上記の原文を参照する。
当時の入力基準の保全検査は、当該改訂時点のツリーで実行する。
以前の2026-11-08改訂フォルダはその時点の検査・採択記録としてbytesを保持する。
新profileの1 BPSを実測・決済保証・DAA換算へ昇格させない。

2026-11-11のユーザー明示指示により、[RFC-0052](rfc/0052-hugging-face-model-acquisition-and-claim-materials.md)／[ADR-0220](adr/0220-hugging-face-model-acquisition-and-claim-materials.md)のHF必須profileを採択した。モデル静的資料はHF固定版の匿名取得・全file/native root照合、登録時独立検査・定期検査・有効期限を持つ別の公開契約とする。独自モデル重み全量配信は必須にせず、claim固有資料供給・取得case・成功停止を維持する。モデルclassの無審査登録と1.0/0.3は変更しない。設計採択と公開有効化を区別する。

後続のユーザー指示による[RFC-0053](rfc/0053-additional-model-burn-pause.md)／[ADR-0221](adr/0221-additional-model-burn-pause.md)が優先する。先行HF公開契約の取得検査・定期認定は廃止し、追加モデルだけをburnで停止・登録者burnで解除する。genesis・年次fork指定は対象外。旧取得証明制度の実装を継続しない。

## 2026-11-20 TB1/FOW1の明示改定

ユーザーの今回の修正指示に基づき、[RFC0061/ADR0229](rfc/0061-palw-internal-threshold-beacon.md)と[RFC0062/ADR0230](rfc/0062-palw-function-output-and-work-admissibility.md)をADOPTED_DESIGN_NOT_ACTIVATEDとして追加した。[共通移行契約](governance/palw-tb1-fow1-transition.md)が新profileの乱数、コア採掘値、登録適格性、Ticket、旧受付drainを所有する。旧beaconへfallbackせず、receiptの非衝突性をLLM関数の費用証明にしない。古い凍結条項を今回承認済み変更の再確認要求へ読み替えない。過去のsource hash・履歴・測定と全旧path適用台帳を保持する。本番activation・未完成のQwen定理は承認済みとしない。

## 2026-11-20 MOE1経済の明示改定

ユーザーはRFC-v2-0011および該当ADRの全面修正、追加モデル限定MisakaOption、coreのMiner100%を明示承認した。追加訂正により、追加モデル登録時に市場を同一原子遷移で生成し、正常な市場未準備状態やseed待ちを設けない。[RFC0063](rfc/0063-additional-model-option-economics.md)／[ADR0231](adr/0231-additional-model-option-economics.md)が今回の正本である。元ADRの旧版は今回改訂記録のbefore/*.md.txtへbytesを保持し、統合歴史payload・過去テストを改変しない。この承認済み改訂のために再確認を求めない。本番native/ABI/activationの完成は宣言しない。

## 2026-11-20 MOE2永久流動性の明示改訂

ユーザーは30% Position購入・20%BILIと購入済みPositionの一部の永久LP化・余剰Position burn・残余50%への変更を指示した。[RFC0064](rfc/0064-reward-buy-and-permanent-liquidity.md)と[ADR0232](adr/0232-reward-buy-and-permanent-liquidity.md)を後続採択とし、原子的市場登録、追加モデル限定、coreの対象R100%、通常売買5%/3%を維持する。既存仮想reserve/whole Positionに対する整数式とdust処理を明示し、一般のV2 AMMの保証を無条件転用しない。永久LP記録済みモデルのcore指定を禁止する。既承認のこの変更について再確認を求めず、旧MOE1/TB1/FOW1ブロック・過去原本・検査結果は保全する。native実装やactivation完了は宣言しない。

## 2026-11-20 MAS1モデル検査とBFT安全停止の明示改定

今回のユーザーは、モデル追加段階のkernel/graph解析と実用性条件、安価な予測・再利用の排除、自動判定不能時のBFT停止への接続を明示指示した。[RFC0065](rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](adr/0233-model-admission-screening-and-bft-stop.md)を後続採択とする。旧「モデル単位検査なし」「品質審査なし」「BFT不使用」はMAS1の限定用途について置換するが、台帳finality・fork choice・reorgへのBFT権限、HF取得審査、計算validity proofは導入しない。UNKNOWNの採掘仮承認を禁止し、G-COSTとQwenの未完成定理をBFT署名で代替しない。MOE2市場原子生成と経済比率、旧責任、元snapshotを保持する。新規実装・公開activationをこの指示から完了扱いせず、既承認の文書変更のための再確認は求めない。

## 2026-11-20 QF1のQwen実行具体化

ユーザーの明示指示で[RFC0066](rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](adr/0234-qwen3-fow1-execution-and-readout.md)を設計採択した。36層Qwen3-8Bの整数A16・HEX64・37-vector Readout・有界hash chunkを具体化する。数値状態の非衝突性、安価な予測不能性、Qwen実機品質又はnative実装を証明済みにしない。MAS1のUNKNOWN停止と全gate、TB1、MOE2、旧claim、元の歴史payloadを維持する。この承認済み文書変更のために再確認を求めない。

## 2026-11-23 ユーザー明示改定 — MAD2二経路モデル採用

[RFC0067](rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](adr/0235-model-admission-kernel-auto-and-bft-decision.md)をモデル採用の後続正本とする。ユーザーはkernel自動追加とBFT YES/NOによる追加の二経路を指示した。BFT経路では数学的定理・費用/品質の証拠提出を必要とせず、共通技術互換と正規quorumで採用を判断する。旧「BFTは停止専用」「UNKNOWNは全経路で採掘不可」はAUTO側と旧profileの規則に限定する。未実装kernel、個別claimの検証、独立Gate C、停止、一回会計を免除しない。計算validity proofは追加しない。この承認済みの文書改定について再確認を求めず、実投票・runtime・公開activationの完了には読み替えない。

同日の追加指示により、元genesisだけをBFT管理（採用・変更・停止・解除）の対象外とする。追加モデルは採用経路を問わずBFT管理対象。BFTはforkモデルも追加でき、Optionなし・Miner=Rを継承するが停止対象とする。後付けGENESIS指定、rule停止、aliasで権限境界を迂回しない。個別claimの正当性検証と永久LP等の既存資産保護は維持する。
