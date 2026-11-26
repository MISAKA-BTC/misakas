# 資本配分と仕事量

資本配分の上限は[RFC-0012](../../rfc/0012-bonds-budgets-and-slashing.md)、全モデル共通会計は[RFC-0016](../../rfc/0016-position-market-and-rewards.md)、仕事量会計の提案は[RFC-0013](../../rfc/0013-model-work-accounting.md)を参照する。

係数は全モデル1.0。共通の平均bondに対する3/2乗は機会・影響のcapとして扱い、仕事量へ重ねて報酬倍率にしない。平均期間、seasoning、退出責任、全体発行上限、個別予算を対象profileへ固定する。origin別0.3や登録モデル専用上限は新MOE3へ適用しない。

MWA1を採用する場合は、modelの有限graph・shape・実処理位置から共通meterでWを計数する。基準機材の秒数、モデル名、任意のBFT倍率でWを置換しない。正しい高速化は許容し、異なるモデルの一回のpromptを一律同額とはしない。

仕事比例の当選時原資とEXEC量は提案状態であり、文書再編で有効化しない。各sourceの予約・消費・未発行終端を一回会計する。
