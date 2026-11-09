# 2026-11-08 — RFC20本への整理と検証先行Finalの採択

入力: 今回添付されたdocs(2).zip。採択根拠: ユーザーが本会話で提示した設計を採用し、
RFCを修正して20本に整理するよう明示的に指示したこと。
状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。変更対象は文書と文書検査ツールである。

[現行20本の入口](../../../rfc-v2/README.md) /
[採択ADR-0215](../../../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) /
[検査結果](VALIDATION.md) / [変更ファイル台帳](changes.json)。

## 1. 採用した設計

**Minerが元の実行traceと結果を署名して固定し、独立に選ばれた検証者が再実行を完了したclaimだけを成功Finalへ進める。Nodeはモデルを持たず、承認情報と有界な局所証拠を検査する。**

Nodeの非保有条件は通常経路、court、cold start、IBD、再起動、reorg後の復元、履歴再検査に共通する。
モデル本体・全重み・checkpoint・GPU・推論環境・Panel workerをNodeの必須依存にしない。
局所裁定に必要な少量の重み・入力・中間値・演算定義は、固定されたrootに対する認証と資源上限を付けて証拠に含める。

モデルが公開済み・共有済みであることを仮定しない。悪意ある運営者がモデルを隠し、
そのモデルのMiner、通常Panel、資料providerを全て支配する可能性を脅威モデルに含める。
その集合だけの署名をFinalの根拠にせず、モデル保有者で絞らないglobal poolから
commit後乱数により検証者を選ぶ。必要資料が得られない正直な検証者は肯定承認しない。

globalな悪意stakeが承認閾値も支配した場合まで、小さな局所positiveと署名だけで
全体計算を無条件に保証する設計ではない。モデル内寡占への対処と、global承認前提の限界を別に明記した。
鍵数やbond数を独立した所有者数と同一視しない。

## 2. 正常・不一致・資料不足の経路

| 状況 | Panel／独立検証者 | Nodeと成功条件 |
| --- | --- | --- |
| 所定の資料を取得でき、canonicalなrootと出力が完全一致 | 全再実行の承認と、自身のtraceから作る局所positiveを一括提出 | 選出・署名・計画全対象・期限・公開異議・依存を確認し、条件成立後のみ成功Final |
| 元rootまたは出力が不一致 | 両者の固定rootを起点に対話で争点を局所化。完成済み反証は直接提出可 | 元Minerの値・演算に束縛した小さな計算を裁定。root不一致だけでslashしない |
| 必要資料が不足し、所定の検証が成立しない | 肯定承認しない。適格な供給要求または期限内の回復へ | 反証を作れなくても成功Final不可。期限内に成立しなければAbortedUnverified |
| 署名済み供給義務への適格要求に、提出機会と期限があるのに不履行 | original・役割・要求・機会・期限の記録を提出 | QualifiedDeliveryDefaultとして帰責。計算有罪とは分ける |

正常positiveに、Minerへ中間値を一つずつ尋ねる対話・二分探索・追加署名を要求しない。
資料の最初の供給、公開乱数の確定、完成証拠の提出は必要であり、通信量がゼロになるという意味ではない。
Panelが同じcanonical traceを再構成すれば、自分の木のopeningを元Miner rootに対して提示できる。

局所positiveは検査対象の関係だけを確認する。独立全再実行の承認とは別であり、
一つの正しい局所計算を全推論の正しさへ拡張しない。
同じ対象の証拠を何人が提出しても、計画中の別対象の検査は満たさない。

## 3. 20本の担当

| 範囲 | 担当 |
| --- | --- |
| RFC-v2-0001〜0015 | 既存のjob・TIR・登録・階層・資源・参加・会計・依存・API・合意・資金・保存・release・共通court・変更管理を保持し、新profileの適用範囲を追加 |
| [16](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md) | Miner original、canonical trace、署名、有限の資料供給義務 |
| [17](../../../rfc-v2/0017-noninteractive-positive-verification.md) | 非対話positive、独立全再実行、元rootとの一致、局所検査計画、承認scope |
| [18](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md) | 対話negative、局所court、公開第三者、qualified default、未検証終了 |
| [19](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md) | Node非保有の境界、自己完結する局所証拠、資源・保存・cold再検査 |
| [20](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) | 非共有・寡占脅威、global選出、事前認証、成功Final、時計、保証限界 |

旧番号RFC48ファイルの現行担当は[対応表](../../../rfc-v2/MAPPING.md)と
[adoption-map.json](../../../rfc-v2/adoption-map.json)で追える。
旧0042は二つの異なるファイルをそのまま保持し、完全パスで区別する。
旧41原文→15冊の歴史source-mapと、89の原付録区間は保存する。

ADRの統合50本は変更せず、新ADR-0215を採択記録として登録した。
入力時点の210の旧番号ADRも保持し、採択記録の追加後は211ファイルとなる。
これらの「冊数」「旧番号原文数」「post-consolidation記録数」を同じ数として扱わない。

## 4. 版と時計の変更境界

新profileでは、ResultCommitted / VerificationPendingから検証を開始し、
Finalを前提としない事前認証predicateでCertificationReadyへ進む。
成功Finalはその後に置く。CertificationReadyだけでは仕事報酬・consensus creditを与えない。
旧Gate Cの「Final後に認証」を新claimのFinal条件にそのまま使う循環待ちを禁止する。

原受理時刻a、通常noticeのa+3,120、係争hardのa+10,000を維持する。
receipt・資料受領・抽選・担当交代で時計をリセットしない。
最後の合法notice後にも6,880 DAAの範囲内で、局所化・提出・裁定・回復を完了させる。
開示をnotice期限直前まで遅らせる攻撃に対しては、
cold取得・独立再実行・異議作成・包含の予算を元の時計内で確保できることを成功条件とする。
保持期限と資料収集期限は同じ値へまとめない。

旧R2の全被覆検査、旧署名、既存claimのFinal、fence、wire、資金権利はその版の意味で保持する。
新しい保証は新しいprofile・scope・署名形式・native規則で導入する。
B=F=R=0のWorkerPaymentやfunded relayの同意・予約・返還を、新しい肯定承認から拡張しない。

## 5. レビューで補強した条件

- モデル保有者内の全員結託と、global承認閾値の侵害を区別した。
- 重複あり抽選のmは位置数であり、独立operator数ではないことを明示した。誠実な位置がk以上なら悪意位置がk未満、という前提を使うpolicyには2k>mを要求する。
- 遅い開示でも、正直な検証者と適格な第三者が元の期限内に検査・異議提出できる予算を要求した。
- 自分が供給・保持を約したoriginal資料の隠匿・破棄・未保存を、外部前提不足によるdefault免責に含めない。
- 局所検査対象集合の被覆と、独立承認の抽選位置数・署名数を別に数える。
- 再起動中のNodeが履歴資料をまだ取得していないことを、過去Minerの新しい有罪理由にしない。

これは文書の意味についてのレビューである。実装試験や形式的な安全性証明の代わりにはしない。

## 6. 検査と残る作業

[検査結果](VALIDATION.md)には、今回入力との差分に対する保全検査と、
以前のsnapshotを基準にする既存監査を別々に記録する。
[input-manifest.json](input-manifest.json)は入力archiveのSHA-256と全実ファイルのdigestを保持する。
[changes.json](changes.json)は追加・変更ファイルと入力／出力digestを記録する。
従来の欠損リンクbaselineや旧source-mapのhashを書き換えて、不一致を隠していない。

新profileの実装・有効化には、Minerの1回の推論からのtrace取得、
全native入口と状態遷移、資金処理、公開反証、cold Node、reorg、
実測の資源・時間policyを接続する必要がある。
未測定のMiner増分を20%等の固定値とはしない。
実装用の未完了gateと受入ケースは各新RFC、特に20に集約した。

## 7. 反映

このZIPはdocs一式の更新版である。別作業があるrepositoryへ反映する際は、
入力からの変更台帳を参照してmergeする。
今回の文書編集だけで稼働ネットワーク、コード、preset、genesis、署名domainを変更しない。

