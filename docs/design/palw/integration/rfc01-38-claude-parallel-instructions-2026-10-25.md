# RFC01〜38 構造実装 — Claude並列作業指示書

作成日: 2026-10-25。構成は **統括Claude 1名＋実装Claude 7名**。この文書は作業依頼であり、設計変更の採択・公開activation・campaign合格を宣言しない。

## 渡し方と基準

統括には「共通指示」と「Claude 0」を渡す。各実装担当には「共通指示」と自分の担当節を渡す。8名を用意できない場合は、統括が空いた担当へ次レーンを順次割り当てる。同じcheckoutを複数人で編集しない。

根拠は[全38本の監査](../../../audit/palw/rfc-implementation-audit-2026-10-25-head1257/README.md)と、各詳細記録。監査対象は `1257f0ae8b2f33f5421e09be416f3aa86af0b47c`。受入完了2/38（21・36）、コード項目充足4/38（17・19・21・36）という区分を出発点とする。36は純ライブラリ＋休眠fenceの宣言範囲だけである。

指示書更新時の確定HEADは `78277650f034f2a1416306b3da741dde10f78b82`。監査snapshotの判定と、その後の個別進展を区別する。以下を既存成果として引き継ぎ、再実装しない。

- `bc2217cc6`：Legacy court ingress事前予約の実装・検証。
- `4890b70dd`：[native claim/court membershipの予約所有権](../../../evidence/rfc/0006-native-claim-court-membership-owner-2026-10-25.md)。局所24テストPASS。
- `a4973592d`：[署名provider→実checker→資金付きnative challenge](../../../evidence/rfc/0007-signed-provider-native-funded-challenge-2026-10-25.md)。局所21親テストPASS。独立sourceやpaid Final承認の成立を意味しない。

開始時に再確認すべき進行中作業は、GDN held-answer認証workspace、mesh一回決済の失敗原子性、paid-check authorityのtyped Resource伝播、normal-source typed読取draft、公開履歴collectorのbaseline照合。これらの未コミット差分を受入済みと数えず、既存担当と所有権を調整する。特に実行中ビルドが参照するRustは編集しない。

ローカル再起動により `/tmp` の旧binary・生履歴・ビルドが失われた。過去の確定証拠と、現在再取得可能な原資料を区別する。新規ログ・binary pin・source manifest・生履歴は永続領域へ保存する。この作業では `/Users/wata/.codex/artifacts/rfc06-12-20261025` を使用中。偶発的な再起動はOS crash gateのPASSにしない。

## 全Claude共通指示（各担当へ必ず渡す）

あなたはMISAKAのRFC残件を実装する担当です。監査の再要約だけで終わらず、担当範囲の構造を既存native consumerへ接続し、実行可能な差分と試験証拠を提出してください。

1. 最初にcheckoutの `AGENTS.md`、`CLAUDE.md`、適用される下位指示、`docs/PRINCIPLES.md`、担当RFC本文と監査詳細を読む。既存の採択設計に沿って実装する。設計の重大変更が必要なら、既存の凍結手続きに従うRFC・ADR案を別成果物にし、採択前の挙動を有効化しない。
2. 計算の正しさの暗号学的証明は禁止。ZK、PCS-based validity argument、GKR transcript等へ置き換えない。確率的検査・公開証拠・決定論的court・経済的抑止を使う。ハッシュ、署名、Merkleは同一性・認証用途に限る。local replayの一致や楽観的Finalで抽選検証者承認を代替しない。
3. モデル二階層を維持する。genesis/年次指定1.0、後登録0.3、モデル審査・昇格・失効・試行手数料を導入しない。全モデルでclaim単位の認証を維持する。
4. 公開full activationの高さは `None` を維持する。DAA 16000で武装しない。通常selected-source/V6 paid bootstrapの拒否、未承認economicsの休眠、延期された画像・動画・モデル改善・Panelのfenceを維持する。公開nodeの別release設定を承認の根拠に転用しない。
5. K3正式仕様・manifest・weights・署名済み入力は未作成。native39→RFC12のW/PWU・anchor・operator・retention・成熟・void写像は未策定。承認済みsource independenceや料金・供給policyも、存在を確認できなければ創作しない。未確定点を具体的な決定票に切り出し、依存しない実装は続ける。新しい型やfixtureだけでその残件を完了扱いしない。
6. 各担当は別worktreeと専用branchを使用する。開始SHA・変更予定ファイル・公開する型/API・依存先を統括へ提出する。他レーンのファイルを無断編集しない。中央ファイルへの接続は小さい差分として統括へ渡す。既存の未コミット差分をreset・削除・持ち出し統合しない。
7. 既存の同等実装と呼出経路を検索してから実装する。新しい並行runtimeや別台帳を増やす前に、既存の正規経路へ接続できるか確認する。wire tag、既存ID、DB schemaの意味を黙って変えない。互換性変更はversionと移行・拒否条件を明示する。
8. 資源は生成前から最後の使用まで所有者と上限を持たせる。local容量不足、入力不正、未承認・未準備を型で区別する。`.ok()`、空値、ゼロ、skipへの変換で失敗を隠さない。認証・料金・責任をtest専用迂回で成立させない。
9. 実native入口からcheckpoint・restart/reorgまでの呼出経路を示す。helperのみの試験、dry-run、fixture上のPASSを実campaignのPASSへ昇格しない。異なるbinary世代の試験件数を合算しない。15 gateは実証できたgateだけ更新し、残りはUNKNOWNを維持する。
10. ビルドは統括の排他予約に従う。共有targetでCargoを同時実行しない。巨大targetの複製や他作業のキャッシュ削除をしない。実行binaryのSHA、source SHA、feature、command、exit、PASS/FAIL/ignored、ログ保存先を残す。大型既存Rustファイルを一括formatしない。
11. 成果は小さなreview可能単位でcommitし、統括へcommit SHAを渡す。公開deploy、activation変更、共有branchへのpushはこの依頼に含めない。文書検査の既存baselineを増やして失敗を隠さない。

### 全担当の提出形式

- RFC要件ID／変更前の実caller／実装内容／変更ファイルとcommit。
- 公開APIと所有権、他レーンとの接続点、永続化・rollback・再実行時の不変条件。
- 実行した試験と失敗を含む証拠。未実行は理由と具体的な実行command。
- 残件を CODE / TEST / EXTERNAL / POLICY に分ける。各項目の解除条件と依存先を記す。
- 「この差分で閉じた要件」と「まだ閉じないRFC全体」を分ける。全MUSTと必要な受入が閉じるまでRFC完了を主張しない。

## Claude 0 — 統括・境界契約・統合

あなたは統括です。各担当に下記レーンを割り当て、共通境界を決めたうえで並列実装を進めてください。

最初にHEAD、dirty差分、稼働中Cargo、空き容量を確認する。既存の実行中試験を中断せず、未コミット実装を受入済みと仮定しない。開始用の確定統合SHAを一つ記録し、そこから専用worktreeを準備する。元checkoutや他作業のdirty差分は保持する。

次の接続契約を担当同士で先に合わせる。新規policyを決める会議にはしない。

| 境界 | 提供担当 → 利用担当 | 合わせる内容 |
|---|---|---|
| 因果graph | B → A・C・D・F | duty/branch ID、依存辺、terminal、deadline、canonical順序 |
| 予約 | A・B → C・D・F | resource vector、予約token所有者、commit/abort、最終解放 |
| 資金・供給 | C → B・D・F | 既存採択済み料金・資本の出典、payer、retention、typed未準備 |
| material | F → A・B・C・D | material identity、取得・保持・import/resume、provenance |
| 認証・出力 | D・E → G | approvalとFinalの区別、winning carrier、公開可能な証拠 |

中央ファイル（`palw_state_v2.rs`、共通API、Params、crate root、Cargo、共有wire/DB registry等）の編集責任を統括へ集める。担当はleaf moduleと中央への最小接続案を出す。統括自身が巨大ファイル全体を書き換える指示ではない。

統合順序は境界型 → Bのgraph → Aの予約 → Cの資金接続 → D/Fのconsumer → E/Gの通貫確認を基本とする。独立したleaf実装・調査は同時に進める。各commit統合後に影響を受ける試験だけを実行し、最後に同一source/binary世代で横断試験を行う。

最終成果は統合branch、変更要件一覧、受入証拠、決定待ち一覧。元の監査snapshotは改変せず、新しい監査差分を保存する。RFC完了件数の更新は全要件を再照合してから行う。

## Claude A — 資源所有権と生成前予約

対象: RFC02・06、RFC08/16の共有資源部分。RFC36の既存完了範囲を壊さない。

目的: native graph、Legacy callback、DB/cache、court/aux consumerを横断して、生成から最後の使用まで予約所有権を閉じる。

- 既存予約済み経路と未予約経路をcaller単位で区別する。監査後に閉じたLegacy ingressを再実装しない。確定済みclaim/court membership予約と進行中のGDN認証workspaceを確認し、重複する変更は統括と調整する。
- scratch buffer、Set/Map、canonical encoding、clone、negative cacheの確保前に必要量を見積もり、scope/worker/session間へtokenを明示的に移譲する。
- Bのgraph/calendarへ同じresource vectorを接続する。キャンセル、panic、再試行、checkpoint失敗で二重解放・予約漏れを起こさない。
- RFC16の容量・不足reportを実測・実予約から構築し、未知と実ゼロを区別する。常時Noneのまま新しいwrapperだけ追加して閉鎖しない。

主所有範囲: resource/admission/workspaceの専用leaf moduleと試験。中央state接続は統括へ。GDN disclosureの認証前確保を含む残件は現callerを再確認して着手する。

受入: 実callerで上限直下の拒否→十分な上限で成功、失敗時のstate不変、成功後の最後の使用まで保持、取消し/再起動で解放を確認する。全モデル費用境界は実測範囲外を明記する。

## Claude B — 因果graph・全branch・期限calendar

対象: RFC23・25・28・29・30・31、RFC26のbranch構造。

目的: ChainClaim中心の空inputsや局所court graphから、採択済みnative/EXEC/material/provenanceの依存閉包へ拡張する。

- preparation、default、non-ORIGINAL dutyも同じgraphへ収容する。合法terminal、branch条件、依存順序、packet対応をRFC根拠付きで列挙する。
- claim-local frontierとcheckpoint builderへ実入力を接続し、EXEC transaction/material/provenance依存を欠落させない。全体走査の追加で有界性を壊さない。
- 全最遅branchが元期限内に収まることを検査する。一般resource-vector calendarと、仕様で許されるbounded混雑extensionを接続し、依存termsを原子的に更新する。
- `OriginalAggregateDraft::reservation_readiness` の常時 `UnresolvedSupplyContract` を、C/Fから取得した実契約の成否に基づく判定へ進める。架空supplierや無条件Readyに置換しない。

主所有範囲: graph/frontier/aggregate/calendarのleaf module。金銭式はC、material transportはF、実allocationはAが所有する。

受入: 多branch・最遅branch・欠落依存・cycle・二重参照・供給未準備を実builderで検証する。root受付前の一括予約失敗で部分更新しないこと、checkpoint/reorg後も同じ閉包になることを確認する。

## Claude C — 資金・供給・retention・一回決済

対象: RFC08・16・26・32・33・34・35。RFC27は採択状態の確認と依存切り分けを含む。

目的: graph上の義務と実原資を結び、production funded schedule/settlement resolverの未接続を、既存採択済み仕様の範囲で解消する。

- fee/capital/retention/profile/roleごとに仕様出典と入力提供者を特定する。常時Err/Noneの箇所を、解決可能な条件と未策定条件に分ける。
- payer、storage price、TTL、全branch material供給、refund/void、rewardの一回精算を既存native台帳へ接続する。root受付前にcapacity/funding/retentionを一括確保し、失敗時は原子的に戻す。
- RFC33の新fee/返還/3MSK slashは採択済み条件・責任・額を確認してから接続する。private economicsの値を公開presetに流用しない。
- RFC27のAccountID・元本lot・新責任/料金/RPCは、proposalと採択済み仕様を区別する。未採択の経済policyを独断で新設しない。依存しないcodec/型/検査も既存IDの意味を変更しない形で実装する。
- native39決済写像は未策定。勝手なW/PWU変換やoperator/anchor推定を実装しない。必要入力・不変条件・具体的な未決定項目を提示し、他の既定経路の接続を進める。

主所有範囲: funding/supply/settlement ledgerのleaf moduleと試験。Bからbranch、Aから予約、Fから保持実績を受け取る。

受入: 実台帳で徴収→実行→決済/返還の保存則、再実行のexactly-once、restart/reorg、資金不足時の無変更を確認する。実coinbaseに到達しない試験はその限界を明記する。

## Claude D — 全stage検査・4adapter・外部検証者

対象: RFC07・10・13・14・24、RFC08 providerの検査consumer。

目的: 検査helperを、実adapter・provider・claim認証・courtまでの一つの経路へ接続する。

- RFC本文から必要な4adapterと全stageを特定し、現実装と対応表を作る。実adapterごとに同一のtyped検査結果・費用/byte/work/IO/RAM報告を実callerへ返す。
- 全対応class/shape/fault/answerについて、original起点のdoor、局所化、challenge、courtを接続する。回答不能・取得不能と計算誤りを区別し、availability失敗を自動的に計算誤りの有罪判定にしない。
- 非Panel外部1人が取得・検査・提出できる経路を作る。producerの非公開状態や全件展開に暗黙依存しない。必要なmaterialはF、資源枠はA、原資はCへ依頼する。
- 承認済み独立sourceが未提供でも、adapter・直接検査・拒否経路の実装は進める。fixtureの別process/別鍵を独立性や抽選承認の実証にしない。
- 署名provider/socket→native challengeの確定済み試験を引き継ぐ。paid-check authorityとnormal-source typed読取の進行中差分は統括が所有権を確認する。別の擬似checkerを作り直さない。

主所有範囲: verifier/adapter/checkerの専用leaf module。共有provider material protocolはFと契約する。

受入: 実adapter→実claim→実検査→公開証拠→native courtの陽性・陰性対照、全stage interruption/資源不足を確認する。実Llama、ctx2M、全形状等の未実行範囲はTESTとして残す。

## Claude E — 出力経路・winning carrier・worker接続

対象: RFC01・03・18・20・38。

目的: 実gateway/worker/SDKの出力を、actual winning carrierとnative contextへ結び付ける。

- actual winning carrier indexのsource of truth、更新・rollback・checkpoint規則を既存仕様から確定し、plain workerのcontext/native handshakeへ接続する。
- live TFP1/TFO1 producer、SDK/worker、他gateway出力経路を同じ認証済みcontextから構築する。独立したtest専用出力系を増やさない。
- completion/cancellation/retry、履歴再読込、複数processでの重複出力・取り違えを防ぐ。署名・provenance・artifactが同一release世代に対応することをGへ渡す。
- RFC18のT0f/T1e/T3c/T3d等の最上位process試験へ接続する。認証再実行費用/grindingの測定条件を明示する。
- RFC20のKV diskspillは条件付き残件であり、現在32Kで必要性が示されていなければ強制導入しない。延期media経路をshared workerから開放しない。

主所有範囲: gateway/SDK/worker/output indexのleaf module。合意側index接続は統括、material配信はFが担当する。

受入: 実workerとgatewayを起動し、正しいcarrierへの出力、誤context拒否、cancel/restart/reorg、複数gateway整合を確認する。SDK単体成功だけでRFC全体を閉鎖しない。

## Claude F — material・provider lifecycle・streaming import

対象: RFC08・32・37、RFC02のimport資源境界。

目的: materialをcollection/codec/assembly/serve/importからrestart/reorg/pruneまで一貫して運ぶ。

- 全collectionのserve/codec/typed assemblyとNativeDemandを接続する。欠落collection・重複・別世代・不正provenanceを明示的に拒否する。
- new wire/resumeを既存互換性方針に沿って接続し、有界frame・有界buffer・途中停止・再開位置を検証する。resumeで認証・順序・費用計上を飛ばさない。
- provider replacementの実受入と、retention中/満了後のmaterial責任、restart/reorg/pruneで必要なmaterialを閉じる。実price/payer/TTLはCから取得し、保持policyを独断で作らない。
- semantic tableは現状borrowed供給でmaterial target対象外という境界を守る。変更が必要なら設計判断として分離する。
- 既存のnative pruned importや実provider/reorg/prune試験を再利用し、まだ閉じていない実P2P経路を明示する。native直接importをP2P受入の代用にしない。

主所有範囲: material collection/transport/import/provider lifecycleのleaf module。Dは検査判断、Fは供給・伝送・保持を所有する。

受入: 実serve→wire→assembly→native import→cold reopen、replacement、reorg/prune、切断resume、サイズ超過を同じ候補で確認する。資源不足で受理済みmaterialを失わないことを確認する。

## Claude G — L2/EVM・release・復旧campaign

対象: RFC09・11・12・17・19・22、RFC03/04/15の拒否受入。

目的: 残る実runtime経路を接続し、同じsource/releaseの証拠で受入可能な形へ整える。

- RFC09のpost-checkpoint L2自主検証/provider discoveryを既存仕様の範囲で実装し、trusted checkpointだけの経路と区別する。
- RFC12のLatest/Safe/Finalized、bridge/market fill、履歴署名を実consumerから調べ、接続可能な残件を実装する。EVMの既修正gas cap/empty/full整合を後退させない。
- old/new binary・catalog/release signature・gateway/worker/artifact・R1〜R8各roleの同一世代記録を作る。RFC17/19はコード充足済みという監査範囲を尊重し、受入不足を新しいモデル審査で埋めない。
- 延期featureの全entry/shared path/restart/resync拒否matrixを現releaseで実行する。Noneの設定確認だけでruntime拒否を断定しない。
- 15 gateのrunner・入力manifest・失敗保存・再開とUNKNOWN集計を整える。小規模の実複数process対照を先に実施し、100k/pruned IBD/OS crash等は十分な資源と対象環境がある場合だけ実行する。SIGKILLだけでOS crashをPASSにしない。
- K3入力未作成とnative39写像未策定はEXTERNAL/POLICYとして維持する。runner接続や未入力拒否は実装できるが、本番受入には数えない。公開稼働nodeの停止・入替・DB操作は行わない。

主所有範囲: L2/EVMの専用leaf module、release/campaign runnerと証拠。中央forkchoice/API接続は統括へ提出する。

受入: 実processと固定binaryでRPC/再起動/reorg/拒否matrixを検証する。公開履歴のheader取得だけではEVM互換性・履歴署名・campaign gateのPASSにしない。

## 統合時の完了条件

構造作業の完了には、型の追加に加えて実consumerへの接続、失敗時の原子性、所有権の終端、必要な永続化/復旧試験が必要。未策定仕様をtyped未準備で保持するのは適切な部分成果だが、仕様未確定の要件が閉じたことにはならない。

全担当差分を同じ統合候補で再確認し、少なくとも `git diff --check` と `python3 docs/maintenance/check_docs.py` を実行する。必要なRust試験は統括がcrate/featureと実callerに応じて選び、稼働中ビルドと競合させない。機械的なリンク検査だけでprotocol受入を完了させない。

最終報告は「要件単位で閉じたもの」「実装済みだが実受入待ち」「外部入力/採択待ち」「延期/対象外」を分け、全38本の件数は再監査後の値だけを示す。
