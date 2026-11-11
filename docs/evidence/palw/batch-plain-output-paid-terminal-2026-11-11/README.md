# Paid plain Output receipt and terminal — 2026-11-11

Source `557edb41194922a43816c81b995fe5e3d197a303`。**core 5件、CLI 3件、RPC 1件が成功**し、Node/RPC/CLIの最終 `cargo check` も通った。[集計・来歴・全digest](summary.json)とrawログを保存する。

実際の署名付き Shared notice、専用 FullIds quote、BCT1 Open/Begin/Part/Finish、BCT2 Verifyによる受理済み `OutputRootFault` receipt、そのreceiptに基づくBCT2 Terminalを同じfixture claimで通した。終端は `CourtFraud` とoffence消費を記録し、owned事前実行と確定dispatcherのstate rootが一致した。body保管料とVerify receipt保管料の支払分離も確認した。別digestは状態を変えず、予約枠不足は `NativeResource` のまま状態・journalを変更しない。

初回R1は、旧逐次計画のテスト用期限と前提準備によりBCT2より前で停止した。fixtureは既存の長いoriginal clockと署名済み履歴保持pledge・semantic bundle writerを使用するよう修正した。公開数値とBCT2自身のhard windowは変えていない。

R2はFullIds搬送とfault receiptまで通ったが、通常起動時のTerminalが `candidate prefix has no audited native owner` で停止した。optional native制限が無効だと旧 `value_requests_v1` は0を返すためである。修正はBCT2 prefixを常にtyped sizingし、通常起動時にもworkspace予約を保持するもの。canonical hash writerの拡張余地とterminal evidenceのメモリも計上した。実メモリ予約量に依存する上限超過は、semantic refusalに変換しない。R3はoptional制限を有効にせず、隔離processでこの経路を検査する。

FullIds専用 `batch-open-plain-output` はoriginalから決めた一つのcanonical rangeを使い、旧Stopとは異なるordinal `u64::MAX - 1` を使う。Nodeは認証済みselected parentからquoteを作り、CLIはnetwork・claim・notice・count・range・ordinal・親root・現在DAAを照合する。`batch-respond --ordinal 18446744073709551614 --target full-ids` の既存応答搬送に接続し、その後 `batch-verify-plain-output`、faultがあれば `batch-convict-plain-output` に進む。いずれも実送信は `--submit` を指定した場合で、既存の予約DAAと直前quoteの一致を要求する。mempool送信を受理済みとは表示しない。

検査の設定は `CARGO_TARGET_DIR=/tmp/misaka-public-contract-test.fd7edK CARGO_INCREMENTAL=0`、`--config 'profile.dev.package.kaspa-consensus-core.opt-level=0'`、`--locked -j2`。core/RPCは `test -p kaspa-rpc-service -p kaspa-consensus-core --lib --no-run` 後、生成binaryに `bct2_ --nocapture` と `numerical_batch_open_selector_is_bounded_and_exclusive --nocapture` を指定した。core worker stackは64 MiB。CLIは `test -p misaka-cli --bin misaka plain_output_ -- --nocapture`。CLI binaryはR3内部資源予約修正の前に生成したが、CLI/SDK/quote operandsは同一で、最終combined checkは修正後全packageを検査する。Nodeの初回checkはquery target変数のshadowingで失敗し、修正後通った。

**範囲:** original/class/custody/サービスgraphは局所fixtureであり、検証済みUTXO handoffもfixtureが供給する。実script/P2P/DAGや公開originalの受付を実行していない。誤ったOutput rootをoriginal作成前に固定した不正系で、正常claimのFinalを示す試験ではない。live legacy courtのarchiveはこのfixtureに含まれず、nested checkpointとclaim/session/deadline索引、全Selection/DA、受理後の他state成長に依存しない資源境界は引き続き未完である。公開profileの数値採択・有効化・送信・main pushは行っていない。
