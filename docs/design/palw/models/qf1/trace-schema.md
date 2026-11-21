# QF1 logical trace schema v1

正本は[RFC0066](../../../../rfc/0066-qwen3-fow1-execution-and-readout.md)。**ADOPTED_DESIGN_NOT_ACTIVATED**。以下はlogical recordのbyte schemaであり、既存native tagの割当・実装済み宣言ではない。

## 共通型

全整数はlittle-endian、固定幅、無符号は`uN`、二の補数符号付きは`iN`。`bytesN`は正確にN byte。float/boolから整数への暗黙変換、余剰bytes、任意fieldを拒否する。全長は既存受付・court上限も満たす。payloadを`kind:u8 || payload_len:u32 || payload`で包む。

全identity/rootの`bytes64`は本instanceで固定された正規digest。context digestは`B2b_64(C_bytes)`、instance digestは登録されたcanonical FunctionWorkDescriptorのQF1 identityであり、同一IDを別bytesへ変更できない。program/node mappingはそのdescriptorに束縛する。これらに未来のsやrootを入れる循環を作らない。

| kind | 名前 | payload（記載順、任意fieldなし） |
| --- | --- | --- |
| 0 | Init | `context_digest:bytes64, instance_digest:bytes64, s:bytes32, R_x:bytes64, L:u32, G:u16` |
| 1 | TirValue | `position:u32, stage:u8, layer:u16, node:u32, tile_start:u32, lane_count:u16, dtype:u8, values:bytes` |
| 2 | DecodeDecision | `j:u32, position:u32, token:u32, max_logit:i32, argmax_node:u32` |
| 3 | Stop | `T:u32, processed_positions:u32, last_token:u32, reason:u8` |
| 4 | ReadoutChunk | `chunk_index:u32, consumed_bytes:u64, chunk_length:u16, final_flag:u8, chain_after:bytes64` |
| 5 | End | `R_x:bytes64, R_y:bytes64, R_z:bytes64, L:u32, T:u32, processed_positions:u32, tir_record_count:u64, decision_count:u32, hash_chunk_count:u32, trace_leaf_count:u64, z:bytes64` |

Endのtrace_leaf_countはgrammarから求まる整数で、R_tau自身を含まない。R_yはEOSを含むT個のLE32 token、R_zはz全64 byteの1 leaf。StopとEndが宣言した値だけを信用せず、位置・生成結果・record列から同じ値を再導出する。

## TirValue

stageはpre=0、layer=1、post=2。layer stageならlayerは0..35、pre/postなら65535。dtype tagとelement bytesはPALW-TIRの型表（i8=0/i16=1/i32=2/idx=5）を使い、i64/i128はcommit不可。valuesはlane_count個を型幅で連結したbytesである。本logical codecは型幅を保持し、既存nativeの4-byte lane格納と同一wireだと推定しない。native bridgeには明示的なrange・型変換検査を要求する。

tile_startは当該nodeのrow-major線形index。tile幅64、最後だけ1..64 lanes。node、shape、論理length、期待dtype、scale、commit可否はprogramから導出する。未指定nodeを足す、同じtileを重複する、tileを欠落することを拒否する。位置を昇順に、pre→layer0..35→post、node昇順、tile_start昇順に並べる。

位置0..L−2はPrefillBodyとしてpreとlayersのcommitだけ。位置L−1..L+T−2はDecodeDecision stageとしてpre/layers/postと選択関係を実行する。final normはpostの独立commit、logitsは全151936 lanesのcommit、argmaxはTopK(k=1)の固定選択関係をcommitする。TopKで同点は最小token IDを選ぶ。Nodeは任意のtokenをargmaxと認めず、少なくとも一つの勝るlogit又はtie違反で局所反証できる関係を維持する。

StateMapは各layerの第二残差後Code[4096]とpostのfinal norm Code[4096]のproducer nodeを指定する。登録時に実graphとの一致を確認する。shapeの一致だけで未使用のdummy nodeを登録できない。TIR checkerは規範の丸め・scaleとregistered model rootの期待parameter indexを検査する。

## Stop / End

reasonはEOS=0、LIMIT=1だけ。`1<=T<=G`、`processed_positions=L+T−1`。末尾が151643又は151645ならEOS、そうでなければT=GでLIMIT。T−1以前にEOSがあった場合は不正。EOSとG到達が同時ならEOS。timeout/cancel/errorをこの成功Stopに符号化しない。

T個のDecisionは`j=0..T−1`、`position=L−1+j`。j=0はprompt末尾の処理から選ぶ。j>0の入力tokenは一つ前のDecision token。Stop後、選択された最後のtokenを処理するBodyStepは存在しない。

## ReadoutChunk

RFC0066のmessageを4096-byte単位に切る。総長をM、chunk数を`H=ceil(M/4096)`とする。k番目はoffset=4096k、length=min(4096,M−offset)、consumed_bytes=offset+length、final_flag=1 iff k=H−1。値1以外の真値表現を許さない。

chain_afterは8個の64-bit BLAKE2b内部wordのlittle-endian連結。最初の入力chainはRFC7693のIVにdigest length64/key length0/fanout1/depth1のparameterを適用したもの。次は直前chunkのchain_after。counter高64 bitは0。非最終chunkの32 compressionは全てfinal=0、最終chunkの最後のcompressionだけfinal=1。最終圧縮blockの余りを0で埋め、counterには実際のmessage長だけを入れる。

raw message bytesはこのrecordへ重複格納しない。Courtはoffsetから固定header/End又は元TirValueのi16値を導出・openingして再構成する。最大2050数値要素・34個の64-lane source tile・最大32 compressionに、前後chunk leafとMerkle pathを加えた実carriageが上限内であることを事前に検査する。padding、source順、chain初期値、counter、最終flagのどれもPanelが自由選択しない。

最終chain_afterはz全64 byteと等しい。zを任意にrootへ入れるだけの受付を認めない。source値自体の算術矛盾は、そのproducerのTIR Courtで裁定する。hashの局所一致を全推論の肯定証明にしない。

## record順序と個数

Init1件、位置順のTirValue（予測位置のpost直後にそのDecodeDecision）、Stop1件、ReadoutChunkをindex順にH件、End1件。

各位置pの規定TirValue数をB(p)とすると、

`trace_leaf_count = sum(B(p), p=0..L+T−2) + T + H + 3`。

B(p)はprogram/commit layout/stageから導出する。資源上限とu64範囲を先に検査する。hash chunk数はReadout message lengthから導出する。自由record、未使用padding、元実行と無関係な署名/commentをtraceへ追加してTicketを変えない。

## 一claimの認証で省略しないこと

model/program/StateMap/数値parameterは登録instanceへ、全TirValue/Decision/Stop/HashChunk/Endは元Minerが固定したR_tauへ、X/Y/Zは同じsの四Rootへ結び付く。別instance、別layer、別位置、別sの本物のleafを提出しても期待index/rootに一致せず拒否する。資料がなくこの関係を開けなければ、未検証の成功へ進めずMAS1/資料caseを適用する。
