# 採択済みwide Q24とA0の統合境界

2026-11-01。[RFC-0043](../../../rfc/0043-wide-q24-llm-output.md) / [ADR-0211](../../../adr/0211-wide-q24-llm-output.md)は、ユーザーが2026-10-31に両案を明示採択した既存の決定である。新しい設計採択や公開網有効化ではない。旧i32の値・class・scheme・wireを維持し、i64 Q24は別profileとして実装する。

## 今回取り込んだ資料

元checkout `/Users/wata/Downloads/MISAKA-testnet` の採択済み本文、旧提案URLの参照stub、[学習済みLLM登録実験](../../../evidence/models/trained-llm-cohort-2026-10-31.md)とそのraw evidenceを保存した。[取り込みmanifest](../../../evidence/positive-court-a0-2026-10-31/integration/wide-design-and-external-evidence-import-20261101.json)に元ファイルのSHA-256とサイズを記録し、既存evidence manifestの全73件を再照合した。原ログ・失敗run・digest・実測値は変更していない。採択本文には、実験が元checkoutでの実績であることを示すA0注記だけを追加した。

元実験はSmolLM2-360M/135Mの登録context8192、Pythia-70Mのcontext2048を維持している。Pythiaは別のi64 Q24 profileで再試験している。この資料の保有だけで、A0のPositive Court実装や最大context全位置の実行・費用が検証済みとはならない。実験側のRust sourceと保持binaryの完全な対応も、evidenceのfile hash一致だけでは証明されない。

## A0に残る実装

- `50339b43a`でadopted schemeと最終post logitsだけのi64 commitを識別するstructural foundationを統合。独立static review後、新7件と既存golden3件・golden_v2の11件、対象Clippyが成功した。[source/原log](../../../evidence/positive-court-a0-2026-10-31/integration/wide-structural-and-goldens-20261101.json)、[レビュー/Clippy](../../../evidence/positive-court-a0-2026-10-31/integration/wide-structural-review-and-clippy-20261101.json)。native admissionの既存scheme whitelistは変更していない。
- 別domainのデータroot/openingとi128 Q48の選択primitiveを`82146b3b9 / 74aac75f8`へ統合した。profile/旧入口11件・source料金2件・wide data/selection5件が同sourceで成功し、Core libraryのstrict Clippyも等価cleanup後に成功。[stage2記録](../../../evidence/positive-court-a0-2026-10-31/integration/profile-wide-stage2-review-20261101.json)。8-byte step leaf、実行・capture・cell・Court・資料費用の全経路接続はまだ必要。
- `f862774d3`で登録program由来の明示profile resolverを追加。旧factsの74-byte保存形式を維持し、旧class record/StepSpace/binding入口は未対応wideを拒否する。canonical tokenizer/CUのゼロ、元の最大context、anchor由来prompt/seed/full jobを維持する。factsだけを読むnative callerへの接続は別途必要。
- A1の全範囲検査とA3の一回のPanel再計算・証拠生成への接続。現FullRelation経路の最終logitsはi32限定で、i64をcastして受理しない。
- 同一候補での実checkpoint、元の最大文脈長、role別費用、cold/warm、複数Node・再起動の確認。

未接続consumerは拒否を維持する。元checkoutの広い未コミット差分、公開preset、既存の生成物pinや他作業の変更を一括コピーしない。実native job/role/資金/P/全consumer接続も別に必要であり、構造認識や登録成功をclaimの信用認証へ読み替えない。
