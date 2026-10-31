# A6 — 未anchor tailとcrash時quotaの追加監査

2026-10-31。基準 `e865ca36c`、runtime実装 `66bef38cc`。A0から共有されたA2のchecksum-valid未anchor tail所見を受け、A6を独立に確認した。**A6 runtime変更なし、回帰2件を追加し、production結合13件が成功した。** [raw log](../../../evidence/positive-court-a6-tail-audit-2026-10-31/tests.log) / [source hash](../../../evidence/positive-court-a6-tail-audit-2026-10-31/source-snapshot.json)。以前の184件とは別の追加runとして記録する。

## 外部protected headと全event再認証

実DBで1 round / 4,096 unique bytesの要求を確定し、そのheadをDB外のContextへpinする。その後、次を個別に検査した。

- 正常にencode/hashされた短い履歴（0 bytes課金）を持つ別DBからの復旧・同期は、pinした要求義務とheadが違うため拒否する。
- 正常にencode/hashされた余分な応答tailを実DBへ保存しても、DB側のheadを自動的にauthorityへ昇格せず、外部Contextの期待headと違えば復旧・startup projection・同期token生成を拒否する。
- checksumと期待headを一致させた対照は通常どおり再生できる。しかしcanonical historyがその応答eventを認めないContextでは、同じbytes・同じheadでもorigin/正規event認証で拒否する。

hashは履歴の同一性であり、履歴採用や計算の正しさを与えない。Contextはnativeのprotected anchor/current viewからhead・origin・accepted DAA・同一DAA内順序を解決する必要がある。これらをpeerのarchiveから導出するdefault実装は提供していない。実native authorityの接続条件は [A6-NATIVE-CONNECTION.md](A6-NATIVE-CONNECTION.md) を参照。

## 正直な中断窓とquota

署名・実TIR fixtureに対する実request proposalを作り、そのexact archive/head/関連料金・quota行を実RocksDB取引へ渡した。同期batchの書込み前と書込み後の返答喪失を注入し、DBをclose/reopenする。

- BeforeWriteは事前pinしたH0だけを採用し、0 round / 0 bytesを復元する。未確定H1へ昇格しない。
- AfterWriteは事前pinしたH1だけを採用し、1 round / 4,096 bytesを復元する。H0へ戻して予算を再生しない。
- 各窓で反対側headによる復元を拒否する。再送を2回行っても累積要求は1 round / 4,096 bytes。
- 4,095-byteの不正な短い応答はWire拒否となり、資料完了・料金支払を生まない。その後の完全応答と重複完全応答はopening込みで各4,172 bytes、合計8,344 bytes / 2 workを計上し、料金は一回だけ10。短い応答やtail metadataで後続課金を割り引かない。
- 再度DBをreopenしても、そのquotaと支払状態を保持する。

ここでのBefore/AfterWriteは実DB fault/reopen試験。別の既存parent testはworkerを実子processとして起動し、sync commit直後のexit86と再起動後一回払いも実行した。ignored表示のworkerはparent経由で実際に呼ばれている。

## 実行と範囲

```sh
CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-testnet/target CARGO_INCREMENTAL=0 \
  cargo test -p misaka-palw-positive-recovery --test persistence \
  --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.dev.package.kaspa-consensus-core.debug=0' -- --test-threads=2
```

最初のlinkはディスク容量不足(errno28)で停止した。この作業自身の再生成可能な古い試験executableだけを除去して同一sourceを再実行し、13 passed / 0 failedを確認した。compile/link失敗を試験成功へ合算していない。

private実DB/実Court adapterの監査であり、新native aux valueの受理や実UTXO支払成功ではない。共有namespaceはA0のaux **151** / versioned key契約が正本。A6 private archiveを未確定のnative row valueへ流用せず、未接続native creditの拒否を維持する。
