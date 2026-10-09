# Forced Public Retrieval — additional RFC/ADR alignment, 2026-10-10

## Result and source

`MISAKA-BTC/misakas`の`pre`で、独立Bonded Seeder設計へ**Forced Public Retrieval（FPR）**を追加した。
開始時HEADは`e4f3f6f3d81c75166ea6308bfff0b8aeaf836ca5`。
[RFC14 §16.13](../0014-panel-independent-fraud-prosecution.md#forced-public-retrieval)と
[ADR173 D11](../../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を追加の規範的設計とする。
この作業は文書のみで、source code、wire、params、activationを変更していない。commit/pushは行っていない。

設計入力はユーザーの「も追加する」と添付`ee887999-083b-41f2-bfaf-5a77c706e64b/貼り付けたテキスト.txt`。
SHA-256は`3d6bc18b3627b9cc5334c6febef8f94ff5bd0c560fe0ae638a4af47ffff0c86e`。
外部Filecoin/Celestia等の背景説明は本改定で再調査しておらず、特定DAの実装/採用/安全性を認定しない。

独立Seederの任意共有/3保証/6項predicate/7・5・15%候補は維持する。
[先行する同日の監査記録](0014-independent-bonded-seeders-audit-2026-10-10.md)とJSONは当時のsnapshotの記録である。
そこにある73リンク/18攻撃ケースのPASSは今回のFPR検証へ流用せず、本記録のJSONを最新の確認とする。

## Policy integration

| 追加方針 | 反映した保証と境界 |
| --- | --- |
| 三段階FPR | 通常Torrent取得 → 公開Range Challenge → Forced Full Disclosure。普通のpublic bondが有界な全量義務まで要求でき、事前算術proofやowner/Seeder/監査quorum許可は不要 |
| 全量義務 | 全required file/rangeの実データ公開、固定rootへの認証・全量復元/root再計算、公開再取得/保持。少量応答・私的送信receipt・成功署名・hashだけでは閉じない |
| 公開DA | 全Seeder/全監査者共謀を扱う最終公開には独立した実データ可用性の仕組みとその仮定を要する。具体的DA採用/codec/state/coverage/包含・保持・検閲・reorgはOPEN。DAなしmodeは正直な外部取得者の仮定を明示 |
| Torrent/同一性 | DAは異常時の証拠/供給復旧経路。正式infohash/root binding、immutable Model IDと通常Torrent資格を置換しない。DA公開成功だけでREADYに戻さない |
| 代替Seeder | 期限内の認証代替公開を全量coverageへ一度だけ算入。元の契約を履行済みにできる条件と残る個別責任は事前定義。単独停止だけでモデル失格にせず、署名付き不正回答は代替で免責しない |
| 重大Slash | provider・正式session/challenge・bytes/root/pathに署名が拘束された不正回答、明確なscopeの矛盾保証を客観的に検査。額/offence/回収可能額は承認前に固定 |
| 提供default | 正式期限内の非開示は原因不問の契約不履行。1事象/期間/並行義務の上限を事前固定し、DDoS/故意・計算fraud・重大違反と区別。二重回収禁止 |
| CHALLENGED | 適格FPRの受理でのみ開始。開始だけでBond/既存報酬を没収せず、対象の未検証workは公開/裁定前にFinal/確定weight/不可逆settlement・pruningへ昇格しない |
| 費用/DoS | 最大実bytes/帯域・DA/保持/検証・並行枠をREADY前に予約。登録だけのモデルを保持可能とし、予算不足は新資格を閉じる。bond/fee・要求統合・公平な検査枠・回答reserve・absolute期限/有限cleanupを固定 |
| 新Seederへの参加 | Full Fetch成功者・取得中peerの共有は任意。新Bonded Seederには自身の保存/公開提供/独立性/bond/lease検査を要し、偽監査署名だけで自動認定しない |
| 保証の限界 | 全providerがBond喪失を選べばbytesの取得自体は失敗し得る。Slashで消えたbytesを復元するとは主張せず、新reward/確定weightを止め他モデル/chainを維持する |

FPRは既存6項を増減せず、`BondedSeederAvailabilityValid`へ全量公開義務と履行/default、
`IndependentFullFetchPolicyValid`へ全量認定の証拠条件、`AvailabilityLeaseValid`へ最大費用/担保/保持予約、
`G14VerificationPossible`へ段階的公開・代替・裁定を含むcold deadlineを内包する。
CHALLENGEDのFinal/weight制約は別のwriter/reader/undo条件として併用する。資格だけで個別計算を正当化しない。
TRDCはsession/stage/割当・公開先/DA policy、期限と全量/不足range coverageを指定branchの履歴へbindする。
自己申告・missing history・別forkのmapを不在証明にしない。

## Revised documents and review scope

| 文書 | 追加した条件 |
| --- | --- |
| [RFC14](../0014-panel-independent-fraud-prosecution.md) §16.4/16.5/16.8/16.10–16.13 | 6項への内包、TRDC/責任の接続、FPR全体、T2b、7追加攻撃ケース、ECON/MEAS/weightと資格状態 |
| [public-verifier ADR173](../../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) D9/D11/優先順位/完成証拠 | FPR必須機能、重大Slash/有界default、実データ/DAと仮定、CHALLENGED/費用と限界 |
| [RFC06](../0006-palw-layer-sharded-panels.md) §3.1、[RFC10](../0010-permissionless-palw-panel-and-claim-completion.md) §0.2 | sharded/permissionless seat役割と独立providerの公開契約を分け、全量要求/代替/責任/weightを追加 |
| [RFC08](../0008-palw-claim-backed-consensus-blocks.md) §6.1、[RFC12](../0012-palw-only-consensus-and-native-evm-settlement.md) §2.1、[RFC08 spec](../../design/palw/rfc-0008-implementation-spec.md) | root/slice/settlement/weightのFPR依存、finite cleanup/undo、EXEC weight/DAA 0と独立chain活性 |
| [RFC11](../0011-permissionless-model-and-long-context-onboarding.md) §17、[RFC15](../0015-panel-free-permissionless-verification.md) §4.2 | READY予算と静的登録の分離、Panel=0も全量公開/新責任を要し、監査票を計算真偽へ流用しない |
| [RFC09](../0009-palw-remote-miner.md) §3.5、[RFC13](../0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) §10 | FPR session/stage/snapshot/coverage/実bytes/DA/期限/費用/結末を表示・記録。暗黙の支払/契約を追加せず、toolingは提案のまま |
| [ADR62](../../adr/0062-data-availability-court.md)、[ADR166](../../adr/0166-verifier-unavailability-is-not-producer-fraud.md) | 旧producer court forfeiture/PanelUnavailableへFPR timeoutを直結しない。既存bounded courtを全モデルDA実装と混同しない |
| [ADR67](../../adr/0067-classes-are-chain-data-kernels-are-the-build.md)、[seat-root ADR173](../../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md)、[ADR175](../../adr/0175-registered-models-are-permanently-immutable.md) | 既存tiers/readiness/immutable fenceから新FPR/DA/責任実装を推論せず、全node保持や内容更新を導入しない |
| [RFC index](../README.md)、[ADR index](../../adr/README.md)、先行evidenceのbanner | 最新FPR要件・未実装gateへ索引を接続。先行check件数はその版の履歴として明示 |

開始時の全RFC/ADR Markdown **191件**とRFC08 spec 1件を一覧化・用語検索し、上の追加条文と差分を確認した。
全件の検索範囲と、読解した関連条文の範囲は[JSON記録](0014-forced-public-retrieval-alignment-2026-10-10.json)で区別する。
これを全履歴の意味論的証明やruntime auditとは扱わない。本記録追加後のRFC/ADR Markdownは192件となる。

## Open implementation and acceptance gates

- FPR admission/escalation/sessionと全range coverage、代替履行/default/offenceのcodec/署名domain/branch binding。
- Stage 3実DAの選択・承認、認証state/実bytes/保持/再取得・包含・検閲/正直性仮定と独立review。
- 正確な重大Slashと原因不問defaultの別上限・担保予約・回収/未獲得報酬処理・資金保存。
- 最大全量公開の実規模MEAS、費用reserve/不足・補充、fair admission/回答枠・spam/cooldownと有限清算。
- 全Seeder/監査者共謀、小規模だけ回答、hash-only/偽DA/末尾欠落、代替DDoS、要求乱用、CHALLENGED/private maturity、DA停止/reorg、予算不足/全公開拒否の7追加攻撃ケース。
- Rule E全writer/reader/undoとCHALLENGED/fork choice/Final/settlement/pruning、合法な旧Finalと他モデル/HEARTBEAT/BASE-0活性。
- 全25ケースの独立devnet、公開transport/実artifact/公開DAでのE2E、network別移行・activation。RFC15は別固有gateも必要。

FPRやDAは新しいtrustless転送証明として実装済みではない。契約ペナルティも攻撃原因・故意の証明ではない。
第三者の実取得を無条件保証せず、仮定が崩れても不正reward/確定weightを出さない安全性を別に要求する。

## Documentation validation

**PASS（文書改定の範囲）:** ローカルリンク1,886件を検査し、追加/変更行56件に欠落file/anchorなし。
開始時snapshotから増えたリンク/anchor不良は0件。改定Markdown fence、`git diff --check`、6項predicate、
25行の攻撃matrix、FPRの役割/責任/費用/weight/DA境界と関連8RFCへの接続を確認した。
結果と全件inventoryはJSONへ保存した。既存リンク切れ10件は`preexisting_link_findings`へ残し、今回のPASSへ含めない。
runtime/PoR/TRDC/FPR実装試験、実swarm/full-fetch/DA/devnet、外部暗号review、activation/deploymentは実行していない。
