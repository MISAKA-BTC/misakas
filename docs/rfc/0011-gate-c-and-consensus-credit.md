# RFC-0011 — 独立Gate C・合意credit認証

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0011](../adr/0011-independent-gate-c-certification.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 目的

独立Gate Cは、claimのFinalを合意weight・EXEC credit・受賞・決済へ利用する前に、抽選された検証者による仕事認証を要求する。合意資源の付与は本RFCの独立認証条件を必須とする。

Gate Cは計算の暗号学的証明ではなく、明示した正直割合、資料取得、参加、期限内包含の前提を持つ認証である。G14の一人の正直Panelという条件から、Gate Cの故障比率は導けない。

## 2. 状態を分離する

```text
ClaimFinal != SecurityCertified != AwardAccepted != Settled
```

claimはFinalであっても認証待ちになり得る。未認証claimはfork choiceの仕事weight、EXEC credit、受賞支払い、決済frontierへの成功効果を持たない。ローカルreplayの一致を独立認証の代替にしない。

## 3. 抽選と独立性

母集団、bond、capability、役務期間、重み、actor除外、抽選位置数、承認閾値、復元/非復元、失効条件を対象profileのDrawPolicyで固定する。元claimの検証Panelと同一の承認票を流用しない。

同じbondへ複数位置が割り当てられるprofileでは、抽選位置数と人数を別に報告する。一人に多数位置が集まった事実を、多数の独立検証者がいたこととして表示しない。固定された抽選規則を、都合のよい人数へ変更して成功させない。

乱数はprofileが指定するTB1 purpose/sessionから取得し、対象集合を公開前に固定する。model採用のBFT票をGate C票へ転用せず、用途別authorityと署名domainを検査する。

## 4. 検証者の仕事

検証者は元claimのmodel/input/trace/Readoutと認証された資料を取得し、対象profileの検査義務を実行する。完全再実行を要求するprofileでは部分検査だけで承認しない。model非保有の検証者は取得から始められるが、取得機会がないまま正常承認してはならない。

承認はclaim、抽選record、担当位置、検査policy、世代、元期限に結合する。不一致は適格反証へ、資料不足は資料case又は未検証終端へ進める。

## 5. 認証述語

Nodeは元claimの状態、正規抽選、適格な署名、重複排除、必要承認数、拒否/失効、期限、未解決case、依存を同じcanonical状態から確認する。無署名、少数の任意承認、別roundの承認、失効後の票を数えない。

認証後でも責任期間内の有効反証・取消は規定の成功効果を停止する。認証の寿命、失効、再審、依存creditへの伝播はprofileで固定し、再抽選による無限の救済を行わない。

## 6. 合意・決済への入口

合意仕事を加算する関数を共有し、Final、SecurityCertified、成熟、失効、現在の依存、資本・責任上限を確認する。producer、validator、RPC見積もり、cold replay、IBDで別の簡略判定を作らない。

決済frontierは認証された仕事と選択履歴から導く。heartbeatでDAAだけが進んだことを認証済み仕事の進行とみなさない。信用のない区間の活性と新規認証の循環を別に検証する。

## 7. 検証項目

Finalのみの受賞拒否、別actor/claim/epoch署名、抽選前署名、重複位置、同一operator集中、認証後conviction、restart、reorg、pruned import、旧profileの読取り互換を確認する。正当な否認とlocal障害の理由を保持する。

## 8. 認証consumerと位置数

既存の75抽選位置・必要賛成41位置のprofileは、そのDrawPolicyの一例として元規則を保持する。復元抽出で同一bondが複数位置を得る場合も、位置の重複とoperatorの独立性を別に検査・表示する。G14一席を理由に必要賛成位置を減らさない。

safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来報酬、EXECは同じ認証gateに従う。未認証claimの合意creditは資本cap以内であってもゼロとする。利用者原資の有料役務fee/refundは、その個別契約の条件で別に処理し、新しい発行creditを作らない。
