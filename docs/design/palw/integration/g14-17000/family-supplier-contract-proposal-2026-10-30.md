# Court family supplier 契約の具体案 — 2026-10-30

2026-10-30 採択追記: ユーザーは「新契約を採択して接続まで進める」と回答した。
正式仕様は [RFC-0042](../../../../rfc/0042-court-family-supplier-consent-and-native-reservation.md) と
[ADR-0210](../../../../adr/0210-court-family-supplier-consent-and-native-reservation.md)。
以下は採択前の具体案と不足の記録であり、現在の統合検証結果ではない。公開有効化は別である。

提案時の状態: **Draft / 未採択・未有効化**。実装案は
`consensus/core/src/palw_court_family_supplier_v1.rs`。
wire tag、consensus object、rooted installer、本番受付、金融効果は登録していない。
署名・builderの実装と試験コードが存在することを、統合試験合格や全Court分岐の完了とは扱わない。

[RFC-0035 §3–4](../../../../rfc/0035-court-original-funded-contract-and-role-consent.md)
は、新しい役割に本人の署名済みconsent、現在authority、受付・応答・保持期間、資本・報酬条件を要求する。
旧custody pledgeを新しい全分岐・将来の役割へ流用しない。同RFCの2026-10-22追記は、
7-family SourceMaterialOnly graphにsemantic・parameter等の全閉包が含まれないことを明記している。
既存の承認済み見積りを使うことと、新しい供給者の金融・default条件を採択することは別である。

## 署名する内容

`FamilySupplierTermsV1` は、version、network、ruleset、claim、source、canonical相対graph、
role、nativeな正確なtarget inventoryのcommitment、supplier、sponsor、材料byte上限、
受付開始・終了、応答終了、材料保持終了、fee/capital/retention/settlementの各profile、
fee額、collateral額、nonceを拘束する。supplierとsponsorが同じcanonical bytesへ、専用のML-DSA domainで各自署名する。
元claimのまだ未知の受理DAAを署名したことにはしない。

shape検査は有限な `accept_from <= accept_until < respond_until <= material_until < u64::MAX`、
非ゼロのscope/profile/inventory、正のbyte上限と担保を要求する。
verificationはnative導出したexpected termsとの完全一致、受付interval、双方署名を検査する。
expected DTOだけではnative導出、bondの現在authority、実原資、予約、履行を認定しない。

## 四つの役割とnative inventory

| role | 必要なscopeと導出元 | 現状の不足 |
| --- | --- | --- |
| `ParameterAndPrompt` | 登録program/layout、全数値rootのparameter union、prompt commitment・Flat/Merkle形・必要tile集合 | opening/promptのbyte需要は導出済み。供給者consent、全targetのservice packet、実完了との接続は未実装 |
| `SemanticTable` | 元claimのimmutable class semantics、EOG/rendering table root、vocabulary、必要entry/proof集合 | 既存semantic materialの認証を新しい永続intakeとsupplier dutyへ接続する必要がある |
| `TokenizerArtifact` | tokenizer root、認証済みEOF inventory、実raw byte数、全IDとkey proofの必要集合 | rooted EOFだけをraw材料の供給完了と扱わず、raw保持・転送・履行を接続する必要がある |
| `PrefixGeneratedSelectionAndStop` | 元Prefix job、境界state、生成ID、logits/selection、native stop grammarと必要semantic依存 | 境界stateの既存familyだけでは生成後のselection/stopを覆わない。全依存と終端を導出して予約する必要がある |

同一材料を共有する場合はtarget identityで重複を除く。原claim・inventory・role・generationが異なる契約へ
完了rowや署名を転用しない。旧source pledgeは元の範囲を維持する。

Parameter／Promptの現行実装は `InputMaterialDemandV1` を実受付budgetとpreclaim requirementsで共有する。
parameter openingの実widthとprompt tileのnative widthを使い、scratchも事前予約する。
これは需要の導出であり、将来の供給者やservice slotを予約済みにはしない。

## 採択前に決める金融・責任の対応

新しい単価・担保率・期間は本案で追加しない。次の対応を明示してからnative derivationを固定する。

| 決める対応 | 推奨する既存意味の再利用 | 未決の境界 |
| --- | --- | --- |
| 四roleの費用と予約資本 | 既存の承認済みfunded-original/service/material-intake profileを使い、native packet・byte数からchecked計算。実funded escrowと専用担保から一度だけ確保 | 既存profileで各roleの全packet/byte費用を導出できるか。未定義のroleを既存の固定source packet一個へ置換しない |
| 応答・保持期限 | 元claimのhard、既存reorg retention、依存材料のsigned covenantをnative導出し、supplierが明示同意した上限内だけ受理 | 四roleに必要な正確な依存閉包。既存署名の自動延長や無期限保持への変更は禁止 |
| 供給不足・default | 既存courtの供給記録と明示的inclusion前提を維持。未供給・未確定はno-slash hold/recovery。材料不開示を計算不正へ読み替えない | どの新role dutyと認証済み完了rowを対応させ、どの既存service-bond chargeへ帰属させるか |
| feeの保留・支払・返金 | 既存held/once/physical queueと原支払者への一度だけ返金を利用。対象義務の履行・defaultは同じ履歴で巻き戻す | RFC-0039 fee-only条件をこの新契約にも採るか。既存providerのH=`enrollment_until_daa`を別契約へ無断転用しない。timely/late default、未解決・未使用枠の扱いを明示する |

これらの対応を選ばずに、旧provider/signatureの意味を変更したり、未知の費用をゼロにして受付を通さない。
凍結設計の変更に該当する決定は、[設計凍結手続き](../../../../governance/design-freeze.md)
に従う新規RFC・ADRと明示採択が必要である。本メモはその採択を代行しない。

## 実行可能な完了条件

nativeな全target/依存/byte/費用導出 → 現在authorityと双方署名の照合 → 実escrow・担保・
全branch service/resource/material予約の一括install → 認証済みintake/終端と供給・defaultの対応 →
held/支払/refundの一回精算 → restart/reorg/pruning/IBDで同じ義務と材料を復元、までを接続する。

誤署名・別claim/inventory・不足資金・短いhorizon・容量不足では全installを戻し、
正当な全roleの履行、timely/late default、混雑後の支払、重複consumer、rollbackを同一候補で検査する。
その証拠が揃うまで既存aggregate readinessの不足・Prefix未対応の拒否は維持する。
