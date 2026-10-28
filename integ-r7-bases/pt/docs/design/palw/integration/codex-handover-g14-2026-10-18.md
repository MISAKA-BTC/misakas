# Codex への引き継ぎ — G14 の残り全部(2026-10-18)

Claude 側の 3 本の lane(drill、TIR frame と genesis、native と DAG)を、Codex に引き継ぎます。各 lane は、未 commit の作業を commit・push し、自分の branch に lane 別の引き継ぎ書を書いてから止まります。**走っている検証(テスト・fit・drill)は止めていません。** そのまま走らせ、結果を読んでから続けてください。

規則は [codex-handover-2026-10-18.md](codex-handover-2026-10-18.md) の §0(必ず読むもの)と §4(規則)のとおりです。G14 の現状は `docs/audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md` の §11〜§13 にあります。

## 1. lane ごとの場所

| lane | branch | worktree(このまま使う) | lane 別の引き継ぎ書(branch 上) |
|---|---|---|---|
| drill・受入試験 A〜G・最終段 | `integ/g14-drill` | `/Users/wata/Downloads/MISAKA-testnet/.claude/worktrees/agent-a74e0e62dabe9dcf5` | `docs/design/palw/integration/handover-g14-drill-2026-10-18.md` |
| TIR frame・T-N1・RFC02 identity・RFC13/20・genesis | `integ/g14-tir-pricing-r2` | `/Users/wata/Downloads/MISAKA-testnet/.claude/worktrees/agent-abd1b9bfd824ba6a9` | `docs/design/palw/integration/handover-g14-tir-genesis-2026-10-18.md` |
| native(v11/v29/v39)・court 容量・検出 policy・RFC-0023 段階 1・RFC18 (ii)〜(iv)・GDN filer | `integ/g14-court-capacity-r2` | `/Users/wata/Downloads/MISAKA-testnet/.claude/worktrees/agent-a76d982ea89a88e54` | `docs/design/palw/integration/handover-g14-native-dag-2026-10-18.md` |

各 worktree には build 済みの `target/` があります。worktree を新しく作るより、そのまま使うほうが速いです。lane 別の引き継ぎ書には、走っている処理の PID・コマンド・ログの場所、結果の読み方、次の手順が書いてあります。もし引き継ぎ書がまだ無ければ、その lane の最新の commit と proof doc §13 から状態を読んでください。

## 2. 残っている作業(優先順)

**G14(最優先)**
1. **drill lane**
   - G-D02a:有罪が確定したら、Valid に署名した seat 全員へ kind-3 の FalseValid を自動で filing する(`kaspad/src/palw_filer_false_valid.rs` の `note_proof_v1` に接続)。D-02 で、共謀 seat 3 人が lock の額だけ slash されることを確かめる。
   - G-D02b:fork-id の切り替えをまたぐ reorg で予約が消えたら、filer が出し直す。
   - G-D02c:毎回嘘をつく producer に、外部の bond 1 つが追いつけるようにする(1 本の carrier に複数の filing をまとめる、または 1 block に複数の carrier を出す)。
   - F の境界:未払いの報酬は、すべて告発の窓に収まること。
   - 直したら D-02、sparse、skip-all(default まで)を drill し直す。
2. **native lane**
   - v39 の料金を、実際に運ぶ bytes で課金する(ユーザー決定)。
   - Gate C の m=47 に足りる bond を harness に用意する(値は変えない)。
   - Master と mining の経路から、action 80 の Final 後の告発まで実 claim で通す。確かめること:徴収、receipt・weight・認証の取り消し、attestor への charge、正直な claim は変わらないこと。
   - GDN filer をノードの定期処理につなぎ、node E2E を行う(全 seat が共謀し、外部の bond 1 つで追及)。
   - `rfc0018_dag_siblings` の失敗 3 件を直す。
3. **TIR lane**
   - T-N1 の支払いを node 水準で示す(quantum が実際に使われるところまで)。
   - 共謀した seat の slash を示す(licence を有罪の確定より前に着地させる)。
   - 02-B:既定でない semantics を宣言した class での node 実行。

**RFC18**
4. 複数モデルの並列受入(drill lane の引き継ぎ書を参照)。RFC-0023 段階 1 の各段階の時刻を、それぞれ別に測ること。

**genesis(TIR lane)**
5. #2 は 32K の長い fit が走っています。合格したら `--steps upload,verify,cleanup` を実行してください。HF に上がっている `00000.palwseg` は、ユーザー決定により残します。不合格なら README に「未検証」と書きます。
6. 続いて #1 → #3 → #4 → #5 を 1 つずつ処理します。#6・#8・#10 はこの Mac では処理できない見込みです(関門で止まります)。#6 はユーザーが自分の機械で `docs/operations/models/genesis-6-qwen38-27b-palwart.md` の手順で変換します。HF への push はユーザーが許可済みです。

**最終段(上がすべて終わってから)**
7. 統合する。4 本の G14 lane に加えて、Codex の branch(`codex/rfc23-stage2`、`codex/rfc18-dag-width`、`codex/red-tests`、`codex/acceptance-rfc`、`codex/rfc08-09-10-12`)を 1 本の release 用 branch にまとめ、全 lane の対象試験と t12 の指紋の試験を流す。
8. live の t12 の DAA を読み取り専用で確認したうえで、新しい fence をすべて DAA 16,000 に設定する。測定記録を出荷用の release binary で実測して pin する。測れない値は作らずに報告する。
9. 出荷 binary で 16,000 を跨ぐ drill を行う。D-01〜D-18、A〜G、RFC18、RFC19 R-7/R-8、正直な claim が支払われること、不正な claim が slash されること、restart・reorg・IBD・pruned import。画像・動画・モデル改善が開かないこと(ADR-0182)も確かめる。
10. すべて通ったら、main(fast-forward のみ)・pre・integ に push する。**t12 のノードと VPS への配備は、ユーザーの承認を得てから。**

## 3. 決定済みのこと(変えない)

ADR-0183〜0187、RFC-0023 / ADR-0186 に記録済みです。

- Panel floor 240,000 MSK
- court class の保持期間 ≥ 4W + 開示窓
- 退役 ≥ Gate C の告発期間 + 余裕
- RFC16 の平均窓は 7 日(5,040 DAA)
- beacon は固定 seed で起動し、以後は seal-reveal 方式
- onboarding の窓 1,024 DAA、binding の責任期間 1,208 DAA
- RFC08 の責任期限 = Final の下限 + window_court(署名済みの入力から導く)
- RFC07 の PanelLicensed は記録用の event だけ
- RFC18 の報酬は 1 本
- G-N2 の値は実装値で採択、veto は 0
- text29 の carriage period は 1 DAA
- v39 は実際に運ぶ bytes で課金

## 4. 報告

proof doc の §13 以降に、日付付きの節として追記してください。試験で示していないことを「完了」と書かないでください。
