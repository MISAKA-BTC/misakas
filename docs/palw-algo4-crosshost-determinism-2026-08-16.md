# PALW algo_id=4 PoW経路 cross-host決定性 — 4 host × 61 seed (2026-08-16)

> **This describes the WITHDRAWN float lane, and no longer applies to any network.** The CPU
> determinism class exists because llama.cpp ships hand-written per-ISA kernels whose reductions sum
> in different orders — a real property of that runtime. The execution family replaced it
> (ADR-0053): pinned integer arithmetic in this tree's own Rust, with **no `target_arch` branch on
> the execution path** and `runtime_class_id` left at zero, because the integer family's identity is
> its graph and not its host. **There is no CPU class today, and arm and x86 hosts are not
> separated** — for verifiers or producers. Kept as the record of what the float lane cost and of
> why the network left it. See `testnet11-node-operator.md` §2.


公開テストネット Track A ゲート2。ゲート1の偽造耐性監査ハーネス
(`scripts/misaka-palw-forgery-audit.py`, seed決定的導出) を **live t10 の4 host全部**で
そのまま実行し、v1 PoW経路 (`--mode verify`) のタグを label 毎に突き合わせた。
v2 trace のクラス校正 (2026-08-15, 4/4一致) は済んでいたが、**PoW経路そのものの
4 host一致はこれが初測定**。

## 実行環境 (preflight で全一致を確認してから実行)

| host | label | worker sha | GGUF sha | manifest行 sha |
|---|---|---|---|---|
| A 160.16.131.119 | broadwell-8c | 2bd857f8… | aaf42c8b… | 14f5628d… |
| B 95.111.236.186 | epyc-6c | 同上 | 同上 | 同上 |
| C 5.104.81.23 | epyc-8c-c | 同上 | 同上 | 同上 |
| ibm 169.58.39.220 | epyc-8c-d | 同上 | 同上 | 同上 |

preflight で worker バイナリ・GGUF・`--mode manifest` 全文が byte 一致していることを
先に固定 — **以後のタグ差異は「設定ドリフト」ではなく「実算術の分岐」としてしか
読めない状態**にしてから測った (Ollama期の偽critical教訓)。

## 結果 — PASS

- **305/305 一致**: 61 label × 5 タグフィールド (output_commitment / gemm_trace_root /
  operation_schedule_commitment / prefill_tokens / decode_tokens) が4 host間で byte 一致。
- 正規化 digest (label順ソート・5フィールド canonical JSON の sha256):
  **4 host すべて `311d7eab15ddf8ca…`** — この1行が同一性の証跡。
  ローカル Metal は `ca7a593da0596c00…` (クラス分離、期待通り)。
- fleet 上の決定性対照 (nonce0 再実行) も一致。エラー 0。
- identity フィールド: x86 4 host で7種すべて uniform。Metal と比べると
  `runtime_class_id` / `runtime_manifest_hash` / `shape_profile_id` の3つだけが分かれ、
  `model_profile_id` / `trace_scheme_id` / `cu_ruleset_id` / `schema` は一致 —
  分かれたのは「ランタイムクラスを名指しするフィールドだけ」で、設計通りの切れ方。

## クラス分離の定量 (x86 vs Metal, 61 label)

| 層 | 一致 | 含意 |
|---|---|---|
| decode_tokens | 61/61 | 長さは常に一致 |
| output_commitment | 47/61 | **14 label はテキスト自体が分岐** |
| gemm_trace_root | **0/61** | trace 層は完全分離 |

v2 での単発観測「クラス跨ぎは答えを再現して受領書を否認する」の定量版。さらに
14/61 は答え(トークン列)自体も分岐する — クラス跨ぎ検証は受領書どころか答えも
一致しない場合がある。**公開ネットは単一決定性クラスの pin が前提**、の実測根拠。

x86 fleet データ上の distinctness はゲート1 (Metal) と同型:
gemm 60/60 相異 / フルタグ 60/60 / output **51/60** (テキストattractorはCPU実装でも
実在し、Metalの55/60より多い9重複 — gemm層が全ペアを救う構図は不変)。

## 検証コスト実測 (ゲート3の入力)

| host | s/推論 (min/mean/max) |
|---|---|
| A broadwell-8c | 6.9 / 7.8 / 19.2 |
| B epyc-6c | 4.9 / 6.1 / 18.4 |
| C epyc-8c-c | 11.3 / 13.6 / 19.6 |
| ibm epyc-8c-d | 10.2 / 15.7 / 32.1 |
| (参考) M4 Metal | 0.9 / 1.0 / 1.5 |

fleet の1ヘッダ検証コスト ≈ 6–16s。T=120s ブロックで検証負荷 5–13% — 余裕は
あるが、host によって3倍近い開きがある (ページキャッシュ冷温で wall-clock は
さらに変動)。IBD追いつき速度の床は最遅 host が決める。

## 運用上の教訓 (criticalを出さずに測るための手順)

1. **B (epyc-6c) は常時 swap 6.4GB+/load 12 の慢性圧迫 host** (kaspad 9GB/11GB)。
   無防備に走らせると過去のOOM sweep (misakastake表示死) を再演する。今回:
   `systemd-run -p MemoryMax=2200M -p MemorySwapMax=0 -p CPUWeight=15-20` の
   transient service で包み、**死ぬのは監査側**に固定。初回 run で swap +600MB を
   観測した時点で即停止 (44/61 は保持、ハーネスは追記再開可能)、他 host 完走後に
   watchdog (60s毎 swap 監視、+400MB で自動 abort) 付きで残り17を回収 —
   2回目は swap 増分 **-3MB** で完走。
2. 実行前 canary: 全 host の kaspad etime/rss を記録し、完走後に再起動が
   ないことを確認 (今回4/4無傷)。
3. GGUF は `/tmp` 置きなので reboot で消える — preflight の sha 照合が
   「消えて別物を再取得した」事故も同時に検出する。

## 再現

```bash
# 配布 (script は seed 決定的なので host 側に他の入力は不要)
scp scripts/misaka-palw-forgery-audit.py <host>:palw-gate2/
ssh <host> 'cd palw-gate2 && PALW_WORKER=~/palw-class/palw-worker \
  MISAKA_PALW_GGUF=/tmp/Qwen3.5-2B-Q4_K_M.gguf python3 misaka-palw-forgery-audit.py'
# 回収して照合
scripts/misaka-palw-gate2-compare.py   # gate2/<label>.jsonl 群 + ローカル結果を比較
```

生データ: 各 host `~/palw-gate2/palw_audit_results.jsonl` (61行、~90KB)。
