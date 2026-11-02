# RFC-0045 基盤とnative境界の検証（未完了）

2026-11-02に着手し、2026-11-03 JSTに検査。採択済みRFC-0045/ADR-0213の実装途中である。**実native Assignment受付、実P認証、全branchの実予約・支払、RFC45全体の完了を示さない。公開presetはNoneのまま。**

## 確定した検査

| 検査 | 結果 | 記録 |
| --- | --- | --- |
| 共通contract全試験 | 68 passed、失敗0 | `contract-all-fourth.json` / `.log` |
| Core新body/fence/資金/P時計/実G14source geometry | 62 passed、失敗0（4+7+6+4+41） | `core-assignment-source-fourth.json` / `.log` |
| Consensus実DB2、共通lock拒否1、旧G14 native8 | 11 passed、失敗0 | `native-db-filter-legacy-fifth.json` / `.log` |
| 全Consensus試験targetの型検査 | 成功 | `native-all-test-typecheck-first.json` / `.log` |
| contract全試験/Core対象試験strict Clippy | 成功 | `contract-all-test-clippy-fourth.json` / `core-focused-test-clippy-fourth.json` |
| contract/Core/Consensus strict library Clippy | 成功 | `shared-library-clippy-final-baseline.json` / `.log` |

実行済み試験の合計は**141件**。`baseline-artifact-audit-20261103.json`に各実行source map照合、全実行バイナリのSHA256・profile・byte数を記録した。Core/contract実行からnative実行までのRust source変更は、新しい共有filter試験の`Transaction`/`UtxoEntry` import追加だけである。nativeバイナリはリポジトリ既定opt2、debug assertions/overflow checks有効、RUSTFLAGS/RUST_MIN_STACKなし。sampled process-group RSSは監視でありOS強制quotaとは呼ばない。

旧G14 8件は実Consensus/RocksDBの既存経路で、承認不足の支払保留、drawn承認後の一回払い、reorg、署名/不正裁定、途中再起動、DB最終参照close/reopenでのpending/settled一致を確認する。IBD/pruning等の正確な範囲は元の`../legacy-native-history-20261102/README.md`に従う。新Assignmentが受理された試験へ読み替えない。

新DB2件は実RocksDBを完全に閉じて開き直し、データfixtureのjournal整合性・tamper・同batch失敗時の非変更を確認する。payloadは意図的なdata fixtureであり、actual accepted-prefix、役割/原資/P認証を証明しない。共通filter1件は欠損/不正projectionの拒否試験で、正常な新支払を示さない。

## 実装範囲と残件

実sourceにはappend-only op8–13 codec、aux151 kinds10–18、別purposeのA/C/prover資金資格・coin lock、役割別署名、nonce/一消費/会計の構造restore、Assignment journal実DB store、private同checkpoint row writer、actual carrier codec/ordering境界を追加した。Node資料catalogはResultを作らず登録TIR/jobから導出する。全preset fingerprint6件は実pre-Assignmentバイナリと一致した。

実native予約/P/Case execution/checkpoint ownerが欠損する受付は明示拒否のまま。単一writerのprivate setterやDTOの署名/quote/保存状態を受付・creditへ昇格しない。`AssignmentGateCClockV2`は既存policyのchecked時計だけで、pending-fraud全branchの有限予約を生成しない。Final後のprosecutionとResult commitを分ける。

`candidates/`は**未適用・未コンパイルの次の差分**。base/candidate SHA・patchを分け、今回の141件の合格を候補へ移さない。元policy/preimage custody、実typed derivation lease、raw carrier ordinal rehearsal/final checkpoint、actual material outcome/同UTXO DB batch、tentative/accepted支払分離とonceの候補を準備している。P・Case/source session・全将来資源と実送金の完成は別途必要。

通常P2PKH inputはpayerの実署名を要する。payerが止まっても支払/返金を終結できる署名custodyは現op8–13にない。別のRelay RFC/ADR draftを準備したが、**未採択**であり実装許可や本番有効化ではない。既存の資金規則/署名を省略してこの欠損を隠さない。

## 失敗記録

初期strict Clippyはnested-if/未使用import、native型検査はmissing field/trait/importで停止した。追加fingerprint試験の一件はraw scheduled-params hashをgenesis handshake projectionと誤比較して失敗し、比較対象を修正した。失敗した全`.json`/`.log`を残し、未実行targetを合格に数えない。

`operator-scripts/`に各実行のcontrollerを保存。実行sourceは各JSONのbefore/after SHA mapで固定し、実行中はactual sourceを凍結した。

原captureのtrailing spaces・末尾改行も証拠として保持するため、Gitには`.log.gz`を保存し、`log-storage.json`で圧縮前後SHA256を固定する。各run JSONの`log_sha256`は**展開後の元byte**に一致する。ローカル`.log`は検査時の元byteのまま残し、改行整形しない。新cloneではgzip展開して同名`.log`へ戻せる。過去run labelのcontrollerを上書き実行せず、新outディレクトリ/labelで再現する。
