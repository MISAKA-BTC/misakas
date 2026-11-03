# A0 — TIR v1 interchange annex（共通契約 v2/r1）

2026-10-31。A1の確定済み検査器 `40cbcb559ce30e80c3b842442b0ac25fe0a84b2f` をA0へ取り込み、node moduleを登録した。これはnative受付・課金・信用の接続完了ではない。

共通codeは `misaka/palw/positive-contract/src/tir.rs`。元の [r1 commit schema](COMMON-CONTRACT.md) のfield順序・署名domain・IDは変更しない。ここでは未割当だったTIR payloadのprivate framingと初期profileの識別を一か所に定める。global native tagの割当ではない。

## 初期profile

TIR v1、全nodeを登録前にcommit対象にしたprogram、tiled i32 logits、固定長greedy、同値は最小token index。既存登録programのcommit属性を後から変更して同一classとして扱わない。sparse/wide/checkpoint再利用等の他profileはA1で明示的に対応するまで拒否する。現在の学習済みLLM群がこのprofileで検査可能、という主張はしない。

計画のderivationは`tir::exact_derivation_id()`、selectorは`tir::greedy_selector_id()`を使用する。独立した同名hash・文字列・domainを各担当で定義しない。`coverage_plan_id(result, ExactStreamV1, exact_derivation_id(), canonical_leaf_count)`がplan IDになる。A1は登録programからleaf数を導出し、Panelの数や一覧を正本にしない。

## TIRと共通commitのfield対応

| 共通field | TIRの正本 |
| --- | --- |
| `job` | `PalwJobContextV2.job_id` |
| `execution` | 同contextの`context_hash()`。Miner任意nonceを使わない |
| `registered_class` | nativeに登録されたclass IDと、`class.class_id(registered_artifact_root)`の一致 |
| `artifact` | nativeの登録artifact root |
| `program` | 登録classの`graph_ir_root()` |
| `input` | 正規jobの`prompt_token_ids_hash`。flat/Merkle formもnative規則から解決 |
| `tokenizer` | 正規jobの`tokenizer_id`。canonical attemptのゼロを維持し、登録tokenizerはclass ID/metadataで別に認証する |
| `execution_rules` | 正規jobの`cu_ruleset_id` |
| `generation_seed` | 正規jobの`execution_seed` |
| `temperature_q24` | 初期profileでは0 |
| `prefill` / `decode` | 正規jobの`declared_prefill_tokens` / `exact_decode_tokens` |
| `max_context` | TIR contextの`max_context_tokens`。trained configやlayoutの上限は別途既存TIR規則で照合する |
| `selector` | 上記の共通greedy selector ID |
| Witness `execution_record` | Panel自身の`PalwTirStepBindingV1.committed_execution_root` |
| Witness `candidate_output` | Panelのgenerated IDsから既存`palw_attempt_output_root_v1`で導出 |
| Witness `provenance` | `tir::provenance_id(result)`。登録class/artifact/program/input/tokenizer/CU rules/dependenciesの固定されたtuple |

scope、Miner割当、job/execution/input、登録rootの正本はA4のrooted admission側で保持する。受信packetを読んでから同じpacketから期待値を作るfixtureでは、自己整合した別モデル差替えを検出したことにならない。A7は期待登録情報とjobを別に固定した正常・別root/別inputの試験を用意する。

provenance IDは資料identityの宣言であり、G14 openingの認証、資料の到達、算術検査、fee確保を代替しない。Witnessに記された全IDはNode側で期待値と照合する。

## 一つのrelationのpayload

外側は既存のr1 `EvidenceFrame`（magic `PC2R1EVD`、header込み284 bytes）。初期profileは`relation_count == 1`、`first_relation`がcanonical leaf index。

内側は`tir::ExactPart`：

```text
ASCII PC2R1G14 (8)
cone_bytes_length (u32 LE)
trace_bytes_length (u32 LE; 0xffffffff means None)
existing G14 PalwTirConeRefutationV1 Borsh bytes
existing G14 PalwTirTraceEventDisclosureV1 Borsh bytes, iff Some
```

空cone、Some(empty trace)、切断、余剰bytes、長さ不一致、範囲外は拒否する。内側のheaderは16 bytes。payloadはu32長の外枠に収まる必要がある。

`ExactPart`と`EvidenceFrame`のfieldは非公開。decodeは借用sliceを返すだけで、内側のvectorを確保しない。A1はその後で既存G14のbounded scan・heap予約・opening認証・算術検査を行う。cone/traceの両方を合わせてheap/cpu/byteの上限を適用する。

## 資源quote

外枠と内枠の追加は合計300 bytes。`tir::raw_part_budget(full_frame_cap)`で既存checkerへ渡すcone+trace上限を求める。外側u32 payload長の上限にもclampし、overflowしない。

A4は`canonical_leaf_count × full_frame_cap`を全候補・全試行で予約する。A1の内側bytesだけをcase/block/epochの総量として使わない。commit、署名、資料搬送、保管、interpreter/native workspace、再送、journal再encode等の別費用もA4/A5/A6で加算する。300 bytesを引くだけでNode総RSSの証明が完成したことにしない。

## 接続時の確認

- A1: `NodeChecker` adapterでこのderivation、selector、provenance、payloadを使い、同じchecked terminalへ進める。
- A2: 入力・登録metadata・署名条件を上表へ結び付ける。任意jobやsourceを認証済みdutyへコピーしない。
- A3: A0のResult/Witness型と署名contextを直接使用する。旧prototype型をaliasしない。初期joint fixtureは新しく登録した全node-commit i32 program。
- A4/A5/A6: full-frame capを一体予約し、現在branch/state/clockを再確認して適用。旧prototypeの整数holdや4KiB資料木をnative契約へ転用しない。
- A7: 同じcandidateで、全員共謀・偽Panel・最終工程省略・自己整合した別model/inputを試す。subprocess timeout時はprocess groupを停止し、子の終了後にlog/hashを確定する。

独立Pythonから作った追加の [vector](tir-vectors-r1.json) をRustと照合する。元のr1 vector・過去の実行ログは書き換えない。

## Panel作者によるpacket認証

Native受付では、署名していないrelayの不正なproofで正直なWitnessを拒否しない。`EvidenceFrame::id()`はcanonical frame全体をstream hashし、Panel作者は`EVIDENCE_SIGN_CONTEXT`（`misaka/positive/evidence-part/v2/r1`）でその64-byte IDにML-DSA-87署名する。A4はrooted Witnessのauthor keyを解決し、`EvidenceFrame::authenticate`を通したpacketだけをNodeCheckerへ渡す。戻り値`AuthenticatedEvidenceFrame`は作者認証のみで、native受付・drawn membership・計算合格・支払の権限ではない。

署名済みpacketは別relayが同一bytesで搬送でき、relay自身の署名へ置換しない。case/result/witness/plan/位置/payloadの変更は署名を無効にする。未認証packetはdeliveryだけを拒否し、候補のattempt・Miner責任・料金を変更しない。NodeのLocalResourceLimit、古いview、Incompleteも算術不正にしない。

この作者署名は4627 bytes追加される。`signed_wire_bytes()`はunsigned EvidenceFrame＋署名の長さで、native eventのversion/tag/length、carrier transaction等は別途加算する。前述の300 bytesはunsigned EvidenceFrameとinner codecのoverheadであり、署名を含まない。A4のquoteは署名検査CPU・全署名bytesも全parts/全試行で予約する。未計測の実LLM費用が許容されるという主張はしない。
