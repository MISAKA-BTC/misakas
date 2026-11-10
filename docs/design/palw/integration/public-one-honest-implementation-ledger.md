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
