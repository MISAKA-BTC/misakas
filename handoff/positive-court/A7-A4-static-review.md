# A7 — A4候補の独立静的レビュー（2026-10-31）

固定source `d746fe4550a5dd1c2fabc1925c08a82d9879aea6`、証拠commit `85c328af7` を3担当と親でread-onlyレビューした。Cargo実行・production編集なし。A7の243件成功の対象 `bd9623914` とは別の候補であり、成功件数は合算しない。

## [P1] rooted Panel dutyを候補の状態変更より前に確認する

`consensus/core/src/palw_positive_court_v2/coordinator/contract.rs:278` の `open_candidate` はwitness署名を検査するが、既存rooted prover opportunity/roleに基づくPanel認可を検査しない。292–299行でattempt、last_attempt、candidateを更新し、300–301行でcontextのidentityを確認する。rooted duty確認は `carrier.rs:77` のframe受理時にだけある。`CarrierVerifierV2` にopen時の認可APIはない。A1のcontext認証も署名・binding・plan等を確認する処理で、role認可を与えない。ここで必要な認可は、Positive後のGate C用P drawとは別である。

具体的な確認手順（未実行）: integration fixtureの `FrameDuty` を既存seed 92のPanelに固定したまま、別seed 93の鍵で正しいresult/scope/planに対するwitnessと対応する有効CheckContextを作る。現コードのopenはこの別署名者の候補を開ける。続くframeではauthorityが拒否する一方、正規Panelの別候補は既にCheckingの候補によってOrderとなる。別witnessの有効contextを渡す変形ではIdentityエラーでもattemptが消費され、正しいordinalで反復するとmax_attemptsに達する。

rooted stateから独立に解決したprover opportunity/roleを、attempt消費・候補占有より前に要求する必要がある。自己署名できることと、今回のrole認可は別の条件。既存の「認可されたPanelの不正候補にもattemptを課す」方針は、この欠落を補わない。機会・roleを与えられていない鍵でopenを試み、全counterと既存候補が不変で、その後に正規Panelが進めるnegative試験が必要。

## [P2] relayの再送だけでclaim全体のwire予算を消費させない

同 `contract.rs:374–381` は、同一frameのretry判定より前に `wire_bytes` を加算する。`carrier.rs:3–5,69–81` は同一Panel署名packetのrelayを許し、delivery送信者自身の新しい認可は要求しない。

第三者は取得した正規packetと署名をコピーし、同じ時刻で繰り返し渡せる。各retryはcoverageを進めずにclaim予算を消費する。残額が必要な次frameの署名込みbytes未満になると、その正規frameもBudgetとなり、期限まで完了できない。現在の `positive_court_a4_integration.rs:519–530` はretryの累積課金を肯定するが、第三者が予算を消費する条件を検査していない。

同一accepted packetのACKをclaim進捗・予算について冪等にするか、claim予算の消費を認可するdeliveryの根拠が必要。通信・署名検査自体の資源費用は別に有限に制御する。コピーした署名しか持たないrelayが多数再送しても、正規の後続frameが完了できるnegative試験を追加する。

両件はprivate APIをnative ingressへ接続する前の修正事項である。現在の公開networkにlive handlerがあるという主張ではない。A4が通知したbody→quote完全一致・record mutation前validate helperとは別の境界であり、A4の後続 `a874827f3` でもこの2ファイルは固定sourceと同一だった。

## その他の検査と証拠の範囲

`admission.rs` は署名・独立resolve・prepareの後、commit前にroot世代/clock/解決結果を再照合し、単一store commitへ渡す。`funding.rs` はMiner/Payer署名、rooted quote、checked fee+capital、実output index/amount、root安定とadmission結合を検査する。この範囲で追加の具体的bypass・部分mutationの不具合は確認していない。実UTXO/lock/native resolverはstore/view契約の未接続範囲。

`native_body.rs` の固定DTO幅、version/kind、opaque長、署名数、ExactFrame長・関係数、EOF、encode前の長さ検査も照合した。追加の具体的なcodec不具合は確認していない。内側record/G14の検査、総quote、受付権限をbody decodeだけで満たすことはできない。

A4証拠の17個のsummaryは198成功・失敗0と一致し、選択46 source SHAは固定commitと一致する。13の同じleaf試験名はcore/Nodeの2driverで再実行されており、198の独立scenarioとは扱わない。文書もこの重複を明記する。実A1 checker、A0署名carrier、A5 private store、body/funding前検査と旧private journalの検証で、live176・実UTXO lock・単一native DB writer・native reopen/IBDを成功fixtureで代替した過大claimは確認していない。

証拠manifestは選択46ファイルのhashであり、全source集合・実行binary・生ログ・外部native archiveのSHAは記録していない。成功ログにはcache使用もあるため、このログだけから全crateを新規buildしたとは判定しない。A0統合後は全source/lock/binary/logを固定し、修正negativeを含めた同一候補で再実行する。

修正責任はA4/A0へ引継ぐ。A0は両件の修正までnative body/core Courtの取り込みを保留した。既知の同一FrameId再送をCase消費へ重複計上しないことと、新たなinvalid/retry-auth packetのhistory/cost保持を区別する。

A0後続 `e280be137` では有効化境界8件が専用integration target `positive_court_fence_v2` へ移り、旧lib filterは別の1件のみとなる。A0最新候補へA7 runnerを接続する際は専用targetを明示し、要求した試験集合の不足を検知する。過去の `bd9623914` の8件成功は、その固定候補について有効。

公開activation、モデル二階層、claim単位のGate C、A7の既存成功・失敗ログは変更していない。
