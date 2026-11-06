# Positive Court — 最新ZIPのソース照合
確認日は2026-10-31。過去回答のコード行番号を再利用せず、今回のZIPから確認した。既存ソースのコメントには過去の状態説明もあるため、現在のネットワーク稼働状態まで証明するものではない。
- 入力: `misakas-main (1)(1).zip`
- ZIP SHA-256: `38ccb4ed8387b345b8c3c2627885d49070fb3872ee0f57a330d7b62eb2710b37`
- Git commit: ZIPのみからは未確定。
本文は設計の根拠照合であり、全ソース監査・Rust build・実Node試験ではない。
## 参照ファイルの同一性

| ファイル | SHA-256 | 行数 |
|---|---|---:|
| `docs/PRINCIPLES.md` | `7de5364029db47b54ba9bce6c895ac49ce3084b29acc539922a2ad077d548900` | 348 |
| `docs/rfc/0014-panel-independent-fraud-prosecution.md` | `b6abbf1fcc2c62fe91397078274339ac6cb11942ccd4632dc6a00296a2d3bab8` | 1043 |
| `docs/rfc/0026-court-policy-master-contract.md` | `8dfdc4e80aa7a50f7f2b00f1dc302c347bb50a8da8e8f169865cef6fe038c687` | 194 |
| `consensus/core/src/palw_security_certification_v1.rs` | `df4814edd71d1d3ffbc7e9e6f8842f02a542c77208ca44ce96f1817f4af28227` | 863 |
| `consensus/core/src/palw_security_certification_fold_v1.rs` | `487010ee6cc57ae04e0496b36cd8371f18af854e321470684505b6d2c35f6214` | 1240 |
| `kaspad/src/palw_kernel_roles.rs` | `cf409624bab5668fa746bfcb54a8b091ca1a954f87169362c1f330754dbc9ca0` | 926 |
| `consensus/core/src/palw_court_shared_original_aggregate_v1.rs` | `2b31e0163fbbac838ddebd87ce8278436e7ca0d78c21825f68d8231d2084e5ae` | 290 |
| `consensus/core/src/palw_court_shared_material_intake_fold_v1.rs` | `2b39e890b92ca55b822d096047ad71edaf629be1d9a62bb910e4a763da56c4b0` | 365 |
| `consensus/core/src/palw_claim_local_v1.rs` | `74e5be4236b6c0daca14ec16e62c0728d12f2649bee2799ad71d4ffc8061c1c9` | 1071 |
| `consensus/core/src/palw_native_settlement_v1.rs` | `99185c2f413874b54d7b74d8a1f5640df22a418ff35833098f1a768cf3c4e0c7` | 1140 |
| `consensus/core/src/config/params/full_activation_v1.rs` | `aa83311c999769fe5ba52d3f06a5025924c601a6c67c0a48a38be3255d5bafa3` | 512 |
| `consensus/core/src/palw_row_stream_v1.rs` | `f8a783e325f8e920c071292057fdd27a17d46ff663e68e7bbefe4da66ddf19ed` | 454 |
| `misaka/palw/sdk/src/lib.rs` | `45ffa21a9117f6e3eec219427006794ba572b8ca13946d6f282d5930b429fe98` | 670 |
| `docs/rfc/0029-owner-independent-shared-tir-query-data.md` | `9fb6257c28155e32e66c3dba004f6baf2e2c0661ad49ca858ff0c9a7ba6c723e` | 224 |
| `docs/rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md` | `da14fa673ee16cbf368bf1caa10b27a388a06aa4425de5d51062be75a6c1ae36` | 68 |
| `docs/rfc/0035-court-original-funded-contract-and-role-consent.md` | `746be1d58ec259fb8aaca4b55dd988025f9f558cd05b7c7d145cc0a468786042` | 135 |
| `docs/rfc/0023-unresolved-claim-liability-and-independent-progress.md` | `cfe54859d881c7201abbd9e2253d29fff83af7615b170f1e5b9efb4932cf8759` | 201 |
| `docs/rfc/0036-authenticated-palw-state-map-and-row-access.md` | `2a7da16331868ede8756e30c9e2d069a4827dee36b07ef62fa1729123b690920` | 90 |
| `docs/rfc/0037-ibd-bounded-row-stream-and-storage-generations.md` | `55f9c44976a289045f147c59f0a5cddbfe56efa1e0c81689d7cf5384b59515d3` | 89 |
| `docs/rfc/0040-authenticated-prefix-work-dependencies.md` | `73c697298446b50af899349f1f32147c5da5300beef81c8c19b39c60b2fcf7e3` | 43 |
| `docs/rfc/0018-dag-parallel-multi-model-consensus.md` | `3672c8774d690749527351c5276e622f19a623798a16e29386c2265392b28390` | 429 |
| `docs/rfc/0017-model-eligibility-examination-and-renewal.md` | `6f91c20b1650ec66336df78bd6715a742e97edde4c5cdbc396afb3544c32fabc` | 46 |
| `docs/rfc/0019-annual-model-upgrade.md` | `ea6c0bbcb654f575bb8396e909f2b31ed78004a3b4897fba1d7088b8e0c66d0e` | 446 |
| `docs/rfc/0041-genesis-bond-free-bootstrap.md` | `eb0d90281513b079f0075ae58ca66dbef383eee2e30bd5e2cd9478d0f8ea582d` | 30 |
| `docs/adr/0209-genesis-bond-free-bootstrap.md` | `17d64fc16ff93a4bb80336c6ef3274a78b403ee06c7d79b5cca0df0c93def867` | 17 |
| `docs/AGENTS.md` | `bcf4ed4283f8b96714dd7901248c9f204e79dd7989ee155ac1c97b5b5b0e70e1` | 13 |
| `AGENTS.md` | `c3ded94d9a07d9fd3fd99fb1e5ebbafeea54f51f693c99059ac899aba31cb6af` | 76 |

## 主な境界の抜粋

以下のL番号は元ファイルの行。引用は今回の設計との衝突・接続点を確認する目的。

### `docs/PRINCIPLES.md`

```text
L43: この節は本書の他の節と下位の全文書に優先する。§2・§4・§6と下位文書のうち、検出確率の下限の証明や「不正の期待利益が負」を
L44: 最終条件・有効化gateとする記述は、この節に従って読む。
L45:
L46: 1. **経済Goal。** モデルを共有して外部minerのbond参加を増やしたモデルが、非公開のまま自己資本だけで採掘するモデルより、
L47:    モデル全体として多くの配分を得る構造を実現する（モデル別のブロック生成機会とcoinbase配分、7日平均bondの`α = 1.5`（2026-10-16 ユーザー決定、2026-10-22 再確認。旧 30 日）、[ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md)）。
L48:    chainはモデルが共有されたかを判定せず、実際に拘束されたbond資本と、検証が成立したかだけを読む。非公開は次の二つで不利になる。
L49:    - 公開モデルへbondが集まる結果として、閉鎖モデルが相対的な配分を失う。
L50:    - 検証attestation gate（ECON round 2、readiness §3f）により、bondから抽選された検証者が自分のモデルのコピーでclaimを検査し、
L51:      所定数が承認しないmodel-epochにはモデル配分を払わない。誰にも共有されないモデルは検証が成立せず、配分を受けられない。
L52:      理由は「検証が承認されなかった」であり「モデルが配布されなかった」ではない。claim・escrow・Final・weight・slashには触れない。
L53: 2. **共有されたモデルの不正は客観的に裁ける。** 登録モデルが1人にでも共有され、正しいPanel（正直な検証者）を建てられれば、
L54:    そのモデルのclaimの不正は、claim固有の公開・認証済み証拠から局所化し、客観的なcourtで立証してslashできる。これを制度要件とする（§3、
L55:    [ADR-0173](adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)）。
L56: 3. **非公開モデルの不正は有界化する残余リスクである。** 誰にもモデルが共有されず実効的な検出確率が0（`p = 0`）になる場合、
L57:    不正の排除は保証しない。これは有効化を止める安全性gateではなく、bond単位のBlock / Reward / Final Weight上限、共通拘束期間、
L58:    Final前の払い出し禁止で被害を有界化する残余リスクとして扱う（[ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)）。
L59:    上限は正直なminerにも不正なminerにも同じに適用する。
L60: 4. **検出確率と期待利益は評価指標である。** 検出確率の下限の証明と「不正の期待利益が負」は、最終条件・有効化gateにしない。
L61:    測定・導出して報告する評価指標とし、未知の値はUNKNOWNと表示する。根拠のない検出確率の仮定、検査対象外の演算を検査済みと表示すること、
L62:    検査されていないのに検査されたと扱うことの禁止（§2・§4）は維持する。
L63: 5. **達成条件は二段で評価する。**
L64:    - **(a) モデル全体:** 非公開のときと共有して外部bondを集めたときで、モデルの総bond・配分割合・経済規模がどれだけ変わるか。
L65:    - **(b) 所有者個人:** 元の所有者自身の取り分が共有で増えるか。マイナー報酬はモデル登録者へ直接分配しない。登録者の収入はModelPositionの売買手数料と、自らMiner/Verifierとして参加した場合のその役割の報酬である（[RFC-0015 §8.5.5](rfc/0015-panel-free-permissionless-verification.md)）。モデル内の配分、自己資本を追加してbondを増やす場合との資本コスト・期待収益の差を含める。
```

```text
L223: <a id="8-llm計算の暗号学的証明を使用しない--2026-10-15ユーザー決定"></a>
L224: ## 8. モデル計算の暗号学的証明を使用しない（2026-10-15、ユーザー恒久決定）
L225:
L226: **MISAKA は LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用せず、今後も使用しない。**
L227: この節は本書の他の節、既存・将来の ADR・RFC・設計・実装・有効化計画・研究項目に優先する。
L228: 過去の文書に採用・拡張候補と書かれていても、その記述は撤回対象であり、採用を継続する根拠にはならない。
L229:
L230: 1. **採用禁止。** zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント（PCS）等に基づく succinct argument / validity proof、再帰証明・proof aggregation を使用しない。GKR や producer が送信する多項式証明 transcript による計算 validity argument へ迂回しない。導入を将来課題や研究候補にも残さない。
L231: 2. **検証方法。** 確率的検出（サンプリング再実行・決定論的再計算・代数的スポットチェック）を行い、不一致を公開証拠で局所化し、決定論的に裁定し、経済的に抑止する。直接の Freivalds 検査などは、その検査器が読んだ入力と検査した関係だけを対象とし、全モデル計算の暗号学的証明とは呼ばない。
L232: 3. **合意 credit の認証。** 抽選された検証者の承認を、正直な割合の明示的な前提の下で用いる。その割合、抽選・重み・相関、取得可能性、期限、検出範囲を報告する。署名や quorum は承認者と対象の認証であり、算術真実の暗号学的証明ではない。異議時の有罪判定は公開証拠と決定論的 court による。 **楽観的な Final または一致した local replay は、抽選された検証者の承認を代替しない。** material の再計算、契約の受入、データの同一性確認だけで consensus credit の承認を得たと扱ってはならない。
L233: 4. **利用できる暗号機構。** ハッシュ・署名・Merkle commitment・VRF/beacon は、データの同一性・認証・乱数に使用できる。Merkle opening は保持データの所属を認証するもので、モデル計算の正当性を証明するものではない。
L234: 5. **実装と互換性。** 禁止した方式を新規生成・受理し、報酬や weight・credit の計算正当性の根拠にしてはならない。旧履歴の読み取りや wire ID の保存が必要な場合も、その目的に限る。旧 ID の意味を別方式へ再解釈せず、必要な移行は明示的な version/fence で行う。
L235: 6. **評価。** 128-bit 等の数字を全計算の暗号学的健全性として保証しない。許容する検査の誤検出・見逃し、反復・grinding、正直な検証者の参加、データ取得、費用と経済条件を、それぞれの前提を明示して評価する。未測定値は UNKNOWN とし、禁止方式の実装を不足解消の条件にはしない。
L236:
L237: 7. **理由。** ZK などで LLM の計算を実用的に証明することは不可能である(ユーザー判断)。
L238: 8. **既存文書。** ZK・succinct proof・compact proof を「将来」「研究」として挙げている箇所(例: `docs/design/palw/rfc9-l2-fork-choice.md` 案 C、RFC-0015 §10、`docs/design/palw/rfc-0009-remote-record.md`)は本決定により除外する。過去の記述は履歴として残し、除外注記を付ける。
L239: 9. **エージェント(Claude Code、Codex など)へ。** 設計・実装・レビュー・提案で上記の暗号学的計算証明を持ち出さない。既存の記述や実装を見つけたら、除外である旨の注記を付けるか撤回する。リポジトリ直下の `CLAUDE.md`・`AGENTS.md` にも同じ決定を記載している。
L240:
L241: (本節は 2026-10-15 に Codex と Claude Code がそれぞれ追記した 2 つの §8 を 1 つに統合したもの。)
L242:
L243: ## 9. モデルの二階層とモデル単位の検証の不採用 — 2026-10-15 ユーザー決定(凍結前提)
L244:
L245: **本節は原則凍結した前提である。** 通常は再審・変更しない。変数・パラメーター調整と重大問題時の例外変更は [§11](#design-freeze) に従う。例外では新規 RFC と新規 ADR の両方の採択、設計責任者の明示的承認、対象節の改定を要する。今回、階層規則や数値は変更しない。
L246:
L247: 1. **genesis と年次フォークで指定されたモデルは 1.0 である。** 年 1 回程度のソフトフォークまたはハードフォークで、モデル ID と固定 artifact root の組を承認リスト(ApprovedModels)に追加する。追加のみで、既存モデルは上書きしない(ADR-0175)。リストへの指定は政策的決定であり、「公開されている」「独立検証可能である」ことの技術的証明ではない。
L248: 2. **後から登録したモデルは、登録した時点から審査なしで 0.3 に固定する。** 昇格・更新・失効・試行手数料はない。何年経っても、どれだけ使われても 0.3 のままである。**1.0 になる経路は、年次フォークでの指定だけである。**
L249: 3. **モデル単位の検証・審査は行わない(不採用)。** 理由: モデルに対して検証を行うと、その検証において何を信頼するか(誰が・何を根拠に「正しく共有・運用されている」と判断するか)という問題が新たに発生し、それは解決しない。モデルの公開状況・保持者数・取得可能性・共有状況をチェーンが審査することはしない。
L250: 4. **係数は配分の重みに一度だけ掛ける。** `A_m = c_m · S̄_m^1.5`(α=1.5、7 日平均の適格 bond。2026-10-16 ユーザー決定・10-22 再確認、旧 30 日)。この P_m を物理ブロック機会(RFC-0016 V+)と coinbase 配分の両方に使い、受理ブロックの報酬に重ねて掛けない。登録モデル全体に発行上限を設け、多数登録で 0.3 の枠を増やせない。
L251: 5. **claim 単位の検証は、すべてのモデルで維持する。** モデル単位の検証を行わないことは、claim の検証を省くことではない。各 claim は階層に関係なく、確率的検出・公開証拠による局所化・決定論的裁定(court)・claim ごとの抽選検証者による認証(`SecurityCertified`、正直な割合の明示的前提)を受ける。認証されていない claim は、どの階層でも fork choice・決済深度・Rule E 参加・settled anchor・weight を進めない(§0 の 2026-10-15 改定、RFC-0018)。
```

```text
L328: <a id="design-freeze"></a>
L329: ## 11. 採用済み設計の原則凍結と例外変更
L330:
L331: **今回のユーザー指示を文書化（整理日 2026-10-17）。** 設計の変数・パラメーター以外は原則凍結し、通常変更禁止とする。重大な問題が確認された場合は、新規 RFC と新規 ADR の両方、レビュー、明示的採択によって変更できる。
L332:
L333: 変数・パラメーターの調整も、構造と不変条件を維持し、旧新値・根拠・影響・試験・承認を [台帳](governance/parameter-register.md) に残す。数値で認証や責任を外す変更を通常調整としない。
L334:
L335: 本書の「恒久」「再審しない」「変更不可」は通常の変更禁止として維持し、重大問題の例外手続きは本節に統一する。本書自体の改定にも設計責任者の明示的承認を要する。意味不変の実装・修正・最適化・試験・文書整備は継続できる。未採択 Draft の採用・技術方式の撤回・稼働有効化は今回行わない。
L336:
L337: 正本: [設計凍結](governance/design-freeze.md)。[ADR-0181](adr/0181-design-freeze-and-controlled-change.md) / [RFC-0021](rfc/0021-design-freeze-and-documentation-governance.md)。
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md`

```text
L35:
L36: **後続改定のscope:** モデル自体の入手・公開は合意条件にしない。以下のG14/Dispute Completenessは、
L37: verifierが正しい登録modelを保有/取得できた条件で、claim固有証拠から裁定する能力を指す。
L38: モデル取得を目的とする要求は適格DA/courtとせず、全weights/反復rangeの強制公開へ拡張しない（§16、ADR177 D2/D7）。
L39: 閉鎖時の実効検出と経済安全性は別に未立証であり、旧無条件の全量取得保証を意味しない。
L40:
L41: Panelは通常時の検査・receipt集約を担う。claimの真偽をPanel多数派の信頼だけで決めない。
L42: 不正を見つけた任意の適格な第三者が、Panelへの所属やgenesis operatorの協力を必要とせず、
L43: 認証された公開データから不正位置を絞り込み、有界のexact courtまたは既存の
L44: `ExecutorRefuted`へ到達できる仕組みを作る。
L45:
L46: **classを報酬・work weightの対象にする前に、第三者だけでこの処罰経路を完結できる
L47: Dispute Completenessを合意上のadmission条件にする。**
L48: ハッシュが開くこと、producer自身がcourt evidenceを作れること、成功するcourt kernelがあることは、
L49: 独立した第三者が証拠を取得できることの代わりにならない。
L50:
L51: この条件を完全に満たした後の固定Panel撤去は[RFC-0015][R15]で別に定義する。
L52: **Panel外の普通のpublic bondが、producerの秘密状態を使わず、public authenticated materialだけから
L53: 不正をlocalizeして客観的convictionまで進める状態に達するまで、Panel=0を有効にしてはならない。**
L54: 本RFCの文書作成・merge、operatorのみの成功、小geometryやliar instanceを使うcourtテストは、
L55: この前提の達成ではない。Panel=0は検証の廃止ではなく、固定Panelからpermissionless verifierと
L56: objective fraud proofへの移行であり、lifecycle・担保・Finalを含む別upgradeを要する。
L57:
L58: 目標は次の二つに分ける。
L59:
L60: 1. **検出後の保証:** 正直なverifierが、不正を示す検証可能な不一致を期限内に発見した場合、
L61:    producerが必要なclaim固有証拠を開示すれば客観裁定へ、開示しなければ定義されたDA/default処理へ進む。
L62:    不正claimをPanelの多数票だけで経済的Finalに逃がさない。
L63: 2. **検出の保証:** 完全な決定的検証なら、対象範囲について不正を検出する。
L64:    ADR0171の確率的constraint検査なら、承認済みsuiteの条件付き見逃し確率を受け入れる。
L65:    「1人オンラインなら全ての偽claimが必ず検出される」とは書かない。
L66:
L67: 「1人がいる」は、任意の小さなPC1台が無制限の全claimを裁ける意味ではない。
L68: そのverifierは対象classの承認済み検査・局所化を実行でき、許容claim rateを処理する資源、
L69: 必要なbond/exposureとfee、期限内のネットワーク到達性を持つ必要がある。
L70: 計算資源、DA、chain inclusion、challenge randomnessへの仮定はPanel多数派の仮定と別に明示する。
L71:
L72: ## 1. 現行コードの到達点
```

### `docs/rfc/0026-court-policy-master-contract.md`

```text
L26: ## 2. 時計(F01・F09)
L27:
L28: ### 2.1 主契約
L29:
L30: `a` を original claim の受付 DAA(merged block の header ではなく、accepting chain block の DAA。RFC-0019)とする。
L31:
L32: | 時計 | 値 | 起点 | 根拠と強さ |
L33: |---|---|---|---|
L34: | 通常の申立て受付の締切 | `a + 3,120` | `a` | RFC-0014 2026-10-19 改定(ユーザー採用) |
L35: | 係争の hard deadline | `a + 10,000` | `a`(申立てからではない。再申立てで reset しない) | 同上 |
L36: | 各手番の応答期限 | 手番ごと。hard deadline とは別の制約 | 手番の開始 | 同上 D。値は未決 |
L37: | door ≤ 3,200 | 対象 profile の**性能目標** | `a` | RFC-0024・0025(Draft)。主契約ではない。3,120 → 3,200 は明示承認の対象 |
L38: | 混雑延長 `E_i` | **hard deadline の内側**での予約調整に限る:`a + 3,200 + E_i ≤ a + 10,000` | `a` | 本書の整理。RFC-0025 §4.3 の `E_max` は `≤ 6,800` に読み替える。それを超える延長は別の将来方針で、採用していない |
L39:
L40: - 10,000 を維持するので、**10,000 の内側で処理できる負荷だけを original claim の受付で引き受ける**。受け付けた後に、正当な court だけを容量不足で捨てない。例:64 KiB × 10,000 件を court 用 25,000 B/DAA で運ぶと、転送だけで 26,215 DAA かかる(RFC-0025 の仮定による算術)。この負荷は 10,000 の内側に入らないので、受付の段階で抑制する。
L41: - `10,000 − 3,120 = 6,880` は予算の差であり、最後に受理できる位置には等号・排他的境界の余裕を別に入れる。遅い申立て・毎回合法な最遅応答・最後の証拠・収録・回復の全部が残余に収まることを、機械的に検査する(T06)。
L42: - 3,120・10,000 は bond 全額の無条件返還時点でも、全証拠の削除時点でもない(§3.3)。
L43:
L44: ### 2.2 境界表(F09。値は未決、形式だけ固定)
L45:
L46: notice、root、各 round、choice、最終 fragment、`collect_until`、hard deadline、default sweep、refund / bond exit のそれぞれについて、次を表にして実装・試験で共有する:起点、単位(合意 DAA)、受理区間(閉/開)、等号、同一 block 内の処理順(canonical ordering)、reorg 後の再計算。RFC-0014 10-12 改定の `d <= deadline` は例示であり、`collect_until` は排他的(`daa < collect_until` で受理、既存 fold も `daa >= collect_until` を拒否)。両者は別の時計なので矛盾ではないが、同じ「期限」という語で置き換えない。
L47:
L48: ## 3. 事前予約と責任期間(F01・F07)
L49:
L50: ### 3.1 受付時の予約
L51:
L52: original claim を報酬対象として受け付ける前に、hard deadline までに必要な検証・開示・最終裁定・保持・回復の資源(bytes、検証 work、IO、保持 byte-DAA、control 数)と、料金・担保を予約する。指定の burst で全 claim が同時に争われる場合を評価する。確率的 overbooking は、全員共謀時の最悪保証として表示しない。
L53:
L54: ### 3.2 混雑と過負荷
```

```text
L66: ## 4. 無応答の帰責(F02、P0)
L67:
L68: ### 4.1 規範
L69:
L70: チェーン上の「期限まで有効な応答がない」は、次の 2 つで同じ履歴になり得る。
L71:
L72: ```text
L73: A:Miner が応答しなかった
L74: B:Miner は有効な応答を署名・送信したが、配送や収録を妨害された
L75: ```
L76:
L77: 署名は発言の帰属を認証するが、未収録の送信や受信時刻は認証しない。したがって、**チェーン上の不在だけから A だけを処罰し B だけを救済する判定は作れない**。無応答による default は、次の 4 つが揃ったときに限る。
L78:
L79: 1. accepted な義務、応答権限(誰の鍵か)、必要 bytes / work、予約、期限、適格な応答の完了条件が、合意状態にある。
L80: 2. 「予定された正規の処理機会」が、合意状態から決定論的に計算でき、その機会が与えられた。
L81: 3. 合意で観測できない個別の通信妨害は、明示した inclusion / network 前提として扱い、その前提の外側を保証しない。
L82: 4. 前提が崩れた場合の扱い(Final 保留、担保と証拠の保持、復旧)を、無応答 default とは別の経路として定める。
L83:
L84: - `service_ok` のようなフラグを置くだけでは解決しない。その根拠がローカル回線測定、Panel の申告、mempool の観測なら、全ノードで同じ判定にならない。
L85: - 有効な応答が来たときだけ時計を進める案は採らない(永久沈黙で時計も止まる)。逆に、clock や heartbeat で DAA が進むこと自体は応答機会の証明ではない(RFC-0008)。
```

### `consensus/core/src/palw_security_certification_v1.rs`

```text
L8: //! * **`SecurityCertified`** — the claim may add CONSENSUS credit: safe-frontier advance, safe weight, provisional/live weight,
L9: //!   settlement depth, rule-E participation, settled-anchor credit and the issuance-funded (coinbase) reward leg.
L10: //!
L11: //! A claim is `SecurityCertified` only by a **commit-then-draw, m-of-k positive attestation from the global verifier pool**:
L12: //!
L13: //! 1. **Commit** at `Final` (the claim, its producer and the participating stake are fixed: the population is cut at the commit DAA —
L14: //!    no bond registered within `verifier_maturity_daa` of it, and never the producer's own operator).
L15: //! 2. **Draw** `m` positions, stake-weighted with replacement, from that GLOBAL pool (never "the model's holders": a closed model's
L16: //!    registrant cannot fill it), with a seed from the first execution-lane seed anchor strictly later than the commit
L17: //!    (`round_seed_anchor`, ADR-0130) — randomness that did not exist when the claim was fixed.
L18: //! 3. **Attest** within `attest_window_daa`: each drawn bond re-executes the claim and signs a verdict (the RFC-0017 attestation object,
L19: //!    kernel inner 75, naming the certification id instead of an exam id).
L20: //! 4. **Decide**: `Certified` iff at least `k` positions attested `Reproduced` (no mismatch), fewer than `veto` positions attested a
L21: //!    negative verdict (`Unavailable`, `NotReproducible`, or a mismatch on the claim), the claim is still `Final` with no pending
L22: //!    accusation, and the G14 public-prosecution window (`prosecution_window_daa` after `Final`) has elapsed. Otherwise, at the
L23: //!    deadline, `NotCertified` / `Vetoed`; no population is `NoPopulation`; no draw by `certify_by_daa` is `Expired`. One attempt per
L24: //!    claim, counted per class; never retried.
L25: //!
L26: //! A **public-check path** (`PublicCheckCertified`) exists for classes with a pinned deterministic public check (G14-complete public
L27: //! material): [`PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1`] is EMPTY in this binary, so the path certifies nothing until a class is
L28: //! pinned with its measurement record.
L29: //!
L30: //! **PREMISE-BASED, not an unconditional proof (user decision 2026-10-15).** The pillar is the drawn verifiers' signed approval, under
L31: //! the explicit premise that **the honest fraction of drawn verifier stake is at least the threshold** (`k/m`). No cryptographic
L32: //! computation proof is a required condition. The f = 1/3, f = 0.6 and worst-case figures the RFC reports are the impact if the premise
L33: //! fails — not conditions that change the rule. A certified claim later convicted by G14 public prosecution is voided (its record
L34: //! leaves `Final`), and every reader of Gate C then reads it uncertified: its safe weight is taken back (`reverse_convicted_final`),
L35: //! its participation and settlement depth vanish.
L36: //!
L37: //! **Residual assumption (stated, not removed):** the honest fraction of drawn verifier STAKE. A closed (p = 0) model is never
L38: //! reproduced by an honest drawn verifier, so it certifies only if the adversary holds ≥ k positions and fewer than `veto` honest
L39: //! negatives arrive — probability [`palw_certification_false_pass_f64`] per claim at adversarial drawn-stake fraction `f`. Signatures
L40: //! are attestations, not recomputation proofs: a certificate is as strong as that assumption and the court behind it.
L41: //!
L42: //! Every value here is a NAMED, UNAPPROVED design value (RFC-0018 §11); validation refuses the fence armed.
```

```text
L500: ) -> bool {
L501:     active
L502:         && registered_daa.saturating_add(policy.verifier_maturity_daa) <= committed_daa
L503:         && collateral >= policy.verifier_min_collateral_sompi.max(1)
L504:         && operator != producer_principal
L505: }
L506:
L507: /// `m` stake-weighted positions with replacement over the pool (the RFC-0017 / R7-ECON draw). `None` on an empty pool.
L508: pub fn palw_certification_draw_v1(
L509:     seed: &EligDigestV1,
L510:     id: &EligDigestV1,
L511:     pool: &[PalwEligibilityVerifierV1],
L512:     m: u16,
L513: ) -> Option<Vec<EligDigestV1>> {
L514:     crate::palw_model_eligibility_v1::palw_eligibility_draw_v1(seed, id, pool, m)
L515: }
L516:
L517: /// Whether a class has a pinned deterministic public check (the additional path).
L518: pub fn palw_certification_public_check_v1(class_id: &EligDigestV1) -> bool {
L519:     PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1.contains(class_id)
L520: }
L521:
L522: /// The facts about the claim the verdict reads, at `now`.
L523: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
L524: pub struct PalwCertificationClaimFactsV1 {
L525:     pub still_final: bool,
L526:     pub accusation_pending: bool,
L527: }
L528:
L529: /// **The verdict** at `now` (`None`: undecided — wait). Order: a claim no longer `Final` withdraws; a draw that never happened by
L530: /// `certify_by_daa` expires; the veto; `k` positives with the prosecution window elapsed and nothing pending certify; the deadline
L531: /// fails the rest. A public-check class certifies once the prosecution window has elapsed with nothing pending.
L532: pub fn palw_certification_verdict_v1(
L533:     policy: &PalwSecurityCertificationPolicyV1,
L534:     row: &PalwClaimCertificationRowV1,
L535:     facts: PalwCertificationClaimFactsV1,
L536:     now: u64,
L537: ) -> Option<PalwCertificationOutcomeV1> {
L538:     use PalwCertificationOutcomeV1 as O;
L539:     if row.outcome.is_some() {
L540:         return None;
L541:     }
L542:     if !facts.still_final {
```

### `consensus/core/src/palw_security_certification_fold_v1.rs`

```text
L326:             return Err(refuse("the drawn bond already attested this certification"));
L327:         }
L328:         let mismatch = attestation.claim_mismatches.contains(&0);
L329:         if attestation.claim_mismatches.iter().any(|i| *i != 0) {
L330:             return Err(refuse("a certification examines one claim: only index 0 may mismatch"));
L331:         }
L332:         let positive = matches!(attestation.verdict, PalwEligibilityVerdictV1::Reproduced) && !mismatch;
L333:         if positive && attestation.transcript == [0u8; 64] {
L334:             return Err(refuse("a positive certification attestation commits a transcript"));
L335:         }
L336:         // G14 lane closure G-N2: a Reproduced attestor of an owning39 mining claim answers for it — a lock on its free collateral
L337:         // (the seat-lock relation over the claim's reserved exposure and the k-of-m quorum) through the claim's liability; a
L338:         // conviction charges it. An attestor that cannot back it is not counted.
L339:         if positive && key.first().copied() == Some(CERTIFICATION_NATIVE_MINING_PREFIX_V39) {
```

### `kaspad/src/palw_kernel_roles.rs`

```text
L620:         // -- Gate C drawn verifier (identity-armed) --
L621:         for claim in palw_kernel_cert_duties_v1(&route, ledger, &me, current_daa) {
L622:             use misaka_palw_kernel::model_eligibility_v1::{
L623:                 ELIGIBILITY_ATTESTATION_VERSION_V1, EligibilityAttestationV1, EligibilityVerdictV1,
L624:             };
L625:             let row = &ledger.claims[&claim];
L626:             let (verdict, transcript) = match model_of(&row.class_binding_id) {
L627:                 None => (EligibilityVerdictV1::Unavailable, [0u8; 64]),
L628:                 Some(model) => {
L629:                     let salt = misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-cert/salt/v1", &me);
L630:                     match misaka_palw_remote::pursuit::kernel_check_program_v1(ledger, &claim, model.as_ref(), salt) {
L631:                         Ok(None) => {
L632:                             let mut seen = Vec::with_capacity(128);
L633:                             seen.extend_from_slice(&claim);
L634:                             seen.extend_from_slice(&me);
L635:                             (
L636:                                 EligibilityVerdictV1::Reproduced,
L637:                                 misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-cert/transcript/v1", &seen),
L638:                             )
L639:                         }
L640:                         Ok(Some(_)) => (EligibilityVerdictV1::NotReproducible, [0u8; 64]),
L641:                         Err(why) => {
L642:                             warn!(
L643:                                 "[{PALW_PANEL}] Gate C: claim {}: not checkable here ({why}) — attesting Unavailable",
L644:                                 Hash64::from_bytes(claim)
L645:                             );
L646:                             (EligibilityVerdictV1::Unavailable, [0u8; 64])
L647:                         }
L648:                     }
L649:                 }
L650:             };
L651:             info!("[{PALW_PANEL}] Gate C: this drawn verifier attests {verdict:?} for claim {} (KNODE)", Hash64::from_bytes(claim));
```
