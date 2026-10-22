# 2026-10-22 fence — DAA 16,000 は武装しない

ユーザーは pin のあと fence と公開まで進めるよう指示した。公開するのはこの判断と、pin した値である。`PALW_T12_FULL_ACTIVATION_DAA` は `None` のままである。

対象の一覧と、あとで高さを入れるときの手順は [r7-fence-16000.md](../g14-parallel-2026-10-22/r7-fence-16000.md)。高さだけを `Some(16_000)` にすると、record の無い公開 testnet-12 は validation が拒否する（`armed_on_public_testnet_12_without_records_it_is_refused`）。

## 空のままにした一覧

| 一覧 | 状態 |
| --- | --- |
| `PALW_G14_ARMING_RECORDS_V1` | 空。gated fence は、要件 1 件につき 1 つの非零 evidence digest と、実行した binary の source commit と、review digest が要る |
| `PALW_RFC0016_MEASURED_RECORDS_V1` | 空 |
| `PALW_RFC0016_ARMING_RECORDS_V1` | 空 |
| `PALW_RFC0608_MEASURED_RECORDS_V1` | 空 |
| `PALW_TIR_FRAME_COST_RECORDS_V1` | 空 |
| `approved_panel_beacon_policies_v1` | 空。目安の k / D / delay / window / retry は承認ではない |
| `contrib/t12-deploy-kit/fleet.env.example` の `FULL_ACTIVATION_FLAG_DAY_DAA` | 空のまま |

要件の全文は `consensus/core/src/palw_g14_gates_v1.rs` の `PALW_*_ARMING_REQUIREMENTS_V1` にある。D2、X3、N4、route required、reward liability、legacy filer、legacy held DA、court capacity は、いずれも node E2E と出荷 binary の跨ぎ drill を要件に含む。この pin では、その suite・E2E・drill を実行していない。digest を目安や文書の hash で埋めてはいない。

## 値があっても武装できないもの

- X3 の要件 2 は、最遅の対応ハードウェアと実際の保持価格での measured policy である。P3 の Δ と `c_retain` は目安で、Mac13,1（M1 Max、最低要件ハードウェアではない）からの保守的な換算である。
- D2 は、予約・料金の移動・slash の配線が無い。slash 定数 3 MSK を適用する fold は無い。lane P3 は武装しないよう記録している。
- `palw_bond_budget_v1`、`palw_model_bond_allocation_v1`、`palw_material_income_v3` は 16,000 の一覧に無く、値以外の理由で validator が拒否する。これらが無いと settlement once、model tier、monetary material は載らない。
- V12-a の `block_opportunity = true` と RFC-0016 V+ は同時に武装できない。rights epoch 720 は t12 の bundle epoch 1,000 と組めない。[pin-2026-10-22.md](pin-2026-10-22.md)。

## 高さを入れるときに一緒に要る変更

record が要件どおりに揃い、上の同時武装の拒否がコードで解けたあと、1 つの変更で次を行う。

- `PALW_T12_FULL_ACTIVATION_DAA = Some(16_000)`
- `FULL_ACTIVATION_FLAG_DAY_DAA=16000`
- `== None` を見ている試験の更新と、出荷 params / schedule id の再 pin

この commit はその変更ではない。
