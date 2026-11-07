# RFC-0032 Aによるsemantic canonical internal replayの可能性

read-only調査。旧任意Part受付/既存価格/時計/署名義務を変更せず、新service namespaceのprivate builder候補として検討。実装/採択/完全profile/実資金成立の主張はない。

## 結論

認証済み完成materialが揃っていれば、旧streamの分割履歴をコピーせず、既存**裁定primitive**へ決定論的なcanonical packet列を内部投入して終端結果を構成する方式は可能性がある。これならPart長ごとの全組合せを公開予約graphへ展開する必要を減らせる。ただし『同じmaterialなら旧owned court全体が同じ』ではない。verdict primitive以外のmove課金・時刻・署名・retentionは分割に依存し得るため、旧sessionの履行/責任をこの内部replayで代替してはならない。

## materialとreplay候補

### Constraint EOG / rendering

- 元original job/登録semantics/token-table rootをnative resolverで固定。認証済みEOG列、各native `next_rendering_id()`のpresent/length/openingとraw rendering bytesを共通material storeで要求。
- `PalwConstraintStreamCourtV1::{push_eog,finish_eog,next_rendering_id,begin_rendering,push_rendering,finish_rendering,finish_eog_token}`等の実primitiveへ投入。
- EOGは既存PART_IDS上限内の最大連続slice、renderingは既存PART_BYTES上限内の最大連続sliceを選ぶ。最終短partは実EOF。元openingが不在/長さ違い/bytes不一致ならNeed/Errで停止し、完成とは呼ばない。
- canonical packet列は内部計算手順であり、旧actorの署名付き `Move` を偽造しない。filerのterminal intentはnative network/original/source/semantic family/target/native policy/材料digestに別domainで束縛する。

### Tokenizer

- 元登録のsigned raw byte length/key-event bound/JSON-rootと、取得したoperand raw tokenizer bytes、登録tableのcompact key proofs、rendering opening/bytesが必要。既存certificateのactual_bytesだけではraw materialの取得を証明しない。
- `PalwTokenizerTableStreamV1::packet`はbytes<=65536、proofs<=64、Borsh packet<=90000、offset一致、かつparser pending_key順の実消費を検査する。proof-only packetも合法。
- canonical builderはactual parserをclone/checked stepして、pending keyが出るまでraw bytesを最大合法範囲で進め、必要な**認証済みのそのkey proofだけ**を順に添付する。次proofでencoded carrierが超過するならそこでpacketを終了し次packetへ。bytes/proofs双方0のpacketは生成しない。全bytes/全proofが消費されたことを実primitiveで再確認しsealへ進む。
- packet envelope上限は新wrapper overheadも含めてactual Borshで確認。旧90000-body allowanceをそのまま新署名込みcarrier容量だとしない。scratch/parser clone/hash/compact-proof圧縮・展開を別にmeterする。
- builderが必要proofを導出できない場合、鍵を捏造/未検証RawJSONを信用せずNeedを返す。parserの現在状態adapterだけでは全資料取得・全合法分岐閉包にはならない。

### TIR PrefixState

- actual original header/登録layoutからprefix boundary leaf列とnative orderを導出。共通sourceの認証済みstep leaf/preimage/openingと必要parameter/prompt materialで既存prefix裁定primitiveを走らせる。
- legacy GDN/head/page等へTIR leafを変換したふりはしない。native TIR Prefix用validator/境界順序とslotsを保持。
- 完成認証materialから内部の最長合法partを作ることと、旧custodianのleaf response署名/期限を満たすことは別。旧`NativeTirPrefixCollect`のabsence結果を内部replayから作らない。

## 分割不変性の範囲

同じ元context/完成認証materialについて、成功してEOFまで到達したprimitiveのhash/判定がchunk境界で変わらないことは、現在のincremental parser/hashを使う同一性試験で確認できる。必須比較はcanonical maxchunk対1byte/短part/異なるproof-only境界、各familyの正常/違反/欠落material。

しかし旧owned stream全体は分割不変とは限らない:

- `PalwOwnedConstraintStreamV1`はmoves_used/maximum_movesを持ち、細分割で費用・失敗位置が変わる。
- 各moveには元scope/phase/actorの署名があり、canonical builderはその署名を作れない。
- retained transcript bytes/署名数、進捗時刻、応答/collect義務は異なる。
- 元groupの未完成prefixや宣言署名はpending liabilityとして保存すべきで、canonical完成証拠で消してはいけない。

従って新namespaceで署名付きintentから**同じ終端証拠/結果だけ**を構成し、old streamの責任・保持と別に扱う。internal replayをold paid movesのreceipt/署名履行へ転用しない。

## Selected MAX_REQUESTSと共謀者の浪費

現 `PalwSelectedEventSessionV1::demand_view_v1` はownerが任意legal座標を追加し、requests.len>=MAX_REQUESTSで拒否する。native resource quoteもその回数を価格化する。外部者に必要な座標より先に無関係座標を埋められるため、旧counterを共通訴追の唯一の入口にしてはならない。

候補は新共通namespaceで、canonical terminal builderがnative predicateから要求するNeedを、original/source/family/coordinateで重複排除した共通frontierへ追加する方式。対象座標は元D×ceil(V/L)の有限source宇宙に限定し、owner/filer別counterを増やさない。取得済みFullIds/TraceTileのopening/lanesで既存eventを構成し、`PalwSelectedTiledEventV1::authenticate_view_v1`で再認証できれば（全行を取得済みの場合のみ`of_public_rows_view_v1`も使える）、旧sessionに新しいdemandを発生させずprimitiveへ渡せる。

ただしこれは旧MAX_REQUESTSを引き上げることではない。新経路の全座標/全Need/実material stream/role responseをoriginal受付時に予約・原資確保できなければ拒否する。旧counterを無視して旧sessionへ追加受理する実装、予算を使い切ったら無料fallbackする実装は不可。無関係intentが共有material/終端slotを占有しないことと、全owner共謀下の外部1人が進めることは別途adversarial試験が必要。

## 実取得前提（省略不可）

- original/source/登録program/layout、元semantics/tokenizer root、full generated IDs、必要TraceTile/step leaf、parameter openings、prompt、EOG/rendering raw bytes、tokenizer raw bytes/key proofs、prefix boundary materialが、同じauthenticated scopeで取得可能。
- model parameterは外部検証者が自分で取得する既存Aルート。被告へmodel bytes補完を要求しない。全tokenizer/rendering rawもhashだけでは取得可能にならない。
- signed custody/history grantの主体・期限・原material保持horizonを原本から確認。チェーンに保存済みでない資料の永続性をblob digestだけで保証しない。
- authentication/hash/parser/index/copy/output所有/retention/原資/実feeをnative original planに含める。private builder meterだけで配送・保持・資金成立を代替しない。

## 通常実装として進められる部分と未決部分

既存RFC32 Aの範囲で通常実装可能: read-only private canonical replay builder、認証済みmaterial-only store、決定論的packet生成、既存primitiveとの判定同一性試験、Need/未対応/予算不足の明示、署名付きintentのscope検査案、元wireを触らない隔離試験。現行RFC32の具体実装はTIR numerical terminalなので、semantic拡張のscope/grammarは候補として記録し、全family成功を先に宣言しない。

未決/別途レビューが必要: 新public有料ingressとnative row/decoder/tag、全Needのoriginal予約SUM/価格/capital、資料提供者の新保持・供給義務、F02配送/収録前提とdefault、旧MAX_REQUESTS/旧時計/旧署名義務の置換、消費したfeeの償還・escrow settlement/pruning。これらをprivate builder追加で採択/成立したことにしない。
