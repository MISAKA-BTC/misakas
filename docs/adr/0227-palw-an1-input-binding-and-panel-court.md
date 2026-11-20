# ADR-0227 — PALW-AN1の実入力束縛をPanel＋Courtへ接続する

<!-- FOW1-TB1-20261120-BEGIN -->
## 後続採択と適用範囲 — 2026-11-20

ユーザーの今回の修正指示により、新規採掘の正本を[ADR-0230](0230-palw-function-output-and-work-admissibility.md)／[RFC-0062 PALW-FOW1](../rfc/0062-palw-function-output-and-work-admissibility.md)、乱数源を[ADR-0229](0229-palw-internal-threshold-beacon.md)／[RFC-0061 PALW-TB1](../rfc/0061-palw-internal-threshold-beacon.md)へ置換する。新方針は**ADOPTED_DESIGN_NOT_ACTIVATED**。以下は旧AN1草案の判断記録であり、単独の新規採掘適格性を与えない。

新profileはAN1の実入力・Panel＋Courtを継承し、コア採掘出力zの非衝突性、複数試行費用と全checker coverageを共通登録条件へ追加する。EXEC/PANELは独立TB1秘密に固定し、四Root Ticketを使う。0062のactivation A以後は旧AN1の新規受付を閉じ、旧受理claimのみ固定drainで扱う。本番の定理・適合モデル・native実装・公開有効化の完了とはしない。
<!-- FOW1-TB1-20261120-END -->

- 日付: 2026-11-18（Asia/Tokyo）
- 状態: **DRAFT_NOT_ACTIVATED**
- 対応RFC: [RFC-0059](../rfc/0059-palw-an1-input-binding-and-panel-court.md)
- 同時改訂: RFC-0056 §17、RFC-0057 §17、RFC-0058 §16
- 記録: [AN1改訂README](../maintenance/revisions/2026-11-18-an1/README.md)

## 背景と2026-11-18の指示（旧記録）

ユーザーは、先行のwhitepaperに基づくRFC改訂の途中で、方式をPALW-AN1へ変更し、**Panel Court方式を維持してwhitepaperとRFCを修正する**よう明示した。本ADRの以下の本文はその時点の指示を設計草案へ反映した記録である。現在の追加条件と乱数生成は冒頭の後続採択に従う。

Groth16をFinal必須条件とする作業中の案は本改訂へ含めない。既存AGENTS/PRINCIPLESの計算validity proof不使用、モデル非保有Node、局所裁定、単独反証と整合させる。設計草案の作成指示を、nativeの公開activationや性能検証済みという承認に読み替えない。

提供AN1説明は実入力へのnonce束縛、一意な実行、直接転用防止、全関係からの局所反証、一人正直Panelの条件付き安全性を述べる。元AN1のHTML/ZIPは未取得なので、報告された試験数・trace長は本改訂の再実行結果と分ける。今回の実際のRFC基準は添付AuditRevision ZIPである。

## 判断

以下の判断1・3の未指定beaconは、新規FOW1でTB1 EXEC/PANELへ限定する。判断8の費用定理はFOW1では必須の採掘gateであり、AN1三Root Ticketをそのまま新familyへ用いない。

1. **AN1を別familyにする。** 旧SWC1の未来R_ticket方式とは異なり、事前登録Cと実行beaconからsを作り、実入力・出力・traceの三RootからTicketを導出する。旧codecの意味を黙って変更せず、Dを二重に適用しない。
2. **nonceを実tokenへ入れる。** 固定単射のbyte→token表とchallenge prefixを使い、実際の入力読出しまで検査する。切詰め、UNK、template削除で束縛を失わせない。
3. **二つのfreezeを設ける。** Cと全候補は実行beaconへの最初のアクセス前、元claimと選出poolは未来Panelイベント前に固定する。候補の一部だけ計算する選別は費用分析へ残す。
4. **同一Cに一つのslotを割り当てる。** 別資金・別carrier・再署名でslotを増やさず、sと実仕事の消費もFinalへ原子的に接続する。BaseWorkKeyの有用成果creditは別の用途別規則にする。
5. **Panelの完全再実行を維持する。** 一部の正しいCourt、他人のpositive、資料ACKを全体検査の代替にしない。共有資料・正しい最適化は許すが本人労働を証明したとはしない。
6. **Nodeは期待indexを導く。** 元claim、固定program、正規入力、model rootからCourtの期待Root/位置/重みindexを決める。認証された誤りと偽openingを区別する。
7. **反証と未充足を成功より優先する。** 一人の有効反証は多数承認に優先し、資料不足・構造不正・適格異議未解決をtimeout成功へしない。全consumer、原期限、容量予約、同batch順序、現在state、reorgを接続する。
8. **費用定理を分離する。** AN1の新しい入力は通常LLM費用の下界を意味しない。sigma依存入力へ旧FULL1の固定A/BのbC≥KN等を流用しない。

## 代案と採らない推論

| 案・推論 | 判断 |
| --- | --- |
| Ticketにnonceを付けるだけ | 実入力との結合がなく、AN1の目的を満たさない |
| 長い共通promptの後ろへ自由文字列nonce | tokenizer/切詰めとcacheの境界が曖昧。固定実token prefixを採る |
| beaconを見てからnonceを無制限に登録 | 入力選別・候補集合の分析が変わる。今回のAN1では事前登録を採る |
| 元claimをPanelの結果から補完 | 後出しが可能。選出前に元Rootを固定する |
| 正しい局所Courtが複数あるので全体正当 | 全関係検査ではない。Panel完全再実行と単独反証を維持 |
| Groth16等の計算proofを追加 | ユーザーの最新指示と既存方式に従い、本案へ導入しない |
| 正しいcheap traceをslash | 同じ正しい記録の生成手順は観測できない。費用会計と不正を分離 |
| nonceごとに違う入力だから同額の費用 | 関数の評価困難性を示していない。別のc_min定理を要する |
| 同一slotのconsumeだけで全再包装を防げる | 同一C別slotの問題が残る。登録一意性とs/実仕事消費を加える |

## 保持する条件

全model共通容量とdifficulty、120秒の制御epoch、外側全carrier合計1 BPS、資料・反証の保護、モデル無審査1.0/0.3、既存経済上限を維持する。固定court件数、モデル別枠、120秒必ずFinal、1秒settlementを追加しない。

モデル非保有Nodeには、認証された小さい局所witnessを渡す。大きな演算を無制限の一命令へ隠さず、全local関係・Init/End・構造をprofileに定義する。モデル登録とその品質・実費保証の審査を混同しない。

## 帰結と限界

owner/nonce、実入力、元claim、Court、creditの対応が仕様として追跡可能になる。一方、challenge tokenで利用者jobの実入力が変わるため、元prompt単独と同じ出力・品質が得られるとは限らない。入力変換をサービス契約へ明示する。

少なくとも一人の能動的な正直Panel、実資料取得または未取得時の成功停止、反証の包含と裁定、完全なlocal checker、正しいnative処理が安全性の前提である。全Panel悪意・検閲・無期限の資料不足を仮定なしで解決しない。独立参照実装はその境界を含む反例を検査する。

本番のnative接続、実署名・beacon、cold取得、実LLM、最悪資源、全carrier、DAG/settlement、LLM費用下界は未完のgateとして残す。旧採択・旧署名・過去ログ・digestは保存し、新たな検証結果だけを今回のbytesへ結び付ける。

## 状態の扱い

以下は旧AN1草案としての状態を保持する。新規採掘に適用する採択状態・置換点・未有効化条件はADR0230と0062 §9/§10が所有する。

この記録は草案である。AN1を本番へ有効化するための採択、codec/パラメーター、実装・検証・公開条件はRFC-0059 §15に従う。現行RFC-v2の20冊・ADR-v2の50冊を維持し、新0059/0227を統合後の追加草案として案内する。
