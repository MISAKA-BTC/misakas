# 撤回済み経路の削除・merge 引継ぎ（2026-10-10）

作業ブランチは `codex/docs-retired-designs`、`pre` の `c20fca1d8` を基点とする。docs 短縮の `64578ff89` と VM fallback 削除の `c502feae9` を含む。最新 `pre` への merge は後続担当が行う。

- 公開 gateway: 利用者自身のローカル推論という ADR-0144 の範囲から外れるため、公開待受・公開 opt-in を削除。gateway は loopback 限定。ローカル claim の予算オプションは `--claim-budget-permille`。
- V1 credit: PALW V2 の credit に移行したため、旧判定・class resolver・processor の旧設定保持を削除。
- HA takeover: RFC-0012 で専用 DNS validator 役を廃止する方針で、validator 側にも利用経路がないため、token 生成・署名・CLI purpose を削除。
- inactivity leak: 対象の DNS validator lineage を廃止する方針で、設定検証も拒否するため、設定候補ツールの setter を削除。
- algo 4/5 PoW: commitment 偽造を排除できない方式のため、driver・runtime dispatch・miner 探索・resident PoW mode・fixture/calibration・専用 launcher を削除。

互換用の Params encoding、旧 algo ID、signer の wire 番号は予約として維持する。全6ネットワークの `identity_id` / `params_id` は変更前後で一致し、derived transformer のソース hash も維持する。現行 V2・Kernel・native EVM・ローカル推論・診断 replay を利用する。

## 検証

- node・miner・CLI・gateway・signer・extension・consensus の7パッケージを `cargo check --locked` で確認。
- PoW・signer・host security・extension・gateway の unit/bin テスト148件、Params 関連128件、signer wire 関連16件、pruning proof 関連13件が通過。PoW integration 7件と CLI の Ollama request/receipt テストも通過。
- native 診断 worker の build が通過し、旧 `pow-agent` が起動時に拒否されることを確認。derived-transformer 文書の vector 再実行も通過。
- shell/Python/YAML の構文、新規ローカルリンク、`git diff --check` を確認。

## 既存の残件

`python3 scripts/check-repin-enumeration.py` は既存108ファイルの未分類 frozen literal により exit 2。全件が変更前の HEAD にも存在することを確認済み。今回削除したファイルの古い分類エントリは除去し、VANISHED は0件。分類判断が必要な別作業として残す。
