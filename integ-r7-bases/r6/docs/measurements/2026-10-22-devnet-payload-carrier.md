# 2026-10-22 isolated devnet payload carrier measurement

状態: **実node測定終了、観測範囲付きの結果**。G14の武装、予約付きcourt service、実モデルdoor、Panel採算の合格判定ではない。元の2026-10-19測定は書き換えない。

## 構成と区間

同一host上のloopback限定4 node、既存floor-only/lane 0・2・10構成。固定したML-DSA-87対応senderを1 processで動かし、負荷開始前に実UTXO RPCで600個の独立outputを確認した。1000 MSKの独立devnet資金は実験準備であり、Account Bondや必要運用資本の採択値ではない。資金供給・node・sender・観測の引数は[生記録](2026-10-22-devnet-payload-carrier/raw/)に保存した。料金、mass上限、fence、G14有効化設定は変更していない。

- 準備: 実受理された33件の2段階UTXO分割。sender開始から全600 output確認まで1,271.123秒。分割件数を必要block数や処理期限保証と同一視しない。
- 負荷: payload 17,408 B、目標1 tx/s、600秒。実際の単調時計区間は600.028秒。
- 送信終了後: 300秒の追加観測。その後sender・observer・driver・4 nodeを所有PID単位で停止し、全PID不在を確認。終了コードはすべて0、エラー出力は空。

[manifest](2026-10-22-devnet-payload-carrier/raw/launcher/manifest.json)はnative base `0e3a23bd8c5987787797048d3df0626949ccae61`＋source patch SHAと追加measurement source SHA、3 executable SHA、実行Python SHAを固定する。後続の文書commitを測定binaryのbuild元と呼ばない。 共通wRPC clientは初期Python manifestの列挙外だったため、[追加source照合](2026-10-22-devnet-payload-carrier/analysis/wrpc-source-identity.json)でtracked baseとbyte一致を確認しsnapshotを補った。この事後照合を初期manifest記録と呼ばない。[build由来](../evidence/palw/g14-completion-2026-10-22/payload-tools-build-provenance.json)と[tooling記録](../evidence/palw/g14-completion-2026-10-22/payload-load-tooling.md)を併読する。

## 観測結果

[全件trace audit](2026-10-22-devnet-payload-carrier/trace-audit.json)ではsender/observerの開始・終了、必要RPC、DAA進行、区間coverage、attempt/result対応が揃い、記録上の欠損・RPC unknown・再送重複は0だった。

| 項目 | 観測値 |
| --- | ---: |
| 負荷用の異なる送信attempt | 600 |
| 対応txidでRPC受理 | 600 |
| 達成した異なるRPC受理率 | 0.9999533355 tx/s |
| 最後の観測chainで受理確認 | 90 |
| 終了までchain受理を未確認 | 510 |
| 最後のmempool観測に残る負荷用ID | 505 |
| 負荷区間の最大観測mempool負荷用ID | 515 |
| 観測したselected-chain detach block | 0 |

mempoolの未掲載とchain受理は同じ事象ではない。未確認510件を505件へ減らしたり、差の5件を拒否扱いにしたりしない。90件の受理も不可逆finalityではない。[queue観測](2026-10-22-devnet-payload-carrier/queue-observations.json)には全snapshotと区間分類を保存した。負荷区間の最初は負荷用1件、最後は515件、追加観測の最後は505件だった。

**この構成・観測区間では、1 tx/sの投入を処理が吸収できず滞留した。** 達成投入率と滞留を観測できた点で、計測txが合計7件だった過去の測定とは範囲が異なる。ただし、最大安定処理率・どのmass次元が律速か・別host/本番構成の飽和点・予約courtの供給保証は測定していない。auditの`saturation`判定はUNDETERMINEDのまま保持する。

90件のmempool初観測→chain受理観測の組は、15.139〜685.530秒、中央値179.695秒だった。これは初観測同士の差で、正確な送信/到着時刻のlatencyではない。510件が右打切りのため全600件の中央値・p99・最大確定latencyへ昇格させない。送信attemptごとのDAAは記録していないため、補間して「送信と同DAA／次DAA」とは書かない。carrying header DAA、accepting block DAA、mempool初観測DAAは別々に保存している。

## byte・mass・feeの範囲

負荷1件の送信記録はpayload 17,408 B、署名済みTransaction DTOのBorsh長24,920 B、compute mass 35,609、transient mass 99,596。storage massは実input/output金額に依存する。scalar massは3次元の最大値であり、block検査は各次元の和を別々に制限する。

負荷600件のpayload合計は10,444,800 B、Borsh DTO合計は14,952,000 B。これはP2P framingやblock-network encoderのbyte数ではない。過去のfloor block 111,927/111,949 Bとencoder同一性なしに差引きしない。

送信ログ上の実input−output feeは、準備33件が180,347,038 sompi、負荷600件が746,970,000 sompi（7.4697 MSK）。準備と負荷を混ぜない。escrow principalをfeeへ加算しない。Panel報酬・機材費・回線費・人件費・利益をこのfeeから作らない。

## 独立算術照合

[独立再計算](2026-10-22-devnet-payload-carrier/independent-recalculation.json)は、送信対象の実観測bodyと実資金rootのRPC記録からinput/output候補graphを復元した。準備33件＋負荷600件の計633件で、native txid、署名済みBorsh DTO長、compute/storage/transient mass、input−output feeが送信記録と一致した。MATCH 633、MISMATCH 0、UNKNOWN 0。全600負荷件のstorage massは181だった。異なる完全bodyが同じtxidを持つ場合は一致扱いしない。

使用範囲は1 input、native v0、ML-DSA-87の既定sender文法、最大32 outputだけで、汎用transaction検証器ではない。実signatureの暗号検証、原子的native UTXO provenance、合意受理をこの解析器は実施しない。MATCHをRPC/native受理や実escrow成立へ昇格させない。計算式と定数のnative source SHAは[source一致記録](2026-10-22-devnet-payload-carrier/analysis/source-identity-check.json)で確認した。

[解析コードの8 synthetic試験](2026-10-22-devnet-payload-carrier/analysis-tests.log)も成功した。これは633件の算術照合と別の検査であり、native consensus試験件数へ加算しない。元の実行中に固定したaudit sourceをそのまま呼び、圧縮読取だけ独立readerを使った。実行後にruntime sourceへ修正を混ぜていない。

## 原資料・再現と限界

[raw圧縮metadata](2026-10-22-devnet-payload-carrier/raw/observer.jsonl.xz.json)に元gzip SHA、元byte数、展開内容SHA、XZ SHAを記録した。元gzip 9,759,610,667 Bはローカル元directoryに無変更で保持し、展開内容16,933,410,437 Bと完全一致を往復検査したXZ 51,834,608 Bを保存した。senderも[同じ完全一致検査](2026-10-22-devnet-payload-carrier/raw/sender-events.jsonl.xz.json)で圧縮した。実験用秘密鍵とnode appdirはGit資料に含めない。

再集計は[実行手順](2026-10-22-devnet-payload-carrier/analysis/run_analysis_steps.py)、[個別commandと終了コード](2026-10-22-devnet-payload-carrier/analysis-steps.json)、[固定したruntime source](2026-10-22-devnet-payload-carrier/runtime-python-source/)、[source一致検査](2026-10-22-devnet-payload-carrier/analysis/source-identity-check.json)に記録する。個々のcommandは保存先を新しいdirectoryへ向け、既存原資料を上書きしない。

同一host・既存producer構成・大きな観測I/Oを含む一回の測定である。観測負荷自体の影響を別途定量化していない。600件は標準payload carrierであり、全G14 court分岐、予約/公平性、再起動・reorg・pruned IBD全campaignではない。実モデルdoor ≤3,120、全分岐のCPU/IO/保持費、Account Bond実装、実Panel採算、heartbeat増加率は未測定である。
