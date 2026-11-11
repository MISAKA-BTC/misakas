# Selection Batch material and terminal bounds — 2026-11-11

`89e6bdb05` と `9a26f2765` は、plain originalのSelectionを共通予約へ接続するための入力導出を追加した。[source・binary・ログのdigestと検証結果](summary.json)。公開quote、受付、Final、activation、main pushの成功ではない。

## Material envelope

FullIdsと全row tileを資料ID順に並べ、全D行について各range位置の最大bytesを取る。FullIdsが常に先頭とは仮定しない。D×Vの比較対象配列は構築せず、一行分のscratchを再利用する。各供給役割には同じ分割境界の独立した需要が必要であり、同じ資料IDを一つの供給義務に縮約しない。分割は最大64 ranges・256 Partsを守り、保持量は実際の一行分ではなく、各位置の最大値の和に全役割数を掛けてchecked演算する。

元source・行・役割に対応する提出rangeについて、ID、順序、件数、非ゼロのnative長上限、WholeRange認証を照合する。欠損、並べ替え、資料の置換、長さ超過、別の認証方式、範囲外のdemandを拒否する。これはrange一覧の照合であり、署名、notice、supplier権限、予約済みslot、bodyの認証を代替しない。

通常起動でもenvelopeとprojected rowのnative ownerを保持し、実Vec capacityを予約量と照合する。探索上限は資料entryの訪問数であり、sort比較回数やCPU時間の保証ではない。native役割数の選択、原子的なbook予約、費用・default/recoveryはこの導出に含まれない。

## Atomic terminal inputs

元claim、登録済みprogram・layout、native sourceから、既存DecodeTokenのstripped proof、conviction evidence、署名control、CourtEndEvidence::TirConvictionの最大Borsh bytesを導く。programの実長・digest、class ID、layout digestが異なる入力は拒否する。V4のpenalty window・bias探索、sampling、二つのlane key、ID・tile・openingの数量とSource構築要求量も返す。設定の次元を読むだけの処理でV4のlogit-bias配列をコピーしない。

これらはnative費用への入力であり、ServiceCostの測定済み係数や完全なworkspace上限ではない。実際の呼び出し回数、署名・codec、paid material取得、archiveの行数、journal/undo、aggregate決済、UTXO/transaction massを含む費用は共通compiler側で別途導く。既存convictionのrent契約を維持し、保持費用を別名で二重課金しない。

## Verification and remaining integration

core/RPCのlibrary test buildは4分28秒で成功。BCT2関連10件と既存の署名付きSelection裁定1件が成功した。後者はgreedy・sampling・V4の正常/不正と2-tile正常の7シナリオを含む。新しい資料テストは全行・全役割のnative座標とcanonical順序、最大値、range置換拒否、探索予算不足、別envelope拒否を検査する。

新しいサイズテストは1/2 tile、異なるD、V4について実際のBorsh型の最大形状と一致する。最大形状のopening値と署名bytesはサイズ照合用であり、有効な裁定・署名として扱わない。既存の署名付き裁定試験も、公開T12の対象claimの証跡ではない。

共通compiler、対象行・beatを固定するversioned notice、実際の予約slotで動くatomic Selection終端、DA/default/recoveryは未接続。さらに現行Batchの`request_inner`はclaim全体の最初のnoticeを見て別noticeを拒否し、repairもその一件からinactive routeを選ぶ。これを採用済みone-honest profileの保護枠と同一視してはならない。選出bond、有限の第三者、防御の権利をnativeに正規化して各caseの予約を保持し、別caseの先行noticeで除外されない構造が必要である。供給役割数はこれらの申立て権利数ではない。canonicalな資料bytesの共有と、各役割の実際の提出観測も区別する。
