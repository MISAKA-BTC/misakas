# ADR-0209 — genesis bond を廃止し、BASE 回復と通常 bond 登録で起動する

Status: **Accepted policy / 候補 C10 の実装・private node 統合検証済み**（方針採択: 2026-10-28、ユーザー決定。検証: 2026-10-31）。公開起動は未実施。仕様は [RFC-0041](../rfc/0041-genesis-bond-free-bootstrap.md)。

## 判断

新 genesis は事前登録 bond をゼロにする。後から取得できない genesis bond という地位を廃止し、運営者を含む全員が通常の公開 bond 登録・成熟・担保条件を使う。

初期は PALW ブロックがない状態で約30分待ち、既存の BASE 回復規則を利用する。その後、BASE で得た資金 **または genesis に含まれる UTXO** を通常のトランザクションで bond にし、必要な資格を満たして PALW を開始する。genesis UTXO 自体は廃止しない。所有者を自動で bond 保有者・検証席・特権 anchor にする仕組みも置かない。

[ADR-0061](0061-zero-seat-genesis-and-right-sized-collateral.md) の任意の zero-seat genesis を、新しい起動方針では必須とする。既存の報酬総量、通常の支出・成熟条件、claim 認証・court の省略を認める決定ではない。

## 根拠と状態

ユーザーは、後から参加できない役割を残すことは不平等で permissionless ではないと判断し、この方針を明示した。再採択の質問は不要である。

約30分の待機起点、無 bond BASE の実受付と報酬、UTXO 支出から bond・PALW までの接続は実装で確認する。既存 heartbeat と bonded BASE の違いを無視して完了と扱わない。稼働済み chain の履歴を変更せず、新 genesis と既存ネットワークの特権除去を区別する。


## 実装・検証追記 — 2026-10-31

[候補 C7 の有限 node 記録](../evidence/palw/rfc0041-bond-free-bootstrap-2026-10-31/README.md)に、空 genesis registry から実 BASE 支払、所有 genesis UTXO の通常支出・署名登録・成熟、最初の PALW と抽選検証者による75/41認証、競合支出の reorg、同DBからの Consensus 再起動までの PASS を記録した。core 3 件・node 1 件と7 package統合ビルドの結果であり、公開配備や実LLM再実行campaignを宣言しない。並行編集後の候補は別結果として管理し、C7 の PASS を流用しない。


## 後続候補の再検証 — C10（2026-10-31）

[候補 C10 の記録](../evidence/palw/rfc0041-bond-free-bootstrap-2026-10-31/candidate-c10/README.md)に、Prefix helper統合失敗の解消、7 packageビルド、RFC0041 core 3件・node 1件、関連回帰11件のPASSを追加した。実行バイナリは固定コピーし、SHAを実行前後で照合した。C7と継続編集中のソースは別管理とし、公開起動・実LLM再実行campaignの宣言は行わない。
