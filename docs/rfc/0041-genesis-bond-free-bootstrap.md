# RFC-0041 — genesis bond を廃止し、公開の BASE 回復経路から立ち上げる

Status: **Adopted launch policy / 実装・立ち上げ検証未完**（2026-10-28、ユーザー決定）。[ADR-0209](../adr/0209-genesis-bond-free-bootstrap.md)。[RFC-0010 §3.2](0010-permissionless-palw-panel-and-claim-completion.md#32-freeze-the-eligible-population-and-draw-policy) の分散性・公開参加要件を補足する。

## 決定と理由

新しいネットワークの genesis にあらかじめ bond を登録する方式を廃止し、初期 bond 数をゼロにする。後から参加した人が取得できない genesis bond という役割・資格・優先権を置かない。最初の参加者も後からの参加者も、通常の公開された bond 登録経路を使う。これは [ADR-0061](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) の「ゼロ bond の genesis も可能」という方針を、新しい立ち上げではゼロを必須にする決定である。

廃止するのは事前登録の bond とその特別な地位であり、genesis に入っている UTXO ではない。初期 bond の資金源には **BASE で得た資金と、所有する genesis UTXO の両方**を使える。genesis UTXO の所有だけで自動的に bond、検証席、anchor、承認権限を与えない。通常の支出署名、担保、登録、成熟、役割ごとの資格と会計を適用する。新しい資金配布・発行量・成熟免除をこの決定から追加しない。

## 立ち上げの順序

1. genesis bond がゼロの状態で開始する。
2. PALW ブロックがない状態で約30分待ち、既存の無 PALW 時の BASE 回復規則を使う。最初から BASE を30分採掘する特別な起動モードを新設する意味ではない。
3. BASE で得た資金、または genesis UTXO を通常の公開トランザクションで bond にする。
4. 適用される成熟・担保・参加条件を満たした参加者から PALW を開始する。Panel、claim ごとの認証、court、報酬枠の条件はそれぞれ維持する。

この回復・登録経路は、運営者の秘密鍵、事前登録 seat、固定 operator 名簿、個別許可を必要としない。同じ条件を満たす後発参加者も同じ役割に参加できる。bond ゼロの起動と、claim 認証を省略する Panel=0 は別である。

## 実装との照合と完了条件

約30分はユーザーが指定した起動順序の要件であり、現行の全 preset でこの動作を確認済みという記録ではない。既存コードの bonded BASE と bondless heartbeat を取り違えて、待つだけで資金を得られると結論しない。heartbeat の clock/fee 機能だけでは BASE の報酬経路の確認にならない。

実装確認では、PALW が一度も生じていない genesis からの待機起点・合意上の時間判定、無 bond の回復ブロック受付、BASE の資金取得、genesis UTXO の支出、通常 bond 登録と成熟、PALW への移行を通して検査する。停止中に進まない DAA を壁時計30分と同一視せず、既存回復規則の実際の時計・境界を明示する。未接続があれば実装差分として閉じ、暗黙の特権 bond で代用しない。

稼働済み chain の genesis、過去の bond 登録、担保 UTXO、過去の承認を履歴ごと書き換えない。既存ネットワークで残る genesis 身分への特権依存は、公開参加ルールへの versioned な移行として扱う。全項目実装の DAA 17,400 fence 方針と、新 genesis の起動手順を混同しない。

## 採択記録

2026-10-28 のユーザー指示は、genesis bond の廃止、PALW ブロックが約30分ない状態で既存 BASE 回復を待ってから bond・PALW へ進む順序、初期資金源に BASE 収入と genesis UTXO を含めることを明示した。本 RFC はその決定の記録であり、実装・検証・ネットワーク起動の完了宣言ではない。
