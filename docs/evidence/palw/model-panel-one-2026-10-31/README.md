# 全モデル G14 Panel=1 — 2026-10-31 検証記録

状態: 実装・ローカル検証済み。**配備・有効化は未実施**。移行 fence は現在 `None`。
[RFC-0042](../../../rfc/0042-all-model-g14-panel-one.md) / [ADR-0210](../../../adr/0210-all-model-g14-panel-one.md)。

## 配備候補の基点

稼働中の `2ca68baee2c9` に実装を重ねる。main の `6e31509bb` に含まれる別件の terminal service / operational release 修正は持ち込まない。
過去の状態遷移を変更したバイナリへ同時更新しないためであり、その修正自体の採否の変更ではない。
元の main 基点の実装はローカル branch `codex/g14-model-panel-one-main-base` に保存した。

## 実行結果

[core-regression.log](core-regression.log): **123 passed / 0 failed**。新経路と関連する既存 Panel・R-core・vertex・経済条件・params の回帰試験。

```sh
RUST_MIN_STACK=33554432 RUST_TEST_THREADS=2 \
CARGO_BUILD_JOBS=2 cargo test -p kaspa-consensus-core --lib \
  --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.test.package.kaspa-consensus-core.debug=0' -- \
  model_panel_one palw_panel_v2 rcore_m4_sampled_and_v3_door \
  palw_vertex_v1 palw_weight_cap_v1 palw_user_status_v1 \
  palw_g14_gates_v1 palw_economic_safety_v1 config::params::tests --nocapture
```

このコマンドではコンパイル時間を抑えるため core の最適化を無効にし、非最適化時の大きな既存 fixture に合わせてテストスレッドの stack を 32 MiB にした。
指定なしの非最適化実行では既存の maturity fixture が stack overflow した。protocol の上限や判定を緩めたものではない。

[node-check.log](node-check.log): 稼働中の基点上で `cargo check -p kaspad --features evm --all-targets` **成功**。
文書検査は新規不備 0、既知の未解決参照 43。baseline は変更していない。

## 確認した境界

- commit 自身の anchor では抽選しない。1 個の独立 bond から 1 席を割り当てる。commit 後に登録した bond はその抽選に入れない。
- 移行後に登録された別モデルも同じ経路。旧 claim の 5 席と新 claim の 1 席が同じ fold に混在できる。
- 全 scope の承認、実際の ML-DSA-87 署名・context、署名不正・重複・mask 不正の拒否。
- licence tag 4 / basis 1 / served mask 1、実際の担保予約、Final、delta apply / revert、carriage の cold restore。
- 1 席の lock は全 gain の価格。Final 前後の虚偽承認では lock と既存 S4 action が徴収される。
- Panel=1 Final だけでは Gate C の weight / frontier を得られない。future fence は旧 handshake / kernel policy identity を維持し、params / schedule identity は変わる。

## 未確認

この記録の unit fixture は claim / ledger の境界を試す小さい構成であり、1.5B・8K の実計算、Linux release 実行、fleet の切り替え、実際の公開 TIR court の全手順、実運用の検出率・応答率を測った結果ではない。
Linux 用ビルドと配備検証は別途記録する。これらの pass を配備完了や `SecurityCertified` と読み替えない。
