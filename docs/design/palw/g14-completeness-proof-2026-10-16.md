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

DAA 14,000 のリリース計画([t12-daa14000-release-plan-2026-10-15.md](t12-daa14000-release-plan-2026-10-15.md))は、新しい fence を DAA 14,000 で有効化すると決めている。
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
([g14-objective-conviction-remaining §2](g14-objective-conviction-remaining-2026-10-15.md)、[status.md](../../audit/g14-review-2026-10-13/status.md) 末尾)、
RFC06〜08 の 7 fence も測定記録が空のため休眠のまま載せること([remaining-work §0.5](rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md))である。

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
| 画像・MTP の経路(genesis model は text-only class として扱う) | genesis の text class は画像 slot を持たないので、V5 job は構造的に拒否される(`palw_fp_job_version_offered_v1`、`palw_fp_job_v5.rs:303-309`・`:361-375`)。vision tower と MTP head は frontend が lower しないか名前で落とす([coverage-p1p2-record](coverage-p1p2-record.md)、[genesis8-status](genesis8-status.md))ので、class program に含まれず claim できない(明示の拒否文は無い)。**しかし、画像 slot を持つ他の登録 class の FP Job V5 は t12 で DAA 5,300 から受理され報酬を得る**(`PALW_DRILL_FP_V5_ENTRY` `palw_fp_job_v5.rs:457` が `PALW_T12_INT11_FENCES_V1` に入る)。画像 bytes と source id は worker と Panel にしか渡らない(`palw_fp_job_v5.rs:15-17`)。除外の fence(N4)は武装できない。 | **不成立**(G-02) |
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
| G-01 | **CRITICAL** | **DAA 14,000 で報酬を出す lane は、G14 を満たさない legacy Panel route だけである。** P14-min・P14-g14 のどちらでも、legacy V2 Attempt / Free Prompt(と同じ Panel route の TIR v1・gen v1・shard 等)は int-12 の規則のまま報酬と weight を出す。legacy route を G14 に乗せる 2 fence は武装できない。fence 前に登録された class も報酬を得続ける。 | `palw_legacy_public_filer_v1.rs:313`、`palw_legacy_held_da_v2.rs:104`、`PALW_T12_INT11_FENCES_V1`(`params.rs:11551`)、R11、GAP-80([activation-readiness-matrix](activation-readiness-matrix.md)) | ユーザー判断が要る: (a) LG14-A / LG14-B を検証して DAA 14,000 で武装する、(b) DAA 14,000 以後に受理する legacy route の新規 claim から、G14 経路の無い family の報酬と weight を外す(N4 と同じ形の claim gate)、(c) Gate C を武装可能にして武装し、legacy claim を未認証のまま残す。 |
| G-02 | **CRITICAL** | **対象外の画像・非公開 source の work が報酬を得る。** FP Job V5 の画像 bytes と source id は worker と Panel にしか渡らない。PanelDa も同じ。全 Panel が共謀すると、外部の検証者は material を得られない。t12 で DAA 5,300 から有効で、除外 fence(N4)は武装できない。 | `palw_fp_job_v5.rs:15-17`・`:457`、`palw_ruling_fences_v1.rs:16-21`、`validation.rs:18` | `palw_public_evidence_reward_v1` を検証して DAA 14,000 で武装する(規則の source は既にある)。 |
| G-03 | **CRITICAL**(P14-g14) | **TIR 公開 frame は任意である。** TFP1 を付けない TIR FP claim は frame を持たずに旧経路で受理される。共謀する producer は frame を付けないだけでこの lane を避けられる。 | `palw_tir_public_fold_v1.rs:77`、`objects.rs:10-14`・`:3566-3573` | fence 以後、tiled な TIR FP claim には TFP1 を必須にする。または frame の無い TIR claim には Final・credit・報酬を与えない。 |
| G-04 | **CRITICAL**(P14-min) | **Gate C を武装できないので、対象外を「未認証」にする防御が無い。** Gate C の述語は fence 未満で `true` を返す。 | `palw_rfc0018_v1.rs:147`・`:546`・`:628` | Gate C を武装可能にする(RFC-0018 §12 の U-1〜U-8)。それまでは受理時の拒否だけで I10 を満たす。 |
| G-05 | **CRITICAL**(P14-min) | **legacy FP の選択・constraint・prefix・tokenizer の不正は、正直な seat 1 人でも裁けない。** DAA 5,300 以後の FP claim は decode-token close で裁けず、FP の選択証明は拒否され、contradiction 14〜18 は武装できない fence の下にある。validation の注記自体が「seat quorum で止める」とする。 | R5・R6、`palw_court_v2.rs:1281-1291`、`validation.rs:1292-1298` | これらの job version を、経路が武装されるまで受理時に拒否する。または TIR frame と同じ「job 自身の規則」の door を legacy FP にも作り、武装する。 |
| G-06 | HIGH | **legacy route の bystander は追い越される。** live の規則では非 seat の DA session は claim を止めず、追及中に claim が Final になる。court による先取り(`ClaimUnderSession`)も live。node-E2E がこの挙動を固定している。 | `lg14a_unarmed_the_reservation_is_dropped_by_name_and_the_bystander_is_outrun`(`lg14a_legacy_filer_e2e.rs:1123`)、R1・R3 | G-01 (a)。`DirectProofFirst` を独立の武装できる fence に分ける。 |
| G-07 | HIGH | **legacy の非 seat 予算と court 上限を Sybil が使い切れる。** | R2・R14 | 予約(LG14-A)の武装、または予算を告発者ごとに分ける。 |
| G-08 | HIGH | **legacy FP の報酬は conviction の窓が閉じる前に払われる。** coinbase は licence + 1,200 から払われ、Final の反転は 3,000 まで可能で、払った coinbase は戻らない。 | R10、`dos_repro_1_fp_final_receipt_rights_unpriced_unforfeitable` | TIR frame と同じく Final floor を責任の期限まで上げる(`palw_claim_final_floor_v1` の frame 分と同じ形)、または receipt の carve を vesting にする。 |
| G-09 | HIGH | **X3 の絶対期限が無い。** live では held decoy の敗北後に窓が再付与される。staged court は期限を後ろへ動かす。期限が動くので、追及の開始予算を固定できない。 | `palw_ruling_fences_v1.rs:6-15`、`validation.rs:22`、[spec register](mainnet-spec-register-2026-10-12.md) X3 | `palw_claim_hard_deadline_v1` の値を決めて検証し、武装する。 |
| G-10 | HIGH | **武装できる G14 fence に、武装拒否も測定記録 gate も無い。** staged court・TIR route・Float32・TIR conviction は、前提が揃えば validation を通る。未実行の code が武装され得る。 | `validation.rs:2861-2886`、`params.rs:4988-5016` | V1〜V7 の記録が pin されるまで `cannot be armed` を返す。他 lane と同じ measured-value gate を付ける。 |
| G-11 | HIGH | **rule E を武装できず、V6(担保の無い private heartbeat 枝)が残る。** G14 の conviction 自体は journal で再編に一貫するが、取り込まれた private branch の Final の credit を止めるのは fork choice である。 | `palw_fork_choice_rule_e_v1.rs:102`、[finality-palw-consistency](finality-palw-consistency.md) V6 | ADR-0178 の rule E(と Gate C の rule E v2)を検証して武装する。 |
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
| G-T1 | **CRITICAL**(P14-g14) | TIR frame | **outside owner の 8 枠は先取りできる。** 枠の上限は他の検査より先に効き、material horizon まで解放されない。枠が無いと外部の者は material を demand できず、DA default を collect できず、id 列も得られない。 | `palw_tir_public_fold_v1.rs:509-511`、`palw_staged_court_fold_v1.rs:235-241`、[remaining R1](g14-objective-conviction-remaining-2026-10-15.md)(Lead 既定、ユーザー確認待ち) | idle な owner の枠を回収する(一定 DAA demand の無い owner を退去させ deposit を没収)、または dry run が通る finding を持つ bond のために常に 1 枠を空けておく。 |
| G-T2 | HIGH | TIR frame | **選択の不正を node が局所化しない。** owner は step root か output root の不一致からだけ疑い、trace だけが違う場合は log して skip する。選択証拠の builder はあるが呼ばれない。 | `kaspad/src/palw_tir_public_owner.rs:555-562`・`:574-580`、`kaspad/src/palw_tir_public_filer.rs:54-66` | 認証済み id 列から最初に食い違う position を求め、その行の tile を demand し、decode-token pin を作って filing する。fold 水準の DecodeToken filing の試験を足す。 |
| G-T3 | HIGH | TIR frame | **replay は prompt ids に依る。** prompt ids は `palw_fp_derived_work` 以後にしか書かれず、G14 fence はそれを要求しない。prompt ids が無い frame を owner は skip する。 | `objects.rs:3615-3630`、`palw_state_v2.rs:19126`、`palw_tir_public_owner.rs:317-319` | G14 fence に `palw_fp_derived_work` を前提として足す。または `install_original_v1` で prompt row を書き、`collect_until` まで保持する。 |
| G-T4 | HIGH | TIR frame | **正直な抽選 seat 1 人の node 経路が無い。** owner は抽選 seat を skip し、Open も抽選 seat を拒否する。`queue_finding_v1` に finding を渡すのは owner だけ。consensus 上は抽選 seat も filer になれるが、node はその finding を作らない。 | `palw_tir_public_owner.rs:308-309`、`palw_tir_public_fold_v1.rs:37-47` | Panel の replay finding を `queue_finding_v1` に回す。または Valid を出していない抽選 seat を owner として開けるようにする。 |
| G-T5 | MEDIUM | TIR frame | **同時 demand のもとで正直な custodian が DA default になり得る。** responder は同時 4 demand、demand ごとに 1 move を 10 DAA ごとに再計画する。応答窓は他 owner の同時 demand を数えない。 | `palw_tir_public_custodian.rs:868`・`:994`・`:1154-1174`、`palw_tir_public_owned_v1.rs:1805-1830` | 応答窓を同時 demand 数で伸ばすか、custodian ごとの同時 demand を consensus で制限し、測定する(R7)。 |
| G-T6 | MEDIUM | TIR frame | **証拠が薄い。** DecodeToken と停止規則の filing、frame 経由の seat の DA default、`ProducerFrozen` と receipt spend の拒否の assert、node 試験がいずれも無い。`g14_an_outsider_detects…` は producer の課金だけを assert し、replay は試験 helper で node の owner code ではない。 | §3.7 | §5.2 の T-N1〜T-N7。 |
| G-T7 | LOW | TIR frame | conviction は各 Valid seat の S4 lock の存在を assert しない(DA 側は assert する)。 | `palw_tir_public_fold_v1.rs:916-920`・`:677-683` | 同じ assert を足す。 |
| G-K1 | HIGH | Kernel / OPV | **Panel 許可の暫定 seat が passing receipt に署名しても課金されない。** 署名者の責任を負う唯一の経路は paid-claim trap で、休眠・1 sompi の probe 値で、slash の結果を捨てる。 | `settle.rs:43-93`、`palw_service_kernel_e2e.rs:760-768`、`palw_paid_claim_trap_v1.rs:58`、`palw_paid_claim_trap_fold_v1.rs:288-292` | Final の前後で conviction 時に署名者を清算する種別を足す。または報酬対象の kernel class を OPV に限る。 |
| G-K2 | HIGH | Kernel / EXEC | **試験の seam が本番の受理の代わりをしている。** REAL root は `cfg(test)` でしか受理されず、typed root の適格と artifact attestation も多くの node 試験で seam に依る。 | `processor.rs:234-263`・`:294-310` | 本番の受理経路を作り、seam なしで node 試験を再実行する。 |
| G-K3 | HIGH | Kernel / EXEC / 全 lane | **検証者の誘因(P0)。** 嘘をついた本人の Sybil が報奨を取れ、正直な claim の検査には何も払われない。抑止は価格付けされているが、正直な finder は無報酬になり得る。 | `econ_bounty.rs:168`・`:218`、`c4r4.rs:333`、[activation-readiness-matrix §2](activation-readiness-matrix.md) | salted sample の watcher 報奨(readiness matrix §2 の案)。 |
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

- [g14-completion-matrix.md](g14-completion-matrix.md) §2i〜§2k(条件 C1〜C10 と route 表)
- [g14-objective-conviction-remaining-2026-10-15.md](g14-objective-conviction-remaining-2026-10-15.md)(R1〜R8 の checkbox)
- [legacy-route-g14-filer.md](legacy-route-g14-filer.md)、[legacy-route-g14-held-da.md](legacy-route-g14-held-da.md)(LG14-A / LG14-B)
- [rfc-0018-implementation-status-2026-10-15.md](rfc-0018-implementation-status-2026-10-15.md)(Gate C)
- [mainnet-spec-register-2026-10-12.md](mainnet-spec-register-2026-10-12.md)(X3・N4・T6・X4 と job version 表)
- [activation-readiness-matrix.md](activation-readiness-matrix.md)(GAP-80・GAP-81、検証者誘因)
- [t12-daa14000-release-plan-2026-10-15.md](t12-daa14000-release-plan-2026-10-15.md)
- [../../PRINCIPLES.md](../../PRINCIPLES.md) §0・§6・§8・§9

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
