# lane PANELDA 報告 — PanelDa(非公開テキスト)の G14 経路

branch `claude/g14-fam-panelda`(基点 d8aded6de)。画像(FP V5)は実装しない(ユーザー 2026-10-26)。

## 決定
**ADR-0206 は accepted(2026-10-26 ユーザー決定)**: 期限付き必須公開、「期限まで非公開」への縮小を含めて採用。値は目安のまま(1,040 / 2,000 / 32,768 / 13,107,200 sompi)。

## 方式
`prompt_token_ids_hash` で束縛 → service 中は seat のみ → `a+1,040` までに誰でも ids を公開(hash が中身を固定)→ 外部者に notice 窓 2,080 DAA → 公開 FP 経路(③〜⑥)で告発。未開示は default: claim void・deposit burn・計算の有罪/計算 bond slash なし。開示まで報酬・credit なし(Final floor)。

## 実装(すべて休眠 fence `palw_panel_da_disclosure_v1` の下、全 preset `None`)
1. **carrier**: `PalwConsensusObjectV2::PanelDaPromptDisclosedV1`(tag 233、署名不要)。kind→fence 表・A-2 行・割当(233、**統括の確認要**)・wire pin・may_ride(1〜32,768 ids)・processor/kaspad/rpc の網羅 arm。fence 未満は A-2 で unjudged。
2. **fold row**: `panel_da_disclosures`(`PalwPanelDaDisclosureRowV1` version 1)、delta 236、carriage tail 0xE8、root `panel_da_disclosures/v1`(いずれも非空時のみ)、native inventory(手で追記、generator は既存 drift のため未使用)。
3. **deposit**: 受理時に producer bond の空き担保で lock(`palw_bond_committed_v1` に計上)、開示で返却、default で burn(`burn_registration_fee`、conviction 記録なし)。
4. **保留**: `palw_claim_final_floor_v1` に `deadline+1`。`finalize_claim` 冒頭で未開示なら `PanelDaDisclosureDefault`(Borsh 140)で void。
5. **経路接続**: 開示が claim の prompt row(TIR frame の material horizon まで)を書き、外部者の replay 入力になる。CLOCK(`palw_court_clock_v2`)を fence の前提に(validation)、期限は notice 窓の内側。
6. **価格**(central、要確認): derived-work 下で PanelDa は ids 不在で拒否されていた(ADR-0145 §6)。governed PanelDa は prompt 全体を既払い扱い(prefill 0、ADR-0082 D10 と同じ読み)で価格付け — 過少払いのみで二重払いはない。answer accounting 武装時は従来どおり拒否。
7. validation(2026-10-27 改): 順序(`palw_panel_da`・`palw_court_clock_v2` が同じか下)と外部者の窓の検査はそのまま。武装の拒否は配線が欠けた場合だけ(tag 233 の kind と fence、delta 236、void reason 140、deposit、bundle mirror)。**DAA 17,000 で slice・6 family と一緒に武装(ユーザー決定 2026-10-27)**。

## 試験
- production fold(T-N1 harness、fence は TEST-ONLY で DAA 0): 正直+開示 → Final・支払い・担保不変 / 嘘+全 owner 共謀 → 開示後に外部 bond 1 つが有罪・producer と Valid seat を slash / 未開示 → `PanelDaDisclosureDefault`、producer は deposit 分だけ減、offence なし、seat 不変、支払いなし / fence 境界(fence 未満は従来の拒否、row なしの開示は拒否、PublicDa は row なし)— 4/4 pass、既存 T-N1・tir_fp_lane も pass。
- `g14_fam_panelda` 7/7、`g14_close_gates` 12/12、`palw_t12_full_activation_16000` 8/8(fleet identity)。
- 本 lane 以外の既存失敗(基点で未照合): a2u の tag 245/255 行と pin drift、`routing_v2` 4 件、`liveness_lifecycle`、`public_filer_targeting`、`t12_a2u_new_kinds_uniform`(tag 201–204 の fence 未 arm で停止、本 kind まで到達せず)、`palw_court_shared_tir_state_v1` と `p2p-flows` のコンパイルエラー、worker の build script。

## 残件
- **実ノード E2E(mempool → template → processor)は waiver(ユーザー決定 2026-10-27: drill 省略、未実施項目は waiver)**。証拠は fold 試験まで。
- tag 233 の割当確認、`full_activation_v1.rs` への登録と record(統括)。

## 衝突確認(2026-10-27、統合 branch cd54005fb を grep)
tag 233・delta 236・void reason 140・tail 0xE8 は本 lane 以外で未使用(object tag 140 は別 enum)。価格(prefill 0)はユーザー承認済み 2026-10-27。
