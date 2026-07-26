# PALW per-epoch beacon-seed HISTORY — design memo (ADR-0045 D3-a)

- **Status:** Design memo (no code). Fulfils ADR-0045 D3 precondition (a) for PCPB wiring.
- **Date:** 2026-07-27
- **Inputs:** ADR-0045 D3; ADR-0044 harness の実測挙動(`palw_lifecycle_e2e.rs` が beacon
  commit/reveal → Healthy seed advance → carry を受理経路で駆動した際の観測)

## 今日の「seed の真実」は 3 系統あり、どれも PCPB の要求を満たさない

1. **Header 刻印**(`header.palw_beacon_seed`)— 各ブロックが自分の R_E を運ぶ。過去 epoch の seed は
   `palw_epoch_seed_at` の buried selected-parent 降下で解決できるが、`map_while(ok)` は **pruned
   history で打ち切って fail-closed** する。つまり pruning point より深い epoch の seed は解決不能。
2. **Per-block state 行**(`PalwBeaconStateV1`、block-keyed)— R_E の運搬体。pruning pass が
   per-block 行として削除し、frontier は **pruning point の 1 行だけ**を持ち越す。
3. **Epoch accumulator**(`PalwBeaconAccumViewV1`)— commit/reveal の集積。`retain_future_of` が
   「もはや有効な commit/reveal を受けられない過去 epoch」を**設計として捨てる**(bounded 化)。
   これは seed の履歴ではなく seed の**材料**であり、保持もされない。

PCPB(遅延 provider 束縛)は mint 検証時に `derive_b(R_{e}, …)` を **ticket が束ねられた過去 epoch の
seed** に対して再計算する必要がある。ticket の検証可能 lookback は batch lifecycle 全長に及ぶため、
上の 3 系統では「pruned 済み・retain 済みの epoch」の seed が正当に必要になる場面が存在する —
これが ADR-0040 §5.14 が配線を止めた理由の (a) である。

## 設計: bounded per-epoch seed history store

**新 store `PalwBeaconSeedHistory`(epoch → Hash64 = R_E)**。

- **Writer 座標:** epoch 境界を跨ぐ**最初の chain block の virtual commit**。値は当該ブロックの
  header 刻印と同一の共有導出(`derive_palw_beacon_state_core` の結果、c==v 済み)から取り、
  同一 WriteBatch で書く。fork-local 性: selected chain の巻き戻しでは epoch 境界ブロックの
  再 resolve が同じ座標で上書きする(per-epoch 値は「その epoch を最初に閉じた selected chain」の
  関数であり、reorg 後は新 chain の境界 commit が正)。
- **保持規律(ADR-0044 と同じ discipline):** 窓は
  `ceil(paid_work_walk_bound_daa / epoch_len) + palw_audit_epoch_inclusion_window_epochs + 2`
  epoch(= batch lifecycle 全長 + 監査包含窓 + 境界余裕)。これは harness が pin した
  `walk_bound < pruning_depth` と同じ「検証 lookback は必ず retained 領域に収まる」規律の
  epoch 版で、store を chain 長と独立に bounded にする。窓外の削除は pruning pass の
  per-epoch sweep(`sweep` は per-block delete と同じ batch)で行う。
- **Pruning snapshot への carry:** `PalwPruningPointSnapshotPayloadV1` に
  `beacon_seed_history: Vec<(u64, Hash64)>`(昇順・重複なし・窓内のみ)を追加し、writer の
  coherence 検査に (i) 窓内条件、(ii) epoch 単調、(iii) **pp header の刻印との整合**
  (`history[pp_epoch] == pp.palw_beacon_seed`)を足す。joiner はこれを import して
  「pruned 領域の epoch seed」を検証可能にする — frontier の active_nullifiers と同型の役割。
  ⚠ ADR-0044 harness が実証した通り、snapshot writer の version/dup 検査は **`new()` で作った
  canonical 行**を前提とする。新 rows も `Default` を経由してはならない(3 件の既知バグ類型)。
- **読み出し API(fail-closed):** `beacon_seed_at(epoch) -> Option<Hash64>`。窓外は None =
  PCPB 検証は fail-closed(ticket 拒否)。これは §5.17.3 の bounded-window 規律と同じ形で、
  「honest ticket は常に窓内」を admission パラメータが保証する。

## 不変量とテスト(実装時)

1. **c==v:** 履歴行 == その epoch を閉じた chain block の header 刻印(equality test)。
2. **carry:** DegradedGrace の epoch でも行は書かれる(R_E = carried R_{E-1};ゼロではない —
   harness の実測: Healthy 1 回で以後の carry は非ゼロを保つ)。
3. **bounded:** 窓サイズ超の epoch 行が存在しない(store 監査、ADR-0044 harness へ 1 assert 追加)。
4. **snapshot 整合:** payload の履歴 == store の窓内履歴(1c と同型の payload/store 等価 assert)。
5. **reorg:** epoch 境界跨ぎの sink reorg 後、履歴行が新 selected chain の導出と一致
   (`palw_algo4_sink_reorg_…` の隣に配置)。

## 範囲外(このメモでは決めない)

- PCPB 配線そのもの(ADR-0045 D3 の (b) 全 clause 同時有効・(c) G12 e2e)。
- 監査 seed resolver(`palw_epoch_seed_at`)の置き換え — 現行の header-walk は retained 領域では
  正しく、履歴 store は **pruned 領域への拡張**として追加する(置換ではない)。
