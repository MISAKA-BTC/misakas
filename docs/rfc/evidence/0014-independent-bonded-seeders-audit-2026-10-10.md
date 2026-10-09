# Independent Bonded Seeder policy / RFC–ADR alignment audit — 2026-10-10

> **Later same-day FPR addition:** This records the independent-Seeder revision before Forced Public Retrieval was added. Its 73-link/18-attack-case validation and JSON hashes describe that snapshot. The [FPR addition and latest validation](0014-forced-public-retrieval-alignment-2026-10-10.md) supplements this policy and supersedes those current check counts; do not reuse this earlier PASS as FPR implementation evidence.

## Result and scope

`MISAKA-BTC/misakas`の`pre`の文書改定記録。開始時HEADは`c20fca1d8e3611e201e54d6b662d3d7874ff6328`。
確認時HEADは`e4f3f6f3d`（別作業のimmutable-registration commit）。その変更は保持し、
本改定の比較は開始時の文書snapshotを使う。本作業ではcommit/pushしていない。
現行の主設計は[ADR173 D9–D10](../../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)と
[RFC14 §16](../0014-panel-independent-fraud-prosecution.md)である。
**Panel/minerの常時Seeder義務を撤回し、モデル別の独立Bonded Seeder群を必須とする。
登録同一性・独立した公開取得・PALW/G14計算検証をそれぞれ検査し、ANDで資格へ接続する。**

全既存RFC/ADR Markdown **190件（ADR側169、RFC側21。索引・過去のevidenceを含む）**を一覧化・全文検索した。
RFC08 implementation spec 1件も対象へ加え、合計191件の開始時snapshotと比較した。
関連条文・改定差分の読解と索引への反映を実施し、確認した現行条文の旧Seeder義務・配布任意との衝突を修正した。
全件の機械的な検索範囲と、条文を読んだ範囲は[JSON記録](0014-independent-bonded-seeders-audit-2026-10-10.json)で区別する。
この確認は、全履歴の一字一句の法則証明、runtime監査、devnet試験、暗号方式の安全性証明ではない。
本記録の追加後、RFC/ADR Markdownは191件となる。

前回の[2026-10-09記録](0014-torrent-availability-alignment-2026-10-09.md)は歴史的記録として残す。
そこにあるPanel/full-shard Seeder義務と8項の表記は将来要件として本改定が上書きする。
前回のリンク・試験件数を今回のPASSとして流用しない。既存のコード・wire・params・activationは変更していない。

## Source decisions

2026-10-10のユーザー指示と添付2文書を設計入力とした。図のPanel Seeder / Miner Seederは任意参加の役割として扱い、
「Panel・minerにはSeeder義務を課さず、モデルごとに独立したBonded Seeder群を必須化する」という明示方針を優先した。
一般peerの共有は推奨されるが、独立群の必須枠の代用にも無署名の提供責任にもならない。

| 添付 | SHA-256 |
| --- | --- |
| `1ca52651-640d-4e98-9020-0be64edd683d/貼り付けたテキスト.txt` — 独立Bonded Seeder、7/5、15%等の初期候補 | `0c888f956a871905c284f9767024ee7e7a6a6fb8f9de17c319aff755f80a4d3a` |
| `36227162-8d45-4b76-8a83-e7a99a181c8e/貼り付けたテキスト.txt` — 公開challenge、期限付き回答・不履行の判定 | `c2bc78500b63e4b458cc4a059ecc8cdad9ed6efde7012fb275ba51d81d51c361` |

現メッセージの同一性・全量取得・実計算の3保証、停止原因に依存しない冗長性、公開監査の限界も反映した。
外部transportのT01–T05は既存のpinned source確認記録を保持したもの。この改定で再取得・実装評価したとは記載しない。

## Revised clauses

| 文書 | 確認・改定した境界 |
| --- | --- |
| [RFC14](../0014-panel-independent-fraud-prosecution.md) §16.1–16.4 | 任意共有、独立群だけのcoverage/f/帯域、静的不変manifest・正式infohash/root binding、piece認証後の全量root再計算、6項の資格predicate、監査/PoR/計算検証の分離 |
| RFC14 §16.5–16.7 | TRDCの指定履歴/期限/認証応答map、包含・検閲・reorg/pruningの限界、単独停止とモデル資格の分離、有限清算、提供予算/固定契約rate/復旧escrow、自動再配分禁止 |
| RFC14 §16.8/16.10–16.12 | 18攻撃ケース、独立devnet/Rule E/ECON/MEASの未完gate、7/5/f=2/15%・監査・coding候補、資格epochと停止/復旧状態 |
| [public-verifier ADR173](../../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) D9–D10/優先順位 | 3保証のAND、任意Panel/miner共有、独立提供、同一性失敗/必要保証失効時のモデル単位の新規資格停止、提供defaultと計算fraudの分離 |
| [RFC06](../0006-palw-layer-sharded-panels.md) §3.1 | sharded seatにも常時seedを課さない。計算検証に必要な取得・認証、署名したclaim-materialの開示/保持責任は維持 |
| [RFC08](../0008-palw-claim-backed-consensus-blocks.md) §6.1 | 新REAL/rootの6項資格と計算検証、Rule E未完gate。EXEC_TX/EXEC_SLICEは追加weight/DAA 0、既存permitの遡及失効を防止 |
| [RFC10](../0010-permissionless-palw-panel-and-claim-completion.md) §0.2 | permissionless Panel選出と独立Seeder供給を分ける。独立したseat鍵数・正常bind・監査署名は供給保証の代用にならない |
| [RFC11](../0011-permissionless-model-and-long-context-onboarding.md) §17 | 登録/Beacon Conformance/Active Eligibilityを分離。新登録は不変binding、legacyは旧IDを維持し別のimmutable binding。全量同一性と実計算を区別 |
| [RFC12](../0012-palw-only-consensus-and-native-evm-settlement.md) §2.1 | 同じ3保証/独立提供/Rule Eを継承。最新のモデル供給停止で正当な旧workを遡及voidにせず、他モデル/HEARTBEAT/BASE-0を進行 |
| [RFC15](../0015-panel-free-permissionless-verification.md) G14/roles/§4.2/release gates | Panel=0でも同じ独立供給・6項資格・TRDC/監査の限界を保持。計算判定を監査quorumへ委任せず、固有activation gateも省略しない |
| [RFC09](../0009-palw-remote-miner.md) §3.5 | node-less登録者も常時seed不要。独立提供費/不足預託と登録GASを区別し、固定artifact照合・計算検証・期限付き資格を表示 |
| [RFC13](../0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) §10 | tool evidenceをidentity/availability/computation別に記録。登録/conformanceと資格状態を区別し、候補設定・支払・共有を暗黙有効化しない |
| [seat-root ADR173](../../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) | original readiness/試験履歴を保持し、possessionだけでは新資格を満たさない・共有任意を追記 |
| [ADR175](../../adr/0175-registered-models-are-permanently-immutable.md) 配布状態表/追加境界 | 正式manifest/infohash/root bindingを上書きせず、追加包装は新binding。既存immutable fenceの実装/試験を新availability実装と混同しない |
| [ADR67](../../adr/0067-classes-are-chain-data-kernels-are-the-build.md) Decision 4/6境界 | 「HTTP/Torrentは任意、chainはidentityだけ」の旧modeから新reward profileの独立Torrent gateを分ける。全node保持義務とregistration/possessionのcouplingは導入しない |
| [RFC08 implementation spec](../../design/palw/rfc-0008-implementation-spec.md) 受入境界 | 本文・RFC14に合わせ、独立提供と3保証、Rule Eと既存chain活性を受入条件にする |
| [RFC index](../README.md)、[ADR index](../../adr/README.md)、旧2026-10-09 evidence | 現行の関連条文/候補/未実装statusを一致させ、旧Panel義務はsupersededな歴史的記録と明記 |

## Existing clauses and ambiguous words

未改定文書にもADR173の将来設計優先規則が適用される。以下の関連条文は役割と時点を確認し、
この作業で古い実装記録・wire・測定を新資格の証明へ読み替えていない。

| 検索結果/関連文書 | 確認結果 |
| --- | --- |
| [RFC07](../0007-palw-verification-certificates-and-algebraic-checks.md) のticket/Panel `seed` | 抽選・post-commit challengeの乱数であり、モデル配布Seeder義務ではない。監査/PoRの乱数は別domain・承認条件で接続 |
| [RFC10 dormant implementation](../0010-dormant-implementation.md) | `panel-v3/seed`は抽選domain。既存engineの記録は独立モデル供給/TRDC実装の証拠ではない |
| [ADR34](../../adr/0034-palw-execution-class-model-band-routing.md) capability/availability TTL | seatが計算検査できるという宣言・期限。モデル全量の公開提供leaseと同じobjectとして扱わない |
| [ADR35](../../adr/0035-palw-public-testnet-strategy.md)、[ADR122](../../adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)、旧status auditの`dns_seeders` | node discovery/bootstrapの旧記録。モデル独立コピー数やBonded Seeder提供保証を表さない |
| [ADR88](../../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)、[ADR143](../../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md)、RFC04 | 同一line更新・root ownershipはADR175の不変登録改定に従う。配布先やleaseの変更を内容更新権へ戻さない |
| [ADR90](../../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)、[ADR91](../../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)、[ADR94](../../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) | `seeder`は市場reserveを資金投入した人。Torrent提供契約者ではなく、旧「seederは無報酬」を提供報酬禁止と解釈しない |
| [ADR54](../../adr/0054-palw-share-follows-production.md)、[ADR124](../../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) | 既存production/Panel報酬規則。15%案でコードや過去claimの支払を変更せず、将来の独立ECON/保存則gateで内訳を承認する |
| [ADR144](../../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)、[ADR171](../../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md)/[ADR172](../../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) | 有用な推論、確率的検査/exact court、versioned Kernelの方針を維持。独立同一性/取得と実計算検査は相互代用しない |

上記以外はJSONの`text_inventory_and_term_scan`に記録する。検索で対象外となった旧ADRを、
現行runtimeの可用性保証として合格判定したという意味ではない。

## Three guarantees and the six-term predicate

| 保証 | 必須の検査 | 代用できない証拠 |
| --- | --- | --- |
| 同一性 | Model ID、正式manifest/infohash/root対応、全weights/tokenizer/config/spec/sizeの固定。piece/shard認証後の全量復元root再計算 | 同名モデル、宣言root、正しい一部piece、providerの自己申告 |
| 公開可用性 | 独立群だけのcoverage/復元/f/帯域、reviewed PoR、public serving/default、初回・期限付きcold Full Fetch、funded lease/保持と監査仮定 | peer/IP/bond数、PoR単独、監査成功署名だけ、将来も常時取得可能との推論 |
| 計算検証 | claimが固定Model ID/root/specをbindし、第三者が同じmodelで承認済みPALW検査/localization/exact G14 courtを実施 | root一致、正常download、保存証明、3-of-5サービス監査者の多数決 |

`ModelRewardEligible(model, epoch)`は`ArtifactIdentityValid` AND `TorrentBindingValid` AND
`BondedSeederAvailabilityValid` AND `IndependentFullFetchPolicyValid` AND `G14VerificationPossible` AND
`AvailabilityLeaseValid`。単独のmodel資格は個別claimの計算正当性を意味しない。
旧8条件のcoverage/f、独立性、PoR/公開default、cold予算、G14、予約担保/leaseはこの分解に全て含める。
state foldは指定epoch/snapshotの受理済み履歴から同じ結果を導き、swarm接続やwall clockを合意中に参照しない。

TRDCは指定contract/challengeへの期限内有効公開回答の不在をそのbranchの認証応答map/履歴で検査する。
provider署名はchallengeとbytes/root/pathをbindする。local未受信申告・空mapの自己申告・pruned履歴欠落は不在証明ではない。
軽量clientのnon-membership proofは認証state/coverage/codecを別に定義する。
包含遅延・検閲でもdefaultになり得るという仮定と回答reserveを公開し、reorgではundo/replayする。
TRDCは故意・DDoS原因・全世界の不可用性・計算不正を証明しない。

## Candidates and unresolved gates

7独立完全コピー、初回7 READY/継続5～6 DEGRADED/4以下SUSPENDED、初期f=2、
モデル別報酬枠15%の独立提供予算、監査5名/成功3、将来4-of-7符号化は**初期候補**である。
固定params、測定済み保証、稼働中のquorumではない。1完全コピーが残るだけで継続5閾値を満たさない。
4-of-7で有効5から追加f=2に耐えるとは主張せず、`n_valid - f_remaining >= k`を各snapshotで満たすpolicyを要求する。

失敗providerの未獲得分を残存providerへ自動再配分しない。rate/slot/epoch/丸め/未獲得分の返還・失効先を固定する。
15%だけで実費を賄えるとは仮定せず、必要提供費の事前確保・不足預託・モデル停止中の有限復旧escrowを要求する。
追加発行と既存claimの遡及減額を導入しない。

PENDING → READY/DEGRADED → SUSPENDED → RECOVERING → 指定次epochのREADYという境界を記載した。
1 providerの失敗/1監査者の失敗申告だけではモデルを閉じない。必要な保証・監査・lease・資金等が正規期限に失効すれば、
全Seeder停止を待たずモデル単位の新規資格を閉じる。既存claimと担保は固定期限で清算し、他モデル/HEARTBEAT/BASE-0を進行する。

次はOPENのままである。これらの実装・証明・独立試験・レビューとversioned activationなしに新gateを開かない。

- canonical manifest/root vectors、正式binding移行・legacy ID保持、追加包装とclaim選択の一意性。
- 物理的独立性/Sybil/共通設備・upstreamを扱うpolicyと限界。鍵数から独立copyを証明しない。
- PoRのsoundness/extractor/全provider error composition、実Torrent提供帯域、full cold G14予算。
- Full Fetch監査の公開pool/選択/報酬・正直性/共謀・選択的配布・監査者DDoS・包含/失効/異議規則。
- TRDCのcodec/state root/history coverage/response reserve、検閲・fork-relative replay/IBD/pruning。
- 提供費/担保/並行義務/復旧escrowと支払保存、競合停止による増益の防止。
- 18攻撃ケースの独立devnetと実artifact MEAS。100TB sparse/mockは実downloadの性能証明ではない。
- Rule Eの正確な仕様、全weight/settlement/IBD reader・writer・undo。escrowだけでweight安全性は成立しない。
- 他モデルとchain活性、有限cleanup、明示的network別移行。RFC15 Panel=0はさらに固有gateを要する。

## Documentation validation

**PASS（文書改定の範囲）:** ローカルリンク1,834件を調べ、追加/変更行の73件に欠落file/anchorなし。
開始時snapshotから増えたリンク/anchor不良は0件。改定Markdownのfence、`git diff --check`、
厳密な6項predicate、18行の攻撃matrix、現行文書の旧mandatory Panel-seeding表現の除去を確認した。
旧義務を記録した2026-10-09 evidenceはsuperseded banner付きの履歴として除外する。

既存リンク切れ10件は残っている。旧ADRの別名リンク・未配置先やRFC indexの旧`INDEX.md`参照を含み、
JSONの`preexisting_link_findings`へ列挙した。このPASSはそれらを修復・全履歴を合格としたという意味ではない。
結果と全件の検索/読解範囲はJSONに保存した。Rust runtime/PoR/TRDC実装試験、実swarm/full-fetch/devnet、
外部暗号reviewはこの文書改定では実行していない。既存テスト記録を新gateのPASSとして使わない。
