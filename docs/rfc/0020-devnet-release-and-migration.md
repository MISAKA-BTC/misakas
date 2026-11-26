# RFC-0020 — ローカルdevnet・公開条件・移行

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0020](../adr/0020-devnet-activation-and-document-governance.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 開発の目標

実LLM推論から正規claim、Panel/Court、Final、独立Gate C、Ticket当選、権利一回消費、PALW受賞block、通常合意受理、報酬決済までを同じprofileで接続する。環境管理や観測器の拡張だけを完了としない。

## 2. 実装と試験の順序

1. 入口から受賞・決済までのcall graph、状態遷移、各阻害条件を一覧にする。
2. 未接続の正常経路と拒否・予算・一回性を実装し、必要な短時間試験で確認する。
3. 正規の短時間devnet設定で実LLM由来の受賞まで通す。
4. 初回受理、P2P伝播、3peerの同一状態、決済を確認する。
5. 二重消費、restart、reorg、pruned復元を確認する。
6. 最後に対象公開profileの期間・難易度・容量による総合試験を行う。

修正対象に関係しない成功済み試験を理由なく繰り返さない。長時間DAA待ちを未接続コードの調査・実装の代替にしない。

## 3. ローカル環境

Node、Studio、Miner、Panel、Gate C、TB1、BFT投票、テスト資金、bond、資料をローカルで用意する。test用鍵・genesis・network・署名domain・データ領域を公開環境と分離する。

Node3台は状態一致の検査用であり、必要operator/署名者数とは別である。G14 1席、独立Gate C、TB1/BFTの定足数を各profileから導き、不要な席数を固定の慣習として持ち込まない。必要な故障前提を減らして公開安全性を主張しない。

genesisに正規modelがある場合は再追加せず、実行artifact・tokenizer・layoutと必要なbondを準備する。新規追加を試す場合はBFT YESからmodel/marketの原子登録までを別に確認する。

## 4. 高速化と待機の削減

Fastでは決定論的fixtureと短い期間を、実LLMの短時間devnetでは隔離genesisに事前固定した期間・難易度・時計を用いる。通常のblock生成・検証・状態遷移を使い、既存台帳の途中で規則を変えない。

heartbeat、次slot通知、イベント駆動、準備済みcheckpointを利用する。DAAやFinalの直接書換え、架空当選、認証省略をしない。元の期間でのFull結果と高速化した結果を区別する。

既存TFO3等の経路を用いる回帰試験は元familyを明示し、新FOW1/TB1の成功として数えない。新規公開profileの成功はそのprofile自身で確認する。

## 5. 試行予算と資源

試行数、wall time、RAM、disk、保存世代、最大Job、必要報酬・bondの支払可能額を開始前に検算する。落選を確認した同じ固定Ticketを繰り返し再評価せず、必要なら予算内で新規の正規試行を作る。

進行が止まったら期限待ち、入力拒否、モデル未承認、資料不足、当選落選、受賞不能、容量不足を区別する。保存台帳を保ったまま同じ原因を繰り返すのではなく、必要な実装又は設定を修正する。

snapshotは有限数とし、複製の実容量を確認する。不要な生成cacheを除去しても、活動中のclaim・署名・責任・曝露の保存を壊さない。

## 6. 完成の証拠

claim ID、受付block/DAA、Panel承認、Court終端、Final、Gate C、正規当選Ticket、受賞block hash、初回submitBlock、P2P伝播、3peerのroot、仕事キー所有、消費index、予算差分、報酬UTXO、settled frontier、終了codeを保存する。

claim Finalと台帳確定を混同しない。通常block、heartbeat、fixtureだけの受賞、既受理blockの再提出は実LLM受賞の証拠ではない。E2E、容量・負荷、合意安全性の分析を別の結論として報告する。

## 7. 変更管理と公開

文書採択、code接続、短時間試験、Full試験、互換性・移行確認、公開activationを順に判定する。必要な署名codec、wire、policy、roster、fence、旧claim cut/drain、rollbackと停止条件を確定する。

既受理claimの受付時規則、署名、元期限、依存、責任を保持する。旧新consumerの二重発行、旧AUTOからBFT承認への偽装、旧beacon fallback、モデルroot差替えを禁止する。旧readerの維持は新規旧方式の入口を許すことではない。

リリースは固定commit/binary/設定を対象にする。既知失敗を隠して全成功と表示せず、公開の阻害条件を具体化する。VPS・配備・公開設定・実支払いは、ローカル検証とは別の明示的な作業として扱う。

## 8. 文書の再編と参照

現行RFCとADRはそれぞれ0001〜0020の20本である。同番号のRFCへ数式・形式・状態遷移・制約・受入条件を、ADRへ背景・判断理由・代替案・帰結を置く。旧71RFC・235ADR本文とその改訂複製をこのdocsに保持しない。対応表は旧参照の解決に限り、旧ルールの再採用やruntimeの自動移行を意味しない。

RFCの番号を変えてもwire・署名domain・既存enum値・block/claim IDを改番しない。残された実測ログは当時の構成の結果として読み、現行profileの完成証拠へ昇格させない。
