# 全機能 DAA 17,400 有効化の readiness — 2026-10-28

**公開候補と開発中コードの区別:** この調査は全機能実装の作業記録であり、公開した G14-only バイナリ `655b6cf3a` の機能一覧ではない。RFC-0039 の通常 verifier fee と §6 provider fee-only 条件、RFC-0040 の prefix 参照はユーザー採択済みで、実装・統合検証は別ブランチで進行中。provider 役務を有限化する A/B 案は採択していない。以下の初回調査・案にある未決表記は履歴として読む。最新の判断は [ADR-0207](../../../../adr/0207-finite-fee-consent-and-provider-payment-bounds.md) と [ADR-0208](../../../../adr/0208-authenticated-prefix-work-dependencies.md) に記録する。

**最新の完了範囲（2026-10-28、ユーザー指示）:** 全項目の実装完了まで。DAA 17,400 は fence 方針として維持する。公開配備と長時間の証拠キャンペーンは今回の完了条件から外す。実行可能な全経路の実装、整合したビルド、必要な focused tests は引き続き必須であり、未実行の検証を成功と扱わない。subset のみの実装や拒否だけを全項目完了の代用にしない。以下の旧配備計画・実行記録は履歴として保持する。

状態: **未完了・公開保留**。ユーザーは従来の G14 一式から、従来未有効だった機能も含む全体 / DAA **17,400** へ要求を変更した。一時指定された17,100は取り消され、全実装の完了後に17,400で有効化する目標になった。この記録は範囲を G14 だけへ戻さない。17,400 の既存バイナリ・試験記録を、全機能 17,400 の完了証拠として扱わない。

本記録は現状の阻害要因と実装・検証計画であり、設計採択、新しい waiver、実測記録、配備完了の宣言ではない。既存の恒久原則と凍結設計は維持する。公開 fleet の最終状態は [配備継続記録](release-17400-2026-10-28.md) に別途記す。

## 範囲と現在の証拠

`consensus/core/src/config/params/full_activation_v1.rs` には G14 の集合と FULL_ACTIVATION の集合が別に存在する。従来の G14 集合は Panel=1、6種類の legacy/FP route、PanelDa disclosure、Gate C と attestor liability、モデル二階層を含む。FULL 集合にも入っていない休眠 fence があるため、FULL の定数だけを変更しても「従来未有効だった全機能」の確認にはならない。

必要な最初の成果物は、実際に組み上げた testnet-12 preset の全 fence について、現行値、17,400 の予定値、依存、機能・wire の実装状態、要求証拠と阻害要因を対応させた一覧である。すでに有効な fence を 17,400 に移動して過去の規則を変更してはならない。恒久的に不採用のモデル単位審査や、mainnet 後へ延期した機能を通常の休眠機能と同一視しない。これは既存原則との整合を確認すべき点であり、今回の目標達成を装って除外するものではない。

従来候補 `655b6cf3a8f61ebdce920af867f6bedd87e717d6` の Linux ビルドと probe、証拠元 `da46131a41fe7c90ec263b0f26ab9b444e4c74a2` の19 suite、19件の re-arm 検証は、記録した範囲について有効である。新しい全体 scope と高さの source・policy・identity へ自動的に付け替えない。

## SettlementOnce: 古い説明と実際の未実装を分ける

`palw_rfc0608_measured_v1.rs` の open blocker は現在も validation を拒否する。その説明には修正済みの項目も残っている。

- native admission は `PalwAdmissionLiabilityV1::NativeClaim { hard }` と署名済み期限 + court window を使う実装がある。
- native39 mining receipt は `native_mining_receipt_leg_v39` で issuance と weight の `NoLiableClaim` と明示される。requester fee の責任条件を勝手に当てはめない既存の判断とテストがある。
- audit1004 有効下では trap 前支払の経路と有効後の経路を区別する必要がある。古い説明の全項目をそのまま新しい阻害要因と数えない。

一方、次の4系統は実コードに未解決の条件が残る。

| 支払系統 | 現在の拒否・不足 | 実装前に確定し、認証して保持する必要のある項目 |
| --- | --- | --- |
| paid V2 enrollment 外の kernel claim | `palw_settlement_liability_fold_v1.rs` の kernel leg / admission が producer-signed paid lifetime を取得できない | 通常 kernel claim の producer が同意する最大責任期限、材料保持期限、予約・退役との対応。既存 paid V2 の値を無署名の通常 claim に流用しない。採択済み形式に項目が無ければ、版を区別した署名対象・decode・admission・保存・replay が必要 |
| paid trap 外の verifier check fee | `palw_kernel_route_fold_v1.rs` の通常支払が `Unsourced`。draw の締切は verifier の同意した責任期限ではない | verifier fee の担保または保留額、本人が同意する責任範囲と終期、告発・没収・解除の条件。paid V2 held check fee の既存実装が対象範囲を覆うか確認し、対象外への無条件拡張をしない |
| material provider fee | `palw_monetary_material_fold_v1.rs` の event-relative covenant に有限の horizon / hard deadline がなく `Unsourced` | provider の署名済み責任上限と継続役務との関係。既存 `window_daa` と `enrollment_until_daa` は存在するが、役務終了条件そのものを有限化していない。新 version が必要なら旧 covenant の意味を保存し、最大 enrollment + window、延長・終了・refund の規則を明示する |
| beacon source fee | `palw_kernel_route_fold_v1.rs` が claim reservation、X3、material TTL のない source の支払を `Unsourced` として sponsor に返す | sponsor-signed `BeaconSourceFeeTermsV1` の `hold_until`、`final_by`、`deposit` と支払 obligation の対応。claim ではないため適用しない項目があるなら、TTL / retirement 等をゼロや無限大で埋めず、適用範囲を版・型として明示した承認済み規則が必要 |

上表は不足する入力・意味の特定であり、新しい経済規則の採択ではない。[既存調査 §4.6](../rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md) も後続の版・責任条件を未実装としている。既存設計に未指定の意味を加える場合は、通常の実装修正と区別して正式な設計手続きを通す。

調査時点で、登録済み worktree にある `palw_rfc0608_measured_v1.rs` は63コピー・2種類で、差は整形のみだった。未取り込みの完成済み blocker 解消版は見つからなかった。これは全 Git object を検証済みという意味ではない。

必要な試験は、各正当な支払の成功に加え、署名・期限・予約欠落の拒否、重複 consumer の一回精算、保留中の没収、refund、restart / reorg / IBD の会計一致、fence 前互換である。未解決箇所を実装してから blocker 記述を実態に合わせる。blocker 定数の削除だけでは実装にならない。

## RFC06–08 の測定記録

`PALW_RFC0608_MEASURED_RECORDS_V1` は空である。対象は monetary material、material recovery V2、paid claim challenge、beacon source opt-in / fee、memory routing court V2、settlement once。open blocker 解消と別に、network domain と正確な policy root に対応した harness、source、hardware、network conditions、raw data、導出、独立 review の記録を要求する。evidence / review digest は別の非ゼロ値で、実在する成果物へ結び付ける。過去の G14 waiver や値の見積りを新 scope の測定済み記録へ書き換えない。

## RFC-0023 stage 2: 実装と測定の両方が残る

`palw_claim_local_v1.rs::validate_palw_claim_local_settlement_v1` は、runtime integration 未完と material retention / transition drills 未実測のため、`Some` を無条件に拒否する。単に測定値を足せば済む gate ではない。

checkpoint 型、namespace 付き依存 identity、一部 reward reader、frontier 接続、構築前の resource reservation は存在する。一方、[RFC-0023 の実装記録](../../../../rfc/0023-unresolved-claim-liability-and-independent-progress.md) の最新追記も、全 native input の provenance / material archive、pruning / IBD / restart / reorg 後の外部反証、同一 artifact の複数 node campaign を未完としている。

実装確認・補完の単位は、(1) 全 claim / native namespace の入力 birth と実依存閉包、(2) credit / reward の一度だけの解除・取消、(3) 取得できる実材料と署名済み covenant の保持・転送・GC、(4) checkpoint、fork-choice frontier、transaction settled、native lifecycle と pruning / IBD の一貫した接続である。部分的に接続済みの箇所を再実装する前に経路ごとの coverage を照合する。

その後、未解決 A と独立 B、A に依存する C を同じ候補の複数 node で実行し、B の Final・認証・保留解除・支払・settled が独立に進むこと、A の後日の有罪が A/C にだけ作用すること、保持と転送後に外部検証者1人が反証を完了できること、全 node の checkpoint / unresolved set / settlement 一致を検証する。保持 byte 数、CPU/RAM、transition と復元費用は実測値として別途記録する。既存 unit 合格はこの受入試験の代替ではない。

## 公開までの順序

1. 全 fence の現状一覧と依存・実装・証拠の対応を確定する。
2. 上記の runtime / authenticated-term 不足を解消し、必要な設計・値の判断を記録する。
3. 同一候補で対象試験と node campaign・測定・独立 review を完了する。
4. 17,400 の安全な事前配備が可能か最新 DAA を確認し、正確な source / height / policy の readiness を生成する。到達済みの高さを新しい未来の fence として扱わない。
5. 新 identity と hash を実バイナリで probe し、stage / dry-run / rolling upgrade / DNS join / fleet 収束を確認して公開する。

現時点で2–5は完了していない。実装上の拒否を維持したまま事実を記録し、全機能を有効化済みと表示しない。

## 初回調査時の追加実装案: fee の署名済み有限契約

この節はレビュー用の具体案であり、経済規則を変更済みではない。kernel の通常 claim については既採択 ADR-0198 の X3 を hard deadline の出所として実装中であり、paid V2 の明示 lifetime を置換しない。材料 covenant の存在・健全性を別に要求する。

### 通常 verifier fee

新しい consent の署名対象を `version / network / ruleset / claim / verifier bond / draw-slot / fee / liability_until / material_until / forfeiture policy digest / nonce` とする。claim と draw slot を固定し、別 claim・別席への署名再利用を拒否する。`liability_until` は元 claim の受理に束縛された X3 hard を覆い、`material_until` は元 claim の実 covenant を超えない。値は後から now を基準に延長しない。fee は署名済みの保留契約を受理した時に poster escrow から専用 row へ移す。

推奨する支払形は責任期限後の全額解除で、期限内に Clean attestation の対象が有罪になれば保留 fee を没収する。既存 bond の署名者責任とは別会計にし、同じ金額を二重に没収しない。**追加の判断が必要:** 保留 fee の没収率と対象 verdict。paid-V2 trap の現在の probe 値を通常 verifier の承認済み政策と扱わない。これを採る場合は、hold/release/forfeit、reorg、cold restore、CLI/SDK の明示署名を新しい版で実装する。

### Material provider fee

現行 V1 は「権利が全て終了し、最後の実支払 + W とその finality まで保持する」という署名済みの無期限になり得る契約である。`enrollment_until_daa + W` をそのまま期限にして既存署名の意味を変えてはならない。

有限 V2 の署名対象は現行 enrollment 全体に加え、`service_until / last_admissible_payment_daa / material_until / late-payment disposition / fee release policy digest` を含める。`service_until` は accepted consumer の既存 service/backstop を覆い、`material_until >= last_admissible_payment_daa + W` は checked arithmetic とする。契約の hash に provider と sponsor の両署名を要求し、原 claim の依存集合・材料 bytes 上限・全 provider offer を引き続き拘束する。新しい消費者を追加する場合は、その hard/material 期限が既存署名済み上限内かを受入時に検査する。

**追加の判断が必要:** 実支払 queue に許す最大遅延 Q と、Q を越えた未払い権利の扱い。推奨案は、受入時に有限支払枠を予約し、払えない権利は確定した期限で元の資金提供者へ一度だけ返す形である。ただし、finality 不成立時のサービス停止・材料破棄・refund が安全かを別途設計する必要がある。署名済み材料期限を越える pending payment を残したまま「完了」にしない。V1 契約は旧 cohort として元の条件を保持し、V2 に自動変換しない。

### Beacon source fee

現行 producer opt-in と sponsor funding はすでに source の有限 `hold_until`、`final_by`、`deposit`、context を署名で拘束する。新しい数値を選ぶ前に、それらを fee obligation に結び付ける typed adapter を検証する。正当な fee は standing Final かつ `now > hold_until` でのみ支払う。source claim の実 covenant・保持 row・deposit の物理予約・正確な未払い promise を検査し、欠落は拒否する。claim の TTL/retirement を一律「適用外」にして gate を通さない。旧 block の意味を変えず、settlement fence 以降の新 cohort に限定する。
