# RFC-v2-0013 — 公開Bootstrap・Genesis・有効化とRelease接続

## 2026-11-09公開資料・一人前提の追加境界

[採択RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)により、独立選出されたPanelの中の一人が、モデルを事前保有せずに公開資料を取得して全体検証できる新profileを採択する。初回claimから、commit後seed・モデル未保有のPanel取得・一件の完全承認・公開異議・初回creditまでを同じbinaryと設定で試験する。固定bootstrap seedの例外を後続未知乱数の証明にせず、Finalを先に必要とする循環を作らない。 成功の規範所有者は[20](0020-verification-gated-final-and-adversarial-models.md)、公開方針は[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。以下の先行採択・基礎契約と衝突する箇所は、この新profileに限って本境界と16〜20を適用する。旧claim・歴史付録は保持し、状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。設計採択、条件付き論証、抽象検査、native実装、実モデル実測、公開activationを区別し、未成立profileを有効化しない。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

新profileは設計採択済みであり、実装・測定・公開有効化は未完了。[20](0020-verification-gated-final-and-adversarial-models.md)のgateを既存release手順へ接続し、preset・fence・genesis・旧claimの署名版を自動変更しない。bootstrap条件と新claimの検証条件を混同しない。

**優先関係:** 2026-11-08の先行採択はADR-0215と公開裁定v0.2を根拠とする。今回の公開資料・一人前提の新profileには冒頭の0050／0218・RFC16〜20・[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を優先する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0041](../rfc/0041-genesis-bond-free-bootstrap.md)。

**関連RFC:** [03 モデル登録・変換・Artifact](0003-model-onboarding-and-artifacts.md) / [04 モデル二階層・年次カタログ](0004-model-tiers-and-annual-catalog.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [10 DAG合意・Gate C・Native EVM決済](0010-dag-consensus-and-native-settlement.md) / [11 Bond予算・配分・Fee同意・決済会計](0011-bond-budgets-fees-and-payments.md) / [12 認証State・保存・Pruning・IBD](0012-authenticated-state-storage-and-ibd.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. 新しいネットワークの立ち上げ

旧0041の採択済みgenesis-bond-free方針を収録し、他RFCの有効化条件を集めるrelease上の入口とする。ここで新しい発行量、funding制度、特権役割、fence高さを決めない。

## 2. Genesis Bondはゼロ

新しいネットワークでは事前登録bondをゼロとする。初期参加者と後発参加者は同じ公開bond登録・担保・成熟・役割資格を使う。genesis UTXOの保有自体をPanel席や承認権限にしない。

## 3. BASE回復からPALWへ

PALWがない状態で約30分待ち、既存のBASE回復規則を利用するという採択済みの起動順序を維持する。BASEで得た資金または所有するgenesis UTXOを、通常の公開transactionでbondへ移す。

合意上の時計、genesisからの待機起点、無bondで受理できる回復block、実際の報酬、登録と成熟を検査する。bondless heartbeatの存在だけで初期資金を得られると扱わず、DAA停止を壁時計の30分へ置き換えない。

## 4. 有効化の境界

新genesis、既存testnetのflag day、mainnetの設定確定は別の作業である。旧0041が参照する17,400をmainnetの高さに転用しない。15の除外・休眠・未採択台帳と、各機能のrecordを照合する。

公開・配備は文書統合の効果ではない。最新候補のbuild・対象試験・複数node起動・復旧結果を、そのnetwork／artifact／設定へ結び付ける。

## 5. 受入確認

ゼロbondからBASE資金、genesis UTXOからの資金、公開登録、成熟、PALW開始、Panel参加、courtを含む処理までつなぐ。固定運営者の鍵や秘密の初期bondで不足を補わない。既存chainのgenesisや過去の権利は書き換えない。

---

## EXEC常設サービスと初期creditの追加採択

EXEC producerは起動時から稼働し、有効scheduleがなければ待機する。
最初の成功Finalだけを特別扱いせず、[07](0007-claims-exec-and-answer-accounting.md)の共通発行gateでcreditを得る。
既存のBASE回復・公開bond登録・成熟・検証者資格を経て通常経路へ入る。
genesis用の無償permit、認証省略、最初のFinalの先払いを追加しない。

claim・証拠・work anchor・認証・schedule commit・回復をEXEC creditへ循環依存させない。
[10](0010-dag-consensus-and-native-settlement.md)の1 BPSは、必要条件が成立した通常運用の目標であり、
bootstrap開始直後やcredit不足の間の無条件生成保証ではない。
新版署名・clock profile・lead／horizon／grace・credit期限・混合負荷の実測を確定し、
旧schedule／既発行permitと会計が共存できることを確認してから公開有効化を判断する。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0041 | 1–30 | [RFC-0041 — genesis bond を廃止し、公開の BASE 回復経路から立ち上げる](#source-v1-0041-L00001) |

---

<a id="source-v1-0041-L00001"></a>

### 旧RFC-0041 / 原文1–30行

出典: [0041-genesis-bond-free-bootstrap.md](../rfc/0041-genesis-bond-free-bootstrap.md)。配置理由: 主題別統合。

### RFC-0041 — genesis bond を廃止し、公開の BASE 回復経路から立ち上げる

Status: **Adopted launch policy / 実装・立ち上げ検証未完**（2026-10-28、ユーザー決定）。[ADR-0209](../adr/0209-genesis-bond-free-bootstrap.md)。[RFC-0010 §3.2](../rfc/0010-permissionless-palw-panel-and-claim-completion.md#32-freeze-the-eligible-population-and-draw-policy) の分散性・公開参加要件を補足する。

#### 決定と理由

新しいネットワークの genesis にあらかじめ bond を登録する方式を廃止し、初期 bond 数をゼロにする。後から参加した人が取得できない genesis bond という役割・資格・優先権を置かない。最初の参加者も後からの参加者も、通常の公開された bond 登録経路を使う。これは [ADR-0061](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) の「ゼロ bond の genesis も可能」という方針を、新しい立ち上げではゼロを必須にする決定である。

廃止するのは事前登録の bond とその特別な地位であり、genesis に入っている UTXO ではない。初期 bond の資金源には **BASE で得た資金と、所有する genesis UTXO の両方**を使える。genesis UTXO の所有だけで自動的に bond、検証席、anchor、承認権限を与えない。通常の支出署名、担保、登録、成熟、役割ごとの資格と会計を適用する。新しい資金配布・発行量・成熟免除をこの決定から追加しない。

#### 立ち上げの順序

1. genesis bond がゼロの状態で開始する。
2. PALW ブロックがない状態で約30分待ち、既存の無 PALW 時の BASE 回復規則を使う。最初から BASE を30分採掘する特別な起動モードを新設する意味ではない。
3. BASE で得た資金、または genesis UTXO を通常の公開トランザクションで bond にする。
4. 適用される成熟・担保・参加条件を満たした参加者から PALW を開始する。Panel、claim ごとの認証、court、報酬枠の条件はそれぞれ維持する。

この回復・登録経路は、運営者の秘密鍵、事前登録 seat、固定 operator 名簿、個別許可を必要としない。同じ条件を満たす後発参加者も同じ役割に参加できる。bond ゼロの起動と、claim 認証を省略する Panel=0 は別である。

#### 実装との照合と完了条件

約30分はユーザーが指定した起動順序の要件であり、現行の全 preset でこの動作を確認済みという記録ではない。既存コードの bonded BASE と bondless heartbeat を取り違えて、待つだけで資金を得られると結論しない。heartbeat の clock/fee 機能だけでは BASE の報酬経路の確認にならない。

実装確認では、PALW が一度も生じていない genesis からの待機起点・合意上の時間判定、無 bond の回復ブロック受付、BASE の資金取得、genesis UTXO の支出、通常 bond 登録と成熟、PALW への移行を通して検査する。停止中に進まない DAA を壁時計30分と同一視せず、既存回復規則の実際の時計・境界を明示する。未接続があれば実装差分として閉じ、暗黙の特権 bond で代用しない。

稼働済み chain の genesis、過去の bond 登録、担保 UTXO、過去の承認を履歴ごと書き換えない。既存ネットワークで残る genesis 身分への特権依存は、公開参加ルールへの versioned な移行として扱う。全項目実装の DAA 17,400 fence 方針と、新 genesis の起動手順を混同しない。

#### 採択記録

2026-10-28 のユーザー指示は、genesis bond の廃止、PALW ブロックが約30分ない状態で既存 BASE 回復を待ってから bond・PALW へ進む順序、初期資金源に BASE 収入と genesis UTXO を含めることを明示した。本 RFC はその決定の記録であり、実装・検証・ネットワーク起動の完了宣言ではない。
