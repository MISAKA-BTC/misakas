# 全claim受付難易度 — 添付mainの入口監査

- 日付: 2026-11-17
- 対象source: `MISAKA-BTC/misakas main@2d8212b634880b0967fbf24b00b41936c2322c14`
- 添付ZIP: `misakas-main(1).zip`。ZIP commentを読み、上記SHAとの一致を確認した。
- 読取先: `/workspace/scratch/484d3328ce3c/misaka-main-source`
- 対象: Attempt、FreePrompt、TFP1/TFO1/TFO2、atomic original、FP receipt、court block budgetとactivation hook。
- 方法: `rg`による入口・呼出箇所の検索と、実関数の順序・状態変更の読解。コード変更・nativeテスト実行・実機測定は行っていない。

ZIP/GitHub間のblob照合は、主作業の[source-provenance.json](source-provenance.json)を参照する。GitHub treeは`d99e8148554f04b7f53a19d4717a1f9b4796d3f3`、対象3,582 blobは`docs/`、`consensus/core/src/`、`consensus/src/`、root `AGENTS.md`で、不一致0件と記録されている。さらに`rpc/service/src/service.rs`を別の1 blobとして照合済みで、Git blob SHAは`cff6e380055ae300f301e7217ccee4dc861b1b97`である。RPCの引用を元の3,582件へ重複計上しない。レビュー担当はこの保存済み照合記録とZIP commentを確認した。

## 1. 結論

既存のAttempt抽選、FreePromptの署名・重複・資金・Panel容量検査、funded originalの原子的な資金・役割・court予約、courtのblock budgetは実装されている。「何も制限がない」という説明は正しくない。

一方、調べた各入口には、**全種類の新originalを、高価な検証責任の開始前に一つのcourt負荷連動Dへ通す共通経路はない**。FP receiptの抽選はclaimがFinalになった後の判定であり、新規検証責任の流入制御の代わりにはならない。

本文中の「コードがある」「明示profileでarmするhookがある」「測定済み」「公開ネットワークで稼働している」を区別する。この監査は前二者をソースから確認するものであり、稼働中ノードの設定・binary・canonical履歴を観測していない。

## 2. 経路の比較

| 経路 | 確認した順序 | 全claim共通Dとの関係 |
| --- | --- | --- |
| Attempt | headerのbyte/形式上限、PAV3ならnative material検査、stateless/署名、pins、stateful資格・資金・重複、既存class/W ticket | 既存ticketは新claimを作る前にある。ただし全Nodeコストより前のfilterではなく、全route共通のcourt負荷連動Dでもない。 |
| FreePrompt新規 | payload decode、class/形式/署名、仕事量導出、claim・work重複、bond・資金・既存容量、original準備、Panel選出・reserve・claim記録 | FP receipt ticketはここにない。 |
| TFP1/TFO1/TFO2 original | literal FPに対する上記条件、source/actor構成、全branchとservice book候補、実資金同意、同checkpointでinstall | 既存の原子的予約を保持し、その前に新Dを接続する必要がある。 |
| FP receipt | 既存claimのFinal、quantum未消費、beacon/maturity/window、receipt ticket、producer/bond/class | 検証責任を受け入れた後の権利使用判定。 |

## 3. Attempt — 抽選はあるが、入口全費用を除去しない

header shape検査はform別のbyte capを先に調べ、その後にenvelopeをdecodeする。[pow_layer0.rs:789–827][attempt-size]

実processorはPAV3なら`verify_prior_with_recovery_typed_v4`でnative materialを検査し、ticketを用意してからfull admissionへ進む。[palw.rs:10924–10946][attempt-material]

`check_palw_attempt_admission_full_v2_with_bootstrap`はstateless、signature、DA/retention/witness pins、stateful admission、class/W ticketの順である。stateful admissionの末尾には`attempt_id`重複拒否がある。[palw_admission_v2.rs:1160–1185][attempt-full] [同:931–938][attempt-dedup]

Wの更新はmodel block数とexpected block数に基づき、class ticket targetは`MAX × min(1, CCU/W)`である。これは既存の経済的な仕事・生成条件であり、将来court責任のCPU/RAM/I/O vectorを観測するCAD1とは別の役割を持つ。[palw_work_target_v1.rs:94–124][work-target] [palw_admission_v2.rs:1192–1253][work-reader]

従って「Attemptに難易度がない」は誤りである。また「既存Wを通せば、decode・署名・material検査もすべて未実行で済む」とも言えない。CAD1は必要な既存資格を維持しながら、重い処理の前に新しいcompact入口を設ける変更になる。

## 4. FreePrompt — 新規受付とFinal後抽選は別

通常extractorはaccepted transactionのpayloadをBorsh decodeし、class別投影関数へ渡す。投影はclass規格を確認し、commitment署名を検査してから仕事量導出へ進む。[palw_fp_objects_v3.rs:305–348][fp-decode] [同:420–485][fp-auth]

native foldの`FreePromptCommitted`では、次を確認できる。

| 位置 | 処理 |
| --- | --- |
| [objects.rs:3974–3997][fp-bond] | claim重複、executor鍵とbondの一致、producer floor、frozen bond |
| [同:4111–4118][fp-capacity] | 検証shape/期限、既存Panel・inflight容量 |
| [同:4219–4250][fp-price] | 価格、quanta、導出仕事量、priced quantaに対する既存容量 |
| [同:4292–4297][fp-dedup] | `fp_work_id_v1(class_id, prompt_hash, bond)`による重複拒否 |
| [同:4406–4419][fp-original] | `prepare_paired_original_v1`、`admit_original_v1`、staged contract準備 |
| [同:4438–4449][fp-install] | budget計画、Panel authorization選出、`reserve_for_claim`、`write_claim` |
| [同:4519–4532][fp-panel] | deadline、Panel consent capture、original frame install |

FP receipt側は、まず既存claimが`Final`であることを要求し、その後に未消費quantum、beacon、使用期間、receipt ticketを検査する。[palw_fp_admission_v3.rs:186–237][receipt]

従って、receipt難易度を重い新規検証より前の全claim流入制御として説明できない。新Dは元claimの受理経路へ追加する必要がある。

## 5. TFO3、TFO2 wire v3、atomic original

`consensus`、`rpc`、`core`、現行`docs/rfc-v2`と`docs/STATUS.md`を検索した範囲で、`TFO3`、`TFO_3`、`TFO-3`は0件だった。同じ範囲で`UnifiedAdmissionV1`、`MISAKA/UAI1`、`PresealId`、`GlobalServiceBookV1`も0件だった。名称の不在だけで機能の不在を判定せず、上記の実際の入口・状態変更も併せて確認した。

mainにある`SIGNATURE_ALIAS_WIRE_VERSION_V3`は、magicが`TFO2`のままの内部wire revisionである。別carrierのTFO3と呼ばない。以前のpreの進捗をこのmainの実装として転記しない。[palw_tir_funded_original_envelope_v2.rs:1–11][tfo2-v3] [同:70–89][tfo2-codec]

paired originalはcheckpoint内で処理され、失敗時にはrestoreする。`OriginalAdmissionV1`にはPlainとFundedがあり、prepared/fundedがなければ`Ok(None)`である。[objects.rs:42–68][atomic-checkpoint] [palw_funded_original_coordinator_fold_v1.rs:257–292][original-enum]

Funded経路は現在actor authorityとrole consentを確認し、全合法branch・family責任を導出してbook候補を作る。actual carrier funding outputとpayer署名scope、contract/outpoint重複も検査する。install時には現在のbookで再検査する。Batch fundingもservice row不変と元hard clockを確認してからinstallする。[同:627–724][original-plan] [同:809–862][original-funds] [同:918–947][original-install]

これは原子的受付・court予約の実装証拠であり、全route共通Dの接続証拠ではない。CAD1合格を理由にこれらの資金・全責任・期限の再検査を省略してはいけない。

## 6. 共通入口で取り逃しやすい経路

以下は現行コードから導いた、新設計への接続上の注意である。

- Attemptは別のheader入口であり、Funded分岐へgateを足すだけでは対象にならない。
- non-TIR/plain FPは`admit_original_v1`から`Ok(None)`へ進み得る。TFP1 PlainもFundedと別である。
- evaluation claimは通常original準備より前に専用handlerへreturnする。[objects.rs:4003–4021][eval-return]
- quoteにも入口予算が必要である。RPCのoriginal quoteはbyte cap後にdecode・native quoteを呼び、processorのrent preflightもnative preclaim quoteを呼ぶ。[service.rs:4245–4273][quote-rpc] [palw_accept.rs:3068–3082][quote-native]
- batchでoriginalを複数作る場合、外側wrapper一つの合格で内側全件を無制限に通さない。

推奨される接続順は、各route adapterの上位で、有界なサイズ・形式・認証・identity/一回性を確認し、CAD1を判定した後に、高価なgeometry/actor graph/planner、既存の実資金・全責任予約、原子的installへ進むことである。これはRFC-0056が要求する新規作業であり、現mainの完成経路ではない。

## 7. 既存のサイズ・認証保護を過小評価しない

TFO1はversion、bounded inner bytes、outer byte capを検査する。TFO2はinner、supplier数、semantic tableを有界に読む。funded候補の検査は実資金出力、payer署名、元FP署名、現在producer authorityを照合する。[palw_tir_funded_original_envelope_v1.rs:37–100][tfo1-codec] [TFO2 codec][tfo2-codec] [palw_tir_public_fold_v1.rs:128–155][original-auth]

original quoteもhex長を確認してからallocation/decodeする。[service.rs:4245–4260][quote-rpc]

これらを全route・全失敗経路のOOM保護が証明済みという意味に広げない。例えば通常FP extractor自身はBorsh decodeを呼ぶため、上流のaccepted-tx条件、native resource所有権、process全体のqueue/RAM/CPU上限との接続も必要である。今回、その全経路の実動作は試験していない。

## 8. court budget、profile、measurement、runtimeの区別

| 区分 | ソースから確認できること | この監査からは言えないこと |
| --- | --- | --- |
| court block budget | processorはnormal/one-move budgetを事前検査し、超過objectをdropする。foldもcourt opをchargeする。[palw_accept.rs:2219–2257][court-budget] [objects.rs:47–49][atomic-checkpoint] | これだけで全originalの将来責任や全Nodeの資源が保証されるとは言えない。 |
| base preset | `params.rs`の複数presetは`palw_court_capacity_v1: None`。[例:7579][preset-none] | 実行時に明示profileでarmされていないとは断定できない。 |
| 明示activation hook | 推定policyを設定・mirrorへ同期するhookが存在する。[full_activation_v1.rs:476–487][capacity-hook] | hookの存在は現在の公開稼働の証拠ではない。 |
| measurement | 上記hookは`measurement: None`。コメントとvalidation契約は`UserDecision`免除による推定値採用を区別する。[同][capacity-hook] | 推定値採用を実測済みと報告しない。 |
| paid ingress schedule | production関数は承認済み推定scheduleを返す。[palw_court_paid_ingress_v1.rs:48–55][ingress-schedule] | 「すべて未採択なので常に値がない」という古い説明を流用しない。 |
| 現在の稼働 | 今回はlive binary、設定、activation anchor、canonical履歴を観測していない。 | 公開有効化済み、特定profileで常時稼働中という結論は出さない。 |

RFC-0056の新Dには別途native実装・全入口接続・測定・移行・activationが必要である。旧court budgetの存在や私的profileでのarm可能性を、新Dの完成として扱わない。

[attempt-size]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/pow_layer0.rs#L789-L827
[attempt-material]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/virtual_processor/processor/palw.rs#L10924-L10946
[attempt-full]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_admission_v2.rs#L1160-L1185
[attempt-dedup]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_admission_v2.rs#L931-L938
[work-target]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_work_target_v1.rs#L94-L124
[work-reader]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_admission_v2.rs#L1192-L1253
[fp-decode]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_fp_objects_v3.rs#L305-L348
[fp-auth]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_fp_objects_v3.rs#L420-L485
[fp-bond]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L3974-L3997
[fp-capacity]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4111-L4118
[fp-price]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4219-L4250
[fp-dedup]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4292-L4297
[fp-original]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4406-L4419
[fp-install]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4438-L4449
[fp-panel]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4519-L4532
[receipt]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_fp_admission_v3.rs#L186-L237
[tfo2-v3]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_funded_original_envelope_v2.rs#L1-L11
[tfo2-codec]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_funded_original_envelope_v2.rs#L70-L89
[atomic-checkpoint]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L42-L68
[original-enum]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_funded_original_coordinator_fold_v1.rs#L257-L292
[original-plan]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_funded_original_coordinator_fold_v1.rs#L627-L724
[original-funds]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_funded_original_coordinator_fold_v1.rs#L809-L862
[original-install]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_funded_original_coordinator_fold_v1.rs#L918-L947
[eval-return]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_state_v2/objects.rs#L4003-L4021
[quote-rpc]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/rpc/service/src/service.rs#L4245-L4273
[quote-native]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/virtual_processor/processor/palw_accept.rs#L3068-L3082
[tfo1-codec]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_funded_original_envelope_v1.rs#L37-L100
[original-auth]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_tir_public_fold_v1.rs#L128-L155
[court-budget]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/src/pipeline/virtual_processor/processor/palw_accept.rs#L2219-L2257
[preset-none]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/config/params.rs#L7579
[capacity-hook]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/config/params/full_activation_v1.rs#L476-L487
[ingress-schedule]: https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_court_paid_ingress_v1.rs#L48-L55
