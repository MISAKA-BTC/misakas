# A6 aux151 — native row 検査・復元登録

2026-10-31。branch `codex/rfc42-a6-recovery`。A0 の共通 VALUE schema を取り込み、A6 の private recovery archive と区別して native aux151 を検査する。scanner/heap、参照復元、独立負例を3 agentで実装し、親が native entry point の登録と統合検証を担当した。

## 接続と検査

`palw_native_resource_v1/positive_rows.rs` は10 kind の field/variant を網羅する wire/heap visitor と key-aware scanner を持つ。共有 schema の field/variant が増えると、visitor の見直しなしにはコンパイルできない。Borsh map の前段で重複 Positive key を拒否し、typed decode の局所整合と後段の署名・参照整合を分ける。generator は typed scanner と厳密 key 順序の登録を保持し、template drift 時はエラーにする。

`palw_positive_rows_v2::restore::validate_positive_aux_restore_v2` を `PalwChainStateV2::assert_internal_consistency_v3` および `PalwKernelRouteStateV1::from_served_rows_v1` に登録した。valid root でも必須 row、署名、journal head、receipt literal が欠ければ復元を拒否する。結果 DTO は structural diagnostic であり、native admission・算術再実行・実 funding lock・payment・Gate C を作らない。`from_served_rows_v1` は公開 row 再構築の整合性 API であり、それ自体は native RAM ticket を取得しない。strict native resource admission は予約を持つ carriage wrapper を通す必要がある。

短い ClaimIndex も大きい `NativeRowValue` の inline サイズを課金する。raw key/value Vec の容量に加え、固定 decode workspace と case/attempt uniqueness index を全 row から保守的に予約する。型全体の enum サイズを encoded row 比率で割り引かない。共有 hash の逐次 Borsh 入力は domain と ID を変えず、成功する固定 row 検査に一時 Vec を要求しない。

## authority と寿命の境界

初期 JournalHead は native ResultAdmission を含む sequence=1、previous=ZERO。history origin は Case.last_event、初期は Result.origin と一致させる。private archive の初期 records=0 と混同しない。後続 head/history の canonical preimage、accepted prefix と正規 event 順序は native backend が再構築する必要があり、root membership だけでは代替できない。

SourceUnion.completed_units は保持する ServiceReceipt literal 数と一致する。native v2 には receipt 単独の pruning 規則を追加しない。private archive の旧 undo/receipt pruning と alias しない。A2 の local spent I/O budget は reorg で巻き戻さない別 namespace・別寿命であり、A6 chain transaction undo の RelatedRow に入れない。

実 native carrier dispatcher、受理履歴 backend、UTXO/funding authority、P 認証はそれぞれの owner の接続責務である。public preset の activation は追加していない。暗号学的なモデル計算証明を用いず、モデル二階層の凍結を維持する。

## 検証記録

最終候補 `d2bdb174c` の core は **101 passed / 0 failed**（新規row15、native資源61、route25）。資源61は32MiB stack、それ以外は標準stackで実行した。共通契約30、DB30、private production13の先行成功は別欄とし、この101件へ合算しない。

最終ログ、source snapshot、再実行コマンドは [aux151 evidence](../../../evidence/positive-court-a6-aux151-2026-10-31/) に保存する。過去の R2/tail-audit evidence は上書きしない。最終件数と実行条件は evidence の `validation.json` を正とする。

core は低容量の検証用 override `profile.dev.package.kaspa-consensus-core.opt-level=0` / `debug=0` で検証する。初回の既存 native resource suite は標準 test stack で `direct_restore_success_keeps_native_owner_and_default_off_state_parity` が stack overflow で停止し、32MiB stack では61件通過した。これは成功件数へ合算しない。前候補 binary の保護 copy がなく、新旧原因を断定しない。元の失敗ログも evidence に保持する。

監査で Positive 復元関数自体の未最適化固定 frame が大きいことを確認したため、空の aux151 を薄い dispatcher で返し、各 row kind の参照検査を関数に分割して不要な stack 同居を減らした。論理 memory quote は実測 RSS・compiler frame 上限・30GB footprint の保証ではない。

最終候補でも標準 stack の既存復元試験1件は overflow を再現した。32MiB 条件の成功を標準条件の成功と扱わない。分割後のARM64未最適化 prologue は薄いwrapper432 bytes、present loop37,984 bytes、Case検査114,032 bytes（各関数単体）。原因を既存実装へ帰属する baseline 比較は行っていない。最初の最終buildのENOSPCと、Cargo/直接metadata試行の失敗も成功へ数えず原ログを保存した。その後同じ固定sourceのfull test compilationと実行が成功したため、最終sourceの型検査はこの正常buildで確認済みである。

source・Markdown の `git diff --check` は成功。evidence の原ログにある末尾空白・空行は保持し、この whitespace 検査から除外する。原ログを整形して成功扱いにはしない。

## レビュー後の追補

受理順序、成功receiptの完全性、費用下限、Route manual review登録の追加検証は [A6-ORDER-COUNTERS-VALIDATION.md](A6-ORDER-COUNTERS-VALIDATION.md) に分離した。ここに記録したd2bdb候補と101件の結果は履歴として保持する。
