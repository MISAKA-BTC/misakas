# S3 TFO3受賞Ticketの難易度

保存時点はDAA 6445。停止中の3ピアのRocksDBを読み取り専用で開き、ブロックヘッダー、ジャーナル、carriageから復元したstate rootを照合した。全Ticket、当落、beaconと使用期限は、先行claimについて3ピアが実際に返した `s3-terminal-draw.json` に基づく。現在のclaimはDAA 4002で受付、DAA 4324でReceiptLicensed。DAA 7122までの通知期間が残っている。

両claimの計算量は56,924,364、quantum数は21、切り捨て後の1枚あたりの計算量は2,710,684。ネットワーク基準は2,707,216。WとW₀はともに355,649,611,200。DAA 3293と6445でpooled targetは経済的上限に一致した。隔離S3の倍率16,384を含む実効targetは42,492,857,642,785,034,246,503,491,573,461,008,384で、先行claimのネイティブ抽選結果に一致する。

128 bit Ticketが一様で各indexが独立という仮定で、1枚の当選確率は `p = (target + 1) / 2^128`。同じtargetで21枚抽選できる新規claimの当選確率は `1 − (1 − p)^21`。1枚で12.4875%、1 claimで93.9261%、期待1.0647 claim。先行claimは21枚全落選で、最小Ticketも使用期間内の最大targetを超えた。2件目の値は現時点の価格での事前確率であり、実際の抽選にはFinal、独立Gate Cとbeaconが必要になる。

先行claimの受賞候補時点の支払必要額は320,084,650,080 sompi、export capは405,180,000,000 sompi。1件支払った後の上限差額は85,095,349,920 sompi。2件目のDAA 6445での金額も同一で、残余reward予算は6,721,777,651,680 sompi。実際の支払いには受賞時点の予算判定が必要になる。先行S2は4枚が当選し、export cap 25,323 sompiで候補を拒否した。

## Epoch境界での調整

| 境界DAA | 閉じたepochのAttempt | 受賞 | pooled targetの結果 |
|---:|---:|---:|---|
| 1000 | 4 | 0 | 128 bit最大値まで緩和。同じブロックの後段で初回のWを設定 |
| 2000 | 0 | 0 | 空の集計で早期終了。最大値を据え置き |
| 3000 | 2 | 0 | Wに基づく経済的上限まで引き下げ |
| 4000 | 1 | 0 | 経済的上限を維持 |
| 5000 | 6 | 0 | 経済的上限を維持 |
| 6000 | 2 | 0 | 経済的上限を維持 |

空epochの早期終了はS3でも発生した。DAA 1000でpooled targetを更新した時点ではWが未設定で、Wを初期化した後に上限を再適用する処理がない。DAA 2000でもその高い値を持ち越した。DAA 3000で再び集計対象が生じてから上限が適用された。これは初期の抽選を易しくしうる挙動で、先行claimのFinal（3395）とbeacon（3821）は上限適用後にある。今回の診断で既存S3の合意パラメータを変更する必要はない。

短時間のネイティブ状態遷移試験は終了コード0、試験本体0.01秒。Wを2倍にした場合、Attemptあり・受賞なしの境界で新しい上限までtargetを下げることも確認した。保存台帳のジャーナル全件を逆順に読む試作は300秒で打ち切った。確定した選択親とmergesetを使い、既にseed済みの同一epochで自ブロックとmergesetがすべてheartbeatの場合に対象行の読み出しを省くと、3ピア照合と6境界の採取を含む診断全体が47.34秒で完了した。

## 再現

T12 worktreeのルートで実行する。引数は保存したdevnetのホーム。スクリプトがそのスナップショットから同じgenesis設定、各ピアのデータディレクトリ、使用するsaltを特定する。

```sh
export CARGO_TARGET_DIR=target/protected-core-tests CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 RUST_MIN_STACK=67108864
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib the_pooled_target_walks_on_the_lanes_output_and_never_past_the_price_ceiling
cargo +1.93.0 build --locked --offline -p kaspad --features private-integration-clock --example tfo3-consumed-receipt
python3 docs/evidence/palw/tfo3-genesis-award-2026-11-24/read-s3-difficulty.py \
  target/protected-core-tests/debug/examples/tfo3-consumed-receipt \
  "$HOME/.local/share/misaka-tfo3-award/network-s3"
```

## S2の署名付き受賞直前fixture

停止したS2の選択親を読み、実行者の隔離鍵で21個のPFS3 envelopeを作る。署名位置はfixtureの入力で、ブロック提出は行わない。選択親からGHOSTDAGを遡ってbeaconを導出し、現行の署名・Gate C・期限・Ticketの合意判定を通す。候補DAAの補助金と担保予算を別に照合する。DAA 3741で index 5、8、11、19 がTicket判定を通過し、4枚とも期限翌日の判定で OutsideUseWindow になった。実際の受賞に必要な320,084,650,080 sompiをこのclaimは支払えない。[署名付き照合結果](s2-signed-admission.json)

S2を停止した状態で、上のコマンドでビルドしたreaderを使用する。

```sh
s2="$HOME/.local/share/misaka-tfo3-award/network-s2"
db="$s2/run-1795500358368189000/peer-0/misaka-testnet-12/datadir/consensus/consensus-001"
claim=$(jq -r .claim docs/evidence/palw/tfo3-genesis-award-2026-11-24/s2-native-spend-preflight.json)
target/protected-core-tests/debug/examples/tfo3-consumed-receipt --cold-signed-draw "$db" \
  "$s2/bootstrap/protected-policy.json" "$(jq -r .registration.salt "$s2/run.json")" "$claim" "$s2/bootstrap/bond-0.seed"
```

## 担保を予約した現行Gate Cの短時間fixture

`kaspad/tests/tfo3_funded_receipt.rs` は固定した経済設定と合成の実行記録から、1席Panelの署名、Final、独立Gate Cの75抽選・41席閾値、固定beaconでのTicket判定を通す。未認証の支出を拒否し、当選quantumから作った署名付きPFS3を処理する。claimの予約から62 sompiと1ブロック分を消費し、二重消費を拒否する。残額では同額の2回目を賄えない。受理したbondの支払先でcoinbaseを組み立てて照合し、受理DAA 198のworker UTXOを得る。後続attemptがFinalを経てDAA 291で独立認証されると、決済frontierは198に達する。受賞と決済の差分について巻き戻し・再適用と保存形式からの復元を照合する。

この試験の入力は合成ヘッダー・mergeset・実行記録。実LLM、通常submitBlock、P2P初回伝播はS3実走の記録で判定する。2026-11-24の実行は1件成功、終了コード0、試験本体0.91秒。T12 worktreeでの再現コマンドは以下のとおり。

```sh
export CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=target/protected-core-tests RUST_MIN_STACK=67108864
cargo +1.93.0 test --locked --offline -p kaspad --features private-integration-clock \
  --test tfo3_funded_receipt -- --nocapture
```

## S3で照合する実装経路

| 段階 | コードと照合対象 |
|---|---|
| Studioからclaim受付 | `studio_lifecycle`・`follow_studio_queue_submission` がStudioのジョブ、outbox、署名された実行記録を結び、`studio_admission` が選択チェーン上の受付を読む。受付済みチェックポイントの再開は `resume_admitted_studio` を通る。 |
| Panel・Court・Final | `observe_admitted_studio` が当該claimと実行者を固定し、独立再実行、承認carrierの採用、反証の有無、Finalを照合する。 |
| 独立Gate C | `try_attest_claim_certification_v1` と `sweep_security_certification_v1` が抽選席の署名、判定、訴追期間を処理する。認証DAAを基点に受賞用のdraw slotを求める。 |
| Ticket・署名 | `tfo3_draw_view` がbeaconと各indexのTicketを算出する。`PalwProducerService::produce_receipt` は現在の選択親と候補DAAで支出可能な当選quantumを選び、同じテンプレートに実行者の署名を結ぶ。 |
| 通常受理・P2P | `submit_receipt_block` とRPCの `submit_block_call` は同じ `FlowContext::submit_rpc_block` に入る。ここで `validate_and_insert_block`、P2P通知、virtual更新を行う。実走での初回受理の証拠は `receipt_delivery` と3ピアの選択チェーン読み戻しに残る。 |
| 一回消費・予算 | claim受付時に仕事キーを索引化する。`apply_receipt_spend` が二重消費を拒否し、`bond_budget_receipt_spend_v1` の予算更新とquantum消費を同じ遷移で反映する。仕事キーの所有者は元のclaimのまま。 |
| UTXO・決済 | `CoinbaseManager::expected_coinbase_transaction` が受賞を支払う子のcoinbaseを構築する。`receipt_payout` が後続ブロックでのcoinbase受理とUTXOを照合する。`receipt_settlement_view` がそのUTXOの受理DAAを指定して `getPalwSettlement` を読み、後続attemptの認証に基づくfrontierを3ピアで照合する。 |
| 巻き戻し・再起動 | `apply_delta_v2`・`revert_delta_v2` が仕事キー、quantum、予算を含む遷移を再現する。`palw_devnet_reorg.py` は保存台帳の複製上に競合枝を作り、同一受賞を含む共通prefixの支払い・消費・決済と、受賞ブロック自身の差分を検査する。 |

実装の入口は [Studio・3ピアの観測](../../../../scripts/drills/protected_private_registration_run.py)、[ブロック生成](../../../../kaspad/src/palw_producer.rs)、[通常受理](../../../../protocol/flows/src/flow_context.rs)、[claim・受賞状態の遷移](../../../../consensus/core/src/palw_state_v2.rs)、[決済](../../../../consensus/core/src/palw_settlement_v1.rs)。

## 実claimの受賞と60分停止（2026-11-25）

DAA 6445のスナップショット `eb54edd4f11d485594811d5e45b58a66` を起点に、受付済みのStudio claimを再開した。追加のStudio推論は0件。1席Panelの承認済みclaimはDAA 7123でFinalになり、DAA 7766で独立Gate Cの75席が抽選された。3署名が42席を占め、DAA 7795に閾値41を満たした。

DAA 7866のbeaconで21枚中index 0と6が当選した。index 0のTicketは `13913882171893198229823730785476142575`、実効targetは `42492857642785034246503491573461008384`。同じbeaconを親に署名したPFS3受賞ブロックのhashは `bb8a1337a59c242e442dd0f0c3bc0a1e905849baabb533ec0d4377713021200c5e436cea49254f7ba756386bbc5e527d163a7b82ed0f08796421f72cbea587eb`。peer 1の初回受理はsubmit block、peer 0・2はrelay。支払いを含めた子の後続ブロックで、3ピアが同じoutpointに320,084,650,080 sompiを確認した。受賞前後の予算差とUTXO額が一致し、ネイティブ再検査は同じquantumを `QuantumAlreadySpent` で拒否した。

後続attemptを制約したのは、基本クラスのモデル別ブロック配分枠（ADR-0177）。DAA 7194と7296のattemptは仕事の受付で `ModelBlockOpportunity` を返した。配分epoch 10はDAA 7280開始で、1ブロック分はDAA 7561、2ブロック分は7842に達した。7766の仕事がGate Cのanchorになり、7866のbeaconを経て報酬UTXOが7867で受理された。epoch 11ではDAA 8201で基本クラスの1ブロック分が発生した。この間も、受賞済みclaimの支払いと一回消費は選択チェーンに残った。

UTXOを覆える後続attempt `4501e672586bf6c9d7833296e53c4f2592f17e879ecfe44d55a4d6d3c74ceb197f9b2aee2bc0ec76f3d36f40e19cf6293f76521c4884f6c2bfd928d2dfa3456e` はDAA 8247で受付。停止前に3ピアで読んだDAA 8300ではProvisional、Panel未結成。DAA 8302の決済RPCは3ピアとも `settled=false`、frontier 7766、pending anchor 1件だった。同epochで基本クラスに次の1ブロック分が発生するのはDAA 8402。署名・Panel・Final・認証の進行にはそれぞれの合意条件がある。

3600秒で停止を要求し、終了処理を含む3615.97秒で終了コード130。新しいスナップショットは `9e327e58d2e04f2599b17b337e5a644b`。3ピアの停止DBを開き直し、DAA 8305の同一選択ブロック・同一rootを確認した。3ピアそれぞれで受賞ブロック自身の保存差分を逆適用して署名済みの親rootを復元し、再適用して受賞後rootが一致した。決済後の稼働ノードによる競合枝への切替は未検証。[識別子・21 Ticket・各ピアの初回受理・会計・決済RPC・停止DB検証](s3-second-claim-integration.json)

### 保存された受賞差分の再検査

次の読み取り専用コマンドは新しいブロックを作らず、冷停止した3ピアの同じ受賞を再検査する。T12 worktreeで実行する。readerのビルド方法は上の「再現」を参照。

```sh
set -e
pool="$HOME/.local/share/misaka-tfo3-award"
image="$pool/.network-s3.snapshots/9e327e58d2e04f2599b17b337e5a644b/data"
capture="$pool/run-1795531117263218000-funded-quantum-audit"
reader=target/protected-core-tests/debug/examples/tfo3-consumed-receipt
salt=$(jq -r .registration.salt "$image/run.json")
for peer in 0 1 2; do
  db="$image/run-1795508221092963000/peer-$peer/misaka-testnet-12/datadir/consensus/consensus-001"
  "$reader" --cold-journal "$db" "$image/bootstrap/protected-policy.json" "$salt" "$capture/award-applied.borsh"
done
```

今回の実走の入口は、元チェックポイントを保持したS3の `scripts/palw-devnet up --home "$pool/network-s3" --resume-node "$pool/s3-zstd-optimized-private-node"`。起動時の環境は `KASPAD_STORAGE_CAP_BYTES=16000000000`、`KASPAD_ROCKSDB_PRESET=private-palw`、`MISAKA_PRIVATE_HEARTBEAT_NONCE_DIR="$pool/s3-honest-nonces"`。Metal workerには `--seconds 3600 --hash-limit 268435456 --batch 16384` を指定し、同じnonceディレクトリを渡した。経過時間3600秒または空き3 GiBでローカル監視が `scripts/palw-devnet down` を発行する構成を使用した。停止済みの最新ホームをこの起点に戻す操作は、この読み取り専用の再検査には不要。

### 基本クラスの後続attemptに必要な枠

停止時に配分epoch 11の基本クラスが予約済みだった量は1,000,000 block units。次の1,000,000 unitsが発生するDAAは8402であり、署名によるGate C認証や決済の予定時刻を意味しない。直前に確定したfrontierのattemptは受付7766、Final 7989、認証8250。UTXOの受理7867を覆うには、別の受付済みattemptがFinalと認証に到達する必要がある。

修正後の `model-allocation` 診断は、報酬のsompi残高と独立にモデルの利用可能block units、次の配分境界、基本クラスの通常producerが使う配分モデルを返す。履歴行とブロック予約の内部行はモデル一覧から除く。隔離ランナーは、Gate Cの抽選または決済に受付済みattemptが必要で、同じ選択親のモデル枠が1ブロックに満たない間、producer専用の45秒待機枠を使わずに通常の進行を続ける。beacon観測はヘッダーの規則に従う。保存した実走は起動時の旧ノード・旧ランナーで得た結果。

```sh
PYTHONPATH=scripts/drills python3 -m unittest discover -s scripts/drills/tests -p test_protected_receipt_payout.py
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 RUST_MIN_STACK=67108864 \
  cargo +1.93.0 --config 'profile.test.package.kaspa-consensus-core.opt-level=0' test --locked --offline \
  -p kaspa-consensus-core --lib allocation_view_reports_only_models_and_their_remaining_block_units
```

2026-11-25の最終確認はランナー21件が0.123秒、ネイティブ配分照合1件が0.02秒で、いずれも終了コード0。変更後のconsensus-coreの型検査も終了コード0。

### DAA 8305の停止台帳で特定したPanel待ち

3ピアの保存DBを読み取り専用で開き、各tipのジャーナルrootでcarriageを検証し、UTXO受理DAA 7867に対して合意の `palw_settlement_v1` を実行した。3件の返信は全フィールド一致。frontierは7766、depthは0、未確定の後続attemptは1件、未認証Finalは0件だった。完全な返信は `s3-second-claim-integration.json` の `afterStop.coldSettlement` に保存した。

後続attemptの受付・bind baseは8247。G14のspan長は1 DAAで、停止時に保持していたseed anchorのspanも8247だった。G14はspan先頭がbind baseを厳密に超えたanchorを要求するため、このattempt自身のseedは使えない。表示された8267は旧方式のanchor slotで、G14のPanel選出日ではない。bind期限は8847（このDAAを含む）。基本モデルの次の配分境界8402はこの期限内にある。そこから通常のattemptが受理されて新しいseedを供給できるか、その後のPanel承認、Final、Gate Cまで進むかは実走で確認する条件として残る。

隔離ランナーは受賞UTXOを覆うattemptをexecutorの通常RPCで照会する。G14の後続seedを供給できる場合と、Final後のGate Cの抽選が未実施の場合に、配分枠がある選択親を通常producerに提示する。既にseedがあるときや署名待ちにこの待機枠を使わない。再開入口から通常producerのbondを引き継ぐ。DAA 8305のホームを読み取りだけで `resume_options` に渡すと `resume_settlement` が選ばれた。

上の読み取り専用手順と同じ `image`、`reader`、`salt` を使用する。最後の引数は保存した報酬UTXOの `output.blockDaaScore`。

```sh
for peer in 0 1 2; do
  db="$image/run-1795508221092963000/peer-$peer/misaka-testnet-12/datadir/consensus/consensus-001"
  "$reader" --cold-settlement "$db" "$image/bootstrap/protected-policy.json" "$salt" 7867 || exit 1
done
```

この検査の各終了コードは0。決済ランナーの短時間試験は22件、終了コード0。新しい診断を返す隔離ノードは `cargo +1.93.0 build --locked --offline -p kaspad --features private-integration-clock --bin misaka-rfc12-private-node` でビルドする。実走の60分枠を消費した後、この照合ではS3を起動していない。

### 追加30分枠での決済追跡

DAA 8305の同じ受賞と報酬UTXOを追跡した。全再開で追加Studio推論は0件。各回で3ピアの再起動後の受賞claimは認証済み、消費済みindexは `[0]`。診断ビルドから前回使用した高速ビルドへ切り替える前に、両者のprivate profile出力が一致することを確認した。終了時刻を最初の起動時から固定し、Metal workerの切替時は前workerのバッチ数の上界を総上限から引いた。後半はノード内蔵のCPU heartbeat採掘を使用した。

- DAA 8402で通常producerが次のattemptを生成し、受付8247の後続claimにG14の1席を確保した。DAA 8406にReceiptLicensed、監査後のFinal期限は8526、Final記録は8527。DAA 8532で3ピアがこのFinalとGate C保留を返した。
- 最終停止時はDAA 8602。3ピアの冷停止DBに対する `--cold-settlement` は同じroot、frontier 7866、未認証Finalの後続attempt 1件、進行中の後続attempt 1件を返した。報酬UTXOの受理DAAは7867で `settled=false`。この1 DAA差は、あと1ブロックで決済されるという予測には使えない。
- 基本モデルでは同一epochの2枠を8247と8402で使用した。記録された配分重みでの3枠目の計算値は8604。受理された後続attemptによる独立Gate C抽選、署名・認証、決済の実走確認が残る。最後の実走は3 GiBの空き容量下限で停止し、残り時間が3ピアの再起動所要時間を下回った。
- 報酬支払をその実行中に3ピアで確認した後の監視では、既知のUTXO受理ブロックの選択状態と決済frontierを先に読むよう変更した。受理ブロックの離脱または決済成立を検出したら3ピアの証跡全体を再検証する。短時間試験23件、終了コード0。最後の再開にはこの変更を使用した。

詳細は `s3-settlement-continuation.json` に記録した。再現には上記の読み取り専用ループで `image="$pool/.network-s3.snapshots/ef7c7a37e92c440fa3ef4f487455f1b8/data"` を指定する。DB用のrunディレクトリと引数7867は同じ。元の6445、8305の各スナップショットも保存している。
