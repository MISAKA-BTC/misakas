# RFC-0015 — DAG・PALWブロック・台帳決済

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0015](../adr/0015-dag-awards-and-settlement.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 合意の権限

PALWの正規仕事・権利・合意規則に基づいてDAGを選択する。BFTのモデル採用/停止は対象モデルの認可であり、DNS validator、別BFT ledger finality、reorg vetoを復活させない。

同じ親状態、ブロック本体、適用policyから全Nodeが同じ遷移とPALW state rootを計算する。RPC応答や外部サービスの「承認済み」を合意状態の代替にしない。

## 2. 受賞ブロックの条件

受賞候補は元claim、model/profile、正規Ticket、当選権利、仕事キー、producer/beneficiary、選択親、報酬原資、署名を束縛する。validatorは親状態からFinal、独立Gate C、成熟、現在の停止/取消、期限、予算、未消費を再計算する。

署名検査、当選確認、権利消費、予算減算、work identity、報酬出力、market適用が必要なprofileではその決済を、同じ原子遷移へ結合する。途中だけ成功して二回支払いできる状態を作らない。

## 3. 通常受理と伝播

producerは通常submitBlock経路を使い、他peerはP2Pで受信して同じ検証を行う。特別な台帳書換え、検証省略、別の強制受理RPCを成功経路に置かない。

初回受理、重複再提出、選択履歴への包含、報酬UTXOの生成、決済を別イベントとして記録する。heartbeat・通常block・既受理block再送をPALW受賞と数えない。

## 4. DAG上の一回性

複数modelや並行siblingの仕事を扱う場合も、canonicalな順序・mergeset上限・期限・認証・消費キーを共通化する。RED取り込みや別親への再掲で同じ仕事のweight・報酬を増やさない。

Panel anchorのproducer特権と採用認可を混同しない。anchor/grinding、遅延claim、認証未成立区間、partition後再合流を含めて合意の収束を評価する。局所実行検証が正しいだけでDAGのcommon-prefixが証明されたとはしない。

## 5. 決済frontier

claim Final、Gate C認証、受賞block選択、報酬出力の包含、safe/settled frontier、支出可能性を別状態にする。報酬出力の受付位置を覆う認証済み後続attempt等、対象profileの決済条件を満たしてからSettledとする。

直前frontierが報酬位置の一つ前まで進んだだけでは決済済みにしない。DAAだけの進行や未認証後続attemptを確定根拠にしない。整数の境界と包含位置はprofileの正規比較を使う。

## 6. 取消とreorg

認証失効、conviction、依存取消によるweight/権利停止は対象scopeへ伝播する。消費済み権利、未成熟報酬、確定済み取引を一律に同じrollback対象とせず、受理時規則に従う。

選択枝から受賞が外れたら旧報酬を現在枝の決済と表示しない。同じ権利を別候補が消費した場合は、署名、claim、権利、Panel/Gate C、全peer受理を再検査してから観測checkpointを更新する。

保存deltaのundoで親root、redoで子rootと全会計を再現する。市場処理は同じblockの更新前reserveから再計算し、改変された差分を適用・巻戻しの両方で拒否する。

## 7. EVM/native接続

nativeの正規支払・権利をEVM側へ結合する場合も、同じsourceと一回キーを共有する。EVM表示、receipt、pendingの楽観実行から確定native残高を作らない。最終決済は共通frontierと対象会計規則に従う。

## 8. 受入条件

実LLM claim由来の受賞を通常経路で生成し、3peerで同一block hash、選択状態、適用後root、仕事キー、権利消費、予算差分、報酬出力を照合する。重複block、別候補競合、無Gate C、資金不足、選択親変更、再起動、reorg、pruned復元を検証する。

## 9. EVMとnativeの状態境界

EVM状態の正本は対象profileのselected-parent lineageと正規に取り込まれたpayloadである。DAGに本体が存在するだけのpayloadや楽観的RPC結果を確定EVM状態へ含めない。遅延受理・mergesetの取り込み順と再実行範囲はparent選択に結び付け、rollback後に同じ正規順序から復元する。

nativeは8小数、EVMの表示単位は18小数とするprofileでは`EVM_NATIVE_SCALE=10^10`を固定する。deposit/withdrawは実native原資と一回キーに結合する。withdraw額がscaleの整数倍でない場合に無断丸めして通貨を作らない。reserve lock、mint対応、burn対応、native払出しの保存則を同一遷移で検査する。

ヘッダー拡張を採るprofileでは、64-byteの`evm_payload_hash`、64-byteの`evm_commitment_root`を規定順に扱い、`EvmPayload64`、`EvmCommitment64`等の既存domainで照合する。EVM内部の32-byte値をnative Hash64へ暗黙変換しない。EVM署名とnative PQ署名は異なるdomain・実行環境に閉じる。

chain ID、gas schedule、payload limit、script bridge、receipt形式と発効はnetwork policyへ固定する。未採択の並列lane・別finality案を現行の受賞条件へ追加しない。EVMの存在はLLM計算の暗号学的証明を許可するものではない。
