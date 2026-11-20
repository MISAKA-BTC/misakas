# TB1・FOW1統合改訂 — 2026-11-20

**状態: ADOPTED_DESIGN_NOT_ACTIVATED。** 今回の明示的な修正指示に基づき、モデル登録・コア採掘出力・仕事会計・乱数・移行を同じ新規profileへ統合した。完全な本番実装、具体的Qwenの費用定理、公開有効化を完了したという記録ではない。

## まず読む文書

- [共通移行契約](../../../governance/palw-tb1-fow1-transition.md): 全用途の採用・置換・継承と、新規受付・旧責任の境界。
- [RFC0061 TB1](../../../rfc/0061-palw-internal-threshold-beacon.md) / [ADR0229](../../../adr/0229-palw-internal-threshold-beacon.md): 添付方式を統合した乱数正本。
- [RFC0062 FOW1](../../../rfc/0062-palw-function-output-and-work-admissibility.md) / [ADR0230](../../../adr/0230-palw-function-output-and-work-admissibility.md): LLM関数由来の採掘値・登録適格性・仕事の正本。
- [公開裁定方針v0.5](../../../design/palw/consensus/public-adjudication-policy-v0.5.md): Panel＋Courtへの接続。
- [全旧記述の適用台帳](BEACON_AND_POLICY_COVERAGE.md)、[変更ファイル一覧](changed-files.json)、[検証結果](VALIDATION.md)、[入力の来歴](SOURCE_PROVENANCE.md)。

## 五項目の修正

| 指摘 | 反映した規則 |
| --- | --- |
| RFC59/60・ADR227/228の保証対象 | 自然回答y、コア推論の採掘出力z、試行ID s、補助receipt rhoを分離。元文書の該当本文も改訂し、rhoの非衝突性をz・費用の保証にしない。 |
| モデル登録 | ModelRegisteredとFOW1Admissibleを分離。具体的系列・kernel・数値意味・入力分布・Readout・全traceを不変descriptorへ結合。共通TheoremRuleと有界適合checkerを要求。 |
| beacon・Ticket | 完全BGW VSS＋PQ認証私信＋署名付きshare certificateのTB1へ統一。EXEC/PANELを独立化し、SCHEDULE/ROLEを用途分離。FOW1に追加ADMISSIONや第三Ticket抽選を設けない。 |
| 仕事会計・再利用 | 初回Ticket照会前の複数試行費用を同じruleで評価。正しいcache・batch・外注も攻撃者の能力に含め、receipt再包装や旧quantumで仕事を増幅しない。 |
| 優先順位・移行 | activation境界以後の全新規PALW採掘・報酬・credit・合意workにFOW1/TB1を必須化。旧受理claimのみ旧署名・元期限・責任でdrainする。 |

## 「同じ出力が一回」の意味

同一試行をMinerとPanelが再実行した場合は、同じ出力を得る必要がある。異なる試行のコア採掘出力zについてのみ、登録された系列の定理に明示した非衝突上界を要求する。自然言語の回答yは重複してよい。

次の三条件を全て満たした具体的なruleがなければ、新profileの採掘を有効化しない。

1. **G-EXEC:** 全実行関係とReadout/Endが認証され、不正な元実行に有界な反証を構成できる。
2. **G-COLL:** 許容入力・候補選別・共有seed・破棄候補を含む範囲で、異なる試行のz衝突上界が証明されている。
3. **G-COST:** 初回Ticket照会前に必要な仕事を、前処理・共有・cache・batch・外注等を許した複数試行の計算モデルで証明する。

個別artifactの構造・数値・入力等の適合前提は有界に全件検査する。暗号・故障上限・正直Panel・native安定性などの外部安全性仮定は明示した条件として保持し、Nodeがその真偽を証明したとは扱わない。未知のモデル重みの性質を「外部仮定」に移して登録を通すことも認めない。

## 新Ticket

\[
T=H_{\mathrm{FOW1-ticket}}\left(\operatorname{Enc}_{P}(s,R_x,R_y,R_z,R_\tau)\right).
\]

sとTicketは32byte。Ticketを256bit unsignedとして読み、事前固定した一つのtargetで一度だけ判定する。zを含む規範実行を終えてから四Root・Ticketを作る。Readoutへ自分のR_z/R_tau/Ticket/rhoを戻さない。署名乱数・receipt・Panel seed・証明のsupport subset・自由saltから別Ticketを作らない。

この式を規定しただけで費用下界が完成するわけではない。G-COSTは別の必須条件である。

## TB1の採用範囲

添付の提案RFC0060は、既存MOR1と番号が重複していたため0061へ統合した。添付ZIPはbyte一致で[原本](upstream/Misaka_PALW_TB1_Design_Bundle.zip)に保全した。

TB1はn=3f+1、私的復元t=f+1、公開certificate q=2f+1を区別する。予測不能性の境界は**最初の正直share公開前**である。固定された全qualified dealerを含め、後からsubsetを選ばない。旧work/header由来、seal-reveal、BLS、定数、別roundへfallbackしない。

旧版のPanel乱数を次のrosterへ派生する代わりに、統合版は独立したROLE sessionを使う。EXEC権の割当はSCHEDULEへ分ける。未知の受付順位を要する別controlを採用する場合だけADMISSIONを使い、通常FOW1の受付順はTicketとslot IDのままとする。これら追加purposeの実装・合成検証は元のfixture範囲外である。

## 履歴と実装の保全

現行20 RFC-v2・50 ADR-v2には各担当の適用節を追加した。凍結対象の原RFC/ADR、統合中の元条文、当時の検証結果は、今回の改訂を過去に適用した形へ書き換えていない。旧sourceの残存は新profileの選択肢ではなく、互換検証と来歴のためである。

原文の全該当箇所には台帳で適用先を付け、現行入口・実装計画には明示的な置換案内を追加した。過去のsource manifest、既知不整合baselineの書換えで検査を見かけ上合格にしない。

## 未完成の事項

- 完全BGW・PQ通信・native安定broadcast・曝露journal・全consumerの実装と合成評価。
- 初期holder集合とfault仮定、ROLE移行、最悪時容量・復旧の実証。
- 具体的なLLM系列のG-COLL/G-COST、およびその全artifact適合前提を検査する実装。
- 公開networkでの有効化境界と旧受理claimのdrainの実装・検証。

これらが未完成の間は新規profileのactivation条件を満たさない。現在のruntimeを文書変更だけで停止又は変更したとも、任意のQwenが採掘適格になったとも表示しない。
