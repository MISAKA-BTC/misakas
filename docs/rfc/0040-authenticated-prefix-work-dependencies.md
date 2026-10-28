# RFC-0040 — 再利用 prefix の生成元を署名済み work identity で拘束する

Status: **Adopted / 実装中・未検証・未有効化** (2026-10-28、ユーザー「採択して実装・検証を進める」)。[ADR-0208](../adr/0208-authenticated-prefix-work-dependencies.md)。
[設計凍結手続き](../governance/design-freeze.md)に従う採択済み設計である。有効化は実装・証拠実行・review の完了後に行う。

## 問題と変更が必要な理由

[RFC-0023](0023-unresolved-claim-liability-and-independent-progress.md) は、別 claim の出力を入力にする claim の決済が生成元の責任を継承することを要求する。
現在の `PalwFpPrefixStateV1` の署名対象には `state_root`、`prefix_tokens`、`class_id` があるが、生成元 claim の identity と受理点がない。
同じ root の検索、モデルの一致、producer bond の一致は実際の依存の証拠にならない。現在の stage 2 の reward reader は非 genesis prefix を unknown として保留している。
生成元不明を依存なしと見なす変更は行わない。

## 提案する wire と受理規則

既存 payload、ID、署名の意味は変更しない。新しい version の input envelope と、生成元による prefix export statement を追加する。具体的な tag/magic の割当ては採択後に registry の衝突検査を伴って行う。

- **受理前の source export intent** は network domain、生成元 work namespace、source nonce、export を含める前の versioned base claim commitment、class、prefix token 数、state commitment、export ordinal、署名済みの最大責任・材料保持期限を含む。生成元 producer が署名する。base commitment の hash 対象から export/envelope/signature を除くことを wire で明記し、自己参照を作らない。まだ決まっていない受理 block / DAA / blue score は、この署名に含めない。
- **受理後の export identity** は、consensus が上記 intent を実際の生成元 claim ID と受理 block / DAA / blue score に結び付けて記録する。consumer はこの受理済み identity を参照する。同じ raw claim ID の再受理も、受理点が違えば別の source である。producer の申告した受理点を authority にしない。
- export statement の commitment は生成元 claim の受理時に拘束し、後から都合のよい export を追加できなくする。生成元の検査・court は、この prefix が生成元の計算から導かれるかを再実行で検査する。署名や commitment は計算の正しさを証明しない。
- 消費側 envelope は、消費 claim の versioned base commitment（自身の dependency envelope と signature を hash 対象から除き、ここにも自己参照を作らない）と、参照する export identity、完全な prefix descriptor を署名で一緒に拘束する。consensus は受理済み export の descriptor と完全一致すること、および生成元が消費側より前に受理されていることを検査する。
- fold は受理済み行から dependency edge を作る。producer が自由に指定した dependency の一覧を authority としない。存在しない生成元、foreign network、descriptor 不一致、未来の入力、cycle、retired identity のすり替えは受理しない。
- 消費側の hard deadline と、その資料保持 tail を含む必要期間は、生成元および推移的な全 source の実際の署名済み責任・材料期限と予約上限内に収める。新しい消費者の受理で source の期限を延長しない。足りなければその依存を使う受理を拒否する。退役済み source の資料を「まだありそう」と推定しない。
- 独立な genesis prefix は従来どおり入力 edge を持たない。新 fence 後に非 genesis prefix を再利用する新 claim は上記 reference が必要になる。外部で生成し source claim を持たない prefix を、依存なしの credit に昇格する経路は本提案には含めない。

## 旧 cohort と資料保持

fence は消費 claim の受理点で適用する。fence 前の受理済み claim の wire、ID、支払い条件を変更しない。旧 export が存在しない claim を、新ルールの source として後付け認定しない。

生成元が退役しても、その export と消費側の受理事実、元の検査 policy、必要な retained material は消費側の責任閉鎖まで checkpoint の資料集合に残す。pruning/IBD は digest だけを渡して済ませない。資料の上限、保管費、期限は既存の有限な signed retention policy と整合するものを採用し、無限保管を暗黙の前提にしない。上限を満たせない受理は拒否する。

生成元が有罪になれば、その source を実際に参照した claim の credit・未払い reward を拒否する。消費側への fraud slash を依存だけで自動追加しない。独立した claim は影響させない。

## 検証・有効化条件

1. wire/identity の旧 cohort 同一性、foreign source、再受理 ID、署名差替え、descriptor mismatch、cycle の拒否。
2. 生成元 A、A を使用する C、独立 B の実 fold による受理・Final・Gate C・支払い・決済の分離。
3. A の退役後も C の依存が消えず、A の有罪化で C の credit だけが拒否され、B は継続すること。
4. 同一候補 artifact の複数 node で pruning/IBD/restart/reorg 後に資料を取得し、外部の正直な告発者が反証を完了すること。
5. byte/resource 上限と実測、独立レビュー、正確な network/policy/height に対する record を pin してから arm する。

## 判断する点

この新しい署名済み export/reference を採用し、fence 後の非 genesis prefix を使う新 claim に要求するか。これは過去の wire の再解釈や検査の省略ではなく、新しい受理条件である。
