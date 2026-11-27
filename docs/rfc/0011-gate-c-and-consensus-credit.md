# RFC-0011 — G14・公開Courtによる合意credit

- 文書区分：**DESIGN_SPECIFICATION**

- 改定日　2026-11-27。DESIGN_SPECIFICATION、採択済み。公開有効化はRFC-0020による。
- 判断理由は[ADR-0011](../adr/0011-independent-gate-c-certification.md)、Finalの遷移は[RFC-0009](0009-court-and-claim-final.md)に従う。

## 1. 安全性の前提

新profileでは独立Gate Cを廃止する。G14の選出1席が指定された仕事を完全再実行し、元claimとassignmentに署名する。選出外を含む誰でも全FOW1 claimを発見し、同じモデル・入力・実行資料を取得して再実行し、公開Courtへ提出できる。Public Panelはこの経路を使う参加者の呼称で、登録資格・抽選・定足数を追加しない。

安全性には、不正claimごとに少なくとも一人の正直な監視者が実際に検査し、資料を取得するか供給違反を申し立て、元期限内に有効な要求・反証が包含されて裁定されることを仮定する。監視者の不在、取得不能、検閲、Court容量不足の下では不正を見逃し得る。署名や未告発の件数から監視の実施を推定しない。

## 2. Finalと反証

各Nodeは選択枝で元claim、選出1席の有効な完全再実行承認、資料公開と供給義務、元の異議期間の経過、全依存、Courtの終端を確認する。期間中、未解決case、未履行資料義務、成立した反証、取り消された依存では成功Finalを与えない。根拠を欠く異議には元の有界受付規則を使う。正当な単独反証は虚偽承認に優先する。BFTはモデルの採用・停止・廃止を担う。

Minerの不正とPanelの虚偽承認は、各人の署名・義務・違反証拠・責任lotに個別に結び付けて裁く。片方の有罪だけで他方をslashしない。選出外の立証者も、元証拠・identity・決定論的分配planの条件を満たせば受給できる。累計実回収額から49%をfloorした枠の増分を一度だけ割り当て、残差をburnする。通常のPanel役務5%は別原資とする。

## 3. 合意・決済・保存

producer、validator、RPC、再起動・IBDは受付時profileと同じ選択親から仕事の適格性を導く。新profileでは上記の成功Final、当該効果が要する成熟・当選・未消費権利・依存・予算を検査する。独立抽選、attestation、SecurityCertified行を要求または合成しない。fork choiceのsafe weight・safe frontier、immature/live量の区分、rule E、EXEC credit、受賞・発行、settlement depthとsettled anchorに適用する。保留中の仕事から成功creditを先取りしない。受賞の選択と報酬UTXOの決済はそれぞれの原条件を満たす。heartbeatによる時計の進行だけでfrontierを進めない。

未決Court、資料供給・責任の元期限、依存、未消費権利、既受理の署名、reorg用の差分を保存する。pruned importは受付世代、Finalの根拠と未解決義務を検査し、巻き戻しで同じ元期限と残高を復元する。

## 4. 移行と公開判定

新規則の開始点・network・profile版を明示し、claim受付時に固定する。開始前のclaim、既存Gate C抽選・署名・認証、成立報酬、検証lot、元期限は旧契約で完結する。現在時刻で既存claimを新規則に移さない。旧wire tagを再定義しない。移行未実装のノードで認証フラグを切って新profileの成功としない。

公開前に各FOW1経路で、正常な仕事、選出外監視者によるMinerと虚偽承認Panelの反証、資料不供給、期限境界、重複提出、旧新claimの同居を通常入口から試験する。無効claimのweight・EXEC・報酬・frontierへの寄与がゼロであること、有効claimがGate C票なしで進むこと、ArchivalとPrunedのIBD・再起動・reorg後のroot・責任・UTXO一致を照合する。1 BPSと平均600秒は測定対象で、異議期間を短縮する条件に使わない。

## 5. 後発反証の条件付き保証

対象claimごとに、認証された有界局所反証の存在、実際に活動する正直な監視者、資料取得または帰責可能な不供給、期限内の包含・処理、正しいcheckerと状態遷移、回収までの担保拘束を仮定する。公開前に、局所違反の存在から提出資格、期限・容量、個人署名と責任lot、実回収と一回決済までの各含意を、対象profileについて示す。資料不供給しか立証できない場合の裁定対象は対応する供給契約に限る。

もう一つの必須条件は不可逆な利益の防止である。初回申立可能期間、適格な係争・未履行義務、成立反証の各状態で、対象claimから生じる新しい合意weight、EXEC credit、当選権利の消費、受賞出力、safe・settled frontierへの寄与を各consumerでゼロにする。正常claim、独立した別claim、旧世代を対照に、同一処理単位の反証優先をproducer・validator・RPCで照合する。
