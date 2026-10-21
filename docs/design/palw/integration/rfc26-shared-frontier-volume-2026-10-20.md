# RFC-0026 F03 — 一経路と全共有分岐の byte 数を区別する

**状態:** 調査・条件付き導出。新 profile の採択・本番性能の測定・G14 の完成を意味しない。[A/A 詳細契約案](rfc26-service-shared-frontier-contract-2026-10-20.md) §5 の許容集合を閉じる際の確認事項。

## 対象と前提

対象は現行 `PalwTirPublicResponseV1::StepNode` の Borsh codec と、現行の root からの canonical な段階下降。`N` は original header の `step_leaf_count` であり、**context 長・token 数・パラメーター数ではない**。2M context の `N` を 2M としていない。

以下の下限が適用されるのは、全 canonical branch の末端 frontier が合法な共有開示要求として受理され、その要求を現行 codec で全て提供する責任を引き受ける場合だけ。将来 profile の合法要求集合、再利用、受付条件、最大敵対負荷がこの前提を満たすかは別に確定する。申立てを恣意的に非適格として外部者の経路を消す回避は採らない。

## ソースと確認

`consensus/core/src/palw_tir_court_v1.rs`:

- `PALW_TIR_STEP_NODE_DEPTH_V1 = 10`。
- `palw_tir_step_tree_height_v1` が original leaf 数から高さを導出する。
- `palw_tir_step_node_frontier_v1` が各回答の `(下位 level, first, end)` を導出する。
- 最後の段の全 frontier は、端数 node を含め、level 0 の original leaf nodes を重複・欠落なく分割する。

`consensus/core/src/palw_tir_public_source_v1.rs` の `StepNode` 回答は `frontier: Vec<Hash64>` と `siblings: Vec<Hash64>`。実 codec は各 frontier hash を 64 bytes で literal に送る。空の siblings を用いた shape-only fixture で `1 + 4 + 64k + 4` bytes と確認した。これは認証済み回答やモデル計算結果を作ったものではない。

release 試験 `palw_court_shared_frontier_volume` は上の geometry を `N = 2, 3, 1,024, 1,025, 65,537, 2,000,000` で確認し、codec を `k = 1, 2, 1,024` で serialize して確認した。全クラス・全 `u64` 入力の網羅試験ではない。

## 条件付きの payload 下限

既存 codec で上の全末端 frontier を 1 回ずつ送ると、frontier 値だけで `64N` bytes。siblings、vector prefix、制御、署名、step preimage、operand、再取得、IO、検証 work は含めていない。multiproof による重複 sibling の削減を、この literal frontier の全量まで削減したと扱わない。

| 条件付きの例 | 値 |
| --- | ---: |
| synthetic original leaf 数 `N` | 2,000,000 |
| `64N`（frontier 値だけ） | 128,000,000 B |
| 仮定する単一 claim 用の供給 `B` | 12,500 B/DAA |
| `ceil(64N / B)` | 10,240 DAA |
| original hard deadline の予算 | 10,000 DAA |

この `B` は RFC の例と同じ条件を置いた算術入力で、承認済みの capacity・測定された保証下限ではない。この表から実モデルの不可能性や実ネットワークの性能を断定しない。全量の codec 集計、実モデルの `N`、合法な全要求集合、実供給、物理的な block capacity は未被覆。

## 実装への帰結

1. 一人が一経路を降りる door の式と、全 owner 共謀下で共有開示を提供する資源予約を分ける。小さい packet fixture で scheduler が通っても F03 は閉じない。
2. 現行 owned plan は `fresh` の資源費用に `N` を含む。資本・work の上限を持つことと、全分岐が期限内に搬送されることは別。予約 adapter は profile の閉じた集合を実際に導出し、byte/work/IO/control/保持と残余期限を検査する。
3. 全量が収まると確認できるまで `OneOutsiderCapacityIncomplete` を解除しない。モデルの除外、上限の引き上げ、unknown 項のゼロ化を達成手段にしない。
4. 未達なら、その負荷で G14 未達と報告する。SC64、現行 4 分岐、16 分岐候補、先読み profile の数字を混ぜない。数値 fold 検査は維持する。

## 追加調査: 旧 main snapshot の preparation clock の起点

再割当の native fixture で、既存 owner acquisition を待ってから作った packet 区間が original `+10,000` を越え、`InvalidInterval` で拒否された。hard を引き上げず、この拒否を回帰試験に残した。以下は **旧 main `0f808b81d` の `PalwStagedPublicCourtConfigV1::network_v1()` を使う quote に限定した静的導出**。現統合候補の式ではない。CPU・ネットワーク・実モデルの測定値でも、全ての future profile の下界でもない。

- `palw_rendering_resources_v1::network_v1`: `acquisition_daa = 4,096`、`inclusion_margin_daa = 4`、`reorg_replay_moves = 64`。
- `palw_tir_public_owned_v1::work_allocation_from_lanes_v1` は `maximum_restarts` をその `reorg_replay_moves` から取る。`preparation_lanes_v1` も一致を検査して同じ値を持つ。
- `derive_tir_public_preparation_plan_v1` は `one_pass = max(step_leaf_count, positions) + decode×logit_tiles + ceil(prefill/1024) + ceil(decode/1024) + 32` とし、`passes = maximum_restarts + 1`、`maximum_slices_per_actor = one_pass×passes + durable_slices` を求める。durable 項も非負。
- 同関数の quoted clock は `preparation_daa = acquisition_daa + (maximum_slices_per_actor + 1)×inclusion_margin_daa`。preparer は並行でも、これは一人分の serial な最悪枠であり、custodian 数で割る定義ではない。

従って、この config に限り `maximum_slices_per_actor >= 32×65 = 2,080`、`preparation_daa >= 4,096 + 2,081×4 = 12,420 DAA`。leaf・positions・disk など、それ以外の非負項を下界式から落としても `10,000` と `3,120` を越える。これは **設定が算出する門の時刻** の下界であり、実 CPU がその時間を必要とするという意味ではない。

`palw_tir_public_allocation_v1::derive_tir_public_allocation_v1` が `original_acceptance + preparation_daa` を `preparation_until_daa` として固定する。`PalwTirPublicOwnedV1::bind_preparation_v1` は owner の `reserved_daa < preparation_until_daa` を拒否する。よって、この legacy quote を RFC-0026 の original `+10,000` にそのまま接続して、owner 経路が成立したとは表示できない。最遅 owner の demand/response/localization/terminal はこの後であり、ここではその費用すら下界に足していない。

再割当 journal の成功 fixture は同じ original frame の準備区間で book を動かすもので、owner 経路の成功を代替しない。次に必要なのは、actual native profile がどの config を固定するかの区別、logical walk / prepared publication / WorkDebit control の対応、canonical chain clock と再実行予算の対応、公開 frame の局所化による資源削減の検証である。clock の差し替えや batch 化は未採択の別 version/profile として検証し、既存の signed plan・ID の意味を変更しない。再実行数・上限・費用を根拠なく削ったり、扱うモデルを減らして達成したことにしない。

## 現統合候補との対応 — 2026-10-21

進捗記録では既に旧mainとX3の違いを区別していたが、この単独調査文書にはsource commitの明示が不足していた。上の旧式・値は履歴として保持し、現候補 `0988d4279` へ適用しない。[旧/現sourceのhashと監査範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/preparation-source-scope-audit.json)。

現行 `derive_tir_public_preparation_plan_v1` は `passes = maximum_restarts + 1`、`maximum_slices_per_actor = passes × PALW_TIR_FRAME_PREPARATION_DEBITS_PER_PASS_V1` とし、定数は1。grant数を論理leaf数や16KiB durable slice数と同一視しない。workの総量、公開・再読込・IO、各actorのlane限度、restart限度は別途保持される。grantをまとめたことはその計算や保持を無料にすることではない。

現行 `tir_public_preparation_door_v1` の導出は次のとおり。`U`はcustodian upper、`R`はreorg replay moves、`M`はinclusion margin、`A`はacquisition DAA、`C`はcontrol bytes、`B`はparallel bytes/DAA。

```
S = (R + 1) × 1 + 1  // passごとのdebitと最後のReady
preparation_daa = A + S×M + ceil(U×S×C / B)
```

これはclock.checkとchecked arithmeticを通るprofileに対する現コードの式。上の旧network入力のうちA=4,096、R=64、M=4だけを代入しても、現式の既知部分は4,360であり、旧式の12,420ではない。U/C/Bは当該profileの実際の値が必要で、未知の並列項をゼロにして完成時間を作らない。この代入は本番profileの採択でも実機の時間測定でもない。準備後の応答・局所化・witness・裁定はさらに別の項である。

全量のsource公開量の条件付き64N下限は、上記の全末端要求を合法に引き受けるという前提の下で別途残る。1passのgrant集約やserial/parallelの区別だけでは、all-owner共謀下の全要求集合、実供給、fresh signerの可用性、元の完全な予約/料金、3,120/10,000の全経路は証明されない。[現行controlと予約対象の接続監査](rfc26-original-service-admission-map-2026-10-21.md)へ進む。
