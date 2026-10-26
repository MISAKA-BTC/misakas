# lane PANELDA 報告 — PanelDa(非公開テキスト)の G14 経路

branch `claude/g14-fam-panelda`(基点 d8aded6de)。画像(FP V5)は実装しない(ユーザー 2026-10-26)。

## 方式
ADR-0206(proposed、武装はユーザー判断): **期限付きの必須公開**。`prompt_token_ids_hash` で束縛 → service 中は seat のみ → `a+1,040` までに ids を公開(誰でも提出可、hash が中身を固定)→ 外部者に notice 窓 2,080 DAA が残る → 公開 FP family(③〜⑥)で告発。未開示は default: claim void・deposit 没収・**計算の有罪/計算 bond slash にしない**。開示まで報酬・credit は保留。比較した案 B〜E と不採用理由は ADR。

## 実装
- fence `palw_panel_da_disclosure_v1`(Params、全 preset `None`、Some-only hash、`for_each_fence`・正規化・fork_id 名前表に追加)。validation: `palw_panel_da` と `palw_court_clock_v2` が同じか下、窓の算術、**配線未完のため武装は常に拒否**。
- `PalwG14RouteFamilyV1::FpPanelDaDisclosure`(末尾追加、番号不変)、mirror `panel_da_disclosure_from`、`palw_g14_fp_route_families_at_v1`(fence 未満・V5 は従来の `NonPublicEvidence` と byte 同一)、`palw_g14_route_fp_refusal_v1` が新関数を使う。
- `palw_panel_da_disclosure_v1.rs`: 期限・状態(Pending/Disclosed/Defaulted、遅延開示で復活しない)・開示受理・settlement 保留・default の帰結・新しい利用者文面。
- 値(目安)`palw_g14_values_v1` 末尾 4 個、台帳 `docs/governance/parameter-register.md`「2026-10-26 lane PANELDA」。

## 試験(targeted)
`g14_fam_panelda` 7/7、`g14_close_gates` 12/12、`palw_t12_full_activation_16000` 8/8(fleet identity 維持を含む)。lib 内 unit test 3 本は未実行(full lib test build を避けた)。

## 残件(武装の前提)
開示 carrier(version 付き tx object)、fold row と `disclosed_at`、deposit lock/burn、Final・settlement 保留の fold 配線(CERT・F02 duty)、CLOCK lane との結線、実ノード e2e(嘘+全員共謀→開示後に外部者が有罪・slash)。現状の試験は規則レベル。record は空、full_activation 一覧には未登録(pin は統括)。

## ユーザー判断
1. ADR-0206 の採否(紛争なしでも期限で公開される=「期限まで非公開」への縮小)。
2. 目安値 1,040 / 2,000 / 32,768 / 13,107,200 sompi。
