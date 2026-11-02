> **2026-11-03採択済み:** ユーザーが対のRFC/ADRを明示採択。正式文書は[RFC-0046](../../../rfc/0046-positive-native-funded-relay.md)。以下は採択前原文であり、実装・検証・有効化完了を意味しない。

# RFC番号未割当 — Positive native原資の先行relay資料と有限custody

- 状態: **未採択の設計案。実装許可、稼働中の受付、本番数値、公開有効化を意味しない。**
- 日付: 2026-11-03。
- 対の判断案: [Relay ADR draft](positive-native-relay-adr-draft-2026-11-03.md)。
- 基礎: [RFC-0045](../../../rfc/0045-positive-court-canonical-assignment.md)、[ADR-0213](../../../adr/0213-positive-court-canonical-assignment.md)、[PRINCIPLES §11](../../../PRINCIPLES.md#design-freeze)。
- **採択済みRFC45への追記ではない。** 新local op14、relay資料の受付・custody owner、追加row/material namespace、原資受付の新前提は、採択済み8–13を超える別の設計判断である。RFC/ADR双方のレビュー・明示採択を要する。番号・wire/domain/DB suffixは本書では予約・割当しない。

以下のMUST/MUST NOTは、対の案が採択された場合だけの提案規則である。

## 1. 問題と限定した到達点

RFC45のA、後続C、op13 prover原資は実payer P2PKHを使い、通常のML-DSA script認証を省略しない。nativeのqualified-spend検査は、正しく署名されたtransactionの出力・保存則・権利を制約する。それだけでpayerの将来署名は生成できない。nativeのdueや返金可能額を計算できても、payerが署名を止めれば実UTXOを動かせない。

本案は、サービス開始前にpayerが一度発行する**通常transaction用の署名**を、別の先行carrier Rで正規保存する。将来原資carrier FはRをhashへ戻さず、nativeがF自身の実funding outpointからRを解決する。以後、relayは保管済み署名と自分のfee入力でnativeが現在認める精算を搬送する。これは新発行や支払権の委譲ではない。金額、recipient、service due、WorkerPayment、未終了duty、capital/liability、onceは実native ownerが毎回判定する。

本案が目指すpayer独立性は、**既に受理・保管された現在cursor用のRがあり、実義務が有限に終了したとき、追加payer署名なしにそのcursorを一回全額精算できること**である。relay、inclusion、必要資料の取得・保持、既存P/fraud等の実ownerに関する前提は残る。ネットワーク全体の無条件の時間保証とはしない。

任意の部分引出しをpayerから独立させる案ではない。部分精算後の新cursorには、その具体的なoutpoint向けR2を**親の部分精算を受理する前に**正規保存する必要がある。R2がなければ部分精算を拒否し、旧cursorとその既存Rを保つ。terminal精算と条件付きpartial進行を区別する。

本案は、実P/source/duty/liability/history ownerの欠損を解決しない。P policyやpending-fraudを短縮したり、保存Pass・整数floor・任意trait・local replayを支払能力にしたりしない。これらが未解決ならF受付又は支払をtyped missingで拒否する。

## 2. 現コードの根拠と循環

2026-11-03の作業checkoutで確認した箇所を示す。以下は既存コードの説明であり、relay受付の実装済み宣言ではない。

| 根拠 | 現在の性質 |
| --- | --- |
| `consensus/core/src/hashing/sighash_type.rs`、`ALLOWED_SIG_HASH_TYPES_VALUES` | `NONE | ANY_ONE_CAN_PAY`、値`0x82`は既存の許可型 |
| `consensus/core/src/hashing/sighash.rs`、`mldsa87_*` / `calc_mldsa87_signature_hash` | NONEはoutputs hashをzero、ACPは他入力集合hashをzeroにするが、現在入力とpayload全文等は署名する |
| `consensus/core/src/hashing/tx.rs`、`id` / `write_transaction` | 非coinbase txidはsignature script・sigop count・massを除外する。payload、inputs/outpoints、outputsは含む |
| `crypto/txscript/src/lib.rs`、`execute_script` / `check_error_condition` / `check_mldsa87_signature` | signature scriptはpush-only、最終cleanstack、key/signature長は固定 |
| `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs` | PQ input class確認と通常script実行が必要。OP_TRUEやunknown-versionで代用できない |
| `wallet/keys/src/kaspa_pq.rs`、`sign_transaction_inputs_mldsa87` | 汎用wallet helperはALL固定。限定relay signerは未実装 |
| `consensus/core/src/palw_positive_court_v2/coordinator/assignment_body.rs` | 現在の新bodyは8–13。op12はAssignmentId、optional Case、bounded receipt参照 |
| `consensus/core/src/block/transactions_v3.rs` | serializationは既存transaction vector。local allocation ticketはsidecar資料ではない |

Fの出力を将来spendする署名はF.txidを署名する。その署名をF.payloadへ入れると、F.txidが当該署名に依存し循環する。Fの署名scriptへ任意資料を隠す方法も上記script規則を満たさない。txidの定義やr1 projectionからfieldを追加で省略して解決してはならない。

依存関係は次に限定する。

```text
literal F（Rへの参照なし） → F.txid / funding index / amount / payer script
                         → relay署名 → Rのcanonical body / R.txid
Rの先行受理・実custody    → F受付時のnative lookup（Fのhash入力ではない）
```

Fの通常input signature scriptはtxidから除外されるので、Fの署名を完成してから又は完成する前にRを準備できる。ただしFのtxid対象fieldを変えれば別Fとなり、対応する別Rが必要である。Fの同一txidを理由に通常scriptを省略しない。

## 3. 変えない契約

- payer outputは既存ML-DSA P2PKHのまま。既存script、sighash、txid、通常fee/maturity/sequence/mass規則を変更しない。
- r1 Result/Case/Service/FundingConsent、二field-zero funding projection、P subject、旧TFO1、KernelJob/Native39のIDと意味を保持する。
- A/C/op13は別の実原資。元のpayer consent、refund、worker、fee/capital/reward/total caps、期限、actual own output、現在UTXO/他lockを再検査する。Rはそれらの代用にならない。
- Assignment nonce、全Result横断一消費、全prover合計予算、署名済み取消条件、once、未払due/floorを維持する。R/R2で新予算・新clockを与えない。
- WorkerPaymentは既存Positive/P等の用途別条件を要し、Reward・発行・weight等へ伝播しない。B=F=R=0を保つ。
- モデルの1.0/0.3、年次指定、モデル審査不採用を変えない。本案のhashと署名はデータ同一性・認証に使い、モデル計算の正しさを暗号学的に証明する方式を用いない。

## 4. 固定relay transaction template

最初のprofileは一種類、`NativeOp12NoCaseNoReceiptsV1`を提案する。

| field | 固定値／native導出値 |
| --- | --- |
| transaction version | 0 |
| locked input | 必ずinput 0。登録対象のexact native outpoint |
| populated entry | 登録で宣言するexpected amountと、native payer keyから導出する既存P2PKH script |
| input sequence / sigop count | 0 / 1 |
| lock time / gas | 0 / 0 |
| subnetwork | 既存`SUBNETWORK_ID_PALW_LIFECYCLE` |
| payload | 既存lifecycle version、object176、literal `(2,1,op12{assignment,case:None,receipts:[]})` |
| sighash type | 既存`SIG_HASH_NONE | SIG_HASH_ANY_ONE_CAN_PAY`、`0x82` |
| signature context | 既存transaction用`kaspa-pq-v2/tx/mldsa87` |

現在のraw wrapperではpayloadは77 bytes（outer 2+1、body 4+1+64+1+4）となる。実装時のcodec試験でexact bytesを固定し、他のwrapperや任意payloadを同一templateとして扱わない。template IDはこの正規導出規則と全固定fieldへbindする新purpose domainを中央登録する。

この署名で変えられるのは出力と他のfee入力である。現在入力のoutpoint、entry amount/script、sequence、sigop count、transaction version/lock time/subnetwork/gas、payload全文とhash typeは変えられない。署名hashに入らないentryのDAAを、future AcceptedOriginとして作ってはならない。

signerは、outputs空の**署名hash計算用template**を使ってよい。それを有効なbroadcast transactionや受付済みUTXOと扱わない。実spendは現在nativeが導出した全出力、必要なforeign fee inputs、通常のcanonical `<signature || 0x82> <payer key>` unlock scriptで構成し、Full script検査を通す。

このprofileのop12で`receipts:[]`は「carrierがreceipt subsetを選ばない」を意味する。nativeは実rooted historyから、当該accountについて現在適格なdue・refund・burn・必要continuationを全て導出する。空リストから「serviceなし」「P認証済み」「残責任なし」を導かない。`case:None`でも、Consumedならactual AssignmentUseとexact Case/Resultを解決する。input 0の現在lockからA/C/prover purposeを一意に決め、複数locked inputsを拒否する。既存の通常payer署名による非empty receipt op12をこのtemplateへ読み替えない。

## 5. 新しい登録operationの提案

### 5.1 local op14

object176内のappend-only local `RegisterFundingRelayV2 = 14`を提案する。現8–13 decoderを黙って14対応にせず、独立したfence・bounded scanner・signature/work quoteを登録する。global object tagは追加しない。

bodyは`version:u16=2, revision:u16=1, operation:u8=14, terms:FundingRelayTermsV2, purpose_signature:[u8;4627], relay_signature:[u8;4627]`とする。keyはtermsのnative bondから解決する。外側transactionの全通常認証とmass/feeを別に検査・計上する。

`FundingRelayTermsV2`のfield順序を次の案とする。enumは有限なfixed variantsのみで、untrusted Vecは設けない。

| 順 | field |
| --- | --- |
| 1 | version:u16、revision:u16 |
| 2 | contract_network:Digest、contract_ruleset:Digest、assignment:AssignmentId、contract_anchor:NativeHash64 |
| 3 | relay_ruleset:Digest、relay_policy:Digest |
| 4 | payer_bond:NativeOutpoint、payer:PrincipalId、refund_payload:Digest |
| 5 | funding_purpose:RelayFundingPurposeV2、target:RelayTargetV2 |
| 6 | funding_outpoint:NativeOutpoint、expected_amount:u64、template:RelayTemplateV2 |
| 7 | register_before_daa:u64、funding_admission_before_daa:u64、custody_until_daa:u64 |
| 8 | metadata_fee_schedule:Digest、physical_custody_profile:Digest、max_metadata_fee:u64 |

purpose tag案は`PosterA=0 / CaseC=1 / ProverJoin=2`。targetは次の四種類を提案し、purposeとの組をnativeで限定する。

- `InitialAssignment { bundle:Digest, poster_nonce:u64 }`。
- `InitialCase { result:ResultId, funding_scope:FundingScopeId, consent:FundingConsentId }`。
- `InitialProver { prover:NativeOutpoint, join:Digest }`。
- `Continuation { original_funding:NativeOutpoint, predecessor:NativeOutpoint, parent_spend:NativeHash64, original_contract:Digest }`。purposeは元accountを維持する。`parent_spend == funding_outpoint.transaction`を必須とする。

terms IDとpurpose署名contextは新purposeとして別登録する。domain候補は`misaka/positive/funding-relay-terms/v2/r1`、purpose署名context候補は`misaka-positive/funding-relay-register/v2/r1`。これは既存transaction署名のcontextを変えない。terms IDは両署名、R.txid、accepted origin、post-state root、actual local leaseを含めない。literal R material digestは全body14（両署名を含む）の別domain hashとし、termsへ戻さない。

### 5.2 Rの受付が確認するもの

1. native fence、network、relayの実既発火rule scope、signed contract anchorのactual ancestry、完全な有限policy/schedule/profile、frame/signature/heap/work/DB caps。
2. actual native bond/keyとpurpose認証。initial Aは現在のposter、initial Cはaccepted Assignmentのposter、initial joinはjoin本人のprover/payerへ限定する。initialは実仕事受付資格を確認する。continuationは元accountの保存key・契約権を確認し、退役した既存契約を新しい仕事受付と混同しない。
3. expected scriptはnative keyの既存P2PKHへ独立導出し、refund payloadとpayer Principalを検査する。expected amountは正でnative金額範囲・R profileの宣言上限内にあることを確認する。既に読めるAssignmentの将来C上限又はcontinuationの現在残額も越えないことを確認する。まだ搬送されていないFのconsent/basisを検査済みとは扱わず、実Fで全上限を再検査する。
4. 宣言されたexpected entryと固定templateから通常ML-DSA transaction sighashを再計算し、relay署名を実検証する。**このexpected entryは署名確認用DATAであり、将来UTXOの存在又は受理を認定しない。**
5. 固定target宣言と現在実際に読める契約をboundedに照合する。initial Aのbundle digestは将来Fとの一致用であり、まだないfull bundleの検査成功ではない。nonce参照だけでnative counterを増やさない。initial C/joinは現在accepted Assignmentとそのscope/capsを確認するが、未搬送の新r1 consent/joinを受理しない。continuationは現在の元account・predecessorを確認するが、未来parent spendの適格性や新出力の存在を認定しない。
6. actual R carrierの利用可能feeからmetadata chargeを一度割当し、実custody・DB write/replay容量を同じnative checkpointで確保する。資料、row、fee charge、pending countは一緒にcommit/undoする。

初期profileのinitial A登録は、宣言nonceがnativeの現在next nonceと等しいことだけ確認できる。counterがなければ既存規則のnext=0として読み、Rだけでcounter rowを作らない。同nonceで複数の将来Fを準備できる量はpending capsで制限し、最初のF成功後に他のRが新Assignment権を持つことはない。

Rのみでassignment/job/Case、実funding output、coin lock、通常推論receipt、役割契約、Assignment nonce消費、計算credit、WorkerPayment、発行・payoutを作らない。Rが登録したのは、料金を払った有限metadata custodyだけである。

同じexact outpoint/purposeの再送は、full terms・両署名・全materialが元のaccepted preimageと一致し、実custodyが維持されている場合だけ無効果に扱う。異なるterms、署名byte、target、custody期限への差替えをidempotent成功にしない。通常carrier feeと二回目のregistration chargeを区別する。初期profileにpayerによる早期R取消・material回収・同key上書きは設けない。

## 6. FとRの具体的な順序

初期profileでは、Rが**Fを検査するselected-parent canonical state**に既に存在することを要する。同じcarrierへ同居させず、同じblock内の未確定な先行処理へ依存しない。将来、同mergesetの先行Rを認める場合は、普通のUTXO walk前にも見えるactual prefix interleaveと共通mempool/template規則を別profileとして検証する。

1. payerはliteral op8/op9/op13のF、actual funding output、元の必要同意を完成し、Rに依存しないF.txidを確定する。
2. payerはそのexact F outpoint/amount/P2PKHに対する固定op12 relay署名と、新purposeのR terms署名を発行する。
3. Rを通常transactionで搬送する。nativeが署名、実metadata fee、caps、custodyを受理し、正規rowとraw materialを保存する。これが失敗してもFはまだ仕事・Case・原資契約として受理されていない。
4. Rのcanonical受理を確認後、Fを搬送する。F payloadにR digest/txidを追加しない。
5. F受付はactual own funding outputと現在composed UTXO、元の全署名・scope/caps、bond、capacity、nonce/onceを再検査する。exact outpoint＋purposeでRをnative lookupし、現在lineageのR origin・full material・署名・custody残期間・target一致を確認する。actual entryのamount/scriptでrelay署名をもう一度検証する。
6. Fの必要なA/C/join row、lock、役割・資源、実R bindingを一checkpointで確定する。R receiptだけで実原資やservice capacityを充足したと扱わない。

元のr1 FundingConsentとoutput-dependent scopeはそのまま検査する。C用R termsがResultIdを持つのは、完成したC carrierを準備した後であり、Result前のAに架空Resultを置かない。R purpose署名が元payer consentを置換することもない。

RだけがcanonicalでFが来なければmetadataサービスとその費用だけが成立する。Fが普通のtransactionとして受理されても、R等の前提を欠いてinitial native contractが拒否された場合は、そのpayer出力に契約を作らない。既存契約のlocked inputを消費する部分精算は別であり、後からnative処理だけをskipしてspendを残してはならない。

## 7. 部分精算とterminal精算

### 7.1 全current cursorに残す不変条件

新profileの全live accountは、現在の実funding cursorと同額・同scriptのaccepted Rを持ち、そのRは少なくとも必要なterminal/rollback期間を実保持する。accountの元funding outpoint、budget、scope、once subjectはcontinuationで変えない。

最初のFでこの条件を作る。部分精算Sは、旧cursor用の既存Rでinputを認証できるが、新cursorを作るなら、その具体的なS.txid/output index/remaining amount向けR2をSの前に受理していなければならない。SはR2 hash/txidを参照せず、native lookupする。

Sのoutputsやfee inputsを変えるとS.txidが変わり、R2のtargetと一致しなくなる。relayが自由に後からそれらを変えてもpartialが成功するとは保証しない。payerはSの具体形を確認してR2を署名・登録する必要がある。

S受付は、旧current row、適格due、WorkerPayment条件、未払due/floor、実出力保存則、既存once、R2のactual accepted material/署名/custodyと元accountへのcontinuation linkを、同じcurrent prefixで再検査する。R2は新A/C/join、新予算、floor解放、義務終了、未来AcceptedOriginを作らない。

R2がなければ**Sのlocked-input spend自体を拒否**し、旧UTXO・row・R・nonce/onceを維持する。通常UTXO更新の後でmetadataだけを拒否する実装は不適合である。R2があれば、SのUTXO spend/new output、account debit/current cursor、R2 binding、once、settlement/journalを同checkpointで行う。

### 7.2 terminal pathの正確な保証

actual native ownerが全サービス・P・fraud・duty・capital/reservation/liabilityの終了を確認し、未払適格dueを確定できたとき、現在cursorのRは次の一回全額精算を許す。

- WorkerPaymentが適格なら契約どおり一回払い、不適格が確定していれば成功対価を返金する。
- 適格service feeを実receipt/providerへ払い、署名済みの実burnがあれば一度だけ破棄する。
- 解放可能なfee/capital/surplusを正規payerへ返す。残額はzero、continuationなしとする。

これらの出力と全額保存則はnativeが拘束する。NONE署名を任意recipientへの送金権にしない。feeはforeign inputsが負担し、同じfeeをbond burn・relay charge等へ二重割当しない。

terminal時は新UTXOを再lockしないのでR2を要せず、追加payer署名も要らない。partialを拒否して旧cursorを維持した場合も、その旧cursor用Rが残る。部分精算を一度受理した後は、最初の消費済みRを再利用するのではなく、受理済みR2が次のcurrent Rとなる。

これはPを短縮する保証ではない。P Pending中にworker reserveを返したり、時計だけで未終了dutyをzeroにしたりしない。有限な全branch終了bound、実history/custody、relay供給・inclusionの前提がなければterminal完了を宣言しない。

### 7.3 RFC45の残る条件付きpartial進行

RFC45はP未完／未検証でも適格service feeと安全返金を別判定し、部分返金floorを検査する。本案はその権利を消さない。payer署名付きSと事前R2が揃えば、Pを待たずにnativeが認めるpartialを進められる。

ただしpayerが新R2を拒めば、そのpartialを強制できない。本案は「全ての安全partialがpayerなしで必ず進む」と主張しない。この限界、partialが条件付きであること、およびfinite terminal fallbackを区別して採択・測定する必要がある。terminalだけを動かしてRFC45の全partial要件が完成したと扱わない。

## 8. 有限metadata料金・custody owner

新`FundingRelayPolicyV2`と完全なfee/physical profile preimageを、Some-only Params fingerprintとactual fired-rule scopeへ登録する案とする。既存r1 ruleset recipeは変えない。contract scopeと新relay metadata serviceのscopeを区別し、後のfork policyで元契約を解釈し直さない。未定値には公開defaultを与えず、private finite preimageを必須にする。

最低限、global/per-poster/per-proverのpending registrations、同account/nonceの候補数、全continuation数、全stored rows、expected amount宣言上限、raw/decoded/frame bytes、署名数、temporary/retained heap、DB/batch bytes、replay reads/work、登録window、funding window、custody期間、rollback/pruning marginを正かつ有限にする。zero/MAX sentinelを無料・無制限の代用にしない。期限と料金の全加減乗算をcheckedにする。

登録料金はadmission＋署名検査＋全row/material bytes＋必要retained-byte×DAA＋worst replay/retirementの有限quoteを持つ。最初のprofileでは、**Rのactual available native transaction feeからquote額を一度burnし、既存のcollectible feeから差し引く**方式を提案する。元A/Cの将来refundを新予算とみなさず、原資不在のcustody報酬queue・mintを作らない。ordinary tipは残りの実feeだけである。失敗したRの通常feeと、成功したmetadata registration chargeを区別し、同じRの正規idempotent再送から二回目のregistration chargeを作らない。

purpose署名はこのnonrefundable metadata chargeの上限も認証する。Rの普通のfee inputsの所有者は通常scriptで支出を認証する。Fが来なくても、実登録・保管した有限metadataの料金をA/Cの架空債務へ付け替えない。

必要ownerは、actual current-prefix native budget writer、共通fee allocation owner、実RAM lease、実RocksDB hard-quota/WriteBatch、retention/replay/cleanup ownerである。local capacity failureは再試行するlocal resource errorであり、モデル計算の有罪や偽service receiptにしない。on-chainの有限resource bookとlocal physical ticketを区別し、lease IDをR termsへhash backしない。write成功のboolや自己申告capacityを実custodyに昇格しない。

Rが受理されたという事実だけで、FのP/source/fraud/reservation全branchがreadyとはしない。Fは元のactual ownerから最遅終了・資料保持・返金・rollback boundを解決し、Rのcustody残期間が覆うことを確認する。missing actual ownerは別の受付拒否であり、短いP TTLで補わない。

`register_before`と`funding_admission_before`は新R/新bindingの受付期限であり、既にboundとなったcurrent Rのterminal署名を無効にする期限ではない。現在UTXOが残っているのに、この時計だけで署名packageを消してはならない。Fの実capital/physical quoteは、current Rの完全な固定packageをlive accountのnative state slotとして保持する容量も含める。このslotが実際に保有するのは、少なくともexact terms/template/expected entry、payer key、purpose署名byte、relay transaction署名byte、actual accepted bindingと再認証に必要な資料であり、hashや保存済み成功flagだけではない。historical raw journalをpruneした後も、この完全なcurrent bound packageとnativeのaccepted provenanceを実readerが再生できなければならない。current packageは各current cursorに一つ、rollback用旧packageは元契約の有限continuation/retention上限内で予約し、固定expiryでlive署名byteを削除しない。

Rのpending custodyからFの実account slotへの移管、R2をcurrentとした際の旧packageのrollback枠を同writerで実予約する。これは新資金や追加請求権を作らず、同じaccountの署名capと全continuation上限内で行う。raw journalを回収できるのは、必要なaccepted資料が別の実native保持先から完全に再生できる場合か、account終了・rollback/pruning条件を満たした場合だけとする。

期間単価のquoteは実policyの有限義務・relay/refund inclusion windowと明示した供給/inclusion前提を含む。無期限censorshipまで料金で保証したと扱わず、前提外でもclockによる資金解放や署名資料の安全性を破る回収はしない。必要なcurrent state slot・延長保持を実ownerが確保できないprofileはFを受理できない。保存できるというinteger summaryだけをこのownerの代用にしない。

## 9. namespace・固定row・DB materialの提案

### 9.1 aux151 append候補

既存0–18を保持し、次の19/20を**中央レビュー対象の候補**とする。番号・実byte長・domain文字列は採択とinventory更新後にのみ確定する。

| kind候補 | key / value案 |
| --- | --- |
| 19 FundingRelayAuthorization | key: exact funding outpoint＋purpose tag。value: full fixed terms、native payer key、一つのpurpose signature、actual R origin/material、metadata quote/charge参照 |
| 20 FundingRelayCustody | 同じkey。value: terms ID、一つのrelay transaction signature、actual custody/reservation/material参照、保持clock、optional exact binding（actual F/S origin、元account/contract/current predecessor） |

key案は既存Borsh `(2u16,1u16,kind)` 5 bytes＋NativeOutpoint 68＋purpose 1＝74 bytesであり、142-byte上限内にある。AssignmentIdとprover bondはvalueへ認証保存する。同じoutpointを異なるpurpose/Assignmentに二重登録・二重bindしない。有限三purposeの同outpoint範囲を照合し、hashだけの曖昧な重複検査にしない。

各rowは一つの署名と固定fieldのみとし、16,384-byte ceiling以下のexactサイズを試験する。full metadataをrow内の無制限Vecへ詰めない。19/20の両方、署名、full material、custody、fee、pending countが一checkpointに属する。20のbindingはactual F/S受付でのみ作り、R受付時に未来originを埋めない。

全pending countはrooted rowsから再現可能であり、有限global上限に収まるindex/range scan又は明示したtyped countersで管理する。counter追加が必要なら、そのkey/codec/undo/snapshotも中央schemaとして確定し、local singletonに依存しない。F成功時のpending→bound、unused R expiry、reorgの各増減を同じwriterで行う。

### 9.2 raw DB material

新DB material候補は`DatabaseStorePrefixes::PalwStateV2Tip`内の**suffix 0x4c（未割当提案）**＋64-byte relay material digestとする。2026-11-03読取時点の対象storeでは0x49がnative identity、0x4aがCase journal、0x4bがAssignment journalであり、同じ対象内の0x4c使用は検索で見つからなかった。これは全repository・全pending branchの中央collision review完了を意味しない。global DB prefixを無断追加しない。

raw materialは正規body14の全byteとactual accepted framingを保存する。digest一致だけでorigin又はcustodyを認めない。現在canonical row19/20から選ぶprivate readerが、先に有限read/bytes/work/heap枠を確保し、実accepted R bytes、両署名、元scope、target、fee、custody、F/S bindingを再生する。world-wide local spent fileやpeer提供rowで成功を補わない。

DB stagingはordinary UTXO/PALW state、metadata burn、rows/counters、native reservationと同じWriteBatchに束ねる。orphan raw bytesは残ってよいが、row/root/lineageなしには権威を持たない。memory上のDB接続を保持したrestartだけでなく、全接続を閉じて再openする試験を要する。

### 9.3 collision inventoryと互換性

| namespace | 現状／本案 |
| --- | --- |
| global lifecycle object | 176を維持。別global tagは提案しない |
| object176 local operation | 0–13を保持し14を提案。未採択時はpredecode前に拒否 |
| aux151 local key/value kind | 0–18を保持し19/20を提案 |
| aux140 settlement once | 既存legの意味を借りず、採択済みAssignment支払purposeとのexact debit/onceへ接続。Rだけで支払markerを作らない |
| DB material suffix | 同storeの0x49/0x4a/0x4bを保持し0x4cを提案。中央登録は未了 |
| domains/署名context | R terms/material/purpose/templateの新domainをinventoryで衝突確認。既存transaction contextは不変 |

None時は全shipped fingerprint/root/wireを不変にする。`CooperativeV1 / RequiresR_V2`というfunding relay profileの選択は、**元のAssignment intakeのactual fired scopeで一度だけ決定し、その認証済みpolicy/materialへ固定する**。後続C、全prover join、continuation、取消・期限切れ・精算まで、この元の選択を維持する。Fが後日のcurrent Paramsで処理されることを理由に、profileを再選択しない。元の選択を再生する資料がなければUnavailableであり、都合よくどちらかへfallbackしない。

後日のrelay activationは、既にacceptedの旧Assignment、その将来C/join、既存funding accountへRを遡及必須化しない。旧契約は元のcooperative profileとそのpayer署名依存・実liveness限界を維持する。旧署名・元r1 consentをrelay登録認証として再解釈せず、新保証付きへ自動昇格しない。逆に、元からRequiresRのAssignmentについても、後日のforkで原資料・codec・継続サービスを消したりRequiresRを外したりしない。満期・責任終了まで元profileを実行できるよう保持し、変更するなら別の明示的なfork移行規則を要する。

**既存accountの任意opt-in／移行は本案で定義・承認しない。** 既存契約への必要な合意、追加費用、資源、署名、支払権・返金権の保存を含め、別の設計判断とする。公開presetはNoneを維持する方針であり、公開網のlive Assignmentが存在しないと本draftが実査・確認したわけではない。実装・移行・公開判断前に全presetと対象網のactual stateを確認する。live契約不在という未確認の仮定を、遡及適用の安全性の根拠にしてはならない。

## 10. reorg・復旧・pruning

- Rのcanonical受理をundoしたlineageでは、そのRを必要とするF/Sのnative受付も再検査される。raw bytesが残ってもRを復活させない。
- F又はSのundoは、funding cursor、R binding、pending count、資金/役割/nonce/Case、actual payment/once、journal、UTXOを一緒に戻す。R2がorphan又は未bindへ戻ったことから新予算を作らない。
- 現在cursorのRと、rollbackで必要になる過去cursorのRを、actual policyの必要保持期間まで残す。old Rを新R2で上書きしない。
- snapshot/IBDはcurrent accounts、row19/20、必要pending/binding証跡、native counts/reservations、raw materialを同じrootに照合する。通常sig検証、actual accepted ancestry、元のscope、feeとcustodyの再生を省略しない。
- unused Rはexclusive funding expiry後、実custody契約終了とrollback/pruning条件を満たしてから回収できる。bound Rはaccountがdrainedでもrollback horizonを確認する。時計だけで既存義務・coin lockを解除しない。
- 大きなraw資料を回収した後も、同じF/accountの再受付を防ぐ既存nonce/Use/join/once又は必要な小さなbinding証跡を維持する。retained registry数・DB quotaも有限にし、永久metadata増殖を無料にしない。
- 必要資料が失われた場合は`Unavailable`で停止し、署名・custody・支払済み状態を捏造しない。正規復旧なしにlocked valueを解放しない。実local I/Oはchain undoで未実行には戻らない。

## 11. 代替と選ばない境界

| 代替 | 判断案 |
| --- | --- |
| その都度payerが通常op12へ署名 | 会計安全性の検証には使える。payer独立のliveness完成とは扱わない |
| terminal一回だけ、途中進行を全面廃止 | 元cursorのrelay署名は単純になるが、RFC45のpartial/service/refund要件を満たしたことにならない |
| F payload内に自分のrelay署名を同居 | txid循環のため不採用 |
| r1 projection/txidからさらにfieldを省略 | 既存ID・認証の意味を変えるため本案では行わない |
| script bypass、OP_TRUE、P2SH、署名cacheへの成功注入 | 通常script保持と両立せず不採用 |
| local sidecarや任意traitで資料受理を宣言 | canonical受理・復旧・custodyを与えないため不採用 |
| 新規detached block attachment | 別のwire/retention/ownership契約が必要。本案の範囲外 |

## 12. 採択条件・必須試験・有効化

採択判断はop14、row19/20、R terms/署名/template、先行custody、F/R非循環、R2のconditional partial、有限terminal fallback、追加resource/fee/restore規則の範囲に限定する。採択後も、全owner・consumer実装、数値、実測、独立review、公開網activationは別判断である。

必須試験は次を含む。

1. 固定77-byte payload、0x82、key/script/amount/outpoint/sequence/sigop/version/locktime/subnet/gas/contextの一致と一field変更拒否。追加fee入力・許可出力変更では署名が維持され、nativeは不適格recipient/fee流用を拒否すること。
2. F→署名→Rの構築で循環しないこと。FがR hashを参照しないこと。F payload/outputs変更で旧Rが無効になり、signature script変更ではtxidが変わらなくても通常Full検証を再実行すること。
3. R単体ではA/Case/funding/lock/role/nonce/credit/WorkerPaymentが増えないこと。料金・caps・expiry・actual bond/purpose/metadata custody不足を拒否し、同feeを二重burnしないこと。
4. actual F出力/現在UTXO/元payer consent/r1 scope/refund/capsがRの宣言と異なる場合の拒否。別Assignment/purpose/outpoint、孤立material、別lineage、R期限・custody不足の拒否。
5. A/C/op13全purposeでの先行R照合と一checkpoint rollback。Rを消すreorg、F/Sだけを消すreorg、競合nonce/Use/join、同outpoint複数purpose・別terms再送の拒否。
6. partial Sの前にR2が必要であること。R2なしでは旧UTXO/lock/Rが不変、R2ありでは正しいnew cursorとbindingへ一回だけ進むこと。Sのfee inputs/outputs変更で別txidになれば対応しないR2を拒否すること。
7. P PendingでWorkerPaymentを拒否しつつ、適格service/refundのpartialが正しいfloorとR2で進むこと。R2非協力時はpartialを強制できない限界を記録し、有限なactual終了後のterminal pathでは追加payer署名なしに全due/返金が進むこと。
8. P NoPopulation/Expired/NotCertified、pending-fraud、prover放棄、全live/retired join、未終了retention/dutyを含む実branch。P/fraud finite owner欠落を短縮TTLやsaved flagで通さないこと。
9. DB全接続を閉じたrestart、reorg/rollback、snapshot/pruned IBD、missing/corrupt/truncated/duplicate資料、hard-quota不足、NativeHeap/Wireの全field登録。raw orphanのみから成功しないこと。
10. 全shipped None fingerprint/root/wire、旧r1/TFO1/0–13/0–18互換性、元Assignment intake profileの固定（後日activationをまたぐ旧A→C/joinにRを遡及必須化しないこと）、historical raw pruning後のlive署名package保持、WorkerPayment→Reward誤用拒否、元account予算とonceの不変。Miner/Panel/Node/custody/relay別のCPU/RAM/通信/保持/復旧とnative全block/global資源を実測すること。

本draftの存在、prepared signer、pure money試験、typed missingの拒否だけを、native liveness又はRFC45完成の証拠にしてはならない。
