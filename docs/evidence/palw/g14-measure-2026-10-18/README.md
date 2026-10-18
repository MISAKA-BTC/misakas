# G14 fence の実測 — 決定済みの値・導出式と、測るもの(2026-10-18)

対象モデルは `Misakachain/Huihui-Qwen3.5-2B-abliterated-PALW` だけです(PALWART3、32K、fit 合格)。測定機は Apple M1 Max・10 cores・32 GiB、release ビルドを使います。
この文書は「値・導出式の決定」と「測るものの一覧」です。**実測記録としての pin(`PALW_G14_ARMING_RECORDS_V1`、`PALW_TIR_FRAME_COST_RECORDS_V1`、X3 の policy)と、validation の拒否の解除は、まだ行っていません。** 決定・pin・有効化は別の段階です。

## 1. ユーザーが決めた値・導出式(2026-10-18)

### 検出 policy
- `epsilon = 1/100`(見逃し 1%)。
- 保証する不正の範囲:`d_min_abs = 1`、`d_min_ppm = 100000`。つまり、最低 1 区間かつ全体の 10% 以上が検出できる不正である claim を対象にします。
- 必要な検査数 K は固定値ではありません。claim ごとの N と被覆率 q から、既存の `k_min` 関数で導きます。
  - 例:q = 100%、N = 1,000、D_min = 100 なら K = 43 です。このとき、1 区間だけの不正に対する検出率は 4.3% です。
  - 疎な不正は、全再実行・court・経済的抑止で扱います。
- **q = 100% は本番の値として使いません。** 被覆率は負荷下で実測します。q < 99% のときは、検査区間を増やしても全体の検出率は 99% に届きません。
- TIR の全再実行方式は変えません。

### X3(既存の窓を保つ)
```text
prepare_daa  = window_bind + window_receipt          = 1,200
observe_daa  = window_challenge_at(activation)       = 120
Δ            = cold_fetch + check + localize + carrier + reorg_slack   (どれも実測。0 で代用しない)
resolve_daa  = B_disclose + B_court + Δ              = 4,200 + Δ
hard         = accepted_daa + 5,520 + Δ
```
有効化するときは、対象の `Params` から窓を読み直し、一致することを確かめます。保持費用 `retention_cost_per_daa_sompi` は、測るまで未定のままにします。

### TIR frame
- `maximum_door_daa = 3,120`(既存の責任窓 120 + 3,000 を保つ条件での上限)。これは「そこまでに追及できた」という測定結果ではなく、超えてはいけない上限です。
- 既存の測定 `docs/design/palw/evidence/tir-frame-measure-2026-10-16/` の扱い(小型 fixture の測定であり、本番の武装の証拠ではありません):

| パラメータ | 既存候補 | 扱い |
|---|---:|---|
| `action_parser_steps` | 15,625,804 | 再計測の基準値。実モデル・出荷ビルドで確かめる |
| `action_parser_steps_per_custodian` | 18,575 | 同上 |
| `frame_passes_per_check` | 2 | 維持 |
| `control_bytes` | 14,320 | 維持。出荷版の最大サイズと照合する |
| `inclusion_margin_daa` | 4 | 未確定(ネットワークでの観測が要る) |
| `parallel_bytes_per_daa` | 125,000 | **実効容量としては使わない**。理論上の上限 |
| `acquisition_daa` | 1 | 実モデルでは未確定 |
| `collection_daa` | 8 | inclusion margin とあわせて未確定 |
| `reorg_replay_moves` | 1 | 未確定(V7 で観測する) |

### RFC-0016
- `tilt_k_permille = 1000`(補助の傾斜は off)。これは検出の検査数 K とは別物です。
- calibration 用の値(`sigma = 250`、キュー長 8 など)は、本番値に上げません。

## 2. 測るもの(成果物)

どの成果物にも、次の情報を付けます。
- source SHA、ビルドの条件、モデル・class ID、artifact の同一性
- prompt 長・decode 長、並行数、機材、試行回数、生ログ、導出式

あわせて、次の決まりを守ります。
- 観測した最大値・設計上の上限・採用した余裕を分けて書く。
- 平均値を最大負荷の保証として扱わない。
- 逐次の試験と並列の負荷測定を区別する。

| ファイル | 中身 | 決まる値 |
|---|---|---|
| `detection-measurements.json` | class・プロファイルごとの区間数 N、opening bytes、区間と全再実行の時間、負荷下の被覆率、裁定の結果、実際に徴収した担保(funnel のカウンタではなく、担保の減額と精算の結果と照合する) | 検出 policy の資源・期限・K・経済の入力 |
| `court-measurements.json` | 4 種類の処理(一撃裁定、court を開く、dissection の move、close)ごとの `class_ns`・`class_bytes`・`class_memory`、block の割当、競合下で裁定が終わる時刻 | 既存の `policy()` で `op_work`・`block_budget`・`memory_per_byte`。`rho_sompi` と block 時間のうち court に渡す割合は方針なので、ユーザーの判断事項 |
| TIR frame の再測定 | 実モデルでの frame の操作、custodian 数ごとの増え方、搬送の遅延、競合下の bytes/DAA、復旧の量 | frame の費用と期限 |
| `x3-measurements.json` | 5 つの時間項を重ならないように分けた記録、保持する量と期間 | Δ、`resolve_daa`、保持の価格 |
| `claim-loads.json` | class ごとの `da_bytes_per_claim`・`court_units_per_claim`、プロファイルとの対応 | `PALW_RFC0016_CLAIM_LOADS_V1` |
| RFC-0016 の `events.jsonl`・`summary.json` | `rfc0016_sigma_k_measure.py` で候補値を振った結果。この Mac の能力を class ごとに重複して数えない | RFC-0016 の候補の選択 |

V1〜V7・D-01〜D-18・受入試験 A〜G は、パラメータの代わりにはなりません。採用した値で、必要な挙動が成り立つことを示す証拠です。

## 3. 適用範囲の限界

Huihui-2B(32K)での測定は、そのプロファイルの証拠です。より大きなモデル、より長い context、t12 で最も遅い対象ノードに、そのまま広げません。正式な記録に入れるのは、t12 が許容する最大のプロファイルと最も遅いノードに当てはまる証拠だけです。
