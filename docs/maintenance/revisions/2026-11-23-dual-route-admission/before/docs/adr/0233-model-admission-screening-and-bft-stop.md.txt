# ADR-0233 — モデル追加時の構造・実用性検査と限定BFT停止を採択する

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../governance/qwen-fow1-profile-transition.md)。Qwenの具体例QF1でも、G-USE・構造解析・反復費用検査・現行停止状態の全条件を維持する。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 対応RFC: [RFC-0065 MAS1](../rfc/0065-model-admission-screening-and-bft-stop.md)
- 適用・移行: [model-admission-safety-transition](../governance/model-admission-safety-transition.md)
- 根拠: 今回のユーザーによる「安く推測できるモデルを追加時に弾き、自動で弾けない場合はBFTで停止する経路へつなぐ」という明示指示。

## 背景

FOW1はG-EXEC・G-COLL・G-COSTを要求するが、新規採掘のG-COSTは、対象となる重みと規則を固定して複数の入力で費用を比較する。計算graphの形、zの長さ、モデルの評判又は有限testだけで採掘資格を付与すると、定数化、cache、軽いReadout、採掘専用の無用な重い枝、未知の短絡を使うモデルが費用認定をすり抜けるおそれがある。

ユーザーは、実用性を保ったモデルを対象に、登録段階のkernel/graph解析と、不明時のBFT停止を追加するよう指示した。旧「モデル単位の検査をしない」「品質審査をしない」「BFTを使わない」という包括的な記述は、今回採択する限定用途について修正する必要がある。

## 判断

1. **一般登録と採掘資格を分ける。** artifact登録はpermissionlessとし、追加登録時のMOE2市場原子生成を維持する。市場・bond・モデル名から採掘資格を発生させない。
2. **MAS1の自動検査を採掘入口へ追加する。** lowering、kernel、入力、Readout、useful state、trace、cache、MoE、既知の短絡を対象に、PASS/FAIL/UNKNOWNを区別する。未対応や未認証の全weight属性をPASSにしない。
3. **実用性条件G-USEを加える。** 公開・共通・版固定のsuiteと客観的scoreを用いる。軽量Readoutと検証負担を分離し、回答に不要な巨大演算の費用をLLMの有用仕事として認定しない。有限testの合格はG-COSTや全入力の品質の証明ではない。
4. **非報酬評価を分離する。** 未適格モデルの検査はNON_REWARDING_MODEL_EVALUATIONで行い、採掘・credit・weight・受賞権利・MOE2買付を生まない。独立したTB1-MODEL_AUDITと、提出後のTB1-PANEL、既存の完全再実行・局所Courtへ接続する。
5. **不明な新規モデルは閉じたままBFT reviewへ送る。** UNKNOWNはREVIEW_HOLD、期限満了は未適格終端とする。BFTが止まっても仮承認しない。既PASSEDモデルへの未認証の通報一件は永久vetoにしない。
6. **限定BFT停止を導入する。** 事前確定したTB1 holder rosterに別domainのMODEL_STOP権限を付与し、n=3f+1、q=2f+1の署名でartifact/profile又はTheoremRule versionを停止する。claim Panelの一人正直という仮定をBFTの故障仮定へ転用しない。
7. **BFTへ台帳決定権を与えない。** certificateは既存native順序で処理する停止controlである。fork choice、reorg拒否、block finality、claimの算術真偽、既確定残高の改変を権限に含めない。DNS-BFTを復活させない。
8. **解除は全適格性の再確認を必要とする。** artifact/profileの解除にはfreshな全PASSと、現在rosterのq署名による対象StopId集合の解除を必要とする。G-COST不明を多数決でPASSへ変えない。burn Resume、owner変更、core指定、旧解除証明は安全停止を消さない。TheoremRule versionの停止は個別モデルの解除で消さず、別途採択した新rule versionと全適合再検査を必要とする。
9. **全成功consumerへ接続する。** 同batchでは安全停止を新しい成功・発行・受賞権利消費より先に反映する。停止中も元期限・Court・資料・返還を維持する。既確定の効果はBFTだけで遡及没収しない。
10. **未知の安価算法を完全に発見したとはしない。** 検出coverage、未確認の費用条件、誤停止、停止までの遅延、BFTとnativeの活性前提を公表する。正しい高速化そのものをslashしない。

## 旧規範の置換範囲

| 対象 | 今回の限定改定 |
| --- | --- |
| RFC0062 §0/§2/§4/§7/§8、ADR0230 | FOW1の三gateを維持してMAS1検査・G-USE・停止状態を追加。数学的費用保証の欠如はBFTで補完しない |
| RFC-v2-0002/0003/0005/0006/0007/0010/0011/0016〜0020 | kernel、登録、評価、control、Final、権利消費・会計・資料・reorgの担当規範へ接続 |
| ADR0180、PRINCIPLESのBFT・モデル無審査条項 | 台帳BFTは禁止を維持。モデル安全停止と共通実用性条件だけを例外として採択 |
| RFC0053、ADR0221 | burn停止の独立性を維持。burnだけでMAS1停止を解除しない |
| RFC0061/ADR0229、TB1用途表 | MODEL_AUDITを独立purposeとして追加。MODEL_STOPは署名権限であり乱数sourceではない |
| MOE2、追加モデル市場・core配分 | 率・原子市場生成・永久LP・通常市場取引を変更しない。採掘停止を市場未準備にしない |

追加モデルの経済的burn停止からcoreを除外する旧規則は、MAS1/FOW1の安全適格性の除外ではない。既存旧profileのcore/追加modelへこの文書を遡及適用しない。

## 検討した代替案

- 全ての未知モデルをBFT承認だけで許す案は不採用。必要な反復費用検査を署名で省くため。
- benchmark合格又は非衝突testだけで採掘可能とする案は不採用。過学習・短絡・共有・安い入力選別を排除できないため。
- 採掘専用の大きなhash/行列演算、Groth16の証明費用を仕事根拠にする案は不採用。今回の「実用的なLLM推論を使う」という目的を満たさないため。
- 単独の未検証通報で既PASSEDモデルを停止する案は不採用。許可された通報の大量投入が無期限vetoになるため。
- BFTが過去の正しい計算や保有資産を没収する案は不採用。危険管理と計算不正を区別できなくなるため。

## レビューと残る条件

本変更で示す抽象性質は、UNKNOWN/FAILから成功しない状態不変条件、q署名に必要な正直署名数、理由別停止とstale解除拒否である。一般のモデル解析の完全性やQwenのG-COSTを証明したものではない。

今回の文書レビューでは、モデル非保有Nodeに不可能な全weightの推定を要求しないこと、BFTによる仮承認を置かないこと、低rank等を自動的な詐欺扱いにしないこと、非報酬評価の循環を避けること、停止対象とnative順序、burn/owner/alias/reorgによる迂回を確認する。独立外部レビュー・実装・実機試験を実施済みとはしない。

停止集合に少なくともf+1人の正直者が含まれることは、危険を必ず正しく発見するという保証ではない。既PASSEDモデルの未知攻撃には停止までの窓があり、false positiveは可用性を損なう。権限を与える実roster、fault budget、control包含、suite・threshold、bounded checker、FOW1定理、旧pending drainがそろうまで公開有効化しない。

## 実装・有効化

新しい型は論理設計である。公開設定・配備・支払は変更しない。新しいruntime flag、wire ID、BFT署名者、品質閾値を今回の文書で本番値として割り当てない。一般登録と市場生成、採掘資格、非報酬評価、risk control、消費・Finalの全入口をnative実装で接続し、旧履歴を保持して試験した後に明示activationを行う。

保存原本・変更一覧・文書検査結果は[改訂記録](../maintenance/revisions/2026-11-20-model-admission-safety/README.md)に保存する。以前のテスト成功や今回のリンク検査を本番の安全性・実装完成に読み替えない。
