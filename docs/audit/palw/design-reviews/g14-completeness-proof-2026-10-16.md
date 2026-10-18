# G14 完全性の検証 — 被覆と健全性の論証(2026-10-16)

- **対象:** `origin/integ/rfc02-rfc06-12-g14` @ `f244ed92d`(この文書の branch `integ/g14-proof` はそこから分岐し、文書だけを足す)。
- **方法:** source の読解だけで行った。cargo は使っていない(build・test・実ノードはいずれも未実行)。試験の名前と水準は source から読んだもので、**どの試験も PASS と書いていない。**
  行列の「実行」列は全て `unrun` である。実行は別の検証 agent と、その後の real-node drill が行う。
- **状態語:** ユーザーの 3 段階(implemented / verified / armable)を混ぜない。本書の COVERED は「source に (a)〜(h) の全部がある」という意味で、verified でも armable でもない。
- **防御的な文書である。** 不正の分類は「何が規則違反か」だけを定義し、攻撃の手順は書かない。暗号学的な計算証明は使わない(PRINCIPLES §8)。
  ここでの「証明」は、公開 material の同一性認証と局所的な決定論的裁定に基づく、被覆と健全性の論証である。

## 0. 結論

**検証した主張(ユーザーの言葉):**

> producer + Panel全員が共謀しても、active planがcoverする計算不正・job/input/output/state/DA違反については、Panel外の1つのpublic bonded verifierが、
> producerの秘密状態を使わず、公開・認証済みmaterialだけから客観的convictionまで進められる。つまり計算をしないでclaimを出すminerがいてもpanelに１つでも
> 正しいpanelがいて計算と一致しないことを確認できれば、客観的にそれを証明して slash・ブロック生成・報酬の拒否ができる状態に持っていくことができる。

**判定: f244ed92d の source では、DAA 14,000 の active plan について、この主張は成り立たない。** 理由は次の 4 つである(詳細は §4、§6)。

1. **DAA 14,000 で報酬を出す lane は、G14 を満たさない legacy Panel route だけである。** source に DAA 14,000 の武装一覧は無い。G14 のための fence は全 preset で `None` で、
   Gate C・kernel route・EXEC・native・legacy の公開 filer と held DA は、この binary では武装そのものが拒否される。live の V2 Attempt と Free Prompt は int-12 の規則のまま
   報酬と weight を出し続け、全 seat が共謀すると、外部の検証者が conviction まで届かない分類が残る(計算不正、FP の入力・状態・tokenizer・選択、期限、容量)。
   FP の選択・constraint・prefix・tokenizer の不正は、**正直な seat 1 人でも**裁けない(G-01、G-05、G-06)。
2. **対象外の work が報酬を得る。** FP Job V5 の画像・source id と PanelDa は Panel にしか渡らない material に依るのに、t12 で報酬を得る。除外の fence(N4)は武装できない(G-02)。
   Gate C を武装できないので、「未認証のまま無報酬」という防御も使えない(G-04)。
3. **武装できる G14 の部分を全部武装しても(P14-g14)、迂回できる。** TIR 公開 frame は TFP1 を付けた claim にしか付かない任意の仕組みで、共謀する producer は付けなければよい
   (G-03)。outside owner の 8 枠は先取りできる(G-T1)。staged public court は、武装できない legacy filer の予約が無いと seed も bundle も開けない(G-18)。
4. **主張の文言に最も近い実装(TIR 公開 frame)でも、証拠は fold 水準の未実行 source だけである。** node 水準の試験と drill は無い。選択の不正は node が局所化しない(G-T2)。

**成り立っている部分(source、未検証):**

- **TIR 公開 frame**(休眠、武装は可能)は、frame を持つ claim について、計算(F01)・出力(F04)・DA(F08)・借用(F09)・同一性(F10)・搬送(F11)・再編(F15)・退出(F19)・
  偽告発への耐性(F21)で、(a)〜(h) が source に揃う。抽選 seat でも外部 bond でも同じ object・同じ裁定になり、producer と全 Valid custodian の slash、claim の void、
  door が閉じるまでの Final 保留、Gate C の待機、凍結による attempt 拒否が source にある(§3.7)。
- **Kernel route / OPV と EXEC slice**(武装不可)は、node-E2E が最も厚い。ただし Valid 署名者を課金しない(G-K1、G-X1)。

**cell の数(7 lane × 24 分類 = 168 cell、§3.9):**

| 見方 | COVERED | PARTIAL | GAP | OUT-OF-SCOPE | n/a |
|---|---|---|---|---|---|
| **A. P14-min(文書の推奨どおり、新 G14 fence を武装しない)** | 4 | 25 | 16 | 120 | 3 |
| B. P14-g14(この binary で武装できる G14 fence を全部武装) | 13 | 34 | 34 | 74 | 13 |
| C. 参考: 全 lane の fence を武装したと仮定した source の状態 | 40 | 86 | 17 | 9 | 16 |

A の OUT-OF-SCOPE 120 は、休眠のため報酬も credit も生まない lane の cell である(I10 の確認は §1.3)。A で報酬を出す 2 lane(48 cell)のうち、COVERED は 4 だけである。

## 1. 範囲の定義

### 1.1 source 上の「DAA 14,000 の active plan」の実体

DAA 14,000 のリリース計画([t12-daa14000-release-plan-2026-10-15.md](../../../design/palw/integration/t12-daa14000-release-plan-2026-10-15.md))は、新しい fence を DAA 14,000 で有効化すると決めている。
ただし、**f244ed92d の source には DAA 14,000 で何を武装するかの一覧が無い。**

- `consensus/core/src/config/params.rs` に `14_000` の fence 値は無い(唯一の一致は genesis subsidy の定数 `:12401`)。`14,000` は comment にしか現れない(`consensus/core/src/lib.rs:1009` ほか)。
- testnet-12 の出荷 ruleset は `palw_t12_shipped_params`(`params.rs:10731`)。最後に武装した一覧は int-11/int-12 の DAA 5,300 一覧である
  (`PALW_T12_INT11_FENCES_V1` `params.rs:11551`、`PALW_T12_INT11_FLAG_DAY_DAA = Some(5_300)` `:11620`)。int-13 の 4 fence は `PALW_T12_INT13_DAA = None`(`:11726`)。
- G14 に関わる新 fence は全 preset で `None` である。多くは、この binary では**武装そのものが拒否される**(validation が `cannot be armed` を返す)。

| fence | 役割 | t12 | この binary で武装できるか |
|---|---|---|---|
| `palw_independently_work_certified_v1`(Gate C) | `SecurityCertified` と credit の剥奪 | `None` | **不可**: `PALW_RFC0018_ARMABLE_V1 = false`(`palw_rfc0018_v1.rs:546`、拒否 `:628`)。calibration の例外も無い |
| `palw_probabilistic_constraints_v1`(kernel route) | Kernel / OPV | `None` | **不可**(`palw_probabilistic_constraints_v1.rs:27-39`、隔離 calibration genesis だけ例外) |
| `palw_panel_free_v1` | OPV(Panel=0) | `None` | **不可**(`palw_panel_free_v1.rs:178-209`、gate 記録の pin は空 `palw_panel_free_gate_v1.rs:42`) |
| `palw_exec_payload_v2` | EXEC slice | `None` | **不可**(`PALW_EXEC_PAYLOAD_V2_ARMABLE = false` `palw_exec_v2.rs:553`、drill の例外も kernel route を要する `:652-658`) |
| `palw_funded_execution_v11` / `palw_native_final_v27` | native funded | `None` | **不可**(`palw_funded_execution_v11.rs:19-30`、`palw_native_final_fence_v27.rs:34-45`) |
| `palw_native_execution_v39` / `palw_native_mining_v39` | native v39 | `None` | 測定記録が空(`palw_native_execution_v39.rs:222`、`palw_native_mining_v39.rs:578`)。mining は Gate C を前提とする(`:529-563`)ので calibration でも不可 |
| `palw_legacy_public_filer_v1`(LG14-A) | legacy route の共通 filer・予約・Final 保留・直接証明 | `None` | **不可**(`palw_legacy_public_filer_v1.rs:287-315`) |
| `palw_legacy_held_da_v2`(LG14-B) | legacy route の公開 descent と held DA | `None` | **不可**(`palw_legacy_held_da_v2.rs:101-110`) |
| `palw_public_evidence_reward_v1`(N4)、`palw_claim_hard_deadline_v1`(X3)、`palw_court_model_scope_v1`(D2) | 2026-10-12 の裁定 | `None` | **不可**(`validation.rs:10-28`。規則の source は `palw_ruling_fences_v1.rs` にある) |
| `palw_fork_choice_rule_e_v1` | rule E(ADR-0178) | `None` | **不可**(`palw_fork_choice_rule_e_v1.rs:102`) |
| `palw_staged_public_court_v1` | staged public court | `None` | 可(ConsensusV2 だけを要求、`validation.rs:2879-2886`) |
| `palw_staged_tir_route_v1` / `palw_staged_float_constraint_v1` | staged route の拡張 | `None` | 可(staged court が同じ高さ以下、`validation.rs:2861-2877`) |
| `palw_tir_public_conviction_v1` | TIR 公開 frame(tag 170/171) | `None` | 可(前提 fence が揃えば有効、拒否規則なし、`params.rs:4988-5016`) |

リリース文書の推奨は、`palw_tir_public_conviction_v1` を休眠のまま載せること
([g14-objective-conviction-remaining §2](../../../design/palw/integration/g14-objective-conviction-remaining-2026-10-15.md)、[status.md](../../g14-review-2026-10-13/status.md) 末尾)、
RFC06〜08 の 7 fence も測定記録が空のため休眠のまま載せること([remaining-work §0.5](../../../design/palw/integration/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md))である。

### 1.2 本書が評価する二つの plan

- **P14-min(文書の推奨どおり)。** DAA 14,000 で新しい G14 fence を何も武装しない。DAA 14,000 以後も報酬を出すのは、int-12 で武装済みの legacy V2 Panel route の lane
  (V2 Attempt、Free Prompt、および同じ Panel route に乗る TIR v1・gen v1・shard 等)である。
- **P14-g14(この binary で武装できる G14 の全部)。** P14-min に加え、`palw_staged_public_court_v1`、`palw_staged_tir_route_v1`、`palw_staged_float_constraint_v1`、
  `palw_tir_public_conviction_v1` を DAA 14,000 で武装する。Gate C・kernel route・EXEC・native・legacy filer / held DA は、この binary では武装できないので、P14-g14 にも入らない。

**どちらの plan でも Gate C は効かない。** Gate C の述語は、fence が武装されていないとき全ての claim に `true` を返す(`palw_independently_work_certified_v1`、
`palw_rfc0018_v1.rs:147`)。このため、対象外の work を「Gate C で未認証のまま残す」防御は DAA 14,000 では使えない。対象外を無報酬にできるのは、受理時の拒否だけである。

### 1.3 対象外の確認(I10: 対象外の work は報酬も consensus credit も得ない)

ユーザーが挙げた対象外の各項目について、受理時に拒否されるか、報酬経路が存在しないかを source で確かめた。

| 対象外 | source で確かめたこと | 判定 |
|---|---|---|
| 画像・MTP の経路(genesis model は text-only class として扱う) | genesis の text class は画像 slot を持たないので、V5 job は構造的に拒否される(`palw_fp_job_version_offered_v1`、`palw_fp_job_v5.rs:303-309`・`:361-375`)。vision tower と MTP head は frontend が lower しないか名前で落とす([coverage-p1p2-record](../../../evidence/palw/implementation-records/coverage-p1p2-record.md)、[genesis8-status](../../../design/palw/integration/genesis8-status.md))ので、class program に含まれず claim できない(明示の拒否文は無い)。**しかし、画像 slot を持つ他の登録 class の FP Job V5 は t12 で DAA 5,300 から受理され報酬を得る**(`PALW_DRILL_FP_V5_ENTRY` `palw_fp_job_v5.rs:457` が `PALW_T12_INT11_FENCES_V1` に入る)。画像 bytes と source id は worker と Panel にしか渡らない(`palw_fp_job_v5.rs:15-17`)。除外の fence(N4)は武装できない。 | **不成立**(G-02) |
| seeded Float32 | Constraint job は staged route で全高さ拒否(`palw_staged_preclaim_v1.rs:155-161`、`palw_float_decode_v1.rs:227-228`)。ただしこの検査は staged court が武装されたときだけ受理時に走る。**Constraint 以外の seeded Float32 job を拒否する受理規則は見つからない。** native lane は greedy しか表現できない(`DecodeRuleV1::Greedy` のみ、`palw_flat_execution_v11.rs:959`)。 | **部分的** |
| frame 上の V3/V4 typed receipt | frame を持つ claim では名指しで拒否(`require_qualified_typed_receipts_v1`、`palw_tir_public_fold_v1.rs:1086-1093`、呼び出し `objects.rs:57-65`、`palw_state_v2.rs:18104`)。V2 の licence は exact な atomic credit を要する(`:1073-1084`)。 | 成立(frame 武装時) |
| Kimi | class 受理で拒否(`palw_class_admission_v2.rs:2662-2664`、`palw_kimi_k3` は全 preset で `None`)、attempt 規則でも拒否(`palw_attempt_rules_v1.rs:213-215`)。coverage 上の扱いが 2 か所で食い違う(G-16)。 | 成立 |
| fence 前に登録された class | complete snapshot capability でだけ拒否(`processor/palw.rs:2975-3004`、`palw_class_registration_origin_v1.rs:13-15`)。**claim の受理・credit を拒否する規則は無く、legacy route で報酬を得続ける。** 報酬 gate も legacy route を免除する(`Exempt("LEGACY_PANEL_ROUTE")`、`palw_opv_bootstrap_v1.rs:929-934`)。 | **不成立**(G-01 に含む) |
| 出どころの無い責任の項 | `palw_settlement_once_v1` 以後だけ名指しで拒否し、fence 未満は no-op(`refuse_unsourced_admission_v1`、`palw_settlement_liability_fold_v1.rs:456-463`、呼び出し `palw_bond_budget_fold_v1.rs:2503`・`:2565`・`:2632`・`:2735`、`palw_kernel_route_fold_v1.rs:961`・`:1004`・`:1082`)。該当する leg(native funded / text / job / mining、verifier check fee、material provider fee 等)は全て休眠なので、DAA 14,000 では報酬経路自体が無い。 | 成立(該当 leg が休眠のため) |
| 武装されない休眠 fence | 所有 fence が無効な kind の object は、受理 walk の前に落とされ、slot・rent・予算・fold を一切使わない(`palw_accept.rs:2522-2535`、`palw_lifecycle_objects_v2.rs:1128-1130`・`:1662-1668`・`:1784-1787`)。tag 170/171 は G14 fence の所有(`:1465-1466`・`:1623-1624`)。block は残り、carrier の通常 fee 以外は何も得ない。native の job Final は依頼者の escrow を動かすだけで、consensus credit は Gate C 認証済みの v39 mining receipt からしか来ない(`palw_native_mining_fold_v39.rs:531`・`:630`、commit は Gate C が無ければ拒否 `palw_security_certification_fold_v1.rs:93-110`)。 | 成立 |

### 1.4 論証の形 — 不変条件

主張は「lane L で不正 F が起きたとき、Panel 外の 1 人の公開 bonded verifier が客観的 conviction まで進め、制裁と拒否が実際に効く」である。
本書はこれを、各 (L, F) について次の不変条件 I1〜I10 が source に揃っているかに分解する。全部が揃い、その試験が通れば、(L, F) について主張が成り立つ。
1 つでも欠ければ、その cell は COVERED にしない。

| ID | 不変条件 | 行列の列 |
|---|---|---|
| I1 単独性 | 責任を負わない Active な bond 1 つが、他の bond の署名・投票・協力なしに告発できる。seat 役割、genesis bond、多数決、Sybil と共有する予算を要しない。 | (a) |
| I2 公開性 | 告発・裁定の入力は、claim の root と登録 commitment に対して認証できる公開 material と、検証者自身の登録 model のコピーだけ。producer の秘密 capture・秘密 checkpoint・ServedView を使わない。 | (b) |
| I3 有界な局所化 | 不一致は、宣言済みの bytes・work・round の上限の中で、有界な terminal(1 leaf、1 cone、1 token、完全な id 列)まで局所化される。上限で切り詰めない。 | (c) |
| I4 決定論的裁定 | 裁定は state と証拠の純関数。述語が成り立たないか error なら state は変わらない(無罪の証明にも有罪にもしない)。応答が無いときは計算不正を推定せず DA default とする。 | (d) |
| I5 制裁の到達 | conviction は claim を void し、producer と、その claim に Valid を署名して責任を負う bond を slash する。担保が退出していないこと(exit hold)を受理時と終局時に保証し、徴収額を記録する。 | (e) |
| I6 信用の剥奪 | void された claim は、Final weight、safe weight、frontier、settlement 深さ、rule E 参加、escrow、model 配分、coinbase leg、receipt spend、producer のブロック生成資格のいずれにも寄与しない。既に加えた分は取り消す。 | (f) |
| I7 支払い前の裁定可能性 | 裁定の door / window が閉じる前に、報酬・credit・担保の解放が起きない。後からの clawback を前提にしない。 | (f) |
| I8 一回性と再編一貫性 | 同じ違反の徴収は 1 回だけ。reorg・restart・IBD・pruned import の後も同じ裁定・同じ state root。 | (e)(f) |
| I9 飢餓耐性 | 正直な検証者は、容量上限・lane・手数料・期限のいずれによっても、共謀者に先取りされて締め出されない。 | (a) |
| I10 対象外の無報酬 | active plan の対象外の work は、受理時に拒否されるか Gate C で未認証のまま残り、報酬も consensus credit も得ない。 | §1.3 |

**この論証が保証しないこと(前提として明記する):**

- **検出確率。** 誰も検査しない claim は通る。G14 は、不一致が見つかった後の追及が Panel の多数決に戻らないことを保証する。検出の確率は保証しない(PRINCIPLES §0-4)。
- **model の入手。** 検証者は登録 model を自分で入手していることが前提である(ADR-0177)。誰にも共有されない model の不正は、bond 単位の上限で有界化する残余リスクである(PRINCIPLES §0-3)。
- **「計算しなかった」こと自体。** 計算を省いても、公開された全ての値が正しければ、この経路では違反として立証できない。ユーザーの例が言うとおり、対象は「計算と一致しないことを確認できた」場合である。
- **Gate C の認証の前提。** `SecurityCertified` は抽選検証者の正直な割合(`k/m`)を前提とする認証であり(RFC-0018 §7.2)、G14 とは別の層である。
  G14 の conviction は、認証済みの claim でも void して credit を取り消す(I6、RFC-0018 I-C3)。

## 2. 不正の分類(網羅を目指した一覧)

各分類は「何が規則に反するか」だけを定義する。F01〜F19 はユーザー指定、F20〜F24 は本書が追加した。

| ID | 分類 | 定義(規則違反として何が成立するか) |
|---|---|---|
| F01 | 計算の省略・偽造 | claim がコミットした中間値・最終値(step leaf、logits tile、cone の出力、kernel の行)が、登録 program・登録 model・コミット済み入力からの決定論的再計算と一致しない。 |
| F02 | 誤った job | 支払い・割当てを受けた job(job id、decode 設定、budget、class)と異なる job の結果を、その job の claim として出す。 |
| F03 | 誤った入力・prompt | コミットした prompt ids・入力 binding・prompt anchor が、job の公開入力と一致しない。 |
| F04 | 誤った出力 | 公開した output ids / output root が、コミット済み実行から導かれる出力と一致しない(実行が正しく出力だけ差し替える場合を含む)。 |
| F05 | 誤った状態 | prefix state、history、KV、memory / retrieval の typed root の宣言が、公開 material からの再導出と一致しない。 |
| F06 | tokenizer / detokenizer | 登録 tokenizer による encode / decode と、claim の id 列・文字列の対応が一致しない。 |
| F07 | 選択・sampling | 正しい logits に対し、job 自身の選択規則(greedy argmax、seeded Decision 11 sampling、V4 の penalty / bias / ban / stop)と異なる token を選ぶ、または停止規則に違反する。 |
| F08 | DA 違反 | 応答義務のある material を出さない、一部しか出さない、認証できない bytes(garbage)を出す。 |
| F09 | 借用・再利用 trace | 別 claim・別 job の trace / commitment を流用し、当該 job の計算と結び付かない claim を出す。 |
| F10 | claim / job 同一性の差し替え | 署名済みの claim・job・class・artifact・allocation の同一性を、受理後または搬送中に別物へ置き換える。 |
| F11 | carrier / envelope 操作 | TFP1・HFP1・control・grant などの搬送 object の改竄、重複、再送、pairing 違反、復号時の過大割当て。 |
| F12 | 時間・期限の駆け引き | 遅延応答、期限直前の提出、追及が終わる前に Final・void・retirement・lock 失効へ到達させる、期限の延長の悪用。 |
| F13 | 容量占有・Sybil | owner 枠、DA session 枠、court lane、inclusion 容量、共有の非 seat 予算を共謀者で埋め、正直な検証者を締め出す。 |
| F14 | grinding | beacon・抽選・seed・監査標本を試行して、監査回避・seat 獲得・認証の通過を得る。 |
| F15 | reorg / fork | private branch で Final を作る、conviction を reorg で外す、fork ごとに異なる判定へ導く。 |
| F16 | producer + Panel 全員の共謀 | 全 seat が Valid に署名する(本書の基本前提)。この行は「その前提の下で他の行が成り立つか」を lane 全体として評価する。 |
| F17 | 怠惰・共謀する検証者 | 再実行せずに承認する checker・抽選 attestor(T6 の lazy verifier、Gate C の m-of-k 承認者)。 |
| F18 | 自己告発・報奨金の循環 | 自分の Sybil で告発して報奨を回収する、正直な告発者を先取りして報奨を奪う。 |
| F19 | bond の分割・退出・再登録 | 制裁の前に担保を引き出す、bond を分ける、別 bond で再登録して制裁と凍結を逃れる。 |
| F20(追加) | model / artifact 同一性 | 登録 root と異なる重みで実行する、誰も入手できない閉鎖 model で claim する。 |
| F21(追加) | 偽告発・griefing | 正直な producer を不当に告発する、告発を乱発して正直な claim の Final を遅らせる、正直な告発者の exposure を焼く。 |
| F22(追加) | 担保不足・集約責任の超過 | 同じ bond で担保を超える同時責任を負い、slash の上限(担保額への clamp)で制裁を薄める。 |
| F23(追加) | 対象外 material の持ち込み | 対象外の profile(画像入力、MTP、seeded Float32、typed receipt、Kimi など)を、対象内の lane の claim として報酬・credit に乗せる。 |
| F24(追加) | 非決定性の主張 | 浮動小数・量子化・実装差を理由に、判定を不能または曖昧にする。 |

## 3. 被覆行列(不正分類 × lane)

### 3.0 読み方

- 列 (a)〜(h) はユーザー指定のとおり。(a) 検出者、(b) 公開・認証済み material、(c) 局所化、(d) court と客観的裁定、(e) 制裁、(f) ブロック生成・報酬・weight の拒否、
  (g) code の位置、(h) 試験と水準。lane ごとに共通する (a)(b)(c)(d)(e)(f) は lane の冒頭に一度書き、各行には差分だけを書く。
- (i) 状態: **COVERED**(source に (a)〜(h) が全部ある)、**PARTIAL**(一部が欠ける)、**GAP**(1 人の正直な検証者からの conviction 経路が無い)、
  **OUT-OF-SCOPE**(受理時に拒否されるか、休眠で報酬・credit を生まない。§1.3 で確認)、**n/a**(その lane では構造上その不正が成立しない、または別 lane の cell に従う)。
- (i) は P14-min を基準にする。休眠の lane は、参考として「武装時」の source 状態も書く。
- 試験の水準: **unit**(純関数・kernel ref・SDK)、**fold**(state transition または processor fold。mempool・template を通らない)、**node**(署名済み carrier → mempool →
  node 自身の template → chain-block fold → ConsensusApi read)、**N\***(node だが、harness が休眠 fence を validation を通さずに武装したもの)。
- **実行: 全 cell `unrun`。**
- 行番号は f244ed92d の値。path は repository root からの相対 path(`processor/palw.rs` は `consensus/src/pipeline/virtual_processor/processor/palw.rs`)。

### 3.1 legacy V2 Panel route の共通参照(R#)

legacy の 2 lane と staged court が共有する参照。

| R | 内容 | code |
|---|---|---|
| R1 | court 中の先取り(`ClaimUnderSession`): fence 未満では kind 3/4/7 は court が開いている間拒否される。修正(`DirectProofFirst`)は武装できない fence の下 | `palw_offence_attribution_v1.rs:446-471`・`:2408`・`:2637`・`:2861`、`transition_builder.rs:5117-5122`、`processor/palw.rs:5932-5934` |
| R2 | DA-8 の非 seat 予算は claim ごとに同時 3・生涯 16 で、Sybil と共有 | `palw_da_rcore_v1.rs:34-40`、`fold_read.rs:188`・`:203-209` |
| R3 | Final を止めるのは seat の DA session(と休眠の予約)だけ。非 seat の session は止めない(V3S-08) | `palw_rcore_deadline_v1`(`palw_state_v2.rs:3934-3958`) |
| R4 | 予約(tag 154)は武装できない `palw_legacy_public_filer_v1` を要する | `palw_legacy_public_filer_fold_v1.rs:432-434` |
| R5 | FP の直接証明(contradiction 14〜18、`FpCanonicalWhole`)は同じ fence の下 | `palw_offence_attribution_v1.rs:1784-1810` |
| R6 | FP の選択証明(`ForgedOutput` / `ForgedOutputTiled`)は FP claim で拒否(fence 後の語彙外 id だけ例外)。decode-token close も DAA 5,300 以後の FP claim を裁かない | `palw_offence_attribution_v1.rs:2088-2100`・`:1566-1573`、`palw_decode_close_admits_claim_v1`(`palw_court_v2.rs:1281-1291`) |
| R7 | producer の conviction 経路(live なら void、Final 後は S3)。Valid 署名者は別の kind-3 filing で課金 | `transition_builder.rs:3565-3620`、`kaspad/src/palw_filer_false_valid.rs:704-716` |
| R8 | 署名者の lock は committed の間だけ slash できる(Final + 1,000 まで)。Final の反転は Final + `window_court`(3,000)まで可能 | `palw_state_v2.rs:2747-2776` |
| R9 | slash は担保額で黙って clamp(`debit == 0 → Ok(0)`) | `slash_bond`(`transition_builder.rs:9004-9015`) |
| R10 | FP の receipt spend は Final で coinbase に払う(`paid_now: carve`)。Final floor は licence + `window_challenge`(1,200)で、延ばすのは TIR frame だけ | `palw_state_v2.rs:19331-19348`・`:19388`、`:3626-3650` |
| R11 | 報酬 gate は legacy route を免除 | `palw_opv_bootstrap_v1.rs:929-934` |
| R12 | live で動く外部 filer は genesis operator の bond だけで、row 0 だけを告発する。自己整合した garbage trace は row 0 に答えられ、告発者の exposure を焼く | `kaspad/src/palw_operator_da.rs:14-36`・`:180-195` |
| R13 | node の fraud filer は fence 未満では動かない | `kaspad/src/palw_fraud_filer.rs:2650` |
| R14 | network 全体の court session 上限 | `palw_state_v2.rs:8678-8680`、`da_sessions.rs:752-760`、`objects.rs:2002-2011` |
| R15 | fused の `StepLeaf` は DA unit として拒否(`DaUnitNeedsDissection`) | `palw_da_rcore_v1.rs:1015` |
| R16 | 公開 FP job は filer fence の後か staged 契約の下でしか保持されない。fence 未満では Final の FP claim は DA 告発できない | `palw_legacy_public_filer_fold_v1.rs:57-73`・`:398-420` |
| R17 | 退出 gate | `palw_state_v2.rs:3312-3347` |
| R18 | SR-1: 全 seat が Valid を出すと、licence 時に producer の escrow 項を解放 | `palw_state_v2.rs:3473-3510` |

N\* の注記: legacy と staged の node 試験の多くは、harness が休眠 fence を直接武装する(`lg14a_legacy_filer_e2e.rs:103`、`kaspad/src/palw_service_public_e2e.rs:175`)。
武装できない fence の下の試験は、DAA 14,000 の active plan の証拠にならない。

### 3.2 lane: V2 Attempt(legacy、t12 で稼働中)

**DAA 14,000: 稼働中で報酬・weight を出す(P14-min・P14-g14 とも)。** 報酬 gate は免除(R11)。G14 の外部経路を作る 2 fence は武装できない。

**共通部品(P14):**
- (a) 正直な seat は served capture から(ServedView 依存)。外部の者は consensus 上は R2 の session と kind 3/4 を使えるが、node の filer が無く(R13)、Final を止められず(R3)、
  live で動く外部 filer は genesis operator の row 0 だけ(R12)。
- (b) claim の root、公開 binding。fused leaf は DA unit として出せない(R15)。
- (c)(d) one-move court `ShardCourtAccused`(producer 以外の Active・floor 以上の bond が出せる。terminal phase の claim は拒否、`objects.rs:637-700`)、held dissection、
  kind 4 `ExecutorRefuted`、kind 3 `PanelFalseValidV2`。
- (e) R7 と kind 3、DA-7 の S4。R8・R9 の制限。
- (f) Final までの escrow、vesting row、receipt の root 没収、`reverse_convicted_final` による weight の取消(`transition_builder.rs:5596-5641`)。

| F | (i) P14 | 要点(差分) | (g) code | (h) 試験(水準) | 実行 |
|---|---|---|---|---|---|
| F01 | **GAP** | 外部の局所化は生涯 16 session を超え、fused leaf は出せない。node filer も無い | R2・R3・R13・R15 | `t46b_injected_step_fault_convicts_before_final`(`t46_false_valid_real_claim.rs:1509`、fold)、`t46e_after_final`(`:1610`、fold)。gap を固定: `t54g_gap_a/b/c`(`kaspad/src/palw_filer_held_e2e.rs:1382`・`:1323`・`:1358`)。休眠 fence 下: `lg14a_a_non_seat_newcomer_convicts_a_computation_lie_before_final`(`lg14a_legacy_filer_e2e.rs:812`、N\*)、`lg14b_a_lie_in_one_fused_tile_is_localized_and_convicted_by_a_fresh_outsider`(`kaspad/src/palw_legacy_held_e2e.rs:1116`、N\*) | unrun |
| F02 | PARTIAL | `IdentityMismatch` は fence なしで受理されるが、binding は R2 の session でしか得られない | `palw_offence_attribution_v1.rs:1170-1200` | `t18b_a_borrowed_root_answers_another_job`(`t46_false_valid_real_claim.rs:3537`、fold)、`t18z_a_job_fault_convicts_every_valid_signer`(`:5093`、fold) | unrun |
| F03 | PARTIAL | `PromptNotAnchored` の Tile 証明は attempt 用で fence なし | `palw_offence_attribution_v1.rs:1822-1828` | `t46o_a_prompt_gather_fault_convicts_in_the_networks_carriage`(`:2684`、fold) | unrun |
| F04 | PARTIAL | proof 8/10/11 は attempt で受理 | R6 | `t46c_forged_output_full_mask_only`(`:1570`、fold) | unrun |
| F05 | PARTIAL | activation leg・checkpoint の嘘は DA default にしか届かない。fused 8k/2M history は GAP | R15 | t54g の gap pin | unrun |
| F06 | n/a | attempt の id は anchor から導出し、この lane に tokenizer は無い | — | — | — |
| F07 | PARTIAL | greedy の proof 8/10/11 | R6 | `t46u_a_logits_fault_convicts_the_full_seat_only`(`:5070`、fold) | unrun |
| F08 | PARTIAL | preimage の無い junk は default。garbage trace は row 0 に答えられる(R12)。court default は seat を有罪にしない | R12 | `t18c_ii_a_root_with_no_preimage_defaults`(`:3890`)、`t46r_a_producer_withholding_void_convicts_no_seat`(`:3016`)、`t46s_a_court_default_convicts_no_seat`(`:3178`)(fold) | unrun |
| F09 | COVERED | block をまたぐ replay は mint 時に拒否 | `palw_attempt_v2.rs:14-31` | `audit_repro_03_regression_one_execution_is_one_claim_across_chain_blocks`(`consensus/core/tests/audit_repro_03_attempt_lane_cross_block_execution_repla.rs:573`、unit) | unrun |
| F10 | PARTIAL | F02 と同じ搬送 | 同上 | 同上 | unrun |
| F11 | COVERED | 受理時に署名と同一性を検査 | `palw_attempt_v2.rs:27-31` | `h1_the_gate_passes_what_the_fold_takes…`(`t12_h1_carrier_gate.rs:116`、node) | unrun |
| F12 | **GAP** | 非 seat の session は Final を止めない。court による先取り(R1) | R1・R3 | `lg14a_unarmed_the_reservation_is_dropped_by_name_and_the_bystander_is_outrun`(`lg14a_legacy_filer_e2e.rs:1123`、node、**gap を固定する試験**) | unrun |
| F13 | **GAP** | DA-8 予算と court 上限を Sybil が使い切れる | R2・R14 | 修正は休眠 fence の下だけ: `lg14a_coalition_reservations_cannot_exclude_a_new_bond`(`consensus/core/tests/lg14a_legacy_dispute_fold.rs:232`、fold)、`lg14a_a_sybil_reservation_and_sessions_pre_empt_nothing`(`lg14a_legacy_filer_e2e.rs:980`、N\*) | unrun |
| F14 | PARTIAL | panel seed の fence は DAA 750 で武装済み | `PALW_T12_POST_LAUNCH_FENCES_V1`(`params.rs:10873`) | `audit_repro_03_one_winning_draw_admits_every_sibling_in_its_bucket`(`:657`、unit) | unrun |
| F15 | PARTIAL | strict economic win(750)、rule E は武装不可(G-09) | `palw_fork_choice_rule_e_v1.rs:102` | `t46m_reorg_and_restart`(`:2448`、fold) | unrun |
| F16 | **GAP** | 全 seat 共謀で、外部からの完結経路が無い(F01・F12・F13 の合成) | R11・R12・R18 | — | — |
| F17 | **GAP** | この lane に trap は無い(T6 は kernel route 用で休眠)。署名者の責任は Final + 1,000 で切れる | R8 | — | — |
| F18 | PARTIAL | producer は予約・告発できないが、その Sybil は報奨(r = 10 %)を先取りできる。自己告発の純損益は負 | `fold_read.rs:162`、`palw_reporter_reward_amount_v1`(`palw_state_v2.rs:1236`) | `r_7_a_self_conviction_nets_negative_over_a_grid`(`palw_state_v2/tests.rs:30139`、unit) | unrun |
| F19 | PARTIAL | 署名者は Final + 1,000 以後に退出できるが、Final はまだ反転し得る | R8・R9・R17 | — | — |
| F20 | PARTIAL | class の artifact root binding と seat の保有(N1)。producer の保有を明示に検査する規則は無い(spec register N1/N2) | `palw_class_seating_v1` | — | — |
| F21 | PARTIAL | 正直な claim は one-move verdict で守られる。正直な外部告発者は garbage trace で exposure を焼かれ得る(R12) | R12 | `t46s`(fold) | unrun |
| F22 | PARTIAL | 集約責任(ρ は DAA 5,300〜5,585 で 1,000 まで)と R9 の clamp、R18、bond 式 v1(G-12) | R9・R18 | — | — |
| F23 | PARTIAL | Kimi は拒否。画像生成 class(gen v1、DAA 5,300 から有効)の attempt を除く規則は確認できない | `palw_attempt_rules_v1.rs:213-215` | — | — |
| F24 | n/a | 整数の floor op | — | — | — |

### 3.3 lane: Free Prompt(legacy、t12 で稼働中。FP V4 / V5、constraint、prefix、tokenizer の job は DAA 5,300 から受理)

**DAA 14,000: 稼働中で報酬・weight を出す。** live では FP job は配信されるだけで chain に保持されず(R16)、operator filer は FP claim を replay しない
(`kaspad/src/palw_operator_da.rs:191-195`)。報酬は Final で receipt spend を通じて払われる(R10)。共通部品は §3.2 と同じ。

| F | (i) P14 | 要点(差分) | (g) code | (h) 試験(水準) | 実行 |
|---|---|---|---|---|---|
| F01 | **GAP** | 外部の者は material を得られない。正直な seat は served capture から | R12・R16 | `t46g_fp_claim`(`t46_false_valid_real_claim.rs:1845`、fold)。休眠 fence 下: `lg14b_public_fp_bootstrap_own_replay_localizes_and_convicts_after_colluding_valid`(`kaspad/src/palw_legacy_held_e2e.rs:1249`、N\*) | unrun |
| F02 | PARTIAL | FP pin の同一性検査は live。Final 前だけ | `palw_offence_attribution_v1.rs:1176-1187` | `fresh_public_bond_service_convicts_a_public_foreign_job_and_reveals`(`kaspad/src/palw_service_public_e2e.rs:593`、N\*) | unrun |
| F03 | **GAP** | `FpCanonicalWhole` は fence の下(R5)。Tile 証明は attempt 用 | R5 | `palw_service_public_e2e.rs:613`・`:618`・`:623`(N\*) | unrun |
| F04 | PARTIAL | output root の同一性だけ(`OutputMismatch`、`palw_offence_attribution_v1.rs:1326-1364`) | R6 | — | — |
| F05 | **GAP** | `PrefixStateMismatch`(17)は fence の下だが、prefix job は DAA 5,300 から受理 | R5 | `palw_service_public_e2e.rs:698`・`:702`(N\*) | unrun |
| F06 | **GAP** | contradiction 16/18 は fence の下 | R5 | `kaspad/src/palw_filer_tokenizer_tests.rs`(unit) | unrun |
| F07 | **GAP** | greedy・seeded・V4 penalty・stop の選択は、**正直な seat を含め誰も裁けない**(R6)。validation の注記も「seat quorum で止める」とする(`validation.rs:1292-1298`) | R5・R6 | constraint の場合だけ: `palw_service_public_e2e.rs:663-693`(N\*) | unrun |
| F08 | PARTIAL | Final の FP claim は DA 告発できない(R16) | R16 | `palw_service_public_e2e.rs:633`・`:653`・`:658`(N\*) | unrun |
| F09 | COVERED | receipt spend で execution root ごと没収 | `palw_state_v2.rs:19346-19348` | `dos_repro_1_a_sibling_of_the_convicted_execution_spends_nothing`(`consensus/core/tests/dos_repro_1_fp_final_receipt_rights_unpriced_unforfe.rs:630`、unit) | unrun |
| F10 | PARTIAL | F02 と同じ | — | — | — |
| F11 | COVERED | 受理時検査 | — | `t12_h1_carrier_gate.rs`(node) | unrun |
| F12 | **GAP** | coinbase は licence + 1,200 から払われ、責任は 3,000 まで続く | R10・R3 | `dos_repro_1_fp_final_receipt_rights_unpriced_unforfeitable`(同 file `:279`、unit) | unrun |
| F13 | **GAP** | R2・R14 | R2・R14 | — | — |
| F14 | PARTIAL | k block の beacon fold | `palw_fp_beacon_v3.rs:14-22` | — | — |
| F15 | PARTIAL | §3.2 と同じ | — | — | — |
| F16 | **GAP** | `dos_repro_1` は t12 の経済で「支払い / Valid quorum の slash 可能額 > 150 倍」を assert する | 同 file `:612-618` | 同(unit) | unrun |
| F17 | **GAP** | §3.2 と同じ | R8 | — | — |
| F18 | PARTIAL | §3.2 と同じ | — | — | — |
| F19 | PARTIAL | §3.2 と同じ | — | — | — |
| F20 | PARTIAL | §3.2 と同じ | — | — | — |
| F21 | PARTIAL | §3.2 と同じ | — | — | — |
| F22 | **GAP** | F16 の 150 倍(支払いが徴収可能額を大きく超える) | 同上 | 同上 | unrun |
| F23 | **GAP** | FP Job V5 の画像・source id と PanelDa は Panel にしか渡らない(`palw_fp_job_v5.rs:15-17`)のに報酬を得る。除外 fence(N4)は武装不可 | `palw_ruling_fences_v1.rs:16-21`、`validation.rs:18` | — | — |
| F24 | n/a | 整数 | — | — | — |

### 3.4 lane: Kernel route / OPV

**DAA 14,000: OUT-OF-SCOPE(24 cell 全部)。** `palw_probabilistic_constraints_v1` は武装拒否(§1.1)。fence 未満で kernel object は A-2 の規則で判定されずに落ちる(§1.3)。
node-E2E の harness は validation を通さずに fence を直接武装している(`g14_kernel_route_e2e.rs:9-12`)。

以下は「武装したら」の source 評価。この lane は G14 の node-E2E が最も厚い。

**共通部品(kernel crate `misaka/palw/kernel/src/ledger.rs` ほか):**
- (a) 縮尺した dismissal fee 以上の空き担保を持つ任意の登録 bond(`ledger.rs:2608-2651`)、demand は任意の bond(`:2989-2993`)。genesis 後に登録した bond で試験済み
  (`canonical.rs:775-819`)。注意: 検証者は登録 model を保有する必要がある(`c4r4.rs:707`)。block の court 予算は Sybil と共有(`ledger.rs:179-181`、F-C4R4-05 の縮尺 fee と
  OPV の検閲費用関係 `opv.rs:191-197` で緩和)。
- (b) ledger state から作る公開記録(`ledger.rs:2283-2296`)、提供済み position は state に保持(`:837-838`)。秘密 state への依存は無い。ただし多くの node 試験は、
  artifact attestation と OPV 適格に `cfg(test)` の hook を使う(`processor.rs:234-263`)。本番の attestation 一覧は空(`palw_kernel_route_v1.rs:161-164`)。
  本番の onboarding 経路を通る試験は `opv_bootstrap.rs` と kaspad の `g14k_*` だけ。
- (c) `FreshVerifierV1::try_proof`(`ledger.rs:2575-2578`)。demand は 1 position。
- (d) `adjudicate`(`ledger.rs:2571-2594`)。
- (e) `convict`(`ledger.rs:2764-2860`)が producer の予約を slash(現担保で clamp `:2780`)、告発者は最大 49 %(`:2818`)、残りは burn。
  **seat / 署名者の清算種別が無い**(`settle.rs:43-93`)。kaspad の試験は「共謀 seat は課金されない」ことを assert する(`kaspad/src/palw_service_kernel_e2e.rs:760-768`)。
- (f) Final 前の conviction は報酬 0(`ledger.rs:3289-3295`)。Final 後は、報酬が Final の時点で coinbase queue に入っていて回収されない(`palw_kernel_route_fold_v1.rs:1200-1218`、
  collateral > reward の関係 `ledger.rs:224-231` だけが裏付け)。kernel claim は weight を持たない(`palw_kernel_settlement_v1.rs:22-24`)。

| F | (i) 武装時 | 固有の code | (h) 試験(水準) | 実行 |
|---|---|---|---|---|
| F01 | PARTIAL(OPV: 揃う。Panel 許可 mode: seat slash なし) | 共通 | `g14_kernel_route_a_covered_lie_is_convicted_by_an_outsider_through_the_real_path`(`g14_kernel_route_e2e.rs:857`、node)、`:906`、`canonical.rs:1269-1294`、`g14_c4r3_mandatory_3_a_relayed_lie_covered_by_every_seat_is_convicted_by_an_outsider`(`c4r3.rs:111`、node)、`palw_service_kernel_e2e.rs:676`(node、実 service) | unrun |
| F02 | COVERED(包含時に拒否) | `binding_fault_v1`(`ledger.rs:2260`) | `canonical.rs:1309`・`:1314`(node) | unrun |
| F03 | COVERED(v1/v2。v4 の prompt tile は GAP-30 で報酬対象外) | 包含時に拒否 | `canonical.rs:1319`・`:1324`(node) | unrun |
| F04 | COVERED | decode court(`ledger.rs:2580-2585`) | `canonical.rs:1279`・`:1284`(node) | unrun |
| F05 | PARTIAL(v1/v2 history は carrier fit で拒否、Memory / Composite の適格は seam 経由、Retrieval は適格にならない `palw_opv_bootstrap_v1.rs:729`・`:754`) | spec court | `r4x_typed_roots_e2e.rs:595`・`:662`・`:709`・`:771`(node) | unrun |
| F06 | OUT-OF-SCOPE(job は token id、class は tokenizer root を束縛しない、`claim_subject.rs:41`) | — | — | — |
| F07 | COVERED(greedy。seeded・penalty・stop は表現できない `job.rs:30-33`・`:58-60`) | — | decode court の試験 | unrun |
| F08 | COVERED(単一 program。v4 は PARTIAL) | default `ledger.rs:3180-3240`、default は不正を消さない `:3214-3223`・`:2687-2697` | `g14_kernel_route_e2e.rs:950`・`:1003`・`:2440`、`c4r3.rs:231`・`:254`(node) | unrun |
| F09 | COVERED | job ごと 1 claim(`ledger.rs:840-843`)、seal → reveal(`:2272`) | `canonical.rs:1309-1324`(node) | unrun |
| F10 | COVERED | 行為者 = 署名者(`ledger.rs:1546-1580`) | `k2_opv.rs:58`(unit、kernel ref) | unrun |
| F11 | PARTIAL | 署名 message(`palw_kernel_route_v1.rs:140-150`) | `c4r3.rs:438`(node)。**ignored の FAIL**: `conformance/c4r4.rs:160`(A-2、tag 113) | unrun |
| F12 | PARTIAL(関係は強制、値は暫定・未測定 GAP-07) | `opv.rs:199-250` | `g14_kernel_route_e2e.rs:1164`・`:1207`・`:2472`(node) | unrun |
| F13 | COVERED(F-C4R3-02 / -03 / -05) | `opv.rs:581-605`、`palw_kernel_route_v1.rs:98-109` | `c4r3.rs:396`・`:490`(node)。F-C4R4-05 は kernel ref のみ | unrun |
| F14 | PARTIAL(GAP-B12 は未解決、暫定 seat は grind できる `palw_kernel_route_fold_v1.rs:31-34`) | sealed beacon v3 | `opv_bootstrap.rs:1227`・`:1307`(node) | unrun |
| F15 | COVERED(kernel の Final は weight を持たない、journal) | — | `recovery.rs:290-560`(node) | unrun |
| F16 | PARTIAL(conviction は届くが seat は無傷、G-K1) | `ledger.rs:52-55` | 全 node 試験で全 seat が署名 | unrun |
| F17 | **GAP**(誘因。`palw_verifier_pay_v1` は休眠) | — | `econ_bounty.rs:218`(観測) | unrun |
| F18 | **GAP**(P0 設計: 抑止は価格付け済みだが、正直な finder は無報酬) | `opv.rs:159-170` | `rfc15_panel_free.rs:264`、`rfc02t_sealed_bounty.rs:51`(node) | unrun |
| F19 | COVERED | 予約中は withdraw 不可(`ledger.rs:1477-1496`)、V2 の退出も kernel 予約で止まる(`palw_state_v2.rs:2376-2378`) | `rfc15_panel_free.rs:300`(node)、`k2_opv.rs:739` | unrun |
| F20 | PARTIAL(本番の attestation 一覧は空、試験は seam) | `palw_kernel_route_v1.rs:161-164` | `model_artifact.rs`(node) | unrun |
| F21 | COVERED(dismissal fee、honest twin) | — | `canonical.rs` の honest twin(node) | unrun |
| F22 | PARTIAL(slash は現担保で clamp、閉鎖時の不足 debit を許す `palw_kernel_route_fold_v1.rs:1117-1124`) | — | — | — |
| F23 | OUT-OF-SCOPE(pipeline `PipelineNotOnboardable` `palw_opv_bootstrap_v1.rs:795`、2M `BoundsExceeded` `check.rs:91`、private material `gate.rs:111`) | — | — | — |
| F24 | n/a(整数 kernel) | — | — | — |

### 3.5 lane: native funded(v11 / v29 / v39)

**DAA 14,000: OUT-OF-SCOPE(24 cell 全部)。** 武装拒否と測定記録 gate は §1.1。native の job Final は依頼者の escrow を動かすだけで、consensus credit は Gate C 認証済みの
v39 mining receipt からしか来ない(§1.3)。

以下は「武装したら」の source 評価。Panel は無い(OPV が前提)ので、G14 の行為者は producer の operator ではない任意の bond である。
**job court は標本ではなく全公開 replay**、標本を使うのは mining の Gate C 抽選だけ。

**共通部品:**
- (a) v11: producer 以外の bond と operator(protected `:173-174`)、DA は任意の demander(`palw_flat_execution_da_fold_v11.rs:57`)。v29: paid owner
  (`palw_native_text_fold_v29.rs:1019-1064`)。v39: producer と同じ operator を除く owner(`palw_native_protected_fold_v39.rs:231`・`:249`)。
- (b) コミット済みの post / trace / book と登録 source binding(`palw_flat_execution_court_fold_v11.rs:204-275`)。秘密 state への依存は無い。
- (c)(d) salted seal → reveal → `verify_flat_fault_v9` ほか(`:25-150`・`:204-275`)、v29 `reveal_paged`(`palw_native_text_fold_v29.rs:1359-1512`)、
  v39 `check_flat_filing_v39`(kernel `flat_point39.rs:521-549`)。
- (e) producer reserve の slash(v39 `palw_native_job_fold_v39.rs:829-833`)。**担保が reserve 未満なら裁定そのものを拒否**(`palw_flat_execution_court_fold_v11.rs:282`、
  `palw_native_job_fold_v39.rs:830`、`palw_native_text_fold_v29.rs:1305-1306`、G-14)。
- (f) Final は retention の経過と未決の seal・owner・DA が無いことを要する(`palw_native_job_v39.rs:459`、`palw_native_job_fold_v39.rs:944-957`)。窓の前に払う経路は見つからない。

| F | (i) 武装時(v11 / v29 / v39 の最も弱いもの) | 固有の code | (h) 試験(水準) | 実行 |
|---|---|---|---|---|
| F01・F04・F05 | PARTIAL | 共通 | v11: kernel `flat_execution11.rs` `all_native_faults_refund_one_escrow_and_pay_only_collected_original_slash`(unit)。v29: なし。v39: SDK `owning39_independent_filing.rs` `independent_replay_locates_original_carry_fault_and_self_checks_filing`(unit) | unrun |
| F02・F10 | PARTIAL | seal が job / book / claim / vector を束縛 | v11 `job_identity_cannot_change_with_funding_carrier_and_unclaimed_refunds_once`、`funded_outer_identity_refuses_old9_and_cannot_alias_two_jobs_or_nonces`(unit) | unrun |
| F03 | PARTIAL | — | `execution11_native_commit_binds_original_job_input_and_reserves_producer_but_incomplete_final_cannot_pay`(node、binding のみで conviction は無い) | unrun |
| F06 | OUT-OF-SCOPE(token id のみ、consensus に tokenizer は無い) | — | — | — |
| F07 | PARTIAL(greedy。seeded・penalty は表現できない) | `palw_flat_execution_v11.rs:959` | — | — |
| F08 | PARTIAL | `palw_flat_execution_da_fold_v11.rs:26-360`、`palw_native_public_fold_v39.rs:127-237`・`:370-400` | fold / node 試験なし | — |
| F09 | PARTIAL | `MINING_MASTER_USED`(`palw_native_mining_fold_v39.rs:414-416`) | unit | unrun |
| F11 | PARTIAL | — | `execution11_native_unknown_seal_chunk64_drops_before_allocation_with_or_without_signature_and_own_fence`(node) | unrun |
| F12 | PARTIAL | — | unit | unrun |
| F13 | **GAP**(v11: job ごと 4 枠 `palw_flat_execution_court_fold_v11.rs:107-110`、v29: 生涯 4 owner `palw_native_text_fold_v29.rs:1160-1161`、v39: `st.admitted >= 4` は減らない `palw_native_protected_fold_v39.rs:250`) | — | なし | — |
| F14 | PARTIAL(mining の Gate C 抽選) | `palw_security_certification_fold_v1.rs:403-418` | — | — |
| F15 | PARTIAL | — | なし | — |
| F16 | **GAP**(mining: `accusation_pending: false` 固定 `palw_security_certification_fold_v1.rs:427`、veto 0 `palw_security_certification_v1.rs:84`) | — | なし | — |
| F17 | **GAP**(Gate C の attestor を slash する経路が無い) | — | なし | — |
| F18 | PARTIAL(finder ≠ producer、`finder_permille ≤ 1000` で全額回収もあり得る) | — | v11 `historical_source_theorem_burns_same_operator_bounty_and_never_pays_a_close_caller`(unit) | unrun |
| F19 | PARTIAL(担保不足で裁定拒否。mining の `reserved_exposure` を slash する経路が無い `palw_native_mining_v39.rs:728`) | — | なし | — |
| F20 | PARTIAL(登録 source binding) | — | — | — |
| F21 | PARTIAL(dismissal fee) | — | — | — |
| F22 | **GAP**(担保不足で裁定拒否、G-14) | 上記 | なし | — |
| F23 | OUT-OF-SCOPE(greedy のみ) | — | — | — |
| F24 | n/a(整数) | — | — | — |

### 3.6 lane: EXEC slice

**DAA 14,000: OUT-OF-SCOPE(24 cell 全部)。** 武装拒否(§1.1)。REAL root の class は `cfg(test)` の `exec_v2_test_admit_class_v1` でしか受理されない(`processor.rs:294-310`)。

**共通部品:** (a)〜(d) は kernel route から継承する(slice は自分の kernel claim を通してだけ検証・有罪・default になる、`palw_exec_v2_verify.rs:1-17`、同期
`palw_exec_v2_fold.rs:1443-1590`)。(e) slice の executor の kernel 予約を slash。**REAL claim は V2 側で課金されずに void される**(`WorkSliceProvenFalse` /
`WorkSliceDefaulted` は charge 0、`transition_builder.rs:9134-9145`、`palw_state_v2.rs:4683-4690`)。(f) slice が未検証の間は REAL root の Final を保留
(`palw_exec_v2_fold.rs:155-160`)、conviction は suffix と root を void(`void_suffix_v2` `:1592-1637`、`void_prefix_v2` `:1644-1686`)、settle 後は `invalidate_final`
(`palw_exec_v2_rewards_v1.rs:569-600`)で weight を戻し round rights を没収する。報酬 leg は `liability_until` の後(`:448`)。

| F | (i) 武装時 | 固有の code | (h) 試験(水準) | 実行 |
|---|---|---|---|---|
| F01 | PARTIAL(slice と prefix claim 経由の REAL output は揃う。REAL の V2 step trace は legacy route) | prefix binding(`palw_exec_v2_fold.rs:880-940`) | `x8_g14_an_outsider_convicts_a_slices_kernel_claim_and_its_suffix_and_root_void_on_the_real_node`(`exec_slices.rs:1128`、node)、`:1785`、`exec_v2_fold_v1.rs:4402`(fold) | unrun |
| F02・F10 | COVERED(受理時に拒否) | `palw_work_slice_v2.rs:784-810`、`palw_exec_v2_fold.rs:945-950` | `t12_exec_v2_carriage.rs:640`、`exec_slices.rs:1000`(node) | unrun |
| F03 | COVERED(GAP-62 は code で閉じている) | `palw_work_slice_v2.rs:781-782` | `exec_slices.rs:1628`・`:1785-1797`(node) | unrun |
| F04 | COVERED | kernel の decode court | `exec_slices.rs:1724`(node) | unrun |
| F05 | COVERED(token state。pipeline slice は `VerificationKindUnsupported` で拒否 `palw_work_slice_v2.rs:806`) | `palw_exec_v2_verify.rs:43-58` | kernel route の試験を継承 | unrun |
| F06 | OUT-OF-SCOPE | — | — | — |
| F07 | COVERED(greedy、kernel から継承。それ以外は表現できない) | — | — | unrun |
| F08 | COVERED(GAP-60 の node 試験がある) | — | `x8_g14_a_withheld_slice_is_demanded_by_an_outsider_and_defaults_its_suffix_and_root_on_the_real_node`(`exec_slices.rs:1628`、node)、`exec_v2_fold_v1.rs:3214`(fold) | unrun |
| F09 | COVERED | — | `exec_slices.rs:1561`(node) | unrun |
| F11 | COVERED | `palw_exec_v2.rs:509-523`・`:367-369` | `t12_exec_v2_carriage.rs:436`・`:558`(node) | unrun |
| F12 | COVERED(暫定値) | `palw_exec_v2_fold.rs:942-944`、`palw_exec_v2_rewards_v1.rs:35-66` | `exec_slices.rs:1181`(node) | unrun |
| F13 | PARTIAL(network 全体で 256 open root、bond ごと 4、squat できる) | `palw_work_slice_v2.rs:45-62` | — | — |
| F14 | PARTIAL(kernel route を継承) | — | — | — |
| F15 | COVERED(lane の block は weight を持たない) | — | `t12_exec_v2_carriage.rs:328`・`:1363`・`:1425`(node) | unrun |
| F16 | **GAP**(REAL root の V2 Valid 署名者は課金されない、G-X1) | `transition_builder.rs:9134-9145` | `exec_v2_fold_v1.rs:3036`(root に課金が無いことを assert、fold) | unrun |
| F17 | **GAP**(kernel と同じ誘因の問題。Final は選択された承認も要する `palw_exec_v2_fold.rs:162-170`) | — | — | — |
| F18 | **GAP**(kernel と同じ) | — | — | — |
| F19 | COVERED | — | kernel `exec_liability.rs`(unit、kernel ref) | unrun |
| F20 | PARTIAL(REAL root は seam でしか受理されない) | `processor.rs:294-310` | — | — |
| F21 | COVERED(honest twin) | — | `exec_slices.rs` の honest twin(node) | unrun |
| F22 | PARTIAL(kernel と同じ clamp) | — | — | — |
| F23 | OUT-OF-SCOPE(pipeline slice) | — | — | — |
| F24 | n/a(整数) | — | — | — |

### 3.7 lane: TIR 公開 frame(tag 170/171、`palw_tir_public_conviction_v1`)

**DAA 14,000:** P14-min では休眠(OUT-OF-SCOPE、24 cell)。P14-g14 では武装できる(拒否規則も測定記録 gate も無い、`params.rs:4988-5016`)。
主張の文言に最も近い形を source で実装した lane である。ただし lane そのものに 2 つの迂回がある。

- **frame は任意である(G-03)。** TFP1 envelope を付けた FP claim だけが frame を持つ(`prepare_paired_original_v1`、`palw_tir_public_fold_v1.rs:77`、`objects.rs:10-14`)。
  付けない TIR FP claim は旧 ClaimBundle / legacy route の受理に落ちる(`objects.rs:3566-3573`)。
- **frame が受理しない original**(job tail、flat logits scheme)も旧経路に落ちる(`palw_tir_public_fold_v1.rs:332-334`)。

**共通部品:**
- (a) 告発者: `conviction_filer_v1`(`palw_tir_public_fold_v1.rs:718-731`)— producer でも Valid credit / grant の保有者でもない、Active で floor 以上の任意の bond。
  Valid を出していない抽選 seat と外部 bond は同じ object を出し、同じ裁定になる。DA の owner(Open / Demand / Collect)は `independent`(`:37-47`)で、抽選 seat はなれない。
  owner は claim ごと K_owner = 8(`palw_tir_public_claim_v1.rs:277`、検査 `palw_tir_public_fold_v1.rs:509-511`、数え方 `palw_staged_court_fold_v1.rs:237-241`、枠は material horizon まで解放されない)。
- (b) 元 root への opening、EOF まで認証した完全な id 列、rooted chunk から再構成して再認証した回答(`palw_tir_public_owned_v1.rs:2228`・`:2231`)。program は state の登録値を
  native に埋め、持ち込みの program は拒否する。replay には state に残る prompt ids(`fp_claim_prompt_ids_v1`、`chain_state.rs:1290-1292`)が要る。prompt ids は
  `palw_fp_derived_work` 以後に価格が derived work を持つときだけ書かれ(`objects.rs:3615-3630`、`palw_state_v2.rs:19126`)、G14 fence はこれを要求しない(G-T3)。
- (c) cone / logits primitive、decode-token primitive、完全な id 列。witness は prepaid の terminal 上限内で、16 KiB part の分割搬送(`TirWitness`)により切り詰めない。
- (d) `apply_conviction_v1`(`palw_tir_public_fold_v1.rs:781-931`: door `:794-796`、一回性 `:803-815`、Arithmetic `:819-840`、DecodeToken `:841-871`、Output と停止規則 `:872-906`)、
  `adjudicate_original_v1`(`palw_tir_objective_v1.rs:272`)、`adjudicate_original_decode_v1`(`:293`)、`adjudicate_original_output_v1`(`:103`)、`original_stop_rule_fault_v1`(`:317`)、
  選択規則 `PalwTirSelectionRuleV1`(`palw_tir_court_v1.rs:2322-2340`)と `check_tir_decode_token_tiled_rule_v1`(`:2362`)。述語が成り立たないか error なら filing を拒否し、state は変わらない。
- (e) 全責任 bond の exit hold を終局時に assert(`palw_tir_public_fold_v1.rs:908-914`)、credit 済みの Valid custodian 全員に S4(`charge_covering_signers_v1`
  `transition_builder.rs:3112-3141`、呼び出し `:916-920`)、producer は `convict_by_court_verdict_v1`(`transition_builder.rs:3565`、呼び出し `:922`)で void・CourtConviction・filer への報奨・
  ADR-0160 の intent-class 没収と凍結。execution-proving の違反は root で没収(`:924-926`)。DA は `staged_selected_default_charge_v1`(`transition_builder.rs:2998`、fold `:664-691`)。
  exit hold の索引 `palw_staged_court_fold_v1.rs:3094-3100`、退出時の強制 `palw_state_v2.rs:3347`。
- (f) Final floor を frame の `collect_until` まで上げる(`palw_claim_final_floor_v1`、`palw_state_v2.rs:3626-3650`、frame 分 `:3645-3648`)。Gate C は door が開いている間待つ
  (`palw_tir_public_prosecution_open_v1` `palw_staged_court_fold_v1.rs:244`、読み `palw_security_certification_fold_v1.rs:431-434`)。void の claim は receipt spend が Final を要するので
  受信検証で拒否(`processor/palw.rs:9971-9973`、没収 root `:9981-9986`)、template の spendable 一覧からも外れる(`processor/palw.rs:3548`・`:3585-3589`)。有罪の producer は凍結され、
  attempt の受理で `ProducerFrozen`(`palw_admission_v2.rs:651-652`)。

| F | (i) P14-g14(fold / node) | 固有の code | (h) 試験(水準、すべて source のみ) | 実行 |
|---|---|---|---|---|
| F01 | **COVERED**(fold)。node の自動検出は PARTIAL(owner は step root の不一致からだけ疑う `kaspad/src/palw_tir_public_owner.rs:555-562`) | Arithmetic `:819-840` | `g14_full_collusion_one_outside_verifier_convicts_slashes_and_refuses_the_claims_standing`(`palw_tir_public_fold_v1.rs:1399`、fold)、`g14_an_outsider_detects_a_fault_and_its_filed_finding_convicts_and_slashes`(`:1963`、fold) | unrun |
| F02 | PARTIAL(fold は揃うが固有の試験が無い) | job pin `palw_tir_objective_v1.rs:122-131`、`palw_tir_public_fold_v1.rs:799-802` | なし | — |
| F03 | PARTIAL(prompt ids が state に無い class は replay できず、owner は skip `palw_tir_public_owner.rs:317-319`) | prompt 認証 `palw_tir_court_v1.rs:1095`・`:1109`・`:1283` | なし | — |
| F04 | **COVERED**(fold)。node は PARTIAL | `:872-906`、owner `:413-431` | `g14_output_identity_convicts_from_the_complete_authenticated_ids_only`(`:1491`、fold)、`g14_owner_output_contradiction_is_the_output_door`(`palw_tir_objective_v1.rs:605`、unit) | unrun |
| F05 | **GAP**(frame の semantic stage は ClaimBundle 契約を通して staged court に依るが、staged court は武装できない予約を要する、G-18。owner も semantic court を動かさない) | `:209-260`・`:446-452` | frame 経由の試験なし | — |
| F06 | **GAP**(同上) | 同上 | なし | — |
| F07 | **GAP**(fold の door と停止規則はあるが、steps と output が一致し trace だけ違う場合を owner は log して skip する `palw_tir_public_owner.rs:574-580`。選択証拠の builder `kaspad/src/palw_tir_public_filer.rs:54-66` は呼ばれない) | `:841-871`、`palw_tir_court_v1.rs:2362` | `g14_the_selection_door_judges_the_original_jobs_own_rule`(`consensus/core/tests/palw_tir_court.rs:564`、unit)、`g14_original_stop_rule_is_the_jobs_own_decoder`(`palw_tir_objective_v1.rs:582`、unit)。DecodeToken・停止規則の filing の fold 試験なし | unrun |
| F08 | **COVERED**(fold)。node は PARTIAL(responder は同時 4 demand `palw_tir_public_custodian.rs:868`・`:994`、応答窓は他 owner の同時 demand を数えない `palw_tir_public_owned_v1.rs:1805-1830`) | `:664-691`。garbage は Finish できないので demand が開いたまま collect できる | `g14_one_outside_bond_reaches_the_preparation_default_with_no_panel_signature`(`:1535`、fold)、`g14_an_honest_custodian_answers_in_time_and_is_not_slashed`(`:1920`、fold、producer のみ) | unrun |
| F09 | **COVERED** | TFP1 pairing `:58-69`・`:80-89`、`DuplicateWork`(`objects.rs:3455` 付近) | `new_original_frame_cannot_license_an_unqualified_valid_or_reinterpret_typed_receipt`(`:1120`、unit) | unrun |
| F10 | **COVERED** | `:28-35`・`:799-802`・`:859-861` | 同上、`g14_history_pledge_rides_tfp1_and_must_name_the_same_original`(`palw_tir_public_fp_envelope_v1.rs:405`、unit) | unrun |
| F11 | **COVERED** | 署名 `:573`、現行 row 比較 `:693-697`、rent `:650`・`:657`、TFP1 の fee `palw_accept.rs:2549-2556` | `the_common_filing_is_signed_by_the_literal_filer_and_identical_for_seat_and_outsider`(`kaspad/src/palw_tir_public_filer.rs:268`、unit)、`g14_staged_witness_assembles_in_order_and_convicts_once`(`:1594`、fold) | unrun |
| F12 | PARTIAL(door は固定、Final floor と Gate C 待ちがある。応答窓の同時性は F08) | door `:794-796`、X3 期限 `:414-418`、witness door `:949` | `:1399` の door 外拒否(fold) | unrun |
| F13 | **GAP**(K_owner の 8 枠を先取りできる、G-T1) | `:509-511`、`palw_staged_court_fold_v1.rs:237-241` | なし | — |
| F14 | PARTIAL(選択規則は job から取り filer は選べない。seed 自体の grinding は job 側の規則) | `palw_tir_court_v1.rs:2330-2340` | — | — |
| F15 | **COVERED** | delta journal、consumed offence key | `:1399` の journal revert、`g14_fresh_node_import_continues_the_original_frame_identically`(`:1648`、fold) | unrun |
| F16 | PARTIAL(fold は揃う。抽選 seat の node 経路が無い: owner は抽選 seat を skip `palw_tir_public_owner.rs:308-309`、Open も拒否、G-T4) | 全経路 | `:1399`(fold。credit 済み frame は `apply_grant_ready_v1` を通さず手組み) | unrun |
| F17 | PARTIAL(誰かが告発したときだけ S4。この lane に trap は無い) | `:916-920` | `:1399` | unrun |
| F18 | PARTIAL(producer の Sybil も filer になれる。報奨は collected の割合で、自己告発の純損益は負 `palw_state_v2.rs:1236-1256`) | `:718-731` | `r_7_a_self_conviction_nets_negative_over_a_grid`(unit) | unrun |
| F19 | **COVERED** | exit hold | `:1399`(`:1414-1417` の assert) | unrun |
| F20 | PARTIAL(登録 class と layout を state から取る `prepare_original_v1` `:313-334`。固有の試験なし) | — | — | — |
| F21 | **COVERED** | — | `g14_honest_twin_is_not_convicted_and_a_holding_predicate_certifies_nothing`(`:1471`、fold) | unrun |
| F22 | PARTIAL(Valid seat の S4 lock の存在を conviction 時に assert しない。DA 側は `:677-683` で assert) | — | — | — |
| F23 | OUT-OF-SCOPE(job tail・flat は frame の受理を拒否 `:332-334`、V3/V4 typed receipt は名指しで拒否 `:1086-1093`。ただし旧経路へ落ちる) | — | `:1120`(unit) | unrun |
| F24 | n/a(整数 TIR。Float32 は staged の別 fence) | — | — | — |

### 3.8 lane: staged public court(`palw_staged_public_court_v1`)

**DAA 14,000:** P14-min では休眠(OUT-OF-SCOPE)。P14-g14 では武装できるが、**seed も bundle も開けない。** 新しい staged seed は告発者自身の生きた予約(tag 154)を要し
(`palw_staged_court_fold_v1.rs:1010-1014`)、`BundleOpen` も予約を要する(`:2870-2881`)。予約は武装できない `palw_legacy_public_filer_v1` を要する(R4)。staged court の validator は
この依存を要求しない(`validation.rs:2879-2886`、G-18)。したがって P14-g14 では、required-family の FP claim(prefix state、Constraint、raw tokenizer)は HFP1 envelope 付きで受理されるが、
誰も追及できない。TIR frame の semantic stage(F05・F06)も同じ理由で届かない。

source にあるもの(legacy filer も武装した場合): required family の PublicPrefix、Constraint(Flat / Tiled、Float32 は greedy のみ)、raw tokenizer、history。source default は、
producer 自身が未払いの集合にいるときだけ producer を課金し、沈黙した covering 署名者に S4 を課す(`transition_builder.rs:2998-3045`)。producer の lock は `collect_until` まで committed
(`palw_state_v2.rs:2756`)、退出は保留(`palw_staged_court_fold_v1.rs:491-498`)、retention は `material_until` まで延長(`:672`)。

| F | (i) P14-min | (i) P14-g14 | (i) 武装時(legacy filer も) | 要点 / (g) | (h) 試験(水準) | 実行 |
|---|---|---|---|---|---|---|
| F01〜F04・F09・F10 | OOS | n/a | n/a | この lane の担当ではない(§3.2・§3.3 の経路に依る) | — | — |
| F05 | OOS | **GAP** | PARTIAL | PublicPrefix family | `public_prefix_fold_v1.rs:254`・`:604`・`:1683`(fold)、`fresh_public_bond_service_native_public_prefix_initial_convicts_all_custodians`(`palw_service_public_e2e.rs:707`、N\*) | unrun |
| F06 | OOS | **GAP** | PARTIAL | raw tokenizer | `registered_a16_false_tokenizer_length_restarts_and_collects_full_panel_through_rooted_court19`(`kaspad/src/palw_service_tokenizer_e2e.rs:14`、N\*) | unrun |
| F07 | OOS | **GAP** | PARTIAL(greedy constraint のみ。V4・seeded は対象外) | Constraint family | 同上 | unrun |
| F08 | OOS | **GAP** | PARTIAL | source default | `staged_source_default_charges_the_producer_only_when_it_is_itself_in_the_unpaid_set`(`public_prefix_fold_v1.rs:1843`、fold) | unrun |
| F11 | OOS | **GAP** | PARTIAL(lane 1・2 を継承) | — | — | — |
| F12 | OOS | **GAP** | **GAP** | Final は `collect_until` まで floor されない。予約が生きている間だけ止まる(R10) | — | — |
| F13 | OOS | **GAP** | PARTIAL | claim 全体の上限なし(`palw_staged_court_fold_v1.rs:1071`)、予約者ごと 1 row(`palw_rendering_resources_v1.rs:366`)、inclusion margin 4 DAA、負荷入力は仮値(`palw_staged_load_bound_v1.rs:22-27`) | — | — |
| F14 | OOS | n/a | n/a | 継承 | — | — |
| F15 | OOS | **GAP** | PARTIAL | `reorg_retention_daa` 256 | — | — |
| F16 | OOS | **GAP** | PARTIAL | 全 custodian を有罪にする N\* 試験がある(F05・F06) | 同上 | unrun |
| F17 | OOS | **GAP** | **GAP** | trap なし | — | — |
| F18 | OOS | **GAP** | PARTIAL | owner は producer でも Panel seat でもない | — | — |
| F19 | OOS | **GAP** | COVERED | producer と署名者の lock を `collect_until` まで保持 | — | — |
| F20 | OOS | n/a | n/a | — | — | — |
| F21 | OOS | **GAP** | PARTIAL | — | — | — |
| F22 | OOS | **GAP** | PARTIAL | — | — | — |
| F23 | OOS | OUT-OF-SCOPE | OUT-OF-SCOPE | seeded Float32 Constraint は全高さで拒否(`palw_staged_preclaim_v1.rs:155-161`)、Float32 は fence 未満で拒否 | — | — |
| F24 | OOS | n/a | n/a | — | — | — |

### 3.9 集計

168 cell(7 lane × 24 分類)。lane ごとの内訳(C / P / G / O / n/a)。

| lane | A. P14-min | B. P14-g14 | C. 全武装の仮定(参考) |
|---|---|---|---|
| V2 Attempt | 2 / 15 / 5 / 0 / 2 | 同左 | 3 / 18 / 1 / 0 / 2 |
| Free Prompt | 2 / 10 / 11 / 0 / 1 | 同左 | 3 / 16 / 3 / 1 / 1 |
| Kernel / OPV | 0 / 0 / 0 / 24 / 0 | 同左 | 11 / 8 / 2 / 2 / 1 |
| native funded | 0 / 0 / 0 / 24 / 0 | 同左 | 0 / 17 / 4 / 2 / 1 |
| EXEC slice | 0 / 0 / 0 / 24 / 0 | 同左 | 13 / 5 / 3 / 2 / 1 |
| TIR 公開 frame | 0 / 0 / 0 / 24 / 0 | 9 / 9 / 4 / 1 / 1 | 9 / 11 / 2 / 1 / 1 |
| staged public court | 0 / 0 / 0 / 24 / 0 | 0 / 0 / 14 / 1 / 9 | 1 / 11 / 2 / 1 / 9 |
| **計** | **4 / 25 / 16 / 120 / 3** | **13 / 34 / 34 / 74 / 13** | **40 / 86 / 17 / 9 / 16** |

C の legacy 2 lane は LG14-A / LG14-B と N4 を武装したと仮定した評価で、休眠 fence 下の N\* 試験に依る。P14 と異なる cell は、V2 Attempt では F01・F12・F16 が PARTIAL、
F13 が COVERED。Free Prompt では F01・F03・F05・F06・F12・F16 が PARTIAL、F13 が COVERED、F23 が OUT-OF-SCOPE(F07・F17・F22 は GAP のまま)。
C の TIR frame の F05・F06 は、legacy filer があれば staged court に届くので PARTIAL にした。

## 4. 敵対的な gap 探索

producer と Panel 全員が共謀する側に立ち、「不正な claim が、1 人の正直な検証者からの conviction 経路を持たないまま、報酬・ブロック機会・consensus credit に届く道」を探した。
手順は書かず、成立する条件と修正だけを書く。

- **CRITICAL**: active plan(P14-min、または武装できる全部の P14-g14)のもとで、不正な claim が conviction 経路なしに報酬か credit に届く。
- **HIGH**: 経路が塞がれ得る、一部の不正分類で経路が無い、または武装時に成り立つはずの lane で必須の要素が欠ける。
- **MEDIUM**: 制裁・回収の量が不足する、条件付きで経路が狭まる、証拠が不足する。
- **LOW**: 整合性・保守の問題。

### 4.1 lane をまたぐ所見

| ID | 重大度 | 所見(成立する条件) | 根拠 | 必要な修正 |
|---|---|---|---|---|
| G-01 | **CRITICAL** | **DAA 14,000 で報酬を出す lane は、G14 を満たさない legacy Panel route だけである。** P14-min・P14-g14 のどちらでも、legacy V2 Attempt / Free Prompt(と同じ Panel route の TIR v1・gen v1・shard 等)は int-12 の規則のまま報酬と weight を出す。legacy route を G14 に乗せる 2 fence は武装できない。fence 前に登録された class も報酬を得続ける。 | `palw_legacy_public_filer_v1.rs:313`、`palw_legacy_held_da_v2.rs:104`、`PALW_T12_INT11_FENCES_V1`(`params.rs:11551`)、R11、GAP-80([activation-readiness-matrix](../../../design/palw/integration/activation-readiness-matrix.md)) | ユーザー判断が要る: (a) LG14-A / LG14-B を検証して DAA 14,000 で武装する、(b) DAA 14,000 以後に受理する legacy route の新規 claim から、G14 経路の無い family の報酬と weight を外す(N4 と同じ形の claim gate)、(c) Gate C を武装可能にして武装し、legacy claim を未認証のまま残す。 |
| G-02 | **CRITICAL** | **対象外の画像・非公開 source の work が報酬を得る。** FP Job V5 の画像 bytes と source id は worker と Panel にしか渡らない。PanelDa も同じ。全 Panel が共謀すると、外部の検証者は material を得られない。t12 で DAA 5,300 から有効で、除外 fence(N4)は武装できない。 | `palw_fp_job_v5.rs:15-17`・`:457`、`palw_ruling_fences_v1.rs:16-21`、`validation.rs:18` | `palw_public_evidence_reward_v1` を検証して DAA 14,000 で武装する(規則の source は既にある)。 |
| G-03 | **CRITICAL**(P14-g14) | **TIR 公開 frame は任意である。** TFP1 を付けない TIR FP claim は frame を持たずに旧経路で受理される。共謀する producer は frame を付けないだけでこの lane を避けられる。 | `palw_tir_public_fold_v1.rs:77`、`objects.rs:10-14`・`:3566-3573` | fence 以後、tiled な TIR FP claim には TFP1 を必須にする。または frame の無い TIR claim には Final・credit・報酬を与えない。 |
| G-04 | **CRITICAL**(P14-min) | **Gate C を武装できないので、対象外を「未認証」にする防御が無い。** Gate C の述語は fence 未満で `true` を返す。 | `palw_rfc0018_v1.rs:147`・`:546`・`:628` | Gate C を武装可能にする(RFC-0018 §12 の U-1〜U-8)。それまでは受理時の拒否だけで I10 を満たす。 |
| G-05 | **CRITICAL**(P14-min) | **legacy FP の選択・constraint・prefix・tokenizer の不正は、正直な seat 1 人でも裁けない。** DAA 5,300 以後の FP claim は decode-token close で裁けず、FP の選択証明は拒否され、contradiction 14〜18 は武装できない fence の下にある。validation の注記自体が「seat quorum で止める」とする。 | R5・R6、`palw_court_v2.rs:1281-1291`、`validation.rs:1292-1298` | これらの job version を、経路が武装されるまで受理時に拒否する。または TIR frame と同じ「job 自身の規則」の door を legacy FP にも作り、武装する。 |
| G-06 | HIGH | **legacy route の bystander は追い越される。** live の規則では非 seat の DA session は claim を止めず、追及中に claim が Final になる。court による先取り(`ClaimUnderSession`)も live。node-E2E がこの挙動を固定している。 | `lg14a_unarmed_the_reservation_is_dropped_by_name_and_the_bystander_is_outrun`(`lg14a_legacy_filer_e2e.rs:1123`)、R1・R3 | G-01 (a)。`DirectProofFirst` を独立の武装できる fence に分ける。 |
| G-07 | HIGH | **legacy の非 seat 予算と court 上限を Sybil が使い切れる。** | R2・R14 | 予約(LG14-A)の武装、または予算を告発者ごとに分ける。 |
| G-08 | HIGH | **legacy FP の報酬は conviction の窓が閉じる前に払われる。** coinbase は licence + 1,200 から払われ、Final の反転は 3,000 まで可能で、払った coinbase は戻らない。 | R10、`dos_repro_1_fp_final_receipt_rights_unpriced_unforfeitable` | TIR frame と同じく Final floor を責任の期限まで上げる(`palw_claim_final_floor_v1` の frame 分と同じ形)、または receipt の carve を vesting にする。 |
| G-09 | HIGH | **X3 の絶対期限が無い。** live では held decoy の敗北後に窓が再付与される。staged court は期限を後ろへ動かす。期限が動くので、追及の開始予算を固定できない。 | `palw_ruling_fences_v1.rs:6-15`、`validation.rs:22`、[spec register](../../../design/palw/integration/mainnet-spec-register-2026-10-12.md) X3 | `palw_claim_hard_deadline_v1` の値を決めて検証し、武装する。 |
| G-10 | HIGH | **武装できる G14 fence に、武装拒否も測定記録 gate も無い。** staged court・TIR route・Float32・TIR conviction は、前提が揃えば validation を通る。未実行の code が武装され得る。 | `validation.rs:2861-2886`、`params.rs:4988-5016` | V1〜V7 の記録が pin されるまで `cannot be armed` を返す。他 lane と同じ measured-value gate を付ける。 |
| G-11 | HIGH | **rule E を武装できず、V6(担保の無い private heartbeat 枝)が残る。** G14 の conviction 自体は journal で再編に一貫するが、取り込まれた private branch の Final の credit を止めるのは fork choice である。 | `palw_fork_choice_rule_e_v1.rs:102`、[finality-palw-consistency](../../../design/palw/consensus/finality-palw-consistency.md) V6 | ADR-0178 の rule E(と Gate C の rule E v2)を検証して武装する。 |
| G-12 | MEDIUM | **live の bond 式 v1 は、licence 時に escrow 予約と issuance slot を解放する。** SR-1 は全 seat が Valid を出したときに producer の escrow 項を解放する。共謀する Valid は同じ slot を回せるので、同時の責任が徴収できる額を超え得る。v2 は武装できない。 | `palw_bond_formula_v2.rs:5-14`・`:541`、R18 | bond 式 v2 の値を決めて武装する。 |
| G-13 | MEDIUM | **Valid 署名者の課金は lock がある間だけ。** covering seat の slashable lock が無ければ黙って飛ばす。lock の期限(Final + 1,000)の後、反転の期限(3,000)の前の conviction は署名者に届かない。slash は現担保で clamp し、seat の 1 claim あたりの露出は `min_collateral_sompi` が上限。 | `transition_builder.rs:3125`・`:8996-9015`、R8 | conviction 時に「各 Valid seat の lock が残っていること」を assert し(TIR の DA 側 `palw_tir_public_fold_v1.rs:677-683` と同じ)、lock の期限を claim の責任期限まで延ばす。 |
| G-14 | MEDIUM | **担保が不足すると裁定そのものを拒否する lane がある。** native lane は producer の担保が reserve 未満だと裁定を拒否し、不正な claim は Final に進み得る。他 lane の slash で担保を減らせば起こせる。 | `palw_flat_execution_court_fold_v11.rs:282`、`palw_native_job_fold_v39.rs:830`、`palw_native_text_fold_v29.rs:1305-1306` | 担保の多寡に関係なく claim を void して依頼者に返金し、残る担保を slash する。 |
| G-15 | MEDIUM | **認証後・Final 後の conviction は weight だけ戻し、支払い済みの報酬は戻さない。** Gate C 認証時に held reward を払う。prosecution window(設計値 7 日)の後でも retirement までは conviction できる。kernel route の FinalReward も Final 時に coinbase queue へ入る。 | `credit_certified_v1`(`palw_security_certification_fold_v1.rs:295-359`)、`reverse_convicted_final`(`transition_builder.rs:5596-5641`)、`palw_kernel_route_fold_v1.rs:1200-1218` | 報酬 leg を責任期限まで保留する(EXEC の leg はそうしている、`palw_exec_v2_rewards_v1.rs:448`)。 |
| G-16 | LOW | **Kimi の扱いが 2 か所で食い違う。** coverage は Kimi router を uncovered とするが、`palw_cell_uncovered_disposition_v2` は router binding のもとで covered とする。 | `palw_cell_coverage_v1.rs:364-368`・`:534-538`、`palw_router_binding_v2.rs:8` | §4.3 の決定(対象外)に揃える。 |
| G-17 | MEDIUM | **drill で注入できる不正は 4 種類だけ。** `--palw-drill-tamper-leaf`・`-fp-leaf`・`-eval`・`-boundary`。出力・選択・tokenizer・state・DA の差し控え・TIR frame の不正は注入できない。 | `kaspad/src/args.rs:627`・`:2020`・`:2620`・`:2686` | §5.3 の注入手段を足す。 |
| G-18 | HIGH | **staged public court は、武装できない legacy filer に依存するのに、validator がそれを要求しない。** 武装しても seed も bundle も開けない。TIR frame の semantic stage も同じ。 | `palw_staged_court_fold_v1.rs:1010-1014`・`:2870-2881`、R4、`validation.rs:2879-2886` | staged fence に legacy filer fence を前提として要求する。または staged court を tag 154 から切り離す。 |
| G-19 | LOW | **legacy と staged の node 試験は、harness が休眠 fence を validation なしに武装する。** lane 1 の node 試験(lg14a・g14v)は、署名者の slash と coinbase / weight の拒否を assert しない。staged の試験は Final と receipt spend を扱わない。 | `lg14a_legacy_filer_e2e.rs:103`、`palw_service_public_e2e.rs:175` | §5.2 の試験を足す。 |

### 4.2 lane 固有の所見

| ID | 重大度 | lane | 所見 | 根拠 | 必要な修正 |
|---|---|---|---|---|---|
| G-T1 | **CRITICAL**(P14-g14) | TIR frame | **outside owner の 8 枠は先取りできる。** 枠の上限は他の検査より先に効き、material horizon まで解放されない。枠が無いと外部の者は material を demand できず、DA default を collect できず、id 列も得られない。 | `palw_tir_public_fold_v1.rs:509-511`、`palw_staged_court_fold_v1.rs:235-241`、[remaining R1](../../../design/palw/integration/g14-objective-conviction-remaining-2026-10-15.md)(Lead 既定、ユーザー確認待ち) | idle な owner の枠を回収する(一定 DAA demand の無い owner を退去させ deposit を没収)、または dry run が通る finding を持つ bond のために常に 1 枠を空けておく。 |
| G-T2 | HIGH | TIR frame | **選択の不正を node が局所化しない。** owner は step root か output root の不一致からだけ疑い、trace だけが違う場合は log して skip する。選択証拠の builder はあるが呼ばれない。 | `kaspad/src/palw_tir_public_owner.rs:555-562`・`:574-580`、`kaspad/src/palw_tir_public_filer.rs:54-66` | 認証済み id 列から最初に食い違う position を求め、その行の tile を demand し、decode-token pin を作って filing する。fold 水準の DecodeToken filing の試験を足す。 |
| G-T3 | HIGH | TIR frame | **replay は prompt ids に依る。** prompt ids は `palw_fp_derived_work` 以後にしか書かれず、G14 fence はそれを要求しない。prompt ids が無い frame を owner は skip する。 | `objects.rs:3615-3630`、`palw_state_v2.rs:19126`、`palw_tir_public_owner.rs:317-319` | G14 fence に `palw_fp_derived_work` を前提として足す。または `install_original_v1` で prompt row を書き、`collect_until` まで保持する。 |
| G-T4 | HIGH | TIR frame | **正直な抽選 seat 1 人の node 経路が無い。** owner は抽選 seat を skip し、Open も抽選 seat を拒否する。`queue_finding_v1` に finding を渡すのは owner だけ。consensus 上は抽選 seat も filer になれるが、node はその finding を作らない。 | `palw_tir_public_owner.rs:308-309`、`palw_tir_public_fold_v1.rs:37-47` | Panel の replay finding を `queue_finding_v1` に回す。または Valid を出していない抽選 seat を owner として開けるようにする。 |
| G-T5 | MEDIUM | TIR frame | **同時 demand のもとで正直な custodian が DA default になり得る。** responder は同時 4 demand、demand ごとに 1 move を 10 DAA ごとに再計画する。応答窓は他 owner の同時 demand を数えない。 | `palw_tir_public_custodian.rs:868`・`:994`・`:1154-1174`、`palw_tir_public_owned_v1.rs:1805-1830` | 応答窓を同時 demand 数で伸ばすか、custodian ごとの同時 demand を consensus で制限し、測定する(R7)。 |
| G-T6 | MEDIUM | TIR frame | **証拠が薄い。** DecodeToken と停止規則の filing、frame 経由の seat の DA default、`ProducerFrozen` と receipt spend の拒否の assert、node 試験がいずれも無い。`g14_an_outsider_detects…` は producer の課金だけを assert し、replay は試験 helper で node の owner code ではない。 | §3.7 | §5.2 の T-N1〜T-N7。 |
| G-T7 | LOW | TIR frame | conviction は各 Valid seat の S4 lock の存在を assert しない(DA 側は assert する)。 | `palw_tir_public_fold_v1.rs:916-920`・`:677-683` | 同じ assert を足す。 |
| G-K1 | HIGH | Kernel / OPV | **Panel 許可の暫定 seat が passing receipt に署名しても課金されない。** 署名者の責任を負う唯一の経路は paid-claim trap で、休眠・1 sompi の probe 値で、slash の結果を捨てる。 | `settle.rs:43-93`、`palw_service_kernel_e2e.rs:760-768`、`palw_paid_claim_trap_v1.rs:58`、`palw_paid_claim_trap_fold_v1.rs:288-292` | Final の前後で conviction 時に署名者を清算する種別を足す。または報酬対象の kernel class を OPV に限る。 |
| G-K2 | HIGH | Kernel / EXEC | **試験の seam が本番の受理の代わりをしている。** REAL root は `cfg(test)` でしか受理されず、typed root の適格と artifact attestation も多くの node 試験で seam に依る。 | `processor.rs:234-263`・`:294-310` | 本番の受理経路を作り、seam なしで node 試験を再実行する。 |
| G-K3 | HIGH | Kernel / EXEC / 全 lane | **検証者の誘因(P0)。** 嘘をついた本人の Sybil が報奨を取れ、正直な claim の検査には何も払われない。抑止は価格付けされているが、正直な finder は無報酬になり得る。 | `econ_bounty.rs:168`・`:218`、`c4r4.rs:333`、[activation-readiness-matrix §2](../../../design/palw/integration/activation-readiness-matrix.md) | salted sample の watcher 報奨(readiness matrix §2 の案)。 |
| G-K4 | MEDIUM | Kernel | v4 segment の material は block body にしか無く、責任期限と pruning の規則が無い(v4 は現在報酬対象外)。 | `seg_da.rs:511-519` | v4 を報酬対象にする前に、pruning を責任期限まで止める規則を足す。 |
| G-X1 | HIGH | EXEC | **REAL claim は V2 側で課金されずに void される。** REAL root を licence した V2 seat は逃れ、REAL claim 自身の V2 step trace は legacy route で裁かれる。 | `transition_builder.rs:9134-9145`、`exec_v2_fold_v1.rs:3036` | GAP-81: root を kernel route の claim にするか、prefix claim の conviction 時に V2 署名者と予約を課金する。 |
| G-N1 | HIGH | native | **告発枠の先取り。** v11 は job ごと 4 枠(GC まで解放されない)、v29 は生涯 4 owner、v39 は生涯 4 owner と生涯 32 の DA 要求。producer の operator ではない Sybil が 4 枠を取って reveal しなければ、正直な外部検証者は Final まで締め出される。 | §3.5 F13 | 期限切れの未 reveal seal で枠を解放する、自己完結の fault filing は枠なしで有罪にできるようにする、または枠を価格で増やす。 |
| G-N2 | HIGH | native(mining) | **v39 mining claim は Final 後に追及できない。** `accusation_pending` は false に固定、veto 0、attestor の slash も mining の `reserved_exposure` の court も無い。credit は job court(commit 前に閉じる、G-N1 の影響を受ける)と k/m の前提に依る。 | `palw_security_certification_fold_v1.rs:427`、`palw_security_certification_v1.rs:84`、`palw_native_mining_v39.rs:728` | mining claim に公開告発の窓を設け `accusation_pending` に入れる。有罪なら認証を外して receipt と safe weight を取り消す。Reproduced と署名した attestor を slash する。veto > 0。 |
| G-N3 | LOW | native | finder の取り分が 1000 ‰ になり得るので、自己告発で全額を回収できる。 | `flat_execution11.rs:108`、`palw_native_final_v27.rs:253` | 取り分を 1000 ‰ 未満に制限し、一部を必ず burn する。 |

### 4.3 指定された確認項目への回答

| 確認項目 | 結果 |
|---|---|
| G14 agent 自身が挙げた穴: 選択だけの不一致の局所化 | **確認(未解決)。** G-T2。 |
| 同: prompt ids を保持しない frame | **確認(未解決)。** G-T3。 |
| 同: owner 枠の充填 | **確認(未解決)。** G-T1(ユーザー判断待ちの Lead 既定)。 |
| 同: 1 move ずつの応答 throughput | **一部確認。** responder は同時 4 demand・1 move ずつで、応答窓は同時性を数えない(G-T5)。「node 全体で carrier 1 本」という記述は code に見つからない。最大 profile は未測定(R7)。 |
| 同: 最新 E2E の seat slash | **片方だけ。** `g14_full_collusion…`(`palw_tir_public_fold_v1.rs:1399`)は各共謀 Valid の S4、false-Valid offence の消費、正直な抽選 seat の無傷を assert する。ただし credit 済み frame を `apply_grant_ready_v1` を通さず手組みする。`g14_an_outsider_detects…`(`:1963`)は producer の課金と記録だけを assert する。 |
| 窓が閉じる前に払う lane | **ある。** legacy FP の receipt spend(G-08、live)。kernel route の FinalReward と Gate C の held reward(G-15、休眠)。TIR frame・native・EXEC は窓の後(Final floor、retention、`liability_until`)。 |
| conviction が Gate C の credit を取り消さない経路、frontier を戻さない経路 | Gate C の述語は void の claim に false を返し(`palw_rfc0018_v1.rs:147-160`)、`reverse_convicted_final` は認証済みの weight を引き(`transition_builder.rs:5596-5641`)、frontier は毎 block state から再計算され(`palw_gate_c_frontier_v1`、`palw_security_certification_v1.rs:328`)、pruning は巻き戻し先を越えない(`palw_frontier_rollback_bound_v1` `:355`、`processor/palw.rs:797-833`)。**例外:** retire 済みの claim は対象外(設計どおり、floor `note_retirement_frontier_floor_v1`)、v39 mining claim は告発できない(G-N2)、支払い済みの報酬は戻らない(G-15)。**DAA 14,000 では Gate C 自体が武装できない**(G-04)。試験: `rfc18_frontier_rolls_back_on_conviction_and_pruning_never_passes_a_rollback_target`(`palw_rfc0018_v1/tests.rs:599`、unit)。 |
| 担保不足で slash を飛ばす経路 | **ある。** lock の無い covering seat を飛ばす(G-13)、`slash_bond` の clamp(R9)、native の裁定拒否(G-14)、kernel の clamp と閉鎖時の不足 debit(`palw_kernel_route_fold_v1.rs:1117-1124`)。TIR frame は終局時に全責任 bond の exit hold を assert するので、退出による逃げは塞ぐ(G-T7 を除く)。 |
| 容量・手数料で正直な検証者が締め出される経路 | **ある。** legacy の DA-8 と court 上限(G-07)、TIR の K_owner(G-T1)、native の 4 枠(G-N1)、kernel の共有 court 予算(F-C4R4-05 で緩和)、staged の予約者ごと 1 row。手数料: TIR の owner は自分の rent と deposit を前払いする(`open_v1`)。staged の inclusion margin の入力は仮値で、margin が足りるかは測定項目(`palw_staged_load_bound_v1.rs:22-27`)。 |
| chain が保持しない material を要する検証(pruning と窓) | pruning は裁判中の履歴を消さない(`palw_pruning_point_allowed_v2`、`processor/palw.rs:797`、Gate C 下は `palw_pruning_progress_ceiling_v1` `palw_security_certification_v1.rs:505`)。TIR frame の行は rooted state にあり pruning-point carriage で運ばれる。**欠け:** legacy FP の job は filer fence の後でしか保持されない(R16)、FP V5 の画像・source と PanelDa の prompt はそもそも chain に無い(G-02)、TIR の prompt ids は derived work 次第(G-T3)、kernel v4 の segment は block body のみ(G-K4)。node-local の custody material(準備済み capture)は chain から復元できない(custodian の義務そのもの)。 |
| V+ 権利、model tier、bond 式 v2 との相互作用 | **DAA 14,000 では 3 つとも武装できない**(V+: 測定記録が空 `palw_rfc0016_v1/values.rs:196`、tier: `palw_model_tier_v1.rs:350`、bond v2: `palw_bond_formula_v2.rs:541`)。設計上の確認: Gate C の claim 単位の認証は tier に関係なく全 claim に必要(`rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt`、`palw_rfc0018_v1/tests.rs:534`)。tier の係数は配分に一度だけ掛かり、conviction 経路には影響しない。live の bond 式 v1 の問題は G-12。V+ の権利を有罪 claim が消費した場合の扱い(N7「正直な claim の支払いが下がらない」)は、V+ を武装する前に別途確認が要る(本書では未検証)。 |

## 5. 試験計画 — 証明義務

**どの試験もこの監査では実行していない。** 以下は、COVERED と書いた cell を verified に上げるために通さなければならない試験である。
fold 水準しかない cell は、node 試験と drill を通るまで「実装済み・未検証」のままである。

### 5.1 COVERED cell ごとの必須試験(既存)

| lane | cell | 必須試験(既存、未実行) | 水準 | 足りないもの |
|---|---|---|---|---|
| V2 Attempt | F09・F11 | `audit_repro_03_regression_one_execution_is_one_claim_across_chain_blocks`、`h1_the_gate_passes_what_the_fold_takes…` | unit / node | — |
| Free Prompt | F09・F11 | `dos_repro_1_a_sibling_of_the_convicted_execution_spends_nothing`、`t12_h1_carrier_gate.rs` | unit / node | — |
| TIR frame | F01・F16 | `palw_tir_public_fold_v1::conviction_e2e_tests::g14_full_collusion_one_outside_verifier_convicts_slashes_and_refuses_the_claims_standing`、`g14_an_outsider_detects_a_fault_and_its_filed_finding_convicts_and_slashes` | fold | node 試験。後者に seat の S4 の assert。前者の credit を `apply_grant_ready_v1` 経由で作る |
| TIR frame | F04 | `g14_output_identity_convicts_from_the_complete_authenticated_ids_only`、`palw_tir_objective_v1::g14_owner_output_contradiction_is_the_output_door` | fold / unit | node 試験 |
| TIR frame | F08 | `g14_one_outside_bond_reaches_the_preparation_default_with_no_panel_signature`、`g14_an_honest_custodian_answers_in_time_and_is_not_slashed` | fold | Ready 済み Valid seat の DA default、同時 demand のもとでの応答 |
| TIR frame | F09・F10・F23 | `new_original_frame_cannot_license_an_unqualified_valid_or_reinterpret_typed_receipt`、`palw_tir_public_fp_envelope_v1::g14_history_pledge_rides_tfp1_and_must_name_the_same_original` | unit | — |
| TIR frame | F11 | `g14_staged_witness_assembles_in_order_and_convicts_once`、`kaspad palw_tir_public_filer::the_common_filing_is_signed_by_the_literal_filer_and_identical_for_seat_and_outsider` | fold / unit | mempool・carrier の node 試験 |
| TIR frame | F15・F19 | full-collusion 試験の journal revert と exit hold、`g14_fresh_node_import_continues_the_original_frame_identically` | fold | 実 reorg・実 IBD の node 試験 |
| TIR frame | F21 | `g14_honest_twin_is_not_convicted_and_a_holding_predicate_certifies_nothing` | fold | — |
| Kernel / OPV | F02・F03・F04・F07・F08・F09・F10 | `g14_kernel_route_a_covered_lie_is_convicted_by_an_outsider_through_the_real_path`(`g14_kernel_route_e2e.rs:857`)ほか、`canonical.rs:1269-1324`、`c4r3.rs:111`・`:231`・`:254`、`palw_service_kernel_e2e.rs:676`、`k2_opv.rs:58` | node / unit | 本番 onboarding(seam なし)での再実行 |
| Kernel / OPV | F13・F15・F19・F21 | `c4r3.rs:396`・`:490`、`recovery.rs:290-560`、`rfc15_panel_free.rs:300`、`canonical.rs` honest twin | node | — |
| EXEC | F02〜F05・F07〜F12・F15・F19・F21 | `exec_slices.rs:1000`・`:1128`・`:1181`・`:1561`・`:1628`・`:1724`・`:1785`、`t12_exec_v2_carriage.rs:328`・`:436`・`:558`・`:640`、`exec_v2_fold_v1.rs:3214`・`:4402`、kernel `exec_liability.rs` | node / fold | REAL root を seam(`exec_v2_test_admit_class_v1`)なしで受理する試験 |
| staged(武装時) | F19 | `public_prefix_fold_v1.rs` の lock 保持 | fold | legacy filer を武装できる ruleset |
| 横断 | I6(Gate C の剥奪) | `palw_rfc0018_v1::tests::rfc18_frontier_rolls_back_on_conviction_and_pruning_never_passes_a_rollback_target`、`rfc18_gate_c_is_the_identity_below_its_fence_and_on_every_shipped_network`、`attack_tests::rfc18_t0_no_frontier_no_weight_no_settled_anchor_no_reward_from_uncertified_claims` | unit | Gate C は武装できないので、node では calibration でも走らない |
| 横断 | F18 | `palw_state_v2::tests::r_7_a_self_conviction_nets_negative_over_a_grid` | unit | — |

### 5.2 新しく書くべき試験(gap を閉じる証明義務)

試験の名前は提案である。修正を入れる前に書くと、規則どおりに失敗する(gap を固定する試験になる)。

| ID | 試験 | 水準 | 閉じる所見 |
|---|---|---|---|
| T-N1 | `g14_tir_frame_full_collusion_node_e2e`: 署名済み TFP1 → mempool → template → fold、genesis 後に登録した外部 bond の Open / Demand / filing、conviction、producer と各 Valid seat の slash、`ProducerFrozen` による attempt 拒否、receipt spend の受信拒否、Gate C の待ち | node | TIR frame の COVERED cell を node 水準へ |
| T-N2 | `g14_tir_frame_decode_token_and_stop_rule_filing_convicts`(greedy・seeded・V4 penalty・stop を 1 つずつ) | fold → node | F07 / G-T2 |
| T-N3 | `g14_tir_frame_without_tfp1_earns_nothing_past_the_fence` | fold | G-03(修正後) |
| T-N4 | `g14_tir_owner_slots_cannot_be_preempted_by_idle_owners` | fold | G-T1 |
| T-N5 | `g14_tir_frame_replays_without_derived_work_prompt_ids` | fold | G-T3 |
| T-N6 | `g14_tir_seat_finding_reaches_the_filer_queue` | unit / node | G-T4 |
| T-N7 | `g14_tir_conviction_asserts_each_valid_seat_lock` | fold | G-T7 / G-13 |
| T-N8 | `legacy_route_claim_without_a_g14_path_earns_nothing_past_14000`(G-01 (b) を選んだ場合) | fold → node | G-01 |
| T-N9 | `n4_v5_image_and_panel_da_claims_take_no_new_reward`(`palw_public_evidence_reward_v1` を武装した ruleset) | fold → node | G-02 |
| T-N10 | `legacy_fp_selection_and_constraint_jobs_are_refused_until_their_route_is_armed`(G-05 の拒否案) | fold | G-05 |
| T-N11 | `kernel_route_valid_signers_are_charged_on_conviction` | node | G-K1 |
| T-N12 | `exec_real_root_v2_signers_are_charged_when_a_slice_is_proven_false` | fold → node | G-X1 |
| T-N13 | `native_conviction_is_not_refused_for_short_collateral`(v11 / v29 / v39) | fold | G-14 |
| T-N14 | `native_owner_slots_free_on_unrevealed_expiry`(v11 / v29 / v39) | fold | G-N1 |
| T-N15 | `certified_and_final_rewards_are_held_until_the_liability_horizon` | fold | G-15 / G-08 |
| T-N16 | `staged_court_validation_requires_the_legacy_filer_fence` | unit | G-18 |
| T-N17 | `armable_g14_fences_refuse_without_a_measured_record` | unit | G-10 |

### 5.3 実ノード drill の scenario 一覧

**前提:** 出荷する binary そのもの、salted drill genesis(`--palw-drill-genesis-salt`)、複数 process。producer 1 と Panel の全 seat が同じ共謀者の鍵で動き、全 seat が Valid に署名する。
正直な検証者は **genesis 後に通常の `BondRegistered` で登録した bond 1 つ**で、Panel に入らず、自分で取得した登録 model のコピーと公開 RPC だけを使う。

**各 scenario で観測すること(公開 RPC から):** conviction の記録、producer と Valid 署名者の担保の減少、claim の void、producer の attempt 拒否(`ProducerFrozen`)、
報酬の流出 0(coinbase の出力)、weight 0(`finalWeight` / `safeWeight`)、settlement 深さと frontier に寄与しないこと。steady・reorg・restart・pruning・fresh IBD の各段で同じ観測を繰り返す。

| D | 注入する不正 | lane | 観測する終局 | 既存の注入手段 / driver | 不足 |
|---|---|---|---|---|---|
| D-01 | 1 step leaf の計算偽造 | legacy FP | outsider の conviction・slash・void・報酬 0 | `--palw-drill-tamper-fp-leaf`、`misaka-palw-shard-court-devnet-drill.sh` | 告発者が seat。genesis 後 bond の outsider と全 seat 共謀のモード |
| D-02 | 1 leaf の計算偽造 | legacy Attempt | 同上 | `--palw-drill-tamper-leaf`、`misaka-palw-t12-rcore-drill.sh`(D-6) | 同上 |
| D-03 | shard 境界の偽造 | TIR shard | 同上 | `--palw-drill-tamper-boundary` | 同上 |
| D-04 | 評価結果の偽造 | improvement eval | 同上 | `--palw-drill-tamper-eval` | 同上 |
| D-05 | TIR frame の cone 偽造 | TIR frame | 外部 owner の検出 → filing → conviction・seat S4・凍結 | なし | TFP1 producer の tamper 手段、owner / filer の自動運転の観測 |
| D-06 | 選択違反(greedy / seeded / V4 penalty・stop) | TIR frame・legacy FP | conviction | なし | tamper 手段、node の選択局所化(G-T2・G-05) |
| D-07 | 出力の差し替え | TIR frame・legacy | output door の conviction | なし | tamper 手段 |
| D-08 | prompt / 入力の差し替え | 全 lane | 入力違反の conviction | なし | tamper 手段 |
| D-09 | prefix state / KV の偽造 | staged・TIR | staged semantic court | なし | tamper 手段、G-18 の解消 |
| D-10 | tokenizer の不一致 | staged | tokenizer court | なし | tamper 手段、G-18 の解消 |
| D-11 | DA の差し控え・garbage・部分応答 | 全 lane | DA default・徴収 | なし | withhold / garbage の手段 |
| D-12 | 借用 trace・同一性の差し替え・carrier の改竄 | 全 lane | 受理拒否 | なし | 改竄 carrier を送る fixture |
| D-13 | owner 枠・DA 枠の先取り | TIR frame・native・legacy | 正直な検証者が締め出されないこと | なし | Sybil fixture(枠を埋める) |
| D-14 | private branch の Final を reorg で持ち込む | 全 lane | credit が残らないこと | `rfc0018_dag_drill.py attack`(Gate C 武装が前提で、出荷 preset では `Blocked`) | Gate C を使わない形の観測 |
| D-15 | 自己告発(producer の Sybil が filer) | 全 lane | 純損益が負 | なし | fixture |
| D-16 | 制裁前の退出・分割・再登録 | 全 lane | exit hold・凍結が効く | なし | fixture |
| D-17 | 対象外の work(V5 画像、PanelDa、seeded Float32、job tail、flat、Kimi) | 対象外 | 受理拒否または報酬 0 | なし | 対象外 job ごとの fixture |
| D-18 | TFP1 を付けない TIR FP claim(G-03) | TIR | 修正後: 報酬 0 / 修正前: 旧経路で報酬を得ることの記録 | なし | fixture |

**既存の driver との対応:**

- `scripts/drills/rfc0018_dag_drill.py` の `attack` は「閉鎖 model / 自前 Panel」の credit を 6 指標で観測する(`adversary_metrics`、`:407-430`)。不正の注入と conviction の観測は無く、
  Gate C が無い ruleset では `Blocked` になる(`:418`)。G14 の drill は、この driver の段階構成(steady・reorg・restart・pruning・IBD)と観測関数を再利用し、`scenario_g14` として
  D-01〜D-18 の fixture と、conviction・slash・凍結の観測を足すのが最短である。
- `scripts/drills/misaka-palw-shard-court-devnet-drill.sh` は D-01 の形(tamper → 告発 → void → 担保の減少)を持つが、告発者は seat である。
- `scripts/drills/misaka-palw-t12-rcore-drill.sh` は heartbeat が conviction を運ぶこと(D-6)を持つ。
- **G14 専用の drill driver は repository に無い。** TIR frame・staged court・legacy の outsider を実 process で動かす driver が要る。Rust の node-E2E harness
  (`g14_kernel_route_e2e.rs` の `World`、`kaspad/src/palw_service_*_e2e.rs`)は in-process で、drill の代わりにならない。

## 6. 判定

### 6.1 主張は成り立つか

**f244ed92d の source では、DAA 14,000 の active plan について主張は成り立たない。**

- **P14-min(文書の推奨): 成り立たない。** 報酬を出すのは legacy Panel route の 2 lane だけで、48 cell のうち COVERED は 4、GAP は 16(計算不正 F01、FP の入力・状態・tokenizer・選択、
  期限 F12、容量 F13、全 Panel 共謀 F16、lazy verifier F17、集約責任 F22、対象外 F23)。対象外の画像・PanelDa の work は報酬を得る。Gate C は武装できない。
- **P14-g14(武装できる全部): 成り立たない。** TIR frame が加わるが、frame は任意で(G-03)、owner 枠は先取りでき(G-T1)、選択の不正は node が局所化せず(G-T2)、
  semantic stage と staged court は武装できない予約に依る(G-18)。legacy route は P14-min のまま報酬を出し続ける。
- **主張の核(1 人の外部検証者が、秘密 state なしに、多数決なしで conviction・slash・拒否まで届く)は、source に実装として存在する。** TIR 公開 frame の fold
  (`apply_conviction_v1` と Final floor・Gate C 待ち・exit hold・凍結)と、kernel route / EXEC の node 経路がそれである。ただし前者は未実行の fold 水準、後者は武装できず
  署名者を課金しない。

### 6.2 残っているもの

**ユーザー判断が要るもの:**

1. G-01: legacy route を DAA 14,000 以後どう扱うか((a) LG14-A / LG14-B を武装、(b) G14 経路の無い family の新規 claim から報酬・weight を外す、(c) Gate C で未認証にする)。GAP-80 の続き。
2. G-02: N4(`palw_public_evidence_reward_v1`)を DAA 14,000 で武装するか。
3. G-05: legacy FP の選択・constraint・prefix・tokenizer の job を、経路が武装されるまで受理拒否にするか。
4. G-03: TIR FP claim に TFP1 を必須にするか。
5. G-T1: K_owner の枠の回収規則(Lead 既定、ユーザー確認待ち)。
6. G-04 / G-11: Gate C と rule E を武装可能にするための RFC-0018 §12 の判断(U-1〜U-8)。
7. G-09: X3 の期限の値。

**code が要るもの(重大度順):** G-T1、G-03、G-05、G-18、G-T2、G-T3、G-T4、G-06〜G-08、G-10、G-K1、G-K2、G-X1、G-N1、G-N2、G-12〜G-15、G-T5、G-T7、G-16。

**未実行の試験:** §5.1 の全部(TIR frame の fold 試験 8 本と unit、kernel route / EXEC の node 試験、legacy の N\* 試験、Gate C の unit)。§5.2 の 17 本は未作成。

**drill:** G14 の drill driver が無い。§5.3 の D-01〜D-18 を、出荷する binary で、DAA 14,000 をまたいで行う必要がある。現状の注入手段は 4 種類だけ(G-17)。

### 6.3 この文書が変えないもの

code は変えていない。fence の値、wire、指紋は変わらない。本書の COVERED は source に部品と試験の source が揃っていることを示すだけで、verified でも armable でもない。

## 付録 A. 関連文書

- [g14-completion-matrix.md](../../../design/palw/integration/g14-completion-matrix.md) §2i〜§2k(条件 C1〜C10 と route 表)
- [g14-objective-conviction-remaining-2026-10-15.md](../../../design/palw/integration/g14-objective-conviction-remaining-2026-10-15.md)(R1〜R8 の checkbox)
- [legacy-route-g14-filer.md](../../../design/palw/verification/legacy-route-g14-filer.md)、[legacy-route-g14-held-da.md](../../../design/palw/verification/legacy-route-g14-held-da.md)(LG14-A / LG14-B)
- [rfc-0018-implementation-status-2026-10-15.md](../../../design/palw/integration/rfc-0018-implementation-status-2026-10-15.md)(Gate C)
- [mainnet-spec-register-2026-10-12.md](../../../design/palw/integration/mainnet-spec-register-2026-10-12.md)(X3・N4・T6・X4 と job version 表)
- [activation-readiness-matrix.md](../../../design/palw/integration/activation-readiness-matrix.md)(GAP-80・GAP-81、検証者誘因)
- [t12-daa14000-release-plan-2026-10-15.md](../../../design/palw/integration/t12-daa14000-release-plan-2026-10-15.md)
- [../../PRINCIPLES.md](../../../PRINCIPLES.md) §0・§6・§8・§9

## 付録 B. 追記 — cross-lane gates lane が変えた cell(2026-10-16、branch `integ/g14-close-gates`)

本文は書き換えない。以下は `2373782d7`(base `e71c69ff2`)で入れた変更と、それが動かす cell だけを記す。
新 fence はすべて全 preset で `None`(Some-only hash)。testnet-12 の 3 id は base と同一(shipped: params `5ee7fd8e…` /
identity `5de80e64…` / schedule `1678e073…`、launch: `b8564b88…` / `5de80e64…` / `93da24cc…`)。状態は「実装済み・fold 水準の
試験が通った」で、node E2E と fence をまたぐ drill は未実施(armable ではない)。

### B.1 gap ごとの処置

| gap | 処置(code) | 試験(`consensus/core/tests/g14_close_gates.rs`、実行済み・PASS) |
|---|---|---|
| G-10 | staged court / staged TIR route / staged Float32 / TIR conviction は、その network・高さを名指しする arm-readiness record(V1〜V7 の evidence・review・source commit)が pin されるまで `cannot be armed`。`PALW_G14_ARMING_RECORDS_V1` は空(`palw_g14_gates_v1.rs`) | T-N17 `armable_g14_fences_refuse_without_a_measured_record`、`an_arming_record_admits_exactly_its_network_and_height` |
| G-18 | staged court は `palw_legacy_public_filer_v1` が同じ高さ以下で有効であることを要求(切り離し案は不採用: 予約が Final hold と予約者ごと 1 row の上限を担うため) | T-N16 `staged_court_validation_requires_the_legacy_filer_fence` |
| G-02(N4) | 規則は不変。無条件拒否を record gate(T-N9 fold・node E2E・D-17 drill)に置換。未 pin のため依然 `cannot be armed` | T-N9 `n4_v5_image_and_panel_da_claims_take_no_new_reward`(fold: V5 画像・PanelDa は claim gate で拒否、公開 V4 は従来どおり、fence 前は旧規則) |
| G-01(b)・G-05 | 新休眠 fence `palw_g14_route_required_v1`(ユーザー決定 2026-10-16 で DAA 14,000 の予定 entry)。route は fence state(mirror)から読む | T-N8 `legacy_route_claim_without_a_g14_path_earns_nothing_past_14000`、T-N10 `legacy_fp_selection_and_constraint_jobs_are_refused_until_their_route_is_armed` |
| G-08 | 新休眠 fence `palw_reward_liability_horizon_v1`: legacy FP claim の Final floor = max(従来, licence + window_challenge + window_court)(`palw_claim_final_floor_v1` 内) | T-N15 `certified_and_final_rewards_are_held_until_the_liability_horizon`(fold: licence 1,004 で Final floor 1,124 → 4,124、その間 receipt spend は拒否) |
| G-15 | 同 fence: Gate C の certified reward は final + window_court を過ぎた sweep か、それより早い retirement 時に 1 回だけ、認証が残る場合のみ支払う。kernel route の FinalReward は kernel lane の所有で未変更 | T-N15(述語と source 検査。Gate C は武装不可のため fold 実行なし) |
| G-09 | X3 候補 `candidate_from_horizons_v1`: T_prepare = bind + receipt、T_observe = window_challenge_at、T_resolve = disclose + court。testnet-12 @14,000 で 1,200 + 120 + 4,200 = **5,520 DAA**(既存 retention 5,400 より 120 長い)。B_cold_fetch・B_check・B_localize・B_carrier・B_reorg_slack・c_retain は未測定(0 = UNKNOWN)。武装は拒否のまま | `x3_candidate_is_derived_from_existing_horizons_and_stays_refused` |
| G-16 | consumer v2 でも Kimi router を `RefusedAtAdmission` に統一(`palw_cell_coverage_v1.rs`) | `kimi_is_out_of_scope_in_both_coverage_dispositions` |

### B.2 option (b) を testnet-12 の DAA 14,000 で武装したときに変わること

DAA 14,000 以後に**受理される** claim だけが対象(それ以前の claim は旧規則で Final・receipt spend・vesting まで進む)。
arm-readiness record の要件は T-N8・T-N10(fold)、node E2E、fence をまたぐ drill で、経済の測定値は要しない(報酬を外すだけ)。

- **報酬・weight・credit を失う family(route が武装されるまで):**
  - V2 Attempt の全 class(BASE-0 floor、genesis model class、TIR v1・gen v1・shard class の attempt)。class gate が
    `G14_ROUTE_ABSENT` で拒否。producer の pre-check も同じ答えを返す。自 block の attempt が拒否されると、G14-for-rewards と同じく
    block は不正になる(skip すると carve が未検証のまま払われるため)。merge された attempt は skip。復帰条件: LG14-A
    (`palw_legacy_public_filer_v1`)と LG14-B(`palw_legacy_held_da_v2`)の両方が武装されること(現在どちらも武装不可)。
  - Free Prompt の全 job version。frame(TFP1)の無い claim は selection の route を持たない。
- **受理時に拒否される job version(carrier は通常 fee だけ払い、object は落ちる):**
  - FP V6 constraint(6)、prefix state(11)、prefix inherit(12): `G14_JOB_VERSION_UNJUDGEABLE`。
  - FP V4(7): `G14_ROUTE_ABSENT`(selection と tokenizer の route が無い)。
  - FP V5(8)と PanelDa: `G14_ROUTE_ABSENT`。公開証拠が無いので route は存在し得ない(N4 を武装すれば N4 の code が先に出る)。
  - FP V3(5)は DAA 5,300 から既に拒否。
  - FP の復帰条件: `palw_tir_public_conviction_v1`(selection・計算・DA)と、staged court + legacy filer(constraint・prefix・
    tokenizer)が武装され、かつ commitment 自身が TFP1 frame を持つこと。
- **変わらないもの:** 評価 claim(9、利用者払い・報酬経路外)、tensor gen(10、weight なし)、heartbeat、休眠の kernel・native・EXEC。
- **注意(drill で確認すること):** route が 1 つも武装されていない DAA 14,000 では、新しい PALW claim が 0 になり safe weight が増えない。
  block 生成は heartbeat lane に依る。

### B.3 行列で動く cell(本文 §3 の判定を、各 fence を武装した場合について上書きする)

- **A 列(P14-min)に (b) を加える場合:** V2 Attempt と Free Prompt の 48 cell は、DAA 14,000 以後に受理される claim について全部
  OUT-OF-SCOPE になる(受理時に拒否、報酬・credit なし)。n/a の 3 cell は n/a のまま。新規 claim についての集計は
  0 / 0 / 0 / 165 / 3。14,000 前に受理された claim は、settle するまで §3.2・§3.3 の判定のまま。
- **Free Prompt F23:** GAP → OUT-OF-SCOPE((b) か N4 を武装した場合)。
- **Free Prompt F12(G-08):** liability fence を武装した場合、GAP → PARTIAL(支払いは旧来の反転期限の後。Final 後の反転は retirement まで残る)。
- **B 列(P14-g14):** G-18 と G-10 の gate により、この binary で staged court・TIR conviction を武装できない
  (record 未 pin、staged court は武装不可の legacy filer を要する)。P14-g14 は作れず、TIR frame と staged court の列は A と同じ OUT-OF-SCOPE。
- **Gate C(G-15):** 認証済み reward の支払いが責任期限の後になる(休眠、Gate C 自体が武装不可)。
- **G14 from genesis(test-only `palw_g14_genesis_test_params_v1`):** 隔離 genesis で validate が通る。block 0 から attempt と
  未 frame の FP は拒否、V5・PanelDa は常に拒否。frame 付き FP は TIR route が解決する高さ(testnet-12 の前提 fence により
  DAA 1,700)から受理される。LG14-B と X3(承認値なし)は載せていない。

### B.4 残り

option (b)・N4・liability の node E2E と fence をまたぐ drill(record の evidence)、G-10 の V1〜V7、Gate C を通した G-15 の fold 実行、
kernel route の FinalReward(kernel lane)、LG14-A / LG14-B 自体の武装可能化(option (a) の経路)、X3 の未測定項目とユーザー承認。

## 7. 追補: TIR 公開 frame lane の close-out(2026-10-16、branch `integ/g14-close-tir`)

本書の本文(§0〜§6)は書き換えない。この節は TIR 公開 frame lane(§3.7)で変えた cell と gap の状態だけを記録する。
**検証の水準は fold(consensus-core の lib test)と node 側の unit である。node 水準の E2E(T-N1)は書けていない(下の「T-N1 が書けない理由」)。**
状態語は §0 のとおり(COVERED = source と fold 試験が揃う。verified / armable ではない)。

### 7.1 gap の状態

| ID | 状態 | 変更(要点) |
|---|---|---|
| G-03 | **閉(fold)** | fence 以後、登録 TIR class の FP commitment は transaction で対になった `TFP1` が無ければ受理時に拒否(`G03_FRAMELESS_TIR_ORIGINAL`)。frame が受理しない形(job tail、flat scheme、`PanelDa`、制約 job で semantic 契約なし、開けない semantic stage)は frame を作れないので、受理されない。判定は fence 自体(`tir_public_conviction_active`、params から)で、rules が解決したかではない。`palw_tir_public_fold_v1::require_original_frame_v1`、呼び出し `objects.rs`(N4 の直後、価格付けと書き込みの前)。 |
| G-T1 | **一部閉(fold)** | K_owner = 8 は維持。slot を持つ owner が native の grace(`reorg_retention_daa`、最低 inclusion margin)の間 progress(Open・Demand・回答の Finish・Collect、`spent.last_progress_daa`)が無ければ idle とし、上限時の Open が最も長く idle な slot を回収する(その row は退役: 未払い demand を閉じ turn を使い切る。deposit は `slash_bond` で焼却、誰にも払わない)。未払いの demand を応答時計内に持つ owner と、遅い回答を受けた直後の owner は回収されない。**残る穴:** 能動的に demand を続ける共謀 owner(turn 予算内)は slot を保持できる(経済的な上限のみ)。grace 値の妥当性は未測定。 |
| G-T2 | **閉(fold + node 実装、node は unit のみ)** | node の owner は trace か step が食い違うと、まず完全な committed IDs を取得して replay の IDs と比較する。最初の不一致位置の trace tile を demand し、consensus-core `palw_tir_public_selection_probe_v1` が job 自身の規則(greedy / seeded / V4)で pin を組む。反証できればそのまま filer へ、反証できなければ(committed row が replay と違う)算術の降下へ戻る。根が全て一致しても replay の答えが停止規則に反すれば Output。 |
| G-T3 | **閉(fold)** | frame の設置時に claim の公開 prompt ids を ADR-0145 の prompt row に書き、material horizon(epoch 境界に切り上げ)まで保持する。derived work が書いた row は延長だけ。job の ids でないもの(`PanelDa` の空列を含む)は frame を拒否。canonical job は ids を持たず job から導出できるので row を書かない(node は `job_for_anchor` で導出)。 |
| G-T4 | **閉(fold + node unit)** | Valid credit / grant を持たない抽選 seat は outsider と同じ owner になれる(`independent`、`palw_tir_public_source_for_row_v1`)。owner になった seat はその claim で grant できない。node の owner は抽選 seat を skip しなくなり、seat が見つけた不正(`seat_faults`)の claim を先に追う。finding は同じ filer book に入る。 |
| G-T5 | **一部(node のみ)** | responder の同時 demand を 4 → K_owner(8)、受理済みの次の move は即送る(同じ move の再送だけ `COURT_MOVE_REPLAN_DAA` を待つ)。consensus の応答窓は同時性で伸ばしていない(行に閉じた不変条件を壊すため)。最大 profile での間に合いは未測定。 |
| G-T6 | **一部** | T-N2〜T-N7 と genesis 試験を fold で追加・実行。既存 fold 試験 4 本の壊れた fixture を直した。T-N1 は未作成。 |
| G-T7 / G-13(TIR 側) | **閉(fold)** | conviction は、false-Valid の offence が未消費の各 credited seat について、その claim の lock が存在し期限内であることを assert(無ければ filing ごと拒否、黙った skip なし)。grant 時に lock を `bond_exit_after_daa`(≥ door の閉鎖)まで延ばす既存処理に加え、door が開いている間は S2 licence の redraw、Panel-v3 の shard redraw、vertex の equivocation 没収が frame の credited lock を外さない(`palw_tir_public_frame_keeps_lock_v1`)。 |

### 7.2 §3.7 の cell の変更(P14-g14 の列。水準は fold)

| F | 旧 | 新 | 根拠 |
|---|---|---|---|
| F03 | PARTIAL | COVERED(fold) | G-T3: prompt ids を frame が保持。T-N5 |
| F05 / F06 | GAP | OUT-OF-SCOPE | semantic stage を持つ original は legacy filer fence が無い限り受理しない(G-18 を frame が要しない)。G-03 と合わせて受理されない |
| F07 | GAP | COVERED(fold)。node は unit | T-N2(greedy・seeded・V4 penalty・stop)、G-T2 |
| F13 | GAP | PARTIAL | G-T1(idle は回収、能動的な占有は残る)。T-N4 |
| F16 | PARTIAL | COVERED(fold) | 抽選 seat の経路(G-T4、T-N6)、各 seat lock の assert(T-N7) |
| F22 | PARTIAL | PARTIAL | lock の assert は入った。clamp(R9)と集約責任は不変 |
| F23 | OUT-OF-SCOPE(旧経路へ落ちる) | OUT-OF-SCOPE(受理拒否) | G-03。T-N3 |
| F08 / F12 | PARTIAL | PARTIAL | G-T5 は node 側だけ |

TIR frame lane の集計(B. P14-g14、fold 水準): 旧 9 / 9 / 4 / 1 / 1 → 新 12 / 8 / 0 / 3 / 1(C / P / G / O / n/a。F01・F04・F08…の node 水準は引き続き PARTIAL)。

### 7.3 T-N1 が書けない理由(重要)

**IR(TIR)class の Free Prompt lane は consensus でも node でも閉じている。** frame は TIR FP claim にしか付かないので、現状どの network でも frame を持つ claim は生まれない。
- node: TIR backend の `execute_free_prompt` は `TIR_FREE_PROMPT_CLOSED_V1`(RFC-0002 Phase H まで閉)を返す(`misaka/palw/tir/exec/src/node/backend.rs`)。RPC も同じ文言で拒否する(`rpc/service/src/service.rs`)。
- consensus: `palw_fp_derived_work`(t12 は genesis で武装)以後、FP の価格は class の `fp_work_profiles` を要し、無ければ `FreePromptClassHasNoWorkProfile`。この row は FP lane の認証(legacy の `PalwShapeProfileV3` を持つ object)でしか書かれず、IR class にその経路は無い。canonical claim(ids を運ばない)も同じ fence で `FreePromptPromptNotAccountable`。
- したがって「署名済み TFP1 → mempool → template → fold」の本番経路を通る T-N1 は、Phase H(IR class の FP 認証と work profile、TIR backend の FP 実行)が無い限り作れない。fold 試験は frame を直接設置して検証している。
- **帰結:** DAA 14,000 で `palw_tir_public_conviction_v1` を武装しても、t12 で報酬が frame を通って流れることはない。legacy route を (b) で止めると、Phase H が入るまで FP の報酬経路は無い。

### 7.4 実行した試験(この branch)

fold 水準の試験は署名を合成し、署名検証後の半分(`apply_verified_control_v1`)と credited frame の fixture(`with_custody_for_tests_v1`、cfg(test))を使う。
本番の受理経路(ML-DSA、carrier fee、mempool、template)は通らない。

- `cargo test -p kaspa-consensus-core --lib -- palw_tir_public shipped_presets_have_pinned_fingerprints`: `69 passed; 1 failed`。
  失敗 1 本は `palw_tir_public_source_v1::tests::flat_original_binding_does_not_acquire_tiled_trace_obligations`(flat scheme の source 試験。
  frame は flat を受理しない。この branch はその code path を変えていないが、base での結果は未確認)。
  T-N2〜T-N7、genesis 試験、既存の frame fold 試験 8 本、`shipped_presets_have_pinned_fingerprints`(t12 の指紋 pin)は ok。
- `cargo test -p kaspad --lib -- tir_public_owner palw_drill_tir_frame_tests tir_public_filer`: `5 passed; 0 failed`
  (T-N6 の node 半分、G-T2 の owner 判定、drill flag の parse、既存の filer 試験 2 本)。

### 7.5 武装に要る記録(測定値と判断。未測定は UNKNOWN)

1. **前提(判断): IR class の Free Prompt lane(RFC-0002 Phase H)。** これが無いと frame に claim が来ない(§7.3)。
2. owner の idle grace(`reorg_retention_daa` = 256 DAA)が、実 node の owner loop の往復(回答の受理から次の Demand の受理まで)より長いこと。
3. K_owner 本の同時 demand のもとで、最大 profile の回答(最大の unit body と `Ids` 本体)が応答窓(`acquisition_daa` + moves × inclusion margin)に収まること(G-T5)。
4. custodian の準備が最大 profile で preparation door 内に終わること(R7 / R8)。
5. 最大 vocab の decode-token pin(全 generated ids と 4,096 lane の tile 2 枚)が terminal witness ceiling と分割搬送に収まること。
6. ML-DSA 検査の反復を含む native CPU 予算(FullSUM、R7)。
7. tag 170/171 の A-2 wire pin を Rust 試験で確定し、native inventory 生成器を再実行して、手で直した 4 行(`PalwTransitionExtrasV1`、`PalwTirPublicSpentV1`)と一致させること。
8. V1〜V7 と drill D-05 / D-06 / D-07 / D-18 を、出荷 binary で fence をまたいで実行すること(D-05 / D-06 は 1 が前提)。
9. 能動的に demand を続ける owner による slot 占有の費用と継続時間の評価(G-T1 の残り)。

## 付録 C. lane 閉鎖で変わった行列 cell(2026-10-16、branch `integ/g14-close-lanes` @ `5bdd427fb`)

本文(§0〜§6)は監査時点 `f244ed92d` の記録として残し、書き換えない。この付録は §3.4〜§3.6 の「武装時」列のうち、閉鎖 commit で変わった cell だけを記す。
DAA 14,000 の P14-min / P14-g14 列(3 lane とも OUT-OF-SCOPE)は変わらない。新しい規則は各 lane 自身の fence(`palw_probabilistic_constraints_v1`、
`palw_exec_payload_v2`、native funded / Final27 / v39)の下だけで効き、全 preset で `None`・validation が武装を拒否する。`Params` の field は足していない
(t12 の `palw_t12_flag_day_9000` 9 件と `shipped_presets_have_pinned_fingerprints` が通り、params id / identity は不変)。fence を DAA 0 にしても成り立つ。

| lane | F | 旧(武装時) | 新(武装時) | 変更 | 試験(水準・結果) |
|---|---|---|---|---|---|
| Kernel / OPV | F16 | PARTIAL(seat は無傷、G-K1) | **COVERED** | 数えられた passing receipt ごとに署名者責任 lock(kernel aux 143 `0x01`、ADR-0151 の seat lock 関係)。committed に入り責任期限まで退出不可、有罪で 1 回 slash、期限・timeout で解放(`palw_g14_lane_closure_fold_v1.rs`、`palw_kernel_route_fold_v1.rs:1237`・`:2085`・`:2148`) | T-N11 `kernel_route_valid_signers_are_charged_on_conviction`(node、本番 onboarding 経路で attestation hook なし、Final 前・Final 後・正直 claim の解放、replay)PASS |
| Kernel / OPV | F19 | COVERED | COVERED(seat にも退出保留) | 署名者 lock が V2 committed に入る(`palw_kernel_route_v1.rs:605`) | 同上 |
| Kernel / OPV | F08(v4) | PARTIAL(v4 の material は block body のみ、pruning 規則なし、G-K4) | PARTIAL(pruning 規則あり、v4 は報酬対象外のまま) | segmented claim に最初に応答した block を記録(aux 143 `0x02`)し、claim が有罪にできる間は pruning point がそれを越えない(`processor/palw.rs:815`) | node 回帰のみ(下記)。専用試験は未作成 |
| EXEC slice | F16 | GAP(REAL root の V2 署名者を課金しない、G-X1) | **COVERED** | REAL claim 自身の piece(prefix claim)が反証されたら V2 Valid 署名者を S4(covering-signer funnel、1 回)。後続 executor の slice は V2 seat が署名していないので課金せず、その kernel claim と receipt 署名者(G-K1)が負う(`palw_exec_v2_fold.rs:1675`) | T-N12 `exec_real_root_v2_signers_are_charged_when_a_slice_is_proven_false`(fold、EXEC fence 0)PASS。`a_pending_prefix_holds…`(fold)を新規則に更新 PASS |
| native funded | F22 | GAP(担保不足で裁定拒否、G-14) | PARTIAL | v11 / v16 source / v29 / v39 の裁定・default は担保不足で拒否しない:void・依頼者へ escrow 返金・残る担保だけ徴収、finder は徴収額に比例(`palw_g14_lane_closure_v1.rs` の `palw_native_*`、各 fold の G-14 注記) | T-N13 `native_conviction_is_not_refused_for_short_collateral_v29` / `_v39`(book 水準)PASS。fold / node 水準は native の harness が repository に無く未作成 |
| native funded | F18 | PARTIAL(finder ≤ 1000‰) | PARTIAL(finder ≤ 490‰) | native policy の finder 取り分を ADR-0032 の 49 % 以下に制限(`palw_flat_execution_v11.rs:1015`)。正直な finder の誘因は G-K3 と同じく未解決 | unit `g14_lane_closure_values_come_from_documented_relations` PASS |
| native funded | F13 | GAP(4 枠の先取り、G-N1) | PARTIAL | v39:期限切れ owner が枠を返す(`palw_native_protected_fold_v39.rs:250`・`:601`)。v11 / v29 は未解決(下記) | 試験なし(native harness 無し) |

集計(§3.9 の C 行、武装時の参考):Kernel / OPV 11/8/2/2/1 → 12/7/2/2/1、native funded 0/17/4/2/1 → 0/19/2/2/1、EXEC slice 13/5/3/2/1 → 14/5/2/2/1。

**閉じていないもの:** G-N1 の v11 / v29(4 枠は producer が前払いした回答義務の上限でもあり、kernel quota の subject 計数と lifetime catalogue の再設計が要る)、
G-N2(v39 mining claim の Final 後の公開告発:job quota interval を越える告発の搬送、material 保持、attestor の責任額、veto は RFC-0018 U-6 のユーザー判断)、
G-K2(EXEC REAL root の seam `exec_v2_test_admit_class_v1` は GAP-81 のまま。kernel の Panel 許可 class は T-N11 が seam なしで通す)、
G-K3(検証者誘因 P0:readiness matrix の案 A / B の選択はユーザー判断)、native の fold / node 試験。

## 付録 D. 追記 — legacy route を閉じる lane が変えた cell(2026-10-16、branch `integ/g14-close-legacy`)

本文と付録 B は書き換えない。以下は `fdb5d41cf`(code)と `acb7dd346`(node E2E)、base `b52cb2892` で入れた変更と、それが動かす cell だけを記す。
新しい規則はすべて休眠 fence の下にある(全 preset で `None`、`PALW_G14_ARMING_RECORDS_V1` は空)。testnet-12 の ids は base と同一
(launch `b8564b88…` / `5de80e64…` / `93da24cc…`、shipped `5ee7fd8e…` / `5de80e64…` / `1678e073…`)。状態は「実装済み・fold と node E2E が通った」で、
測定と fence をまたぐ drill は未実施(armable ではない)。

### C.1 gap ごとの処置

| gap | 処置(code) | 試験 |
|---|---|---|
| LG14-A / LG14-B の武装 | 無条件の拒否を arm-readiness record の gate に置換(`PalwG14GatedFenceV1::LegacyPublicFilerV1` / `LegacyHeldDaV2`、要件 `PALW_LEGACY_FILER_ARMING_REQUIREMENTS_V1` / `PALW_LEGACY_HELD_DA_ARMING_REQUIREMENTS_V1`、測定値を要する)。LG14-B は ConsensusV2 と、同じ高さ以下の LG14-A を要する。`Params::validate_palw_v2_with_arming_records_v1(records)` は pin の列だけを差し替えた全 validation(node は `validate_palw_v2` = pin の列で検査する) | `lg14_arming_needs_a_record_naming_the_network_and_height` |
| G-06 | reservation の Final hold と `DirectProofFirst` は既存(LG14-A)。追加: live な reservation を持つ非 seat は、先に開いた decoy court の横で自分の held dissection を 1 本開ける(`check_further_held_dissection_v1`、seat の `1 + seat_count` 枠は消費しない。fence 未満では reservation が存在しないので判定は不変)。`DirectProofFirst` は独立 fence にしない: RFC-0014 §7.3 の AND の片側で、hold なしでは時計に、それなしでは decoy court に負ける。option (b) は両方を要するので、分けても G14 でない武装状態と record が増えるだけ | `g06_a_reserver_opens_its_dissection_beside_a_decoy_court_and_holds_final`(fold)、node E2E の Final 保留 |
| G-07(DA 側) | 既存の予約別予算(予約ごと 34 session、bond ごと live 64 / retained 64、DA-8 の共有予算を読まない)で閉じている。code の追加なし | `g07_a_coalition_cannot_spend_the_outsiders_budget` |
| G-07(court 上限) | **閉じていない**(C.3) | `g07_residual_the_network_court_cap_is_shared`(残余を固定) |
| G-13 | LG14-A 以後に受理された claim の Valid lock の寿命を `window_court`(責任期限 `F + window_court` = liability row の期限)にする(`PalwStateParamsV2::valid_lock_life_for_claim_v1`。licence、supplementary、Final の 3 か所と F2-lock の遡及短縮)。依拠された seat の slashable lock を失った kind-3 conviction は拒否(`palw_g13_relied_lock_lost_v1`) | `g13_a_valid_lock_lives_to_the_liability_horizon`、node E2E の seat slash |
| Free Prompt | TIR frame で回復できる family に残っていた LG14-A への依存は「LG14-A が武装不可」だけ(G-18)。record gate で解消。G-05 の job version は option (b) のまま拒否 | `fp_recoverable_families_have_no_remaining_lg14a_dependency` |
| G14 from genesis | `palw_g14_genesis_test_params_v1` に LG14-B(DAA 0)を追加。gates lane の genesis 2 試験を更新(V2 Attempt は block 0 から route あり、LG14-B を外すと拒否) | `g14_close_legacy_from_genesis`、`g14_genesis_*`(gates) |

### C.2 動く cell(§3.2 V2 Attempt、LG14-A + LG14-B を武装した場合)

P14 の `2 / 15 / 5 / 0 / 2` → `10 / 11 / 1 / 0 / 2`(C / P / G / O / n/a)。

- GAP → COVERED: F12(Final 保留、直接証明、予約者の dissection)。
- GAP → PARTIAL: F01、F13、F16(残りは fused leaf の嘘が依る court 上限 = C.3。非 fused の全 seat 共謀は node E2E で conviction・slash まで届く)。
- PARTIAL → COVERED: F02・F03・F04・F07・F10(binding と局所化を予約の自前予算で得られる)、F08(held unit の沈黙は DA default、node E2E)、F19(G-13 で lock が責任期限まで残る)。
- 変わらない: F05(2M held class は回答不能のまま)、F14(GAP-B12)、F15(rule E、G-11)、F17(lazy verifier の誘因、trap は経済値を要する)、
  F18(報奨の先取り、ADR-0032 の残余)、F20(producer の model 保有を chain で審査しない — PRINCIPLES §9 の凍結前提)、F21(証拠が N\* 試験のみ)、
  F22(bond 式 v2 = G-12、測定値を要する)、F23(gen v1 画像 class の attempt)。

Free Prompt(§3.3)は option (b) により DAA 14,000 以後の新規 claim では OUT-OF-SCOPE のまま(付録 B.3)。frame 付き FP の route に LG14-A 由来の欠けは残らない。

### C.3 閉じていないもの

- **G-07 の court 上限。** `palw_max_concurrent_court_sessions_v1`(testnet-12 で 42)は 1 block 1 court move × rung window の積で、超えると正直な mover が rung を落として default になる。
  bond ごとの予算や分割は複数 bond の連合を止めず、撤廃は slot 保証を壊す。閉じるには、測定した block あたりの court 予算(上限がそれに比例する)か、slot × 時間の価格(burn)が要る。
  影響は fused leaf の嘘だけで、その間も reservation が claim を hard deadline まで保留する(Final・報酬・weight なし)。
- **G-12 / F22**(bond 式 v2 の値)、**F17**(検証者の誘因)、**F14**(GAP-B12)、**F15**(rule E)。
- **G-13 の capacity。** LG14-A 以後の claim では lock が `F + 1,000` から `F + window_court` に伸びる(2026-09-26 の V02 の短縮はこの claim には適用しない)。Panel の資本効率への影響は record の V5 で測る。

### C.4 実行した試験(2026-10-16、この branch、targeted のみ)

- `cargo test -p kaspa-consensus-core --test g14_close_legacy`: **8 passed**(arming、FP 依存、G-06、G-07、G-07 残余、G-13、genesis、t12 identity)。
- `--test g14_close_gates`: **12 passed**、`--test lg14a_legacy_dispute_fold`: **9 passed**、`--test t12_aheld_same_leaf_decoy`: **1 passed**。
- `cargo test -p kaspa-consensus --lib -- lg14a_legacy_filer_e2e::`: **14 passed**(新規 `legacy_v2_attempt_full_collusion_outside_bond_convicts`、
  `legacy_v2_attempt_held_da_withholding_ends_in_da_default` と既存 12 件)。全 seat 共謀の E2E では 5 seat 全員が liable で slash された。
- 実行していない: workspace 全体、clippy、kaspad(`palw_legacy_held_e2e` など LG14-B の kaspad 試験)、lib unit test。

### C.5 DAA 14,000 で LG14-A / LG14-B を武装するために要るもの

- record: `LegacyPublicFilerV1`・`LegacyHeldDaV2`(testnet-12 genesis、height 14,000)と option (b) `G14RouteRequiredV1`。各 requirements の evidence digest、review、source commit を、reviewed change で
  `PALW_G14_ARMING_RECORDS_V1` に pin する。
- 測定(V5): class ごとの hard-deadline envelope(GAP-07)、descent の round 数・answer bytes・34 session 予算、lock 延長下の Panel capacity、court 上限の競合。
- drill(出荷 binary で 14,000 をまたぐ): D-02(legacy Attempt の 1 leaf 偽造を、genesis 後 bond の outsider が全 seat 共謀下で conviction・slash)、D-11(held unit の差し控え → DA default)、
  D-13(Sybil の予約・session・decoy court の先取り)、D-16(制裁前の退出: lock が責任期限まで残る)。14,000 前に受理された claim は旧規則で settle すること。

## 8. 追補: IR class の Free Prompt lane(RFC-0002 Phase H、2026-10-16、branch `integ/rfc02-ir-free-prompt`)

本文(§0〜§7)は書き換えない。この節は、§7.3 と §7.5 の 1(IR class の Free Prompt lane が無いので frame に claim が来ない)に対して入れた変更と、
それで動く cell だけを記す。設計は [phase-f-integration §6](../../../design/palw/tir/phase-f-integration.md#6-phase-h--the-free-prompt-lane-of-an-ir-class-2026-10-16)。
状態は「実装済み・fold 水準と node 側 unit の試験が通った」。node E2E(T-N1)と drill は未実施で、armable ではない。

### 8.1 入れたもの

- **fence は新設しない。** lane は `palw_tir_public_conviction_v1`(前提 fence を含めて解決した高さ)が効いている所でだけ開く。
  G-03 により、開いた lane の IR FP claim はすべて TFP1 frame を持つ。fence 未満では従来どおり `FreePromptClassHasNoWorkProfile` で拒否する
  (byte-identical。新しい object kind・state table・Params field・指紋の入力は無い)。
- **work は admitted program から導く。** `tir_classes` の program と、frame が要求する `TirLayout` row から、step space の閉形式で leaves を数え、
  新しい位置の構造的 work vector で compute を数える。compute 時代だけで値付けし、leaves 時代は名前付きで拒否する。
- **lane の認証は被覆。** 到達する全 primitive を 1 つの認証済み family が覆う IR class だけが FP の認証 gate を通る(attempt lane と同じ規則)。
- **node と producer。** TIR backend の FP 実行は integer engine を使い、選択には decode-token door と同じ規則(`select_v1`)を使い、decoder が止まる所まで回す。
  drill の動詞は D-05 と D-06。header を export し、IR root で commitment を組み立てる。fence 以後、IR class の node 自身の claim は prompt ids を運ぶ。

### 8.2 §3.7 の cell と §7.3 への影響

- **§7.3 の consensus 側の閉塞は解消した(fold 水準)。** 実 ML-DSA-87 の source pledge を持つ TFP1 付き IR FP claim が、本番の transition
  (`apply_palw_transition_v2_with_extras`)で値付けされて受理され、frame が設置される。そのとき lane が読む fence はすべて DAA 0。
- **cell の判定は変えない。** frame lane の cell(§7.2)は frame が設置された claim についての判定で、その水準は fold のままである。
  ただし、§3.7 の各 cell の前提であった「frame を持つ claim が存在し得る」が、fold と node unit の水準で成り立った。
- **新しく見つけた GAP(frame lane の所有、未修正): frame の native allocation が、testnet-12 の Panel 規模で u64 の checked 算術を溢れる。**
  network の staged config と ladder 2^22 のもとで、corpus の全 class(step leaf 52〜222、vocab 24)が frame を作れるのは custodian 上限 2〜4 までである。
  上限 6(5 seat の Panel と producer)では `required-family sum overflows maximum_parser_steps` または
  `native TIR allocation exceeds u64` で拒否される。G-03 があるので、このような class は fence 以後 FP claim を一切取れない。
  実 model(leaf 数が桁違いに多い)では、なおさら frame を作れない見込みである。
  試験 `tir_frame_allocation_overflows_at_testnet12s_panel_gap_pin` がこの gap を固定している(修正したら反転する)。
  fold 試験が通るのは、court の tiny fixture(leaf 数がさらに小さい)だからである。

### 8.3 実行した試験(この branch、実行済み)

- `cargo test -p kaspa-consensus-core --lib -- tir_fp_lane`: `6 passed; 0 failed`(fold 3 本、unit 3 本)。
- `cargo test -p kaspa-consensus-core --lib -- palw_tir_public fp_compute_pricing derived_work palw_fp_execution_v3 palw_fp_objects_v3 shipped_presets_have_pinned_fingerprints palw_tir_court_v1 free_prompt`:
  `136 passed; 1 failed`。失敗した 1 本は §7.4 で既に失敗と記録された `flat_original_binding_does_not_acquire_tiled_trace_obligations`
  (`job context shape`)で、この branch はその source を変えていない。base での再確認はしていない。
  `shipped_presets_have_pinned_fingerprints`(t12 の指紋 pin)は ok で、testnet-12 の identity は変わらない。
- `cargo test -p misaka-palw-tir-exec --features node --test free_prompt`: `5 passed; 0 failed`。内容は次のとおり。
  - honest run(11 class): frame の完全な allocation が通り、derived leaves が capture と一致し、door は fault を出さない。
  - drill: selection の嘘 11 件をすべて door が conviction した。
  - V4 の stop: decoder が止まる位置まで回す。
  - lane 外の job は拒否する。
  - 上の gap pin。

### 8.4 T-N1 の状態

**T-N1 は未作成で、end-to-end では動いていない。** 残っている障害は次のとおり。

1. 上の allocation の GAP。testnet-12 の Panel 規模では、corpus の class でも frame を作れない。
2. 本番経路の harness。kaspad の `palw_service_public_e2e`(public bond service → PQ mempool → template → consensus)に、次を足す必要がある。
   - IR class の登録(`ClassRegisteredTirV1`)と registry の lifecycle を `Active` まで進めること
   - TIR family の認証(`FamilyCertified`)と `ClassLaneCertifiedTirV1`
   - layout の公開(tag 169)
   - frame とその前提 fence を test で武装すること(staged court が要求する legacy filer は検証で武装を拒否されるので、検証を迂回する)
   - 共謀する seat の `TirPublicGrantReadyV1`
   - 外部 owner と filer の service 実行
3. Gate C の待機の観測。Gate C はこの binary では武装できない。

### 8.5 DAA 14,000 で frame とこの lane を武装するのに残るもの

- 8.2 の allocation GAP を frame lane が修正すること。
- T-N1。
- `palw_legacy_public_filer_v1` の武装可能化(G-18。frame の前提である staged court の前提)。
- G-10 の記録 V1〜V7。この lane の evidence(上の fold 試験と node 試験、T-N1)を加える。
- drill D-05、D-06、D-07、D-18。genesis 後に登録した IR class で、出荷 binary を使い、fence をまたいで行う。
- 未測定の値(UNKNOWN): 最大 genesis profile での generic backend の FP 実行時間と bind window の比較、§7.5 の 2〜6。
- node が class semantics の runtime id を chain から読むこと(`with_fp_class_facts_v1` は未接続)。

## 9. 追補: frame allocation の u64 溢れの修正(2026-10-16、branch `integ/g14-tir-alloc`)

§8.2 の GAP(frame の native allocation が testnet-12 の Panel 規模で u64 を溢れる)を直した。この branch は fence を武装しない。

### 9.1 testnet-12 の custodian 上限は 6 ではなく 257

testnet-12 は `palw_tir_shard_v1` を武装している(`PALW_T12_INT11_FENCES_V1`、DAA 5,300)。
そのため、登録された TIR class について `staged_registered_tir_maximum_custodians_v1` が返す上限は
max(seat 数, `MAX_STRATIFIED_SEATS_V1` = 256) + 1 = **257** である。V2 Panel の 5 seat と producer を足した 6 ではない。
§8.2 の「上限 6」はこの点で過小だった。shard licensing(×1024、上限 5,121)は testnet-12 では休眠している。

### 9.2 原因(`derive_tir_public_work_allocation_v1`)

1. **custodian ごとの積が和であるべき所で積になっていた。**
   全 lane を外部 owner の lane(全 slot の answer を読むので slot 数に比例する)で値付けし、claim 全体をその slot 倍にしていた。
   正しくは、owner の lane 1 本と custodian ごとの lane(自分の slot の answer だけ)の和である。
   claim 全体の cold preparation も owner の allocation を複製していた。preparer は他の slot の answer を読まない。
2. lane の work に「全 actor の preparation 数」(`slots × passes`)を掛け、`maximum` でさらに slot 数を掛けていた。
3. acquisition の `maximum_stream_bytes` を answer の byte として読んでいた。これは保管・運搬の envelope(生 copy 8 個、move ごとの row journal)である。
   しかも turns に再取得が含まれているのに、さらに `passes` を掛けていた。
4. 16 KiB の WorkDebit slice を、replay の RAM traffic を含む全 IO で数え、`passes` を二重に掛けていた。
   node が WorkDebit を切る単位は、論理 unit(leaf、tile 行、1024 ID、replay 位置)と、durable byte の 16 KiB(読んだ answer、各 pass で公開する prepared generation)である。

`original_controls` は accepting action ごとに frame 全体を検証し、その費用を frame(∝ 上限)× action 数で数える。
action 数が上限の 2 乗に比例していたので、SUM は上限の 3 乗で増えていた。修正後は、`original_controls` の注記どおり 2 乗である。

### 9.3 修正

- `PalwTirPublicWorkAllocationV1` に 2 つの field を足した(generated の inventory 2 行を更新した)。
  - `per_custodian`: literal custodian の lane。
  - `prepared_bytes`: 1 pass の prepared generation の byte 数。`tir_public_prepared_bytes_v1` で求め、executor plan と共有する。
- `maximum` は lane の和である(`per_actor + per_custodian × maximum_custodians`、`lanes_sum_v1`)。
  owner の fold は、signer が owner なら `per_actor`、custodian なら `per_custodian` で上限を判定する。
  したがって、共謀した custodian が owner の lane を使うことはできない(既存試験を強化した)。
- claim 全体の preparation は `preparation_lanes_v1` を使う。全 preparer が custodian の lane を持つ。
  preparation contract は `per_actor == per_custodian` と `maximum == lanes_sum` を検査する。
- preparation の slice 数 = 論理 walk × passes + prepared generation の 16 KiB slice × passes × 2(書いて再度開く)。
  preparation の締切は、preparer 1 人の直列時間(slice ごとに 1 inclusion margin、最後に Ready)である。
  `capture_debit_v1` は `maximum_slices_per_actor` を超える grant を拒否する。
- saturating 演算と u128 の切り詰めは使っていない。checked 算術のままで、項の数え方だけを直した。

### 9.4 cell(`tir_frame_allocation_holds_at_testnet12s_custodian_upper`、gap pin を反転)

上限は、出荷 bundle に対する fold 自身の resolver から取る(DAA 14,000 で 257 と assert)。条件は network の staged config と ladder 2^22。
corpus の 11 class(step leaf 52〜222)はすべて、上限 1〜6、33、257 で `Complete` になった。主な値(SUM、`accepted_daa` からの差):

| class | n | 上限 | parser | rent(sompi) | preparation_until | collect_until |
|---|---|---|---|---|---|---|
| moe-top2-shared | 52 | 6 | 1.92e15 | 1.99e15 | +9.07e5 | +6.33e7 |
| moe-top2-shared | 52 | 257 | 2.36e18 | 2.44e18 | +9.07e5 | +1.70e9 |
| dense-gqa-2layer | 118 | 6 | 4.04e15 | 4.19e15 | +2.03e6 | +1.31e8 |
| dense-gqa-2layer | 118 | 257 | 4.98e18 | 5.16e18 | +2.03e6 | +3.56e9 |
| gdn-k2-v4-grouped | 222 | 6 | 7.39e15 | 7.65e15 | +3.80e6 | +2.39e8 |
| gdn-k2-v4-grouped | 222 | 257 | 9.14e18 | 9.47e18 | +3.80e6 | +6.50e9 |

- 上限 257 での最大は parser 9.14e18 で、u64 の上限 1.84e19 に対して約 2 倍の余裕しかない。
  支配項は、accepting action ごとの全 frame 検証(`owner_validation` と、controls の preparation 検証)である。
- 上限 1,025 と 5,121(shard licensing、testnet-12 では休眠)は依然 `native TIR allocation exceeds u64` になる(probe で確認した。assert はしていない)。
- 実 model(leaf 数が桁違いに多い)は上限 257 で溢れる見込みである。owner の lane は turns に比例し、turns は leaf 数と tile 数で増えるため。未測定。

### 9.5 新しい発見(設計判断が要る。未修正)

**frame は構築できるが、network_v1 の価格と時計では払えず、Final に届かない。**

- **rent。** `rent_per_parser_step` は 1 sompi なので、rent はほぼ parser step 数である。
  - 原 FP carrier が払う最低 rent(`palw_tir_public_minimum_rent_v1`)は、上限 6 で 2e15〜8e15 sompi(2e7〜8e7 MSK)、上限 257 で 2.4e18〜9.5e18 sompi(2.4e10〜9.5e10 MSK)。
  - 外部 owner の Open の rent(`owner_terms_v1`)は、corpus dense で上限 6 のとき約 2.2e15 sompi(2.2e7 MSK)。上限 257 では約 2.7e18 sompi。
  - G14 の「外部の 1 人が conviction できる」は、この費用を払えることが前提である。
- **時計。**
  - preparation_until は +0.9e6〜3.8e6 DAA。外部 owner の Open は `reserved_daa ≥ preparation_until_daa` を要する。
  - collect_until(Final の下限)は、上限 6 で +6e7〜2.4e8 DAA、上限 257 で +1.7e9〜6.5e9 DAA。
  - 原因は、WorkDebit と move を 1 件ずつ `inclusion_margin_daa` = 4 で直列に積む時計のモデルである(R7、未測定)。
- **preparer の負担。** 正直な preparer は、1 frame あたり 2.3e5〜9.5e5 件の WorkDebit を出す必要がある。
  prepared generation(最大 tile 長 65,536 での leaf 形式の上限で、2.8e7〜1.2e8 byte)を 16 KiB に分け、65 pass 分、書いて再度開くため。
- **帰結。**
  - DAA 14,000 で `palw_tir_public_conviction_v1` を武装しても、IR class の FP claim は frame の rent を払えない。
  - 払えたとしても、Final と決済に届かない。
  - T-N1 も、network_v1 の時計では node E2E で走らせられない。Open までに 0.9e6 DAA 以上を採掘する必要がある。
- **判断が要る点。**
  - 価格(parser step 単価、frame 検証を action ごとに 32 回 × 32 step/byte と数える係数)。
  - 時計(debit ごとに直列の inclusion margin を積むか)。
  - restart の回数(`reorg_replay_moves` = 64)。
  - いずれも未測定であり、この branch は値を作っていない。

### 9.6 実行した試験(この branch)

- `cargo test -p misaka-palw-tir-exec --features node --test free_prompt`: `5 passed; 0 failed`。
  gap pin を反転した `tir_frame_allocation_holds_at_testnet12s_custodian_upper` を含む。honest run は上限 6 で完全な frame を作る。
- `cargo test -p kaspa-consensus-core --lib -- palw_tir_public tir_fp_lane shipped_presets_have_pinned_fingerprints tir_preparation_is_claim_wide aux_registry_tests palw_native_resource`:
  `126 passed; 2 failed`。
  - 失敗 1: `flat_original_binding_does_not_acquire_tiled_trace_obligations`。§7.4 と §8.3 で既に失敗と記録されたもの。
  - 失敗 2: `ledger_expansion::…::flat_seven_and_nine_use_actual_nested_program_and_metadata_inventories`(kernel の `KernelLedgerV1` codec、`invalid Borsh option`)。
    この branch は kernel ledger の codec と table を変えていない。base での再確認はしていない。
  - `shipped_presets_have_pinned_fingerprints`(t12 の指紋 pin)は ok で、testnet-12 の identity は変わらない。
  - lane の試験 `protected_work_lane_exhaustion_does_not_spend_outside_owner_allocation` も ok。custodian が owner の lane 幅の debit を出すと拒否されることを、この試験に足した。
  - `tir_preparation_is_claim_wide_retained_and_rebuilt_without_an_owner` と frame の fold 試験も ok。

### 9.7 T-N1

**未作成。** §8.4 の障害 1(allocation)は解消した。代わりに、§9.5 の時計が新しい障害になった。

- network_v1 では、外部 owner が Open できるのは `preparation_until`(+0.9e6 DAA 以上)より後である。
  そこまでに、各 preparer は 2.3e5 件以上の WorkDebit を出す必要がある。
  これは node E2E(TestConsensus、PoW を省略、本番の template)で採掘できる量ではない。
- 小さい fixture config(restart 0、margin 1 など)を使えば数千 DAA に縮む。
  ただし、それは「preset の court 窓を保つ」という既存 harness の前提を外すことになり、測定値でもない。そのため、この branch では採らなかった。
- harness の残り(§8.4 の 2)は変わっていない。

## 付録 E. 追記 — court 容量と Panel 担保の lane(2026-10-16、branch `integ/g14-court-capacity`)

ユーザー方針([リリース計画](../../../design/palw/integration/t12-daa14000-release-plan-2026-10-15.md)「court の全体容量と Panel 担保」)の実装。新しい fence は `palw_court_capacity_v1` の 1 つだけで、全 preset で `None`。testnet-12 の identity は変わらない。

### E.1 court 容量(G-07)

- **fence と記録。** `palw_court_capacity_v1`(`consensus/core/src/palw_court_capacity_v1.rs`)。policy は測定記録 `PalwCourtMeasurementV1` から決定的に導出する(最も安い class の時間を 1 work unit とする)。
  - 武装の条件:LG14-A が同じ高さ以下、arm-readiness record(`PalwG14GatedFenceV1::CourtCapacityV1`)、その measured-values evidence が測定記録の digest であること。
  - placeholder policy(今日の 1 block 1 court 操作、ρ = 0)では、実ネットワークは武装できない。
- **admission。** court session を開くには次の 3 つを要する。
  - challenger 自身の live な LG14-A reservation(claim 固有の hard deadline と Final 保留)
  - Active で floor 以上の bond
  - 占有 escrow `ρ × (ticket 窓 + 最大 session) × weight`(A-6 の free half に置く)
- **block 予算。** 決定論的な cost unit(work・証拠 byte・working set)で数え、実行時の CPU 時間は使わない。
  - 課金の対象:one-move の告発と `CourtOpened` は variant で、dissection move・分割 close の完成 chunk・terminal 証拠 part は既存の slot 規則の述語で判定する。slot 規則が数えていた work は、fence 以後も 1 つも無料にならない。
  - 完成 chunk は、組み立てる close 全体の byte で課金する。
  - 受理側(`palw_accept.rs`)が落とし、fold は第 2 の鍵として拒否する。
- **時間料金。** 料金は `ρ × 占有 DAA × weight` で、admission 時に固定する。
  - 精算は終了時の実占有分だけで、escrow の残りは返す。
  - conviction・DA default の場合は、被告(producer)の担保から先に払う。正直な告発者の負担は 0。
- **スケジューラ。** 待機中の entry がある間は、空き枠でも直接 open しない。
  - 空いた容量は、block の先頭(どの object よりも前)で、claim の hard deadline が早い順に ticket として与える。
  - 新しい entry は、最悪ケース計算(占有者は全員 backstop まで、前の entry は ticket + 最大 session)で、自分と既存の全 entry が期限内に裁定へ届く場合に限って受ける。
- **option (b)。** WIP の方式を採った。family `LegacyFusedLeaf` は、`LegacyPanelAttempt` の条件に `palw_court_capacity_v1` の in force を加えたもの。
  - `fused_attention` の class の V2 Attempt は、court 容量の fence までは報酬なし(`palw_g14_route_attempt_refusal_for_class_v1`)。
  - 非 fused の class は、LG14-A と LG14-B だけで経路ありと数える(one-move で裁定でき、court 枠を使わないため)。
- **残る限界(安全側の扱いは未実装)。**
  1. 資金のある連合が、正直な追及が来る**前に**、待機列を claim の期限まで予約で埋められる。この場合、後から来た正直な追及は「保証なし」として admission で拒否される。待機中になった追及は守られるが、到着前の先取りを抑えるのは ρ の価格だけである。方針にある「新規の報酬対象 claim の受理量を制限する」は、まだ実装していない。
  2. 同じ claim に多数の bond が decoy の予約を置ける。各 decoy は枠と料金を消費するが、数を制限するものは料金しかない。
  3. one-move の告発は、session の move と同じ block 予算を使う。容量は「予算はすべて move に使える」と仮定して導出している。
  4. miner による block 内の順序・検閲は、範囲外である。

  このため、fused 経路は、drill の記録(V3 飽和・継続的な埋め直し)と上の 1 の扱いが揃うまで、「解決済み」としない。

### E.2 Panel 担保(G-13 の効率)

- `palw_panel_liability_v1.rs`:同じ bond の担保を `L_active` / `L_resolved` / accuser / V02 reserve に分けて読み、`R_new = max(0, min(C_work − L_active, C_total − L_active − L_resolved − R_other))` を計算する。
  - fold の work room は、常に `R_new(accuser + reserve)` 以上、`R_new(accuser)` 以下である。二重の担保にはならない。
- **規則の追加は 1 つ。** live な LG14-A reservation がある終端 claim の lock は Active に数える(`palw_claim_under_dispute_v1`)。
  - LG14-A より前は `legacy_disputes` が空なので、t12 の V02 の分割は byte 単位で同じになる。
- **validation。** LG14-A と `palw_final_lock_life` が両方ある場合、`palw_final_lock_full_collateral` を両者の遅い方の高さ以下で要する。
- **測定**(fold、t12 の launch ruleset、floor claim)。
  - floor claim 1 件:eligibility 640.17 MSK、Valid seat あたりの lock 160.09 MSK、bind → Final 122 DAA。
  - 担保 × ロック時間(claim 1 件・seat 1 席あたり):旧 179,456 MSK·DAA、新 499,627 MSK·DAA(×2.78)。
  - 定常で 1 席が支えられる claim 率(per DAA)。期待値は 1 席あたり、健全時 0.333(0.6 claims/DAA × 5/9)、10-01 のピーク時 2.944。

    | 規則 | genesis 席(939,063 MSK) | Panel floor 席(130,000 MSK) | 健全時に必要な最小担保 | ピーク時に必要な最小担保 |
    |---|---|---|---|---|
    | 旧(V02:F + 1,000、分割あり) | 3.724 | 0.327 | 131,395 MSK | 753,326 MSK |
    | 新(F + window_court、分割あり) | 1.589 | 0.140 | 238,119 MSK | 1,696,051 MSK |
    | 反実仮想(分割なし、validation で拒否) | 0.841 | 0.116 | 372,238 MSK | 3,288,102 MSK |

  - 実 fold の stream(3 DAA ごとに 1 件、6,000 DAA)で受理できた件数:

    | 規則 | genesis 席 | Panel floor 席 |
    |---|---|---|
    | 旧 | 2000/2000 | 1946/2000 |
    | 新 | 2000/2000 | 959/2000 |
    | 反実仮想 | 2000/2000 | 802/2000 |

  - **結論。**
    - 健全時の率は、genesis 席なら新規則でも 4.8 倍の余裕で支えられる。
    - 10-01 のピークは、新規則では genesis 席の 0.54 倍しかない。genesis bond 8 本のうち、ピーク率を支えられるものは 0 本である。
    - Panel floor の席は、新規則では健全時の率にも届かない(0.42 倍)。
    - 分割(Resolved を業務枠から外す)による処理量は、分割なしと比べて genesis 席で 1.9 倍、Panel floor 席で 1.2 倍になる。
- **late conviction。** F + 2,000(責任期間 F + 3,000 の内側。その前に anchor を 30 件 settle 済み)で、新規則は Valid の 5 席それぞれから 160.09 MSK を全額 slash した。旧規則は 0 だった。

### E.3 実行した試験(この branch)

- `cargo test -p kaspa-consensus-core --test g13_panel_liability`:**5 passed**(validation、late conviction、Active + Resolved ≤ C と同時の 4 件の詐欺、資本効率、restart/reorg)。追加した `a_live_reservation_keeps_a_final_claims_locks_active` も **1 passed**。
- `cargo test -p kaspa-consensus-core --test g14_court_capacity`:**10 passed、1 ignored**(timing harness)。
  - 内訳:arming、飽和と継続的な埋め直し、期限の早い entry の洪水、scheduler、fence なしで 43 番目が拒否されること、bond 分割、最大負荷の予算、restart/reorg、fused 経路、t12 identity。
  - 飽和の試験:連合が 42 枠を DAA 1,018 で占有し、最初の backstop は 4,018。正直な追及は 1,078 に ticket を得て、1,138 に裁定(hard deadline は 998,799)。
  - bond 分割:4 session の料金は、1 bond でも 4 bond でも 504,000,000 sompi(session あたり 126,000,000)。
- timing harness(`--ignored`、macOS aarch64 debug。**測定記録ではない**):
  - held opening(`ShardCourtAccused`、10,218 B):fold 33–136 µs/op。
  - 42 件の opening を入れた block:2.15–4.51 ms。
  - 42 session の default sweep:0.68–0.73 ms(空の block は約 0.63 ms)。
  - 部分記録の policy は、未測定の class があるため拒否される。
- 実行していない:workspace 全体、clippy、release build での測定。

## 11. 追補: P0 容量 — court 共有枠の飽和(G-07)と TIR owner 枠(2026-10-17、branch `integ/g14-court-capacity-r2`)

外部監査(`g14-external-audit-2026-10-17/rfc1_20_audit_ja.md` §6.1・§6.2・§7 P0(1)(2)・§8 C)と付録 E.1 の残る限界 1〜3 への対応。本文と付録 A〜E は書き換えない。

- 新しい規則はすべて既存の休眠 fence `palw_court_capacity_v1` の下にある(全 preset で `None`、記録は binary に pin されていない)。
- testnet-12 の ids は変わらない(`testnet12_identity_did_not_move`:launch `b8564b88…` / `5de80e64…` / `93da24cc…`、shipped `5ee7fd8e…` / `5de80e64…` / `1678e073…`)。
- 状態は「実装済み・fold 試験が通った」。測定記録・fence をまたぐ drill・node E2E の再実行は未実施で、armable ではない。

### 11.1 court 容量 round 2(E.1 の限界 1〜3)

| 限界 | 処置(code) | 試験 |
|---|---|---|
| 3. one-move の告発が session の move と同じ予算を使う | policy に `one_move_budget` を追加(測定記録の `block_one_move_ns` / `block_one_move_bytes` から導出)。one-move class はこの予算だけ、session の操作は `block_budget` だけを使う。容量は session class の work だけから導出する。受理側(`palw_accept.rs`)も同じ 2 本の累計で落とす | `g07r2_one_move_accusations_have_their_own_budget` |
| 1. 正直な追及の到着前に、連合が待機列を期限まで埋める | **reserve pool。** 容量の `reserve_permille` を、court-demanding claim(fence 以後に受理された fused-attention の V2 Attempt)の最初の court row 専用にする。claim ごとに一生で 1 回(`PalwDisputeClaimV1::court_reserve_used`)。reserve row は容量を理由に拒否されず、空いていれば開き、そうでなければ reserve の列(期限の早い順)に並ぶ。<br>**bounded admission。** 幅 W = ticket window + court window の hard-deadline bucket ごとに、in-flight の court-demanding claim を M = ⌊reserve 枠 / 2⌋ 件までしか受理しない。超える attempt は `CourtClaimAdmissionFull` で skip される(claim なし・報酬なし、block は有効)。producer の事前確認は `palw_court_claim_admission_v1`。<br>**spare 側。** それ以外の row は spare pool に入る(round 1 の保証つき scheduler)。保証が成り立たなければ拒否ではなく best-effort で受理し、保証つきの全 entry の後ろで ticket を与える | `g07r2_a_late_honest_reserve_row_is_never_refused_and_convicts`、`g07r2_the_reserve_bound_serves_every_first_row_by_its_deadline`、round 1 の 3 試験(spare pool に移した) |
| 2. 1 つの claim に多数の decoy 予約 | **価格。** 同じ claim の j 番目の同時 court row は、escrow と料金が j 倍になる。conviction 時は従来どおり producer の担保から先に払い戻す | `g07r2_a_late_honest_reserve_row_…` 内の assert(row 2・6・35 が base の 2・6・35 倍) |
| (安全側の締め) 処理されなかった追及 | **unserved = unpaid。** lapse の時点で、live reservation の court row がまだ待機中(queued または ticket)であり、その row が hard deadline の 1 service window(ticket + session)以上前に受理されていた場合、pre-Final の claim は `PanelUnavailable` で void になる。報酬・weight・credit なし、slash なし、deposit と escrow は返す。1 window より後に出された row は対象外で、griefing に使えない | `g07r2_an_unserved_prosecution_means_the_claim_earns_nothing` |

**保証の主張と、その根拠。**
- reserve row について。
  - 前提:bucket ごとに M = C_res/2 件以下、各 row の service は W 以下、非 preemptive、EDF、row は `hard − 4W` までに出される。
  - 主張:どの row も `hard` までに裁定に届く。
  - 需要と供給:幅 x の窓に期限を持つ claim は M·(⌈x/W⌉+1) 件以下で、供給は C_res·(x − W)/W 以上。x ≥ 4W − L、L = 4W で成り立つ。
  - 連合が他の claim の上で何を占有・予約・補充していても、reserve の枠には届かない(spare 側で起きることは reserve に影響しない)。
  - 試験:離散事象 simulation(C = 2 / 7 / 21 / 64、bucket 境界をまたぐ配置など 3 パターン)で遅延 0。最悪で `hard − 3W` に完了した(lead 4W のうち 1W しか使っていない)。負荷を 4M に増やすと遅延が出ることも確認し、simulation が失敗を検出できることを示した。
  - これは simulation による確認で、形式証明ではない。
- spare row について。
  - 拒否されない。
  - 保証つきの row は round 1 の最悪ケース会計で守られる。best-effort の row は守られない。
  - best-effort の row が 1 window 以上前に出され、それでも処理されないまま lapse に達した場合、その claim は報酬を得ない。

**fold 試験の数値(TEST policy。測定値ではない)。**
- 容量:42 枠(reserve 21 / spare 21)、W = 3,042 DAA、M = 10、L = 12,168。
- 8k 行の in-flight 上限 5 のため、late-honest の試験では 200 ‰(reserve 8、spare 34、M = 4)を使った。
- late-honest の試験:
  - 連合は bucket を埋め、5 件目の attempt は skip された。別 bucket の attempt は受理された。
  - 連合は reserve 3 件と spare 34 件を占有し、68 件を事前に積み、30 block の間、継続的に補充した。
  - 正直な outsider は DAA 1,055 に到着し、reserve row として即時に開いた。DAA 1,114 に conviction(hard deadline は 998,799)。正直な追及者の収支は 0 で、producer は slash された。
- spare pool(round 1 の試験を移したもの):
  - 飽和:DAA 1,100 に 21 枠が飽和し、正直な追及は 1,160 に ticket を得て、1,220 に裁定。
  - flood:20 件が先行し、21 件目は best-effort で受理された(拒否されない)。正直な追及は保証を保ち、1,160 に ticket、1,220 に裁定。
- 料金:session あたり 126,000,000 sompi(bond 分割の試験は不変)。

**reward の liveness(利用者の判断に関わる数値)。**
- bounded admission により、court-demanding claim の持続的な受理率は λ ≤ M / W = p·n·turn / (2W) に抑えられる。p = reserve 割合、n = block あたりの session 操作数、turn = 42。
- TEST / placeholder 相当(n = 1、p = 0.5):λ ≤ 0.0033 claims/DAA。これは健全時の 0.6 claims/DAA の 1/182。
- 健全時の 0.6 を fused class だけで受けるには n ≥ 174 session 操作/block(p = 0.5)が要る。
  - dissection move 1 回を node E2E の one-move と同じ 213 µs(開発機・debug)と仮定すると、block あたり約 37 ms。
  - **session 操作の class は未測定**であり、測定記録がない限り武装できない。
- 安全側には影響しない。足りない間に止まるのは新規の fused claim の報酬だけ。

### 11.2 TIR owner 枠(K_owner = 8、監査 §6.2)

- **変更した関数。** owner admission に限る(frame pricing・clock には触れていない)。
  - `palw_tir_public_fold_v1.rs`:`open_v1`、`reclaim_owner_slot_v1`、`apply_verified_control_v1` の Acquisition arm(待機中の row の Demand / Reacquire を拒否)、新規 `tir_owner_fair_start_v1`。
  - `palw_staged_court_fold_v1.rs`:新規 `staged_tir_owner_queue_v1`、`staged_tir_owner_admitted_v1`、`staged_tir_idle_admitted_owners_v1`。
  - `palw_tir_public_owned_v1.rs`:新規 `retire_admitted_idle_v1`、`stamp_release_v1`。
- **規則(fence 以後)。**
  - K_owner に達していても Open は拒否されない。claim の owner queue に到着順 `(reserved_daa, key)` で並ぶ。claim が 1 つなら door も 1 つなので、到着順は期限順と同じ。
  - 先頭の K_owner 件だけが demand できる。待機中の row は custodian に負荷をかけない。
  - 前の row が枠を手放すと、その順に admit される。手放す契機は、turn を使い切ること、または admit 済みの idle row が reclaim されること(deposit は burn)。
  - idle は、自分の最後の進捗と admit された時刻のうち遅い方から数える。待っていた時間は idle に数えない。reclaim は release として記録される。
  - **tenure。** 待機中の row がある間、admit 済みの row が枠を保てるのは admit から tenure = max(idle grace, open window / (2·K_owner)) DAA まで。それを過ぎた row は枠を譲る(retire。active だったので deposit は没収しない。取得済みの資料は horizon まで root に残り、conviction に枠は要らない)。
    - 理由:fixture では maximum_turns が 1,625 で、open window(4,096 DAA)を超える。active を保つ owner は、turn を使い切るまで枠を手放さない。到着順の列だけでは open window 内の開始を保証できなかった。
    - 列の回転は、Open と、待機中の row の Demand の時点で行う。
  - 保証:q 件が前にいる verifier は、Open から ⌈(q + 1) / K_owner⌉ tenure 以内に admit される。
  - 同時の custodian 負荷は K_owner のまま。後から出た Open が先に始まることはない。
- **公開資料からの直接の告発。** TIR の conviction(`T::Conviction`)は、もともと owner 枠を要しない(`conviction_filer_v1`:責任のない Active で floor 以上の bond なら誰でもよい)。証拠をすでに持つ者は、枠がなくても告発できる。枠が要るのは、custodian から資料を取得する場合だけである。
- **試験** `g14r2_eight_active_colluding_owners_cannot_lock_out_a_newcomer_who_convicts_in_time`:
  - 8 本の共謀 owner が、producer の期限内の回答で active を保つ(idle reclaim は起きない)。
  - 正直な newcomer の Open は fence 未満では拒否、fence 以後は待機になる。待機中の Demand は拒否される。
  - newcomer の後に出た連合の 8 本の Open は、その後ろに並ぶ。
  - tenure(fixture では 256 DAA)を過ぎた 8 本の共謀 owner は、deposit を没収されずに枠を譲る。列の先頭は newcomer で、後発の連合 8 本のうち最後の 1 本はまだ待機している。
  - newcomer は偽造 leaf を demand し、認証済みの資料を得て、collection door 内に告発し `CourtFraud` で producer を slash した。
  - 同時 demand は常に K_owner 以下だった。
  - 数値(frame の accepted DAA からの offset):open +219,900、open_until +223,996、collect_until +17,528,156。newcomer は +219,902 に到着、+220,179 に admit(277 DAA 後)、+220,183 に conviction。open window の終わりまで 3,813 DAA の余裕。
- **残り。**
  - 連合が newcomer より**前に**待機列を積む場合、待ち時間は ⌈(q + 1) / 8⌉ × tenure になる。open window 1 つの中で列が回るのは 2·K_owner = 16 回なので、q ≥ 128 件(open window の先頭で到着した場合。遅く到着すればそれより少ない件数)を先に積まれると、newcomer は open window 内に始められない。抑えるのは、row ごとの rent(burn)・deposit・bond ごとの row allowance の価格だけ。
  - tenure の値(open window / 16)が、正直な owner が局所化と取得を終えるのに足りるかは未測定(fixture では 1 回の demand に十分)。足りない場合でも、取得済みの資料は残り、再度 Open できる(列の後ろに付く)。
  - 同じ block の中での順序と検閲は範囲外。
  - node の owner worker(`kaspad/src/palw_tir_public_owner.rs` の `staged_tir_owner_slot_open_v1`)は、fence 以後も「枠なし」と読んで Open を控える。node 側は未対応。

### 11.3 Panel floor 席の容量(付録 E.2)— 利用者の判断事項(経済値は変えていない)

- 新しい lock 規則(F + window_court、分割あり)では、Panel floor 席(130,000 MSK)が支えられる claim 率は 0.140 claims/DAA。健全時に必要な 0.333 の 0.42 倍である。
  - 健全時の率を 1 席で支えるのに必要な最小担保:238,119 MSK(floor の 1.83 倍)。
  - 10-01 のピーク時:1,696,051 MSK(13.0 倍)。
  - 旧規則(F + 1,000)なら健全時 131,395 MSK で足りるが、F + 1,000 より後の conviction で Valid 席を slash できない(E.2 の late-conviction 試験では、旧規則で 0、新規則で 5 席 × 160.09 MSK)。
- 判断の選択肢(この lane では実装しない)。
  - (a) Panel floor を ≥ 238,119 MSK に上げる(ピーク時まで支えるなら 1.70M)。
  - (b) claim あたりの Valid lock(160.09 MSK)の式を見直す(bond 式 v2 / G-12 の値)。
  - (c) floor 席が担える件数を健全時の率より下に抑え、残りを genesis 席に任せる(genesis 席は新規則で 1.589、4.8 倍の余裕)。
  - (d) 責任期間 window_court を短くする(late conviction の slash 範囲が狭まる。G-13 とのトレードオフ)。
- どれも、bond の経済値か責任期間の変更になる。

### 11.4 signing scope ごとの責任(監査 §3.4 / §8 E)— 規則は変えていない

| 経路(void 理由) | producer | full-mask の Valid / Pass 署名者 | partial-mask の署名者 | kernel の interim 席 | TIR custodian | 報告者 |
|---|---|---|---|---|---|---|
| court verdict / LG14-B one-move(`CourtFraud`) | slash(`void_and_slash_at`) | verdict では slash されない。別に kind-3 の filing(S4 = lock + action)が要る | kind-3 は site の mask 内の leaf に限る(F-1 / F-4) | — | — | `CourtVerdict` の報酬。偽の告発は告発者が slash される |
| `CourtDefault` | `CourtFraud` と同じ扱い | なし(kind-3 が名指しで拒否) | なし | — | — | 報酬なし |
| `CourtHeldVerdict` | `CourtFraud` と同じ扱い | なし(名指しで拒否) | なし | — | — | 報酬あり |
| DA default(`ProducerWithholding`) | forfeit と strike | covering signer は S4(Licensed / FinalRow の段)。ただし `LegacyHeldV2` の unit では課されない | なし(`is_full` のみ) | — | — | `DaDefault` |
| kernel route(ledger `Convicted`) | `reserved.min(collateral)` | 数えられた Pass のすべて | scope を問わない | **G-K1 で slash**(lock 行、休眠 fence `palw_probabilistic_constraints_v1` の下。T-N11 が assert) | — | 49 % |
| RFC-0007 paid-claim trap | kernel conviction の経由 | Clean receipt の署名者(fence 以後) | — | Clean のみ | — | 記録される |
| TIR public frame | slash | — | — | — | credited exact Valid custodian に S4。lock か exit hold がなければ refuse | `CourtVerdict` |

- **監査 §3.4 の所見。**
  - kernel の interim 席が slash されないのは、G-K1 の fence(`palw_probabilistic_constraints_v1`)が武装されていない ruleset での挙動である。
  - 設計文書(本書 §1 I5、§4.2 G-K1、付録 C の G-K1 COVERED)は、slash すべきだとしており、code は G-K1 で実装済み(休眠)。
  - したがって規則は変えていない。必要なのは G-K1 の fence の武装。
- **整合しない古い記述(訂正の候補)。**
  - `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/c4r3.rs:128` が「interim 席は slash しない」と assert している。G-K1 の fence なしでは今も成り立つと思われるが、実行していない。
  - `g14-completion-matrix.md:576`。
  - `archive/0152/00-header-and-changelogs.md:240`(DA の signer flag を `false` としているが、code は `true`)。

### 11.5 実行した試験(2026-10-17、この branch、targeted のみ)

- `cargo test -p kaspa-consensus-core --test g14_court_capacity`:**14 passed、1 ignored**(timing harness)。
  - round 2 の新規 4 件:`g07r2_a_late_honest_reserve_row_is_never_refused_and_convicts`、`g07r2_an_unserved_prosecution_means_the_claim_earns_nothing`、`g07r2_one_move_accusations_have_their_own_budget`、`g07r2_the_reserve_bound_serves_every_first_row_by_its_deadline`。
  - round 1 の 10 件(飽和・flood・restart の 3 件は spare pool に移した)。
- `cargo test -p kaspa-consensus-core --lib -- g14_tir_owner_slots g14r2_eight_active g14_an_outsider_detects g14_tir_frame_convicts_with_the_fence_at_genesis`:**4 passed**(新規 `g14r2_eight_active_colluding_owners_cannot_lock_out_a_newcomer_who_convicts_in_time` を含む)。
- `cargo test -p kaspa-consensus-core --lib -- palw_tir_public legacy_public_filer court_capacity tir_public_owned staged_court`:**95 passed**。
- `--test g13_panel_liability`:**6 passed**。`--test g14_close_gates`:**12 passed**。`--test lg14a_legacy_dispute_fold`:**9 passed**。`--test t12_aheld_same_leaf_decoy`:**1 passed**。
- `--test g14_close_legacy`:**8 passed**。
  - 付録 E の validation(LG14-A と `palw_final_lock_life` が並ぶ場合、`palw_final_lock_full_collateral` を要する)のため、2 件が失敗していた。前の round からの失敗。
  - test fixture で full-collateral fence を同じ高さに付けて直した。
- `cargo check -p kaspa-consensus --lib --tests`:**compile のみ成功**(acceptance walk の one-move 予算と、node E2E の `measured_court` の round 2 field。実行はしていない)。
- 実行していない:workspace 全体、clippy、`kaspa-consensus` の lib test(node E2E の `lg14a_legacy_filer_e2e::`、kernel の `c4r3`)、kaspad。

### 11.6 閉じていないもの

- **測定記録。** `block_one_move_*`、session class(Opening / DissectionMove / Close)の時間・bytes・working set、`reserve_permille` の承認値は未測定である。武装は拒否されたまま。
- **fence をまたぐ drill。** D-13(Sybil の予約・session・decoy court の先取り)、飽和と継続補充、bucket 満杯時の skip、`PanelUnavailable` の unserved void を、出荷 binary で実行していない。
- **node E2E。** `lg14a_legacy_filer_e2e`(measured_court に round 2 の field を追加しただけ)、kaspad の owner worker、producer の事前確認(`palw_court_claim_admission_v1` を miner がまだ呼んでいない)。
- **unserved rule の適用範囲。** Final 後の stage で予約された claim には適用しない。vesting 中の Final claim の扱いは未実装。
- **同じ claim での decoy-first。** 連合の row が claim の reserve を取ると、正直な row は spare に回る。spare が W 以上飽和し続けた場合、結果は slash ではなく報酬なし(unserved rule)になる。連合が支払うのは C_spare · ρ · weight · W 以上の占有料。
- **liveness の数値。** 11.1 のとおり、fused class の新規 claim の受理率は、測定された session 予算に比例する。

### 11.7 追補(2026-10-17、同 branch):node 側の追随、post-Final の unserved、c4r3 の整合

- **node 側(code のみ)。**
  - TIR の owner worker(`kaspad/src/palw_tir_public_owner.rs`):
    - fence 以後は、K_owner に達していても Open を控えない(`owner_slot_open_v1`)。
    - 待機中の row の Demand は、fold と同じ述語 `staged_tir_owner_demand_admits_v1`(tenure は `palw_tir_owner_tenure_v1`)が admit を示すときだけ署名する。
    - 述語が fold と一致すること(admit を示した block だけ Demand が通ること)は、core の TIR 試験が block ごとに assert している。
  - held filer(`kaspad/src/palw_filer_held.rs`):court 容量の待機(queued、ticket 失効、reservation 未着地、block の予算切れ)を `palw_court_refusal_waits_v1` で Retry に分類する。案件を放棄しない。
  - producer の事前確認(`palw_accept.rs`):`palw_court_claim_admission_v1` で bounded admission を確認し、skip される fused attempt に inference を使わない。
  - decoy 価格は free half の escrow なので、従来どおり `AccusationExposureCeiling` の Retry に入る。
- **post-Final の unserved。**
  - 設計上の要件:G14 の目標は全経路で報酬を拒否すること。Final 後の stage で予約された claim も、hard deadline までに処理されなかった場合は報酬を得てはならない。reservation が vesting を未成熟のまま保持しているので、責任期間の内側に当たる。
  - 実装:Final を反転(safe weight・Gate C credit・usage を戻し、`Voided { PanelUnavailable }`)、minted round rights を没収、未払いの vesting を burn(新しい note `UnservedBurned`、tag 10。誰も告発しない)。coinbase で支払い済みの分は戻さない。
  - 試験:`g07r2_an_unserved_prosecution_means_the_claim_earns_nothing` の post-Final ケースが pass(void を確認)。vesting row が消えることの assert は追加したが、disk 不足で未実行。
- **c4r3。**
  - kernel route 自体が `palw_probabilistic_constraints_v1` の下にしか存在しない。
  - fence あり:G-K1 により interim 席は lock 分だけ slash される。`c4r3.rs:128` の assert は G-K1 より前のもので、実行すると失敗した(差額 183,333,333 sompi = lock)。assert を G-K1(lock 額)に合わせた。
  - fence なし:route も lock もないことを確かめる試験 `g14_c4r3_without_the_route_fence_no_kernel_seat_is_locked_or_charged` を追加した。
  - 規則は変えていない。
- **実行結果。**
  - `cargo test -p kaspa-consensus --lib`(node E2E):
    - `lg14a_legacy_filer_e2e::` 18 件中 17 件 pass(court 容量を武装した `..._with_court_capacity` を含む)。
    - `legacy_v2_attempt_honest_claim_is_final_and_paid_under_full_g14` は失敗:「the row matures by its second clock's bound (9208)」。この試験は前の round の WIP commit `5b64cac9c`(unbuilt, untested)で追加され、一度も通ったことがない。vesting の第 2 clock の成熟(heartbeat だけの chain)の問題で、この lane の規則は dispute のない chain に何も書かない。未修正。
    - `kernel_route_valid_signers_are_charged_on_conviction`(T-N11)pass。c4r3 の 2 件 pass。
  - 実行していないもの(disk の空きが 21 GB で、規則の 25 GB を下回ったため build を停止):
    - kaspad の unit test(`p2_8e_the_folds_answer_to_an_opening_as_a_step`、`g14_tir_seat_finding_reaches_the_filer_queue`)。
    - core の更新した試験(post-Final の vesting assert、述語つきの TIR 試験)。
    - kaspad crate の compile 自体。
- **node 規模の飽和 E2E(依頼 2)は書いていない。理由は次のとおり。**
  - t12 の規則では held regime が bisection を拒否する(`CourtOpened` は refuse)。court session を開けるのは、fused class の held dissection(`ShardCourtAccused`)だけである。
  - その honest 側の node 経路は 8k 行で動かない(P2-8e:whole-capture cap、fold の prefix state、tile の開示)。node harness で連合が session 枠を埋める方法がない。
  - TIR の node harness(T-N1)は未作成(§9.7)。
  - node 上で確かめたこと:court 容量を武装した状態で、正直な outsider の one-move conviction が通る(別予算)。
  - 飽和・補充・待機列・owner 8 本の adversarial 試験は fold 規模のまま。

## 追補: class 単位の検出率 policy(2026-10-17、branch `integ/g14-detection-policy`)

外部監査(`g14-external-audit-2026-10-17/`)§7 P1 の 4 項への処置。詳細は [g14-detection-policy-2026-10-17.md](../../../design/palw/g14-detection-policy-2026-10-17.md)。

- 休眠 fence `palw_detection_policy_v1`(全 preset で `None`、Some-only、武装を拒否)。fence より前の legacy draw(k = 4、domain、golden vector)は不変。
- class・kernel 版に結び付く `PalwDetectionPolicyV1` を入れた。検査方式、chain の数を価格に照合して導く N、D_min、ε、k_max、round、shadow、経済値、資源、期限、局所化を持つ。k_min は厳密な超幾何を整数の固定小数で求め、厳密値を下回らない。
- 経済的抑止: 利得が省いた区間に比例する場合、拘束は D = N にあり、`S ≥ margin·N·c/(q_eff·q_collect)` ですべての D が不利になる(全列挙で確認)。固定利得(選択出力)は D = 1 の `margin·G·N/K` で、宣言した policy だけが要求する。宣言しない場合の覆いは全再実行(seg_detect / palw_kernel_verifier / `--palw-challenge`)と court。
- v2 draw: post-commit の anchor、claim・model・job・policy・checker への拘束、round をまたいで重複なし、anchor から再導出できる。
- k と資源の連動: bytes、再実行、保持(legacy の 512 は k ≈ 44 × 32 claim を持てず拒否)、court/DA の opening、期限。node の opening 保持は policy の上限に従い、落ちた opening を capacity_refusal として数える。
- fault corpus(fixture の整数 cache model、実際の seat seam)では、宣言した不正集合(D ≥ 10%)で 99% 以上を確認した。D = 1 系は解析値 k/N と一致した。正直な claim は 1 度も flag されず、誤実装の seat の局所 flag はすべて参照裁定が棄却した。
- 第 2 段: fence より後の claim gate(attempt と FP。option (b) と同じ形で拒否する)、複数 round の seat、funnel の後段(accepted〜invalidated、deadline / unassigned の loss)、実ノードの試験(1 位置の偽りを k/N の率で検出し、告発で有罪、slash、報酬なし)。
- 未完(同文書 §10): court/DA 予算の実連動(interface は同文書に記載)、kernel route の admission と shadow、同時 claim 数の強制、producer の事前確認、RPC、実 artifact での値の測定。

### 11.8 追補(2026-10-17、`integ/g14-detection-policy` の merge 後):未実行だった試験、受入 G、detection policy との接続

- **未実行だった試験を実行した。**
  - kaspad の unit test 3 件 pass:`p2_8e_the_folds_answer_to_an_opening_as_a_step`(court 容量の待機は Retry)、`g14_tir_seat_finding_reaches_the_filer_queue`(fence 以後の Open)、新規 `g14r2_the_seat_takes_no_duty_beyond_claims_in_flight`。
  - core の TIR 試験(worker の述語が block ごとに fold と一致する assert を含む)2 件 pass。
  - post-Final の vesting が burn されることの assert は pass(`g07r2_an_unserved_…`)。
- **受入 G(`legacy_v2_attempt_honest_claim_is_final_and_paid_under_full_g14`)は pass。期待値の側が誤っていた。**
  - 原因:V-4(b) の licence halt。`palw_second_clock_depth_v1` は、2 × window_court の間に settled anchor が 1 つもなければ `None` を返す。vesting はそれを halt と読み、何も成熟させない。
  - これは設計どおりで、`PALW_T12_SETTLED_ANCHOR_DEPTH` の文書にあるとおり、heartbeat だけの履歴では経済的な締めは何も起きない。
  - 試験は heartbeat だけで F + 3 × window_court を待っていたので、一度も支払われなかった。この試験は前の round の未 build の WIP で追加され、一度も通っていなかった。
  - 修正:正直な producer が 2,000 DAA ごとに licensed claim を出し、chain を live に保つようにした。row は second clock の bound までに成熟し、producer の leg は coinbase で mint された。
  - 規則は変えていない。
- **detection policy への interface(§10 の 1)。**
  - `palw_court_claim_budget_v1(state, params, class, accepted_daa)` を追加した(producer の最小 retention を下限として使う)。実際の retention を渡す版 `palw_court_claim_budget_for_retention_v1` もある。返す値は `PalwCourtClaimBudgetV1`。
    - `compellable_openings`:reservation 1 つが自分の予算で強制できる unit 数。DA session 34(LG14-B が in force なら lane 128 を加える)。
    - `da_bytes`:その数 × 1 object の最大 carriage(16 × 100,000 B)。
    - `first_move_by_daa`:hard deadline から、fused なら 4W、one-move なら W を引いた値。
    - `reserved_sessions`:court-demanding な claim なら 1。
  - 他の bond がこの予算を埋められない理由:reservation は 1 bond 1 claim につき 1 つで、予算はその reservation に属する。reserve row は bounded admission が保証する。
  - claim gate:V2 の attempt と FP の gate は `palw_detection_claim_refusal_with_budget_v1` を通る。policy が宣言した court / DA の値と inspection 期限を、実際の予約で置き換えて plan を作る。
    - 試験 `the_claim_gate_checks_the_policys_needs_against_the_courts_per_claim_reservation`:k_round = 53 の policy は、court fence 未満では受理され、以後は `DETECTION_COURT_BUDGET_BELOW_DRAW` で拒否された(fold の attempt gate でも同じ)。retention が短い claim は `DETECTION_DEADLINE_UNREACHABLE` で拒否された。
    - g14_detection_gate は 5 passed、g14_detection_policy は全件 pass。
  - **数値(利用者の判断に関わる)。**
    - producer の最小 trace retention(bind + receipt + challenge + court)で受理された claim の場合、one-move class でも「最後の保証つき first move」は受理の 1,158 DAA 後(DAA 1,001 → 2,159)。
    - fused class では hard − 4W が受理より前になり、保証が成立しない。
    - court を要する class に報酬を出すには、producer が約束する retention を最小値より長く(少なくとも 4W + W_disclose 以上)する必要がある。retention 規則の変更は判断事項で、変えていない。
  - fee 不足:court の carrier(`carry_priority_v1`)は、funding の走査がその object の最低額で失敗したとき、node が detect して追っている claim について `detection_loss_once(class, claim, FeeShortfall)` を記録する。claim の取り出しは `palw_court_object_claim_v1`、class は funnel の watched から取る。
- **kernel route(§10 の 2・3)。**
  - kernel の claim commit(plain / pipeline / segmented / salted)は、`palw_detection_policy_v1` 以後、class に policy がなければ drop する(何も書かない。OPV gate の drop と同じ形)。
  - plain な single-program claim は、job と生成 token から N を出し(位置数 = prompt + generated − 1)、collateral は policy の claim_collateral として、policy の plan も通す。
  - pipeline・segmented・typed の claim は、policy の有無だけを見る(N が plain の stream と違うため)。
  - kernel route の court は自前の ledger court なので、V2 の予約予算は使わず、policy の宣言値を使う。
  - 試験 `g14r2_a_kernel_claim_of_a_class_without_a_detection_policy_is_dropped`(node):1 passed。
  - seat が claims_in_flight を超えて duty を取らないこと:interval seat(`palw_panel.rs`)が `detection_take_duty_v1` で、class ごとの in-flight を inspection 期限まで数える。超えた duty は取らず、`CapacityRefusal` に記録する。kernel route の seat receipt には node 側の scheduler が存在しない(test harness だけが署名する)ので、配線する場所がない。
- **node の回帰。** c4r3 2 件、T-N11、`..._with_court_capacity`、受入 G、kernel gate の node 試験、いずれも pass。

### 11.9 追補(2026-10-17):kernel route の全 claim 種別の plan と、node の公開検証

- **N を、claim の価格付けされた構造から導出する**(`kernel_claim_detection_facts_v1`)。単位は評価位置。
  - 単一 program(plain / salted):`commitments.len()`。job の stream(prompt + generated − 1)と一致しなければ `NotAtTheClaimsPrice`。
  - segmented:tiled job の `prompt_len + generated − 1`。
  - pipeline:各 stage の `commitments.len()` の和。
  - typed Memory:chunk 長の和。
  - typed Retrieval:`max(1, result 数)`。
  - typed Composite:model stage の `commitments.len()` と retrieval stage の result 数の和。
  - 担保は route の `claim_collateral` とする。detection の gate は、これで policy の plan 全体(抑止条件・n_max・資源・価格)を検査する。
- **node は kernel route の claim を検証しているか。**
  - 検証している。公開 verifier `kaspad/src/palw_kernel_verifier.rs` は、fee 用の bond と kernel model を持つ node で `palw_probabilistic_constraints_v1` または `palw_panel_free_v1` 以後に動く。開いている claim を自分の model で独立に再実行し、filing または demand を court lane に流す。
  - 対象は単一 program・segmented・typed Memory である。pipeline・typed Composite・typed Retrieval は、複数 model や snapshot を要するので対象外。
  - この 3 種別のうち、pipeline には onboarding 経路がない(GAP-B4)。Retrieval class は OPV eligible にならない。ところが、model stage だけの Composite は eligible になり得る。つまり、報酬が付くのに node が検査しない claim があり得た。これは G14 の gap である。
  - **処置:** fence 以後、これら 3 種別の claim は、policy が十分でも drop する(`kernel_claim_kind_publicly_checked_v1`)。公開 conviction 経路のない route に報酬を出さないという option (b) の規則と同じ扱いで、checker ができるまで報酬は付かない。
  - Panel 認可の kernel claim について:node は interim 席の receipt に署名しない(harness だけが署名する)。正直な claim は認可されず、報酬に届かない(liveness の問題)。共謀した席が署名した claim は、上の公開 verifier が検査する。
- **verifier の claims_in_flight。** `palw_kernel_verifier_admit_v1` で、class ごとに policy の `claims_in_flight` 件まで(commit の古い順)を同時に扱う。超えた分は待機させ、`CapacityRefusal` に数える。
- **試験(node、実 fold)。** 9 passed。
  - plain:N = 5。十分な policy なら commit、担保不足なら drop、n_max 超過なら drop。
  - segmented:N = 4,501。十分なら commit、n_max = 4,500 なら drop、担保不足なら drop。
  - typed Memory:N = 5 で同じ 3 通り。
  - Composite:policy が十分でも drop(fence 未満の既存 composite 試験は不変で pass)。
  - 回帰:policy なしの drop、c4r3、K2S の多 segment、Memory の E2E。
  - kaspad:`g14r2_the_kernel_verifier_respects_claims_in_flight`、`g14r2_the_seat_takes_no_duty_beyond_claims_in_flight` が pass。
  - pipeline の claim には node harness がない(onboarding 経路がない)。facts の導出式は上のとおりで、gate は drop する。

### 11.10 追補(2026-10-17):kernel route の interim 席を node のサービスとして実装

- **実装**(`kaspad/src/palw_kernel_verifier.rs` の `kernel_seat_receipt_v1`。公開 verifier の tick の中で動く)。Panel の割当でこの bond が席に指名されていて、期限内で、まだ receipt を出していない単一 program の claim について、次を行う。
  1. claim を公開 commitment と登録 model で再実行する。公開 verifier と同じ replay を使い、cache を共有する。
  2. 自分に割り当てられた scope を、自分が再実行した trace に対して `FreshVerifierV1::check` で検査する。値はすべて claim の commitment で認証され、challenge は claim の公開 binding のものを使う。
  3. 両方が通ったときだけ、passing receipt(`receipt_for_v1`、文脈は `CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1`。harness と同じ署名)を court lane(round u32::MAX − 20)に載せる。
  4. どちらかが通らなければ何も署名しない。その claim は、同じ tick の公開 verifier が seal と filing まで持っていく。
  - policy の `claims_in_flight` を class ごとに守る。超えた duty は取らず `CapacityRefusal` に記録する。
- **試験** `g14r2_kernel_interim_seats_run_as_services_pay_the_honest_claim_and_refuse_the_lie`(kaspad、実サービス 6 本・各 mempool・各鍵):pass。
  - 正直な claim:席 3 本がそれぞれ自分で receipt を出し、ProbabilisticPass、Final に至った。job の escrow から claim reward が支払われた。
  - 偽造(MatMul):どの席も receipt を出さず、同じサービスの verifier が conviction し、producer の bond は claim_collateral 分 slash された。
- **既存試験の修正。** `g14k_the_kernel_service_convicts_covered_lies_…` の producer の担保の期待値に、G-K1 の lock を加えた。producer が他の producer の偽造を cover した席でもある場合、その分の lock も引かれる。修正後 pass。restart の試験も pass。
- 対象外:segmented・typed Memory の claim の席の receipt。fold の receipt arm(`apply_kernel_receipt_v1`)が `ClaimBodyV1::Program` だけを数えるので、Panel 認可の席 receipt は単一 program に限られる。segmented と Memory は OPV 専用で、公開 verifier が検査する。

### 11.11 追補(2026-10-17、利用者の判断):Panel floor の引き上げと、court を要する claim の retention

**1. Panel floor = 240,000 MSK。** G-13 の lock 規則が効く間、すなわち `palw_legacy_public_filer_v1` 以後に限る。この fence は休眠しており、全 preset で `None`。
- **導出。** `PALW_PANEL_FLOOR_UNDER_G13_SOMPI_V1` の文書に式を残した。`g13_panel_liability::capital_efficiency_against_the_old_rule` が、測定した claim あたりの値から最小値を毎回導き直し、floor がそれ以上であることを assert する。
  - λ_seat = 0.6 × 5/9。e = 640.17 MSK、ℓ = 160.09 MSK、T_active = 122 DAA、life = window_court。
  - 健全時の最小 238,119 MSK を切り上げて 240,000 MSK とした。
- **既存の floor の型に従った。** `palw_panel_collateral_floor_at_v1(min_collateral, g13_in_force)` が `10 × min_collateral` と 240,000 MSK の大きい方を返す。draw、bind、floor room、activation の席、class seating、processor の draw policy、`Params::palw_seat_economy_at` はすべてこの 1 つの関数を読む。
  - floor 未満の既存 bond は Active のまま(保有物は何も変わらない)。fence 以後は `palw_bond_may_take_work_v2` に落ち、Panel 席に選ばれない(ADR-0124 D4 と同じ型)。
- **再測定(fold、t12 launch ruleset、新規則 + 分割)。**

  | 席の担保 | 1 席が支える claim 率 | 健全時 0.333/DAA に対して | 10-01 のピーク 2.944/DAA に対して | stream(3 DAA ごと、6,000 DAA) |
  |---|---|---|---|---|
  | 240,000 MSK(新 floor) | 0.337 | **1.01×** | 0.11× | 2000/2000 |
  | 130,000 MSK(旧 floor) | 0.140 | 0.42× | 0.05× | fence 以後は席に選ばれない(0/2000) |
  | genesis 席(939,063 MSK) | 1.589 | 4.77× | 0.54× | 2000/2000 |

  - ピーク時を支えるには 1,696,051 MSK が要り、genesis bond 8 本のうち 0 本しか届かない。ピーク時の容量は未解決のまま残る。
- 試験:`capital_efficiency_against_the_old_rule` pass。stream は実効 floor を読むように直した。

**2. court を要する claim の retention。**
- `palw_court_demanding_retention_daa_v1` = max(最小 retention, 4W + W_disclose + window_bind + window_receipt)。数え始めを licence にしたのは、席や外部 verifier が追及を始められるのが licence 以後だからである。
- testnet-12 の数値:W = 3,042、W_disclose = 1,200、最小 retention は 5,400 DAA、court 用 retention は 14,568 DAA。
- **規則(`palw_court_capacity_v1` 以後)。**
  - fused-attention の V2 Attempt は court 用 retention を約束する。それ未満は `ClassNotRewardable { COURT_RETENTION_BELOW_GUARANTEE }` で、option (b) と同じ拒否になる。
  - court を要しない class が長い方の値を使うことも拒否する。class ごとに正規値を 1 つに保ち、lottery の 2 回目の draw を許さないためである。
  - stateless な DA pin(`check_palw_attempt_da_pins_v1`)は、fence 以後この 2 つの正規値だけを受ける。どちらを要するかは fold が class ごとに決める。
- **正直な producer。** `palw_producer_facts_v2` の `min_trace_retention_daa` を `palw_attempt_trace_retention_daa_v1`(class ごと)にした。kaspad の producer(`palw_attempt_from_execution_v1`)と事前確認(`palw_accept.rs` の court admission)は、この値をそのまま使う。正直な producer は、その期間 trace を保持すると約束する。
- **保証の再確認。** 受理 1,001、licence は 2,201 まで、最後の保証つき first move は 2,201、hard deadline は 14,369。first move は licence 以後に来て、hard より前にある。
- 試験:`g07r2_a_court_demanding_claim_promises_the_retention_its_guarantee_needs` pass。g14_court_capacity は 15 passed / 1 ignored。
- 回帰:g14_close_legacy 8、g14_close_gates 12、lg14a_legacy_dispute_fold 9、g14_detection_gate 5、すべて pass。kaspa-consensus と kaspad の check も通った。
- 残り:rider は lead の retention を運ぶ。lead と class が違う rider の扱いは、試験していない。

## 13. 追補: native 経路の G14(G-N1・G-N2・T-N13)(2026-10-17、branch `integ/g14-court-capacity-r2`)

外部監査(`docs/design/palw/rfc-completion-audit-2026-10-17/report_ja.md` の RFC14 14-C・14-F)と本書 §4.2 の G-N1・G-N2・G-14 への対応。本文と §11 までは書き換えない。

- 新しい規則は、それぞれの native lane の既存 fence の下にある(v11 と text29 は `palw_funded_execution_v11`(text29 は加えて Final27)、v39 mining は `palw_native_mining_v39`)。いずれも全 preset で `None`、validation が武装を拒否する。G14 lane closure(`palw_g14_lane_closure_v1`)と同じ扱いで、`Params` の新 field は無く、testnet-12 の identity は動かない。
- 状態は「実装済み・下記の試験が通った」。武装は拒否のまま。設計上の値の扱いは [ADR-0187](../../../adr/0187-g14-native-public-filing-and-post-final-mining-accusation.md) に記録した。

### 13.1 G-N1(告発枠の先取り)

| lane | 処置(code) | 試験 |
|---|---|---|
| v11 | **枠なしの公開告発** `EXECUTION_ACTION_PUBLIC_FILING_V11`(19)。`job ‖ u32 n ‖ filing`(reveal と同じ文法、kind 0/1/2)。producer の operator 以外の Active bond なら誰でもよい。作業料を前払い(誤りなら没収)、Court の envelope は自分の公開 pool から払う(`debit_public_action`)。producer の予約も、保持 ticket も使わない。検証と有罪は reveal と同じ関数(`convict`)を通る | node:`g14n1_four_colluding_owners_cannot_lock_out_an_honest_outsider_who_convicts_in_time`、`g14n1_a_public_filing_cannot_convict_an_honest_job` |
| v11 | **期限切れの未 reveal seal の解放** `EXECUTION_ACTION_RELEASE_LAPSED_SEAL_V11`(20)。`now ≥ reveal_until`(もう reveal できない)で、protected DA を 1 度も強制していない seal は、job が terminal になる前に、pursuit の 4 ticket、kernel の subject・owner・global の計数、trace supplement、physical owner の席から外れる。未使用の guarantee は消費扱い(返還しない)。protected DA を強制した seal は席を保つので、job あたりの protected 回答は 2 × 4 のまま(producer の前払い上限は不変) | node:`g14n1_a_lapsed_seal_gives_its_ticket_back_but_one_that_compelled_answers_keeps_it` |
| text29 | **枠なしの公開告発** `EXECUTION_ACTION_TEXT_PUBLIC_FILING_V29`(21)。paged court の検証(v25 kind 0/1/2 と window37)を、paid owner なしで通す共通関数 `court` に分けた。生涯 owner 目録(4)は変えていない(producer の回答上限のため) | 未実行(text29 の job を作る harness が無い) |

- **数値(v11 node 試験、test horizon)。** commit 48、challenge_end 348。共謀 4 bond が 4 ticket を取った後も、outsider は ordinary lane で 17 回の要求で資料を得て、DAA 89 に告発で有罪にした。collected 130(reserve の全額)、finder 63(490‰ 以内)、requester への返金 20、producer credit 0。2 回目の告発は何も変えない。独立 replay が一致した。
- **lapse 試験。** reveal 窓は DAA 152 までに閉じ、protected DA を強制した 1 本は席を保ち、他の 3 本は解放された。outsider の seal は DAA 157 に解放された席へ入った。
- **試験 horizon について。** probe policy の challenge window は 4 DAA、retention は 2 DAA で、node の carrier で追及できない。試験では challenge 300・retention 300・DA 回答 20 に広げた(測定値ではない)。
- **残る限界。** ordinary lane の生涯要求数(probe 32、開放 3)と discovery ticket(horizon は challenge_end)は、共謀者が要求で使い切れる。回答の料金は producer に支払われるので、共謀側の費用は小さい。公開告発は「資料が公開されていれば」枠を要しないが、資料の取得経路そのものの先取りは、価格(要求ごとの bond と作業料)でしか抑えていない。

### 13.2 G-N2(v39 mining claim の Final 後の追及)

`palw_native_mining_court_fold_v39.rs`(stage action 80 `ACCUSE_MINING_ACTION_V39`、native39 job fold の子 module)。

- **認証された連結。** mining claim → (Master, stage) → Master Final の component job(header の producer bond が claim の producer と一致し、Final)→ その committed post・consent・class・material。restore はすべて job fold の `restore_job` を通る。
- **資料の保持。** commit 75 が credited component ごとに hold 行を書く。claim の責任期間 `palw_native_mining_liability_until_v39` = 受理 + 認証の全寿命 + receipt の寿命 + prosecution window の 1 回分。その間 collection 53 を拒否する。すでに collection された component は commit で拒否する。
- **告発の種類。** kind 0 は自己完結の filing(job court と同じ `check_flat_filing_v39`)。kind 1 は Final 後の demand(公開 lane と同じ `check_flat_demand_v39`)。claim あたり生涯 8 件、同時 2 件まで。告発者の bond を即時に引き落とし、回答期限は `now + da_answer`。kind 2 は producer の回答(32 KiB × 64 part まで、`check_flat_reply_v39`)で、回答は公開資料になり、bond は producer に支払われる。kind 3 は期限切れの demand の default で有罪とし、bond は告発者に返す。告発の搬送は job の quota interval と独立である。
- **Gate C pending。** demand が開いている間、certification sweep の `accusation_pending` が true になり、決定は待つ。receipt の quantum も使えない(fold と validator の gate の両方)。
- **有罪の効果。**
  - producer の reserved exposure を、bond に残っている分だけ徴収する(G-14)。finder には 490‰、残りは burn。
  - claim は報酬なしで close し、ADR-0176 の予約も close する。
  - receipt を消し、spend で与えた safe weight を全部戻す(spend ごとに累計行を記録)。
  - certification 行を `Withdrawn` にする(決定済みでも Gate C credit を失う)。
  - Reproduced と署名した attestor の lock を charge する。
- **attestor の責任。** native mining claim に Reproduced と attest した bond は、claim の reserved exposure と quorum k に seat-lock 関係(ADR-0151 D1/D4)を当てた額を、自由担保に lock する(aux 143 の `0x03`、committed ledger に数える)。担保が足りなければ attestation は数えられない。collection で解放、有罪で charge する。
- **veto。** RFC-0018 U-6 は採用しない。`veto` は設計値 `PALW_CERT_VETO_DESIGN_V1 = 0` のまま、parameter として残る。有罪は検証済みの filing か default だけで決まり、否定 attestation の数では決まらない。
- **試験(fold、TransitionBuilder 上の実際の行)。** 4 件 pass。
  - `g14n2_a_post_final_conviction_slashes_revokes_the_receipt_weight_and_certification_and_charges_attestors`:collected 4,000、finder 1,960、safe weight 1,000 → 700、attestor 2 本 × 600。
  - `g14n2_a_mining_conviction_is_not_refused_for_short_collateral`:担保 1,000 で 1,000 を徴収、finder 490。
  - `g14n2_an_open_demand_is_gate_c_pending_and_a_hold_keeps_the_component_material`。
  - `g14n2_the_liability_horizon_spans_certification_receipt_and_one_prosecution_window`。
- **未実行。** filing・demand・answer の検証そのもの(job court と公開 lane の関数)を v39 の実 job で通す試験。repository に v39 の job fixture(source upload、class、consent、Master、mining の登録)が無い。

### 13.3 T-N13(担保不足で native の有罪を拒否しない)

- **v11、node の状態の上の fold。** `g14n13_v11_native_conviction_is_not_refused_for_short_collateral_on_the_nodes_state`:node が作った job・book・class・quota の行の上で、processor 自身の block fold(`palw_v2_fold_accepted_for_tests`)に公開告発を通した。producer の担保を reserve の 1/3 と 0 に減らした場合(他 lane の slash を模擬)も、有罪・返金・あるだけの徴収・finder の比例が成り立ち、全額の場合は reserve の全額を徴収した。
- **v39 mining。** 13.2 の fold 試験。
- **v29 と v39 の job。** book 水準(既存)のまま。fold・node 水準は harness が無く未作成。
- **node 水準の担保不足。** 全 lane の予約が committed ledger に数えられる(`palw_bond_committed_v1`)ので、正規の経路で担保を reserve 未満にできない。clamp の経路だけが残り、node 試験では作れなかった。

### 13.4 実行した試験(2026-10-17、targeted のみ)

- `cargo test -p kaspa-consensus --lib -- funded11::public_filing`:**5 passed**(13.1 と 13.3 の node・fold 試験)。
- `cargo test -p kaspa-consensus-core --lib -- g14n2`:**4 passed**。
- 回帰:`cargo test -p kaspa-consensus --lib -- flat_native_v9`(reveal を共通関数に分けた影響を含む)は全件 pass。
  - 同時に回した `t12_a2u_new_kinds_uniform` の 2 件は失敗した。原因は別 lane の fence `palw_material_release_recovery_v2`(RFC08)が harness で武装されていないことで、この lane の変更ではない。そのため、新しい action(v11 の 19〜21、stage の 80)を A-2 の一様試験が見る段まで進んでいない。未確認。
- core の identity:`--test g14_court_capacity`(`testnet12_identity_did_not_move` を含む)15 passed / 1 ignored。`g14_close_gates` 12、`g14_close_legacy` 8、fork-id・identity の lib 試験 103 passed。
- 実行していないもの:workspace 全体、clippy、kaspad、text29 の公開告発。

### 13.5 追補(2026-10-18):text29 の job harness で分かった到達不能

text29 の実 job を node 上で作る harness(`funded11/text29.rs`)を書いた。分かったことは次の 2 点。

1. **text29 の全 action が block に入らなかった(修正済み)。**
   - `paid_restore` の料金式は `bytes² · 64 + bytes · 1024` で、32 KiB の下限があるため最小でも約 6.9e10 work になる。kernel route の block あたりの court work 上限(2^30)を常に超える。
   - そのため登録・court・告発を含め、text29 の action は 1 件も運ばれなかった。
   - 料金を同じ byte 単価の線形(`bytes · (64 + 1024)`)に直した。lane は休眠中で、ADR-0187 に記録した。
   - これで登録・prepare・producer の consent までは node 上で通った。
2. **Fund が成立しない(未解決、値の判断が要る)。**
   - paged29 の同時義務は、試験 class で約 72 packet(period 2 DAA)になる。一方、締切は proof window − maturity = 97 DAA で、`seal_ttl = 100` に固定されている(horizon の check が固定値を要求する)。
   - 密度は 1.48 × SCALE となり、物理 service を超える。DA 回答窓をどれだけ広げても、締切は 97 DAA のままである。
   - 公開告発(action 21)の試験と text29 の T-N13 は書いた。ただし Fund の先へ進めないので、`#[ignore]` で理由を付けて残した。
   - 解決には、`seal_ttl` の固定値、carriage の period、paged の packet 数のどれかを変える判断が要る。
   - 試験で使った horizon(interval 100,000、challenge 30,000、DA 回答 200、retention 300、requests_per_session 128、producer の reserve 1e13 sompi、Final27 の試験 calibration)は、到達性を調べるための値で、測定値ではない。

### 13.6 追補(2026-10-18):text29 と v39 の実 job

- **text29(ユーザー決定 2026-10-18)。**
  - Final27 の予約 scheduler の carriage period を 2 DAA から 1 DAA にした(休眠。`seal_ttl`・締切・責任は不変、ADR-0187)。
  - これで Fund が通り、node 試験 2 件 pass。
    - `text29_harness_registers_prepares_consents_and_funds_a_real_job`
    - `text29_a_public_filing_convicts_a_lying_text_job_without_a_paid_owner`
  - 偽った stopped post は、paid owner なしの公開告発(action 21)で有罪になった。producer は reserve 全額(1e13 sompi)を slash された。正直な job は変わらなかった。独立 replay は一致した。
- **v39(owning39)。** node 試験 `owning39_fixture_registers_consents_and_funds_a_real_job` が pass した。
  - 実 `FlatSourceWireV39` で、source(41)、class(42)、producer の consent(43)、Fund(44)、偽った claim の commit(45)まで通った。
  - 外部の独立 replay(SDK の `execute_independent_replay_v39` / `detect_first_fault_v39`)で filing(762 byte)を作った。court と G-N2 が使う `check_flat_filing_v39` は、node に commit された post の上で Computation と判定した。
  - **固定値の衝突ではなかったもの。** Fund の同時義務の締切は v39 では commit window で決まる。probe の 5 DAA では、約 88 packet が period 1 で service の 17.6 倍になる。これは policy の horizon で、固定定数ではない。試験では 200 DAA にした(負荷 0.45)。
  - **未了。** Master の登録(65〜70)、Master Final、mining の登録・seat・commit(73〜75)、認証、receipt。G-N2 の action 80 を、この job の Final 後に node 上で走らせるところまでは届いていない。
- 試験の horizon(interval 100,000、challenge 30,000、DA 回答 200、commit window 200、Final27 の試験 calibration など)は到達性のための値で、測定値ではない。

### 13.7 追補(2026-10-18):v39 の Master・mining は block に入らない(値の衝突、未解決)

13.6 の owning39 fixture を Master(65)まで延ばした。Master の登録は block に運ばれるが、何も書かれない。

- **原因。** owning39 の Master と mining の metadata prepay は、kernel route の block あたり court work を使う(`charge(…, court = false)`)。
  - 上限は `max_court_work_per_block = 2^30`、`prosecution_reserve_permille = 500`。通常の admission に使えるのは 2^29 = 536,870,912 work。
  - Master 65/66 と component の invocation restore は `bytes × 16,384 + 2^20`。mining 73/75 は `bytes × 8,192 + 65,536`。どの action も固定の 65,536 byte 以上の envelope を restore する。
  - fold は `prepay` が `Ok(false)` を返した時点で何も書かずに戻る。action は運ばれ、料金は取られず、状態は変わらない。
- **数値(node 試験、fixture の 1 段 Master)。**

  | action | restore extent | work | 上限 |
  |---|---|---|---|
  | Master 65 | 71,413 byte(登録 1,400 + source 381 + 固定 65,536 + 4,096) | 1,171,079,168 | 通常 536,870,912、block 全体 1,073,741,824 |
  | mining 73 | 65,612 byte(固定 65,536 + body 76) | 537,559,040 | 536,870,912(688,128 超過) |
  | mining 75、Master 66、component の Fund | 65,536 byte 以上 + source / book | 上と同じ形で超過 | 同上 |

- **帰結。** v39 の mining claim は node 上で作れない。G-N2 の action 80 を、実 mining claim に対して Final 後に走らせることもできない。G-N2 の fold 試験(13.2)は変わらない。
- **判断が要る値。** 料金の単価、固定の envelope、kernel route の block 上限のどれか。text29 の二次式(13.5)と違い、ここは式の形ではなく値の大きさの問題なので、値を変えずに記録した。
- **試験。**
  - `owning39_master_registration_is_refused_by_the_block_work_budget`(pass):上の数値を出し、Master 65 が node 上で何も書かないことを確かめる。
  - Master → component job の Final(51、challenge 300 の試験 horizon)→ Master Final(66)→ 73/74/75 → 偽造 claim への action 80(producer の徴収、報酬なしの close、certification の撤回、hold の解放)、正直な claim は不変、までの node 試験 2 件を書いた。値が決まるまで `#[ignore]` にしてある。
- **Gate C の draw(参考)。** 設計値 m = 47 は harness の bond 数より大きい。node 上の native mining claim の draw は `NoPopulation` になる。receipt と attestor の charge は、node 上では fold 試験(13.2)以上に進められない。

### 13.8 追補(2026-10-18):RFC-0023 段階 1 の残り(休眠 `palw_unresolved_bound_v1`)

- **原因別の定常遅れ(core、模擬)。** `rfc23s1_per_cause_steady_state_lag_is_bounded_by_min_hold_and_l_plus_gap`。
  - 600 DAA ごとに 1 件の claim を、原因ごとの時間だけ未解決のまま保つ。正直な claim は 120 DAA 後に認証される。遅れは実関数 `palw_resolved_through_v1` / `palw_gate_c_frontier_v1` で読む。
  - 保持時間は testnet-12 の値(W = 3,042、W_disclose = 1,200、L = 14,568)。

  | 原因 | 保持時間(DAA) | fence 以後の遅れ | fence 未満 |
  |---|---|---|---|
  | 資料応答の遅延(licence + 回答窓 1 つ) | 4,242 | 4,799 | 4,799 |
  | 正当な court(W_disclose + 4W) | 13,368 | 13,799 | 13,799 |
  | 虚偽告発(W ごとに再告発、上限なし) | 無制限 | 14,999 | 140,001 |
  | Gate C の再抽選の連鎖 | 無制限 | 14,999 | 140,001 |
  | court 容量の飽和(L − W_disclose で裁定) | 13,368 | 13,799 | 13,799 |
  | 遅延 claim の継続投入(期限まで) | 14,568 | 14,999 | 14,999 |

  - どの原因も min(保持時間, L) + 間隔 + 認証時間 以内。上限のない 2 原因だけが段階 1 で L に切られる。
- **node の restart と IBD の一致。** `rfc23s1_node_restart_and_genesis_replay_agree_on_the_resolved_prefix_across_the_fence`(consensus、pass)。
  - fence を DAA 20 に置き、fence の前後で attempt claim を作った(未解決集合に入る)。DAA 160 で resolved prefix 17、最古の未解決は blue 18。
  - 同じ DB での restart と genesis からの replay が、sink、state root、kernel 行、block ごとの delta root、resolved prefix で一致した。
- 残り:§3.3 の段階別の実測(`settled`、native safe)、fence をまたぐ出荷 binary の drill。

### 13.9 追補(2026-10-18):RFC-0018 (ii)(iii)(iv)

- **(ii) 長期の公平性。** 16 carve の emission 予算(ADR-0164 F-EM)と mergeset の round robin の組合せで、model が飢えることを確かめた。
  - 旧規則は class を最初の member の priority で並べた。attempt の多い class ほど最小値が小さく、毎回先頭に来る。
  - 20 model、model 0 は 1 attempt、他の 19 model は各 32 attempt、2,000 block。model 0 が入ったのは 121 block(6 %、公平なら 80 %)。
  - **修正(休眠の sibling fence の中だけ)。** class の順を `H(domain ‖ set seed ‖ class)` にした。set seed は mergeset の attempt priority を並べた hash で、SET の関数のままである。model 0 は 1,618 block、全 model が 1,570〜1,634(期待 1,600)。round robin、class 内の順、16 carve、経済値は変えていない。
  - 試験:`rfc18_long_run_fairness_a_one_attempt_model_is_not_starved_by_flooding_models_under_the_16_carve_budget`。既存の sibling 順の 3 試験も pass。
- **(iii) 遅い model A は共有 frontier だけを止める。** `rfc18_a_slow_model_a_claim_stops_only_the_shared_frontier_not_model_b_final_certification_or_payment`(core、pass)。
  - A は未解決のまま(期限 L)。後から来た B は Final になり、A が開いている間に SecurityCertified、receipt clock(認証済みの支払い gate)が開き、frontier の資格も持つ。
  - 共有 frontier だけが A の期限まで待ち、期限を過ぎると A の解決なしに B が frontier になる。A の責任(未解決集合の行)は残る。
- **(iv) Panel の 1 本の in-flight carrier。** 13.10 に記す。

### 13.10 追補(2026-10-18):Panel の carrier 待ちと RFC06 の held-GDN filer

**(iv) Panel の carrier 待ち(kaspad、出荷の scheduler で模擬)。** `rfc18_iv_one_inflight_carrier_wait_across_many_classes_against_the_receipt_window`。
- 条件:出荷の `PalwCarrierSlotsV1` と `TICK_ORDER`、`MAX_INFLIGHT_CARRIERS = 1`、testnet-12 の `window_receipt` = 600 DAA。class ごとに 30 DAA に 1 duty、その 7 件に 1 件で licence(collector)、別の 7 件に 1 件で supplementary の own receipt、120 DAA に 1 本の possession proof(main が空かなければ V07 の readiness lane)。carrier は送信から 1 または 2 DAA で着地する。負荷は試験値で、測定値ではない。
- **結果(lane なし)。**
  - court の queue が空なら、窓を外し始めるのは 96 class(着地 1 DAA)と 48 class(着地 2 DAA)。genesis の 8 model は外さない(最悪の待ち 1〜3 DAA)。
  - court の queue が空にならない間(DA storm)は、OwnReceipts の site に slot が回らない。own receipt は 1 class でも窓を外した(8 class で 206 件)。licence と proof は外さなかった。
- **変更(node policy、consensus の変更なし)。** own receipt の class lane(`palw_receipt_lane_takes_v1`、1 seat あたり最大 16 本)。main slot が取れないときだけ、receipt はその class の lane に乗る。lane は自分の確認済み output を使い、1 本ずつ in flight。他の chain の carrier の子は作らない(V07 と同じ)。readiness lane の funding の走査は、receipt lane の output も除く。
- **結果(lane あり)。** storm の下でも 16 class まで own receipt は外さない。24 class からは lane の上限(16)を超えた class が外す。licence は main slot のままで、storm では 48〜64 class から外す。残る限界として記録する。

**RFC06 06-A:held-GDN の自動 filer(kaspad `palw_gdn_held_filer`)。**
- 流れ:producer の heap を使わず、自分の recompute で最初の偽 checkpoint を見つける → その transition が読む `StateChunk` を held DA court に 1 件ずつ要求(署名した `DefaultAccusedHeld`)→ chain 上の回答を held DA court と同じ hash 検査で確かめる → transition の evidence を作り、court と同じ terminal(`check_gdn_transition_refutation_v2`)で有罪になることを確かめてから署名 → tag 244 `MemoryRoutingV2::Accused`(`GdnTransition`)。回答された bytes が正直な候補(node slot, head)は飛ばして次へ進む。沈黙は DA-7 の default(fold の sweep が有罪にする)に任せ、filer は何も出さない。
- 試験(genesis hybrid の dev fixture、graph-v8 held v5、n_ctx 160、6 件 pass)。偽 checkpoint は leg と binding を再 commit してあり、chain が持つ hash はすべて整合する。
  - 回答する producer:1 単位を要求・回答、court の terminal が `DeltaState { head: 0 }` で有罪。
  - head 1 だけを偽造:head 0 の回答は正直なので飛ばし、2 単位目で告発。
  - 沈黙する producer:要求は 1 件だけ開き、窓を過ぎると default。
  - 正直な producer:case を開かず、何も要求しない。
  - tick(host の seam 経由):要求 → 回答 → 告発の 2 object を運んで `accused` で閉じる。沈黙は `defaulted`、正直は `honest` で object 0 件。
- **未了。** service の tick への接続(held hybrid の claim の列挙、公開 served material の取得、priority lane への queue)と、node 上の E2E(全 seat 共謀、外部 bond 1 本、cold cache、担保の実際の減額)。告発の fold の効果は既存の tag 244 の arm(`convict_by_court_verdict_v1`)で、ここでは node 上で確かめていない。

**実行した試験(2026-10-18、targeted)。**
- kaspa-consensus:`owning39` 2 passed / 2 ignored(13.7)、`rfc23s1_node` 1 passed。
- kaspa-consensus-core:`rfc23s1`・`rfc18_` 42 passed(修正後の fairness 試験を含む)。
- kaspad:`carrier_wait_many_classes`・`rfc06_held_gdn_filer` 8 passed。
- core の identity:`--test g14_court_capacity`(`testnet12_identity_did_not_move` を含む)15 passed / 1 ignored。
- `rfc0018_dag_siblings` の node 試験 3 件は失敗した。fence ありの 1 件は config の validation が RFC-0018 fence の武装を拒否して起動しない。fence なしの 2 件は sibling が merger の past に入らない。どちらも sibling の順の変更(fence の中だけ)を通らない経路で、RFC18 監査 D16 が記録した既存の失敗と同じ症状である。未解決。


## 15. Codex 継続: native の再確認と担保を備えた試験(2026-10-18)

branch `codex/g14-native-completion`、引き継ぎ commit `ac2f69454` の worktree と target を継続使用。

| 再確認 | 結果 | 根拠 |
|---|---|---|
| `owning39_master_registration_is_billed_on_its_carried_bytes_within_the_block_work_budget` | **1 passed** | 既存の対象 test executable を実行、1.48 s。Master 65 は登録 1,400 bytes、source 381 bytes、restore 5,877 bytes、work 97,337,344 / ordinary limit 536,870,912。mining 73 work 688,128。同 log `/tmp/codex-g14-native-carriage.log` |
| `rfc0018_dag_siblings` | **2 passed、1 failed** | 17.88 s。`rfc18_three_models_three_miners_from_the_same_parents_are_all_retained_and_each_recorded` の launched 側で、card 3 の sibling が merger の selected parent / blue / red に含まれず失敗。直前に経済的な DominanceViolation のログがあった。以前の 3 pass 記録を安定した受入結果として扱わない。原因・再現条件の調査が残る。log `/tmp/codex-g14-rfc18-siblings.log` |
| 引き継いだ G-N2 node 試験 | **0 passed、1 failed** | `m7.log`、716.64 s。anchor 用 floor attempt が Final-weight cap に skip され、Gate C anchor を作れなかった |

### 15.1 試験の担保(実装、結果待ち)

- `owning39/master.rs` の mining fixture で、8 本の genesis bond の担保を 100 倍にした。各 outpoint の実 UTXO も同額にし、増分は main premine から移す。総額一致を assert し、UTXO commitment と genesis hash を再計算する。m=47 / k=32、認証・weight cap の式と本番 preset は変更しない。
- これは TEST harness の資金の用意であり、測定値・実ネットワークの経済値ではない。
- **未完。** import 不足(`MuHashExtensions`)を修正後、`CARGO_INCREMENTAL=0 cargo test -j 3 -p kaspa-consensus --lib -- g14n2_node --nocapture` を再実行中。log `/tmp/codex-g14-native-funded.log`。この時点でビルド中で、認証・77・80・attestor 課金を完了扱いしない。
- 次に G-N2 の結果を読み、GDN filer の tick 接続と E2E を build・実行し、RFC18 sibling の失敗を修正する。各段階の結果を別々に記録する。


### 15.2 担保変更に伴う fixture の domain 参照を統一(2026-10-18)

15.1 の対象試験はビルド成功(14m15s)、実行は 0 passed / 1 failed(0.79s)。新しい premine の commitment から genesis hash を再計算した一方、native policy が旧 network domain を持っていたため、stage 41 が `funded native execution policy belongs to another network` で拒否された。G-N2 の成功ではない。

修正では新しい genesis から domain を導き、funded execution / quota とその root、discovery の quota root、Final27 と queue、owning39 の execution / final / physical-service root を一緒に再計算する。`native.check(domain)` を assert する。m/k や資源上限、担保の裏付けと総額は緩めない。

再実行 log は `/tmp/codex-g14-native-funded-domain.log`。結果待ちで、action 80 の成功として数えない。

### 15.3 GDN service host が使用する prompt と binding(2026-10-18、検証中)

service tick は、保持した最初の byte 列を capture とみなし、`gdn_accuser_view_v2(capture, None)` を呼んでいた。しかし held capture は prompt を保持しない場合があり、job ID からの推測は元の block anchor から生成した prompt と一致するとは限らない。free-prompt の保持 byte 列も capture 単体とは限らず FPC envelope である。

attempt は既存の `attempt_job_for_claim` で accepted block の job と prompt を取得し、その job context と一致する capture を選ぶ。free-prompt は FPC envelope を decode し、class と executor が一致する job から既存の `fp_prompt_for_job` で hash-bound prompt を得る。service host は明示 prompt を渡し、execution root と shape profile が対象 claim と一致することも確認する。

実際の graph-v8 capture を使い、正しい prompt を読めること、別 execution root と別 prompt を拒否することの対象試験を追加した。`/tmp/codex-g14-gdn-service-host.log` で build 待ち。fold E2E は `/tmp/codex-g14-gdn-fold.log` で build 中。いずれも現時点で PASS として数えない。service host の対象試験は full node / P2P E2E の代替ではない。

15.3 補足: pool の最初の capture が別 claim のものでも、後の適切な capture を妨げないよう、選択時点で chain の execution root と class ID に一致する binding を選ぶ。対象試験にも正しい capture の受理と別 root の拒否を追加した。build・runtime 結果は引き続き未確定。

### 15.4 GDN 読み取りが checkpoint 不一致を先に拒否していた問題(2026-10-18、検証中)

`gdn_accuser_view_v2` は folded capture の tile 再構成に通常の full-execution 一致検査を使用していた。checkpoint root だけが偽造された capture でも全 root が不一致となるため、GDN 検出に届く前に拒否される。GDN 専用の読み取りでは、再実行した binding の checkpoint Merkle root だけを対象 capture の値に置換し、execution commitment を再計算した後、他の全 field が一致することを必須にした。通常の capture 検証は full binding の一致を要する。告発の決定論的裁定や consensus の受理条件は変更しない。

対象試験は、既存の graph-v8 実行から本物の folded capture を保持し、state chunk 1 byte と checkpoint tree を偽造して再 commit した serialized capture を service host に渡す。正直な capture は `honest`、偽造 checkpoint は case を開くことを必須とした。結果は `/tmp/codex-g14-gdn-service-host.log` 待ちであり、まだ PASS として計上しない。これは service host の対象試験で、full node E2E は依然必要。

### 15.5 GDN artifact fixture; build interrupted for disk capacity (2026-10-18)

Added `palw_service_gdn_e2e.rs`: write a tiny hybrid artifact with Qwen36Writer, carry a normal signed ClassRegistered through mempool/template, and independently resolve its backend through the node SDK loader and chain class resolver. This is the registration/holding foundation for the prosecution E2E; it does not demonstrate conviction or slash. Not yet built or run.

The GDN fold and service-host builds were interrupted when available disk reached 24,974,090,240 bytes, below the 25 GB limit. Only our cargo processes 13968 / 16947 and their orphan compiler were interrupted. This is incomplete validation due to capacity, not a test failure or pass. TIR node tests, native G-N2, genesis #2 fit and run 12 remain running. Resume the same targeted commands after recovering capacity.
