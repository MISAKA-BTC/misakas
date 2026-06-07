# kaspa-pq ML-DSA-87 PQ-only 移行 — 動作確認手順書 (PR-19 series)

- 対象: ブランチ上のコミット列 `PR-19-S1 … S11c` + crash 修正（`MISAKA-BTC/misakas` に snapshot push 済）
- 統治文書: [docs/adr/0019-mldsa87-migration.md](adr/0019-mldsa87-migration.md)（rev 1.2 = md2 整合）+ [docs/kaspa-pq-design-mldsa87.md](kaspa-pq-design-mldsa87.md)
- リポジトリ: `/Users/wata/Downloads/rusty-kaspa-master 2`（パスに空白を含むのでクオート必須）

この文書は **Phases 1-4 + 5a（コミット済み）** が正しく動くことを自分で検証する手順。Phase 5b-5e / 6 / 7 / 8 と実デプロイは**未実施**（末尾参照）。

---

## 0. 何が入ったか（1行サマリ）

署名方式を **ML-DSA-65 → ML-DSA-87**（NIST cat3→cat5）に移行し、**PQ-only 強制**（legacy secp256k1 / P2SH / 非 ML-DSA アドレスを consensus・mempool で実行不能化）+ **64-byte keyed BLAKE2b-512 アドレス**（`kaspa-pq-v2/address/mldsa87`）+ **premine を単一鍵 ML-DSA-87 P2PKH 化** + **genesis 再生成**。新 genesis につき旧チェーンとは非互換。署名 context は v2、caps は 16_384、識別子は `MlDsa87`（md2 整合, ADR-0019 rev 1.2）。

主要な確定値:
| 項目 | 値 |
|---|---|
| 公開鍵 / 署名 サイズ | 2592 B / 4627 B |
| tx 署名 context | `b"kaspa-pq-v2/tx/mldsa87"`（`MLDSA87_TX_CONTEXT`） |
| tx sighash | `calc_mldsa87_signature_hash` → 64-byte `Hash64`（domain `b"kaspa-pq-v2/sighash/mldsa87"`） |
| アドレス | `Version::PubKeyHashMlDsa87` のみ・payload = 64-byte **keyed** BLAKE2b-512(`kaspa-pq-v2/address/mldsa87`, pubkey) |
| 標準 scriptPubKey | 69 byte: `OP_DUP OP_BLAKE2B_512(0xc4) OP_DATA64(0x40) <64B> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87(0xa6)` |
| caps | `MAX_SCRIPT_ELEMENT_SIZE`=8192 / `MAX_SCRIPTS_SIZE`=16_384 / `max_signature_script_len`=16_384 |
| premine | 15B KAS を単一鍵 ML-DSA-87 P2PKH にロック（旧 2-of-3 multisig P2SH を廃止） |
| PQ 強制 | `Params.pq_enforcement = PqEnforcementMode::Consensus`（全 kaspa-pq ネット, activation=genesis） |

---

## 1. ビルド（全体）

```bash
cd "/Users/wata/Downloads/rusty-kaspa-master 2"
cargo build --workspace
```
**期待**: エラー 0 で完了（`Finished` で終わる）。これが通れば node バイナリ・全 crate がコンパイル可能。

---

## 2. フェーズ別テスト確認

各コマンドの **期待 = `test result: ok`**（下記の合格数は目安）。1コマンドずつ実行。

### Phase 1 (PqEnforcementMode) + Phase 2 §7 (output-class) + Phase 4 (genesis) — consensus-core
```bash
cargo test -p kaspa-consensus-core --lib
```
期待: `214 passed; … 1 ignored`（`ignored` は `gen_misaka_devnet_premine_key` 等の手動 gen テスト）。
特に確認したいテスト:
- `config::genesis::tests::test_genesis_hashes` — 5 つの genesis 定数（hash / merkle / utxo_commitment）が premine と自己整合。
- `config::premine::tests::print_premine_commitment` 系。
- `hashing::sighash::mldsa87_sighash_tests::*`（6 本）— 64-byte sighash の決定性・schnorr digest との分離。

### Phase 2 §6 (legacy opcode/P2SH disable) + §7 + Phase 4 (premine/address) — consensus
```bash
cargo test -p kaspa-consensus --lib
```
期待: `… passed; 0 failed`（約 322）。特に:
- `processes::transaction_validator::tx_validation_in_isolation::pq_output_class_enforcement_tests::*`（5 本）— coinbase/overlay 免除 + legacy 拒否。
- `consensus::utxo_set_override::tests::premine_is_a_single_15b_utxo` — premine が 69-byte P2PKH。
- `pipeline::body_processor::…::validate_body_in_isolation_test` — legacy fixture は `PqEnforcementMode::Disabled` で実行。

### Phase 2 §6 + Phase 3 (sighash64) + Phase 4a (64B address/opcode) + Phase 5a (gate) — txscript
```bash
cargo test -p kaspa-txscript --lib
cargo test -p kaspa-txscript --test pq_policy
```
期待: lib `78 passed; 2 ignored`、pq_policy `4 passed`。特に:
- `bitcoind_tests::test_mldsa65_p2pkh_spend_roundtrip` — **署名→スクリプトエンジン検証=OK の往復**（signer≡verifier の安全網、64B address + 64B sighash + mldsa87 ctx を全部貫通）。
- `standard::tests::pay_to_address_script_pq_gates_legacy` — ML-DSA は通し、legacy 3 クラスを拒否。
- `tests::pq_only_*`（pq_policy）— legacy 署名 opcode 0xa9/ab/ac/ad/ae/af を無効、ML-DSA 0xa6/a7 を維持。
- 注: `test_multisig_mldsa65_2_of_3` は `#[ignore]`（P2SH は launch scope 外）。

### Phase 3b (signer) + Phase 5a (native signer) — wallet-keys / validator-core
```bash
cargo test -p kaspa-wallet-keys --lib
cargo test -p kaspa-pq-validator-core --lib
```
期待: wallet-keys `19 passed; 7 ignored`、validator-core `12 passed`。特に:
- `kaspa_pq::tests::native_signer_round_trips_through_engine` — native ML-DSA 署名器の出力が consensus エンジンで検証可能。

### アドレス層 — addresses
```bash
cargo test -p kaspa-addresses --lib
```
期待: `5 passed`。

---

## 3. 既知の RED（移行とは無関係・対応不要）

```bash
cargo test -p kaspa-mining --lib
```
→ `test_calc_min_required_tx_relay_fee` が `got 480000, want 100000` で **1 件 FAILED**。これは**親コミット `28aa660` でも同一に失敗する pre-existing red**（未変更の `MAXIMUM_STANDARD_TRANSACTION_MASS=480_000` に対するテスト期待値ズレ）。本移行の回帰ではない。確認方法:
```bash
git stash; git -c advice.detachedHead=false checkout 28aa660
cargo test -p kaspa-mining --lib test_calc_min_required_tx_relay_fee   # 同じく FAILED
git checkout -   # 元のブランチへ戻す（必要なら git stash pop）
```

---

## 4. PQ-only 不変条件の確認（設計の肝）

| 不変条件 | 守る場所 | 確認テスト |
|---|---|---|
| legacy secp256k1 署名 opcode は consensus で実行不能 | txscript `is_legacy_signature_opcode` + engine | `pq_policy` |
| P2SH spend は PQ で拒否 | txscript engine `execute()` | `pq_policy` / ScriptHashDisabledInPqMode |
| 非 ML-DSA-P2PKH の **出力**は block/mempool で拒否 | `check_transaction_pq_output_classes`（coinbase/overlay 免除） | `pq_output_class_enforcement_tests` |
| ML-DSA 署名対象 = 64-byte 専用 sighash（schnorr 流用しない） | `calc_mldsa87_signature_hash` + verifier | `mldsa87_sighash_tests` / roundtrip |
| 標準アドレスは 64-byte BLAKE2b-512 P2PKH のみ | addresses + standard.rs | roundtrip / gate test |
| wallet は legacy アドレスへ送らない（作れない） | `pay_to_address_script_pq` | `pay_to_address_script_pq_gates_legacy` |

---

## 5. genesis / premine 再生成手順（変更時のみ）

アドレス幅・premine 鍵・コインベース等を変えたら **この順で**再生成（各段でリビルド要）:

```bash
# (1) premine 単一鍵 + 64-byte owner payload + devnet アドレスを生成
#     → リポジトリ root に misaka-devnet-premine-key.json（gitignore 済・seed は秘密）
cargo test -p kaspa-consensus-core --lib \
  config::premine::tests::gen_misaka_devnet_premine_key -- --ignored --nocapture
#     出力の 64-byte 配列を premine.rs の MISAKA_PREMINE_OWNER_PAYLOAD に貼る

# (2) premine UTXO commitment を再計算
cargo test -p kaspa-consensus-core --lib \
  config::premine::tests::print_premine_commitment -- --nocapture
#     出力の Hash::from_bytes([...]) を genesis.rs の全ネット utxo_commitment に貼る

# (3) 5 つの genesis ブロックハッシュを再計算
cargo test -p kaspa-consensus-core --lib \
  config::genesis::tests::gen_kaspa_pq_genesis_hashes -- --nocapture
#     各ネットの hash_merkle_root / hash を genesis.rs に貼る

# (4) 自己整合の確認（必ず緑になること）
cargo test -p kaspa-consensus-core --lib config::genesis::tests::test_genesis_hashes
```
⚠️ `misaka-devnet-premine-key.json` は **秘密の seed を含む**。gitignore 済み。**絶対にコミット・共有しない**。

---

## 6. コミット列（未 push）

```
b3f1e8a PR-19-S5a  Phase 5a: native ML-DSA signer + pay_to_address_script_pq
3c69500 PR-19-S4b  Phase 4b/4c: premine→P2PKH, caps 10_000, regen genesis
c80af3a hotfix      restore ScriptClass::is_pq_standard()  ← S2 で def 欠落が露見した修正
2aef746 PR-19-S4a  Phase 4a: 64-byte BLAKE2b-512 address
f1f49a9 PR-19-S3b  Phase 3b: verify+sign を 64-byte sighash へ
7b36810 PR-19-S3a  Phase 3a: calc_mldsa87_signature_hash 追加
43077f6 PR-19-S2   Phase 2 §7: 非 ML-DSA-P2PKH 出力クラス拒否
47467e4 PR-19-S1   65→87 値スワップ + PqEnforcementMode + §6
（親 28aa660 = PR-18-S13、移行前）
```
差分一覧: `git log --oneline 28aa660..HEAD`、全差分: `git diff 28aa660..HEAD`。

---

## 7. 未実施（次セッション）

- **Phase 5b-5e**（wallet/API のフル PQ-only 化）: 5b legacy `to_address`/`to_address_ecdsa` を PQ ネットで Err 化 + ML-DSA アドレスマネージャ / 5c ML-DSA account variant / 5d Generator 統合 / 5e fixtures + wallet-core native-test の `kaspa_pq_wasm` cfg ゲート問題。wallet-core は現状ほぼ secp256k1 専用型で、統合は大規模。
- **Phase 6**: `utxo_commitment` を Hash64(64B) 化（hard fork、genesis 再生成を伴う）。
- **Phase 7**: ML-DSA-87 verify の実測ベンチに基づく `mass_per_sig_op` 再校正 + dust 見積。
- **Phase 8**: 識別子の `*87*` リネーム最終化 + `secp256k1` feature 隔離（consensus ツリーから除去）+ SigCacheKey の native 64B 化 + spec/ADR/README 更新 + CI(advisory/secp256k1-tree) ハード化。
- **集計 RPC `getNetworkOverlayStats`**: ユーザーの当初要望（validatorCount/totalStake/nodeCount）。PQ-only コア完成後に再開予定。

## 8. デプロイ（**明示 GO 待ち・破壊的**）

genesis が変わったため、デプロイは**全 devnet ホストのデータディレクトリ全消去 → 新 genesis から再起動 → 再収束 → WASM ウォレット再ビルド/再配布**を伴い、後戻り不可。**現時点では未実施**で、ユーザーの明示的な GO（task #10）まで行わない。コードレベルの作業は実ホストに一切触れていない。

---

## 9. 補足（セキュリティ）

- `libcrux-ml-dsa = 0.0.9` は固定。ローカル advisory-db の `RUSTSEC-2026-0076/0077` は `patched >= 0.0.8` なので **0.0.9 は対象外（clear）**。CI ゲート雛形は `scripts/pq-ci-guard.sh`（`cargo deny/audit` + consensus ツリーに secp256k1 が無いことの確認。後者は Phase 8 までソフト警告）。
- premine 鍵は単一鍵（旧 2-of-3 multisig の冗長性は喪失）。multisig opcode（`OpCheckMultiSigMlDsa65` 0xa7）は dead-but-present で残置、再有効化は別 ADR。
