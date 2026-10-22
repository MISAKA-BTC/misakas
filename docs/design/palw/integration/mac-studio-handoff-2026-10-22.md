# Mac Studio への作業指示 — G14 統合版の重い検証（2026-10-22）

あなた（Mac Studio 上の Claude）は、MacBook 側の Claude が進めている G14 完成作業のうち、**重いビルド・テスト・計測・drill** を担当します。MacBook 側は実装と修正を並列で進めており、1 本のビルド lock に十数本が並んでいるため、検証をこちらへ分けます。結果は git の branch と evidence 文書で返してください。

## 0. 目的と前提

- 目標: G14（RFC-0014、RFC-0026 の T01〜T15）の完成と、testnet-12 で **DAA 16,000 の fence** による全機能有効化。現在の t12 は int-12 release `0b1c11b87`、DAA 約 13,850。
- 対象 branch: `g14/integration-r7` @ **`24cd1f045`**（約 40 本の実装 lane・監査修正 fix-a・CLI/RPC・30 GB ストレージ 3 本・RFC gap A を統合）。新機能は全て fence で休眠（`None`）。full-activation リストの高さも `None`。
- 最初に読む: `AGENTS.md`、`docs/AGENTS.md`、`docs/PRINCIPLES.md`（§8 計算の暗号学的証明を使わない、§9 モデル二階層、§11 設計凍結）、`docs/design/palw/integration/claude-handover-g14-2026-10-22.md`、`docs/design/palw/integration/g14-t12-16000-decisions-2026-10-22.md`、`docs/evidence/palw/g14-parallel-2026-10-22/`（各 lane の notes）。

## 1. 守ること

- **push 先は `studio/<task>` branch のみ。** `pre`・`main`・`g14/*`・`audit/*` には push しない。
- **live の t12 node・VPS には接続しない。** drill は自分のマシン上の private devnet（salted genesis）だけで行う。
- product code（consensus・node）の修正は、下の §3 で許可したもの以外はしない。原因を特定したら修正案を evidence に書いて返す。テスト側の明白な誤り（期待値の誤り・fixture 不足）は直してよいが、assertion を弱めない・削除しない・`#[ignore]` で逃げない。
- fence を実在の高さで武装しない。値（料金・slash 額・cap など）を仮決めしない。
- 実行していないことを実行済みと書かない。計測値には必ずハードウェア（`sysctl hw.model hw.memsize hw.ncpu`）を併記する。Mac Studio は「最低要件ハードウェア」ではないので、その旨を明記する。

## 2. 準備

```bash
git clone https://github.com/MISAKA-BTC/misakas.git misakas-studio   # 既存 clone があれば fetch
cd misakas-studio
git fetch origin "+refs/heads/g14/integration-r7:refs/remotes/origin/g14/integration-r7"
git checkout -b studio/verify origin/g14/integration-r7
git log --oneline -1   # 24cd1f045 であること
cat rust-toolchain.toml   # Rust 1.93.0（rustup が自動で入れる）
```

- zsh で `$b` の直後に `:r` などを書くと修飾子として解釈される。branch 名は必ず `${b}` と書く。
- `misaka-palw-worker` は llama.cpp（`MISAKA_LLAMA_SRC`）が必要なので、workspace コマンドでは `--exclude misaka-palw-worker` を付ける。
- target dir は 1 本にまとめる（例 `export CARGO_TARGET_DIR=$PWD/target`）。ディスク残量を `df -h` で確認しながら進める。

## 3. タスク（優先順）

### T1 — 統合版の一括テスト（最優先）

```bash
cargo check --release --workspace --tests --exclude misaka-palw-worker
cargo test --release -p kaspa-consensus-core --no-fail-fast -- --test-threads=8 2>&1 | tee core.log
cargo test --release -p kaspa-consensus --lib --no-fail-fast -- --test-threads=4 2>&1 | tee consensus.log
cargo test --release -p kaspad -p kaspa-mining -p kaspa-rpc-core -p kaspa-grpc-core -p misaka-cli -p kaspa-utils -p kaspa-database --no-fail-fast 2>&1 | tee others.log
```

- **ハングに注意。** MacBook 側では court 系の 1 テストが 1 コア 99% のまま 27 分止まった（fix-b lane のフィルタ `verify2_b0 fix_b0 court_f02 audit_b03 r5_f02 r6_d4 d2c1 readiness` のどれか。fix-b はまだこの branch に入っていない）。1 テスト 10 分を超えたらハングとして記録し、`--skip` で外して続行する。
- 結果を分類する: 成功数 / 失敗数 / ハング / ignore。失敗ごとに、(a) この統合で壊れたのか、(b) base でも落ちるのか、を判定する。比較する base は **`b1eb33960`**（統合 r6、lane 統合前の compile 済み版）と **`537843abe`**（現在の `pre`）。

### T2 — 既知の失敗の原因特定

MacBook 側の lane から報告された失敗（未確認）:
1. `cargo test -p kaspa-consensus --lib g14_` の **既存 G14 kernel-route E2E が約 11 件 FAILED、最後に SIGABRT**（storage capacity lane の報告。同 lane の変更はテストでは作動しないので別原因の見込み）。
2. fork-id の probe が `palw_tir_container_identity_v1` を設定できない（fork_id テスト 2 件）。
3. full-activation（DAA 16000 リスト）のテスト 1 件（`cargo test -p kaspa-consensus-core --test palw_t12_full_activation_16000`）。
4. `funded_escrow_every_path_reverts_to_its_parent_row`（厳格化した hard 期限より前に書かれたテスト）。

各件について root cause・どの commit で入ったか（`git bisect` 可）・修正案を書く。**2・3・4 と、テスト fixture 起因の 1 は修正してよい**（`studio/fixes` branch に commit。修正ごとに再実行して PASS の行を記録）。product の consensus 規則を変える必要があるものは修正案だけ返す。

### T3 — liveness 計測（稼働中の t12 に関わるので重要）

liveness 監査で見つかった、**現在の t12 で有効な**問題の計測。MacBook では lock 待ちで実行できていない。

- `origin/liveness/court`（`f74ca87d3`）: `consensus/core/tests/audit_liveness_court_fences.rs`、`audit_liveness_court_close_replay.rs`。L-COURT-01（拒否された court/close chunk が mempool で完全裁定・再送無制限、block で枠外）の 1 回あたりの CPU 時間を、close 組立サイズ 1 KB〜2.7 MB で計測。
- `origin/liveness/lifecycle`（`7abbaaed3`）: `consensus/core/src/palw_state_v2/tests/liveness_lifecycle_v1.rs`。L-LC-01/02（object ごとの全 state 複製）を claim 行 1k / 10k / 50k / 200k / 1M で計測し、成長の次数を出す。
- 各 branch を fetch して実行（`git fetch origin "+refs/heads/liveness/court:refs/remotes/origin/liveness/court"` など）。計測値の表、成長の次数、t12 の現在規模（claim 約 262）での 1 tx / 1 block あたりのコスト見積りを返す。

### T4 — T01〜T15 node campaign

`consensus/src/pipeline/virtual_processor/tests/g14_t_campaign/`（実 node harness: Full UTXO・ML-DSA 署名）。

```bash
cargo test --release -p kaspa-consensus --lib g14_t_campaign -- --test-threads=2 --nocapture
cargo test --release -p kaspa-consensus --lib --features g14-t-campaign-merged g14_t_campaign -- --ignored --nocapture
```

- 2 行目の ignored 11 件は、他 lane の API 名を推測して書いたもの（例 `K::PaidIngressOpen`、`palw_shared_duty_v1`）。実際に入っている名前（tag 178 `SharedCourtOpV1`、tag 255 `MaterialIntakeV1`、`palw_funded_escrow_v1`、tag 201–204 など）に合わせて **テスト側だけ** 直してよい。
- T 項目ごとに: どの test が何を確認したか / PASS・FAIL / ignore のまま残る理由。「T 項目全体の完了」とは書かない（values 未承認の経路は本番で拒否されるため）。

### T5 — 出荷 binary での fence 越え drill（private devnet のみ）

```bash
cargo build --release -p kaspad -p misaka-cli   # drill が要求する binary 名は script を参照
python3 scripts/drills/g14_t12_16000_fence_drill.py --plan-only
python3 scripts/drills/g14_t12_16000_fence_drill.py --crossing=full-activation-flag ...   # 低い高さ H（>28）
python3 scripts/drills/g14_t12_16000_fence_drill.py --crossing=factory ...
```

- arming record が未 pin なので、full-activation flag は **validation で拒否されるのが期待値**。何がどの理由で拒否されたかを記録する（どの fence の record が無いか）。factory 経路で fence 越え前後の claim、再起動、reorg が通るかを見る。
- 判定は JSON verdict（F0〜F7）。観測できなかった phase は BLOCKED と書き、PASS と書かない。

### T6 — 30 GB ストレージ計測（時間があれば）

`docs/design/palw/storage/capacity.md` §8 の M1〜M9 のうち、devnet で実行できるもの（staging DB と active の同時ピーク、compaction 一時領域、IBD spill）を小規模で計測。`--storage-cap-bytes` に縮小した値（例 2 GB）を与えて、予約による拒否・低容量時の停止と再開・回収が働くかも確認。

## 4. 返し方

- evidence: `docs/evidence/palw/studio-2026-10-22/T<n>-<name>.md`。必ず書くこと: 実行したコマンド全文、checkout した commit、binary の sha256、ハードウェア、結果の表、未実行の範囲と理由。生ログが大きい場合は gzip にして同じディレクトリに置く（1 ファイル 20 MB 以下を目安）。
- docs を編集したら `python3 docs/maintenance/generate_indexes.py` と `python3 docs/maintenance/check_docs.py`（新規の regressions がないこと）。
- commit trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
- push: `studio/verify`（T1・T3・T4・T5・T6 の evidence）と `studio/fixes`（T2 の修正）。
- 最後に `docs/evidence/palw/studio-2026-10-22/SUMMARY.md` に、タスクごとの状態（完了 / 一部 / 未実行）、失敗一覧と分類、修正した commit、MacBook 側への依頼事項を 1 ページでまとめて push する。MacBook 側はこの SUMMARY を読んで統合する。
