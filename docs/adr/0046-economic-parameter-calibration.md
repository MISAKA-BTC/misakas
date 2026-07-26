# ADR-0046 — 非ゼロ経済パラメータの実測較正: 手順と凍結条件

- **Status:** Accepted(較正手順と凍結条件の確定。数値の凍結自体は実測後)
- **Date:** 2026-07-26
- **Consumes:** ADR-0043(stamp ramp と sibling bound の併存)、ADR-0045(D4: fraud 前提)、
  qwen-8.0 `docs/operator-runbook-calibration-testnet.md`

## Context

public/value 有効化に向け、現在 INERT/placeholder な経済パラメータ群に実測由来の非ゼロ値が要る:

- `PalwSpamParams`(stamp ramp 8-band、window、base_stamp_bits)— 候補 `PUBLIC_REGENESIS_CANDIDATE`
- provider/leaf bond 床(`min_provider_bond_sompi` 10 MSK、`min_leaf_bond_sompi`)
- DA challenge 経済(challenger bond、`max_challenges_per_bond_per_epoch` 4、retention 2,000、
  response window 200)
- slash 量(scheduler bond slash、DA timeout slash)
- replica premium π(現在 neutral 10000 bps に pin)
- `palw_compute_work_scale`(weight — Stage-A では 0)

これらは相互依存する(例: bond 床は slash 量の意味を決め、slash は fraud 抑止力を決め、
premium は provider 参入採算を決める)。根拠のない数値の凍結は「調整済みに見える未較正」になる。

2026-07-25/26 の DA-real mint 実測で、較正の入力になる実データが初めて存在する:
1 leaf batch の全 vertical のブロック数・手数料・時間(たとえば certificate tx fee 293,437 sompi、
DA challenge/response 各 ~360k sompi、mint 窓 6 epoch、provider payout 79,299,440 sompi/leaf)。

## Decision

1. **較正は 3 レーンに分けて行う**(単位が異なるため):
   - **L1 スパム経済**(stamp/bits): 多機 flood 実測(ADR-0043 の再実測と同一キャンペーン)で
     「正直な負荷の p99 × 安全係数 < 攻撃の限界費用」を満たす最小 ramp を選ぶ。
   - **L2 担保経済**(bond/slash): 攻撃利得の上界から導出する。規律:
     `slash ≥ κ × (1 攻撃で得られる期待報酬)`、κ ≥ 3。現在の実測では
     1 leaf の provider 報酬 ≈ 0.79 MSK に対し bond 床 10 MSK は κ ≈ 12 で十分保守的 —
     **bond 床 10 MSK は暫定凍結**し、報酬スケール変更時に再計算する。
   - **L3 報酬経済**(premium π、weight): **ADR-0045 D4 により fraud 配線が前提。**
     それまで π = neutral(10000)、weight = 0 を維持する(現状維持を明示的決定に昇格)。
2. **較正 testnet は ADR-0041 の staging-mainnet と同一 identity で行う**(別網を立てない)。
   qwen-8.0 の calibration runbook の手順をそのまま staging へ向ける。
3. **凍結の形式**: 各パラメータは「値 + 実測 evidence への参照 + 再較正トリガ条件」を
   三つ組で `docs/econ-parameters-frozen.md`(新設)に記録して初めて凍結とみなす。
   evidence の無い値は preset に書かれていても「未較正」と label する。

## Consequences

- L1 は外部(多機)、L3 は ADR-0045 依存。**in-session で閉じられるのは L2 の暫定凍結と
  凍結形式の整備のみ**。
- 2026-07-25/26 実測値が最初の evidence 群になる(certificate/DA carrier 手数料、mint 窓、
  payout 分配)。これらは既に qwen-8.0 evidence/ と runbook に記録済み。
- staging-mainnet(ADR-0048)の soak が L1/L2 の再実測の場になる。

## Definition of done

- [ ] `docs/econ-parameters-frozen.md`(値+evidence+再較正トリガの三つ組台帳)新設
- [x] L2 bond 床 10 MSK の暫定凍結(κ≈12 の根拠つき — 本 ADR)
- [ ] L1 flood キャンペーン(ADR-0043 と同一)→ stamp ramp 凍結(外部)
- [ ] L3: fraud 配線後に π/weight を較正(ADR-0045 依存)
