# PALW algo_id=4 偽造耐性監査 — 61推論・60 seed規模 (2026-08-16)

公開テストネット Track A の最初のゲート。algo_id=5 (Ollama) の定数タグ偽造
(27/27 seedで同一タグ → 約31回のBLAKE2bで偽造可能) を発見したのと**同じ手法**を、
その置き換えである algo_id=4 (`gemm_trace_root` 束縛) に規模を上げて適用した。

- ハーネス: `scripts/misaka-palw-forgery-audit.py` (61推論、~1s/推論 warm、全体~2分)
- 解析: `scripts/misaka-palw-forgery-audit-analyze.py`
- 経路: 実PoW経路と同一 — `palw_pow_seed_v1` (keyed-BLAKE2b をPythonで厳密再現) →
  `palw_pow_prompt_v1` → `palw-worker --mode verify --prompt-stdin --n-predict 128`
- 環境: Metal profile, Qwen3.5-2B-Q4_K_M (palw-unified の pinned worker)

## Seed構成 (60 + 決定性対照1)

| 群 | n | 内容 |
|---|---|---|
| pow | 30 | 実導出 (nonce×20, timestamp×5, pre_pow_hash×5 可変) |
| uniform | 20 | 一様乱数 (決定的PRNG連鎖、再現可能) |
| adv | 10 | 敵対的低エントロピー: 全zero, 全ff, 1bit, counting, ASCII, **1-bitフリップペア** |
| repeat | 1 | pow/nonce0 の完全再実行 (決定性対照) |

## 結果

| 指標 | 値 | 判定 |
|---|---|---|
| 決定性 (同一seed再実行) | タグ完全一致 | PASS |
| **gemm_trace_root 相異** | **60/60** (1770ペア衝突ゼロ) | **PASS** |
| gemm pairwise Hamming | mean 255.9/512 (理想256), min 219 (3.3σ内), max 298 | PASS |
| gemm ビットバランス | 0.5026 (理想 0.5) | PASS |
| 1-bit seedフリップ → gemm | 248–273/512 bit反転 (完全avalanche) | PASS |
| フルタグ (200B) 相異 | 60/60 | PASS |
| identity定数 (model/runtime/scheme/cu) | 4種すべて60実行で定数 | PASS |
| output_commitment 相異 | **55/60 — 重複4組** | 下記参照 |

## 中心的発見: attractorはテキスト層に残存し、gemm層が封じている

output_commitment (出力テキストのみの束縛 = algo_id=5 が依存していた層) の重複:

1. `zero`/`bit0`/`bit255` — 3-way一致 (ほぼゼロのseed hexは同一継続を誘発)
2. `avalanche-base` vs `avalanche-flip1` — **seed 1-bit差で出力テキスト完全一致** (Hamming 0/512)
3. `pow/nonce5` vs `adv/avalanche-flip2` — 無関係seed間の偶然一致
4. `pow/ts+4` vs `uniform/u10` — 同上

一方 gemm_trace_root は**これら全ペアを含む1770ペア全てで相異** (出力テキストが
一致するペアでも 248–273 bit差)。prefill logitsがseed hexに依存するため、テキストが
attractorに落ちてもトレースは落ちない — これが algo 4 が algo 5 の構造的欠陥を
閉じている実測証拠。

**含意**: 出力テキストのみを束縛するいかなる将来設計 (algo 5 系) も、この重複率
(1770ペア中5ペア+1三重) が示す通り復活させてはならない。タグ検証が
gemm_trace_root を含む200Bタグ全体の一致であることが安全性の前提。

## min-entropy について

60サンプルでの衝突ベース評価: 1770ペアで衝突0、ペア間Hammingが二項分布
Binom(512, 0.5) と整合 (mean 255.9, 分散も整合) — 定数タグ欠陥 (H=0, Ollamaの
失敗様式) は1770:1で排除、低エントロピー様式 (H < ~11 bit) も排除。
真の暗号学的min-entropy主張には v2 §12 の 10k seeds が別途必要 (これは
consensus-inert な v2 trace scheme のゲートであり、Track A の公開ゲートは本監査)。

## その他の観測

- decode_tokens は 46–102 で可変 (15値)。`adv/ff` (102) と `adv/aa` (101) のみ
  外れ値 — 反復バイトseedは長い継続を誘発するが、n_predict=128 の予算内。
- operation_schedule_commitment は 15/60 (= トークン数の関数なので期待通り、
  単独では束縛力なし — 束縛はgemm層の仕事)。
- 実行時間 0.87–1.46s/推論 (warm, Metal)。61推論で~2分 — この監査は
  fleet各hostでも安価に再実行できる (cross-host一致測定=Track Aゲート2の入力)。

## 再現

```bash
export PALW_WORKER=<worktree>/target/release/palw-worker
export MISAKA_PALW_GGUF=~/Downloads/misaka-palw-runtime/models/Qwen3.5-2B-Q4_K_M.gguf
python3 scripts/misaka-palw-forgery-audit.py     # 結果は palw_audit_results.jsonl (追記・再開可能)
python3 scripts/misaka-palw-forgery-audit-analyze.py
```

seedは全て決定的に導出されるため、別hostで走らせて jsonl を突き合わせれば
そのままゲート2 (cross-host決定性) の測定になる。
