# 争点の閉包 — 照合と 32K 実モデルの観測(2026-10-19)

測定機は Apple M1 Max・32 GiB、ビルドは release、`--features node` です。測定と試験のコードだけを使っていて、合意の規則は変えていません。
記録の扱いは次のとおりです。観測した最大(observed_max)は、安全な上限ではありません。導出した上限(derived_upper)と、上限で覆えていない条件は、別に書きます。

## 1. 照合(実際の評価器との一致)— `closure_containment_on_fixture_classes`

- fixture の class で、争点になる commit と checkpoint の leaf ごとに、導出した閉包が、実際の評価器が読む step leaf を含むことを確かめました。あわせて、評価器が読む inventory の leaf が、閉包の params の leaf(正確な数と、Gather の行の上限)を超えないことも確かめました。
- 結果:試行 2,411 回、スキップ 172 回、観測 leaf 14,006、導出 leaf の合計 64,362、**違反 0 件**。
- 演算の種類は 15 です:Add、Cast、Clamp、Compare、Gather(data param)、Gather(index)、HistAppend、HistAppend(row)、HistAppend(tile)、MatMul、Mul、ReduceSum、Reshape、State(checkpoint)、Transpose。
- スキップした 172 回は、HistTile の leaf と、書き込み元のない State です。HistTile の値はそれ自体が行の commit なので、HistTile への争点は、その行の commit への争点に帰着します。
- 記録:`closure-containment-1019.json`。
- **覆えていない条件:** 照合は fixture の class だけで行いました。全ての合法な job・争点・境界についての証明は、まだ書いていません(導出は窓の規則と閉包の構造による)。

## 2. 32K の実モデル(観測。上限ではない)— `closure_bound_on_a_real_class`

- 対象:Huihui-Qwen3.5-2B の 32K の class(`declare-layout` 済みの PALWTIR1)。
- 方法:位置 32,767 で、GDN・attention・post の commit node ごとに、最初・中間・最後の tile を起点にして、126 個をサンプルしました。全 tile の最大ではありません。
- 観測した最大:**41,944,082 B**(occ 24・block 2・node 236・tile 0)。

| 演算の種類 | 観測した最大 bytes |
|---|---:|
| HistAppend(tile) | 41,902,080 |
| Gather(data param) | 4,194,304 |
| MatMul | 1,048,576 |
| Reshape | 85,440 |
| Mul | 57,344 |
| HistAppend(row) | 39,680 |
| Slice | 33,792 |
| Clamp / State(checkpoint) | 32,768 |
| Gather(data) | 24,576 |
| ほか | 1,280 以下 |

- 記録:`closure-real-32k-1019.json`。

## 3. わかったこと

- 閉包のほぼ全部(99.9%)が、attention の履歴の tile です。争点の位置 p で、court はそれまでの履歴の tile を全部読みます。そのため、この項は p に比例して増えます。
- 2M で同じ読み方をすると、この項だけで約 64 倍の約 2.68 GB になります(構造からの見積もりで、測定ではありません)。
- 以上から、ADR-0191 の前提が裏づけられました。attention の争点は、履歴の tile の reduction を二分探索で 1 tile まで絞ってから裁く必要があります。最後の裁定が読むのは、1 tile 分(32K で約 41 KB)と、二分探索の中間の値の commitment になります。
- Gather(params)の 4.2 MB と MatMul の 1 MB は、ctx によらない項です。これを最後の block の大きさにどう収めるかは、ADR-0191 の op の分解で扱います。

## 4. 残り

- 全ての合法な job についての、閉包の上限の証明。
- 履歴の tile を二分探索する裁定の実装と、その照合の試験。
- 2M の境界での、縮小した fixture による試験。
