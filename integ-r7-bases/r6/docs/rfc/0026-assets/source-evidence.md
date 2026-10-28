# 根拠抜粋 — RFC全件整合性監査

対象ZIP SHA-256: `367c7bd16c9136fe434eacd2ae70c59dd2a63b5a5ce865f431b9347412288583`。
各引用の `L` はZIP内の原本の1始まり行番号。下記の署名・回数はソースの静的確認であり、テストの再実行やlive t12測定ではない。
指摘の意味・条件・修正案は [audit-report.md](audit-report.md) を参照。

<a id="f01"></a>

## F01 — 3,120／3,200／10,000／E_max の優先関係が一つに定まらない

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L925–L974

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  925 | ## 改定 — 2026-10-19(異議があった claim だけ延長する期限、応答義務に基づく勝敗、court 提出経路の liveness と DDoS 対策:採用方針)
  926 | 
  927 | **状態:** ユーザーが方針として採用(2026-10-19「この方針で進める」)。実装・敵対的試験・実ノード試験・有効化は未実施。数値(3,120・10,000・容量の割合)は設計の起点で、本番の定数ではない。2026-10-12 改定(X3 の固定期限)とは、「異議がない claim はその固定期限のまま」「有効な異議がある claim だけ、事前に定めた規則で関連期限を一括して延長する」という形で両立させる。延長の規則は新しい versioned fence で導入し、旧 claim には旧 ruleset を適用する。関連:[RFC-0024](0024-court-door-3200-to-ctx-2m.md)(door の短縮)、[RFC-0025](0025-scalable-objective-court-many-models-mass-disputes.md)(大量 court の容量予約と混雑延長)。
  928 | 
  929 | ### A. 対話の回数(16 分岐・1 tile 16 位置の場合)
  930 | 
  931 | 履歴 dissection の 1 往復は、Miner が子区間の部分集約値を開示し、Panel または外部検証者が不一致の子区間を選ぶ、の 2 投稿。tile 数は毎往復 `ceil(tile 数 / 分岐数)` に縮み、1 tile で終わる。
  932 | 
  933 | | context | 履歴 tile 数 | 最大往復数 | この探索部分の片道投稿数 | 縮まり方 |
  934 | |---|---:|---:|---:|---|
  935 | | 32K(32,768 位置) | 2,048 | 3 | 6 | 2,048 → 128 → 8 → 1 |
  936 | | 2M(2,097,152 位置) | 131,072 | 5 | 10 | 131,072 → 8,192 → 512 → 32 → 2 → 1 |
  937 | 
  938 | 途中で不正が確定するか、どちらかが応答しなければ、そこで終わり得る。court 全体はこれに、申立て、Miner の root 集約値の開示、最後の局所証拠の提出と裁定が加わる。held regime の時間予算は `1 + 2R + t`(1 = root claim、R = 上の往復数、t = 終端処理の予算)。t = 2 なら 32K で 9 手、2M で 13 手。**これは往復の数ではなく、証拠の分割を含む実際の tx 数・ブロック数とも一致するとは限らない。** 分岐数は ruleset から導出されるので、上の値は 16 分岐を指定した場合の計算であり、稼働中の t12 が 16 分岐であることの確認ではない。直接反証できる leaf では、証拠が揃っていれば履歴の対話自体が不要。
  939 | 
  940 | ### B. 通常の期限と、係争の最終期限を分ける
  941 | 
  942 | | 項目 | 扱い |
  943 | |---|---|
  944 | | 通常の異議受付期間 | original claim の受付から 3,120 DAA |
  945 | | 異議がない claim | 他の保持・責任条件も満たしたら、通常どおり担保を解放する |
  946 | | 有効な異議がある claim | 関連する担保・報酬・Final を保留し、対話型 court を続ける |
  947 | | 係争の最終期限 | **original claim の受付から** 10,000 DAA |
  948 | | 各手番の応答期限 | 10,000 とは別に定め、応答義務者が守る |
  949 | | 終了後 | 裁定結果と、残る保持・復旧義務に従って担保を解放または徴収する |
  950 | 
  951 | - 起点は original claim の受付時とする。3,120 の直前に申し立てられても、残りは `10,000 − 3,120 = 6,880 DAA`。受入条件は、**遅い申立てでも、残りの対話・証拠配送・最終収録・回復の余裕が 6,880 DAA に収まること**。
  952 | - 「申立てから 10,000」を与える案は、original からの拘束が最大約 13,120 DAA になる別の仕様で、採らない。
  953 | - **bond の返還時点だけを延ばしても足りない。** 証拠の受付が 3,120 で閉じれば、後から証明できない。延長の対象には、court の最終期限に加え、該当経路の `collect_until`、開示義務、資料保持、Final の保留、bond の解放条件を含め、1 つの原子的な状態更新で動かす。現行の公開 allocation 経路は `material_until = collect_until + reorg_retention`、`bond_exit_after = material_until` と連動しており、clock を実行時に延長しない前提で書かれている。したがってこれは定数の変更ではなく、**有効な申立てを契機に関連期限を更新する、明示的な新規則**である。
  954 | - 同じ bond が別の claim の責任も負っている場合、このcourt の終了だけで全額は返らない。解放は、claim ごとの責任と予約を基準にする。
  955 | 
  956 | ### C. 「証明できなければ slash」を分ける
  957 | 
  958 | | 起きたこと | 判定・処理 |
  959 | |---|---|
  960 | | 局所再計算で、Miner の主張との矛盾が証明された | 計算不正として Miner 側を slash |
  961 | | Miner が、応答可能な正規の開示要求に期限内に応答しなかった | 開示・応答義務違反として責任者を slash / default 処理 |
  962 | | Miner は必要な応答をしたが、Panel が不正を証明できなかった | **Miner を slash しない。** 申立てを棄却し、Panel 側に規定の敗訴・費用処理 |
  963 | | Panel が次の区間を選ばない、または最後の証拠を出さない | Panel 側の放棄・期限違反として処理 |
  964 | | 正直な当事者に必要な収録容量が与えられず、手続きが終わらなかった | それだけを計算不正の根拠にしない。容量・期限保証の設計不備として扱う(E) |
  965 | 
  966 | - 最後の tile まで正しく絞れた後に Panel が証拠を出さずに期限を迎えた場合、「Miner の正しさが証明されなかったから Miner を slash」としてはならない。Panel が最後に何もしないだけで、正しい Miner を負けさせられるためである。既存仕様は既にこれを区別している:終端では Miner に新たな応答時計を課さず、誰も最後の証拠で閉じなければ、session の最終期限で challenger 側の敗北として終わる。途中の応答待ちでは、応答義務のある側が期限を超えればその側が負ける。
  967 | - 既存コードの、敗訴した challenger の担保からの費用徴収と、fence に応じた係争時間の課金を、新しい期限に整合させて使う。新しい勝敗原則は作らない。
  968 | 
  969 | ### D. 延長の濫用を防ぐ条件
  970 | 
  971 | - **申立てだけで、無料・無制限に他人の bond を拘束できないこと。** 申立てには、対象 claim・不一致の箇所・署名・申立人の担保・費用を必要とし、同じ争点の重複は統合する。ただし、申立ての時点で完成した不正証明は要求しない(これから対話で証明する経路が使えなくなるため)。
  972 | - 申立て権を Panel に独占させず、適格な外部検証者 1 人にも同じ経路を残す。
  973 | - **10,000 まで何度でもやり直せないこと。** 各往復で区間が縮まり回数の上限が決まる構造を保つ。新しい message や再申立てで 10,000 の時計をリセットしない。
  974 | - **各手番の期限を残す。** Miner が最初の応答を 9,999 まで遅らせて、Panel に選択・証明の時間を残さない進行を認めない。全体期限と各手番の期限は別の制約。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L3–L20

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
    3 | - **状態:** Draft / Review Required / 未実装・未有効化。2026-10-19、ユーザー指示により RFC-0025 として登録。
    4 | - **主目的:** モデル数と不正に対する court 件数が増えても、訴追権・客観的裁定・担保回収の可能性を失わず、有限の混雑を処理して復帰できるプロトコルを定める。
    5 | - **通常時の目標:** original claim の `accepted_daa` を起点として `door ≤ 3,200 DAA`(3,120 → 3,200 の変更は明示的な承認対象。§16)。
    6 | - **混雑時の方針:** 合意状態から導出した処理予約に基づき、必要な案件の証拠提出期限・Final 保留・証拠保持・bond 返還可能時点を連動して延長する。全件の固定 3,200 は約束しない。
    7 | - **前提とする RFC:** [RFC-0024](0024-court-door-3200-to-ctx-2m.md)(A=最終裁定の 1 位置化、B=2 段先読み開示、C=先行取得と強制開示、付録 C=逐次 step 搬送の削減)。本文中の「前版」は RFC-0024 を指す。
    8 | - **関連する既存実装:** `consensus/core/src/palw_court_capacity_v1.rs`(休眠。court 専用の block 予算・予約枠・期限順の割当・新規 claim の受付制限・囮の価格・未裁定は無報酬。値は仮)。本 RFC の §5〜§7 はこれを一般化・強化する位置づけで、統合時に対応を照合する(§15 段階 0)。
    9 | - **非変更:** ZK・SNARK/STARK・zkVM・PCS による計算の正しさの証明は使わない([PRINCIPLES](../PRINCIPLES.md) §8)。モデル単位の審査は行わない(PRINCIPLES §9)。本文 §15 段階 1 の「モデル profile を審査」は、claim 受付前に class の形状が証拠上限に収まるかを決定論的に判定することであり、モデルの品質・公開状況の審査ではない。
   10 | - **付属資料:** [0025-assets/](0025-assets/README.md)(ADR 草案、source-audit-v2、capacity_planner.py と 27 試験。登録時に 27 試験の合格を再確認)。前版の資料は RFC-0024 の [0024-assets/](0024-assets/)。
   11 | 
   12 | ---
   13 | 
   14 | > **決定案:** 個々の証拠と検証作業を小さく有界にする。その上で、有限の処理容量を予約・配分し、混雑時には時間と担保・保持責任を一緒に延ばす。容量限界を超える新しい経済的責任の受付を抑制し、既に受け付けた不正の訴追を優先して処理する。ブロック容量の拡張は、実測・安全性審査を経た別の容量 profile として行う。
   15 | 
   16 | ## 0. 前版との関係・根拠
   17 | 
   18 | 本書は前版の A＝最終裁定の1位置化、B＝最大2段の先読み開示、C＝先行取得と強制開示を引き継ぎ、モデル横断の容量管理・混雑延長・pruning/IBD を追加した**統合版**である。実装では本書を上位とし、衝突しない算術・証拠認証の詳細は [前版RFC](0024-court-door-3200-to-ctx-2m.md) を参照する。
   19 | 
   20 | 前版の「全受理負荷で必ず3,200以下」「期限を途中で延長しない」は、v2 の **通常予約と、事前合意済みの有界な混雑延長**に置き換える。待ち行列の開始を遅らせて、元の受付からの時間を隠すことは禁止したままである。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L175–L199

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  175 | 
  176 | `notice_until`と`collect_until`を分離する。有効noticeの受付窓の後にも、反証作成・応答・最終収録に必要な時間を予約する。`notice_until`は最小検査時間と所定収録機会を奪わない形でprofileから導出する。
  177 | 
  178 | 有効noticeの到着がその許容窓の最終位置だった場合も、`F_i`を守れることを要求する。準備や異議検出に必要な仕事を開始前へ移して計上から消さない。modelを事前取得済みとするprofileはその前提を明記し、完全cold profileとは区別する。
  179 | 
  180 | `collect_until`は排他的境界なので、terminalは`now < d_i`で収録する。境界ぴったりに収録してよいとするoff-by-oneを禁止する。[V2-S02](0025-assets/source-audit-v2.md#v2-s02)
  181 | 
  182 | ### 4.3 通常・混雑・過負荷
  183 | 
  184 | - **Normal:** 予約済みの全phase・冷状態・保証対象負荷が`a_i+3,200`に収まる。`E_i=0`。
  185 | - **Congested:** 審査済みの有限burstにより将来の処理枠を予約する。`0<E_i≤E_max`。その案件の期限と責任を連動延長する。
  186 | - **Overload admission:** 新しい経済的claimを受けると`E_max`、保持予算、active state上限、control容量を超えるなら、その新規受付を抑制する。既存の有効courtを捨てない。
  187 | - **Service failure:** 合意されたservice envelope自体が崩れた場合は別扱い。通常の混雑達成として報告せず、安全なFinal保留と復旧手続きへ移る。無制限障害時の有限返還を約束しない。
  188 | 
  189 | `E_max`は一律10,000という暫定値で埋めず、処理容量、burst契約、保持・担保費用、復旧条件から決める。10,000件のburstを対象にするなら、その最終案件まで収容できる値かを確認する。
  190 | 
  191 | ### 4.4 1 originalに複数courtがある場合
  192 | 
  193 | 各courtは個別の`d_i`と予約を持つ。original frameの実効`collect_until`と経済的保留は、そのframeを参照する有効な未解決courtと元の責任窓から必要な最大値を導出する。
  194 | 
  195 | ```text
  196 | frame_collect_until = max(base_frame_collect_until, applicable_court_deadlines)
  197 | ```
  198 | 
  199 | frameの保留が伸びても、既に走っている別courtの個別deadlineや`notice_until`を一緒に延ばしてはならない。全ての判定入口はv2の個別期限・frame保護を正しく参照し、v1の単一termsだけを読むconsumerを残さない。
```

### `docs/rfc/0024-court-door-3200-to-ctx-2m.md` L571–L574

原本SHA-256: `6f59e3c4e2c44b7ad3613d6326e5c0cae80764a6708a957dd9765d95bafe9a30`

```text
  571 | ### C.3 同時発生と証拠サイズ
  572 | 
  573 | - 単発の door だけでなく、同時に N 件の court が走るときの `max_i door_i(2M, N) ≤ 3,120` を受入条件とする。court の容量は `palw_court_capacity_v1` が扱う(court 専用の block 予算、予約枠、期限順の割当、新規 claim の受付制限)。ただし値は仮で、step block の搬送がその予算に計上されているかは未確認。
  574 | - court 1 件の証拠サイズの最悪値を、固定上限として証明する。door が小さくても、証拠量が大きければ 1,000 件同時発生時の帯域問題は残るため。
```

<a id="f02"></a>

## F02 — 「正規の収録機会があった」を全ノードが判定する条件が閉じていない

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L976–L1027

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  976 | ### E. court 提出経路の liveness と DDoS 対策
  977 | 
  978 | 目的:Miner が通信攻撃で応答できなくなり、不正をしていないのに slash される事態を防ぐ。同時に次の 3 つを成り立たせる。
  979 | 
  980 | - 攻撃者が、court の提出経路を大量の tx で占有できないこと。
  981 | - 正当な Miner と外部検証者が、期限内に必要な提出を行えること。
  982 | - 正規の提出機会を与えられなかった Miner を、単なる期限切れで slash しないこと。
  983 | 
  984 | 少額の fee だけでは DDoS 対策として不十分。Miner に毎回高い fee を課すのではなく、悪意ある大量申立ての費用を攻撃者側に負担させ、正当な応答のためのブロック容量を予約する。
  985 | 
  986 | | 仕組み | 設計 |
  987 | |---|---|
  988 | | court 応答用の予約帯域 | 通常 tx の混雑で Miner の応答枠が消えないようにする。割合(例:通常 65% / court 応答 25% / 申立て 10%)は説明用の仮定で、容量試験で決める |
  989 | | 少額の応答 fee | 有効な court の正規応答は低コストにする |
  990 | | challenger bond | 根拠のない大量申立てに経済的負担を課す |
  991 | | 複数経路による提出 | Miner 本人だけでなく、署名済みの応答を第三者も relay できるようにする |
  992 | | 期限と収録保証の連動 | 正規の収録機会がないことを、Miner の計算不正と同一視しない |
  993 | 
  994 | - 予約帯域は、mempool の優先順位だけでは足りない。ブロック作成者が無視してよい優先順位では、悪意ある作成者の検閲を防げない。合意規則による容量予約、収録義務、または検証可能な収録機会の仕組みまで設計する(Ethereum の inclusion list の研究と同じ方向)。既存の `palw_court_capacity_v1`(休眠、値は仮)の court 専用 block 予算・予約枠・期限順の割当・新規 claim の受付制限を土台にする。
  995 | - **fee の負担:**
  996 |   - Miner の正規応答:低額の基本 fee のみ。各手番の応答回数・bytes を制限し、必要な応答容量は事前に予約する。
  997 |   - Panel / 外部検証者の申立て:申立て bond と、予想される court 処理費用を事前に拘束する。
  998 |   - 敗訴・悪意ある大量申立て:客観的な敗訴規則に基づき、費用を申立人側に配分する。
  999 |   - 不正が成立した申立て:正当な訴追者が費用で不利にならないよう、成功報酬・費用補填を設ける(OP Stack の fault dispute game で、手番に必要な bond を反証コストと結びつける設計と同じ方向)。
 1000 | 
 1001 | **DDoS を受けたときの判定:**
 1002 | 
 1003 | | 状況 | 処理 |
 1004 | |---|---|
 1005 | | Miner が正規の収録機会を与えられ、期限内に応答しない | 応答義務違反として default / 責任処理 |
 1006 | | court 帯域が混雑し、予約済みの収録機会を提供できない | 合意規則に基づいて期限を補正する。単なる計算不正として slash しない |
 1007 | | Miner がローカルで DDoS を受けたと申告しただけ | 無条件には延長しない(虚偽の延長請求を防ぐ) |
 1008 | | Miner が無効な証拠を提出した | 無効な応答として処理し、期限をリセットしない |
 1009 | | Miner の署名済み応答を第三者が提出した | 認証と期限条件が正しければ受理する |
 1010 | 
 1011 | 全ブロック作成者が検閲し、Miner がネットワークから完全に隔離される状況まで、有限の期限内での救済を無条件には保証できない。保証できるのは、明示したネットワーク・正直な収録者・容量の前提が成り立つ範囲である。ネットワーク全体の障害と Miner 自身の応答拒否は、chain 上の「応答がない」という事実だけでは区別できない。だから slash の条件を、必要な収録機会が成立したことに結びつけ、その保証が成り立たないときの安全な処理を明文化する。
 1012 | 
 1013 | **同時大量 court:** 既存の court 経路は、`同時 session 上限 = 各手番の期限 × 1 ブロックあたりの court 処理枠` で容量を導く。session 全体の期限や bond の解放だけを 10,000 にしても、各手番を処理する容量や同時受付の上限は増えない。例として、総証拠 32 KiB の court が 1 万件、court 用帯域が 12,500 B/DAA なら、転送だけで約 26,215 DAA かかる(仮定上の算術)。10,000 への延長だけで任意の大量発生は吸収できないので、予約・待ち行列・新規 claim の受付制御、または容量の増強が引き続き必要(RFC-0025)。この構成なら、Miner の bond を常に 10,000 DAA 拘束する必要はない。
 1014 | 
 1015 | ### F. 追加する合意仕様(Court Submission Liveness & Anti-DDoS)
 1016 | 
 1017 | 1. 有効な court の各手番に、最大 bytes・検証 work・必要な収録容量を予約する。
 1018 | 2. 正規の応答には低額で予測可能な fee を適用し、申立人の bond または費用預託から court の予約資源を賄う。
 1019 | 3. 不正な申立ての大量生成、同一争点の重複、無効な証拠の投入による容量占有を、有界な受理規則と費用で抑止する。
 1020 | 4. 提出者以外による署名済み応答の relay を許す。
 1021 | 5. 正規の収録機会を保証する scheduling を定義し、それを満たせない混雑経路では、事前に承認された規則で期限と bond の拘束を整合させる。
 1022 | 6. 提出しなかった事実だけから計算不正を推定しない。正規の応答義務違反は、別の客観的 default として裁定する。
 1023 | 7. 大量 court、tx flooding、検閲、ネットワーク分断、reorg を含む実ノード試験を有効化の条件にする。
 1024 | 
 1025 | ### G. 受入条件
 1026 | 
 1027 | 最終的な受入条件は「期限が来たから誰かを slash できる」ことではなく、**「正直な Miner と正直な検証者のどちらにも、所定の資源で義務を果たす時間が確保され、違反した側だけを客観的に処罰できる」**ことである。10,000 DAA で実際に完了できることは未測定。1 位置化などの証拠削減(RFC-0024)は、期限・担保・証拠保持の整合、応答義務に基づく勝敗、混雑下での収録機会の確保の上に乗る最適化として扱う。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L262–L270

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  262 | ### 5.5 容量枠と実際の収録保証は異なる
  263 | 
  264 | ブロックの上限やcourt用空き容量を定義することは、特定の未収録transactionが必ず入ることの証明ではない。ローカルmempoolにしか存在しないtransactionを「含めなかった」としてブロックをinvalidにしてはならない。
  265 | 
  266 | 本書の保証には、acceptedな義務・予約に対応するpublic carrierへの有界な収録規則または明示的なbase-chain inclusion前提が必要である。consensusで強制できる容量隔離と、proposerの収録行動・ネットワーク配送の前提を分けて記録する。
  267 | 
  268 | 応答時計は**予定された正規の処理機会**に基づいて進める。有効bytesが届いたときだけ時計を進める方式は禁止する。相手が永久に沈黙すると時計も永久停止するためである。
  269 | 
  270 | 逆に、合意上必要な収録機会が与えられなかったことを、相手の資料不提供と同一視しない。service違反の扱いと参加者defaultを区別する。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L51–L60

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
   51 | ### 1.2 条件付きの生存性
   52 | 
   53 | 通常時3,200 DAAおよび混雑時の有限完了は、次の明示的な条件の下で保証する。
   54 | 
   55 | 1. base chainが進行し、適格なcontrol/noticeと証拠に対する有界な収録機会がある。
   56 | 2. ネットワーク・CPU・IO・reorg・復旧が、承認済みservice envelope内にある。
   57 | 3. 新規流入はadmissionによって有界化され、既存予約の容量が後続流入に奪われない。
   58 | 4. 1人の検証者が追う案件について必要な検証者資源と費用が、当該profileに含まれる。
   59 | 
   60 | 「全員共謀」はproducer等の協力を前提にしないという意味であり、**全ブロック生成主体による永久検閲、無限の通信断、無制限の有効申立てを有限容量で全て即時処理することまで保証しない**。1人が1案件を訴追できることと、その1人が1万案件を並列処理できることも別である。
```

### `docs/rfc/0008-palw-claim-backed-consensus-blocks.md` L140–L150

原本SHA-256: `5e6df30068ead9de1eb85c4d3bb14d10f4df49314a105f443c4d68f188591b31`

```text
  140 | root expiry、missing DA、verification backlogではpending workをtimeout/voidの規則で処理し、検証の沈黙をFinalにしない。rootやsliceのfraudは該当するstate依存の後続sliceを無効化する。証拠・義務に基づくconvictionのみでslashし、verifier不在やlocal timeoutをproducer arithmetic fraudにしない。
  141 | 
  142 | ## 5. Anchor、reorg、livenessの分離
  143 | 
  144 | EXECは既存laneと同じchainへのattachmentを持つ。sliceは**root claimのanchor**と**carrierのchain anchor**を別々に拘束し、canonical branch上のactive rootのみを対象とする。reorgでroot/anchorが外れたsliceは新しいbranchのworkとして自動採用しない。再attachmentはversioned再検査を必要とし、同じworkを二度settleしない。
  145 | 
  146 | 共通EXEC acceptanceは、chainのparent stateから決定的にcovered set・順序・資格を検証してfoldする。v2のanchor closureはconsensus-parent walkから分離し、stale headやslice burstがchain template・merge depthを塞がないようにする。既存v1のmergeset経路と、将来のanchor closureを混ぜず、具体的なwire commitmentと互換性は実装仕様で固定する。
  147 | 
  148 | 不正なEXEC blockは拒否し、laneで資格を失ったcarrierはそのlaneの受理対象から外す。producerが勝手に送った不正sliceだけでchain blockやrootを無効にしない。chain blockが宣言したanchor commitment自体が不正なら、既存のblock commitment検査に従う。slice fraudはworkの検証・精算を止めるが、独立したEXEC_TXのpermitや確定済み取引をsliceの結果に依存させない。
  149 | 
  150 | heartbeatとBASE-0はsliceのpending数、検証結果、header種別、node-local producer観測を読まない。court、DA、verifier、長い推論の全てが停止しても、現行mainと同じ規則でchainとdeadline machineryが進む。bounded queues、reserved chain-validation資源、EXEC_SLICE独立quotaを設け、slice floodでheartbeat、BASE-0、REAL、EXEC_TXをstarveさせない。
```

<a id="f03"></a>

## F03 — 料金・有限枠と、外部検証者1人の訴追権を両立させる規則が未完

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L449–L470

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  449 | ### 7.4 sessionの先取りで正直なverifierを排除しない
  450 | 
  451 | 現行ExecutorRefuted adjudicatorはopen_courts_of > 0でClaimUnderSessionを返す [S03]。
  452 | 既存の単一session規則をそのまま安全性の前提にしてはならない。
  453 | 悪意challengerが先にcourtを開き、本物の不正証拠を受理不能にする競合を新fenceで閉じる。
  454 | 
  455 | 新経路では、既に組み立てられた適格なdirect objective proofは、
  456 | 別bondのcourt/sessionが開いているという理由だけで拒否しない。
  457 | gateとstate foldの両方を変更し、同じclaim/root/rulesetに対する証拠を完全検証した後、
  458 | conviction・未処理session終了・担保/exposure清算・deadline解除を一つの決定的transitionとして行う。
  459 | court中の発言だけで他sessionを消すことは許さない。
  460 | 
  461 | 予約はclaimの排他的な所有権ではない。認証された開示は全参加者が再利用でき、
  462 | 別の適格なverifierも必要なprogressまたはexact proofを提出できる。
  463 | 同じsiteで最初にsessionを開いたbondだけが次の証拠を提出できる仕様を禁止する。
  464 | 開示済みの正しいunitを使って別siteの不正を追う場合も、claim単位のabsolute deadlineは維持する。
  465 | 
  466 | session cap・deposit・feeだけでは、資金無制限のSybilによる資源独占を防いだとはいえない。
  467 | interactive枠の選択、共有progress、direct-proof優先処理と総work capを定義し、
  468 | 先取り・全枠占有・別site誘導の試験を必須にする。
  469 | それでも必要carrierがinclusion bound内に受理されない状況は§2の仮定外であり、
  470 | 「一人いるだけで安全」という無条件保証に読み替えない。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L226–L250

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  226 | ### 5.2 original claim受付で責任容量も確保する
  227 | 
  228 | 新しいoriginal claimを報酬対象として受理する前に、そのprofileの検査窓・準備・保護notice・許容する敵対的分岐・最悪court・保持と回復を予約する。全claimを先に無制限受理し、後からcourtだけを容量不足で拒否する構造は禁止する。
  229 | 
  230 | 異議が生じないときの未使用容量は他のbest-effort処理に貸してよい。ただし、予約を行使された時点で取り戻せることと、既に開始した非preemptible処理の最大遅延を含めて保証する。
  231 | 
  232 | 複数claim間で同時不正が起きないという仮定は使わない。指定burstの全claimが同時にcourtを必要とする場合をadmissionで評価する。確率的overbookingを採用するなら、それを全員共謀時のworst-case保証として表示しない。
  233 | 
  234 | 実装の強化前に、既存owner上限等によってこの予約モデルを満たせないprofileを無理に有効化しない。
  235 | 
  236 | ### 5.3 予約カレンダー
  237 | 
  238 | 各courtのphaseテンプレートは、依存順序、最大bytes/work、相手の合法応答窓、所定収録余裕、terminal/default、回復を含む。frame受付時の最悪notice時刻と、実際のnotice受理後の残余予約を整合させる。
  239 | 
  240 | accepted chainの決定的な順序から`reservation_seq`を割り当て、空き処理枠へ予約する。local mempool順、local clock、見えたpeer数、operatorの判断は用いない。予約は合意stateに記録し、再起動・reorg・IBDで再生可能にする。
  241 | 
  242 | **先行案件の予約を、後から到着した通常の申立てで後ろへ動かしてはならない。** 早期完了分で後続処理を前倒ししてよいが、参加者への応答義務・deadlineを不意に短縮しない。
  243 | 
  244 | 資源`r`と任意のカレンダー区間`[u,v)`について、予約需要が、その区間で供給可能な下限と既存予約を超えないようにする。
  245 | 
  246 | ```text
  247 | Σ demand_of_reserved_phases(r,[u,v)) ≤ guaranteed_supply(r,[u,v))
  248 | ```
  249 | 
  250 | これはphaseのrelease時刻、相手の応答依存、最大service gap、fragmentの非preemptible時間、control枠を考慮したチェックでなければならない。期間全体での総bytes一致だけでは十分でない。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L333–L355

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  333 | ## 7. 外部1人の訴追権と大量濫用
  334 | 
  335 | ### 7.1 owner枠占有を安全性の穴にしない
  336 | 
  337 | 既存のK_owner=8とidle回収は、全員がactiveに動く共謀を排除できない。[V2-S04](0025-assets/source-audit-v2.md#v2-s04) v2では、枠を持つownerだけが反証できるという構造から分離した、permissionlessな反証提出・共用開示への接続が必要である。
  338 | 
  339 | 共通の認証済みmaterialは誰でも再利用できる。同じclaimの不正を直接示す有効witnessを、既存ownerの署名・承諾・辞退に依存させない。局所化が必要な検証者にも、control受付と処理予約を含む経路を残す。
  340 | 
  341 | **「外部検証者用1枠」と名付けるだけでは不十分。** 共謀者も別アドレスで取得できる。identity別の優先枠、料金、単なるFIFOだけでこの問題が解決したとは扱わない。
  342 | 
  343 | ### 7.2 boundedな対話とdedup
  344 | 
  345 | 同じoriginal claim、同じroot、同じ争点・範囲・rulesetの再提出は、canonical keyでまとめる。異なる証拠や独立した不正を「同じモデルだから」とまとめて捨てない。counterclaim、branch、最後の反証を含む敵対的増幅をprofileに計上する。
  346 | 
  347 | 有効/無効の判定に仕事が必要な以上、「正当なものだけ無制限に無料受付」は採用しない。一方、capacity capを共謀者が先に埋めれば正しい外部1人を排除できる、という設計も採用しない。
  348 | 
  349 | admissionの保証範囲は、**正規noticeの収録機会が与えられ、既存責任について予約されたboundedな対話経路**である。その経路に対して全ての敵対的分岐・枠占有を展開しても反証できることを、形式化または反例探索と実ノード試験で示す。未証明なら`OneOutsiderCapacityIncomplete`としてarmingを拒否する。
  350 | 
  351 | ### 7.3 停止すべき入口
  352 | 
  353 | 容量が不足するときは、まず新しい報酬対象original claim、optional work、新しい保持義務を抑制する。既存の有効noticeのdeadline・権利を消して帳尻を合わせない。
  354 | 
  355 | 新規モデル登録が共通資源を増やすなら、登録CPU・active metadata・保持負担にも上限と料金を設ける。モデル数を増やしてcontrol枠や検証枠を無料で取り直せない。
```

<a id="f04"></a>

## F04 — MinerのPC停止と、後から発生するMiner署名付き応答の接続が不足

### `docs/rfc/0009-palw-remote-miner.md` L24–L41

原本SHA-256: `6e7e1d57b99d00df635a86c475c6d917714d18640ab3bc885e9efa421810d2ff`

```text
   24 | **miner 自身が full `kaspad` を立てずに claim し、正当に得た報酬を回収できる設計として進めてよい。** pool は必須ではない。ただし「署名済み claim + 任意 relay」だけでは目標に届かない。現在の free-prompt lane は Panel が producer の node から material を取得し、`Final` 後の winning quantum は**同じ bond の producer が receipt block に使う**。この二つを protocol 上で切り離す必要がある。
   25 | 
   26 | **モデル登録者も、自分の full node や Panel を立てずにモデルを追加できる経路を提供する。登録は無料ではなく、ユーザーが BILI の GAS と登録に必要な bond 資金を用意する。** ローカル/委託 builder が artifact と登録 object を作り、ユーザーが自分の鍵で登録 object と funding transaction を署名し、任意 node/relay が提出する。node-less はノード運用を不要にするだけで、登録の審査・署名・経済条件をなくさない。詳細は §3.3–§3.6 と [RFC11 §11.2](0011-permissionless-model-and-long-context-onboarding.md#112-exact-duplicates-must-be-idempotent-without-inventing-a-new-mandatory-registry)。
   27 | 
   28 | 推奨する最終形は次のとおり。
   29 | 
   30 | ```text
   31 | miner: 計算・鍵・bond・署名
   32 |   ├─ 完成済み claim/carrier または署名済み attempt block → 任意 node/relay
   33 |   ├─ content-addressed evidence → 複数の DA provider
   34 |   └─ quantum の公開 redemption authorization → 任意の block builder
   35 | 
   36 | Panel → DA provider から material を取得して検証
   37 | chain → claim の executor bond に帰属を固定
   38 | receipt block → 誰が組み立てても miner の payout に報酬、builder に明示的な手数料
   39 | ```
   40 | 
   41 | miner は chain の必要情報を取得・検証し、計算と署名が終われば PC を止められる。一方、node、Panel、DA、block builder はネットワーク上で稼働し続ける。relay と DA はインフラの機能であり、既存の PALW Panel seat の権限を得ない。
```

### `docs/rfc/0009-palw-remote-miner.md` L59–L63

原本SHA-256: `6e7e1d57b99d00df635a86c475c6d917714d18640ab3bc885e9efa421810d2ff`

```text
   59 | 
   60 | 1. miner に full node、公開受信 port、常時 material server、`Final` 後の常時 receipt producer を要求しない。
   61 | 2. miner の鍵と bond、claim の帰属、登録済み payout は relay・provider・builder に渡さない。pool を必要条件にしない。
   62 | 3. 現行の未来 anchor、Panel 選定、verdict、court、保留・`Final`・void、量子抽選、使用窓、chain 候補ごとの一意性を保持する。
   63 | 4. material 欠落と不正計算の責任を、証明できる範囲で分ける。ローカル timeout だけを slash 証拠にしない。
```

### `docs/rfc/0009-palw-remote-miner.md` L145–L168

原本SHA-256: `6e7e1d57b99d00df635a86c475c6d917714d18640ab3bc885e9efa421810d2ff`

```text
  145 | ## 4. miner から独立した evidence 配信
  146 | 
  147 | ### 4.1 manifest と取得
  148 | 
  149 | claim が現在 bind する `trace_root`、`output_root`、`execution_root`、`trace_chunk_count`、retention deadline に対応する material を、claim 提出前に複数 provider へ配置する。`EvidenceManifestV1` は network、claim id または衝突しない preclaim id、各 root、chunk の順序/length/hash、encoding、最大展開サイズ、保持 DAA を正規化して commit する。claim id が manifest hash を含む場合は循環参照にならない preclaim id を定義する。Panel は任意 provider から必要な chunk/opening を取り、root と claim binding を検証してから既存の再実行・court に渡す。provider の署名だけで material の正しさを判定しない。
  150 | 
  151 | provider は鍵、bond、報酬の代理人ではなく byte 配信者である。storage receipt には provider identity、manifest root、対象 chunk 集合、保持期限、network、署名を含める。必要な複製数、byte cap、同一運営者の重複、rate limit、discovery、料金は実測と攻撃予算から fence ごとに固定する。claim固有証拠の配信scopeをmodel weights取得に拡張しない。MISAKA Torrent/専用Seederは本設計から廃止する。
  152 | 
  153 | **新profileの適用境界（2026-10-06）:** 既存profileへの配信は既存replay/courtを維持する。
  154 | 将来の大型claimは[RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md)・
  155 | [RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md)の小さいconstraint検査へ認証済み証拠を渡し、
  156 | 異常時のみ有界exact courtを使う。manifestはclass/kernel/plan/suiteと証拠をbindし、proof/openingの配信・保持費用も
  157 | 計上する。[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)のKernel更新境界を
  158 | remote executionで迂回せず、未対応演算をVMで処理した結果を有効なclaimとしない。
  159 | 
  160 | ### 4.2 可用性と責任
  161 | 
  162 | storage receipt は**保管の約束**であり、将来の全 Panel からの取得可能性の証明ではない。初期段階では claim の従来の producer 責任を残し、外部配信を node 不要化の運用改善として試す。miner を配信責任から外す fence は、provider 側の bond と客観的な不履行証明が動いた後に限る。
  163 | 
  164 | その fence では、Panel/court が特定 chunk を chain 上で challenge し、provider は deadline 内に hash に合う opening を提出できる。無応答なら provider の担保が対象となる。複数 provider が共通障害で沈んだ場合の claim は無報酬または失効とし、既に正しく計算した miner の fraud slash と区別する。偽 root・不正計算は miner の責任に残る。**単一 Panel の timeout、ローカル fetch 失敗、provider の事前署名だけでは slash しない。** 現行の producer withholding / `DefaultAccused` 経路を変更する際は、旧 claim と新 claim の責任境界を version/fence で分け、同一 failure に二重 slash をしない。
  165 | 
  166 | この客観的 court を用意できないなら「miner は送信後に必ず PC を切れて、配信事故の slash リスクもない」とは約束しない。その場合は miner 自身が provider を手配して責任を持つ段階で止める。
  167 | 
  168 | **実装（lane DA16、2026-10-10 に ADR-0177 で再範囲化）:** provider court は claim 固有 material だけを対象にする（tags 150–153、休眠 fence `palw_provider_court_v1`、kernel route aux table 43–45）。lease の subject は kernel claim のみで、challenge unit は claim の committed position のみ。モデル／artifact bytes の可用性 lease（旧 `Artifact` subject）、READY/LAPSED、tag 104 の可用性 gate、モデル bytes 不提供への charge は合意から撤回した（bytes は decode 可能なまま fence 以下で従来どおり無判定で通り、fence 以上では常に拒否）。provider の charge のうち Final 前は challenger／demander に PALW reporter share（ADR-0032 の 49%、kernel route の accuser・demander と同率）を払い残りを burn、Final 後は全額 burn（自作自演の lapse でも損失は自己申告の有罪と同じ 51% 以上）。共通障害は claim を失効させ有罪にしない。偽 root・不正計算は miner の責任に残る。どの court も compel できる unit は claim 固有のものだけで、累積 scope（モデル bytes 開示 0、unit ごとに 1 回、(claim, 要求 operator) ごとに 16）を `palw_court_scope_v1` が一元判定する。設計・試験は [DA16 design](../design/palw/verification/da16-transport-and-provider-court.md) §2・§7。状態は implemented（armable ではない）。
```

### `docs/rfc/0009-palw-remote-miner.md` L190–L198

原本SHA-256: `6e7e1d57b99d00df635a86c475c6d917714d18640ab3bc885e9efa421810d2ff`

```text
  190 | ## 7. 導入順と受け入れ条件
  191 | 
  192 | | 段階 | 内容 | 合格条件 |
  193 | | --- | --- | --- |
  194 | | A0: remote model registration | prepare/preflight・live terms/費用明細・ローカル署名・有料 carrier relay・accepted registry state の追跡を SDK/CLI/Web に分離する。Artifact/metadata は認証できる公開経路から取得する。 | `kaspad`・Panel がないユーザー PC から、自己負担の GAS と Active bond で登録が本人の class/line/root として受理される。GAS 不足、担保不足、期限切れ quote、改変 relay/owner、二重提出、fold 拒否、reorg、VPS 再作成、seeder 停止を test し、誤課金・誤帰属・偽 readiness を起こさない。 |
  195 | | A: remote claim | rail の署名/funding と node の提出を分離し、attempt の remote template と完成 block relay を実装。複数 RPC と状態監視を入れる。 | miner PC に `kaspad` がなくても claim が本人の bond で chain に入り、改変 relay・stale template・二重提出・reorg で誤帰属しない。material 配信責任は従来どおり。 |
  196 | | B: independent DA | manifest、複数 provider、Panel の root 検証を実装。まず既存責任下で試験し、客観的 provider challenge/court 後に責任を移す。 | miner PC を落としても Panel/court が期限内に material を取得・裁定できる。取得障害と fraud の結果が全 node で一致する。 |
  197 | | C: public receipt redemption | V4 spend、builder fee、bond payout、旧 V3 との fence 境界を実装。 | miner PC を `Final` 後も止めたまま、他者の block が winning quantum を一度だけ使い、報酬が miner に入る。builder の取り分以外の供給・weight・Panel 報酬は現行の上限を超えない。 |
  198 | | D: verified light client | 必要な state commitment/proof と checkpoint 運用を実装。 | 悪意ある RPC、矛盾する fork、古い anchor/bond/class/target を client が拒否する。 |
```

### `consensus/core/src/palw_court_v2.rs` L802–L820

原本SHA-256: `31ef21c8c3e6bc640366ed48f0b8240953360518020585b92f7f8c1b48a43785`

```text
  802 | /// **The responder's root claim, under the responder's key**: the claim's bond, the ADR-0082
  803 | /// responder context, over [`crate::palw_tir_dissect_v1::palw_tir_root_claim_message_v1`].
  804 | pub fn check_court_tir_root_claim_acceptance_v1<V>(
  805 |     state: &PalwChainStateV2,
  806 |     session_id: &Hash64,
  807 |     root: &crate::palw_tir_dissect_v1::PalwTirRootClaimV1,
  808 |     signature: &[u8],
  809 |     verify_mldsa87: V,
  810 | ) -> Result<(), PalwCourtV2Error>
  811 | where
  812 |     V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
  813 | {
  814 |     let (_session, claim) = resolve_court_session_v2(state, session_id)?;
  815 |     let bond = state.bond(&claim.bond).ok_or(PalwCourtV2Error::ChallengerMissing(claim.bond))?;
  816 |     let message = crate::palw_tir_dissect_v1::palw_tir_root_claim_message_v1(session_id, root);
  817 |     if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT) {
  818 |         return Err(PalwCourtV2Error::RungSignatureInvalid);
  819 |     }
  820 |     Ok(())
```

### `consensus/core/src/palw_court_v2.rs` L863–L882

原本SHA-256: `31ef21c8c3e6bc640366ed48f0b8240953360518020585b92f7f8c1b48a43785`

```text
  863 | /// **One round of children, under the responder's key**, at the IR phase's own round
  864 | /// ([`crate::palw_tir_dissect_v1::palw_tir_round_message_v1`]).
  865 | pub fn check_court_tir_round_acceptance_v1<V>(
  866 |     state: &PalwChainStateV2,
  867 |     session_id: &Hash64,
  868 |     round: &crate::palw_tir_dissect_v1::PalwTirDissectRoundV1,
  869 |     signature: &[u8],
  870 |     verify_mldsa87: V,
  871 | ) -> Result<(), PalwCourtV2Error>
  872 | where
  873 |     V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
  874 | {
  875 |     let (_session, claim) = resolve_court_session_v2(state, session_id)?;
  876 |     let phase = state.tir_dissection_v1(session_id).ok_or(PalwCourtV2Error::NoDissection(*session_id))?;
  877 |     let bond = state.bond(&claim.bond).ok_or(PalwCourtV2Error::ChallengerMissing(claim.bond))?;
  878 |     let message = crate::palw_tir_dissect_v1::palw_tir_round_message_v1(session_id, phase.round(), round);
  879 |     if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT) {
  880 |         return Err(PalwCourtV2Error::RungSignatureInvalid);
  881 |     }
  882 |     Ok(())
```

<a id="f05"></a>

## F05 — 現行4分岐の2M対話を保持したまま総量64 KiBにはできない

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L929–L938

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  929 | ### A. 対話の回数(16 分岐・1 tile 16 位置の場合)
  930 | 
  931 | 履歴 dissection の 1 往復は、Miner が子区間の部分集約値を開示し、Panel または外部検証者が不一致の子区間を選ぶ、の 2 投稿。tile 数は毎往復 `ceil(tile 数 / 分岐数)` に縮み、1 tile で終わる。
  932 | 
  933 | | context | 履歴 tile 数 | 最大往復数 | この探索部分の片道投稿数 | 縮まり方 |
  934 | |---|---:|---:|---:|---|
  935 | | 32K(32,768 位置) | 2,048 | 3 | 6 | 2,048 → 128 → 8 → 1 |
  936 | | 2M(2,097,152 位置) | 131,072 | 5 | 10 | 131,072 → 8,192 → 512 → 32 → 2 → 1 |
  937 | 
  938 | 途中で不正が確定するか、どちらかが応答しなければ、そこで終わり得る。court 全体はこれに、申立て、Miner の root 集約値の開示、最後の局所証拠の提出と裁定が加わる。held regime の時間予算は `1 + 2R + t`(1 = root claim、R = 上の往復数、t = 終端処理の予算)。t = 2 なら 32K で 9 手、2M で 13 手。**これは往復の数ではなく、証拠の分割を含む実際の tx 数・ブロック数とも一致するとは限らない。** 分岐数は ruleset から導出されるので、上の値は 16 分岐を指定した場合の計算であり、稼働中の t12 が 16 分岐であることの確認ではない。直接反証できる leaf では、証拠が揃っていれば履歴の対話自体が不要。
```

### `docs/rfc/0024-court-door-3200-to-ctx-2m.md` L191–L211

原本SHA-256: `6f59e3c4e2c44b7ad3613d6326e5c0cae80764a6708a957dd9765d95bafe9a30`

```text
  191 | ### 5.1 方針
  192 | 
  193 | 単に現在の child と「選ばれるはずの次の child」だけを先送りしても、相手は別の child を選べるため最悪往復数は減らない。
  194 | 
  195 | そこで `LookaheadDisclosureV2` は、現在の disputed range の **次の最大 2 段で到達し得る全候補**を canonical な frontier として開示する。元の分岐は 16 のまま。深さ 2 の frontier は最大 256 個で、内部の親部分値は frontier から既存の Sum/Max で復元できるため省略できる。
  196 | 
  197 | ### 5.2 2 種類の frontier を混同しない
  198 | 
  199 | - **commitment tree の局所化:** leaf/subtree の hash と opening を使う。hash は値の由来を認証する。
  200 | - **attention/history の数値的な dissection:** reduction の部分値そのものを開示し、子→親の exact fold を検査する。hash だけを 256 個出しても数値的な整合性検査の代わりにはならない。
  201 | 
  202 | B の時間削減に算入するのは、対象 phase の必要情報が全候補について実際に開示・検証された場合だけである。[S02](0024-assets/source-audit.md#s02)[S03](0024-assets/source-audit.md#s03)[S09](0024-assets/source-audit.md#s09)
  203 | 
  204 | ### 5.3 検証規則
  205 | 
  206 | `LookaheadDisclosureV2` は session、現在の round、親 range、root claim、schema に拘束する。
  207 | 
  208 | 1. 現行の `palw_attn_child_ranges_v1` の切り方を再帰適用し、depth と frontier の座標列を導く。浮動小数の log で木の形を決めない。[S13](0024-assets/source-audit.md#s13)
  209 | 2. 各要素の型・個数・proven interval、各階層の Sum/Max を検査する。中間階層を省略しても、その境界・interval チェックは省略しない。
  210 | 3. 全 frontier が親の主張に fold することを確認する。足りない child、重複、別 root、別 head/layer/position、範囲の gap/overlap は拒否する。
  211 | 4. 検証済み bundle に対し verifier が `ChoicePathV2`（最大 2 個の子番号）を 1 回提出する。各親子関係と途中の主張を accepted state から再構成する。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L70–L84

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
   70 | ### 2.1 3種類のサイズを区別する
   71 | 
   72 | | 項目 | 定義 | v2 draftの扱い |
   73 | |---|---|---|
   74 | | `S_step_fragment_max` | 1個のcanonical proof fragment。定義したheader・認証metadataを含む | **17,408 Bを暫定候補**とする。carrier追加分は別計上 |
   75 | | `S_close_max` | 最終exact courtに必要な全operand・proof・規則・metadata | **32 KiBを開発目標候補**とする。未達なら有効化不可 |
   76 | | `S_transcript_max` | 1つの独立なcourtの、開始からterminalまでの最大オンチェーン総量 | **64 KiBを`SC64`の開発目標候補**とする |
   77 | 
   78 | 32/64 KiBは今回提案する設計目標であり、導出済み・実測済みの上限ではない。正式な`S_*`は完全なcodec、全fault routeと最悪分岐から導出・審査・pinするまで`UNPINNED`とし、armingを拒否する。`SC32`等は将来候補であり、本書の容量表では感度分析にだけ使う。
   79 | 
   80 | `S_transcript`には、notice、request、先読みfrontier、部分値、source opening、原本強制開示、permit/WorkDebit、choice、close、terminal、署名、carrier framingを含む。裁定成立に必須の準備データがチェーン上に載るなら、名前を「preparation」と変えて除外しない。全非協力・キャッシュなしの強制経路を算入する。
   81 | 
   82 | 共有物は**各courtを単独で成立させるときの上限**には全量を算入する。並列court間のdedupは実績として別会計する。ネットワーク再送・orphan/reorg再提出は`S_wire_total`と別の回復予算に計上し、費用や時間から消さない。
   83 | 
   84 | 分割片が17,408 B以下でも総量が448 KiBなら、容量計算には448 KiBを使う。**fragmentへの分割は運搬を可能にするが、総証拠量の削減ではない。**
```

### `consensus/core/tests/review_heldnode_arity.rs` L1–L38

原本SHA-256: `152a55d47e6298118d1e09f0cc573eaf3f06ff17c557d10c2ff29499a9544b30`

```text
    1 | //! **Regression (feat/t12-aheld-node review, residual 1): the dissection arity a node's root claim
    2 | //! declares is the one the acceptance layer derives.**
    3 | //!
    4 | //! The processor refuses any root claim whose arity is not
    5 | //! `palw_court_params_held_at_v2(bundle, kary_at(daa), held_context_at(daa))` (its
    6 | //! `palw_court_params_at`). The panel's DENSE root claim derived it with `palw_court_params_at_v2`,
    7 | //! never held-aware: on testnet-12 (the held regime from genesis) that derivation finds no arity at
    8 | //! all, so a non-held fused class registered there answered no dissection and was convicted by the
    9 | //! silence. The node now derives it the processor's way (kaspad's panel pins the call). This test
   10 | //! pins the numbers the two derivations give on every shipped preset: testnet-12 held-aware 4 (the
   11 | //! plain derivation none — the defect), testnet-11 2 on both sides of its regime's height (7,100) by
   12 | //! either derivation (so its node is unchanged), devnet 2 by either, mainnet not V2.
   13 | 
   14 | use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
   15 | use kaspa_consensus_core::network::{NetworkId, NetworkType};
   16 | use kaspa_consensus_core::palw_court_v2::{PalwCourtV2Error, palw_court_params_at_v2, palw_court_params_held_at_v2};
   17 | use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
   18 | 
   19 | /// `(plain, held-aware)` arities at `daa`: what the dense path used to derive, and what the
   20 | /// processor (and now the node) derives.
   21 | fn arities(p: &Params, daa: u64) -> (Result<u8, PalwCourtV2Error>, Result<u8, PalwCourtV2Error>) {
   22 |     let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("a V2 network") };
   23 |     let kary = p.palw_kary_court_active_at(daa);
   24 |     let held = p.palw_held_context_active_at(daa);
   25 |     (
   26 |         palw_court_params_at_v2(bundle, kary).map(|c| c.dissection_arity()),
   27 |         palw_court_params_held_at_v2(bundle, kary, held).map(|c| c.dissection_arity()),
   28 |     )
   29 | }
   30 | 
   31 | #[test]
   32 | fn review_heldnode_the_nodes_dissection_arity_is_the_processors_on_every_preset() {
   33 |     let t12 = palw_t12_shipped_params();
   34 |     for daa in [0u64, 7_099, 7_100, 10_000, 1_000_000] {
   35 |         let (plain, held_aware) = arities(&t12, daa);
   36 |         assert_eq!(held_aware, Ok(4), "testnet-12 at {daa}: the processor's arity");
   37 |         assert!(
   38 |             matches!(plain, Err(PalwCourtV2Error::NoAdmissibleArity { window_court: 3_000 })),
```

### `docs/spec/palw/04b-tensor-ir.md` L1192–L1202

原本SHA-256: `a3269b9c7c069305bd702b0df4cad59c6ef0b290876f187fba337baae636aaa6`

```text
 1192 | #### 9.5.4 Rounds: the cut, the partials and the fold
 1193 | 
 1194 | The history is `T_h = ⌈H / h_tile⌉` **history tiles**, tile `τ` covering positions
 1195 | `[τ · h_tile, min((τ + 1) · h_tile, H))`. A dispute is over a range of tiles `(first, count)`,
 1196 | initially `(0, T_h)`, with a claim — initially the root's totals `T`.
 1197 | 
 1198 | **The cut** at arity `k` (a power of two, `2 ≤ k ≤ 64`) of a range `(first, count)` with `count ≥ 2`
 1199 | is `w = ⌈count / k⌉` and the children `(first + s, min(w, count − s))` for `s = 0, w, 2w, …` while
 1200 | `s < count` — between 2 and `k` children, in order, the last possibly shorter. A range of one tile
 1201 | has no cut: it is the **bottom**. The number of rounds to reach the bottom from `T_h` tiles is the
 1202 | number of times `count ← ⌈count / k⌉` is applied before `count ≤ 1`.
```

### `docs/spec/palw/04b-tensor-ir.md` L1327–L1362

原本SHA-256: `a3269b9c7c069305bd702b0df4cad59c6ef0b290876f187fba337baae636aaa6`

```text
 1327 | #### 9.5.9 The carriage on the chain (Phase F)
 1328 | 
 1329 | The exchange rides as ADR-0082's does, as three lifecycle objects and one close proof, appended after
 1330 | Phase F's IR objects (each refused below `palw_tir_v1` and, for the moves, where the k-ary court is
 1331 | not armed):
 1332 | 
 1333 | - `CourtTirRootClaimed { session_id, root, arity, signature }` (object tag 64): the root claim
 1334 |   `{ version = 1, elements, totals, finalize }` (`elements: Vec<Vec<u32>>`, `totals` a range claim
 1335 |   `{ partials: Vec<Vec<i128>> }`, `finalize` the carriage above); `arity` MUST be the ruleset's
 1336 |   derived dissection arity. Like every IR binding on the chain, the carriage's binding carries its
 1337 |   class with the program EMPTY: the chain puts back the registered class's program before reading
 1338 |   it, and refuses a carried one;
 1339 | - `CourtTirDissected { session_id, round, signature }` (65): `round = { version = 1, children }`,
 1340 |   one range claim per child of the cut, in order;
 1341 | - `CourtTirChildChosen { session_id, choice, signature }` (66):
 1342 |   `choice = { version = 1, session_id, round: u32, child: u8 }`;
 1343 | - the close proof `TirDissection { bottom }` (proof tag 9), graded against the dispute's phase;
 1344 |   `ComputationMismatch` is `ExecutorGuilty`, no fault `ChallengerDefeated`.
 1345 | 
 1346 | The executor signs the root claim and every round, the challenger every choice, with ML-DSA-87 under
 1347 | ADR-0082's responder and challenger contexts, over messages that open with their own domains (so no
 1348 | signature over another court's move is one over these):
 1349 | 
 1350 | - root claim: `"misaka-palw/tir/dissect/root/v1" ‖ session_id ‖ borsh(version, elements, totals)`;
 1351 | - round: `"misaka-palw/tir/dissect/round/v1" ‖ session_id ‖ le32(the dispute's round) ‖ borsh(round)`;
 1352 | - choice: `"misaka-palw/tir/dissect/choice/v1" ‖ borsh(choice)`.
 1353 | 
 1354 | The clock is ADR-0082's. At the dissected leaf the executor owes the root claim (or a close that
 1355 | acquits it); within the dispute it owes each round, and the challenger each choice, within one rung
 1356 | window of the previous move; silence past its window loses the dispute for the silent party (the
 1357 | executor's at the root claim under ADR-0082's mercies for an opening nobody could have answered,
 1358 | unchanged). At the bottom nobody is clocked: the challenger files the bottom (or the executor an
 1359 | acquitting one), and a dispute nobody closes ends at the session's backstop on the challenger's side.
 1360 | Under the held regime, where no bisection is played, a one-move accusation whose cone close names a
 1361 | dissected tile (and carries nothing else) opens the dispute at that tile, the executor's root claim
 1362 | its first move.
```

### `consensus/core/src/mldsa87_primitives.rs` L10–L21

原本SHA-256: `4193f3adb705aabe5380225fafd3c70ddb8765b8715983d87c69a01e4da10084`

```text
   10 | 
   11 | use crate::Hash64;
   12 | use crate::tx::{ScriptPublicKey, ScriptVec};
   13 | use blake2b_simd::Params as Blake2bParams;
   14 | 
   15 | /// 2592 bytes — an ML-DSA-87 verification key (FIPS 204), equal to `kaspa_txscript::MLDSA87_PK_LEN`.
   16 | pub const MLDSA87_PUBKEY_LEN: usize = 2592;
   17 | 
   18 | /// 4627 bytes — an ML-DSA-87 signature (FIPS 204), equal to `kaspa_txscript::MLDSA87_SIG_LEN`.
   19 | pub const MLDSA87_SIGNATURE_LEN: usize = 4627;
   20 | 
   21 | /// **The ML-DSA-87 P2PKH `ScriptPublicKey` paying `payload`.**
```

<a id="f06"></a>

## F06 — hashだけの局所化と、数値foldの完全性は同じものではない

### `docs/rfc/0024-court-door-3200-to-ctx-2m.md` L195–L211

原本SHA-256: `6f59e3c4e2c44b7ad3613d6326e5c0cae80764a6708a957dd9765d95bafe9a30`

```text
  195 | そこで `LookaheadDisclosureV2` は、現在の disputed range の **次の最大 2 段で到達し得る全候補**を canonical な frontier として開示する。元の分岐は 16 のまま。深さ 2 の frontier は最大 256 個で、内部の親部分値は frontier から既存の Sum/Max で復元できるため省略できる。
  196 | 
  197 | ### 5.2 2 種類の frontier を混同しない
  198 | 
  199 | - **commitment tree の局所化:** leaf/subtree の hash と opening を使う。hash は値の由来を認証する。
  200 | - **attention/history の数値的な dissection:** reduction の部分値そのものを開示し、子→親の exact fold を検査する。hash だけを 256 個出しても数値的な整合性検査の代わりにはならない。
  201 | 
  202 | B の時間削減に算入するのは、対象 phase の必要情報が全候補について実際に開示・検証された場合だけである。[S02](0024-assets/source-audit.md#s02)[S03](0024-assets/source-audit.md#s03)[S09](0024-assets/source-audit.md#s09)
  203 | 
  204 | ### 5.3 検証規則
  205 | 
  206 | `LookaheadDisclosureV2` は session、現在の round、親 range、root claim、schema に拘束する。
  207 | 
  208 | 1. 現行の `palw_attn_child_ranges_v1` の切り方を再帰適用し、depth と frontier の座標列を導く。浮動小数の log で木の形を決めない。[S13](0024-assets/source-audit.md#s13)
  209 | 2. 各要素の型・個数・proven interval、各階層の Sum/Max を検査する。中間階層を省略しても、その境界・interval チェックは省略しない。
  210 | 3. 全 frontier が親の主張に fold することを確認する。足りない child、重複、別 root、別 head/layer/position、範囲の gap/overlap は拒否する。
  211 | 4. 検証済み bundle に対し verifier が `ChoicePathV2`（最大 2 個の子番号）を 1 回提出する。各親子関係と途中の主張を accepted state から再構成する。
```

### `docs/rfc/0024-court-door-3200-to-ctx-2m.md` L556–L569

原本SHA-256: `6f59e3c4e2c44b7ad3613d6326e5c0cae80764a6708a957dd9765d95bafe9a30`

```text
  556 | ### C.2 設計案(組み合わせ)
  557 | 
  558 | 1. **commitment による局所化:** 各段で子 16 個の部分値を全部開示せず、子ごとの commitment(例:32 B × 16 = 512 B)で不一致の子を特定する。
  559 | 2. **実データは最後の区間だけ:** 不一致が特定された区間についてだけ、17,408 B の部分値と裁定に必要な operand を開示する。
  560 | 3. **2 段先読み(本文 B):** 先読みするのは大きな部分値ではなく、可能な限り小さな commitment に限る。全分岐の部分値を先送りすると総 bytes が増え、逐次待ちの削減を打ち消し得る。
  561 | 4. **認証経路の集約:** 同じ root から複数の値を開くときは Merkle multiproof で共通経路を共有する。fixture の実測では認証経路 27,156,480 B に対し、重複除去後の operand は 74,720 B(`release/t12-16000` 20fff6d90)。1,000 件同時の court での disk I/O・検証 CPU の詰まりを避ける。
  562 | 
  563 | これは ZK・SNARK を導入する話ではない(PRINCIPLES §8)。既存の interactive dissection を、hash による commitment と opening で効率化するもので、optimistic rollup の二分探索型 fraud proof と同じ考え方である。
  564 | 
  565 | **成立条件(どれか一つでも欠ければ採用しない):**
  566 | - 既存の trace commitment が、各段の子の状態を認証できること。
  567 | - 双方が提出する子の commitment と元の root との整合性を、ノードが決定論的に検証できること。整合しない側は、その時点で負ける。
  568 | - 最後に開示する実データを、元の commitment に結びつけて検証できること。
  569 | - **分岐点:** 現在の 16 個の部分値が「比較のため」だけのものか、「各段の fold の計算を客観的に検査するため」に必須かを確かめる。前者なら hash 化できる。後者なら、その検査を最後の局所裁定へ移せるか(最終区間で、子の部分値から親の値への fold を 1 回だけ検査すれば足りるか)を検討する。
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L324–L333

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  324 | commitされた不正が問題のsiteにあり、正直なchallengerがそこへ到達した場合には、
  325 | producerは「偽tileを開示してexact裁定を受ける」か「必須tileを開示せずDA/defaultになる」。
  326 | 正しいtileを回答した場合は棄却する。この二択を全siteが不正だという意味に拡張しない。
  327 | 
  328 | ### 5.3 exact semantics
  329 | 
  330 | fused attentionの分解は、元kernelの丸め・saturation・整数範囲・accumulation順序を維持する。
  331 | 非結合的な演算を分割したせいでhonest outputと違う値になるdissectionは禁止する。
  332 | input・weight・state opening、位置とshapeをclaim bindingまで検証する。
  333 | kernelが未対応、またはterminalが上限内に収まらないなら、そのplanはDisputeAdmissibleにしない。
```

### `docs/spec/palw/04b-tensor-ir.md` L1204–L1236

原本SHA-256: `a3269b9c7c069305bd702b0df4cad59c6ef0b290876f187fba337baae636aaa6`

```text
 1204 | **A round.** For each child `(f, c)` the responder states a range claim over positions
 1205 | `[f · h_tile, min((f + c) · h_tile, H))`: for each `r_i` and each `e ∈ L_i` (the root's lists, at
 1206 | every level) the partial `P_i[e] = eval_range(ctx, r_i, [e], S_i, range)` with
 1207 | `S_i = {r_1, …, r_m} \ {r_i}` supplied **from the root's totals `T`** — never from the parent's or a
 1208 | child's claim. A round is admitted iff it has one claim per child, each of the root's shape, every
 1209 | value inside its reduction's bound, and the children **fold** to the claim of the range under
 1210 | dispute, for every `(i, e)`:
 1211 | 
 1212 | - `Sum`: `Σ_child P_i[e] = claim_i[e]` exactly (unbounded integers);
 1213 | - `Max`: `max_child P_i[e] = claim_i[e]`.
 1214 | 
 1215 | A round that does not fold is the responder's self-contradiction; it is refused as a move, and the
 1216 | responder's turn runs on (its silence then loses). The challenger then names one child — by its index
 1217 | in the cut, at the dispute's round — and the named child's range and claim become the dispute's; the
 1218 | round counter advances. A dispute opened on `T_h ≤ 1` tiles is at the bottom at once, and no dispute
 1219 | takes more rounds than the cut's own recurrence from `T_h` (a choice past it is refused).
 1220 | 
 1221 | #### 9.5.5 The bottom
 1222 | 
 1223 | At the bottom — one tile `τ`, positions `[from, to)`, and the claim `claim` the dispute narrowed
 1224 | to — the court evaluates, for each `i` in order and each `e ∈ L_i` in order,
 1225 | `eval_range(ctx, r_i, [e], S_i, (from, to))` with `S_i` supplied from the ROOT's totals `T`, and
 1226 | compares the list of values (flattened, `i` major, then `e`) with `claim` flattened alike:
 1227 | 
 1228 | - the first index at which they differ convicts the executor (`ComputationMismatch { value_index }`
 1229 |   on the dissected leaf);
 1230 | - none: no fault is found and the challenger is defeated.
 1231 | 
 1232 | The bottom rides with its own carriage in the cone close's form, the tile being the dissected one:
 1233 | its checks are a cone close's, and there its convictions STAND (a carried leaf outside its proven
 1234 | interval convicts, PALW-TIR-33, before anything is evaluated). The evaluation reads exactly the
 1235 | evidence the bottom carries — the tile's history rows, the `H`-free leaves of the `H`-local region,
 1236 | and nothing past the range — and a carriage with anything more or less is refused.
```

### `consensus/core/src/palw_tir_dissect_v1.rs` L675–L707

原本SHA-256: `14a0400f3f5b7ee7ac3deaba3b2431593e07d56deca932b2fe161fae82e6b383`

```text
  675 |         for child in &msg.children {
  676 |             if child.partials.len() != self.elements.len()
  677 |                 || child.partials.iter().zip(&self.elements).any(|(v, e)| v.len() != e.len())
  678 |             {
  679 |                 return Err(PalwTirDissectError::Shape("a child claims one value per demanded element"));
  680 |             }
  681 |             check_bounds(&self.bounds, child)?;
  682 |         }
  683 |         for (i, fold) in self.folds.iter().enumerate() {
  684 |             for e in 0..self.elements[i].len() {
  685 |                 let mut values = msg.children.iter().map(|c| c.partials[i][e]);
  686 |                 let folded = match fold {
  687 |                     PalwTirFoldV1::Sum => values.try_fold(0i128, |a, v| a.checked_add(v)),
  688 |                     PalwTirFoldV1::Max => values.max(),
  689 |                 };
  690 |                 let claimed = self.claim.partials[i][e];
  691 |                 match folded {
  692 |                     Some(f) if f == claimed => {}
  693 |                     other => {
  694 |                         return Err(PalwTirDissectError::DoesNotFold {
  695 |                             reduction: i,
  696 |                             element: e,
  697 |                             claimed,
  698 |                             folded: other.unwrap_or(i128::MAX),
  699 |                         });
  700 |                     }
  701 |                 }
  702 |             }
  703 |         }
  704 |         self.pending = msg.children.clone();
  705 |         self.last_deadline_daa = accepted_daa.saturating_add(w_round);
  706 |         self.turn = PalwBisectTurnV1::AwaitVerdict;
  707 |         Ok(())
```

<a id="f07"></a>

## F07 — claim単体の延長が、最短の署名済み依存horizonを越え得る

### `docs/rfc/0008-palw-claim-backed-consensus-blocks.md` L305–L316

原本SHA-256: `5e6df30068ead9de1eb85c4d3bb14d10f4df49314a105f443c4d68f188591b31`

```text
  305 | ### R8-WINDOW — 依存claimの責任・material・post-Final
  306 | 
  307 | 1. **依存担保の床:** bindingの時点で `H_exec = expiry + max(native_window_court, kernel_liability) + kernel_court_deadline + kernel_proof_grace` をchecked arithmeticで依存claim行に固定する（MUST）。計算Final、pre-Final default、root fee精算、cancelでこのhorizonを短縮してはならない（MUST NOT）。支払済み・failed・予約ゼロ・期限切れのworkの後付けを拒否する。restoreは全依存のbase floorを再導出する。
  308 | 2. **Settled後の有罪:** Settled後の依存claimのconviction/defaultは、rootのnative Final、safe weight、native evidence、未解放vesting、payout全体へ一貫して伝播する（MUST）。kernelは自身のcollateralを処罰し、伝播だけを理由に他actorを追加slashしない。`void_claim`だけで巻戻しを済ませない。
  309 | 3. **native紛争期限:** native DA/courtの延長は、依存pathの最短の署名済みhorizon内に収まらなければならない。超える延長は拒否する（§15.4）。既存termを越える延長は、新しい署名・新しい資金による明示の同意だけで行う（未実装）。
  310 | 4. **X3（2026-10-12ユーザー判断）:** claim単位の絶対期限（固定点d0＝claim受理時）を延長しない規則は新しいversioned fenceでのみ適用する。live t12の旧規則は変えない。
  311 | 5. **workerの同意:** `nonce_v2 = H(slice-job-nonce/v2; nonce_v1 || expiry || H_exec || executor_exposure_sompi || policy_root)` を署名済みkernel claimで拘束し、最初の署名済みsliceでだけ予約する（§15.6）。allow-listは同意ではない。
  312 | 6. **Panel署名lifetime（inner 66/65、aux 121、`palw_panel_lifetime_v2`）:** producerはadmission前に絶対受理区間・service/liability endpoint・retry/pause予算・最大PWU/金額へ署名し、native birthで実statementを捕捉する。retryは同じendpointを共有し、pauseはcounterをresetしない。expiryは時間内のcourtを中和せず、獲得済み現金を消さない（MUST NOT）。名前付きパラメータ`PalwPanelLifetimePolicyV2 {maximum_service_daa, maximum_pause_daa, maximum_retries, maximum_enrollments, maximum_enrollments_per_producer}`（不変条件: `0 < maximum_service_daa < u64::MAX`、`maximum_pause_daa <= maximum_service_daa`、`0 < per_producer <= maximum_enrollments`、security/economic review scopeが非零）: **value: USER DECISION REQUIRED**。休眠codeのprobe値は政策値ではない。
  313 | 7. **monetary material covenant（outer 240–242、aux 120、`palw_monetary_material_v1`）:** 全provider・sponsorが完全なroot/依存、capacity manifest、担保、自己資金fee、物理的支払window `W_mat`、payment-relative義務に署名する。liabilityまたはqueueが残る間、holdはMAXとする。materialの解放は**最後の実支払 + `W_mat`**以後であり、release blockが現在のselected chainでfinalizedであることを物理GCの条件とする（MUST）。sparse DAAやqueue滞留でproviderの新しい義務は期限切れにならない。欠落delta、古いtip、未完了import、generation変化、契約の不在はGCを許可しない。名前付きパラメータ`PalwMonetaryMaterialPolicyV1 {max_contracts, max_contracts_per_provider, max_bytes_per_contract, max_bytes_per_provider, minimum_collateral_sompi, minimum_fee_sompi, window_daa (= W_mat), response_window_daa, max_enrollment_daa, cost_report_root}`（不変条件: 全て非零、`max_contracts_per_provider <= max_contracts`、`max_bytes_per_provider >= max_bytes_per_contract`、`cost_report_root`は承認済みreview由来）: **value: USER DECISION REQUIRED**。固定前払feeを未知の支払・回復時間の安全な対価と表示しない（`PaymentAndRecoveryTimeUnknown`）。
  314 | 8. paid material: native paid契約の`material_until`は署名済みEXEC retention horizon以上でなければ、fee/R移転・root/work-use書込みの前に拒否する（§15.11）。
  315 | 9. Remaining（観察、未検証）: 実nodeのnative root Final後の公開告発と支払回復、承諾付きhorizon延長、pruned join、workerの最低対価・資本費用。
  316 | 10. **承諾付きhorizon延長（2026-10-15、休眠実装）:** `palw_horizon_extension_v1`（kernel inner 68、aux 126）。EXECに束縛されたkernel claimの生きている`liability_until`を、worker（claimのproducer）とsponsor（jobのposter）が同一termsへ各自署名して延長する。全員の署名が揃った時点で原子的に適用し、(a) 延長だけ（Final・court窓を短縮しない）、(b) claim固定のhard deadline（claim受理`d0`からの`maximum_claim_horizon_daa`、EXEC rootのnative claimにX3が適用される場合はその`dispute_hard_deadline`との`min`）を越えない、(c) 期限切れhorizonを再開しない、(d) 獲得済み現金を取消さない、(e) EXEC income holdを同じhorizonまで延長する、(f) transferred claimのproviderは自身の署名済みlease（tag 150）が新horizonを既に覆うことで同意する（leaseを書換えない）。名前付きパラメータ`PalwHorizonExtensionPolicyV1 {maximum_extension_daa, maximum_consent_window_daa, maximum_claim_horizon_daa, maximum_open_extensions, require_price_contract, review scopes}`: **value: USER DECISION REQUIRED**。
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L940–L954

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  940 | ### B. 通常の期限と、係争の最終期限を分ける
  941 | 
  942 | | 項目 | 扱い |
  943 | |---|---|
  944 | | 通常の異議受付期間 | original claim の受付から 3,120 DAA |
  945 | | 異議がない claim | 他の保持・責任条件も満たしたら、通常どおり担保を解放する |
  946 | | 有効な異議がある claim | 関連する担保・報酬・Final を保留し、対話型 court を続ける |
  947 | | 係争の最終期限 | **original claim の受付から** 10,000 DAA |
  948 | | 各手番の応答期限 | 10,000 とは別に定め、応答義務者が守る |
  949 | | 終了後 | 裁定結果と、残る保持・復旧義務に従って担保を解放または徴収する |
  950 | 
  951 | - 起点は original claim の受付時とする。3,120 の直前に申し立てられても、残りは `10,000 − 3,120 = 6,880 DAA`。受入条件は、**遅い申立てでも、残りの対話・証拠配送・最終収録・回復の余裕が 6,880 DAA に収まること**。
  952 | - 「申立てから 10,000」を与える案は、original からの拘束が最大約 13,120 DAA になる別の仕様で、採らない。
  953 | - **bond の返還時点だけを延ばしても足りない。** 証拠の受付が 3,120 で閉じれば、後から証明できない。延長の対象には、court の最終期限に加え、該当経路の `collect_until`、開示義務、資料保持、Final の保留、bond の解放条件を含め、1 つの原子的な状態更新で動かす。現行の公開 allocation 経路は `material_until = collect_until + reorg_retention`、`bond_exit_after = material_until` と連動しており、clock を実行時に延長しない前提で書かれている。したがってこれは定数の変更ではなく、**有効な申立てを契機に関連期限を更新する、明示的な新規則**である。
  954 | - 同じ bond が別の claim の責任も負っている場合、このcourt の終了だけで全額は返らない。解放は、claim ごとの責任と予約を基準にする。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L274–L319

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  274 | ### 6.1 有界なextension contract
  275 | 
  276 | v2のoriginal acceptanceに、`base_collect_until`、`extension_policy_id`、`E_max`、最悪保持期限、追加費用の予約を含める。custodianと責任主体は、この最大範囲を含むtermsに署名する。
  277 | 
  278 | ```text
  279 | CourtExtensionV2 {
  280 |   frame_id, prior_effective_terms_hash,
  281 |   scheduler_profile_id, reservation_seq, reservation_root,
  282 |   old_collect_until, new_collect_until,
  283 |   new_material_until, new_bond_exit_after,
  284 |   new_final_floor,
  285 |   extra_resource_charge, escrow_debit,
  286 |   reason_code
  287 | }
  288 | ```
  289 | 
  290 | 新しい名称は提案であり、既存wire tagは割り当てない。extensionはノードが予約状態と承認済み規則から検証する。operator・custodianの申告だけで期限を延ばさない。
  291 | 
  292 | 元の署名対象frameを破壊的に書き換えず、versionedなeffective termsとextension chainを導出する。旧hashに依存するpermit、lease、allocation、source binding、料金照合をすべて更新対象にする。
  293 | 
  294 | ### 6.2 必ず同時更新するもの
  295 | 
  296 | 1つの原子的状態遷移で、影響するcourt/session/原本義務の期限、`collect_until`、Final floor、credit/receipt/escrow保留、`material_until`、custody pledgeの実効責任、bond exit hold、保持料・予約量を更新する。
  297 | 
  298 | **bond返還だけを延期し、古い`collect_until`で正しいconvictionを拒否する実装は禁止する。** 逆に、courtだけ延ばし担保が先に抜けることも禁止する。
  299 | 
  300 | bondの関係は、例えば次のような上界関係を満たすように導出する。
  301 | 
  302 | ```text
  303 | material_until_i ≥ max(collect_until_i,
  304 |                        terminal_settlement_safe_i,
  305 |                        latest_reference_or_recovery_need_i)
  306 | 
  307 | bond_exit_after_i ≥ max(material_until_i,
  308 |                         settlement_and_liability_end_i)
  309 | ```
  310 | 
  311 | 単発のdoorが3,200でも、reorg・settlement・責任保持の余裕によりbondの返還可能時点は3,200より後になり得る。実際の返還transactionの収録はさらに別である。
  312 | 
  313 | ### 6.3 後続攻撃による無期限延長を防ぐ
  314 | 
  315 | 通常の混雑延長は、original受付時と所定のnotice予約時に合意stateから決める。いったん成立した処理予約を、後続の別claim、大量の重複notice、無効応答、partial fragment、再要求で繰り返し延期しない。
  316 | 
  317 | 同一争点の重複は既存の予約を参照し、時計をリセットしない。新しい独立な争点は別予算を消費するが、他の既存予約を奪わない。`notice_until`自体を新しいnoticeのたびに後ろへ延ばさない。
  318 | 
  319 | reorg等による変更は事前合意済み回復予算に限る。予算を超えるservice障害は通常延長の成功として扱わず、事故記録・新規受付停止・既存責任の安全な保護に移る。`E_max`に達したからという理由で、未解決不正を無罪・Finalにすることは禁止する。
```

### `docs/rfc/0015-panel-free-permissionless-verification.md` L444–L466

原本SHA-256: `ed63475109cd6edd721ea85ac96dcd9687d9108ea9253b8dc2201941ed1a1112`

```text
  444 | #### 8.3.3 共通DAA拘束と責任担保の分離
  445 | 
  446 | 受理DAA `d`で消費した経済的発行枠の通常の最早回復時刻を、
  447 | class/仕事量によらない共通期間`W`を用いて`reuse_not_before = d + W`に固定する（MUST）。
  448 | 秒・日への換算を合意時計と同一視しない。
  449 | 
  450 | 早期の検証成功、任意audit/Valid報告、早いFinal、claim取消、void/conviction、
  451 | 再送、別session、同じ仕事の分割によって、`d + W`より早く発行枠を返してはならない（MUST NOT）。
  452 | 失効した報酬の予約を新claimのReward Capとして即時再利用することも禁止する。
  453 | 量産した不正claimは同じ経済的発行枠を消費し、計算省略によって枠の回復を早めない。
  454 | 
  455 | **発行枠の回復と、slash可能な担保・未払報酬の解放は別の条件である。**
  456 | challenge、court、DA、retention/liabilityの責任が残る限り、必要な担保と未払報酬をMUST保持する。
  457 | `d + W`到達だけで既存のslash責任を消滅させず、同じ担保を新旧claimの損失吸収へ重複使用しない。
  458 | 発行枠が回復しても、実際の新claim受理には残存責任控除後のheadroomを要求する。
  459 | slashされた資本を元の担保額`C`のまま再利用可能として数えてはならない。
  460 | 
  461 | §6のChallengeable開始時刻・base window・hard deadlineは公開反証の時計であり、
  462 | `d + W`の発行枠回復時計と別に検査する。`W`経過だけではFinal・報酬成熟を認めない。
  463 | claim固有の適格裁定/証拠義務が未解決なら、Final/担保解放は当該規則で処理する。
  464 | model取得不能だけではFinal/defaultを起動せず、実効検出不足を経済評価の未解決リスクとして扱う。
  465 | 本RFCの「事後反証」はclaim受理後の公開反証を指し、不可逆な経済Finalと
  466 | 責任担保の解放後に無条件で取り消せることを意味しない。
```

### `docs/rfc/0018-dag-parallel-multi-model-consensus.md` L208–L218

原本SHA-256: `3672c8774d690749527351c5276e622f19a623798a16e29386c2265392b28390`

```text
  208 | ### 7.2 認証述語(前提に基づく認証)
  209 | 
  210 | 1. **commit:** 支配されたclaimの`Final`で、claim・producer・母集団の切断点を固定する(`commit_at_final_v1`、`consensus/core/src/palw_security_certification_fold_v1.rs:45`)。
  211 | 2. **draw:** commitより厳密に後のexecution laneのseed anchor(ADR-0130)から作るseedで、**global pool**(Active、commitの`verifier_maturity`前に登録、最小担保以上、producerのoperatorを除く)から`m`位置を担保比例・復元抽選する(`palw_certification_pool_admits_v1`、`palw_security_certification_v1.rs:280`、RFC-0017/R7-ECONの抽選を再利用)。**モデル保有の有無で母集団を絞らない**。
  212 | 3. **attest:** 抽選されたbondは期限内に、そのclaimを再実行して判定を署名する(RFC-0017のinner 75を、exam idの代わりにcertification idで使う、`try_attest_claim_certification_v1`、fold `:111`)。
  213 | 4. **decide:** `k`位置以上が`Reproduced`(mismatchなし)で、claimがまだ`Final`、未解決の告発がなく、**G14公開告発window**(`prosecution_window_daa`)が`Final`から経過したら`Certified`(`palw_certification_verdict_v1`、`:320`)。期限までに`k`未満なら`NotCertified`、母集団不足は`NoPopulation`、`certify_by`までに抽選できなければ`Expired`、`Final`でなくなれば`Withdrawn`。**1 claimにつき1回、再試行なし、classごとに数える**(attemptの有界化と計数)。network全体のepoch当たり抽選数にも上限がある。
  214 | 5. **公開check経路:** 決定論的な公開検査(G14完了classの公開material)が固定されたclassは、告発window経過で`PublicCheckCertified`。このbinaryでは固定classは0件(`PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1 = &[]`)。
  215 | 
  216 | **前提(明示):** 認証は抽選検証者の署名付き承認に依る。**「抽選されたverifier stakeのうち正直な割合が閾値(`k/m`)以上である」ことを前提とする。** 署名は再計算の証明ではない。これは無条件の数学的証明ではなく、前提に基づく認証である。RPCはこれを`securityCertificationPremise`として返す(§7.8)。
  217 | 
  218 | 閉鎖モデルは、正直な抽選検証者が入手も再実行もできないので、正直な位置から肯定承認を得られない。攻撃者が`m`位置中`k`以上を引いたときだけ認証される。
```

<a id="f08"></a>

## F08 — hard deadline後の未解決保留と、共有frontierの有限遅延の境界が曖昧

### `docs/rfc/0023-unresolved-claim-liability-and-independent-progress.md` L15–L33

原本SHA-256: `b7733389cb0462db4d3ce99def2c95d819c78178d61be0d3daaf6daf02449ab3`

```text
   15 | ## 2. 現行規則(変更前の記録)
   16 | 
   17 | - `palw_state_v2.rs:15043–15065`:`resolved_through` = 最も古い未解決 claim の受理位置 − 1。その範囲内で、Gate C 有効後は Final かつ SecurityCertified の Attempt claim の最深位置を frontier にする(F01)。位置は `accepted_blue_score`。
   18 | - 「解決済み」と「認証済み」は別である。Final だが未認証は足止めにならないが frontier の根拠にもならない。Voided は足止めから外れ、credit は得ない(F01b・F04)。
   19 | - frontier には退役による下限(floor)があり、退役前の認証済み claim が後で有罪になると frontier は後退し得る(F03・F05)。
   20 | - **影響範囲:** 利用者の取引の `settled` は `safe_frontier_daa >= daa_score` で判定する(`palw_settlement_v1.rs:118–125`、F07)。native の安全な確定は frontier と `lifecycle_closed` の両方を要する(F11・F13)。fork choice は frontier を比較要素に使う(F09・F10)。pruning は Gate C 下で既に未解決・認証待ち・後退し得る位置から上限を導く(F04・F08)。
   21 | - したがって、A が未解決の間、B の claim は Final・認証・報酬出力まで進み得るが、**B への支払い取引の `settled` は共有 frontier を待つ。**
   22 | 
   23 | 現行規則は、旧規則の 2 つの失敗(未解決 0 件を要求して正常な chain で進まない/空ブロック列が frontier を進める)を避けるためのものである。足止め条件を削除して最深位置だけで進む方式には戻さない。
   24 | 
   25 | ## 3. 段階 1 — 現行 frontier を保ち、足止め時間に上限を置く(次回 fence)
   26 | 
   27 | 1. **claim ごとの未解決期限。** `未解決でいられる期限 ≤ 受理 DAA + L_class`。`L_class` は検証・資料取得・court・必要な収録待ちを含む。受理時に court 予約容量・期限内の処理順・再抽選と追加告発による延長の上限と整合しなければ、その claim を報酬対象として受理しない(既存の bounded admission・detection policy・retention の規則を使う)。新しい告発で責任時計を最初からやり直さない。
   28 | 2. **期限到達の扱い。** 期限到達は不正の証明ではない。fraud slash は証明された不正と明確な応答義務違反(適格 default)にだけ行う。期限で credit を与えない場合も、係争中の担保・証拠は解放しない。
   29 | 3. **測定。** 次を別々に記録する:B の Final、Gate C 認証、報酬保留の解除、支払い出力の作成、その取引の `settled = true`、native の safe/finalized。「出力ができた」と「確定した」をまとめない。
   30 | 4. **継続攻撃。** 1 件ごとの期限が有限でも、別の古い claim が次々と足止めを引き継ぐ場合の定常遅れを測る(資料応答の遅延、正当な court、虚偽告発、再抽選、容量飽和、遅延 claim の継続投入を原因別に)。
   31 | 5. **期限の異なる claim。** A と B の責任期間が異なる場合の退役 floor(退役時の floor 更新は `oldest_unresolved` を再確認しない)、late conviction による後退、その後の pruning・IBD 復元を 1 組の試験にする。
   32 | 
   33 | 段階 1 が保証するのは「未解決 claim 由来の遅れの上限」だけで、認証済みの仕事が生まれない期間やネットワークの停止まで frontier の前進を保証しない。
```

### `docs/rfc/0023-unresolved-claim-liability-and-independent-progress.md` L35–L65

原本SHA-256: `b7733389cb0462db4d3ce99def2c95d819c78178d61be0d3daaf6daf02449ab3`

```text
   35 | ## 4. 段階 2 — 未解決 claim を局所管理する(別の休眠 fence)
   36 | 
   37 | 共通 DAG は維持し、次の 3 つを分けて管理する。
   38 | 
   39 | | 管理するもの | 役割 |
   40 | |---|---|
   41 | | DAG への記録・共通の順序 | どの claim・取引・証拠が取り込まれたかを全ノードで一致させる |
   42 | | claim ごとの認証・決済状態 | その claim に credit・報酬を与えてよいかを決める |
   43 | | 未解決 claim の責任・証拠状態 | 担保、期限、係争、再計算に必要な資料を保持する |
   44 | 
   45 | 1. **未解決集合を明示的に拘束した checkpoint。** A を解決済みと偽装せず、checkpoint に未解決集合(claim、担保予約、検査 policy、乱数・登録状態の証拠、court 状態)を拘束する。Merkle root だけでは足りない。正直な検証者が pruning・IBD の後も実際に資料を取得して反証を完成できること(G14)を要件にする。
   46 | 2. **依存の単位は claim とその依存関係。** B が A の出力を入力に使う、同じ execution root に依存する、同じ担保予約を使う場合は依存とみなし、B も待つ。モデル ID や同じ DAG にいることだけでは依存としない。
   47 | 3. **不可逆な確定の根拠。** 取り消され得る credit(責任期間内の認証済み claim)と、不可逆な checkpoint を支える credit を区別する。未解決 A を数えず、必要な安全条件を満たした独立の仕事だけで checkpoint を支える。
   48 | 4. **後から A の不正が判明したとき。** A と、A に実際に依存した対象にだけ処罰・credit 取消を適用する。無関係な B は止めない。
   49 | 5. **同時に変えるもの。** checkpoint、fork choice(frontier の比較要素)、取引の settlement、native の lifecycle、pruning・IBD・証拠保持を 1 つの fence で一緒に変える。`resolved_through` の条件を消す 1 行の修正にはしない。
   50 | 
   51 | ## 5. 採らない案
   52 | 
   53 | | 案 | 理由 |
   54 | |---|---|
   55 | | モデル別 frontier の最小値 | 最も遅いモデルを待つことに変わりない。休止モデルの扱いも要る |
   56 | | モデル別 frontier の最大値 | 速い 1 モデルだけで、ほかの未解決責任まで安全とみなす |
   57 | | 多数のモデルが進んだら進める | モデル数は独立した安全性ではない。モデル ID を増やせれば操作できる |
   58 | | 一定時間後に未解決 claim を無視する | 担保・告発・証拠保持を失い、不正を逃がす経路になる |
   59 | 
   60 | ## 6. 受入条件
   61 | 
   62 | - A が未解決のまま、A に依存しない B・C が Final・認証・報酬解除・支払いまで進む。段階 2 では B の取引の `settled` も A を待たない。段階 1 では待ち時間が `L_class` の上限内であること。
   63 | - A が後で有罪になったとき、A と A に依存した claim だけが slash・credit 取消を受け、B は影響を受けない。
   64 | - 未解決 A の資料は pruning・IBD・restart・reorg の後も取得でき、外部の正直な検証者 1 人が反証を完成できる。
   65 | - 全ノードが同じ checkpoint・未解決集合・決済状態に収束する。fence の前は byte 単位で同じ動作。
```

### `docs/rfc/0006-palw-layer-sharded-panels.md` L1268–L1279

原本SHA-256: `06ae27140abd9812a80678750240b9e84785ecbd3871fc48fd64483af029f50c`

```text
 1268 | ### R6-OUTSIDE — non-seat verification and pre-emption
 1269 | 
 1270 | 1. A non-seat watcher holds no seat, signs no receipt and needs no outsider selection. It may file
 1271 |    `TirShardCourtAccused` or a `TirStepRun` DA demand within the non-seat budget, using only public authenticated
 1272 |    material; it never asks the producer with a seat's signature.
 1273 | 2. **Pre-emption.** While a DA session, court session or `DefaultDisputed` state is open on a claim, no non-fraud end
 1274 |    (neutral void, receipt/bind timeout) and no Final may take effect; the engine's own decision is deferred, not
 1275 |    re-rolled. An accusation carried in the same block as an already decided end is late, not pre-emptive.
 1276 | 3. **Outsider `Incapable` (reconciles §4.2 with ADR-0177 D1 supplement).** When an outsider cannot obtain or hold its
 1277 |    shard, the part cannot license and the claim ends as a **non-fraud termination** for missing required verification.
 1278 |    This MUST NOT slash anyone, penalize the registrant, suspend the class's registration or stop rewards of other valid
 1279 |    claims, and it is not a judgement of fraud.
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L182–L189

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  182 | ### 4.3 通常・混雑・過負荷
  183 | 
  184 | - **Normal:** 予約済みの全phase・冷状態・保証対象負荷が`a_i+3,200`に収まる。`E_i=0`。
  185 | - **Congested:** 審査済みの有限burstにより将来の処理枠を予約する。`0<E_i≤E_max`。その案件の期限と責任を連動延長する。
  186 | - **Overload admission:** 新しい経済的claimを受けると`E_max`、保持予算、active state上限、control容量を超えるなら、その新規受付を抑制する。既存の有効courtを捨てない。
  187 | - **Service failure:** 合意されたservice envelope自体が崩れた場合は別扱い。通常の混雑達成として報告せず、安全なFinal保留と復旧手続きへ移る。無制限障害時の有限返還を約束しない。
  188 | 
  189 | `E_max`は一律10,000という暫定値で埋めず、処理容量、burst契約、保持・担保費用、復旧条件から決める。10,000件のburstを対象にするなら、その最終案件まで収容できる値かを確認する。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L313–L319

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  313 | ### 6.3 後続攻撃による無期限延長を防ぐ
  314 | 
  315 | 通常の混雑延長は、original受付時と所定のnotice予約時に合意stateから決める。いったん成立した処理予約を、後続の別claim、大量の重複notice、無効応答、partial fragment、再要求で繰り返し延期しない。
  316 | 
  317 | 同一争点の重複は既存の予約を参照し、時計をリセットしない。新しい独立な争点は別予算を消費するが、他の既存予約を奪わない。`notice_until`自体を新しいnoticeのたびに後ろへ延ばさない。
  318 | 
  319 | reorg等による変更は事前合意済み回復予算に限る。予算を超えるservice障害は通常延長の成功として扱わず、事故記録・新規受付停止・既存責任の安全な保護に移る。`E_max`に達したからという理由で、未解決不正を無罪・Finalにすることは禁止する。
```

### `docs/rfc/0018-dag-parallel-multi-model-consensus.md` L203–L206

原本SHA-256: `3672c8774d690749527351c5276e622f19a623798a16e29386c2265392b28390`

```text
  203 | * **I-C2:** `Final`、Panelのlicence、Panelの過半、モデル保有者の集合、登録者が満たせる集合のいずれも、単独で認証にならない(MUST NOT)。
  204 | * **I-C3:** 認証済みclaimが後でG14の公開告発で反証されたら、voidされ、合意creditは取り消される(MUST)。safe weightは`reverse_convicted_final`で引かれ、participation・settlement depthから消える。
  205 | * **I-C4:** DNS-BFTおよびDNS finality anchorに安全性・収束を依存しない(MUST NOT)。
  206 | * **I-C5:** 何も認証されない間も、DAG、通常取引、heartbeat/BASE-0の時計、DAAは進む。止まるのはfinality creditだけ(MUST)。
```

<a id="f09"></a>

## F09 — 受付・手番・collect・hard deadlineの等号と同一block順序を統一する必要

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L908–L923

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  908 | ## 改定 — 2026-10-12（固定期限と有限終了、共通経済会計：提案）
  909 | 
  910 | **状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §5・§3A。
  911 | 
  912 | - **§7.2補足（X3）:** 経済的なclaim受理時点 `d0` で、rulesetに基づく最大の準備期間・観測期間・裁定期間と絶対deadlineを固定する。最初のcourt session受理時まで先延ばししない。
  913 | 
  914 | ```text
  915 | receipt_deadline = d0 + T_prepare
  916 | open_cutoff      = receipt_deadline + T_observe
  917 | hard_deadline    = open_cutoff + T_resolve   (= dispute_hard_deadline)
  918 | effective_deadline = min(proposed_challenge_deadline, hard_deadline)
  919 | ```
  920 | 
  921 |   すべて合意上のDAA単位とする。receiptが早くてもopen_cutoffを早めず、遅いreceiptで時計を延ばさない。新しいinteractive pursuitは `start <= open_cutoff` かつ `start + B_remaining <= hard_deadline` の場合だけ受理する（残り時間が検証・証拠開示・court完了に足りなければ開始させない）。通常Finalはopen_cutoff通過後、未解決の適格pursuitがない場合にだけ可能とし、期限到達だけで未解決claimを自動Finalにしない。direct objective proofは別sessionが開いているだけでは拒否しない。境界は受理 `d <= deadline`、期限切れsweep `d > deadline` のように一意にし、同一blockでの処理順をcanonical orderingで固定する。旧claimには旧rulesetを適用し、新しい期限規則は別のversioned fenceで有効化する。
  922 | - **§16（共通経済部分）:** モデル別claim bond台帳、配分snapshot、係数、正常Finalと非不正終了・没収時の取扱い、閉鎖/公開比較を置く（ADR-0177 2026-10-12改定、受入試験C01〜C18）。
  923 | - **G14との関係:** G14-B（`misaka/palw/kernel/src/budget.rs`）の最悪追及スケジュールは、この `hard_deadline` に収まることを要件にする。長いG14 staged経路にdrill暫定の予算window W = 400 DAAをそのまま適用しない（2026-10-11裁定）。
```

### `docs/rfc/0024-court-door-3200-to-ctx-2m.md` L382–L388

原本SHA-256: `6f59e3c4e2c44b7ad3613d6326e5c0cae80764a6708a957dd9765d95bafe9a30`

```text
  382 | original acceptance 時に `collect_until_daa`、必要な terminal の最終収録余地、default 処理、保持と bond exit を整合させる。新しい challenge ごとに時計を 0 からやり直さない。[S06](0024-assets/source-audit.md#s06)[S12](0024-assets/source-audit.md#s12)
  383 | 
  384 | 終盤の段を単純に `min(now + round_window, collect_until)` で切って、正直な responder の必要時間を失わせる実装は禁止する。各段は残りの必要時間を予約したうえで開始し、そもそも合法的な最悪進行が収まらない profile は admission / activation を拒否する。
  385 | 
  386 | ZIP の public conviction は `accepted_daa ≤ now < collect_until_daa` で受理する。したがって、この経路で `collect_until_daa = accepted_daa + 3,200` とする場合、terminal がちょうど `+3,200` で収録されるのは遅い。最終許容収録位置は `+3,199` であり、上限計算にはこの排他的境界の余裕を含める。DA/default の境界と同一ブロック内の処理順も、最新 fold の規則と照合する。[S12](0024-assets/source-audit.md#s12)
  387 | 
  388 | 「入らなかったら timeout で落とせば 3,200」は達成ではない。正直な protocol participant が所定資源で期限を守れることを示す。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L175–L180

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  175 | 
  176 | `notice_until`と`collect_until`を分離する。有効noticeの受付窓の後にも、反証作成・応答・最終収録に必要な時間を予約する。`notice_until`は最小検査時間と所定収録機会を奪わない形でprofileから導出する。
  177 | 
  178 | 有効noticeの到着がその許容窓の最終位置だった場合も、`F_i`を守れることを要求する。準備や異議検出に必要な仕事を開始前へ移して計上から消さない。modelを事前取得済みとするprofileはその前提を明記し、完全cold profileとは区別する。
  179 | 
  180 | `collect_until`は排他的境界なので、terminalは`now < d_i`で収録する。境界ぴったりに収録してよいとするoff-by-oneを禁止する。[V2-S02](0025-assets/source-audit-v2.md#v2-s02)
```

### `consensus/core/src/palw_tir_public_fold_v1.rs` L913–L933

原本SHA-256: `9092c26113c4636ee8ea7bbcc3550f1648c0134e7cbeccd59b723d42c20531ef`

```text
  913 | ///    filing instead of recording a fictitious collection.
  914 | fn apply_conviction_v1(
  915 |     b: &mut TransitionBuilder<'_>,
  916 |     ctx: &PalwBlockContextV2,
  917 |     frame: &Frame,
  918 |     filer: PalwBondKeyV2,
  919 |     evidence: &crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
  920 | ) -> Result<(), PalwStateV2Error> {
  921 |     use crate::palw_tir_objective_v1::{
  922 |         PalwTirObjectiveOutcomeV1 as Arith, PalwTirOutputFaultV1 as OutFault, PalwTirOutputOutcomeV1 as Out,
  923 |     };
  924 |     use crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1 as E;
  925 |     let claim_id = frame.plan_v1().original_claim;
  926 |     let terms = frame.plan_v1().terms;
  927 |     if ctx.daa_score < terms.accepted_daa || ctx.daa_score >= terms.collect_until_daa {
  928 |         return Err(refused("TIR public conviction is outside the frame's original collection door"));
  929 |     }
  930 |     conviction_filer_v1(b, frame, filer)?;
  931 |     let claim = b.state.claim(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
  932 |     let producer = frame.custody_v1().pledge_v1().producer;
  933 |     if claim.bond != producer || claim.execution_root != frame.original_v1().execution_root {
```

### `docs/rfc/0019-annual-model-upgrade.md` L170–L177

原本SHA-256: `ea6c0bbcb654f575bb8396e909f2b31ed78004a3b4897fba1d7088b8e0c66d0e`

```text
  170 | ### 6.4 受理 DAA と境界の規則(権利を二重に発行しない)
  171 | 
  172 | 1. **claim の受理 DAA** は、その claim を mergeset で適用した chain block の DAA である(`PalwBlockContextV2::daa_score` → `PalwClaimStateV2::accepted_daa`、`transition_builder.rs:4386`)。BLUE と RED の merged block で同じであり、merged block 自身の header DAA ではない(`palw_v2_merged_works` は accepting block の `point` で admission する、`processor/palw.rs:9723`)。
  173 | 2. **階層は allocation epoch ごとに一度だけ読まれる。** roll はその epoch の最初の chain block(DAA `now`)で起こり、`palw_model_tier_of_model_v1(.., now)` で各モデルの階層を読む。同じ epoch で roll は二度起こらない(`rolled` の判定、`palw_bond_budget_fold_v1.rs:2084`)。
  174 | 3. したがって、ある claim の権利と報酬が反映する階層は「受理 DAA を含む allocation epoch の roll DAA における階層」である(`palw_catalog_tier_reference_daa_v1`、`palw_annual_catalog_v1.rs:317`)。edition が epoch の途中の H で有効になっても、新しい階層が効くのは roll DAA ≥ H の最初の epoch からである。
  175 | 4. **RED の跨ぎ:** header DAA < H で掘られ、DAA ≥ H の chain block に merge された RED block は、その accepting block の epoch で一度だけ受理され、その epoch の一つの snapshot から権利を消費する。V+ の権利は epoch ごとに一度だけ発行され、受理時に一度だけ消費される。カタログの有効化は再発行を起こさない。
  176 | 5. **過去を再評価しない:** H より前に受理された claim は、受理時に予約された権利・escrow・報酬のまま完結する。Final・court・payout は階層を読まない。
  177 | 6. 推奨(SHOULD): edition の `activation_daa` を allocation epoch の境界に合わせる。合わせなくても規則は決定論的だが、RPC の「ACTIVE」と配分に効く時点が一致して読みやすい(§13)。
```

<a id="f10"></a>

## F10 — 土台とされるcourt容量モジュールが添付snapshotにない

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L988–L994

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  988 | | court 応答用の予約帯域 | 通常 tx の混雑で Miner の応答枠が消えないようにする。割合(例:通常 65% / court 応答 25% / 申立て 10%)は説明用の仮定で、容量試験で決める |
  989 | | 少額の応答 fee | 有効な court の正規応答は低コストにする |
  990 | | challenger bond | 根拠のない大量申立てに経済的負担を課す |
  991 | | 複数経路による提出 | Miner 本人だけでなく、署名済みの応答を第三者も relay できるようにする |
  992 | | 期限と収録保証の連動 | 正規の収録機会がないことを、Miner の計算不正と同一視しない |
  993 | 
  994 | - 予約帯域は、mempool の優先順位だけでは足りない。ブロック作成者が無視してよい優先順位では、悪意ある作成者の検閲を防げない。合意規則による容量予約、収録義務、または検証可能な収録機会の仕組みまで設計する(Ethereum の inclusion list の研究と同じ方向)。既存の `palw_court_capacity_v1`(休眠、値は仮)の court 専用 block 予算・予約枠・期限順の割当・新規 claim の受付制限を土台にする。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L3–L10

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
    3 | - **状態:** Draft / Review Required / 未実装・未有効化。2026-10-19、ユーザー指示により RFC-0025 として登録。
    4 | - **主目的:** モデル数と不正に対する court 件数が増えても、訴追権・客観的裁定・担保回収の可能性を失わず、有限の混雑を処理して復帰できるプロトコルを定める。
    5 | - **通常時の目標:** original claim の `accepted_daa` を起点として `door ≤ 3,200 DAA`(3,120 → 3,200 の変更は明示的な承認対象。§16)。
    6 | - **混雑時の方針:** 合意状態から導出した処理予約に基づき、必要な案件の証拠提出期限・Final 保留・証拠保持・bond 返還可能時点を連動して延長する。全件の固定 3,200 は約束しない。
    7 | - **前提とする RFC:** [RFC-0024](0024-court-door-3200-to-ctx-2m.md)(A=最終裁定の 1 位置化、B=2 段先読み開示、C=先行取得と強制開示、付録 C=逐次 step 搬送の削減)。本文中の「前版」は RFC-0024 を指す。
    8 | - **関連する既存実装:** `consensus/core/src/palw_court_capacity_v1.rs`(休眠。court 専用の block 予算・予約枠・期限順の割当・新規 claim の受付制限・囮の価格・未裁定は無報酬。値は仮)。本 RFC の §5〜§7 はこれを一般化・強化する位置づけで、統合時に対応を照合する(§15 段階 0)。
    9 | - **非変更:** ZK・SNARK/STARK・zkVM・PCS による計算の正しさの証明は使わない([PRINCIPLES](../PRINCIPLES.md) §8)。モデル単位の審査は行わない(PRINCIPLES §9)。本文 §15 段階 1 の「モデル profile を審査」は、claim 受付前に class の形状が証拠上限に収まるかを決定論的に判定することであり、モデルの品質・公開状況の審査ではない。
   10 | - **付属資料:** [0025-assets/](0025-assets/README.md)(ADR 草案、source-audit-v2、capacity_planner.py と 27 試験。登録時に 27 試験の合格を再確認)。前版の資料は RFC-0024 の [0024-assets/](0024-assets/)。
```

### `docs/rfc/0024-court-door-3200-to-ctx-2m.md` L571–L574

原本SHA-256: `6f59e3c4e2c44b7ad3613d6326e5c0cae80764a6708a957dd9765d95bafe9a30`

```text
  571 | ### C.3 同時発生と証拠サイズ
  572 | 
  573 | - 単発の door だけでなく、同時に N 件の court が走るときの `max_i door_i(2M, N) ≤ 3,120` を受入条件とする。court の容量は `palw_court_capacity_v1` が扱う(court 専用の block 予算、予約枠、期限順の割当、新規 claim の受付制限)。ただし値は仮で、step block の搬送がその予算に計上されているかは未確認。
  574 | - court 1 件の証拠サイズの最悪値を、固定上限として証明する。door が小さくても、証拠量が大きければ 1,000 件同時発生時の帯域問題は残るため。
```

### `docs/design/palw/integration/remaining-work-2026-10-16.md` L24–L35

原本SHA-256: `d516cb81ddfa60db67ef1ee23dc0cfa700c6af804430d1e01f4d44788151f609`

```text
   24 |   - m・k の測定(Gate C と共通)。
   25 | 
   26 | ## 2. G14 の lane — 保存済みで未完了の作業(WIP branch、未 build・未試験)
   27 | 
   28 | | branch | head | 内容 | 残り |
   29 | |---|---|---|---|
   30 | | `integ/g14-tir-pricing` | `61ab3a9a8` | frame の課金・期限を測る harness の途中 | 32×32=1,024 step の課金が要るかの実測。debit ごとの inclusion margin と `reorg_replay_moves` の積み上げの見直し。実測にもとづく cost model と期限。T-N1(試験専用 config)。正直な IR FP claim が Final・支払いまで届く node 試験 |
   31 | | `integ/g14-court-capacity` | `5b64cac9c` | court 容量(`palw_court_capacity_v1.rs` / fold)、Panel 担保の試験の途中 | 処理予算の実測 harness、占有時間の課金、期限付きスケジューラ。42 枠飽和+継続的な埋め直し攻撃の試験。Panel の Active/Resolved 分離(`palw_final_lock_full_collateral`、R_new の式)。late conviction、資本効率の比較 |
   32 | | `integ/g14-drill` | `b8858af58` | 共謀 drill の driver の途中(`scripts/drills/g14_collusion_drill.py`、`g14_drill_factory.rs`) | D-01〜D-18、court/Panel の 7 条件、正常系 P-1〜P-5、RFC-0019 R-7/R-8 の実装と実行 |
   33 | 
   34 | ## 3. G14 の未解決 gap(監査の ID)
   35 | 
```

<a id="f11"></a>

## F11 — 採用済み方針、Draft最適化、旧仕様、別branch実装の置換表が不足

### `docs/rfc/0021-design-freeze-and-documentation-governance.md` L10–L23

原本SHA-256: `1d780be260b557208bf4bdf3310e09bb3b1b7f463d1dbc13cfe6a0f81b68183a`

```text
   10 | ## 方針
   11 | 
   12 | 採用済み設計は原則凍結する。変数・パラメーターは設計を保つ範囲で証拠・影響・試験・承認を残して調整できる。重大問題で設計を変える場合は新規 RFC と新規 ADR の両方をレビュー・明示的に採択する。
   13 | [正式手続き](../governance/design-freeze.md) / [台帳](../governance/parameter-register.md)。
   14 | 
   15 | ## 文書の役割
   16 | 
   17 | README は目的別入口、STATUS は適用範囲、PRINCIPLES は原則、Spec は条件付きの規範、ADR は採用理由、RFC は技術検討、design は詳細設計、testing は方法、audit/evidence は観測、history は旧版とする。分類だけで採用・撤回・有効化しない。
   18 | 
   19 | ## 移行・検査
   20 | 
   21 | 既存主文書の番号・パスを保持し、補助資料を分ける。移動パスと前後の hash、原文を保持する。番号重複は完全パスで台帳管理し、新しい衝突を禁止する。既存の欠落資料を推測で作らず、新しい不備を検査する。
   22 | 
   23 | この整理版で添付索引の次番号 0021 を使用した。他ブランチ予約を統合時に照合する。
```

### `docs/governance/design-freeze.md` L26–L40

原本SHA-256: `aaf370f3d7b68df12574318139ba0d42bd1299a9e1630288b3466574bdde3527`

```text
   26 | ## 例外変更の手順
   27 | 
   28 | 1. 問題・深刻度・対象 network / release / commit / 仕様節と、再現または検証可能な証拠を記録する。
   29 | 2. **新規 RFC** に、問題、代替案、最小変更、影響、不変条件、互換性・移行、試験、失敗時対応を書く。
   30 | 3. **新規 ADR** に、RFC のレビュー、採用理由、不採用案、承認者、残余リスク、旧 ADR / RFC の置換節を書く。相互参照し、両方の明示的採択が揃うまで現行設計を維持する。
   31 | 4. Spec、PRINCIPLES の該当節、台帳、状態・置換関係を整合させる。PRINCIPLES 自体の改定も設計責任者（現行のユーザー承認主体）の明示的承認を要する。
   32 | 5. 実装・試験・独立レビュー・移行・リリースを確認し、必要な **有効化を別途承認**する。文書やコードの merge だけで fence・高さ・wire・fingerprint・稼働設定を変えない。
   33 | 
   34 | 緊急の停止・ロールバックは既存の承認済み運用手順内で行い、恒久的設計変更の抜け道にしません。承認権限の移譲は今回行いません。
   35 | 
   36 | ## 適用時期と履歴
   37 | 
   38 | 明示的に指示された [画像・動画・モデル改善の延期](post-mainnet-scope.md) は、リリース対象と有効化の管理であり、技術方式の差し替えではありません。重大欠陥の存在を仮定した決定ではありません。後日の再開でも凍結された設計を無断で変更できません。
   39 | 
   40 | 過去の「再審しない」「恒久」「変更不可」は通常の変更禁止として維持し、重大問題時の例外手続きは本規程に統一します。既存の禁止方式や技術的決定を今回撤回しません。番号・日付の大きい文書が自動的に上位になるわけではなく、PRINCIPLES と明示的な採択・置換関係に従います。コードとの違いは不整合として記録します。
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L925–L929

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  925 | ## 改定 — 2026-10-19(異議があった claim だけ延長する期限、応答義務に基づく勝敗、court 提出経路の liveness と DDoS 対策:採用方針)
  926 | 
  927 | **状態:** ユーザーが方針として採用(2026-10-19「この方針で進める」)。実装・敵対的試験・実ノード試験・有効化は未実施。数値(3,120・10,000・容量の割合)は設計の起点で、本番の定数ではない。2026-10-12 改定(X3 の固定期限)とは、「異議がない claim はその固定期限のまま」「有効な異議がある claim だけ、事前に定めた規則で関連期限を一括して延長する」という形で両立させる。延長の規則は新しい versioned fence で導入し、旧 claim には旧 ruleset を適用する。関連:[RFC-0024](0024-court-door-3200-to-ctx-2m.md)(door の短縮)、[RFC-0025](0025-scalable-objective-court-many-models-mass-disputes.md)(大量 court の容量予約と混雑延長)。
  928 | 
  929 | ### A. 対話の回数(16 分岐・1 tile 16 位置の場合)
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L16–L37

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
   16 | ## 0. 前版との関係・根拠
   17 | 
   18 | 本書は前版の A＝最終裁定の1位置化、B＝最大2段の先読み開示、C＝先行取得と強制開示を引き継ぎ、モデル横断の容量管理・混雑延長・pruning/IBD を追加した**統合版**である。実装では本書を上位とし、衝突しない算術・証拠認証の詳細は [前版RFC](0024-court-door-3200-to-ctx-2m.md) を参照する。
   19 | 
   20 | 前版の「全受理負荷で必ず3,200以下」「期限を途中で延長しない」は、v2 の **通常予約と、事前合意済みの有界な混雑延長**に置き換える。待ち行列の開始を遅らせて、元の受付からの時間を隠すことは禁止したままである。
   21 | 
   22 | 対象は添付 `misakas-pre(2).zip` の snapshot とユーザー提示の数値であり、最新 branch・出荷 binary・現在の t12 の監査ではない。追加の読解根拠は [source-audit-v2.md](0025-assets/source-audit-v2.md)、前版の根拠は [reference-v1/source-audit.md](0024-assets/source-audit.md) にある。
   23 | 
   24 | コード上の重要な確認事項:
   25 | 
   26 | | 確認事項 | 根拠 | v2 への影響 |
   27 | |---|---|---|
   28 | | 既存 frame は `collect_until`、`material_until`、`bond_exit_after` を連動して導出する | V2-S01 | bond返還だけの延長では不十分 |
   29 | | public conviction は `[accepted_daa, collect_until_daa)` で受理。現行コメントは期限固定・非延長を要求する | V2-S02 | 新ruleset・明示的移行が必要。排他的境界に注意 |
   30 | | custodian の署名済み pledge の保持期間と frame の期限が照合される | V2-S03 | 期限だけ書き換えると署名・契約・料金が不整合になる |
   31 | | owner 上限は8で、全ownerがactiveなら追加Openを拒否する | V2-S04–05 | idle回収だけでは共謀者による枠占有を解決していない |
   32 | | pruning snapshot は未完の close group のbytesも運ぶ | V2-S06 | rootだけ残して未完courtを削除できない |
   33 | | snapshotの既存close上限の試験値は2,250,000 B | V2-S07–09 | 新しい小型証拠上限を既に達成しているとは扱わない |
   34 | 
   35 | ユーザー提示の **17,408 B** は新しい分割片サイズの提案値として扱う。snapshot検索では、この値を現行court stepサイズの定数として裏付ける一致は確認していない。モデルのFFN幅の17,408等と混同しない。
   36 | 
   37 | 以降の「MUST／禁止／必要」は提案する規範要件を表す。未測定の数値・未証明の被覆を採用済み実装の事実として扱わない。
```

### `docs/rfc/INDEX.md` L21–L31

原本SHA-256: `1600c188a758bdff2cc3b00605f297aae2378aa491446131bd6f066513644b6a`

```text
   21 | | RFC-0013 | [Reproducible layouts and resource-bounded model onboarding tools](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) | 原文と後続改定を参照 |
   22 | | RFC-0014 | [Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ](0014-panel-independent-fraud-prosecution.md) | 原文と後続改定を参照 |
   23 | | RFC-0015 | [Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する](0015-panel-free-permissionless-verification.md) | 原文と後続改定を参照 |
   24 | | RFC-0016 | [bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) | 採用設計・測定待ち・休眠 |
   25 | | RFC-0017 | [モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)](0017-model-eligibility-examination-and-renewal.md) | 二階層採用・旧審査撤回 |
   26 | | RFC-0018 | [DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束](0018-dag-parallel-multi-model-consensus.md) | Draft / 個別採択あり |
   27 | | RFC-0019 | [年次モデル追加フォーク: 段階的ソフトウェア展開・年次モデルカタログ・合意は不変](0019-annual-model-upgrade.md) | Draft / 原文参照 |
   28 | | RFC-0020 | [長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない](0020-long-context-calibration-memory.md) | Draft / 部分実装 |
   29 | | RFC-0021 | [設計凍結・例外変更手続きと文書の役割分離](0021-design-freeze-and-documentation-governance.md) | Accepted / 文書管理 |
   30 | | RFC-0022 | [初期 mainnet から画像・動画・モデル改善を外す適用範囲](0022-post-mainnet-media-and-model-improvement-scope.md) | Accepted / 延期・無効化方針 |
   31 | | RFC-0023 | [未解決 claim の責任を保持し、それに依存しない claim を進める](0023-unresolved-claim-liability-and-independent-progress.md) | 原文と後続改定を参照 |
```

### `docs/rfc/TOPICS.md` L41–L48

原本SHA-256: `cfdf9f29926552b966c10f01c3fbf9b6d05118fd54636e954c8f344c114e8016`

```text
   41 | ## Verification / court
   42 | 
   43 | - [RFC-0006 — PALW layer-sharded panels — bounded cell verification from authenticated boundaries, with independen](0006-palw-layer-sharded-panels.md)
   44 | - [RFC-0007 — PALW probabilistic detection — direct checks, authenticated receipts, and deterministic court](0007-palw-verification-certificates-and-algebraic-checks.md)
   45 | - [RFC-0010 — Permissionless PALW Panel binding and claim completion](0010-permissionless-palw-panel-and-claim-completion.md)
   46 | - [RFC-0014 — Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ](0014-panel-independent-fraud-prosecution.md)
   47 | - [RFC-0015 — Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する](0015-panel-free-permissionless-verification.md)
   48 | 
```

<a id="p01"></a>

## P01 — 上位原則：計算不正、資料不提供、訴追中Finalの区別

### `docs/PRINCIPLES.md` L123–L140

原本SHA-256: `a5eeb56f27d3dcabce0cbb5e4ffbc097af04b6eece5dd6ac07e991a361776ac6`

```text
  123 | ## 3. 不正発覚後の客観的裁定
  124 | 
  125 | 承認済み検査で不一致が発見された場合、その後の有罪認定をPanelの投票や主観的判断に委ねてはならない。
  126 | 
  127 | 対象は、すべての報酬対象class・kernel・verification planが対象とする計算、job、input、output、state、およびData Availabilityの違反である。producerとPanel全員が共謀する場合でも、次が成り立たなければならない。
  128 | 
  129 | - Panel外の1人の適格なpublic bonded verifierが、producerの秘密状態やPanelの協力なしに、公開・認証済みmaterialから独立に検査できる。
  130 | - その検証者が、不正箇所を有界に局所化できる。
  131 | - その検証者が、客観的な裁定まで進められる。
  132 | 
  133 | 裁定の結果は、次のとおり扱う。
  134 | 
  135 | - 計算不正が立証された場合、決定論的なexact courtにより、客観的な有罪判定、claimの無効化、および責任追及へ到達する。
  136 | - 義務付けられた証拠が開示されない場合、計算不正を推定して有罪とはしない。客観的なDA/defaultと、それに対応する責任を判定する。
  137 | - 有効な異議申立てが継続中の場合、定められた有界の手続きが完了するまで、不正claimを経済的Finalへ逃してはならない。
  138 | - 不正が立証されなかった場合、正当なclaimや検証者が恣意的に処罰されないよう、棄却・担保返還・費用配分を決定論的に処理する。
  139 | 
  140 | 不正の発見は確率的であっても、発見された具体的な不正の有罪判定は確率的であってはならない。
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L316–L333

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  316 | ### 5.2 回答の処理
  317 | 
  318 | * root/位置/jobに一致しないbytesは適格回答にならない。未回答義務は所定deadlineまで残る。
  319 | * 正しいmembershipを持つ回答はDA義務を満たし、計算の正しさはexact courtへ渡す。
  320 | * bounded one-moveで裁けるものは既存の束縛された判定 [S13] を再利用する。
  321 | * fusedでdissectionが必要なら、開示済みcommitted tileと認証operandを起点に既存held courtへ進む。
  322 | * 回答がない場合は客観的なDA/defaultを処理する。計算fraudの証明として記録しない。
  323 | 
  324 | commitされた不正が問題のsiteにあり、正直なchallengerがそこへ到達した場合には、
  325 | producerは「偽tileを開示してexact裁定を受ける」か「必須tileを開示せずDA/defaultになる」。
  326 | 正しいtileを回答した場合は棄却する。この二択を全siteが不正だという意味に拡張しない。
  327 | 
  328 | ### 5.3 exact semantics
  329 | 
  330 | fused attentionの分解は、元kernelの丸め・saturation・整数範囲・accumulation順序を維持する。
  331 | 非結合的な演算を分割したせいでhonest outputと違う値になるdissectionは禁止する。
  332 | input・weight・state opening、位置とshapeをclaim bindingまで検証する。
  333 | kernelが未対応、またはterminalが上限内に収まらないなら、そのplanはDisputeAdmissibleにしない。
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L473–L489

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  473 | ## 8. challengerの経済条件と処罰
  474 | 
  475 | * interactive開示・court・予約には、適格なpublic bondとsession deposit/exposureを要求する。
  476 |   genesis membershipやPanel seatであることを要求しない。
  477 | * 適格なfraud verdictではdepositを返し、既存reporter/court rewardの原則に従ってbountyを処理する。
  478 | * false accusation、challenger default、未認証materialだけの申告は、それぞれ定義された費用を課す。
  479 | * malformed objectをdecoderで拒否することと、受理済みsessionのdepositをburnすることを区別する。
  480 | * 代数的検査のローカルfailureだけではproducerをslashしない。
  481 |   objective exact verdict、証明された証拠/義務違反、または定義されたDA/defaultが必要である。
  482 | * producer処罰には現行conviction funnelを再利用する。reservation・escrow/rights・S2/S3等の
  483 |   applicable tierを維持し、新しい別経路で`claim.reserved`だけの処罰へ弱めない [S17]。
  484 | * bounty/徴収ledgerはnominal amountと実際のcollectedを区別し、未回収額を支払財源にしない。
  485 | * classの最大gain、同時claims/予約とlock lifetimeを結び付ける。
  486 |   proof受理前に担保が開放される、攻撃がdepositより大きな無償検査を強制する、といった経路を試験する。
  487 | 
  488 | DA/defaultは可用性または義務違反の処理であり、それだけで全Valid signerの計算fraudを証明しない。
  489 | court default、held verdict、execution-proving fraudのbasisは現行の区別を維持する。
```

<a id="p02"></a>

## P02 — 資料取得のオフチェーン経路は、court対話の権威をオフチェーンに移さない

### `docs/rfc/0006-palw-layer-sharded-panels.md` L355–L384

原本SHA-256: `06ae27140abd9812a80678750240b9e84785ecbd3871fc48fd64483af029f50c`

```text
  355 | ## 3. Availability of boundary rows
  356 | 
  357 | A cell reads more of the producer's commitments than a replay does. A replay needs only the tokens and
  358 | compares roots; a cell needs its inputs' leaves (§1.3). These are the routes, in order:
  359 | 
  360 | 1. **From the panel.** The previous shard's seats recompute the boundary rows of the same segment
  361 |    anyway, and its cell's check succeeds only if they equal the committed ones. A seat may take its
  362 |    inputs from any peer: every input is checked against the step root before use, so the source does
  363 |    not matter. A stratified panel can therefore run as a pipeline over shards, shard `i` starting each
  364 |    segment when shard `i − 1` has published its carry-outs. This is node policy and needs no consensus
  365 |    object.
  366 | 2. **From the producer, off chain.** It is ADR-0111's fast path: a signed request and a 6-DAA wait
  367 |    (`PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1`). The producer's served annex (evidence-transport-scope,
  368 |    option B) is the natural carrier: per cell, the carry-in rows and its opening siblings.
  369 | 3. **On chain, enforced.** It uses the IR DA units past `palw_tir_fence2`: `TirStepLeaf { index }` for a
  370 |    leaf, and `TirStepNode` for a 10-level subtree frontier (64 KiB). Withholding past `W_disclose`
  371 |    (1,200 DAA on testnet-12) is a default under the DA court's rules: void and slash (ADR-0062, ADR-0152
  372 |    DA-1..9). *(2026-10-15: demands are limited to claim-specific step material; demands for model weight bytes are
  373 |    prohibited for new rulesets — spec closures, ALL-MODEL item 3.)*
  374 | 
  375 | A cell's inputs are *runs*: one carry-out run per position, and history rows in whole tiles. So this RFC
  376 | adds one unit, **`TirStepRun { first, count }`**: a contiguous run of step leaves with its two boundary
  377 | paths, bounded by the carrier (100,000 bytes). One demand then serves about `⌊100,000 / (lanes × 4)⌋` positions of a segment's carry-ins, less the
  378 | paths (≈ 15 positions of a 1,536-wide residual per unit, against one per `TirStepLeaf`).
  379 | 
  380 | The ADR-0152 session limits (a seat: 1 open, 4 lifetime) stand. A demand names a run, not a cell, so a
  381 | seat whose producer withholds can force a default within its limits.
  382 | 
  383 | A seat that cannot obtain its inputs says `Unavailable { chunk, daa }`, as today: it abstains and is not
  384 | liable (PALW-VF-17). `seat_count − quorum + 1` such receipts redraw (SR-9).
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L292–L325

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  292 | ### 5.1 retrievalとadjudicationを分離する
  293 | 
  294 | 現行のfused `StepLeaf` は `DaUnitNeedsDissection` で拒否される [S09], [S12]。
  295 | これを単純に拒否解除して、巨大fused計算をone-stepで再実行させてはならない。
  296 | 
  297 | 提案unit `CommittedKernelWitness` はまず「何をcommitしたか」を有界に開示する。
  298 | 既存enumのtag・古いobjectの意味は保持し、追加variant/encodingは別fence以降に限定する。
  299 | 
  300 | ~~~text
  301 | CommittedKernelWitness request (proposed)
  302 |     claim_id, dispute_plan_id, kernel_site, witness_part_index
  303 | 
  304 | CommittedKernelWitness answer (proposed)
  305 |     committed output tile / witness preimage
  306 |     membership opening to the claim's bound roots
  307 |     kernel coordinate, shape, layout and entry-state references
  308 |     authenticated query / state / operand references
  309 |     deterministic dissection root and bounded witness-part commitments
  310 | ~~~
  311 | 
  312 | query/operand全体が大きい場合は、bounded partとそのroot、既存held dissectionへの参照に分ける。
  313 | 単一tileの数KB〜数MBというサイズを一般保証しない。
  314 | 全sessionのbytes・CPU・RAM・roundsと最大carrierを別々に検査する。
  315 | 
  316 | ### 5.2 回答の処理
  317 | 
  318 | * root/位置/jobに一致しないbytesは適格回答にならない。未回答義務は所定deadlineまで残る。
  319 | * 正しいmembershipを持つ回答はDA義務を満たし、計算の正しさはexact courtへ渡す。
  320 | * bounded one-moveで裁けるものは既存の束縛された判定 [S13] を再利用する。
  321 | * fusedでdissectionが必要なら、開示済みcommitted tileと認証operandを起点に既存held courtへ進む。
  322 | * 回答がない場合は客観的なDA/defaultを処理する。計算fraudの証明として記録しない。
  323 | 
  324 | commitされた不正が問題のsiteにあり、正直なchallengerがそこへ到達した場合には、
  325 | producerは「偽tileを開示してexact裁定を受ける」か「必須tileを開示せずDA/defaultになる」。
```

### `docs/rfc/0024-court-door-3200-to-ctx-2m.md` L266–L278

原本SHA-256: `6f59e3c4e2c44b7ad3613d6326e5c0cae80764a6708a957dd9765d95bafe9a30`

```text
  266 | producer は推論中に既存の canonical material と取得 index を保存・整列し、execution root 確定後に同じ material を配布する。mirror や verifier は任意に先行取得できる。ただし、検査乱数の決定順序は維持し、未来の challenge を予知する必要がある全候補の巨大な証拠生成を通常処理に課さない。
  267 | 
  268 | B の数値 frontier は dispute の root totals に依存し得る。キャッシュする場合は root totals digest と site を含めて key を作り、別の root claim の集計を流用しない。
  269 | 
  270 | ### 6.2 2 経路
  271 | 
  272 | **任意取得の速い経路:** 認証済み bytes をオフチェーンで取得し、検証者が対応する既存の on-chain opening または bounded filing を提出する。第三者から取得した bytes でも root が一致すれば資料として使えるが、第三者の署名だけを court の正しさの根拠にしない。
  273 | 
  274 | **強制経路:** 任意取得が失敗したら、既存 original obligation に対して on-chain の有効な要求を受理する。指名された義務者、request、deadline、最大サイズは accepted state で確定する。投稿の tag 245 経路との対応は最新 branch の schema を確認して接続し、tag を推測で割り当てない。
  275 | 
  276 | 最悪 door は強制経路で評価する。「事前に取れているはず」を理由にこの経路を削除しない。
  277 | 
  278 | ### 6.3 起点をずらして時間を隠さない
```

<a id="p03"></a>

## P03 — receipt不足の中立終了とcourtの応答義務違反は別

### `docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md` L494–L506

原本SHA-256: `eadad4e17cc30e8acf0b0f882d664fa1bedb871af9e6e63fb7bd1c00d924c07b`

```text
  494 | ### II.7 The trace-serving obligation and its DA
  495 | 
  496 | The producer's duty is to **serve the witness** of a claim to its panel's seats during the receipt window: chunked per interval, on
  497 | request, through the interval lane's off-chain path.
  498 | 
  499 | - **Enforcement is the licence.** A seat that cannot obtain a witness either recomputes, if it holds the class, or files
  500 |   `Unavailable { chunk_index, requested_daa }`. Under ADR-0065 Decision 4, `Unavailable` abstains: a producer that withholds gets no
  501 |   licence, the claim redraws once and then voids at `ReceiptTimeout`, and nobody is slashed for silence.
  502 | - **Naming witness chunks.** Today `Unavailable` names a chunk of the committed trace manifest. To name witness chunks, the manifest
  503 |   gains a witness root (fence `palw_witness_manifest_v1`, §Activation): the producer commits the witness's chunk digests at claim time.
  504 |   Without the fence, Part II still works; seats simply cannot name which witness chunk was withheld.
  505 | - **Availability past the window** belongs to the capture, not the witness. The court never needs the witness (§II.8). `Held` leaves
  506 |   (§I.7) attest the capture's availability where a network wants it beyond the producer.
```

### `docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md` L236–L246

原本SHA-256: `de4435a333e023fe870f0e5dd218a9a7e53e2488e1662436f7a0ba15c1364004`

```text
  236 | モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
  237 | 
  238 | ## 改定 — 2026-10-12（Panel=1暫定mode・有界の交代・正の証拠を必須にしたFinal：提案）
  239 | 
  240 | **状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4。
  241 | 
  242 | - **§0追記:** 通常の新reward-bearing modeは、1 claimにつき同時に有効なassigned verification seatを1つ持つ。Panel=0へのtimeout fallbackを禁止する。候補集合には複数の参加者を認め、non-seatの検査・反証権限を制限しない。Minerとassigned Panelは適用scopeの正しい登録モデルを保有し、実行/検査と認証に必要な能力を備える（ADR-0177 D1補足）。モデル登録者に一般第三者への提供義務は導入しない。合意判定は認証済み状態と証拠に基づき、特定nodeのHTTP結果・model取得速度を直接入力にしない。Panelは承認済みの確率的checker、固定scope、commit済みstatementへの適正なchallenge、必要な正の検証証拠に基づくreceiptを提出し、署名数1だけを計算正当性の証明としない。producer/registrantとの同一bond・operatorを除外する（同一実質所有者でないことの証明ではない）。
  243 | - **有界の交代:** snapshot、claim seal、選出policy、代替候補順序、最大交代数はclaimの規則に固定し、交代のために新しい乱数・候補集合・採掘枠・絶対期限を得ない。健全な検査に必要な残り時間がなければ交代を開始せず、候補か時間が尽きれば非不正・無報酬で終了する。検査不一致が出たら、bounded pursuitまたは完成したobjective proofへ進む。
  244 | - **Final:** `FinalEligible = OriginalRulesetAndIdentityBound ∧ NetworkModelBondBudgetsSatisfied ∧ AuthorizedSingleSeatPositiveEvidenceAccepted ∧ RequiredClaimEvidenceAndCourtObligationsSatisfied ∧ ObservationCutoffPassed ∧ NoUnresolvedAcceptedPursuit ∧ NotRefuted ∧ UniqueWorkAndSettlementSatisfied`。支払、vesting、ブロック権、Final weight、EVM/bridge等のreaderも同じ結論を使う。
  245 | - **1-seat移行監査:** `seat_count=1` の一括置換を禁止する。segment mask、`seats-1`、quorum、最低独立seat数、shard outsider、署名者担保、Panel fee配分、再抽選、FalseValid attribution、DA/court、全Final経路を点検し、単一seatで全検証scopeを担えないprofileはこのmodeで有効化しない（RFC-0002 seating・RFC-0006 shard・RFC-0003 readinessにも適用）。
  246 | - **モデル配分:** 正規Panel予約は1 claimにつき1 seatとし、無応答の旧seat・待機seat・交代回数から架空の追加元本を作らない（ADR-0177 2026-10-12改定）。
```

### `docs/rfc/0014-panel-independent-fraud-prosecution.md` L956–L967

原本SHA-256: `458d6b61eeaaedf943508a960215a02e10127e41f4efefd03b2026c877dd21f9`

```text
  956 | ### C. 「証明できなければ slash」を分ける
  957 | 
  958 | | 起きたこと | 判定・処理 |
  959 | |---|---|
  960 | | 局所再計算で、Miner の主張との矛盾が証明された | 計算不正として Miner 側を slash |
  961 | | Miner が、応答可能な正規の開示要求に期限内に応答しなかった | 開示・応答義務違反として責任者を slash / default 処理 |
  962 | | Miner は必要な応答をしたが、Panel が不正を証明できなかった | **Miner を slash しない。** 申立てを棄却し、Panel 側に規定の敗訴・費用処理 |
  963 | | Panel が次の区間を選ばない、または最後の証拠を出さない | Panel 側の放棄・期限違反として処理 |
  964 | | 正直な当事者に必要な収録容量が与えられず、手続きが終わらなかった | それだけを計算不正の根拠にしない。容量・期限保証の設計不備として扱う(E) |
  965 | 
  966 | - 最後の tile まで正しく絞れた後に Panel が証拠を出さずに期限を迎えた場合、「Miner の正しさが証明されなかったから Miner を slash」としてはならない。Panel が最後に何もしないだけで、正しい Miner を負けさせられるためである。既存仕様は既にこれを区別している:終端では Miner に新たな応答時計を課さず、誰も最後の証拠で閉じなければ、session の最終期限で challenger 側の敗北として終わる。途中の応答待ちでは、応答義務のある側が期限を超えればその側が負ける。
  967 | - 既存コードの、敗訴した challenger の担保からの費用徴収と、fence に応じた係争時間の課金を、新しい期限に整合させて使う。新しい勝敗原則は作らない。
```

<a id="p04"></a>

## P04 — モデル入手不介入、固定identity、claim検証を分離

### `docs/rfc/0017-model-eligibility-examination-and-renewal.md` L9–L31

原本SHA-256: `9928a16fd00c9de212cc78a9b85b4d0f82933d32c8cd89cf74149da8e264fda8`

```text
    9 | ## 現行仕様 — ユーザー判断 2026-10-15 第 4 回・第 5 回
   10 | 
   11 | 本 RFC は、モデル資格の審査の RFC から、**モデル階層の RFC** に改めた。判断記録は [ユーザー判断](../history/decisions/2026-10/user-decisions-2026-10-15-adr-rfc16-rfc17.md) §7b・§7c。
   12 | 
   13 | ユーザー判断(原文、第 5 回):「genesis/fork モデルは 1 倍で有効、後から追加されるモデルは 0.3 倍で、そこから検証などはなく 0.3 倍のまま固定。つまりモデルに対しての検証がなくなった。」
   14 | 
   15 | 1. **1.0 の階層。** `ApprovedModels(h)` に含まれるモデルである。`ApprovedModels(h)` は、genesis のモデルと、高さ `h` までに有効化された年次フォーク(ソフト / ハード)が指定した「モデル ID + 固定 artifact root」の集合である。
   16 |    * 年次フォークは追加するだけで、既存モデルを上書きしない(ADR-0175)。
   17 |    * 指定は政策的決定であり、公開性・独立検証可能性の技術的証明ではない。
   18 | 2. **0.3 の階層。** 後から登録したモデルは、登録した時点から審査なしで 0.3 に固定する。
   19 |    * 昇格・更新・失効・試行手数料・試行上限はない。
   20 |    * 1.0 になる経路は年次フォークでの指定だけである。
   21 |    * registry の `Active` などの状態は、運用状態として階層とは別に残る。
   22 | 3. **モデル単位の検証・審査は、すべてのモデルについて廃止した。** 下記「廃止した審査」の段階(Registered / Probation / Eligible / Lapsed)、抽選審査、更新、失効、試行手数料、試行上限、公平な待ち行列は適用しない。
   23 | 4. **係数は一度だけ掛ける。**
   24 |    * `A_m = c_m · S̄_m^1.5`(ADR-0177)。その `P_m` を RFC-0016 V+ の物理ブロック権利と coinbase 配分の両方が使う。
   25 |    * 受理ブロックの報酬に重ねて掛けない。
   26 |    * 登録 tier 全体の発行上限 `κ` により、多数登録で 0.3 の枠を増やせない。
   27 |    * RFC-0016 の entrant reserve と旧 Probation 予算は審査の廃止で無意味になる。`ε = 0` とし、`κ` だけを上限とする。
   28 | 5. **claim 単位の検証は、すべての階層で維持する**(ユーザー判断、原文:「これは残す。そうじゃないと claim の検証ができない」)。
   29 |    * claim ごとの認証は [RFC-0018](0018-dag-parallel-multi-model-consensus.md) の Gate C(`SecurityCertified`)にある。これは、抽選検証者の m-of-k 承認と G14 公開告発 window による、前提に基づく認証である。court と確率的検出も維持する。
   30 |    * 未認証の claim は、genesis・年次指定・登録のどの階層でも、fork choice・決済深度・Rule E 参加・settled anchor・weight を進めない。
   31 |    * 旧 U-2 の Probation bar は対象が存在しないため削除した。
```

### `docs/rfc/0019-annual-model-upgrade.md` L135–L181

原本SHA-256: `ea6c0bbcb654f575bb8396e909f2b31ed78004a3b4897fba1d7088b8e0c66d0e`

```text
  135 | ### 5.4 追加のみ(ADR-0175)
  136 | 
  137 | * **MUST NOT** 既存モデルを上書き・削除しない。既存モデルの root を変えない。同じ `class_id` は連鎖全体で一度だけ現れる(同じ root での再掲も、別 root での再掲も拒否)。
  138 | * 過去の edition を書き換えると次の edition の `previous_catalog_root` が合わなくなる。
  139 | * リリースをまたいでは、新しいリリースの連鎖が前のリリースの連鎖を**接頭辞として**含まなければならない(`PalwAnnualCatalogChainV1::extends`、`:234`。manifest の `--previous` 検査も同じ)。
  140 | * **「genesis モデル ID の更新」は ID の置換・再割当てを意味しない。** genesis assembly の既存登録、edition 0、旧 `class_id`、旧 `artifact_root`、登録 ID / line / 経済履歴は変更しない。新しい重み・tokenizer・計算仕様等は、別の `class_id` と固定 `artifact_root` を持つ**独立したモデル**として登録し、edition 1 以降に追加する。`genesis_id`(genesis 8 テンプレートの番号)を振り直すことと、合意上のモデル追加を混同しない(ADR-0175)。旧 ID を別 root に向ける alias / 自動切替は認めない。
  141 | 
  142 | ### 5.5 genesis = edition 0
  143 | 
  144 | * edition 0 は genesis モデルを明示して並べる(SHOULD)。並べると edition 0 の root が genesis モデル集合に commit し、manifest と RPC で読める。
  145 | * genesis assembly が登録した class(`registrant_bond = None`)は、並べなくてもチェーン上の事実として 1.0 である(ADR-0179 の `PalwTierSourceV1::GenesisAssembly`、既存の挙動を保持)。したがって edition 0 が空でも階層は同じであり、testnet-12 の DAA 14,000 更新(§12)では edition 0 の中身は機能に影響しない。並べるかどうかはユーザー判断(§14 U-5)。
  146 | 
  147 | ## 6. 階層規則
  148 | 
  149 | ### 6.1 `model_tier_permille`
  150 | 
  151 | ```text
  152 | model_tier_permille(class_id, artifact_root, h) = 1000  if (class_id, artifact_root) ∈ ApprovedModels(h)   — h で有効なカタログ
  153 |                                                 = 300   otherwise                                     — 登録 tier(設計値 c)
  154 |                                                 = 1000  for every model below palw_model_tier_v1      — A-2
  155 | ```
  156 | 
  157 | 実装は `model_tier_permille`(`palw_annual_catalog_v1.rs:302`)で、ADR-0179 の**単一の階層 API**(`palw_model_tier_of_v1`、`palw_model_tier_coefficient_permille_v1`、`palw_model_tier_v1.rs:183`)を合成しただけである。別の階層定義は作らない。所属判定は `PalwModelTierMirrorV1::listed_at`(`palw_model_tier_v1.rs:136`)がカタログ連鎖の `listed_at`(`palw_annual_catalog_v1.rs:253`)を呼ぶ。
  158 | 
  159 | ### 6.2 registry の `Active` は 1.0 ではない
  160 | 
  161 | `PalwModelLifecycleV1::Active` は運用状態である(`palw_model_registry_v1.rs:363` に明記)。登録モデルが何年 `Active` でも 0.3 のままであり、カタログに載ったモデルは `Held` でも配分重みは 1.0 である。階層 API は lifecycle を引数に取らない — これが分離の構造である。RPC は `state`(lifecycle)と `modelTier`(階層)を別の field で報告する。
  162 | 
  163 | ### 6.3 一度だけ、`A_m` に
  164 | 
  165 | * `A_m = t_m · S̄_m^α`(α = 1.5、30 日の miner + 認証済み verifier 資本、ADR-0177 V3)を allocation epoch の roll で一度だけ計算する(`roll_epoch_v3_tiered`、`palw_bond_budget_v1/allocation_v3.rs:209`、fold の呼出は `palw_bond_budget_fold_v1.rs:2121`)。
  166 | * その `P_m = A_m / ΣA` が RFC-0016 V+ の物理ブロック権利と coinbase 配分の**両方**の入力である。受理ブロックの報酬や claim の reservation に 0.3 を重ねて掛けない(0.09 にしない)。
  167 | * 登録 tier 全体の上限 κ(設計値 300‰ of ΣA)を維持する。登録モデルしかないとき、正規化だけでは 0.3 が相殺され絶対上限にならない。κ で削られた分は未発行のまま残り、1.0 のモデルへ再配分しない。bond 単位の Q/B/R/F 上限も維持する。
  168 | * RFC-0016 の entrant reserve は階層 fence 以後 0 になる(審査がないので entrant がいない。`palw_rfc0016_fold_v1.rs:142`)。
  169 | 
  170 | ### 6.4 受理 DAA と境界の規則(権利を二重に発行しない)
  171 | 
  172 | 1. **claim の受理 DAA** は、その claim を mergeset で適用した chain block の DAA である(`PalwBlockContextV2::daa_score` → `PalwClaimStateV2::accepted_daa`、`transition_builder.rs:4386`)。BLUE と RED の merged block で同じであり、merged block 自身の header DAA ではない(`palw_v2_merged_works` は accepting block の `point` で admission する、`processor/palw.rs:9723`)。
  173 | 2. **階層は allocation epoch ごとに一度だけ読まれる。** roll はその epoch の最初の chain block(DAA `now`)で起こり、`palw_model_tier_of_model_v1(.., now)` で各モデルの階層を読む。同じ epoch で roll は二度起こらない(`rolled` の判定、`palw_bond_budget_fold_v1.rs:2084`)。
  174 | 3. したがって、ある claim の権利と報酬が反映する階層は「受理 DAA を含む allocation epoch の roll DAA における階層」である(`palw_catalog_tier_reference_daa_v1`、`palw_annual_catalog_v1.rs:317`)。edition が epoch の途中の H で有効になっても、新しい階層が効くのは roll DAA ≥ H の最初の epoch からである。
  175 | 4. **RED の跨ぎ:** header DAA < H で掘られ、DAA ≥ H の chain block に merge された RED block は、その accepting block の epoch で一度だけ受理され、その epoch の一つの snapshot から権利を消費する。V+ の権利は epoch ごとに一度だけ発行され、受理時に一度だけ消費される。カタログの有効化は再発行を起こさない。
  176 | 5. **過去を再評価しない:** H より前に受理された claim は、受理時に予約された権利・escrow・報酬のまま完結する。Final・court・payout は階層を読まない。
  177 | 6. 推奨(SHOULD): edition の `activation_daa` を allocation epoch の境界に合わせる。合わせなくても規則は決定論的だが、RPC の「ACTIVE」と配分に効く時点が一致して読みやすい(§13)。
  178 | 
  179 | ### 6.5 階層は経済だけ — Gate C は全 edition に
  180 | 
  181 | 年次掲載モデルにも genesis モデルにも、safe frontier・safe weight・settlement 深度・rule E 参加・settled anchor・weight の特権はない。認証されていない claim(`SecurityCertified` でない claim)は、どの edition・どの階層でもそれらを進めない(PRINCIPLES §9.5、§0 の 2026-10-15 改定、RFC-0018 §7)。試験は RFC-0018 の `rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt` に edition 0 掲載と edition 1 掲載の両方を加えた(§11 C-3)。
```

### `docs/PRINCIPLES.md` L224–L251

原本SHA-256: `a5eeb56f27d3dcabce0cbb5e4ffbc097af04b6eece5dd6ac07e991a361776ac6`

```text
  224 | ## 8. モデル計算の暗号学的証明を使用しない（2026-10-15、ユーザー恒久決定）
  225 | 
  226 | **MISAKA は LLM・モデル計算の正しさを暗号学的に証明する仕組みを一切使用せず、今後も使用しない。**
  227 | この節は本書の他の節、既存・将来の ADR・RFC・設計・実装・有効化計画・研究項目に優先する。
  228 | 過去の文書に採用・拡張候補と書かれていても、その記述は撤回対象であり、採用を継続する根拠にはならない。
  229 | 
  230 | 1. **採用禁止。** zero-knowledge 証明、zk-SNARK / zk-STARK、zkVM・zkML、多項式コミットメント（PCS）等に基づく succinct argument / validity proof、再帰証明・proof aggregation を使用しない。GKR や producer が送信する多項式証明 transcript による計算 validity argument へ迂回しない。導入を将来課題や研究候補にも残さない。
  231 | 2. **検証方法。** 確率的検出（サンプリング再実行・決定論的再計算・代数的スポットチェック）を行い、不一致を公開証拠で局所化し、決定論的に裁定し、経済的に抑止する。直接の Freivalds 検査などは、その検査器が読んだ入力と検査した関係だけを対象とし、全モデル計算の暗号学的証明とは呼ばない。
  232 | 3. **合意 credit の認証。** 抽選された検証者の承認を、正直な割合の明示的な前提の下で用いる。その割合、抽選・重み・相関、取得可能性、期限、検出範囲を報告する。署名や quorum は承認者と対象の認証であり、算術真実の暗号学的証明ではない。異議時の有罪判定は公開証拠と決定論的 court による。 **楽観的な Final または一致した local replay は、抽選された検証者の承認を代替しない。** material の再計算、契約の受入、データの同一性確認だけで consensus credit の承認を得たと扱ってはならない。
  233 | 4. **利用できる暗号機構。** ハッシュ・署名・Merkle commitment・VRF/beacon は、データの同一性・認証・乱数に使用できる。Merkle opening は保持データの所属を認証するもので、モデル計算の正当性を証明するものではない。
  234 | 5. **実装と互換性。** 禁止した方式を新規生成・受理し、報酬や weight・credit の計算正当性の根拠にしてはならない。旧履歴の読み取りや wire ID の保存が必要な場合も、その目的に限る。旧 ID の意味を別方式へ再解釈せず、必要な移行は明示的な version/fence で行う。
  235 | 6. **評価。** 128-bit 等の数字を全計算の暗号学的健全性として保証しない。許容する検査の誤検出・見逃し、反復・grinding、正直な検証者の参加、データ取得、費用と経済条件を、それぞれの前提を明示して評価する。未測定値は UNKNOWN とし、禁止方式の実装を不足解消の条件にはしない。
  236 | 
  237 | 7. **理由。** ZK などで LLM の計算を実用的に証明することは不可能である(ユーザー判断)。
  238 | 8. **既存文書。** ZK・succinct proof・compact proof を「将来」「研究」として挙げている箇所(例: `docs/design/palw/rfc9-l2-fork-choice.md` 案 C、RFC-0015 §10、`docs/design/palw/rfc-0009-remote-record.md`)は本決定により除外する。過去の記述は履歴として残し、除外注記を付ける。
  239 | 9. **エージェント(Claude Code、Codex など)へ。** 設計・実装・レビュー・提案で上記の暗号学的計算証明を持ち出さない。既存の記述や実装を見つけたら、除外である旨の注記を付けるか撤回する。リポジトリ直下の `CLAUDE.md`・`AGENTS.md` にも同じ決定を記載している。
  240 | 
  241 | (本節は 2026-10-15 に Codex と Claude Code がそれぞれ追記した 2 つの §8 を 1 つに統合したもの。)
  242 | 
  243 | ## 9. モデルの二階層とモデル単位の検証の不採用 — 2026-10-15 ユーザー決定(凍結前提)
  244 | 
  245 | **本節は原則凍結した前提である。** 通常は再審・変更しない。変数・パラメーター調整と重大問題時の例外変更は [§11](#design-freeze) に従う。例外では新規 RFC と新規 ADR の両方の採択、設計責任者の明示的承認、対象節の改定を要する。今回、階層規則や数値は変更しない。
  246 | 
  247 | 1. **genesis と年次フォークで指定されたモデルは 1.0 である。** 年 1 回程度のソフトフォークまたはハードフォークで、モデル ID と固定 artifact root の組を承認リスト(ApprovedModels)に追加する。追加のみで、既存モデルは上書きしない(ADR-0175)。リストへの指定は政策的決定であり、「公開されている」「独立検証可能である」ことの技術的証明ではない。
  248 | 2. **後から登録したモデルは、登録した時点から審査なしで 0.3 に固定する。** 昇格・更新・失効・試行手数料はない。何年経っても、どれだけ使われても 0.3 のままである。**1.0 になる経路は、年次フォークでの指定だけである。**
  249 | 3. **モデル単位の検証・審査は行わない(不採用)。** 理由: モデルに対して検証を行うと、その検証において何を信頼するか(誰が・何を根拠に「正しく共有・運用されている」と判断するか)という問題が新たに発生し、それは解決しない。モデルの公開状況・保持者数・取得可能性・共有状況をチェーンが審査することはしない。
  250 | 4. **係数は配分の重みに一度だけ掛ける。** `A_m = c_m · S̄_m^1.5`(α=1.5、30 日平均の適格 bond)。この P_m を物理ブロック機会(RFC-0016 V+)と coinbase 配分の両方に使い、受理ブロックの報酬に重ねて掛けない。登録モデル全体に発行上限を設け、多数登録で 0.3 の枠を増やせない。
  251 | 5. **claim 単位の検証は、すべてのモデルで維持する。** モデル単位の検証を行わないことは、claim の検証を省くことではない。各 claim は階層に関係なく、確率的検出・公開証拠による局所化・決定論的裁定(court)・claim ごとの抽選検証者による認証(`SecurityCertified`、正直な割合の明示的前提)を受ける。認証されていない claim は、どの階層でも fork choice・決済深度・Rule E 参加・settled anchor・weight を進めない(§0 の 2026-10-15 改定、RFC-0018)。
```

<a id="p05"></a>

## P05 — Panel=0・VM・メディア等の旧案は現行オンチェーン方針への反例にしない

### `docs/rfc/0005-palw-ml-vm.md` L1–L15

原本SHA-256: `cc096f4afa7bc81205e50f8123553a81d1b4ece4ed7ea6084cf6118aaa41676f`

```text
    1 | # RFC-0005: PALW VM（不採用）
    2 | 
    3 | > **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。
    4 | 
    5 | > **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
    6 | > モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
    7 | > 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
    8 | > **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
    9 | > 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。
   10 | 
   11 | 
   12 | > **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。
   13 | 
   14 | PALW-BVM・PALW-GVM は実装しない。ISA、syscall、guest メモリ、gas、VM court、toolchain を合意規則として維持すると、モデル対応に必要な実装・監査・履歴再生の負担が増えるため。
   15 | モデル拡張には [versioned Kernel と宣言的 plan](../design/palw/models/versioned-kernels.md) を使い、未対応の意味論は審査済みの Kernel 更新で追加する。
```

### `docs/rfc/0015-panel-free-permissionless-verification.md` L934–L949

原本SHA-256: `ed63475109cd6edd721ea85ac96dcd9687d9108ea9253b8dc2201941ed1a1112`

```text
  934 | ## 改定 — 2026-10-12（Panel=0は当面有効化しない：提案）
  935 | 
  936 | **状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4と[RFC/ADRレビューの整理（2026-10-12）](../audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md) N5。
  937 | 
  938 | Panel=0は将来設計として保持するが、当面の実装・release対象から外す。§7のとおり監視市場を作るだけではhonest verifierの存在を保証できず、多数の監視者を登録させるだけでは誰かが必ず正直に検査することの証明にならない。モデル保有Panelを必要とするmode（RFC-0010 2026-10-12改定のPanel=1）を維持し、Panel=0は監視者の離脱、Panelの共謀、検証費用、最大claim負荷、cold model取得時間、不正発覚からcourtまでの時間を測る別の安全性実証の後に判断する。ADR-0176の共通bond予算はPanel=1にも適用し、RFC15の有効化を待たない。§8の共通経済部分にはモデル別claim bond台帳・配分係数（ADR-0177 2026-10-12改定）を置く。
  939 | 
  940 | ## 実装状況 — 2026-10-15（implemented, dormant, unverified）
  941 | 
  942 | **状態:** コードは実装済み・休眠・未検証。build、test、devnet、実node試験は本改定では実施していない（ユーザー規則: 先にコード、検証は後）。いずれもPASSと記載しない。
  943 | 
  944 | * Panel=0のclaim lifecycle（`Challengeable`、固定window、hard deadline、未解決disputeによるFinal阻止、DA/default、liability中のconviction）、mode付きclass identity、derived OPV eligibility、producer reservation、公開verifier（`kaspad/src/palw_kernel_verifier.rs`）、RPCのmode/assurance表示はkernel routeに実装済みである。V2 Attempt経路はPanel規則のまま（旧claimは旧規則）。
  945 | * §8.3のclass非依存Q/B/R/F capと共通DAA拘束`d + W`はADR-0176 budget engine（`palw_bond_budget_v1`）を再利用する。OPV claimは`Q = 1`、`R` = job escrow、`B = F = 0`を予約する。activation gate recordが存在するnetworkでは、budgetのないOPV claimを入口で拒否する。
  946 | * §13.3 `ACTIVATION_ALLOWED`をコード化した（`PalwPanelFreeGateRecordV1`）。12 conjunct（§13.3の10項目、PESG、N5解除のユーザー裁定）が全てPASSで、各々に固有のevidence/independent review digestがあり、genesis・height・terms・budget/allocation policyにbindし、kernel route・bond budget・model allocation・court model scope・rule Eのfenceが同時以前に有効で、かつdigestがbinaryにpinされている場合だけ、`palw_panel_free_v1`の検証が通る。pin一覧は空であり、operator flagや設定だけでは有効化できない。
  947 | * 同日第2段（lead default、ユーザーレビュー待ち）: gateはG14 public court（`palw_staged_public_court_v1`）と`palw_verifier_pay_v1`も要求し、TIR/Float32 staged routeはrecordにbindした報酬profile集合が含む場合だけ要求する。OPV policyを持つ全network（isolated calibration・試験networkを含む）でbudgetのないPanel-free claimを拒否する。kernel claimの`Final`を読む全reader（EXEC work-slice、claim read/RPC、web表示）は一つのsettlement述語（pending / final-revocable / settled / convicted / defaulted、fraud-expiryは`liability_until`）を使う。OPV networkではbondのduty gateがOPV予約・`d + W`・`H_L`の間担保を保持する。
  948 | * 最終段: RFC-0010 V3のbeacon（`ChainPanelBeaconHistoryV1`）が読むOPV Final factも同じsettlement述語から`claim_final`/`da_satisfied`を導く（GAP-B3、追加weight/rewardなし）。EXEC work sliceをOPV claimで検証する結合worldのnode試験sourceを追加した。RFC15のコード段階は完了（未build・未実行・未検証）。
  949 | * 新しいwire tag、object kind、aux table、RPC op、p2p version、funded actionは割り当てていない（`GetPalwKernelClaimResponse`内のproto field 35/36とBorsh wire v2を追加）。testnet-12のparams/schedule idは変わらない。isolated calibrationのparams identityは変わる。
```

### `docs/rfc/0022-post-mainnet-media-and-model-improvement-scope.md` L1–L28

原本SHA-256: `753f83e076fc1228ddf8b29ae0520c9a61457ff66842da20d3fd31e773df4717`

```text
    1 | # RFC-0022 — 初期 mainnet から画像・動画・モデル改善を外す適用範囲
    2 | 
    3 | - **Decision:** Accepted — 追加のユーザー指示の具体化。文書化日 2026-10-17。
    4 | - **Scope:** RFC-0003 の画像・動画経路と RFC-0004 のモデル改善の延期。
    5 | - **Implementation:** 文書方針。runtime の無効化は未確認。
    6 | - **Verification:** node の拒否・回帰試験は未実施。
    7 | - **Activation:** 当面無効、mainnet 開始で自動有効化しない。
    8 | - **Related:** [ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md)。
    9 | 
   10 | ## 1. 要求
   11 | 
   12 | 対象機能は mainnet 後へ延期し、初期 mainnet の必須実装・提供・有効化から除外する。詳細と境界は [正式方針](../governance/post-mainnet-scope.md) に集約する。旧予定表・実装完了記録は今回の有効化承認ではない。
   13 | 
   14 | ## 2. 共通経路との分離
   15 | 
   16 | RFC-0003 全体を無条件に停止せず、必要なテキスト・TIR・検証の共通部分を残す。共有 fence や別の CLI / RPC / admission / fold から画像・動画が開かないことを確認する。改善を止めても ADR-0175 の不変登録を止めない。
   17 | 
   18 | ## 3. 検査
   19 | 
   20 | 通常入口・別入口の対象 job 拒否、改善 epoch / 評価 / 候補選択 / 固有報酬の不進行、共通経路からの迂回不能、再起動・再同期の一貫性、テキスト・認証・予算の回帰を確認する。延期機能の肯定的 E2E 完成は初期 mainnet の条件から外すが、無効化と共通コードの安全性は免除しない。
   21 | 
   22 | ## 4. 再開
   23 | 
   24 | mainnet 後に対象 release と実装・測定・試験・レビュー・互換性・移行を明示し、機能ごとの提供・有効化承認を得る。自動有効化する日付は設定しない。技術構造を変える場合は [設計凍結規程](../governance/design-freeze.md) の手続きを要する。
   25 | 
   26 | ## 5. 非変更事項
   27 | 
   28 | 今回 node コード、定数、wire ID、fingerprint、既存 claim の責任を変更しない。音声・embedding 等の採用を新たに承認しない。以前の試験結果を現在の無効化証拠に流用しない。
```

<a id="p06"></a>

## P06 — pruning周期と証拠保持・bond解放を分離している

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L448–L484

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  448 | 
  449 | ```text
  450 | checkpoint_interval       = 10,000 DAA（作成間隔の提案）
  451 | pruning_point             = base consensusが安全と認める位置
  452 | court/material retention = 各objectの未解決責任と必要期間
  453 | bond exit                 = 判定・保持・決済の必要期間後
  454 | ```
  455 | 
  456 | checkpoint間隔10,000は、直近10,000だけ残して全削除する規則でも、全courtの証拠保持期限10,000でもない。10,000件混雑によってcourtが26,215 DAA以上かかる場合、必要証拠は複数checkpointをまたいで保持する。
  457 | 
  458 | ### 10.2 削除可能性の条件
  459 | 
  460 | objectごとの`retain_until`を、全参照元の要求から導出する。
  461 | 
  462 | ```text
  463 | retain_until(object) = max(
  464 |   unresolved_court_need_end,
  465 |   material_lease_end,
  466 |   settlement_or_liability_need_end,
  467 |   reorg_and_restart_need_end,
  468 |   validated_snapshot_recovery_need_end
  469 | )
  470 | ```
  471 | 
  472 | 各項は同じ絶対DAA時刻で表す。相対期間を異なる起点のままmaxで混ぜない。notice→court→settlement→recoveryと直列に必要な期間は、まず足して絶対期限に変換する。
  473 | 
  474 | 削除は、合意のsafe pruning pointより古く、未解決参照がなく、保持義務と再構築上の必要性が終わり、対応stateが検証可能なcheckpointに反映された場合だけ許可する。pruningされたデータを新しい有効courtが暗黙に読み直す構造を禁止する。
  475 | 
  476 | ### 10.3 データ階層
  477 | 
  478 | **Active state:** reservation、期限、bond exposure、未解決session、必要な部分witness、extension chainの再構築に必要な状態。全検証ノードが同じ意味で復元できる必要がある。
  479 | 
  480 | **Retained evidence:** 未解決または回復上必要な認証済みmaterial。ノード・custodianの責任区分を明示し、rootだけをもってbytesの代替にしない。
  481 | 
  482 | **Resolved history:** finality・reorg・責任期間を過ぎた詳細証拠。検証可能な結果commitmentと残存状態を保ち、履歴はpruneしてよい。archival nodeは監査・再取得の任意提供者であり、現に有効なcourt成立を善意archiveにだけ依存させない。
  483 | 
  484 | 全ノードが全モデル・全原本を永久保持する義務は設けない。ただし現在のconsensusが必要とするbytesを省けることは、具体的なstate/commitment/availability設計で示す。
```

### `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` L492–L518

原本SHA-256: `df986f6fc4fc70781f9fb71149a0ec638511d8f1d8682b5db3c2d59a716b99ad`

```text
  492 | ## 11. 検証可能なcheckpointとpruned IBD
  493 | 
  494 | ### 11.1 checkpointに必要な状態
  495 | 
  496 | 既存base-chain pruning proof/UTXO/state検証に加えて、court関連のprimary stateを含める。
  497 | 
  498 | ```text
  499 | ruleset / active capacity profile / resource template root
  500 | class/profile registryと必要な認証済み参照
  501 | 未確定claim、court/session、notice、next sequence
  502 | 処理予約カレンダー、残余予算、queue、phase cursor
  503 | base terms / effective terms / extension policy / 延長記録
  504 | bond exposure、exit hold、Final/credit/escrow hold
  505 | custody obligation、lease、retention reference
  506 | 未完fragmentの必要bytesまたは合意上十分な認証可能表現
  507 | 既に処理済みのkey/重複防止状態、settlement結果
  508 | ```
  509 | 
  510 | derivableなindexは再構築し、primary stateと食い違えばsnapshotを拒否する。partial close、予約、extensionを落とすとarchival nodeと異なる判定になるため禁止する。既存snapshotが未完close groupのbytesを保持する点を維持する。[V2-S06](0025-assets/source-audit-v2.md#v2-s06)
  511 | 
  512 | ### 11.2 rootの一致だけでは状態遷移の正しさの証明にならない
  513 | 
  514 | snapshot bytesが指定rootに一致することと、そのrootが正しいchain状態を表すことは別である。新ノードは、base consensusの安全性前提の下でcheckpointを認証する既存のpruning/chain-selection/状態検証経路を通す。
  515 | 
  516 | 管理者署名だけ、rootを載せただけ、checksumが一致しただけのsnapshotを「trustless IBD」と称さない。検証済みcheckpointからcatch-upして、同一tipのarchival nodeとstate root・court verdict・bond残高・deadlineが一致することを要求する。
  517 | 
  518 | このRFCは新しいSNARK/STARK等のvalidity proofを導入しない。全履歴再実行なしで何をどの前提で検証できるかは、既存pruning proofの信頼・攻撃モデルと合わせて明記する。
```

### `docs/rfc/0008-palw-claim-backed-consensus-blocks.md` L313–L316

原本SHA-256: `5e6df30068ead9de1eb85c4d3bb14d10f4df49314a105f443c4d68f188591b31`

```text
  313 | 7. **monetary material covenant（outer 240–242、aux 120、`palw_monetary_material_v1`）:** 全provider・sponsorが完全なroot/依存、capacity manifest、担保、自己資金fee、物理的支払window `W_mat`、payment-relative義務に署名する。liabilityまたはqueueが残る間、holdはMAXとする。materialの解放は**最後の実支払 + `W_mat`**以後であり、release blockが現在のselected chainでfinalizedであることを物理GCの条件とする（MUST）。sparse DAAやqueue滞留でproviderの新しい義務は期限切れにならない。欠落delta、古いtip、未完了import、generation変化、契約の不在はGCを許可しない。名前付きパラメータ`PalwMonetaryMaterialPolicyV1 {max_contracts, max_contracts_per_provider, max_bytes_per_contract, max_bytes_per_provider, minimum_collateral_sompi, minimum_fee_sompi, window_daa (= W_mat), response_window_daa, max_enrollment_daa, cost_report_root}`（不変条件: 全て非零、`max_contracts_per_provider <= max_contracts`、`max_bytes_per_provider >= max_bytes_per_contract`、`cost_report_root`は承認済みreview由来）: **value: USER DECISION REQUIRED**。固定前払feeを未知の支払・回復時間の安全な対価と表示しない（`PaymentAndRecoveryTimeUnknown`）。
  314 | 8. paid material: native paid契約の`material_until`は署名済みEXEC retention horizon以上でなければ、fee/R移転・root/work-use書込みの前に拒否する（§15.11）。
  315 | 9. Remaining（観察、未検証）: 実nodeのnative root Final後の公開告発と支払回復、承諾付きhorizon延長、pruned join、workerの最低対価・資本費用。
  316 | 10. **承諾付きhorizon延長（2026-10-15、休眠実装）:** `palw_horizon_extension_v1`（kernel inner 68、aux 126）。EXECに束縛されたkernel claimの生きている`liability_until`を、worker（claimのproducer）とsponsor（jobのposter）が同一termsへ各自署名して延長する。全員の署名が揃った時点で原子的に適用し、(a) 延長だけ（Final・court窓を短縮しない）、(b) claim固定のhard deadline（claim受理`d0`からの`maximum_claim_horizon_daa`、EXEC rootのnative claimにX3が適用される場合はその`dispute_hard_deadline`との`min`）を越えない、(c) 期限切れhorizonを再開しない、(d) 獲得済み現金を取消さない、(e) EXEC income holdを同じhorizonまで延長する、(f) transferred claimのproviderは自身の署名済みlease（tag 150）が新horizonを既に覆うことで同意する（leaseを書換えない）。名前付きパラメータ`PalwHorizonExtensionPolicyV1 {maximum_extension_daa, maximum_consent_window_daa, maximum_claim_horizon_daa, maximum_open_extensions, require_price_contract, review scopes}`: **value: USER DECISION REQUIRED**。
```

## 不存在確認の範囲

F10の「ない」は、このZIPのファイル一覧および展開物に対する確認に限る。別branch・GitHub全体の不存在を意味しない。再現結果は [checks.json](checks.json) の `capacity_module_paths_in_this_snapshot`。対応する別branchの記載はF10原文を参照。
