# t12 node なしマイニング — node／CLI 側の修正(2026-10-26)

対象: MISAKA Studio フィードバック(2026-10-26、main `366763a64`、fleet rev `247ade1f90b6`、PR #124)。branch `claude/t12-nodeless-fixes`(base `321978444`)。**consensus 規則・params・wire・DB は変更していない**(`consensus/` 配下は無変更)。

## #4 (P1) endpoint 引数が wss:// URL を受け付けない

- `misaka_endpoints::wrpc_url(endpoint)` を追加: `host:port` → `ws://host:port`、`ws://`・`wss://` の完全な URL はそのまま(gateway `chain::normalize_endpoint` と同じ規則。gateway 側もこの関数を呼ぶ形に統一)。
- CLI で `format!("ws://{hostport}")` していた全箇所(`operator/snapshot.rs::connect_to`、`node.rs` ×2、`wallet.rs`、`prea.rs`、`palw_derived.rs`、`validator_reader.rs`、`setup/mod.rs`)を `wrpc_url` に置換。`bond register-export --rpc/--quote-rpc`、`register-submit --relay`、`model_remote::relay` はすべて `snapshot::connect_to` を通るので、`wss://wallet.misakascan.com/t12/wrpc/borsh` と `wss://llm.misakascan.com/t12/wrpc/borsh` をそのまま渡せる。
- 試験: `wrpc_url_accepts_hostport_and_full_urls`(公開 wRPC 表の全 URL が無変換)。

## #6 (P2)

- `misaka-palw-fp-rail` の「bond が無い」案内に node なしの手順(`misaka bond register-export → register-sign → register-submit --relay`、公開 wRPC 2 本の例)を追加。
- gateway: `--public-job-budget-permille` を `--claim-budget-permille` の別名として受け付け、非推奨の warning を stderr に出す。

## #1 (P0 の一部) evidence provider を複数に

- `misaka_endpoints::testnet12_evidence_providers_v1() -> &[&str]`(既定は現在の 1 台 `http://169.58.232.113:8088`)、`testnet12_evidence_provider_list_v1()`(カンマ区切り)を追加。`testnet12_evidence_provider_v1()` は先頭を返す互換関数として残す。追加 URL は配備時に表へ足す。
- gateway: `--evidence-provider testnet12` で公開 provider 全部を指定できる(既存どおり繰り返し・カンマ区切りも可、`--evidence-min-copies` は provider 数以下を boot 時に検査)。
- `palw-evidence` の `--providers` 既定値を公開 provider 全部に。`misaka node endpoints`(bootstrap)が `evidence_providers` を一覧表示。

## #5 (P1) release に bond register-* が入るか

- `bond register-export/sign/submit` は feature gate なしで `misaka` binary に入る(導入 `60dceccc1`、fleet rev `247ade1f9` の `main.rs` にも存在)。`contrib/t12-deploy-kit/build-release-5104.sh` は `--bin misaka` を build・install・SHA256SUMS に含めている → **修正不要**。
- 注: release kit は `misaka-palw-gateway`・`misaka-palw-fp-rail`・`palw-evidence` を配らない。node なし miner がこれらを release から取得する前提なら別途追加が要る(統括判断)。

## #2 確認: seat の HTTP provider flag

main `366763a64` の `kaspad/src/args.rs` / `daemon.rs` / `palw_panel.rs`(`PanelConfig::evidence_providers`)で、`--palw-evidence-provider-dir` は `parse_provider_list_v1` で解釈され、directory・`dir:<path>`・`http(s)://` URL を読める。`require_equals(true)` で単一値(`get_one`)なので、複数は**カンマ区切りで 1 回**:

```
--palw-evidence-provider-dir=http://169.58.232.113:8088,http://<追加 provider>:8088
```

(フラグの繰り返しではなくカンマ区切り。空白不可の shell 引数として渡す。)

## consensus 非変更の確認

変更ファイルは `misaka/endpoints`、`misaka/apps/cli`、`misaka/palw/gateway`(main・chain・rail・Cargo.toml)、`misaka/palw/remote/src/bin/palw-evidence.rs`、本文書のみ。`consensus/`・`kaspad/` は無変更。
