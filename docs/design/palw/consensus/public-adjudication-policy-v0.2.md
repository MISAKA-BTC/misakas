# 公開裁定の方針 v0.2 — モデル非保有Nodeと検証先行Final

採択日: 2026-11-08 / 状態: **設計採択・未有効化**。

採択根拠は [ADR-0215](../../../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)。詳細の所有者は [RFC-v2-0016〜0020](../../../rfc-v2/README.md)。[v0.1](public-adjudication-policy-v0.1.md)の原時計、公開の第三者経路、有界な局所裁定、一つの鎖、収集と保持の分離を維持し、以下を新しいclaim版へ追加する。

## 1. Nodeはモデルを持たない

Nodeの通常処理・court・cold起動・IBD・再起動・reorgは、モデル本体の全重み・checkpoint・モデル全体の推論・GPU・モデル配布先への接続を必要としてはならない。認証された局所証拠の重み断片と、有界な実行定義は扱う。通常NodeへPanel workerの実行を強制しない。

資料が欠けたcourtを、全モデル取得や全推論へのfallbackで解決しない。受理した規則と証拠から判定できない場合は、検査未成立として扱う。[RFC-v2-0019](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md)。

## 2. モデルの事前共有も、モデル内Panelの誠実性も仮定しない

悪意ある主体がモデルを隠し、当該モデルのMiner・通常Panel・providerを寡占し得ることを基本の脅威モデルに含める。Minerはclaimを検証する資料を供給し、モデル保有者集合とは独立のglobal poolから選出された検証者が取得して検証する。

未配布モデルの登録を審査・取消する制度は作らない。登録tierは従来どおりであり、各claimに対する検証の成立を要求する。必要資料を最後まで供給できず、所定の独立肯定検証が成立しないclaimは成功させない。[RFC-v2-0016](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md)。

## 3. 正常positiveは非対話

Minerは元の結果とcanonical trace／execution rootを先に署名固定する。選出Panelは同じjob・model・input・rules・seedで独立再実行し、元のrootと出力が一致すれば、自分の記録から元Miner rootに対するopeningを作り、一括提出する。逐次的な中間値の質問、Minerの再承認、相違点探索を正常系の条件にしない。

必要なモデル・入力資料の配信、commit後の公開乱数、証拠提出の通信は残る。非対話は通信量ゼロではない。[RFC-v2-0017](../../../rfc-v2/0017-noninteractive-positive-verification.md)。

## 4. 不一致は対話negative、資料不足は未検証

root・出力が異なる場合、元の固定claimと双方のcommitmentに結び付いた対話で争点を局所化する。完成した反証があれば直接提出できる。Nodeが局所演算を裁き、責任を確定する。

root不一致だけでMinerを有罪にしない。未取得・Panel停止・不完全証拠は、計算不正および客観的な提供義務違反と区別する。所定の肯定検証がないclaimを止めるために、negative証拠の完成は要求しない。[RFC-v2-0018](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)。

## 5. 成功Finalの入口

新版は`ResultCommitted`からglobal独立認証を開始し、必要な肯定承認・局所検査・公開異議・依存・資金条件が成立してから成功Finalへ進む。旧Finalを前提にした認証関数へ循環する条件を追加しない。認証準備済みだけではcreditを出さない。

noticeは元の受理`a+3,120`まで、係争hardは`a+10,000`、最遅notice後の残りは6,880 DAAである。資料公開、draw、receipt、交代で原点をresetしない。資料を遅く公開して独立検査の時間を奪うclaimを成功させない。[RFC-v2-0020](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)。

## 6. 保証の境界

局所positiveは一つの計算関係、root一致は同一性、署名は作成者と主張を認証する。いずれも単独では推論全体の正しさを証明しない。全体承認には、globalに選出された検証stakeの誠実性という明示的前提が残る。

global側まで悪意ある枠が承認閾値を満たす場合の虚偽承認を、無条件に防げるとは宣言しない。公開反証、有限の資料提供義務、取消、徴収可能bondを残す。暗号学的な計算証明の禁止は変更しない。

## 7. 適用範囲と状態

旧署名済みclaimは旧規則のまま処理する。新wire・署名domain・fence・数値policy・有効化高さはこの文書で割り当てない。実装接続、cold資源測定、敵対試験、native end-to-end、releaseの各gateを満たすまで未有効化である。

本方針の採択を、RFC-0044の旧全被覆Node検査が既に軽量化された証拠、あるいは現行presetが変更済みである証拠にしてはならない。
