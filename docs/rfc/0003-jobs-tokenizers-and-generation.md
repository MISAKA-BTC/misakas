# RFC-0003 — Job・tokenizer・決定論的生成

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0003](../adr/0003-jobs-and-tokenizer-contract.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. Jobの固定

Jobはmodel/descriptor、入力、許容長、数値・生成規則、prefix origin、出力型、実行目的を固定する。通常Chat、非報酬評価、採掘、EXECのための仕事を別の目的として認証し、評価結果へ後から採掘報酬を付けない。

FOW1ではowner・nonce・job・epoch・EXEC参照等から導く試行challengeを、実際のtoken入力又はprofileが定める初期状態へ組み込む。回答の後ろにIDを付けるだけでは試行依存の実行にならない。

## 2. tokenizerと入力の同一性

tokenizer、vocabulary、special token、chat template、normalizer、added token、adapterと長さ制限をdescriptorへ固定する。token列、position ID、attention mask、challenge位置、切詰め規則を一意にする。

Nodeが保持しないtokenizer資料を必要とする場合は、受付前に有料のOpen/Part/Finish搬送で正規digestへ結合する。part index、全長、chunk digest、署名、原資、元予約期間を検査し、Finishまで未完成な登録を採掘へ使わない。

genesisにmodel identityがあることと、実行者・検証者がtokenizer/layout/containerを取得済みであることは別である。同じ固定tokenizerを再登録してModel IDを作り替えず、必要な資料供給と認証だけを行う。

## 3. 決定論的生成

各profileは生成器、seed導出、乱数消費順、top-k/top-p、temperature、logit bias、repetition penalty、同点処理、EOS、Stop文字列又はtoken列、最大長を固定する。HTTPオプションの順序や実装の辞書順へ依存させない。

samplingを使うprofileは登録されたPRNGと整数又は固定精度処理を使い、同じseedというだけで別backendの一致を仮定しない。QF1のgreedy・最小token ID同点処理を、sampling可能な全profileへ強制しない。

grammar/schemaは有界状態機械と正規入力を持ち、未対応の拡張は拒否する。MINING profileの範囲を超える機能はChat機能として扱えても採掘claimの意味に混ぜない。

## 4. 位置・停止・出力

入力位置と生成decisionを分離し、最後のprefillから最初の予測を行うprofileでは最後の生成tokenの再評価を仕事へ足さない。EOSやStop後の未実行token、要求最大長、paddingを実行量として申告しない。

回答tokenの支払済み範囲をrecordへ固定する。公開carrierに回答を再掲することは再計算でも追加creditでもない。自然言語の回答が別試行と同じであることだけを不正にせず、FOW1の採掘値・実行・Ticketを別に確認する。

## 5. prefixと並列実行

KV/prefixを再利用する場合は、元model・input・position・状態・生成元の署名済みwork identityと範囲を参照する。別challenge、別numeric profile、認証されないcacheを同じ状態として利用しない。

複数completionは別の固定候補として事前登録し、各候補の費用・予約・Ticketを規則どおり扱う。一件の実行をn個の独立抽選として複製しない。共有weight・batch最適化は許容するが、jobごとの結果と責任を分離する。

## 6. 資金・再開

推論前に実Jobからnative見積もりを取り、実行・claim責任・資料供給・受賞支払いの各予算を確認する。別の基準Jobの費用だけで実Jobの可否を判定しない。

署名済み資金取引と提出carrierのbytes、txid、使用UTXO、予約を保存する。未確定の釣り銭を使う続きのPartは、正規の利用可能条件を待って提出する。再開時に別署名・別Job・別nonceを黙って生成しない。

## 7. 受入条件

tokenizer差替え、欠落part、重複Finish、停止後の水増し、別prefix、同点処理差、sampling消費順差、二重completion、予算不足を拒否する。正常入力はMinerとPanelで同じ実token・停止・回答・Readoutへ到達する。

画像・動画の採掘とモデル改善はmainnet 後へ延期・当面無効である。[適用範囲](../governance/post-mainnet-scope.md)に従い、共有Job型やChat APIから迂回して有効化しない。

## 8. 受付前tokenizerと入力資料

tokenizer資料はprofileが要求するhash・正規metadata・合計長・各part位置を受付前に解決する。Open/Part/Finish型の搬送では、元Open、partの連続性、重複、総量、Finish digestを検査し、未確定の釣り銭を確定済み資金として予約しない。

tokenizer登録の手数料・容量・保持は実在原資で支払い、同じ資料の再掲でmodelや仕事を重複登録しない。モデルがgenesisで固定済みでも、別途必要なtokenizer・container・layout資料の供給と発効を確認する。モデル再登録を資料供給の代用にしない。

長文脈を状態鎖として扱うprofileでは、初期入力、各位置の履歴root、更新、末尾の生成関係を固定する。回答の分割搬送と実行上の停止位置を区別し、分割数だけで仕事量やTicketを増やさない。
