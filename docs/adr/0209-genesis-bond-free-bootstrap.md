# ADR-0209 — genesis bond を廃止し、BASE 回復と通常 bond 登録で起動する

Status: **Accepted policy / 実装・検証未完**（2026-10-28、ユーザー決定）。仕様は [RFC-0041](../rfc/0041-genesis-bond-free-bootstrap.md)。

## 判断

新 genesis は事前登録 bond をゼロにする。後から取得できない genesis bond という地位を廃止し、運営者を含む全員が通常の公開 bond 登録・成熟・担保条件を使う。

初期は PALW ブロックがない状態で約30分待ち、既存の BASE 回復規則を利用する。その後、BASE で得た資金 **または genesis に含まれる UTXO** を通常のトランザクションで bond にし、必要な資格を満たして PALW を開始する。genesis UTXO 自体は廃止しない。所有者を自動で bond 保有者・検証席・特権 anchor にする仕組みも置かない。

[ADR-0061](0061-zero-seat-genesis-and-right-sized-collateral.md) の任意の zero-seat genesis を、新しい起動方針では必須とする。既存の報酬総量、通常の支出・成熟条件、claim 認証・court の省略を認める決定ではない。

## 根拠と状態

ユーザーは、後から参加できない役割を残すことは不平等で permissionless ではないと判断し、この方針を明示した。再採択の質問は不要である。

約30分の待機起点、無 bond BASE の実受付と報酬、UTXO 支出から bond・PALW までの接続は実装で確認する。既存 heartbeat と bonded BASE の違いを無視して完了と扱わない。稼働済み chain の履歴を変更せず、新 genesis と既存ネットワークの特権除去を区別する。
