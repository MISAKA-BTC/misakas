# 実受付deltaと宣言索引の原子的保存

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前ターン`518fce9a5`はpaired FPの保存境界を一つにしたprogress。本ターンは実ノードの受付経路を調査し、全候補の再演・破棄に先立って取り除く必要がある早期DB書込みを修正した。Studio同一claimの公開受付・Final・当選権利消費・対応blockは **NOT_PROVEN**。

## 修正した本番経路

[旧受付フィルタ](../../../../consensus/src/pipeline/virtual_processor/processor/palw_accept.rs)は、単体rehearsalで成功したClassRegistered宣言を直ちに永続化していた。後続objectやブロック全体が失敗した場合も、この補助索引だけが残り得た。将来の全prefix再試行でも破棄した候補の副作用になるため、このwriterを取り除いた。

[宣言のstaging](../../../../consensus/src/pipeline/virtual_processor/processor/class_carriage_commit_v1.rs)は、実際に成功したfoldのobject列とdeltaを照合する。class ID、artifact root、登録DAA、実Class登録の変更、宣言のprofile/canonical IDが一致するものだけを保存候補にする。旧規則のDormant再登録も認める。immutable規則の再開では元の登録DAAと宣言を維持し、再開時点の新しい宣言で上書きしない。宣言bodyとDBコピーの作業量を予約してから固定サイズbufferへencodeし、所有をcommitまで保持する。状態不整合・資源不足はローカル失敗であり、成功候補を一部だけ公開しない。

[processor](../../../../consensus/src/pipeline/virtual_processor/processor.rs)は宣言を、PALW root/delta・UTXO結果・受理statusと同じRocksDB batchに入れる。既存のgenesis用stagingを通常登録にも使い、pruning→PALW→class indexのlock順序を維持する。[store](../../../../consensus/src/model/stores/palw_class_carriage.rs)はbatch処理で旧cacheを無効化し、未commitの新行をcacheへ載せない。class storeのwrite guardをcommit/破棄まで保持する。

これは既存のclass宣言metadataの永続化であり、モデル重みの配信、モデル取得証明、モデル単位の審査を追加するものではない。既存wire・DB schema・モデル係数を変更しない。UTXO-validな別枝による同一IDの索引更新と、現stateのartifact root/登録DAAによるread側照合は従来どおりであり、枝別の宣言ストアへ変更したわけではない。

## 本経路の監査で確定した残件

旧フィルタはTFO3を旧rent見積もり時点で除外する。その後に全prefix preflightを置くだけでは、新しいTFO3を受付できない。完全な新profileの費用・予約plannerをこの入口へ接続する必要がある。

全prefixで候補を除く場合、実AcceptanceData ordinalとUTXOで確定したfeeは保持し、ordinary列に対するBatch/TKR/raw action位置、勝者carrier、返金・prefix・付随資料のmapを再構築する。同一ブロックの登録を読むown attemptも再評価する。TKRの失敗試行やraw actionの既払いfeeを、ordinary候補と一律に除いてはならない。現在の全prefix filter置換は **未実装**。

## 検査範囲

新規宣言と実deltaの対応、batch破棄時の旧行保持、成功commit時のPALW deltaとの同時可視化を回帰コードにする。コードの整形・独立静的レビュー・文書検査を行い、結果をvalidation.jsonへ記録する。Rust実行は[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の通し実装後であり、回帰は **WRITTEN_NOT_RUN**。

G14全枝の費用/予約/期限、FpTokenizer route、完全TFO3のclaim/frame/book/escrow/198/204/206接続、公開quote/署名/受付→Panel/Final→当選権利消費/blockは残る。モデル取得証明は作業対象外。今回配備・送信・公開probeは行っていない。
