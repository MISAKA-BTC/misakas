# G14コード設計レビューとClaudeへのフィードバック

評価日: 2026-10-10 JST。対象: `claude/g14-public-prosecution-integration-9bee39`、push後の `34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada`。
以下の初回レビューのソース参照・FAIL数値はこのcommitに固定する。その後の修正は、新しい統合HEAD `323ea161a5b627431fb831b358e8d1f88457261d`を基点に`codex/g14-prosecution-bounds`で実施した。修正内容と最新の検証結果は[実装修正記録](implementation.md)を参照。

## 現在の判定（Codex修正後）

**G14全域はまだ未達。修正済みの小型・segmented court経路と、未完成の本番到達性を分けて評価する。** ユーザー確認により、正しい登録モデルを保有するverifierを前提とし、ADR-0177を維持する。

`codex/g14-prosecution-bounds`では、公開materialから独立に再実行するcourt、canonical tensor検査、RAM/state/filing上限、small-value commitment court、固定報酬jobの生成長束縛を修正した。さらに`g14/legacy-filer`を統合し、単独の不正leafを見逃すmidpoint探索と、未認証carrierによる公開応答cache汚染を修正した。core・実node試験の件数、再試験、未実行の範囲は[実装修正記録](implementation.md)にまとめている。

次の条件が閉じるまで「producer + 全Panel共謀でも1 verifierで全対象違反を終局まで追及できる」とは判定しない。

1. **legacyの全対象違反と本番完走:** base0-codecの独立replayを持つ共通filerに、認証済みtag-158公開履歴、subtree descent、model-copyを開示しないtag-159 leaf recompute、CKWからfused courtを開く接続を追加した。controllerのfold試験はFinal前後のgather/matmul conviction、fused conviction、node/CKW非応答defaultを通過。公開bindingに基づくjob/trace identity、step/checkpoint countのshape違反、公開token pinに基づくoutput不正をkind-4 reporter経路へ追加した。executionとtraceが一致してもoutputだけが違えば追及する。別jobの実fold convictionはFinal前後を検証し、output/count builderとcache妨害はunitで検証した。大型canonical promptの入力不正も既存PromptNotAnchoredへ自動変換する。PublicDA／認証できるcanonical inputのFP startupを公開accepted commitmentへ接続した。PanelDAの公開court input bootstrapも追加した。ordinary V3/V4 canonical FPのcount/root不正は、認証済み公開jobとbindingからreplay前に直接証明する経路を追加した。最大入力のsession／carrier予算、別job意味論のprefix-state等のbootstrap、checkpoint/traceの残る局所化、全familyの正規eligibility、fresh-nodeのservice tick完走は残る。step treeの一致だけでMismatchをHonestへ変えない。unsupported-codecの小型fallbackは34-session枠を超えるclaimをUnjudgedとする。
2. **累積scopeとcourtの両立:** 64要素hiding tileと累積scopeを実際のresponse/court/reward gateへ接続する。`cumulative_scope_allows_v1`のstubとF-C4R4-17は残る。scope報告関数の成功だけでreward eligibilityを認めない。
3. **DAと包含の期限:** 最大Respond、全chunk、adaptiveな探索round、proof処理が責任期間内に収まる条件を実nodeで立証する。9B/8kの1 positionは2,029 partsであり、現行の固定20 DAAだけでは十分性を示せない。
   whole-root検査のonce-per-claim課金とcacheの対応は、blockの受入・rehearsal・最終foldを同じthread-local scopeで包み、課金済みrootを退避しない修正を追加した。別threadによる旧process cacheの退避を挟むgate・受入・fold試験で、262,143 idsの再計算を1回に抑えた。5本のcarrierの合計transient massは357,140だが、各carrierの重量測定は同じfixture funding outpointを使うため、実block包含の証明ではない。初回の多数claim反例案はmass上限を超えて棄却した。最大人口・実node・期限内包含の検証は残る。
   **carrier料金とfundingの修正:** nodeの共通署名builderをfence-aware rent＋relay carriage feeへ変更した。公開filerのpriority queueとcommon carrierはobject固有の最低額で成熟・unlocked・mempool未使用のUTXOを選び直す。2M proofのrent 13,107,150 sompiに対し選択基準は19,107,151 sompiであり、通常moveの12,000,000 sompi floorは維持する。新しい実署名・料金／fundingの5 V-unitと関連回帰、計108 PASS。これは料金不足による提出阻害の修正であり、full-serviceの採掘・包含、ordinary mature bond、全familyの終局を立証するものではない。
4. **共謀者による負荷と包含:** claim共通のlive 64・生涯256 reservation上限を除き、本人のretained record 64件と担保余力へ帰属させた。他bondの予約数で新verifierを拒否しない。旧閾値を超える予約から公開input→conviction/defaultへ進むV-foldを追加したが、owner数に比例する最大state／response負荷と期限内包含の実node測定は残る。
5. **正規eligibilityと測定:** 各active familyをtest-only admissionなしで登録し、fresh verifier自身のnode/RPC、最大profileの実測RAM・時間、restart/IBD/reorgを確認する。shape計算や小型fixtureだけで代替しない。legacy readerの途中走査の取りこぼしは修正したが、対応外unitの恒久prune後の取得・再要求とfresh-node追及の実node試験は残る（下記）。

**FP startupとFinal後の追及:** free-promptを無条件にUnjudgedとする経路を、accepted transaction・claim hash・記録済みjob pinの照合と独立replayへ変更した。producer captureは実行せず、hostのmemory budgetにはjobのdecode ceilingを使う。FPにはvesting rowがないためFinal後の発見・reservation・DA admissionが止まっていた抜けも、休眠public-filer fence内の共通predicateとliability保持で修正した。PanelDAのinput prefixが読めることを、strangerへの取得保証とは扱わない。試験範囲と残条件は[FP修正記録](implementation.md#公開fp-commitmentinputからの独立再実行とpost-final-fpの追及)を参照。

**Reorgのjob更新:** 受理blockが変わっても旧jobの判定が残る抜けを修正した。candidate snapshotが変わればPendingへ戻し、遅延した旧replay結果を捨てる。実行中のblocking workerは予約とhandleを保持してdrainし、その間は次のreplayを開始しない。探索履歴と未送信carrierもbranch/jobの変更に合わせて破棄する。新4件を含むfiler 14 V-unitで確認したが、実nodeの競合branch→新job再実行→convictionの一続きの検証は残る。

有効化heightは変更していない。X8R統合のfence番号に合わせ、EXEC v2を6、LegacyPublicFilerを7とした。以下は比較用に保持した初回レビューであり、解決済みのcompile・資源バグを現HEADの未解決事項として再掲するものではない。

**FP PublicDA追加時点のLG14-B outsider接続の検証:** LG14-B **10 V-fold＋2 V-unit PASS**（held replay全体では24 PASS）、共通filer **15 V-unit＋FP input/replay 3 V-unit PASS**、公開history API adapter **7 V-unit PASS**、reporter **21 V-unit PASS**。X8R統合後のlifecycle/A2U **31 V-unit PASS**、shipping pin検査 **361 ok・差分なし**。producer DA policyとIRの回帰も確認した。学習対象のtag 158を1つに絞り、frontierをdescent round数で制限し、重複・未選択・改ざんcarrierではcacheを増やさない。過去に回答済みの次unitを選ぶとbackfillを再開する。従来fixtureはsupplied public jobからown replayを注入する。新しいFP fixtureはactual accepted-tx extraction／FP foldとproduction replay helperを使い、Final直後とliability期限直前の追及、期限を跨ぐDA要求、Final後の非応答defaultを確認した。ただしmldsa signature verification callbackとclass admissionはtest fixtureであり、Consensus API readerは別のV-unitで検査する。実nodeのstartup・memory ledger・mempool包含まで立証したものではない。held-court後半のhistory readerにも、固定件数のpagination、最古の認証済みfilingの選択、選択chunkの認証とbackfill、branch確認を追加した。reader／held controllerのfixture回帰は確認したが、fresh nodeのfull service完走と責任期間内の恒久保持は別の受入条件である。

**C2履歴readerの修正:** 以前はDAA差をblock件数へ変換し、途中で打ち切った走査も要求floorまで読んだと記録していた。公開済みunitが`AwaitAnswer`に留まる経路を、再開cursor・開始tipの固定・完了範囲の記録・新しい追及のbackfill・branch変更時の読み直しで修正した。認証済みの利用対象unitだけを重複なくcacheする。node-policyの **11 V-unit PASS**（filer 7、Consensus API adapterのreader 4）がある。これはfresh-node追及のV-node試験ではない。恒久的にpruneされた回答は今回の修正でも取得できず、完了扱いをせず再試行する。現行consensusは既回答unitの再要求も拒否するため、保持・取得・再要求の経路と期限を別途完成させる必要がある。詳細とlogは[修正記録](implementation.md)の履歴pagination節を参照。

**回答済み公開DAの再取得:** 当初のpagination修正だけでは恒久的にpruneされた回答を取得できなかった。追加修正では、休眠tag 156の署名付きsession番号・期限を用いて、Event／Held／LG14-Bの回答済みunitに新しい公開義務を開く。historical回答を消さず、古い署名の再送を拒否する。古いsuffixが欠けても新tipの再公開を読む。上記の「既回答unitの再要求も拒否する」という記述は、この対応unitについては追加修正前の状況である。TIR／pipeline、FP job commitment自体、最大入力のbootstrap予算、全枠飽和とfull-nodeの期限内包含は未達であり、これを全familyのC2/C7/C8達成とは数えない。最終検証は[修正記録](implementation.md)末尾を参照。

**PanelDA入力のbootstrap（今回の追加）:** 公開FP commitmentだけではUser入力が手に入らず、全Panelの配布拒否で再計算前に止まる経路を修正した。公開予約→row-0 binding→認証済みinput chunk→自身のmodel replayへ接続し、不応答は既存DA defaultとする。Merkle入力は2,048 token単位、Flat入力はwhole hash照合とし、court上限と1本のcarrier上限の小さい方を超える義務は要求時に拒否する。登録モデル保有前提は維持する。34-session予算で全contextを扱えること、FP commitment自体の恒久prune、全family・fresh-node serviceの包含は別の未達条件である。今回の関連回帰は **159 PASS**、通常node binary checkは成功。検証結果と範囲は[修正記録](implementation.md)末尾を参照。

**予約の共通人数上限の修正（今回の追加）:** 64-live／256-lifetimeの先取りによる締出しを除き、closed replay guardを含む1 bondのretained recordを64に制限した。本人のreleaseでは予算を回復させず、recordのsettlement／retirementで回収する。RPCにも保持件数を追加した。これは固定global RAM上限又は全familyの包含保証を立証したものではない。最終検証は[修正記録](implementation.md)末尾を参照。

## 初回レビューの判定

**方向は妥当だが、提示されたG14を満たすコード設計としてはまだ十分ではない。**
class/program/plan/commitmentのbinding、Panel投票と独立したexact court、claim固有のdemandと客観的timeout、Final後の責任期間、再生可能なledgerという骨格は適切である。milestone 3には計算不正・output不正の実node経路の前進がある。しかし、最新統合HEADはコンパイルできず、public-prosecution gateが返すRAM・保持stateの上限は実データで破れる。courtの存在や小さいfixtureのPASSだけでは、すべての許可profileについて「有限資源・許可scope・期限内包含を経て終局に達する」ことを証明できない。

この判定と次の不具合説明は初回レビュー時点のもの。最新3730cc90fを統合したCodex branchはcompileでき、公開commitmentからのexact再計算、segmented版のRAM/state計上、bounded demand参加者を実装・検証した。最新結果と残条件は[実装修正記録](implementation.md)の末尾にある。G14全域の完成判定は引き続き保留する。これはdormant実装の完成度評価である。現行のactivation validatorはarmingを拒否する。以下の資源テストは上限式の反例であり、稼働ネットワークへのDoS実証ではない。

## まずG14の主張を正確にする

[ADR-0177 D7](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/docs/adr/0177-model-bond-allocation-without-availability-consensus.md)は、モデル取得を合意保証から外し、**正しい登録モデルを実際に保有・取得できたverifier**をG14の前提としている。全所有者が配布を拒否した場合の取得保証は撤回済みで、実効検出確率は0にもなり得る。この決定を維持するなら、モデル未取得でも常に不正を検出できるという無条件の説明はできない。

また、DA不応答は算術不正の証明ではない。実装上も`Convicted`と`Unavailable/default`は区別される。目標文は、例えば次のようにする。

> producerと全Panelが共謀しても、正しい登録モデルを保有し、必要な資源・bond・時間を確保したPanel外の1つのverifierは、producerの秘密状態やPanelの協力に依存せず、公開の認証済み情報と許可されたclaim固有court要求から、active planの対象違反をlocalizeし、計算・binding不正のexact conviction、又はclaim witness不応答の客観的DA defaultへ到達できる。期限内のトランザクション包含を前提とし、確率的検出の残余誤りと資源上限をprofileごとに示す。

ここでモデル配布拒否をclaim DA defaultに変換しない。確率的検出の成立、exact courtのsoundness、proofの期限内包含はそれぞれ別の証明事項である。

## 今回確認した修正事項

### P1-1: 統合HEADがコンパイルできない

[palw_kernel_route_v1.rs:717](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/consensus/core/src/palw_kernel_route_v1.rs#L717)の`from_served_rows_v1`は、構造体に存在しない`ledger_cache`と未定義の`KernelLedgerCacheV1`をinitializerに残している。

`cargo check -p kaspa-consensus-core --locked`は`E0560`と`E0433`で失敗した。現在の構造体に合わせてこのinitializer要素を除く最小修正後、同じcheckは成功した。`unused_mut`警告は残る。

修正案: [compile-fix.patch](evidence/compile-fix.patch)。これは欠落したcache機能の実装ではない。cache導入が意図されているなら、フィールドだけを足すのでなく、変更後のrowsとの整合・無効化・clone/reorg/read経路まで揃えた別の変更として評価する。

受入条件: 最新の統合HEADでcoreのcheckとcanonical node試験を再実行し、以前のcommitのPASSを新HEADの結果として流用しない。

### P1-2: `max_verifier_ram`が実際のメモリ上限ではない

[gate.rs:148](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/gate.rs#L148)はRAMを`artifact_bytes + evidence_bytes_per_position`としている。[plan.rs:352](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/plan.rs#L352)のartifact bytesはdtypeのwire幅で算出されるが、[Tensor](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-tir/src/tensor.rs#L8)の実表現は`Vec<i128>`である。I8ならデータ要素だけでwire幅の16倍になる。

`wide128_v1(7)`の正規planは`check_plan_v1`とpublic-prosecution gateを通過する。しかし再現値は次のとおりだった。

| 数量 | bytes |
| --- | ---: |
| artifact wire | 3,072 |
| evidence / position | 1,104 |
| 宣言されたmax RAM | 4,176 |
| artifactの既存`Tensor.data` capacityだけ | **20,480** |

さらに[verify.rs:308–374](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/verify.rs#L308)はparam/derived cacheを持ち、[scope全体のloop](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/verify.rs#L650)を通じて成功したderived値を保持する。historyの再構成値、Tensor clone、projection、一時operands等もこの上限式に含まれない。全scopeのcache増加はコード上の追加懸念であり、上表の反例自体はhistoryを必要としない。

修正要求:

1. encoded/public bytesとdecoded/live RAMを別々に導出する。param instance・layer、同時に存在するコピー、projection、Merkle/index metadata、一時領域を数える。
2. 必要ならdtype別の保持表現とstreamingを導入し、derived cacheを最後の利用後に解放する。あるいは現表現を前提に保守的なpeakを計算する。
3. `max_positions`・history window・layer数を最大にした許可profileで上限を検証する。実装側のメモリ制限で停止するなら、そのprofileを「prosecution complete」と認定しない。

### P1-3: `max_retained_state`が認証済みresponseと参加者metadataを含まない

[gate.rs:149–150](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/gate.rs#L149)はcommitmentsと小さなposition metadataだけを保持stateとして数える。一方、[Respond](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/ledger.rs#L2363)はserved materialを保存し、[to_rows](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/rows.rs#L153)は全served値をtable 8へ永続化する。担保の解放だけではserved値は削除されない。

reference ledgerで、prompt 63 tokens・max new 1の正当なclaimに対し、全63 positionを1つのoutsiderが要求し、正しいresponseを受理させた。

| 数量 | bytes |
| --- | ---: |
| 宣言されたmax retained state | 1,582,080 |
| table 8のkeys + Borsh valuesだけ | **4,220,181** |

このdense/MoE fixtureはnodeのcarrier fitで拒否されるため、実nodeにこのサイズのclassを通したとの主張はしない。立証したのはreference gateの上限式が上限になっていないこと。ただしnodeもこのboundsを消費するので、修正は統合設計に必要である。

さらに[FileDemand join](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/ledger.rs#L2283)は異なるdemanderを同じsessionのVecに追加する。`max_sessions = max_positions`は参加者数・参加者ごとのmetadataや精算workの上限を与えない。この点はコード読解による指摘で、今回の63-position再現とは別である。

修正要求:

1. commitment carrier bytesとretained storageを分ける。[carrier_fit_v1:2657](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/ledger.rs#L2657)は現状、後者を前者の代用にしている。保持stateを正しく増やすだけではcarrier判定の意味まで変えてしまう。
2. served responses、open/served demander metadata、proof seals、scope tallies等の全claim状態を含める。参加者・累積joinと各blockのworkにも明示的上限を置く。先着joinでhonest verifierが閉め出されない公開response・shared progressの性質を保つ。
3. 責任期間内のfresh verifierが利用できる保持規則と、終了後のbounded cleanupを定義し、reorg/restart/IBD/pruningで同じrowsになることを検証する。

## G14完成のために残る設計・受入条件

### 許可scopeからterminalへ到達する証明

nodeの[scope admission](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/consensus/core/src/palw_provider_court_fold_v1.rs#L468)はprovider-court fence後、1 requester OPERATOR / 1 claimあたりdistinct unitを16に制限する。別requesterに枠を消費されないことは良いが、**1 verifierが任意の対象不正を16 unit以内でlocalizeできることは別の性質**である。gateの`max_localization_rounds = 2`もunit数・bytes・探索workの証明にはならない。

要求: active profileごとに、認証済みrange/segment commitments等から有限scopeでfaultへ降りる経路を示す。16箇所は正しく応答し、別の箇所にだけ不正を置いたclaimでも、1 operatorで許可要求からexact conviction又は正当なDA defaultに到達できる試験が必要。対象位置を最初からテスト側で教えるケースだけでは不十分である。この項目は到達性の未立証を指摘するもので、今回nodeで新たな攻撃を再現したものではない。

同時にreference ledgerの[累積scope hook](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/ledger.rs#L2206)はまだ`Ok(())`のstubで、`F-C4R4-17`はignored FAILである。単に枠を撤廃したりweightsを強制公開したりして解決せず、ADR-0177の非再構成制約とterminal到達性を同じprofile・同じrulesetで両立させる。両立しないprofileはgateで拒否する。

### 期限内包含・資源・検出確率を分ける

[court budget](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/ledger.rs#L1294)はproof枠を他のobjectから保護するが、junk `FileProof`も同じ枠を消費する。[proof handling:2047](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/misaka-palw-kernel/src/ledger.rs#L2047)はadjudicationの前に予算をchargeし、予算切れなら真正proofもrefuseする。dismissal feeで攻撃を高価にすることは包含保証そのものではない。[arming list CODE 14](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/docs/design/palw/g14-node-e2e-record.md#L622)もこのgapを認めている。

要求: 多数claim・chunk・junk proofが同時に来る条件で、期限内に必要なproofが処理できるadmission/容量/スケジュールの条件を示す。それを保証しない設計なら、G14に包含仮定を明記する。経済条件は実効検出確率・外部利得・net徴収可能額・実費から評価する。1つのterminal proofが正しいことから、監視者が存在すること、必ず検出すること、必ず期間内に包含されることまで推論しない。

### active plan全域と最新HEADでの実node証拠

[milestone 3 changelog](https://github.com/MISAKA-BTC/misakas/blob/34b6c3f5ef13a0639ac1a5a97d3455f8a5a66ada/docs/design/palw/g14-completion-matrix.md#L585)では、quantization/rounding、substituted output、garbage traceのconvictionと、bad boundary/copied/borrowedのinclusion refusalが報告されている。post-genesis bondやfresh nodeの改善もmilestone 2にあり、古い表の「全familyでC1がGAP」をそのまま現状として扱うべきではない。

しかしrouting/TopKとderived historyを持つclassはv2のwhole-instance courtでworst filingが約193 MBとなり、nodeの約1.58 MB carrierに入らない。実model・最大profileのv4 courtsへ統合し、正規登録から同じoutsider経路で試験する必要がある。pipeline/media、task heads、typed roots、EXECのinitial boundary・slice DA default・onboarding/eligibilityについても、reference判定や`cfg(test)` admission seamだけの結果をproduction経路の完成と扱わない。

要求: 各active descriptor/profileの全relation、job/input/output/state/DAに対し、公開read → 認証 → faultを知らないfresh verifierのlocalization → 許可demand/serve又はdefault → exact court → signed/mempool/chunk/template/block → slash/rollbackの経路を最新HEADで検証する。Final前後、false challenge、IBD/pruned start、restart/reorgを含め、未実装profileをregistration/reward gateで拒否する。

確率checkerについては、chainからseedを再現する経路、post-commit unpredictability、grinding/alias/CRT composition、exact courtの独立soundnessを別gateで確認する。private saltによる補助checkerの成功や24件のfixtureのPASSだけから、規定のerror floorや外部soundness review完了を宣言しない。

## 実行した検証と限界

| 検証 | このレビューの結果 |
| --- | --- |
| original HEADの`cargo check -p kaspa-consensus-core --locked` | FAIL: E0560 / E0433 |
| 最小compile patch適用後の同check | PASS、unused_mut警告1件 |
| `cargo test -p misaka-palw-kernel --test k2_ledger --locked` | 19 PASS |
| `cargo test -p misaka-palw-kernel --test c4r4 --locked` | 14 PASS、1 ignored (`F-C4R4-17`) |
| 独立したRAM・retained-state regression | **2 FAIL**、上記数値を再現 |
| canonical nodeの24件 | 既存記録を読んだ。今回このHEADでの再実行はしていない |
| socket RPC drill、実model最大profile、外部soundness/economic review | 今回は未実施 |

証拠: [original core check](evidence/core-check-original.log)、[patch後core check](evidence/core-check-after-compile-fix.log)、[資源反例log](evidence/resource-regressions.log)、[回帰テストソース](evidence/g14_review_regressions.rs)。

回帰テストは対象commitの`misaka-palw-kernel/tests/g14_review_regressions.rs`へコピーして、次で実行する。`mod common`は対象branchの既存fixtureを使う。修正後に両テストがPASSすることは最低条件であり、最大profile全体の保証に代わるものではない。

```sh
cargo test -p misaka-palw-kernel --test g14_review_regressions --locked -- --nocapture
```

## 初回レビューで挙げた修正依頼（対応状況は実装修正記録を参照）

1. `34b6c3f5e`の`from_served_rows_v1`に残る未定義cache initializerを修正し、最新統合HEADでcheckとcanonical node testsを再実行してください。最小compile patchを添付します。
2. `max_verifier_ram`をwire bytesで代用せず、実際のTensor表現・instance・cache・一時領域を含む上限へ直してください。添付wide128 regressionは4,176 Bの宣言に対し、artifact dataだけ20,480 BでFAILします。
3. `max_retained_state`をserved responsesと累積demander metadataを含む上限へ直し、commit carrier bytesを別フィールドへ分離してください。添付reference regressionは1,582,080 Bの宣言に対し、served rowsだけ4,220,181 BでFAILします。
4. ADR-0177の累積scope predicateと、1 operator / claimの16-unit制限下でのlocalization→terminalを一緒に完成させてください。対象fault位置をverifierへ事前に与えない試験と、複数claim/identity経由の再構成試験が必要です。
5. モデル保有、有限資源、包含、確率検出の前提をG14文に明記し、計算不正のConvictedとwitness不応答のdefaultを分けてください。撤回済みのモデルavailability consensusは復活させません。
6. v4 real-scale courtsを統合し、routing/historyを含むactive profile全域とEXEC/typed-root等の正規eligibility経路を最新HEADで検証してください。carrier-fit不能やtest seamのみのprofileは有効化しないでください。
7. proof spamを含む期限内包含条件、ADR-0176のQ/B/R/F会計、fresh verifierの実測期限、beacon/seed再現、外部soundness/economic gateを閉じてください。feeによる抑止と必ず処理される保証を分けて報告してください。
8. completion matrixの冒頭・family表をmilestone 2/3の結果へ更新し、最新HEADのPASS・未実行・FAIL・設計未完を区別してください。これらが閉じるまでarmingのfail-closed方針を維持してください。
