# A1 FP known-wide followup source review

固定commit `cd1567a1162e5b06de351abda506d6a4d6e7009d`、親 `0a5d835aa653f3cf4d0cc904bfccd0b3842c01dd`、Root cherry-pick対象 `babfda57e74b62ed8bb83cfbbac9a535107fd25e`。独立 read-only 静的レビュー。**追加 blockerなし、Rootへの選択取り込み後の実試験に進める。runtime PASSではない。**

変更5file。productionはordinary FPに8行、evalに借用化+guard+clone遅延の7行。新unit testは2関数。残りはhandoff/evidence。owner manifestの3 source SHAを照合し、Root babfdaの4既存fileが親commitとbyte一致、新JSONが未存在であることを確認した。git/index編集・apply・Cargoは行わない。

## 通常 FP (`eval=None`)

既存duplicate/bond identity/status/floor/frozen checks → N4 → G03 frame → container → G14 → class存在/Frozen → 新known-wide拒否 → class verify/admission/seating/share/work pricing → claim/exposure/prefix writes。

新guardはderived-work/original-frame有効性に依存しない。rowのcanonical program末尾64Bに既知wide schemeがある時だけdenyする既存allocation-free helperで、parser/identity認証/legacy許可を新規生成しない。class不存在/Frozenなどguard前の旧エラーは保持。guard後のwide非対応エラーが先になることは意図した拒否変更。narrow/rowlessは旧処理へ続く。

source fixtureは同じobjectからTIR rowだけを外したoff/off対照が実claimを作ることを要求し、derived off/on × original off/onの4組を検査する。ただしoriginal=true・未ペアでは既存G03が先に拒否するため、2組は新guard到達の証拠ではないことを正しく明記している。original=falseの2組はexact wide error、root/exposure/claim不変とfull transition拒否を要求する。

## eval (`eval=Some`)

通常guard前の独立分岐。dormant/epoch/item/subject/policy/roots/位置budget等 → TIR row存在 → layout digest → class存在/Frozen → lifecycle → 新known-wide拒否 → row clone → eligibility/class budgets → program decode/旧GEN context/count → job/claim/reservation writes。旧disabled/layout/Frozen/lifecycle順を保ち、row clone・context/count導出・claim/job/exposure書込みより前に止める。guard前にpolicy/prompt等の既存一時Vec処理はあるので、全allocation前の拒否とは主張しない。

source fixtureは既存narrow evalがclaimを作る対照を要求する。元final commitをuncommitしてwide Castを1個commitし、commit数/layoutを保つ。decode_canonicalと旧GEN context/leaf count一致のassertがあり、単なるgeometry countがwidth対応を保証しない条件を作る。class keyやrowは合成テストmetadataで、native登録/署名検証の証拠ではない。wide拒否時にroot/exposure/claim/job不変、dormant時に旧first error全文を要求する。layout/Frozen/lifecycle個別negativeの新runtime fixtureは無いが、順序はsourceで確認できる。

## write / rollback / scope

ordinary/evalの新guardは自身のclaim/exposure/prefix/job writerより前。apply_objectの既存court-end checkpoint/restore、paired-original checkpoint/restore、common preconditionsは変更なし。共通budget/work予約が失敗で消費されないという一般保証へ拡張しない。テストが全journal/キャリア/外部resource ledgerを比較したとも扱わない。

明示full-layout wide SourceChecker・native登録schema/fence/Params・RFC4のdormant/mainnet延期規則・旧GEN context/leaf numeric計算は変更なし。この2本の実経路のseam修正であり、repository全consumer closureの証明ではない。新native authority/credit/モデル審査/tier変更は無い。

次回同source Core unitで `ordinary_wide_free_prompt_refuses_even_with_derived_and_original_fences_off` と `legacy_eval_denies_wide_before_row_clone_and_keeps_dormant_refusal_first`、既存FP/eval回帰を実行する必要がある。owner executionはNOT_RUN/HOLD。reviewerも型検査・build・testを実行していない。
