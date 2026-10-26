# RFC07 paid check authority の native Resource 保持

状態: **native 2 caller接続済み・限定的 GREEN**。最初の authority/ledger/lock 読取後に別 owner が論理容量を占有する RED を実際に再現した。Receipt と FS の両方で、従来 caller は `NativeResource` を `KernelRouteRefused` に変えていた。これを型付き reader と `kernel_ledger_projection_state_error_v1` に接続し、旧 String API は同じ reader の互換 wrapper にした。修正後の同じ4試験と reader 関連15試験は通った。全 stage の検査・抽選承認 campaign 完了とは扱わない。

## 実consumerの問題

`consensus/core/src/palw_claim_challenge_v2.rs::paid_claim_check_authority_v2` は最初の `paid_claim_authority_checked_v6` を `if let Ok(...)` として扱う。Resource/Decode のどちらでもlegacy `aux_row` 読取へ進める。後続の `ledger()` は `ledger_checked_v1` の型付きエラーをStringへ落とす。実nativeの ordinary Receipt とFS initial-source再検査はそのStringを `KernelRouteRefused` へ変換する。

これはselected承認が省略されることを実証した記録ではない。後続ledgerが同じ容量不足で拒否する場合もあるが、そのローカル失敗は合意上の無効objectと区別する必要がある。

## 今回の限定範囲

新 `palw_paid_check_authority_checked_v2.rs` はprimary authority、optional V6 binding、effective lock、ledgerの型付き結果を保持する。primary読取のErrをversion/absence判定に使わず、Resourceでraw fallbackへ移らない。将来の接続時に旧StringメソッドはSDK互換wrapperとし、2つのnative callerのみ型付きsiblingのエラーを既存 `kernel_ledger_projection_state_error_v1` で変換する。

Params、activation、quorum、source contract/provenance、bootstrap guard、公開presetには変更しない。full selected approval のnative exporter不足を、このエラー分類修正で解決したとはしない。

## RED と修正後の検査

filter: `paid_check_authority_`（4親、Resource親には隔離childが内包される）。REDでは 2 failed / 2 passed。最初の 512 MiB と 8 GiB の論理上限は fixture の先行予約を通せず、狙った第2readには到達しなかったため RED と数えない。次の `1 << 60` は試験用の論理上限で、実 RAM・実容量の測定値ではない。これは最初の読取を通し、callback が残量だけを実 lease として保持した後、Receipt と FS の第2readで上記の誤分類を再現した。

修正後: `cargo test --offline -p kaspa-consensus-core --lib paid_check_authority_ --config profile.test.package.kaspa-consensus-core.opt-level=0 -- --test-threads=1` は **4 passed**。同じ build の test binary で `paid_claim_reader_v1 --test-threads=1` は **15 passed**。この15件には同じ4件も含むので、19件の独立試験とは数えない。`git diff --check` と `check_docs.py` は PASS。いずれも private fixture の in-process 結果であり、実 node、署名済み source、100k、pruned IBD、承認済み normal source の検証ではない。

- ordinary Program fixtureのactual native Lock/Receiptを使う。第1 authority/ledger/lock検査を通過した直後、single-use thread-local `cfg(test)` callbackが別ownerとして実logical quota leaseを保持する。authority/result/stateを注入しない。
- callback到達、actual第2readのResource分類、state root不変、blocker解除後の同一receipt成功を確認する。単純zero-quota入口拒否を旧不具合REDとして数えない。
- actual Lock/Receiptのoutsider拒否、quorum未達のconsumer incomplete、全assigned seat到達後completeを確認する。
- FSの初期bootstrapはinteractive planに対して正しく拒否されるため、そのpositiveを作らない。既存同様の明示的storage initial-lock negativeからactual StageMessageの第2read Resource分類を確認し、quota解除後も未認証sourceは拒否されることを維持する。これは署名済みFS source campaignではない。
- missing/corrupt primary authorityはDecode、既存positiveはtypedとString wrapperで同じauthorityを得ることを確認する。

## 次波に残るnormal String source境界

今回のdraftは旧normal source algorithmを保持する。次の共有chainが失ったResourceを、display文字列の解析で復元したとは主張しない。

1. `paid_beacon_material_v5` (`palw_beacon_source_fee_v1.rs`) → `paid_claim_source_authority_v3` / `beacon_opt_in_sources_v4`。
2. `paid_claim_source_authority_v3` (`palw_claim_interactive_v3.rs`) → `ledger()`、staged-context authority / `validate_paid_round_v4`。
3. `beacon_opt_in_sources_v4` (`palw_beacon_source_opt_in_v1.rs`) → `ledger()` / `finals_read_v1`。
4. `finals_read_v1` (`palw_kernel_route_v1.rs`) → `ledger_native_projection_v9` / `final_export_v1`。

このchainを同algorithmの型付きsiblingへ因数分解するのは別の具体scopeである。別実装のV6 material collectorへの置換は行わない。unknown source provenance・historical signatures・K3・native39→RFC12正式settlement写像を創作しない。
