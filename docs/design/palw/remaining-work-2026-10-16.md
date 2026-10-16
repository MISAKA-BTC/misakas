# 残作業(2026-10-16 時点)— G14・RFC06〜12・RFC02・リリース

ユーザー指示(10-16):トークンが少ないので、RFC07 の認証済み adapter を検証より先に実装する。それ以外は commit・push し、この文書に残作業として残す。
G14 の判定は [g14-completeness-proof-2026-10-16.md](g14-completeness-proof-2026-10-16.md)(本文と §7〜§9・付録 B〜D)、方針は
[t12-daa14000-release-plan-2026-10-15.md](t12-daa14000-release-plan-2026-10-15.md) の最終 3 節を参照。

## 0. 現状の判定

- **G14 の主張(Panel 全員が共謀しても外部の bond 1 つで客観的に有罪)は、まだ証明されていない。** fold 水準と一部 node 水準の試験は通ったが、未解決の gap と未実施の drill が残る。
- **fence は DAA 14,000 から延期した。** 16,000 は目安で、確定値ではない。G14 を全部武装したまま、PALW 報酬・ブロック生成・Final が正常に進むことを、出荷 binary の drill で確かめてから fence する。
- integ(`integ/rfc02-rfc06-12-g14`)は、統合ブランチの検証済み状態と、5 本の G14 lane の merge を含む。新しい規則は全て休眠(全 preset で `None`)で、t12 の identity は変わっていない。

## 1. RFC07 の認証済み selected-approval adapter(今回実装)

- `PalwKernelRouteStateV1::finals_read_v1` は、`PaidClaimV2` Final のうち、`palw_exec_v2_selected_approval_v1` が認めたものに限って beacon の `AttributedWorkV1` を出す。
  - 認める条件:canonical paid Final が、割り当てられた正の quorum(抽選された checker)・bound source・完了 round で native 受理・restore 検査を通っていること。EXEC credit と同じ gate である。
  - optimistic の timeout、Panel-licensed、未承認の paid claim は、従来どおり `None`。
- 残り:
  - 旧 OPV-only の fixture に依存する conformance / OPV bootstrap の試験、約 45 本と core の `rfc0010_production_fold::paid_panel_sources…`。抽選 checker の承認を持つ paid claim の fixture へ移す(迂回で通さない)。
  - Panel-licensed Final の event(route が Panel の seed・epoch を知らない)。
  - m・k の測定(Gate C と共通)。

## 2. G14 の lane — 保存済みで未完了の作業(WIP branch、未 build・未試験)

| branch | head | 内容 | 残り |
|---|---|---|---|
| `integ/g14-tir-pricing` | `61ab3a9a8` | frame の課金・期限を測る harness の途中 | 32×32=1,024 step の課金が要るかの実測。debit ごとの inclusion margin と `reorg_replay_moves` の積み上げの見直し。実測にもとづく cost model と期限。T-N1(試験専用 config)。正直な IR FP claim が Final・支払いまで届く node 試験 |
| `integ/g14-court-capacity` | `5b64cac9c` | court 容量(`palw_court_capacity_v1.rs` / fold)、Panel 担保の試験の途中 | 処理予算の実測 harness、占有時間の課金、期限付きスケジューラ。42 枠飽和+継続的な埋め直し攻撃の試験。Panel の Active/Resolved 分離(`palw_final_lock_full_collateral`、R_new の式)。late conviction、資本効率の比較 |
| `integ/g14-drill` | `b8858af58` | 共謀 drill の driver の途中(`scripts/drills/g14_collusion_drill.py`、`g14_drill_factory.rs`) | D-01〜D-18、court/Panel の 7 条件、正常系 P-1〜P-5、RFC-0019 R-7/R-8 の実装と実行 |

## 3. G14 の未解決 gap(監査の ID)

- **G-07** court の 42 枠:連合が全枠を占有できる。fused leaf 経路は、飢餓耐性を実証するまで (b) で「G14 経路あり」と数えない(ユーザー方針)。
- **G-13 の効率**:Panel 担保の Active/Resolved 分離が未実装。Final+1,000 の無条件解放へは戻さない。
- **TIR frame の価格・期限**:t12 の現行価格では、賃料が 2e7 MSK 以上、告発を開けるのが 0.9e6 DAA 後、Final が 6e7 DAA 後で、実用にならない。実モデルの leaf 数では u64 を超える可能性もある(未測定)。
- **native**:G-N1(v11/v29 の告発枠)、G-N2(v39 mining の Final 後の告発。veto の扱いは RFC-0018 U-6 のユーザー判断)、G-K2(EXEC の試験 seam GAP-81)。
- **G-K3 / F17** 検証者の誘因:正直な検証者が費用を回収できることの証明。自作自演は finder 49% 上限で損になる。
- **G-11** rule E、**G-12** bond 式 v2、**G-04** Gate C の武装:いずれも測定値と判断が待ち。
- **X3** の候補値は 5,520 DAA。未測定の項(B_cold_fetch など)が残る。
- T-N1 の本番経路 harness:IR class の登録 → `Active`、TIR family の認証、layout 公開、`TirPublicGrantReadyV1`、外部 owner/filer の service。

## 4. 一括検証(`integ/validation` `37cf3fa89`、記録は consolidated-validation-2026-10-15.md)

- green:ladder 20 段、t12 identity、G14 対象の 150 試験。
- red:kaspad service E2E 16 件、RFC-0018 sibling 3 件、rfc9_v4、rfc0016 の 1 件、rfc12 ×4、t12_a2u_mixed ×2、SDK ×3、tir-lower ×4、base0 ×3。一部は旧 base でも失敗していた。
- clippy は警告 557 件。lane が落ち着いてから 1 回でまとめて直す(`--exclude misaka-palw-worker`)。
- 最新の integ head で、全 suite を再実行していない。

## 5. リリース(fence 延期後)

全部完了 → 一括検証 → 実ノードでの検証・測定(σ/k、Q/B/R/F、m/k、court 予算、frame の cost unit)→ 出荷 binary で fence を跨ぐ drill → ユーザー承認 → 配備。
fence の高さは、条件が揃ってから決める。

## 6. genesis モデル(RFC-0020 / PALWART3)

- #2 Huihui-Qwen3.5-2B の 32K 変換は、fit の長文チェック(integer 20k/32k 付近)で停止した。再実行は fit から(`genesis10_pipeline.py run --model 2`、変換と record は済み)。
- #8 は Q8_0 GGUF からの 32K streaming 変換(`--stream`)が未実行。再量子化であり、品質ガードは 64GB 以上の機械が要る。
- #6(27B bf16 GGUF streaming、15〜22 h)、#1 の 32K 再変換、#3/#4/#5/#10 が未着手。
- RFC02 の変換 artifact は、main checkout の `target/` 削除で失われた。evidence の sha256 で再生成・照合する。

## 7. ユーザー判断待ち

- RFC-0019 U-1 / U-3 / U-5。
- G14 の Lead 既定値(owner cap 8、V3/V4 拒否、idle grace 256 DAA)。
- legacySettlementWindowDaa と reserve drip の Lead 既定値。
- RFC-0018 U-1〜U-8(Gate C)。
- #3 のモデル選定(暫定)。
- Downloads 削除の A/B/C 区分。
