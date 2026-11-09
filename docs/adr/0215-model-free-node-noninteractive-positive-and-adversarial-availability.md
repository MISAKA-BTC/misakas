# ADR-0215 — モデル非保有Node・非対話positive・対話negative・検証先行Final

Date: 2026-11-08

Status: **Accepted design / implementation and activation pending**

本記録は、ユーザーが本会話で示した採択と追加前提に基づく設計変更である。新しい署名版のclaimだけに適用し、旧claim、旧wire、旧fence、原資、既存の責任を遡及変更しない。

## 1. 採択の根拠

ユーザーは、Nodeがモデルを持たず、Minerが資料を供給し、独立したPanelが検証を完了したclaimだけを成功Finalへ進める構成を採用した。また、正常positiveは逐次対話なしで作成し、不一致時はnegativeの対話で局所化すること、RFC本文を20本以内へ整理することを指示した。

追加の敵対前提は、モデルが共有されていることを仮定せず、悪意あるユーザーがモデルを隠してMiner・Panelを寡占し得ることである。この決定は「既にモデルを持つ正直なPanelがいる」という仮定を置き換える。

この文書は設計採択の記録であり、live networkの有効化承認、測定値、コード完成、第三者監査の完了を記録するものではない。

## 2. 解決する問題

1. Nodeの負荷を下げるための局所courtが、隠れた全モデル取得・再実行に依存してしまう。
2. 必要資料を得られないPanelに反証の完成を要求すると、資料を隠すことで成功への拒否条件を回避できる。
3. 正常系でも中間値の往復照合を要求すると、対話の待機と資料の重複取得が増える。
4. 旧Gate CのFinal後認証へ「認証前はFinal不可」を単純追加すると循環する。
5. モデル保有者に限定したPanelからの署名を、独立した全体認証と取り違える。

## 3. 決定

| 項目 | 採用する規則 | 主RFC |
| --- | --- | --- |
| Minerのoriginal | 一回の推論で記録したcanonical trace／execution root、出力、model、input、rules等を先に署名固定する | [RFC-v2-0016](../rfc-v2/0016-miner-execution-commit-and-material-supply.md) |
| 資料供給 | モデル事前共有を仮定せず、選出検証者と適格なpublic outsiderへclaim固有資料を供給する有限の義務を定める | 同16 |
| 正常positive | 独立再実行で元のrootと出力を照合し、Panel自身のtraceから局所証拠を一括作成する。Minerの追加承認を要求しない | [RFC-v2-0017](../rfc-v2/0017-noninteractive-positive-verification.md) |
| negative | 不一致を固定された原claimへ局所化し、Nodeが有界な計算で裁く。既に完成した反証の直接提出を妨げない | [RFC-v2-0018](../rfc-v2/0018-interactive-negative-court-and-defaults.md) |
| Node | 全重み・checkpoint・全推論・GPU・モデル外部取得を必要とせず、認証された局所断片とboundedな実行定義だけを検査する | [RFC-v2-0019](../rfc-v2/0019-model-free-node-and-bounded-evidence.md) |
| 成功Final | 結果固定からglobal独立認証を開始し、肯定認証・公開異議・依存・経済条件が成立した後に成功Finalを置く | [RFC-v2-0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) |

新profileの正常基準は**選出Panelによる全体の独立再実行**である。これはNodeに全再実行を要求する変更でも、既存のsampled profileを無断で変更するものでもない。局所positiveの成立と全体承認は別の型・scopeとして扱う。

必要な肯定検証が成立しなければ、不正証拠が作れなくても成功Finalと仕事creditを止める。資料不足、Panel停止、root不一致だけでMinerを計算有罪にしない。帰責可能な提供義務違反は、署名済み義務・適格要求・提出機会・期限・不履行を確認して別に裁く。

## 4. 脅威前提と保証限界

モデル内のMiner、通常Panel、providerがすべて同一支配下にある場合を対象とする。認証者はモデル保有者集合から選ばず、結果固定後の未知の乱数を用いて成熟済みglobal bonded poolから選ぶ。producer operator除外、一claim一試行、累積予算と費用を維持する。

global抽選の選出stakeにも必要な割合の正直な参加者がいるという、既存Gate Cの明示的な前提は残る。モデル内の寡占と、global承認閾値までの侵害は区別する。別key／bondは実所有者の独立性を証明しない。

**global側まで悪意が承認閾値を満たす場合、署名と少数の局所positiveから全体の正しさを無条件に保証できない。** root一致は資料の同一性、局所passは局所関係の成立である。計算全体の暗号学的証明、実際のGPU使用、最低エネルギー消費を主張しない。

公開された有限の資料要求と第三者の反証経路を残す。資料の公開予定が遅く、所定の公開検査時間が確保できないclaimは成功させない。単なるURL・manifest・受領署名を可用性の完全証明としない。

## 5. 旧規則の限定的な置換

- RFC-0044 R2の「Minerに新しいtrace生成を課さず、Panel witnessをNodeが全被覆検査する」は、新profileでは16・17・19の分担に置き換える。旧本文と旧profileは保存する。
- 旧Final後認証の意味は旧claimに残し、新profileでは20のpre-Final認証と原子的な成功反映を使う。
- PRINCIPLESの通常時全再実行不要の条項には、新profileの選出Panelに限った例外を明示する。Nodeの全再実行は引き続き要求しない。
- model-epoch配分とclaim固有の認証を混同しない。モデルtier、モデル単位審査の不採用、発行・bond配分の既存規則は変更しない。
- original admissionからのnotice 3,120 DAA、係争hard 10,000 DAA、最後のnotice後6,880 DAAを維持する。新しいreceipt・取得・担当交代で時計をresetしない。
- 暗号学的な計算証明の禁止、一つの鎖、公開の第三者経路、画像・動画・モデル改善の延期は変更しない。

## 6. 採用しない読み替え

- Nodeが不足資料を埋めるためにモデル全体をダウンロードする。
- モデル保有者だけのPanel署名、root一致、無異議、一つの正常計算を全体の正しさと扱う。
- root不一致・取得失敗・Panel停止だけでMinerをslashする。
- 未検証Abortや手続き終了を成功Finalと表示する。
- 新版の条件を旧claimや旧`PositiveChecked`の意味へ後付けする。
- 担保の経済的上限だけで、欠けた検証を成立したと扱う。

## 7. 文書編成

現行RFCの本文は、既存15本と新設16〜20の**20本**にする。旧RFCの完全パス、重複番号、原文、既存統合付録は履歴として保持する。番号付きの旧原文を削除・再採番して20ファイルに見せかけない。

RFC-v2-0016〜0020が今回の採択内容を所有し、既存01〜15は各主題と旧版の原条文を所有する。既存の0042二文書、0043〜0047を含む全旧主文書に現在の参照先を付ける。旧番号、wire ID、署名domainは一括置換しない。

## 8. 検証と有効化

本文・索引・対応表・登録台帳・リンク・旧本文保持を文書として検査する。入力に既にある参照欠落やsnapshot hash不一致は、baselineを書き換えて隠さず報告する。

runtimeの受入は16〜20の試験シナリオに従う。Node非保有のcold起動、寡占・選択的非開示、global悪意quorum、root不一致、期限、証拠サイズ、再起動・reorg、credit取消を別々に確認する。

実wire版・fence・数値policy・受理profileの実測・native end-to-end接続・release approvalがそろうまで有効化しない。本ADRは既存presetや稼働nodeを変更しない。
