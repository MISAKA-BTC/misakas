# A4 — object 176 bounded body schema proposal v2/r1

2026-10-31。A0 [native契約](NATIVE-CONTRACT-R1.md) の176予約に渡すbody codec案。正本のRust定義は `consensus/core/src/palw_positive_court_v2/coordinator/native_body.rs`。ここではglobal tag、native decoder登録、fence、aux row、料金、公開profile、有効化を設定しない。A0の採択・live接続は別作業。

## Byte layout

全整数little-endian。外側は `version:u16=2 || revision:u16=1 || operation:u8 || fields`。以下のfieldは表記順のBorsh。ID/Digestは64 bytes、署名は固定4627 bytes、Vecは `u32 byte_length || literal bytes`。鍵はrooted registrationから解決し、bodyにpubkeyを毎回重複して搬送しない。

| local operation | kind | fields in order |
| --- | --- | --- |
| ResultAdmission | 0 | A0 MinerResultCommit、Miner署名、MinerServiceContract、FundingPlanBasis、FundingConsent、payer署名 |
| WitnessCommit | 1 | A0 PanelWitnessCommit、Panel署名、opaque record Vec |
| ExactFrame | 2 | A0 EvidenceFrame.encode bytes Vec、Panel frame署名 |
| SourceRequest | 3 | A0 SourceRequest、requester PrincipalId、要求署名 |
| SourceResponse | 4 | CaseId、RequestId、既存G14 response bytes Vec |
| Finish | 5 | CaseId、ResultId、WitnessId |
| Expire | 6 | CaseId、ResultId |
| PositiveCourtAttestation | 7 | A0 PositiveCourtAttestationV2、検証者署名 |

local kindはglobal lifecycle番号ではない。未知kindを拒否し、将来追加時にも既存kindを再番号化しない。Finish/Expireにpeer時計、Passed、罰則、支払済みflagを含めない。現在のnative viewのDAA・履歴・検査進捗で判断する。

A0 r1固定DTOのwire幅はResult 1076、Service 328、FundingPlanBasis 345、Consent 288、Witness 649、SourceRequest 213、PositiveCourtAttestationV2 837 bytes（別署名4627を除く）。手書きscannerはこの固定field構成をprimitiveのread/skipで走査し、Vecを含むBorshデシリアライズをpreflight中に呼ばない。DTO変更時はA0のrevision/codec/vectorと一緒にscannerを更新する。

## Bounded scan and ownership

`scan_native_body_v2(bytes, limits)`はbody長、version/revision、kind、固定署名数、全opaque長、ExactFrameのmagic/header/正規長/関係数、切断・余剰をallocationなしで検査する。`decode_native_body_v2`はその後だけBorshのVecを確保する。初期EXACTはrelation_count=1のみで、複数relation profileを未対応として拒否する。

policy指定の `NativeBodyLimitsV2` にdefaultはない。private codec防御ceiling案はbody 16 MiB、opaque 8 MiB、relation count 4096、署名数2。これは測定済みprofile・公開network parameterではなく、A0採択待ちの防御上限。実native outer capとNodeが独立導出したruntime policyで必ずさらにclampする。opaqueを含まないFinish/Expireは外側署名0として数える。ここでのsignature count/capはbodyに明示した固定署名だけであり、opaque record/G14 response内の追加署名・workを含まない。live foldは必ずinner A1/G14 preflight分と実native wrapper費用を合算して予約する。

`NativeWireScanV1`はbodyだけに手書き実装し、共通DTOへ重複implを追加しない。型preflightのheap形状はopaque VecのBorsh growth分 `2 × len`、byte entriesはactual len。`NativeHeapV1`は実Vecのcapacityと同じ既存ownership envelopeを使う。body enumの固定inline sizeは既存preflight側が加える規約であり、heap側で二重加算しない。

このheapはbodyの保持だけを表す。encoded bodyを再確保する際のpeakは別に所有body＋wire bufferのgrowthを予約し、A1内側decodeのheap、プログラム/計画、DB journal、保持/復旧費と分けてquoteへ加える。codec ceilingをNode全体RSS承認や資源予約済みと解釈しない。

## Decode success is not native acceptance

ResultAdmissionのsignature/termsはA0 funding scopeを全fieldから再計算し、実carrier output/UTXO/lock/現在viewをA4/A5の原子的acceptorで確認する。payer consentを資金存在・lockの証拠にしない。

Witnessのrecordはbounded opaque bytesであり、この案では新しいbinding/generated layoutを割り当てない。A0/A1で割り当てたinner codec、record identity、署名されたrootとの照合が必要。SourceResponseも既存G14 response codecのbounded preflightと資料opening認証が必要であり、独自Merkle木や独自課金を導入しない。inner adapterが未割当ならnative受付を拒否する。

ExactFrame outer signatureはA0 `EvidenceFrame.authenticate`と署名domainを使い、実Panel author/duty/current scopeをrooted stateから解決してからA1へ渡す。headerのmembershipや署名だけで計算合格にしない。frame内のTIR partはA1の別preflightで内側Vec・heap上限を検査してから全必須関係を検査する。

kind 7はA0 certification契約 `069d5400784993b0bd8748cf60057a7fbf868096` に従う追加であり、既存0〜6を変更しない。attestationはclaim単位の所見を固定837 bytesで保持し、nativeは実draw/現在viewからexpected policy、draw record、witness、checked material、bond、verifierと鍵を独立解決してA0のvalidate/authenticateを行う。peer値をexpected contextへコピーしない。bodyのdecodeや検証者署名だけで抽選資格・draw承認・Node全検査・Gate C成立・信用を認めず、負の所見をMiner有罪へ変換しない。

新規署名domain、公開activation、native成功stubは追加していない。176のbody型がdecodeできても、case、資源lease、実資金、責任、journal/undo、Gate C、全consumerへの接続が完了したことにはならない。
