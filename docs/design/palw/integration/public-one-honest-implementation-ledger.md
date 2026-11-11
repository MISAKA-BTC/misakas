# PublicOneHonest 実装台帳

[実装方針書](public-one-honest-vertical-implementation-plan-2026-11-10.md) の I0 作業記録。出荷設定と公開ネットワークでの有効化はこの台帳の対象外。以下の既存コードは接続先であり、新 profile の検証結果は未取得。

## 経路と接続先

| 経路 | 既存の接続先 | 新 profile の残件 |
| --- | --- | --- |
| 正常 claim | [Miner 推論](../../../../misaka/palw/remote/src/positive_miner_v2_inference.rs)、[一回の実行からの capture](../../../../misaka/palw/tir/exec/src/node/positive_wide_material_v2/capture.rs)、[Result 固定後の選出と認証](../../../../consensus/core/src/palw_state_v2/positive_assignment_certification.rs) | 同じ実行の初回 trace と manifest を Miner が署名。公開 RPC、native original、選出 bond による全体承認、局所検査、Final を接続 |
| 資料不足 | [Panel 取得キャッシュ](../../../../misaka/palw/remote/src/positive_panel_sources.rs)、[共有資料 intake](../../../../consensus/core/src/palw_court_shared_material_intake_v1.rs) | manifest 入口から全 scope の取得と検査機会までの profile 手続き。各申立人の Q、供給・防御応答、検査・提出枠を原時計で保存 |
| 一人の反証 | [TIR court](../../../../consensus/core/src/palw_tir_court_v1.rs)、[公開裁定 fold](../../../../consensus/core/src/palw_tir_public_fold_v1.rs) | Miner の初回 root に対する関係別 checker、Q 継続、元 claim の失効、依存取消、帰責別の費用・返還・slash |
| 復元 | [virtual processor](../../../../consensus/src/pipeline/virtual_processor/processor.rs)、[PALW state store](../../../../consensus/src/model/stores/palw_state_v2.rs) | original、draw、Q、予約、認証、Final、credit を同じ delta で保存。restart、reorg、pruning、IBD の経路から復元 |
| credit と決済 | [native settlement](../../../../consensus/src/pipeline/virtual_processor/native_settlement.rs)、[EXEC fold](../../../../consensus/core/src/palw_exec_v2_fold.rs)、[round producer](../../../../kaspad/src/palw_round_producer.rs) | 同一 canonical 単位で Q・反証・依存失効を先に評価し、Final 後に一回だけ credit を発行。予約、ScheduleId、permit 消費、取消を接続 |

[共通契約](../../../../misaka/palw/positive-contract/src/public_one_honest.rs) は、元 claim、全体承認、manifest 自体への要求、共有 MaterialId と個別 CaseId、原受理からの期限、抽選前の救済容量の算術を定義済み。native の受理・認証・予約・決済は未接続。既存の署名ドメイン、wire 値、Gate C、二階層の意味は各版で維持する。

## 全実行関係の対応を確定するための台帳

| 関係 | 初回記録から固定する対象 | 接続と測定が必要な項目 |
| --- | --- | --- |
| WellFormed | raw 記録、schema、長さ、型、座標、各 root | 外側 packet の拒否と Miner が署名した raw の違反を分離。最大 parse、bytes、hash |
| Init | program、artifact、tokenizer、prompt と token 列、seed、初期 KV と依存 | 宣言の同一性、入力・状態の正規化、初期 opening。最大 I/O、heap |
| Step | read-from、演算・index・shape、区間還元、state 更新と非更新部分、RNG | 各 kernel の数値意味、初回 trace の区間境界、局所 witness と checker。最大算術、heap、bytes、hash、I/O |
| End | token 選択、停止、出力、長さ、仕事会計 | 最終 state と公開結果の対応、停止規則、会計導出。最大算術、parse、I/O |

対象 profile の実アーティファクト、program、tokenizer、数値規則の digest と、各行の実際の kernel・witness・上限が揃うまで coverage は未確定。全体再実行は Panel 側で行い、Node は個別関係の有界な検査を担当する。

## 資料 scope と認証経路

| scope | 現在コードで使える認証 | profile を接続するための残件 |
| --- | --- | --- |
| Artifact | 既存の inventory root と tensor・layer・row の opening | 可変長の leaf 座標を manifest の要求・予算に結び付ける |
| Program | 公開単一チャンクを元 claim の graph IR root と照合して正規 decode | native の供給義務と取得予算へ接続する |
| Prompt | 公開 UTF-8 原文を認証し、元 claim の tokenizer で再トークン化する | profile ごとの前処理と局所検査を固定する |
| InputTokens | 再生成したトークン列を Flat または Merkle の native commitment と照合する。wide 再実行は Flat に対応 | Merkle の wide 再実行と native の取得予算を接続する |
| Tokenizer | 公開 tokenizer JSON を native の raw commitment と照合してから parse する | 局所検査と native の供給義務を接続する |
| ExecutionRules | ジョブと元 claim の CU ruleset ID を照合 | 乱数・選択・停止・算術規則を実行 profile の資料として固定する |
| InitialState | 元 claim と rooted 入力の digest を照合 | 初期状態と先行実行からの読み込みの正規資料を取得・検査する |
| Dependencies | 元 claim と rooted 入力の digest を照合 | 空集合を含む正規形式、依存 ID の公開・失効経路を接続する |
| Trace | Miner の確定処理は初回 capture のタイルと opening を公開ストアに書く。Panel は公開索引と全タイルを有界取得し、元 claim と独立再実行に照合する。葉数と wide logits の裁定結果は元 claim ID と証拠 ID の組で参照できる | Miner と Panel の daemon への接続、全関係の局所化、裁定結果から native 状態更新への接続 |
| Output | 公開の生成 token 列を native output root と照合し、wide 再実行と比較する | 停止・生成長・会計を含む End 検査を接続する |

I1 のローカル統合テストでは、Miner が wide TIR を一度実行して署名・公開し、Panel は signed original を入口に HTTP で manifest と artifact の行を取得し、登録済み inventory root を検証して別の実行用テープで同じ original を再実行する。artifact の行や公開 trace のタイルが欠けると資料不足で停止し、改変された artifact も拒否する。全ノードを commit するステートレス wide TIR の検査計画について、同じ original と抽選済み bond に束縛した一括承認を再実行後に署名する。統合テストではその承認を native の carrier 検証に渡す。native fold は予約 ID 付き original の受理記録を包含点と照合して保存し、抽選点で draw を保存する。検証済み承認 carrier は包含点と保存済み draw が一致する場合に一 bond 一行で記録する。これらの差分適用、巻き戻し、スナップショット復元をローカルで検証済み。daemon での受付・予約の取得、抽選、必須局所検査、Final、決済は未接続。

I3 の native fold は、抽選された bond の署名済み manifest 要求を、受理済み original、抽選記録、包含点、予約に照合して未解決 case に保存する。テストでは署名済み要求から差分適用・巻き戻し・復元を通し、異議期間後も case が残る限り Final 前の判定が停止することを確認した。認証済み provider 応答も、同じ case を開いたまま保存し、復元時には実際に受理された carrier を再認証する。部外者枠の割当、daemon の carrier 入力、case 終結は未接続。

固定長 manifest の取得と、資料供給義務に結び付いた署名済み応答の認証は別経路。前者は元 claim と資料の同一性、後者は当該 case への応答の帰属を確認する。どちらも単独では検査機会を成立させない。

## 接続時の判定

- 初回 wide capture の元 claim には、step tree と logits trace を束縛した native execution root と、その実行の output root を設定する。現行 capture が要求する Flat 入力は、manifest の単一チャンクから native flat hash で照合できる。Merkle 入力には既存の token tile opening を使い、実行側の対応は別に確認する。
- Program は正規 TIR v1 全体を単一チャンクとし、元 claim の graph IR root を既存の native hash で照合してから decode する。Artifact inventory の leaf は tensor・layer・row 範囲に対応し、入力 Merkle leaf は token tile に対応する。各 leaf を固定幅のバイト範囲と同一視せず、それぞれの座標と既存 opening を結ぶ codec が必要。
- 資料手続きは未確立。署名、URL、別 Panel の receipt、部分的な chunk、期限の経過から全 scope の検査機会を認定しない。各 case に認証できる供給根拠、本人の取得・検査・提出機会、残予算が必要。
- Final の入力は native state から導出する。全体承認、局所検査、notice 終了、検査機会、Q、依存、責任、資金、原期限を一か所で評価する。反証・失効・Abort を成功へ戻さない。
- Final の consumer は、仕事報酬、escrow、worker・service 支払、mint、安全 frontier と weight、settlement depth、Rule E、settled anchor、EXEC credit と予約、schedule、permit、依存の export と use。供給済みサービスの対価と返金は固有の条件を保つ。
- 検証 runner は通常の V1〜V4 と長時間の V5/V6 を分ける。対象 profile、ケース名と件数、実アーティファクト、機材、必要資源上限は候補固定時に記録する。現時点の V1〜V6 は未実施。

公開 T12 の Studio TFP1 は別途、現在の G14 ルート上で同じ claim ID の受理、対話、Final、ブロック生成を照合する。新 profile の契約や既存の T12 復旧試験だけで、この結果を記録しない。

2026-11-10 の公開 T12 では、同期済み seat 7 の受付見積もりが DAA 20,850 で対象の Studio claim をクラス未登録として拒否した。復旧後の seat 5 の作業ブロックは別の claim を含む。Studio の登録見積もりは取得済みで、登録の署名、受付、対話、Final、ブロック生成の記録は未取得。

同日 DAA 20,860 の seat 5 でも、同じ Studio データで登録事前検査は通り、claim の見積もりはクラス未登録として拒否された。新 profile の original 受け入れ記録は合意状態の行、差分適用・巻き戻し、スナップショット復元に接続済み。公開 RPC で受理された署名付き original から、同じ状態遷移で供給・保持を予約して記録を書く処理は未接続。

隔離 Linux ノードの旧ビルド `fa449130a` は同日 DAA 5,057 で、公開 Explorer の DAA 20,868 と pruning point は一致したが同期未完了。IBD は `unsupported native cold tip capacity` を繰り返し、一部の再試行では `IbdChainBlockLocator`／`BlockBody` の待受に別の応答が到着した。公開側は同期済み。新ビルド `244b69d93` で確認する対象は、この隔離環境での IBD 完了と公開側の sink への合流であり、Studio claim の実証とは分けて記録する。

公開 T12 の DAA 20,944 で、Studio 候補のクラス登録 tx `86f7e37feabecda0b80f475bc7d2bec8e7de73e07cb82fa90e3f88b5d27b70ba5c4e4bd5f3863aaac7c174fbfadb55a66c3e6afcb42eeb9d14a6d11b7a6d08a6` を中継した。ローカルの同期済みノードを pin の出所として owner 証明を照合し、署名経路の判定は `VERIFIED_REMOTE`。2 ノードが中継を受理した。確認時点では mempool 受理、ブロック包含と registry fold は未確認。この tx はクラス登録であり、Studio の TFP1 claim の提出記録とは別に追跡する。

同じ登録 tx の搭載ブロックは DAA 20,945 の `6a067b8fbcd7cdb9db13ecf66829b5d3175df1bf253e48a7cb59d94f2b12339c2f676d0fc8dab180203ed99a33c5078c4bd509893a69815f4e00634bd16b7156`。次の受理ブロック `60766c89bcb72a1c462b8b1363a8caf61a1cf4f9308ecb406b331369a5100dcc11d9f8d012717d8d1bd12f744797f88eb979956354ab58ae416aa88a80caeeab` の受理 tx 一覧を seat 5 と Explorer で確認した。DAA 20,946 の登録状態は `ADMISSION_OK`、`Candidate`。同じ未提出 TFP1 候補の見積もりは `TIR_CLASS_CONTAINER_UNBOUND`。

登録済み inventory に一致する PALWART3 をローカルで検証した。container root は `bb10e9a486a4d4f0b925907b7579056cb799380aef404218a5d8bd99e54a502b3e370cd041d651df5c471da81094675cb46b2b5e51a7002bfeeadcdffd8da20c`。既知の未提出 TFP1 候補の署名対象には container root が含まれず、タグ 175 の紐付けで候補 ID は変わらない。固定長署名を仮定した紐付け carrier の手数料見積もりは 469,737 sompi。登録後の producer bond は観測時点で下限に 100,000,000 sompi 足りない。紐付けと補充は未送信。

再同期の隔離カナリアは 223,472 ヘッダーを取得し、検証済み sidecar を再取得せず同じ pruning point の UTXO 取り込みを再試行した。9 GiB cgroup で native RAM 要求 7,420,392,666 bytes が内部上限 6,764,573,487 bytes を超えた。プロセス起動時に有効容量の 70% を内部上限とするため、同じ隔離 DB を 12 GiB cgroup で再起動した。同期完了は未確認。

2026-11-10 12:57 CET の公開 seat 5 では、他ピアからのブロック受理は継続していた。seat 5 自身の producer は、設定クラス `f1c5635c…f623c8` について court の hard-deadline bucket 0 が 21/21 に達したと報告した。このログは当該クラスのローカル生成停止を示す。Studio TFP1 候補の受付や生成の証跡は未取得。

同日 13:12 CET に隔離カナリアの pruning 取り込みは完了し、RPC は epoch 18、phase `idle`、`pruningImportStable=true` を返した。DAA 5,187 で `isSynced=false`。その後の virtual 更新で native RAM の予約済み 3,124,136,432 bytes に要求 7,721,437,122 bytes が加わり、上限 9,019,431,316 bytes を超えた。受信 lease の寿命修正を別途検証する前に、同じ隔離 DB と同じ旧バイナリを 16 GiB cgroup で起動し、RAM 上限だけを変えて検証する。隔離状態を解除するフラグは付けていない。

同日の Studio T12 送信履歴の再監査では、outbox に署名済みの commitment と受付票が各 1 件あった。署名済み carrier の txid `4453a999…43afe` から claim ID `28fbc2b0…71afc3` を再導出し、payload は旧 `PalwFpCommitmentTxPayloadV3` として decode できた。TFP1 framing はない。DAA 18,561 の受理ブロックは公開 seat 5 と Explorer で照合した。Studio の TFP1 実機レポートは `tfp1CarrierSubmitted=false`、`claimId=null` を記録している。別の候補 `7645bc8b…a3711` は outbox 外の未署名データで、公開 DAA 20,969 の claim 照会でも見つからなかった。

公開 seat 5 の DAA 20,973 の fence schedule では `palw_tir_container_identity_v1` と `palw_tir_public_conviction_v1` は DAA 18,400 から有効で、`palw_class_semantics_v1` は未設定だった。未提出の Studio 候補 `7645bc8b…a3711` の native route 再計算では `FpSelection` と `FpTokenizer` が必要で、後者にはクラスの raw tokenizer 資料が必要。公開 T12 の出荷設定を使う [既存の horizon 回帰テスト](../../../../consensus/core/src/palw_tir_public_allocation_v1.rs) は、仮に最小の tokenizer inventory を与えても、現行の token-table 上限に基づく予約だけで 3,120 DAA の責任期限を超えることを検証する。このテストは出荷設定の制約を示すもので、未登録の紐付けを仮定した実候補の公開 quote ではない。

公開 seat 5 の DAA 20,974 では `palw_court_family_supplier_v1` も未設定だった。旧経路は token table の上限 256 MiB を 64 KiB 単位で扱い、開始・終了を合わせて表だけで 4,098 move を確保する。少なくとも 1 DAA/move の出荷設定では 3,120 DAA に収まらない。[既存の認証済み table 経路](../../../../consensus/core/src/palw_tir_public_fold_v1.rs) は、該当する family supplier の設定と、クラスの immutable semantics に一致する正規 table を要求する。単にクラスの宣言サイズを小さくして予約を減らす経路はない。

保存済み commitment から同じ `7645bc8b…a3711` を再導出して、実際の tokenizer と認証済み table（578,215 bytes）でオフライン診断を実行した。DAA 20,974、custodian 上限 3、EOF certificate は fixture。family V4 の要求 8,924,271 DAA は上限 3,120 DAA を超え、そのうち tokenizer 段階の予約は 8,418,420 DAA、資料取得の予約は 72,182 DAA。custodian 上限を 2 または 6 としても要求は 8,841,425 または 9,172,926 DAA。実データのキー処理 98,471 件だけで計算しても、現行の 2 move/event・4 DAA/move では 787,768 DAA。公開 seat 5 の DAA 20,981〜20,982 での実 quote は `TIR_CLASS_CONTAINER_UNBOUND` で拒否し、同じ ID の受付、court、lifecycle の行はなかった。選択チェーンには DAA 20,981 のブロックがあったが、この候補との関連は確認できなかった。

`positive-panel` を有効にした remote crate のテストを実行し、270 件成功、長時間テスト 1 件は ignored。HTTP 統合テストでは、Miner が署名して公開した同じ original に対して、Panel の取得・不足時の署名済み要求、provider の応答 packet の生成と認証、同じ original の独立再実行まで確認した。試験の accepted origin は fixture。公開 T12 の carrier 受付、予約、Q の終結、Final はこの結果の対象外。

公開 seat 5 を DAA 20,987〜20,988 で再照会した。同期済みの testnet-12 ノードは、同じ未署名候補 `7645bc8b…a3711` の original quote を `TIR_CLASS_CONTAINER_UNBOUND` で拒否した。FreePromptClaim、KernelClaim、ClaimDetail、CourtState、claim-lifecycle はすべて `found=false`。選択ブロック `9ced0332…885be` は DAA 20,987 で取得できたが、候補の受付ブロックは取得できなかった。

Linux の T12 復旧候補をリリースビルドし、バイナリの SHA-256 `3704030a2d230a2c74c20c4f8777706f08598e89988c2f8eb8cae3253f2d4c11` を転送先で照合した。seat 5 上の隔離カナリア `t12-exact-recovery-canary-151f9ce7` が既存カナリア DB を開き、保存済みの隔離状態を保持して起動した。初回 RPC では pruning import は epoch 18、idle、stable。カナリアは DAA 5,187、公開 seat 5 は同期済みの DAA 20,994。カナリアは protocol 108 のピアと接続し、IBD でヘッダー 56 件を取り込んで不足ブロック本体の検索に進んだ。公開先端への合流と Studio claim の受付はこの確認には含まれない。

同じカナリアは約 133 秒で不足本体 220,694 件を列挙し、最初の 10 秒間に 198 ブロックを処理した。仮想先端を更新する処理では保存済み PALW tip と UTXO walk の起点が異なる旨を記録した後、履歴ブロックの PALW lifecycle の再適用に進んだ。公開 seat 5 の DAA 20,995〜20,996 では同じ Studio 候補の original quote は引き続き `TIR_CLASS_CONTAINER_UNBOUND`。受付、Court、Kernel、lifecycle の記録は取得できなかった。

対象クラスの登録者鍵と公開状態の登録者公開鍵を照合し、検証済みの PALWART3 紐付け object を作成した。上限 469,737 sompi で公開 seat 5 に送信し、txid `2080b9fb04e67eb445d098f3f17325403d30c24bd82887674ce19d9582f1124103b8008a67349625206f2b02090bbaaf489abb6a31bbadab80cdffd577f459fb` の mempool 受理と実手数料 469,737 sompi を確認した。公開 virtual DAA 20,998 の照会時点では選択チェーンの受理一覧にこの tx はなく、同じ TFP1 候補の quote は引き続き `TIR_CLASS_CONTAINER_UNBOUND`。この tx は紐付け用であり、TFP1 carrier は未送信。

紐付け tx の収録を DAA 20,998 の `bb3b51f45e327b10661de4ddf9815b8fe8a44bf8641488eecb585093af0fbd482e5b5444f3cadb2aa266c2268d4f13131797df8fb70f1da32b7c5fab724e24e9` で確認した。公開 DAA 20,999 の `bdacc76cb9a2a910f820cf7c4be13db74fd8a52bf1d250340e57ca76d8936b59c815ecf0f11978249c70de32a9004e65d17159c31a0fcd3444350362fb38e62e` の受理 tx 一覧にも同じ txid があった。同じ未変更 TFP1 候補の quote は `G14_ROUTE_ABSENT`（`FpTokenizer`）へ変わった。staged public court、legacy public filer、FP tokenizer match は有効だが、クラスの raw tokenizer 資料を宣言する `palw_class_semantics_v1` と family supplier は未スケジュール。quote は `quoted=false` のため、候補は引き続き未送信。

元登録者鍵で同じクラス・inventory root・tokenizer ID に対する `ClassSemanticsDeclaredV1` object を作成した。SHA-256 は `d3f4ed9508d82205408ebb3b5982f131bdc37d56956f1306ff578908be0fd61a`。公開 T12 の [受付検証](../../../../consensus/src/pipeline/virtual_processor/processor/palw_validate/class_registration.rs) は当該 fence が有効になるまでこの object を拒否するため、送信は保留。宣言の受理で raw tokenizer の有無を確認する段階と、実際の供給・裁定が期限内に収まることを検証する段階を分ける。

DAA 20,999 を仮定し、受理済みのクラス登録 carrier から class/layout を復号して、同じ commitment・binding・raw tokenizer を照合した。EOF 証明と semantics の受理は fixture。出荷時の価格・資源条件を維持し、共有受付クロック v5 とその hard deadline のみを使うオフライン比較では、custodian 上限 2 / 3 / 6 に対して `PublicRecoveryExceedsNoticeBudget` の要求がそれぞれ 60,730 / 72,565 / 108,087 DAA、許容はすべて 6,880 DAA だった。v5 の owner queue 待ちを外しても、この候補の公開回復経路は予算内に収まらない。

同じ候補について native の取得プランを直接分解した。custodian 上限 3 の v5 比較は取得 72,182 DAA、提出・収集 383 DAA。取得のうち 64,890 DAA は依存 cone の `StepRun` 21 回に対応し、1 回は最大 256 葉、4,219,029 bytes、3,090 DAA。logits の 12 タイルは 624 DAA。取得枠を短縮するには、同じ原 claim に対する依存資料の認証・供給を保ったまま、`StepRun` を順番に要求する経路を変更する必要がある。SDK example のビルドはローカルの空き容量不足で停止し、この内訳は既存の consensus-core ビルドにリンクした別のオフライン実行で確認した。

既存の共有資料経路が使う native 全 root 入力 cover を、同じ commitment、登録済み class/layout、binding に対してオフラインで実行した。原トレースは 5,154 葉で、G14 の見積もり用設定の 4,096 葉上限を超える。診断で葉上限だけを 5,154 に合わせても、依存探索が 262,144 edge の上限で停止した。現行の共有資料設定をそのまま流用する場合、取得時間とは独立にこの探索上限も満たせない。公開チェーンや設定には変更を加えていない。

同じ候補を、Batch の numerical range 投影が使用する逐次 root 探索にも渡した。見積もり設定では 4,096 葉上限で開始前に拒否。診断で葉上限を実数に合わせると、64 KiB と 256 MiB の workspace 枠では探索開始前に拒否され、512 MiB と 1 GiB の枠では 19 root の処理後に edge 上限で停止した。これは予約枠を変えたオフライン比較であり、実測 RSS、全 root の検査完了、公開 quote を示さない。

逐次探索を同じ候補で完走させるための edge 枠をオフラインで境界確認した。5,154 葉、512 MiB の workspace 枠、既存の per-root・座標・membership 上限のまま、総 edge 3,620,159 では 269 root 後に拒否し、3,620,160 で全 270 numerical root の探索が完了した。探索座標は 802,170、root と parameter の対応は 1,080。1 root 当たりの最大は入力境界 15 件、parameter 座標 4 件だった。callback は座標の件数だけを計測し、資料の wire サイズ・取得時間・裁定完了は計測していない。

全 270 root を再走査し、Batch の既存 `StepRun` 分割と parameter codec を使って response body 上限を算出した。Merkle prompt tile と生成 ID が必要な場合の body 枠も加えた。最も大きい root は葉 46 で合計 277,173 bytes（うち step 266,954 bytes）。1 root は最大 6 ranges、step の単一 range は最大 265,269 bytes。これは root 単位の資料本体の見積もりで、署名・carrier・再試行・service 枠・terminal を含む受付予約ではない。

数値資料の予約時刻を切り分けるため、1 root の全 6 ranges をそれぞれ最大 265,269 bytes とした保守的なグラフを、既存の `BatchCourtWindowV1::reserve_latest_notice_case_v1` に渡した。16 KiB 分割で parts 102 件、Open/Begin/Finish/Verify/Terminal を合わせ 117 packet。各 packet を見積もり profile の 1 slot 全量として、空の service book では受理点から 4,169 DAA で予約に成功し、terminal 窓上端と回復枠の合計も 4,210 DAA で hard 10,000 DAA に収まった。数値分岐だけのオフライン結果で、実 carrier の mass、storage rent、tokenizer 経路、選出者別の予約、実際の再実行量や公開 service book の空きを検証していない。見積もり profile の再実行上限は 2 elements・3 terms・1 source read で、実候補に対してその妥当性は未確認。

同じ候補から公開 T12 の demand rules で native numerical geometry を導出した。見積もり枠では 4,096 葉の上限で拒否し、葉だけを 5,154 にすると 64 root の上限で拒否した。両枠を実数に合わせた診断では 270 root がそれぞれ terminal 1 本を持ち、すべての対象葉が 64 出力値だった。現行の bounded replay builder は再実行の前に葉の `value_count` を target requests に加算するため、見積もり profile の 1 request では全 270 root が対象数の段階で枠外となる。予約時刻の試算とは別に、実際の算術・資料読み出しを計測して service work を見直す必要がある。

Studio が保存した同一 execution の dense capture を使い、候補の binding、step Merkle root、prompt hash、登録 artifact inventory root を照合した。見積もり profile では葉 46 の native bounded replay builder が `builder target request budget is full` を返した。既存の node builder 上限で全 270 numerical root を再実行し、独立の court checker は全件 `NoFaultFound`。builder の最大は 1,043 elements、130 terms、64 target requests、323 source reads。生成した cone の Borsh 最大は 27,111 bytes（葉 3,741、program を含む）。これらの計測値だけを現行の service 算式に代入しても `2 × (1,043 + 130) + 64 + 323 + 1 = 2,734` work で、見積もり profile の 13 work/slot を超える。これは保存済み資料を使う数値裁定のローカル計測で、公開 Batch 受付や全検証を示さない。

builder と checker の上限を上記の実測値に合わせ、全 270 root の再実行と `NoFaultFound` を再確認した。葉 45 では elements、terms、target requests、source reads のどの上限も単独で 1 減らすと拒否された。program を除いた cone の Borsh 最大は 13,335 bytes。前述の空の service book で Verify と Terminal にそれぞれ 2,734 work を計上すると、13 work/DAA では `Reservation(NoCapacity)`、診断上 2,734 work/DAA では受理点から 4,169 DAA で予約できた。この枠は一件の観測に基づく境界で、公開 profile の上限として採用していない。

公開 T12 の binding 受理を、DAA 20,999 の選択チェーンの受理 tx 一覧で再確認した。同じ Studio 候補 `7645bc8b…a3711` の quote は、座席の RPC を提供している稼働バイナリ `8cac52ec4ed1` で `staged rooted row has inconsistent key/immutable contract` を返した（`available=true`、`quoted=false`、plan なし）。この版の `PalwStagedCourtRowV1::check` には `TirContainer` 行の分岐がなく、旧版では正しい binding 行も汎用エラーになる。現在の `main` の `fbfc2b1b2` と復旧カナリア `151f9ce7` は、固定上限付きでこの行を検証する `7cc9d2b2a` を含む。復旧側の RPC は DAA 5,057 と 5,644 で、対象の受理点には未到達。稼働ノードで状態の読戻しを回復した後も、別途 `FpTokenizer` の経路成立と quote 成功を同じ候補で確認する。

公開 RPC の旧版 seat 7 は DAA 20,999 で停止し、heartbeat miner も `own native store/context is unreadable` でテンプレートを取得できていない。カナリア `151f9ce7b` は DAA 5,711 まで進み、同期中。両ノードの RPC が返す consensus params ID `edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189` と schedule ID `4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04` は一致し、予定済み fence の差分はなかった。同じカナリアの実行ファイルを seat 7 の `/root/t12-rel/incoming/151f9ce7b/kaspad` へ転送し、SHA-256 `3704030a2d230a2c74c20c4f8777706f08598e89988c2f8eb8cae3253f2d4c11` と稼働中の引数 24 種の認識を確認した。切り替え前に、同じ受理点の DB を修正版で読み戻せることを検証する。

seat 7 の停止中に DB を複製し、旧サービスを再開した。複製は変更不能な RocksDB テーブル 1,389 個をハードリンクし、メタデータと WAL 19 個をコピーした。隔離起動した修正版 `151f9ce7b` と旧版は同じ DAA 20,999、同じ sink を報告した。元の Studio quote を再送すると旧版は状態の整合性エラー、修正版は `G14_ROUTE_ABSENT (FpTokenizer)` を返した。続いて seat 7 の起動ファイルと照合用ハッシュを修正版に更新し、systemd の bind mount で検証済みの複製 DB を使用するようにした。元の DB は維持。修正版稼働後の公開 seat 7 で binding の受理を再確認し、同じ claim の quote は `available=true`、`quoted=false`、plan なしだった。virtual DAA は 20,999 から 21,000 へ進み、選択先端は `5d398f8b…fbcbbd`（ヘッダ DAA 20,999）へ移った。継続的なブロック生成や claim 由来のブロック生成を示すものではない。

稼働中の seat 7 で DAA 21,000 の fence schedule を取得した。staged public court と legacy public filer は DAA 18,400、FP tokenizer match は DAA 5,300 から有効。一方で `palw_class_semantics_v1` と `palw_court_family_supplier_v1` は未スケジュールだった。quote と受理で共有する G14 判定に、tokenizer 経路が有効でも登録済みクラスに raw tokenizer の byte 数・key event 数がない場合の説明を追加した。拒否コードと判定は従来どおり。既存の経路回帰を、変更後の関数を取り込んだローカル実行で確認した。

`--dense-replay` の関数を取り出した実行に、同じ Studio 候補 `7645bc8b…a3711` の保存済み capture を渡した。root 上限 269 では `numerical root budget exhausted`、270 では全件検査完了。最大は 1,043 elements、130 terms、64 target requests、323 source reads、program 除外後の cone 13,335 bytes。SDK example 全体は型検査済み。この計測は公開 quote や公開の受付予約を成立させるものではない。

公開 seat 7 は virtual DAA 21,001 で、資料宣言と family supplier は引き続き未スケジュール。稼働プロセスは対象の 135M artifact を保持リストへ載せておらず、既存の EOF 自動公開 actor が選ぶ canonical class の指定もなかった。SDK の診断は、固定値の EOF 証明書を組み立てる代わりに、保存済みの実 tokenizer をネイティブの Part/Finish で再生するようにした。ローカルの署名欄と包含時刻は fixture。実ファイルは 2,104,556 bytes で、1 byte を変えると Finish で拒否した。ネットワークの EOF 予約条件で最大 35 move、140 DAA、rent 11,929,756 sompi、deposit 35 sompi（carrier 手数料は別）。この EOF を同じ claim の見積もりへ渡すと、出荷設定で必要 8,567,151 DAA 対上限 3,120 DAA、Batch v5 比較では 72,565 対 6,880 DAA。宣言のみで EOF を省いた比較は計画計算前に拒否した。

保存済みの署名 object を復号し、元の宣言 Borsh と一致すること、T12 の署名ドメインと先に公開照会した登録者公開鍵で ML-DSA-87 の portable 検証が通ることを確認した。seat 7 の `/root/palw-class/t12-studio-7a2616b4/` に対象モデルと `tokenizer.json` を配置し、両ファイルの SHA-256 を送信元と照合した。保持リストへの追加、fence 変更、宣言の送信は未実施。

2026-11-11、公開 seat 7 を再照会した。virtual DAA は 21,006、params/schedule ID は前回と一致。同じ候補の quote は DAA 21,005 の状態で `G14_ROUTE_ABSENT (FpTokenizer)`、`quoted=false`、plan なし。今回も TFP1 の署名・送信は行っていない。

Batch の数値資料量の導出を、旧 allocation と署名済み custody frame が成立する前に呼べる `numerical_batch_projection_v1` へ分離した。既存の native Batch envelope と demand 再構成も共通の範囲計算を使用する。投影は claim/source/prompt 形式と位置別 byte 上限だけを保持し、scope・予約・受理権限を持たない。実 frame との再束縛で claim/source/prompt を照合してから、従来の scope ID を算出する。parameter byte 上限に用いる program は、walker が認証する class record から native decoder で取得し、別 program の引数を受け取らない。

既存の funded Batch fixture に、独立した materialized envelope との上限・scope 一致、別 claim・別 prompt 形式・空 profile の拒否検査を追加した。独立エージェントの静的レビューと `git diff --check` は完了。新規回帰テストはビルド待ちで、今回の変更の動作検証は未完了。前回から実行中だった SDK example のテスト 2 件は成功したが、そのビルドは今回の分離前のコードであり、この変更の検証には数えない。

この分離は公開 quote の成功を意味しない。新しい版の quote/admission 共通 planner、実 service book での予約、その結果を署名対象へ束縛する接続は残っている。現在の Batch book は数値と ConstraintStop が対象で、raw tokenizer と token ID の文字列表現の対応を調べる既存 FpTokenizer 裁定を代替しない。その検査・供給も同じ期限内に収める必要がある。公開 fence、期限、検証 gate は変更していない。

2026-11-11、Batch の service 予約も frame 非依存の共通 kernel に分離した。既存受理 wrapper は従来の allocation/source/profile/Stop と絶対期限の検査を維持し、同じ kernel の結果を元 claim の行へ登録する。グラフの構築と容量予約を分け、未予約の `CompiledBatchAdmissionV1` は private な graph/保持費用と native memory lease を所有し、相対 funding graph ID のみを公開する。quote は実 book への予約成功後に作る。

同じ相対 graph ID でも、仮 namespace と確定 scope では同期限 packet の ID 順が変わるため、容量上の成立性は同一とは限らない。共通 planner は確定した ID でもう一度 compile/予約してから quote 成功を返す必要がある。fixture に、事前構築した相対 ID と確定 quote の一致、共通 kernel と既存受理 wrapper の quote/book 完全一致、容量不足での原子的拒否を追加した。静的独立レビューと diff check は完了し、実行テストは待機中。tokenizer を含む全 branch の新版 planner と公開 RPC への接続は未完了。

先行実行していた SDK example の実ファイル診断が完了した。同じ完全 claim ID について 270 numerical roots の local replay と最大 work 値を出力し、公開費用条件の quote は必要 8,567,151 / 許容 3,120 DAA、Batch v5 比較は 72,565 / 6,880 DAA の拒否だった。raw EOF の再生を含むが、包含時刻・署名欄は fixture で、公開 quote ではない。先行する `cargo check -p kaspa-p2p-flows` も成功したが、今回の reservation 分離後の検証には数えない。

tokenizer の範囲について記述を訂正した。既存 FpTokenizer terminal は raw tokenizer から token ID ごとの rendering が正しいかを判定し、原文から prompt IDs への full retokenization は行わない。rooted tokenizer registration は全 rendering table を再導出して root と照合し、EOG は別提供の署名済み設定を同じ table root で認証する。これを後者の入力変換検証へ読み替えない。

2026-11-11、公開 seat 7 の DAA 21,012 で対象 binding の送信 tx を選択チェーンの受理一覧から読み戻した。受理 block は引き続き `bdacc76c…e62e`、tx は `2080b9fb…59fb`。固定した class・artifact root・container root、完全 claim ID、params/schedule、quote と資金の抜粋を [公開読戻し記録](../../../evidence/palw/public-t12-studio-binding-quote-2026-11-11.json) に保存した。元の RPC 出力は同記録のローカルパスと SHA-256 で識別する。同じ未変更候補は `available=true`、`quoted=false`、`refusal=G14_ROUTE_ABSENT (FpTokenizer)`、空の `planHex`。`palw_class_semantics_v1` と `palw_court_family_supplier_v1` は今回の公開 fence 読戻しでも未スケジュール。binding の再送は不要で、quote 成功・TFP1 送信には未到達。

commitment を native Borsh decoder で読み、executor bond が `550c7e88…70bb:0` であることを確認した。DAA 21,012 の公開 bond は Active、担保 1,299,900,000,000 sompi、producer floor の不足 100,000,000 sompi（1 BILI）。この不足は別の登録者についての推測ではなく、対象候補自身の executor に対する `getPalwProducerFacts` の結果である。`FreePromptCommitted` の受理処理も `palw_bond_producer_floor_shortfall_v1` を呼び、同条件で拒否するため、tokenizer 経路だけを直しても受付は成立しない。

現行の bond 登録は同じ outpoint・公開鍵・operator の重複を拒否し、担保への任意の追加送金を反映する native object は見つからなかった。既存コードが案内する復帰方法は新しい鍵と operator による再登録であり、executor bond の変更は別 claim ID になる。ActivationPool の top-up はこの担保の補充ではない。今回は追加支払・再登録・候補の置換は行っていない。既存候補の不足と、新候補を使う場合の別追跡を混同しない。

通常 TFP1 の Output/Stop 用に、FullIds 配信・署名済み裁定 control・共通 terminal の Batch 予約と BCT2 codec を追加した。受付時の native rules と service cost を含めた receipt は最大 5,454 bytes、保持期限付きの保存行は最大 5,462 bytes。署名は network、claim、scope、source、notice、policy、資料、filer と両 service packet を束縛する。terminal では同じ FullIds と裁定を再検査し、既存の Output evidence から claim 単位／execution-root 単位の失効規則を使う。

保存行は staged key 35、row 42 として末尾に追加した。専用の native verifier writer 以外の挿入を拒否し、受理点での一回の挿入と保持期限後の削除だけを journal で認める。期限・service hold を検査する専用 expiry writer、GC、retirement hold、snapshot の context 検査を接続した。新しい context version 3 は現行の constructor/check では発行・受理できず、既存の V1 数値経路も新版専用 ordinal を拒否する。BCT2 の公開 dispatch、新 profile の authority、修復処理、quote/admission planner への接続は未完了であり、公開受付を有効化した変更ではない。

native schema の再生成が PublicOneHonest の手動 codec と既存 aux table の読み取り分岐を失わせる問題を修正した。生成元に owner-local codec と process-local 型の区別を登録し、既存の aux table 分岐を再生成後も保存する。生成処理を二回実行した出力は同一。今回のコードを含む `cargo check -j 2 -p kaspa-consensus-core --lib` は成功した。新規回帰テストの実行は未完了。先行ビルド（`15ab590b0` 時点）の `g14_native_funded_batch_candidate_v1` と `maximum_graph_with_latest_notice_and_recovery_is_atomic_at_10000` は各 1 件成功したが、新版 Output 差分の検証とは分ける。

tokenizer 登録の接続も再監査した。`install_verified_tokenizer_registration_v1` の呼出しはテスト内だけで、公開の funded carrier から raw 資料を再生して登録行を作る入口は未接続。この行の存在を前提とする opening 計算だけでは公開 quote は成功しない。新入口の費用・配信・再生と、新 allocation の意味を保つ実装が必要である。

公開 seat 7 の DAA 21,019 で、同じ候補の quote を読み戻した。params/schedule は前回と一致し、`available=true`、`quoted=false`、`G14_ROUTE_ABSENT (FpTokenizer)`、plan なしだった。結果と raw ファイルの SHA-256 を既存の公開読戻し JSON の `quoteObservations` に追記した。この照会では binding と bond を再照会しておらず、それらの証跡は DAA 21,012 のものとして維持する。

独立した静的監査で、現在の G14 拒否と、その先の Batch 用登録を分けた。現在の gate は既存 staged/filer/tokenizer-match に加え、対象 class の raw byte 数と key-event 上限を要求する。未有効の class semantics と未受理の宣言を解消することが第一段階であり、検査済み tokenizer row の追加だけで現行 gate が変わるわけではない。既存 EOF は hash と全長を確認するが raw を保持せず、family raw 取得は受理済み claim を前提とするため、受付前の全 rendering 検査へ循環なく接続できない。

[RFC-0051](../../../rfc/0051-funded-preclaim-tokenizer-registration.md) と [ADR-0219](../../../adr/0219-funded-preclaim-tokenizer-registration.md) に、有限の preclaim Open/Part/Finish を Draft として追加した。既存担当エージェントの三回の静的レビューを反映し、署名、先行 parent、競合登録、料金の一回 burn、原期限内の再試行、native raw GC と通常履歴の保持を区別した。設計凍結規約に基づく採択をユーザーへ確認中。料金の数値、公開 activation、runtime 実装は未承認・未実施であり、この案の作成を TFP1 quote 成功に数えない。既存の Output コードのテストは引き続きコンパイル中。

その後、`5bc8b7b88` の core test build が 19 分 16 秒で完了し、`plain_output_control_length_and_signing_scope_v2` が成功した。同じ生成済み test binary で `g14_native_funded_batch_candidate_v1`、`family_supplier_four_role_prefix_native_original_and_financial_transitions`、`family_supplier_constraint_native_original_material_and_adjudication` も各 1 件成功した。[回帰結果](../../../evidence/palw/native-plain-output-v2-regression-2026-11-11.json) に source commit、binary/log の SHA-256 と範囲を保存した。計 4 件のローカル回帰であり、新 tokenizer 登録案の実装・公開 quote・同じ claim の受付を検証したものではない。


## 2026-11-11 — RFC-0051 採択後の native 登録実装

ユーザー指示「採用するゴールまで進めて」により RFC-0051 / ADR-0219 を設計採択へ更新した。追加登録料の一回 burn を含む設計と実装着手の採択であり、公開 profile の測定値や activation 日時を暗黙に固定していない。

`TKR1` の Open/Part/Finish、署名と bounded wire decode、実 UTXO fee からの料金分離、prior parent の全 Part による一回の rendering 検査、native 登録 writer を追加した。新規 Open だけに登録料を計上し、重複・拒否・Part・Finish は追加登録料ゼロ。検査失敗でも消費した Finish 枠を記録し、同じ DAA の別 block からの再検査を防ぐ。正常・Superseded・期限切れでは未使用の実行容量を解放し、元の割当履歴と raw は固定保持期限まで残す。

snapshot / journal 検査で固定 quote・Part 集合・元の割当・終端 / 解放点・bond hold を突き合わせ、期限前削除や他契約の再配置を拒否する。native heap / wire inventory も新行に対応した。core library の `cargo check` はこの段階で成功した。新規登録の統合テストと料金・予約・復元の回帰テストは追加済みだが、実行結果はまだ未記録。

公開 dispatcher、coinbase / template / Round の同一 fee 投影、block 合計 work、CLI / RPC と公開 profile の測定・activation は未接続。新 native writer は公開有効化していない。公開 binding の受理済み証跡は維持し、対象 TFP1 quote 成功・受付・Final をこの実装の成果として報告しない。


本番接続の変更箇所を [チェックリスト](rfc-0051-production-wiring-2026-11-11.md) に記録した。Open 成功時だけ決まる焼却額は、Round の採否や template coinbase の前に、同じ受理順の一時状態から算出して actual fold と照合する必要がある。単に最終段で fee を減らす接続にはしていない。

対象 raw tokenizer をローカルで読み取り、2,104,556 bytes、JSON key events 49,302、model vocab 49,152、33 Part（最終 7,404 bytes）を確認した。[入力形状](../../../evidence/palw/tokenizer-registration-input-geometry-2026-11-11.json) は SHA-256 と測定範囲を記録する。Python での入力確認であり、native parser の測定・登録・公開 quote の証跡ではない。


`ad9f9ae2e` として native 登録と SDK 準備 API をコミットした。core の登録関連 15 件、既存 Batch と Output 各 1 件、SDK 1 件の計 **18 件が成功**。[検証記録](../../../evidence/palw/funded-tokenizer-registration-regression-2026-11-11.json) に完全 source commit、実行 filter、profile、binary/log SHA-256、テスト名を保存した。core test build は 19 分 03 秒。SDK は機能検査用に core の opt-level を 0 にした別ビルドで、性能測定には使用していない。公開受付・quote・Final は引き続き未達である。


`fc3e726e2` に transport policy / Open cutoff、専用 network domain、UTXO handoff、通常 / Batch / TKR1 の受理順、確定 fold の fee parity を保存した。候補 scanner と予約 book は期限処理分を含む同じ総 work 上限を使う。core 17、受理順 4、Batch / Output / SDK 各 1 の計 **24 件が成功**し、ノード library の最新 check も成功した。[検証記録](../../../evidence/palw/funded-tokenizer-registration-phase2-regression-2026-11-11.json) に完全 source commit とログ・binary hash を保存。Batch は opt-level 0 の標準 / 8 MiB stack で overflow し、64 MiB のテスト用 stack で成功したため、その差を記録し性能認定には用いない。公開 preset は未設定で、coinbase / template / Round の焼却照合と scanner の本番境界適用は未接続。公開受付の成功、main push、公開有効化は行っていない。


### 2026-11-11 — RFC-0051 段階3: fee・容量の実行境界

コミット `b41a758851f32dee9ecbe7861bf87bf83c55a2d3` で TKR1 の source body / candidate work 検査、Open の焼却後 fee と Round 予算の照合、body 拒否時の native journal 逆適用、候補内 Finish cache を接続した。carrier ごとの全 state 複製を所有権の移動に置換し、未変更・拒否経路の一時予約を返す。最大五回の native 処理・二回の逆適用・一回の parser を共通の見積もりと policy ID に含める。

[機能回帰30件](../../../evidence/palw/funded-tokenizer-registration-phase3-regression-2026-11-11.json)が成功した。core/node opt-level 0、Batch のテスト stack は64 MiBであり、公開性能測定ではない。[実 tokenizer の搬送 mass 記録](../../../evidence/palw/tokenizer-registration-mass-2026-11-11/README.md)は固定テスト鍵によるオフライン値で、使用 rlib の元コミットは未特定。この値を出荷コミットの適格性として扱わない。

RPC/CLI、snapshot 検査と mempool/peer work の残る境界、実ブロック受理・復元、最大入力での計測を続ける。公開 profile・activation は未設定。公開 TFP1 quote 成功、main push、Final は未達である。


### 2026-11-11 — RFC-0051 段階4: 見積もり・受理状態の参照接続

コミット `d27ddb34261fef09ad88ae307322b135139e969a` に、署名前 Open quote と受理済み Open status の core / node / gRPC / wRPC / CLI 接続を保存した。親の block・root・DAA と見積もり用 virtual clock を分け、応答前に virtual owner と pruning/import context を再照合する。quote は予約・UTXO 支払済みとは扱わず、status は受理された Part、通常・回復 Finish の試行、完了 record を読み戻す。GC/reorg 後の不存在を受付拒否や送信履歴に変換しない。

[回帰22件とビルド記録](../../../evidence/palw/funded-tokenizer-registration-phase4-read-regression-2026-11-11.json)に結果を保存した。node、RPC service、gRPC client/server、wRPC server、CLI の cargo check が成功。初回ビルドの所有権エラーと gRPC テストの型推論エラーを修正して通過した。公開 RPC の利用・登録・有効化は未実施。次は実ブロック受理の試験と署名・送信側の接続を行う。

### 2026-11-11 — RFC-0051 段階5: 実ノードの登録と一回焼却

`4a9bfed6b` の隔離ノード試験で、署名済み Open → Part → Finish → 別 UTXO による同一 Open 再送が成功した。通常の template、実 UTXO 検査、受理 fold、coinbase を通し、新規 Open だけのサービス焼却と支出・釣銭、Complete の読み戻しを検査した。[関連24件の結果](../../../evidence/palw/funded-tokenizer-registration-phase5-node-regression-2026-11-11.json)に source commit、各 binary とログの SHA-256 を保存した。

当初、送出元と受理 block の DAA が必ず一つずつ増えると誤認し、待ち幅を増やす変更を試した。実ノードの reference clock では同一 DAA の送出元と子 block が成立するため、その変更は取り消した。この24件は元の予約規則で成功しており、待ち幅の新しい policy field は採用していない。実装済みの参照 API から Open/Part の受理を読んだ後に、次の control を作る手順を使った。

既存4ホストの読み取り専用[機材観測](../../../evidence/palw/tokenizer-registration-hardware-observation-2026-11-11.json)も保存した。6〜8 vCPU、RAM 約12〜25 GB、root disk 空き約4.9〜37.6 GB は観測値であり、最低要件の採択・最大入力の CPU/RSS/履歴容量測定ではない。再起動・再編・pruned IBD、実 Round の登録、送信用 CLI、mempool/peer work 接続、公開 profile の測定・有効化は残る。公開 TFP1 quote と受付、Final、main push は未達。

続く `0cc8aa68c` で、復元した予約表の件数・geometry を generic service check の map 再構築より前に検査し、snapshot の session/Part 件数と session policy も上限内かを先に確認するよう修正した。既存の policy と有限 work envelope を使用し、新たな公開数値は導入していない。同じ DB の再起動・別ノードの block replay・Finish 前からの実分岐を追加した[回帰25件](../../../evidence/palw/funded-tokenizer-registration-phase5-recovery-regression-2026-11-11.json)が成功。再起動は root・status・UTXO、replay は root・status の一致を検査し、再編では元の charge ID・Part・予約枠を変えず Pending に戻ることを確認した。pruned IBD の転送と実 Round の登録費用拒否は、引き続き別の未完項目である。


`fd68f3f62` に live quote から署名 bundle を作る `tokenizer-registration prepare` を追加した。Open ID・policy・purpose と native quote の一致を確認し、有限長の raw/EOG と登録 opener 鍵を SDK で検査してから署名する。新規ディレクトリへのファイル作成だけで、支払・送信・予約は行わない。[CLI build 記録](../../../evidence/palw/tokenizer-registration-cli-prepare-build-2026-11-11.json)は cargo check 成功と検証範囲を保存する。初回のエラー表示型の不一致は修正済み。公開 endpoint での prepare と支払・送信 CLI の接続は未完了。


`abe969c41` の通常 UTXO funding API は、opener と別の支払者、実 TxScriptEngine、手数料/payload/入力金額の改変拒否を[回帰1件](../../../evidence/palw/tokenizer-registration-funded-builder-regression-2026-11-11.json)で確認した。`3b7bd1dc8` に send CLI と実 Round の費用境界試験を追加し、[ノード2件・CLI1件](../../../evidence/palw/tokenizer-registration-round-sender-regression-2026-11-11.json)が成功した。Round 本文が1 sompi超過すると登録行/UTXO/収入を取り消すが、許可されたヘッダーの permit は既存規則どおり消費する。拒否された本文が同じ Open の料金を先払いしたことにならないことも確認した。Round は非出荷の V2 mechanics fixture を用い、公開 V3 profile の適格性を主張しない。

実 tokenizer の搬送 mass も[出典を固定して再測定](../../../evidence/palw/tokenizer-registration-mass-source-pinned-2026-11-11.json)した。使用した4 library の fingerprint を成功した node test の依存情報と照合し、core/txscript の source tree と binary SHA-256 を保存した。最大 Part は mass 312,004、標準 relay 上限480,000以内。固定テスト UTXO と仮 pins の結果であり、公開登録料・CPU/RSS・履歴保管の資格測定ではない。元の出典未特定の記録は変更せず残す。

[認証済み PALW sidecar の復元試験](../../../evidence/palw/tokenizer-registration-pruned-sidecar-regression-2026-11-11.json)が通過した。Part 受理点の予約 book・session・raw Part を取り込み、同じ Finish 取引の受理と Complete、後続各ブロックの状態 root 一致を確認した。取り込み未完了時には厳格な status RPC が拒否することも検査した。試験は検証済み UTXO を保持し既存の epoch-zero 完了処理を使う構成であり、P2P handshake と UTXO stream 全体の pruned IBD 試験ではない。ノード2件と CLI1件が成功した。

`8a6923139` にオフラインの parser 計測ツールを追加した。入力生成を別プロセスに分け、実 tokenizer と最大 raw/vocab/EOG、巨大文字列、疎な ID、末尾重複キー、最後の leaf 不一致など8ケースの期待結果を確認した。[機能確認記録](../../../evidence/palw/tokenizer-registration-parser-fixtures-2026-11-11.json)は opt-level 0 の既存 core rlib による結果であり、機材適格性や native fold 全体の測定ではない。release build と指定機材での CPU/RSS 測定は別に実施する。

先の入力形状記録の `jsonKeyEvents=49302` は、測定方法に書いたとおり JSON object のキー数だけだった。native の key-event 算式は vocab entries 49,152 と added tokens 17 も加算し、今回の実入力では **98,471** となる。見積もり・容量には native 値を使う。元の JSON は過去の観測として保持し、新記録に訂正を残した。既に取得した EOF の `maximum_key_events=1052278` はこの値を上回っており、公開で新しい登録が成立したことは意味しない。

`ee879a1d6` で TKR1 の local work 計上を通常 mempool 受付と peer relay に接続した。認証済み状態や parser を読む前に、有効 policy の最大 source cost を使い、不正・重複・上限超過も含めて peer と process の別枠に計上する。枠は monotonic clock で補充し、policy ID の変更で残枠をリセットしない。上限到達は retryable とし peer への不正判定にしない。burst/refill は既存 H1 のローカル運用値を参照し、未測定の公開容量や合意値にはしていない。[関連25件と既存 peer 2件](../../../evidence/palw/tokenizer-registration-local-admission-regression-2026-11-11.json)が成功した。実 MiningManager で有効化前の従来扱い、上限超過の早期拒否、資金付き Open の受付と重複拒否を検査した。TKR1 の P2P ネットワーク通し試験はこの結果に含めない。

`8d6d68b00` の[受理済み親状態を用意した native workload 試験](../../../evidence/palw/tokenizer-registration-native-workload-functional-2026-11-11.json)で、実 tokenizer の2件×33 Part と、8 MiB raw の2件×128 Part が成功した。両 session が全 raw を保持する状態から snapshot を復元し、第二の Finish で Complete、第一は未試行のまま Superseded となる。各 carrier の fee 仮実行・巻き戻し、確定 fold、journal undo/redo の root 一致も確認した。最大 raw ケースの snapshot は18,188,793 bytes、累積 journal は21,705,513 bytesで、どちらも Borsh の論理サイズである。macOS/opt-level 0 の機能結果であり、物理 RocksDB 容量や最低要件 Linux 機材の性能認定ではない。測定用 policy は2 session・1 sourceで公開 T12 の M=780 profile ではなく、実 tokenizer ケースの class/artifact も明記した fixture を用いる。公開対象 claim の受付とは区別する。

`13b755c4a` の[pruning import session 回帰](../../../evidence/palw/tokenizer-registration-pruning-utxo-session-regression-2026-11-11.json)で、以前の verified UTXO 保持・stable marker 手動完了を本番の取り込み処理に置き換えた。donor の Part 受理点の past UTXO 集合を復元して header MuHash と一致させ、複数チャンクを staged UTXO set へ投入した。部分 stream と早過ぎる完了は拒否され、全量の streamed/staged commitment 検査と session 完了後にだけ厳格な status RPC が応答する。復元した Part から元の Finish 取引が受理され、各 root と Complete が一致した。ノード2件・CLI1件が成功。P2P 転送、trusted-history proof の交換、自然な pruning point 更新は対象外である。


[Linux release parser 計測](../../../evidence/palw/tokenizer-registration-linux-parser-2026-11-11/README.md)では、source `8a6923139` の通常 release binary を6 vCPU Broadwell / RAM約12 GBの参照ホストで実行し、8ケース×5回が期待結果で終了した。実入力は201.81〜250.16 ms、最大 raw/vocab/EOG ケースは814.00〜1165.11 msだった。最大観測 RSS は key-heavy ケースの89.98 MiB。入力生成を別プロセスに分け、toolchain・binary/input hash・全コマンド・time の原文を保存した。共有ホスト上の少数サンプルによる parser 単体値であり、native 処理、最大 mergeset、物理 DB、公開1 BPS・登録料の資格測定とは区別する。


`7c6f9cf2e` の[差分回帰2件](../../../evidence/palw/tokenizer-registration-differential-regression-2026-11-11.json)で、全 table の一括 verifier と従来の逐次 JSON court primitive を同じ raw commitment に対して比較した。3種類の小さな table の全 ID を、1・7・65,536 bytesの断片と途中 codec 復元で検査し、rendering と key-event 数が一致した。上限を1下げた work、重複キー/ID、不正 merge、途中 EOF、末尾不正入力の拒否と、独立した EOG opening の認証も確認した。小さな機能 corpus の結果であり、全入力の同値性、旧署名契約の置換、実 tokenizer 全 ID の逐次 replay、公開受付を意味しない。


`db9db6bec` の[実 RocksDB 保存・GC 試験](../../../evidence/palw/tokenizer-registration-physical-db-functional-2026-11-11.json)が成功した。共有する前提チェーンから、TKR1 を登録する DB と同じ時刻・ブロック数の空取引 DB を進めた。DAA 136 の raw GC 後に Part/session/book は消え、検査済み登録行と元の body/journal は保持された。強制 flush・compaction・DB close/reopen 後も同じ結果で、最大観測 WAL は登録側15,853,876 bytes、比較側6,891,785 bytes、compaction 後の全ファイル長は3,022,325 / 2,729,517 bytesだった。これは圧縮しやすい空白で埋めた64 KiB入力、macOS opt-level 0の保存機能観測であり、実 tokenizer の保存上限や Linux 性能・公開単価には外挿しない。通常の履歴を raw GC の副作用で削除していない。


`9d9f88348` の[実 tokenizer を使う RocksDB 試験](../../../evidence/palw/tokenizer-registration-physical-db-actual-2026-11-11/README.md)では、2,104,556 bytes・33 Partを予約 DAA 順に実署名・UTXOで受理した。Finish後の flush 済み全ファイル長は登録側22,695,446 bytes / 同じブロック数の空取引側5,418,212 bytes。raw GC・compaction後は14,074,322 / 6,388,433 bytesとなり、DBを閉じて再度開いても検査済み登録と元の35 source body・35 journalが残った。macOS opt-level 0の機能・保存観測で、試験全体43.88秒。公開料金、最大 mergeset、Linux whole-node 性能は別の未完項目である。


`f5be87db0` の[複数パスと物理 carrier scanner 回帰](../../../evidence/palw/tokenizer-registration-native-multipass-functional-2026-11-11.json)が通過した。実69操作と最大 raw 259操作を、fee処理2回・その巻き戻し2回・owned observed rehearsal 2回・確定fold 1回で再生し、共有 Finish cache と費用・root・journal の一致を確認した。論理 RAM/DB 各4 GiBを有効にし、物理 quotaは設けていない。別の scanner-only 試験はM=780のsource partitionを持ち、不正/未解決Finish carrierを各780件計上し、sourceの2件目と候補の781件目を原子的に拒否した。予約表のsource controls=1により同一DAAの合法Finishは最大1であり、物理source数をparser回数と同一視しない。これらはmacOS opt-level 0の機能検査で、実DAG/Roundの最大source集合やLinux最大candidateの性能認定ではない。


[Linux native 2パス測定](../../../evidence/palw/tokenizer-registration-linux-native-2026-11-11/README.md)は、core `8a6923139`・tool `4d0aee958` の release binaryで計20プロセスが期待値と一致した。論理RAM/DB各4 GiBの5回では、実入力69操作のharness全体が2.38〜2.52秒・最大RSS47.14 MiB、8 MiB入力259操作が29.67〜31.29秒・96.69 MiBだった。fixture照合や追加root計算を含む全体時間と、Finish/parser・snapshotなど各APIの時間を別々に保存した。M=1、2 session、Finish1件の基準値であり、最大5パス・M=780候補、物理quota、公開料金/性能の適格性には昇格させない。後に発見した共通writerの一時予約返却修正はこのcoreに含まない。


`ebefd0152` の[M=780 native candidate検査](../../../evidence/palw/tokenizer-registration-native-candidate-2026-11-11/README.md)で、一時予約が状態不変のFinish再送でも累積し、最大8 MiB入力が論理RAM 4 GiBに達する不具合を検出した。`098da5e04` はjournal長・容量が不変でfee照合が成功した場合の返却を共通writerへ移した。同じ上限・入力で実24.08秒／最大70.48秒の機能検査が成功し、確定foldのFinish成立1件・slot消費1件・再送779件の状態不変を確認した。通常コア回帰25件も成功。macOS opt-level 0のモデル化した候補であり、実780-source DAG、公開障害の原因、Linux性能や公開profileの適格性とは区別する。
