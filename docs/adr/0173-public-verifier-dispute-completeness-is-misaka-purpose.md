# ADR-0173 — MISAKA's purpose: public-verifier dispute completeness

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Accepted as the project's design mandate at the user's request, 2026-10-07. Implementation and activation remain pending. This decision assigns no wire id, consensus fingerprint, activation height or deployment.
* **Purpose:** 正しい固定modelを保有/取得できたPanel外のpublic bondが、producer秘密状態なしにclaim固有の認証証拠から不正をlocalizeし客観的convictionへ到達する。モデル入手自体はchainが管理・保証しない（ADR177）。
* **Design path:** [RFC-0014](../rfc/0014-panel-independent-fraud-prosecution.md) specifies the implementation work and completion gates. [RFC-0015](../rfc/0015-panel-free-permissionless-verification.md) specifies a separately gated, deferred Panel=0 mode.
* **Complements:** [ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)'s useful local-inference purpose, [ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md)'s verification direction and [ADR-0172](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)'s kernel-only extension boundary.
* **Audit:** [All-RFC/ADR alignment report](evidence/0173-mission-alignment-audit-2026-10-07.md). The report records document versions, dispositions and the limits of this documentation change.
* **Amendment, 2026-10-10:** 旧Seeder/TRDC/FPR可用性方針は、後続の[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)で撤回した。従来の整合チェックは履歴で、新配分の経済/実装証拠ではない。
* **Further amendment, 2026-10-10:** 旧Seeder/TRDC/FPR可用性方針は、後続の[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)で撤回した。従来の整合チェックは履歴で、新配分の経済/実装証拠ではない。

## 1. MISAKAの中核目標

**後続改定:** 以下の公開検証目標は、検証者が正しい登録modelを得られた条件で成立する。
model配布は合意の管理・強制対象にせず、旧無条件取得保証をADR177で撤回する。

MISAKAは、利用者がローカルで実行する有用なLLM計算を、公開に検証可能なワークとして扱うP2P基盤を目指す。その安全性の中核目標は、**固定Panelの多数派を信用しなくても、普通のpublic bondを持つ独立参加者が、公開され認証された証拠だけで不正を特定し、合意規則が検証するconvictionとproducerへの責任追及まで完結できること**である。

Panelが不正なclaimを一度licenseしても、その多数決で計算上の矛盾が正当化されてはならない。ライセンスは処理状態であり、算術的な正しさの最終根拠ではない。Panelの正直な多数派、特定のgenesis operator、producerと同じプロセスやローカルディスクを、安全性の前提に置かない。

これは設計目標の採択であり、現在のコードがこの条件をすべて満たしたという宣言ではない。「public verifierが実際に不正を発見して証明できる」と「正直な参加者が存在すれば必ずすべての不正を即座に発見する」は別の性質である。

## 2. 用語と適用範囲

| 用語 | この決定での意味 |
| --- | --- |
| 普通のpublic bond | 公開されたidentity・bond・maturity・collateral規則を満たす参加者。Panel非所属でも参加可能で、genesis allowlist、operator anchorの特権、モデル所有者の承認を必要としない。無資源・無担保での無制限な訴追を意味しない。 |
| public authenticated material（ADR177のscope） | claimのinput/state/trace/output/witness等の有限な認証証拠。modelはoff-chainで得た固定rootのbytesを前提にし、model weightsの取得を強制する公開義務にはしない。hash/成功署名だけでは計算検証を代替しない。 |
| producerの秘密状態を使わない | producerだけのcapture、未公開FOLD prefix、KV cache、tile preimage、RAM、ファイル、秘密鍵、内部prover API、producerインスタンスへの参照を訴追の前提にしない。検証者自身の計算・メモリ・秘密乱数は使えるが、convictionは公開検証可能でなければならない。 |
| localize | 全体の異常や検査失敗を、認証された入力・境界・状態に拘束された有界の不正箇所とterminal proofへ落とすこと。leaf番号の報告だけでは完了しない。 |
| objective conviction | 任意の合意検証ノードが、規範的なcourt規則と証拠から同じ有罪結果を計算すること。Panel投票、LLM回答の良し悪し、ローカルHTTP失敗を有罪の根拠にしない。 |
| public-verifier dispute completeness | 許可されたclass/profileの不正について、外部参加者が異常を発見した後、認証された公開証拠から局所化と客観的裁定を有界に完結でき、必要証拠の非開示でその経路を永久に塞げない性質。 |

目標は各承認済みKernel・VerificationPlan・task/context・privacy profileに適用する。任意の未対応モデル、任意の自然言語回答の真偽、未定義のfaultまで裁定できるという意味ではない。登録者が不正箇所を検査対象から外すことも認めない。

## 3. 規範的決定

### D1 — 訴追の権利はPanelへの選出と独立

普通のpublic bondは、Panelに選ばれずとも証拠を取得し、検査し、規範的なdemand/accusation/court/terminal proofを提出できなければならない。Panel bind・receipt・seat schedulingは通常処理の担当割当であり、外部検証者の証拠アクセスや有効なfraud proofの受理条件ではない。

現行のoperator-anchor特権やseat専用APIは移行対象として扱う。新しい設計に永続的な安全性前提として持ち込まない。Sybil対策の公開bond条件や有界の訴追費用は維持するが、少数の指定operatorによる許可と混同しない。

### D2 — 必要な証拠を、producerの善意に依存させない

登録・claimの新しいprofileは、必要証拠のcommitment、取得単位、認証、応答責任、保持期間、deadline、最大bytes/workを定義しなければならない。公開openingから、検証者自身の状態で異なる出力を発見し、**producerが実際にcommitした異なる値**を開いて裁定できることが必要である。正直な検証者のreplay値で、不正producerのopeningを代用しない。

全captureを全ノードに複製する必要はない。ローカル保管・streaming・Merkle/FOLD opening・第三者DAは利用できる。ただし、必要箇所が公開に取得できない場合には、有界の認証された非開示手続で解決できなければならない。「captureは家にある」ことを、producerの私的状態にしかアクセスできない訴追設計の理由にしない。

旧`PanelDa`/`claim_readers_v2`の「producer・bound seats・すでに開かれたcourtのchallengerだけが読める」という条件は、この目標を満たす新しい報酬profileの根拠にはならない。courtを開くために不正の発見が必要なのに、発見に必要な入力をcourt開始前に読めない循環を残さない。公開証拠で裁定できない秘密入力profileは、別の明示的に承認された公開検証可能な証明方式が同等の局所化・裁定・非開示条件を満たすまで、新しい報酬経路で有効にしない。ZK等の方式をこのADRで採用済みとは扱わない。

### D3 — 検出・局所化・裁定・回収を別々に証明

新しい受入試験は次の経路を、独立したpublic bondで連続して成立させる。

```text
公開されたclass / claim / authenticated material
  → 外部public verifierによる不正の発見
  → commitmentに拘束された有界の局所化
  → exact courtによるobjective conviction
  → claim void / 規則上のFinal reversal
  → producerの回収可能な担保・escrowへのslash
```

算術的不正については、必要証拠が開示されればその算術的不正を裁定できることが必要である。必要証拠を非開示にする攻撃については、認証されたdemand、責任あるresponse/disclosure、規範的なdeadline/default裁定の経路を持つことが必要である。非開示のconvictionを、未証明の算術的不正のconvictionと呼び換えない。単なるping失敗・切断・verifier不在をproducer fraudと扱わない。

検査失敗、fault ledger、leaf番号、court kernelの実装、`supports_court()`、既存のfamily certificateのいずれか一つだけで、この連続した性質が証明されたとは扱わない。whole-job replayを使う場合も、公開の入力から始まり宣言した最悪時資源内に収まる必要がある。巨大な無界replayを隠れた最終手段にしない。

### D4 — 多数派のlicenseは有効な客観的証拠を覆せない

新設計では、Panel多数派または全席が悪意でも、客観的な証拠が提出されれば、それを再度Panelに投票させたり、Panelの許可待ちにしたりしない。有効なproofの処理を無関係なopen courtで封じない。重複提出・競合court・reorgは、一度限りの決済と原子的なstate/escrow/lock更新で扱う。

`ReceiptLicensed`と`Final`は、証明不能なclaimを真実に変えない。Final後の責任期間、証拠保持、proof受理、担保解放、報酬回収・reversalを整合させる。無期限の訴追を約束するのでなく、設計で定めた有界期間内の公平な取得・提出・包含機会を確保する。

producerへのconvictionと、false `Valid`へのseat責任は別に証明する。Panel seatをslashする場合は、そのseatが署名したscheme/scope/evidenceの主張と客観的な偽りの関係を示す。別scopeの正しい部分検査に署名したseatを、claim全体が不正だったことだけで自動的に有罪にしない。

### D5 — 報酬・weightの前提をdispute completenessまで強める

新しいclass/profile/kernel/planのmineability・報酬・consensus weightの有効化には、この外部訴追経路の成立を必要条件とする。登録、source fidelity、kernel catalogへの収載、独立したseat readiness、market開設、通常の正直claimのFinalは、いずれも代わりにならない。

受入証拠は、そのprofileが許す最大context・shape・recurrence・MoE・state boundary・dissection/close量と、対応する全fault種類を対象にする。小さい既知fixtureや一つのfamilyの成功だけを、canonical大規模profile全体の証明にしない。未対応の局所化・opening・裁定は、missing capabilityとして報告し、その新しい報酬profileを閉じたままにする。

これは将来のversioned gateである。文書変更によって既存ネットワークのclassを失効させたり、過去のcertificateを再解釈したりしない。旧claimは旧規則で再生し、移行条件を別途指定する。

### D6 — 検出の確率と、発見後の客観性を混同しない

[ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md)の全constraintを対象とする承認済み検査、明示的なsoundness composition、post-commit乱数、residual errorの方針を維持する。courtは発見されなかった不正の確率を消さない。「1 honest seatが存在する」「quorumを集める」「少数のraw trace leafをsampleする」だけを、全不正の検出保証にしない。

外部参加者が異常を実際に発見し、そのprofileの訴追資源を用意し、規範的期間内に有効な証拠を包含できる条件の下で、その後の局所化・convictionを多数派の善意に依存させない。チェーン全体の検閲耐性・clock・DA・経済的監視継続性も別の必要条件であり、「一人で安全」の一言で省略しない。

外部public verifierは、producerとの共有秘密、選ばれたseatだけの秘密preprocessing、監査者の単なる主張を必須にしてはならない。private sketch等を通常処理の最適化として使う場合も、外部検査・局所化・公開court証拠への別の完結した経路を持つ必要がある。

### D7 — Panel=0は完成した基盤の上でのみ検討・有効化

Panelは移行中の担当分散・通常の検査・receipt・経済的監視供給を担い得る。目標はその正直な多数派への依存を除くことであり、このADRはPanelを今すぐ削除する決定ではない。

**RFC-0014の全completion gatesが実環境で成立するまで、RFC-0015のPanel=0は有効にしてはならない。** 成立した後も、RFC-0015固有のpending/maturity、positive verification、DA、exposure、fork-choice、監視の誘因、負荷・migration試験と明示的activationが必要である。`panel_seats=0`への設定変更、quorumの空集合成立、silent acceptanceでは代替できない。ADR-0061のzero-seat **genesis**は、固定Panelによる検証を廃止するPanel=0とは別の意味である。

Panel=0の方向は、固定Panelをpermissionless verifierとobjective fraud proofへ移行すること。検証自体をなくしたり、challengeがなければ必ず正しいと扱ったりすることではない。

### D8 — Post-commit challengeを外部参加者も完全再構成する（2026-10-08）

新しいKernel/model/claim/EXEC slice/public-check経路は、[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)の唯一のpost-commit protocolをbindする。ordinary outsiderは公開のpolicy、statement/prover-message commitment、将来のqualifying PALW useful workとFinal/settlement provenanceから、同じsource順序・seed・query/vector・各interactive transcriptを再構成できなければならない。producer/Panelから配布されたseedを信頼しない。

beaconはcommit済みstatementのchallengeを決めるだけで、semantic/constraint/court/public-material/resource completenessやG14を代替しない。モデル登録はStatic Admission → Beacon Conformance → Active Eligibilityに分け、RegisteredDormantやsampled conformance PASSを報酬資格と混同しない。Kernel作者が選んだfixtureだけの一致もKernel soundnessの証明にはならない。

sourceはconsuming challengeと独立に有効な将来canonical workに限定し、candidate self-source・循環verification・free header reroll・committee/DNS/BFT beaconを排除する。複数Final workのmixだけでunbiasedと主張せず、source unpredictability、grinding、withholding、retry/reorgのboundsを立証する。source不足はpending/既定deadlineで扱い、heartbeat・BASE-0・取引・chainを止めたり弱めたりしない。

同じseedを多数watcherが検査しても独立試行とは数えない。mismatchは認証localization・exact courtへ進め、probabilistic failureだけでconvict/slashしない。有効なexact fraud proofを出すために新beacon待ちを要求しない。challenge再現・公的material・objective outcomeの全経路をG14に追加し、RFC15のPanel=0はその達成後も別gate/activationを要する。この追記は実装・activationの宣言ではない。

### D9 — 永久不変のモデル同一性と入手への合意不介入（2026-10-10後続改定）

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を適用する。
**チェーンはモデルを入手できるかどうかに一切干渉しない。** 登録は自由、登録後のModel ID/root、
weights・tokenizer/config・実行仕様は永久不変とする。取得者は認証piece/shardと全量復元rootを照合する。
claimは同じ固定Model ID/root/specをbindし、root一致とminer計算の正当性を区別する。

MISAKA Torrent採用・専用SeederとSeeder報酬の概念を廃止する。一般的なP2P/mirror/共有は任意の
off-chain運用とし、合意によるSeeder登録・選出・取得監査・
PoR/lease/TRDCを撤回する。モデル非公開・全peer停止・第三者への提供拒否だけでclaim/reward/weightを止めず、
bondをslashしない。モデル取得成功を合意が保証するという旧AND契約も撤回する。
任意のinfohash/manifest bindingを登録する場合は固定rootとの対応を不変にし、必須の取得経路にしない。

### D10 — モデル別拘束miner元本とcoinbase配分（2026-10-10後続改定）

model別の適格拘束元本を`C_{b,m}`、`S_m=sum_b C_{b,m}`とし、同じbondのmodel割当合計は
実効拘束元本以下にする。claim予約件数・rho・root/slice/rider・鍵分割で元本を増幅しない。
model ownerの市場seed/Position/AMMやPanel/Seeder担保も自動算入しない。
`A_m=f(S_m)`、正の分母では`R_m=R_PALW*A_m/sum_j A_j`として既定の総coinbase予算内で配分する。
分母0はmodel配分0、整数残余と未使用枠を規範化する。資本預託だけでは報酬を払わず、有効claimとFinalを要求する。

公開参加が資本と配分倍率を大幅に増やし、閉鎖自己資本より圧倒的に有利になることを経済設計の目標にする。
同額`S_m`なら自己/外部資本の配分は同じであり、所有者の独立性をウォレット数から認定しない。
倍率曲線、競合・自己増資・Sybil/集中・閉鎖時の検出率と既存minerのcapを含め、利益比較で立証する。
既存minerの実収入増や公開が常に利益最大になることは未保証である。

モデル予算と個別bondのQ/B/R/F上限・共通DAA拘束を同時に適用する（ADR176）。
model配分変更だけでblock頻度、DAA、難易度、fork choiceを変えず、Final/courtと全writer/readerを別に実証する。

### D11 — FPR撤回と条件付きpublic prosecution（2026-10-10後続改定）

旧FPR三段階、モデル全量公開、配布default/重大Slash、代替Seeder、公開費予約、
availability起因CHALLENGED-weight停止を撤回する。モデル配布を合意のcourtへ持ち込まない。
claim固有の認証state/trace/output/witnessの適格要求・保持・客観的計算不正/defaultは維持するが、
weight-file/range、全量列挙や反復要求をモデル取得の強制へ転用しない（ADR177 D2）。
必要なoperandの認証と許可unit/累積scopeを実装前に確定する。

G14は、verifierが正しい固定modelを実際に得られた条件で公開証拠からlocalize/convictする能力を検証する。
**全model所有者が提供を拒否しても第三者が必ず取得できるという旧保証は撤回する。**
モデル未取得は計算不正・qualification停止・Slashの根拠ではない。実効検出`p=0`の場合も経済評価へ含める。
適格なclaim裁定が未解決ならそのFinal規則を適用するが、配布不能だけの停止へ読み替えない。
Panel=0、倍率式・経済優位・新会計/weight・court scopeは未実装/未立証であり、別activationを要する。

### D12 — Bondが時間内の採掘能力と総経済・consensus creditを制限する

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を、確率的検証・公開prosecutionと
組み合わせるPALWの追加必須前提とする。モデル計算はbondで制限された枠を使うための正当な仕事であり、
同額bond・同DAA期間・同倍率なら高速偽造でclaim発行能力、ブロック、報酬、Final後の確定weightの上限を増やさない。
claim容量を増やすときは一件の権利を細分化/配分し、bond当たりの総影響を維持する。
受理時予約、共通`d + W`までの早期回復禁止、全報酬/weight経路の帰属、残存責任の担保保持を要求する。

Final前のweight capだけでは成立せず、Final/retired/reversalと全settlement readerまで同じ予算を適用する。
不正反証で報酬失効と適用規則上の担保損失を執行するが、同じ共有担保をclaimごとに回収可能として重複計上しない。
実効検出・徴収率と計算費用を測定し、同じ上限だけで正直な計算の経済優位を証明したと扱わない。
この決定はproducer採掘予算に適用し、public challengerの別責任・費用を消さない。

## 4. RFC/ADR間の優先順位

1. 将来のPALW設計で、Panel-only evidence、honest-majority arithmetic authority、producer-private prosecution、kernel-only adjudicability certificate、無条件のPanel=0と本ADRが衝突した場合は、**本ADRとRFC-0014/0015の厳しいcompletion/activation条件を優先する**。
2. ADR-0172の承認済みversioned Kernel + 宣言的VerificationPlanを維持する。任意のverifier upload、model/fraud-proof VM、TEE validity trust、BFT orchestrator/PKI/committee authorityを代替依存として復活させない。ML-DSA/Hash64/UTXO、既存EVMやclock/fork-choiceの個別規則は、別の改定なしに変更しない。
3. ADR-0144の利用者の有用なローカル推論を対象にする目的は維持し、安全性の中核要件として本ADRを追加する。modelの回答の品質・source fidelity・実行の一致・不正の訴追可能性は別々に示す。
4. 過去のStatus、measurement、実装記録、fenceは履歴として保持する。該当文書の先頭bannerと末尾amendmentが、今後の設計における読み替えを指定する。「Implemented」と書かれた旧bodyだけから、本目標の達成を推論しない。
5. モデル入手の合意不介入とmodel拘束miner元本によるcoinbase配分はADR177を優先する。旧Seeder/Torrent/PoR/監査/lease/TRDC/FPR gateと配布起因weight停止を撤回し、固定登録identityとclaim固有courtのscopeを維持する。
6. public prosecutionのモデル取得前提はD11/ADR177 D7の条件付き保証へ改定する。全保有者拒否時の取得保証と閉鎖時の正の検出率は宣言しない。取得失敗だけで資格停止/SlashやFinal延長を起動しない。
7. D12とADR-0176を全将来PALW採掘経路の共通前提とする。class/work量・claim数の増加から未予約の報酬・block・Final weightを得る経路を改定し、既存実装や旧claimの記録を新保証の完成証拠としない。

## 5. 完成の証拠

RFC-0014のgatesを、少なくとも次の条件で満たすまで本目標を「完了」と呼ばない。

* fresh process・別bond・Panel非所属で始め、producerの内部オブジェクト、captureディレクトリ、既知fault位置を渡さず、公開したclass/claim/materialから不正の発見とlocalizationを行う。
* 正しいmodelを得たverifierで、Panel多数派および全席の偽`Valid`でlicenseしたclaim、served but falseな認証trace/state/FOLD、偽output、held/recurrent/checkpoint境界、fused tile、prompt/weights/identity mismatch、claim固有証拠の非開示を対象にする。正直なclaimにfalse convictionが出ないことも確認する。
* 実際の公開transport、bond admission、demand、court、terminal transition、producerの回収可能なslash、必要なseat責任まで確認する。liar自身の`ServedView`を外部取得の代用品にしたfixtureをこのgateの成功証拠にしない。
* bytes、work、rounds、deadline、保持・再取得、in-flight load、証拠包含、正しいnon-fault応答、duplicate、Final/reversal、reorg/IBD/pruning、activationを宣言した上限で確認する。
* 9B-8kや対応を主張するlong-contextなど、実際のcanonical最大profileで測る。証明可能性と回収可能額・監視インセンティブを区別し、不足担保を文書上のslash額で埋めたことにしない。
* RFC14 §16.8のnon-interference/元本/配分試験、§16.10のweight/規則E、§16.11の公開/閉鎖ECON・MEASを満たす。取得監査を資格gateへ戻さず、条件付き検出率と現実の計算/通信費を測る。
* ADR177の許可claim証拠unitと累積scopeを検証し、モデル取得要求を裁定から排除する。全peer停止でも同一chain/proofの合意結果が変わらず、正しいmodelを持つfresh verifierがclaimを裁定できることを確認する。
* ADR-0176の同額bond/同期間/同倍率での正直な計算対高速偽造、容量拡大時の総量保存、Final weight全経路、共通拘束、分割/再登録/期間境界、徴収可能損失と経済評価を満たす。

## 6. 確認したコードの位置づけ

確認基準は公開`main`の`43f0bcb362d37cba79414940f3fdd368d40d3377`。ローカルworkspaceのruntimeは別の旧HEADであり、この文書群の取り込みはruntime更新を意味しない。

| コード上の経路 | この目標に対する位置づけ |
| --- | --- |
| `palw_court_v2.rs`のcourt opening、`palw_offence_attribution_v1.rs`のExecutorRefuted | 客観的証拠を扱う土台。すべての公開profileで外部参加者がその証拠を作れる証明ではない。 |
| `palw_state_v2.rs`のconviction funnel、licensed/final claimのvoid/reversal、producerのslash、PanelFalseValid | license後の責任追及経路の土台。proof生成、期間、回収可能な額、署名scopeと合わせて評価する必要がある。 |
| `palw_operator_da` | Panel外の検査・DA accusationの土台。開示された自己整合的な偽traceの算術convictionまで、DA accusationだけで完了したとは扱わない。 |
| `palw_filer_held` / `palw_filer_held_e2e` | canonical held-8kのfresh verifierで、dense量、FOLD prefix、committed fused tileに残るgapを記録する。内部producerインスタンスによるserved materialの代用は完成証拠にならない。 |
| class/TIR admission、現行Panel basisとseat lock | 静的なcost/court検査や現行receipt条件である。新しいpublic-verifier completeness gateやPanel=0を文書だけで追加したことにはならない。 |

このADRは必要な実装の優先順位と受入条件を固定する。コード変更、試験によるgate充足、deploymentは別の作業である。
