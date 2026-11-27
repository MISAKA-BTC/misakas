# RFC-0004 — FOW1実行・Readout・Ticket・仕事の一回性

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0004](../adr/0004-execution-bound-ticket-and-work-identity.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 実行と採掘値

固定model/profileについて、`CoreExec(M,P,x)=(y,z,tau)`、`z=Readout_P(規定された推論状態)`とする。yは有用出力、zは同じ推論から取り出す採掘値、tauは規範実行記録である。補助receiptや試行IDをzへ置き換えない。

Readoutの対象層・position・演算後の値・順序・長さ・codec・停止関係を不変descriptorに含める。後から作るRoot/Ticket/receiptを先行実行へ戻す循環を作らない。異なる実装でも同じ試行の正しいy,z,tauは同じになる。

## 2. descriptorとモデル採用

artifact/root、program、tokenizer、入力adapter、数値profile、kernel、Init/Step/Readout/End、全checker、資源上界、TB1の参照規則、Ticket codec、仕事キー、停止・移行規則を一つの不変descriptorへ結合する。

新しいモデル採用はBFTだけで行う。技術互換と正規YESを必要とし、全consumerが同じadmission generation・停止状態を読む。自動解析合格、同名モデル、市場の存在、receiptだけから採用資格を作らない。G-EXECの全局所関係を検証できる条件はBFTでも免除しない。

## 3. 実行からPanelまで

1. owner/beneficiary、全nonce候補、model/job/profile、target、原期限、予算、EXEC/PANEL RoundRefを固定する。必要roundの準備・容量を予約する。
2. EXECの対象集合をnativeにfreezeし、安定条件を満たしてから最初の正直shareを公開する。
3. 正規EXEC certificateのsigma_Eから、`s=H_FOW1-attempt(Enc(C,sigma_E))`を導く。sは32byte。実入力frameへ含める。
4. 規範推論、Readout、四Root、唯一のTicketを生成し、元claimを署名・固定する。
5. 元claim集合、pool、重み、抽選policy、義務を、独立したPANEL roundの公開前にfreezeする。
6. Panelは元claimを完全再実行し、一致時は承認、不一致時は直接反証、資料不足時は資料caseへ進む。

PANELはEXECの公開値を再hashした未来乱数ではない。未来乱数の境界はcertificate完成ではなく最初の正直share公開である。公開済みseedを別候補へ付け替えない。

## 4. canonical四Rootと唯一のTicket

R_x、R_y、R_z、R_tauは別domainのcanonical commitmentであり、R_zの全bytesとtraceのReadout/Endを結合する。木形、leaf順、位置、長さ、空値、padding、整数順序を固定する。

```text
T = H_FOW1-ticket(Enc_P(s, R_x, R_y, R_z, R_tau))
Win = (T < theta)
0 <= theta <= 2^256
```

Tは32byteを規定順で読む256bit unsigned integerである。thetaの上端は257bit相当を必要とする。target snapshotは試行前に固定し、当選を見て変更しない。旧Dを写す場合の式は`floor((2^256-1)/D)+1`であり、profileが明示する場合だけ使用する。

署名乱数、carrier、Panel seed、Court nonce、説明文、任意salt、別Merkle形、receiptをTicketの自由fieldへ含めない。仕事量の複数unitを複数Ticketへ変換しない。Final後の再抽選を追加しない。

## 5. 一回性と原子的な会計

同じ実行の再送・再署名・Panel再実行は追加Ticketを作らない。ComputationalWorkKeyはmodel名やaliasだけの変更で新仕事にならず、prefix originと正規位置を含めて照合する。

claim受付時の仕事キー所有、成功後の用途別credit発行、受賞時の権利消費は別段階のrecordにする。各段階では対応する状態・予算・依存・出力を同じ原子遷移で変更する。受付時に全受賞権が消費されたとみなしたり、受賞時に仕事キーを再所有したりしない。

再送、import、restart、reorg、旧新familyのalias、別資金txから同じ仕事の報酬を二重発行しない。旧claimの署名意味を変更しない。

## 6. 正当性・衝突・費用の区別

G-EXECは全規範関係と有界局所反証の接続である。Panelの完全再実行、能動的な正直者、資料、期限内包含、Court処理、native gateに依存する。局所一致又はMerkle opening一つで全体を承認しない。

G-COLLは異なる試行のコアzの衝突特性、G-COSTは正しいTicket入力を安く予測・生成・共有できる経路の評価、G-USEは実用性の評価である。BFTの採否にこれらの証明提出を必須としないが、評価結果・UNKNOWN・既知の反例を消さない。BFT YESを費用下界の定理として表示しない。

費用比較を実施する場合は、descriptor、入力予定、基準実装、機材、費用単位、回数、予算、閾値を先に固定する。前処理、cache、batch、共通prefix、別Jobへの償却、破棄候補を含め、失敗経路を平均で打ち消さない。PASSはその実験範囲の結果であり、全アルゴリズムへの下界ではない。

正しい高速化は算術詐欺ではない。安価な正しい計算の発見は、BFTの採用見直し・停止理由になり得る。高額な証明生成やslashの存在から通常LLM費用を強制したと結論しない。

## 7. 条件付きの確率

決定的実行とcanonical encodingから、一試行の正しいTicket入力は一つになる。古典Ticketランダムオラクル、事前固定target、返値を見る前に正しい入力であることが定まる相異なる初回照会が高々k件という条件なら、当選確率は`1-(1-theta/2^256)^k`で評価できる。

この式は、任意の予算でk件を生成する費用を証明しない。Qwenへの入力challenge追加、zの長さ、実験で衝突しなかったこと、TB1の予測不能性だけからその下界を導かない。

## 8. 成功効果と移行

モデル採用、元のTB1文脈、選出Panelの完全再実行、公開異議期間、資料義務とCourtを経た成功Final、唯一Ticketの当選、成熟、停止・依存・予算、権利の一回消費を各効果の入口で確認する。

切替後の新規入口はFOW1を使い、旧TFO3/AN1/MOR1/旧quantum抽選へfallbackしない。既受理旧claimは元規則で有限にdrainする。旧reader、wire、enumを保つことと、新規旧方式の採掘を許すことを分ける。

## 9. 受入条件

実入力の差替え、偽R_z、Readout位置違い、偽weight index、Stop不一致、Rootの別codec、再署名による抽選増幅、別ownerによる仕事コピー、無認証Final、二重消費を拒否する。正常claimは直接反証の任意対話を実装しなくても完全再実行・承認へ進める。

## 10. 署名済みprefix exportと依存

新規の非genesis prefix再利用を有効化するprofileでは、生成元は受理前にnetwork、work namespace、nonce、自己参照しないbase claim commitment、完全なprefix descriptor、有限の責任・材料期限を署名する。生成元の受理時にconsensusが実claim IDと受理点へ結合し、消費側はその受理済みidentityを署名対象とする。未来の受理blockを先取り署名させない。

exportのcommitmentと生成元の正しい実行を別に検査する。生成元不在や資料Unavailableを、独立入力又は正常なprefixへ変換しない。消費側のhardと材料保持は推移的な全sourceの元署名期限・予約内に収め、新たな消費でsourceの責任を延長しない。

生成元が失効・conviction・未検証終了になった場合は、依存scopeに従い消費側の成功効果を扱う。新しい署名exportは旧claimへ後付けせず、新envelopeと元fenceを記録する。正規系統のreorg、pruning、IBDでexport・消費・原責任の対応を復元する。この依存設計は採択済みであり、有効化は別工程である。
