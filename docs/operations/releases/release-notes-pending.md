# 次回リリースのリリースノート(作成中)

文書の適用版・対象・採用状態を確認してください。配置は採用・有効化ではありません。[releases の入口](README.md)。

この文書は、まだ出荷していない変更のうち、稼働中の node の動作を変えるもの(node policy)と、休眠 fence の中の変更を分けて記録する。fingerprint・identity は、下のどの項目でも変わらない(`testnet12_identity_did_not_move` pass、2026-10-18)。

## node policy(fence なし、consensus への影響なし)

### Panel の own receipt の class lane(RFC-0018 (iv)、2026-10-18、ユーザー了承)

- **何が変わるか。** seat 自身の supplementary receipt(ADR-0124 D2)を、main の carrier slot が取れないとき、その class の lane で運ぶ。lane は seat の script の下にある、別の確認済み output を使い、1 本ずつ in flight にする。1 seat あたり最大 16 本(`PALW_RECEIPT_LANES_MAX_V1`)。lane を作れる空き output がなければ、従来どおり main slot を待つ。
- **protocol duty の既定。** 設定項目や起動 flag はない。V07 の readiness lane と同じく、空き output がある seat では常に動く(プロトコル義務は起動オプションにしない)。
- **consensus への影響なし。** 運ぶ object(`palw_v2_supplementary_receipt_assemble` が作る `ReceiptLicensed` の supplementary)とその検証は変わらない。変わるのは、どの funding output がその carrier を払うかだけである。他の chain の carrier の子を作らない(確認済み output だけを使う)ので、relay の性質も変わらない。readiness lane と receipt lane は、互いの output を選ばない。
- **理由。** 出荷の scheduler の模擬で、court の queue が空にならない間、OwnReceipts の site に main slot が回らず、own receipt が class 数に関係なく窓(600 DAA)を外した。lane を置くと、16 class まで外さない(G14 完全性証明 §13.10)。
- **運用。** lane を使うには、seat の pay address に空き output を追加する。output が足りないと、lane は作られない(`unfunded_at` で 1 DAA に 1 回だけ探す)。

## 休眠 fence の中の変更(出荷 preset では動かない)

- RFC-0018 (ii):sibling 順の class rank(`palw_dag_sibling_attribution_v1`)。
- v39 の Master・mining の料金を、action が実際に運ぶ bytes で計る(`palw_native_execution_v39` / `palw_native_mining_v39`、ユーザー決定 10-18、ADR-0187)。
- RFC-0006 06-A:held-GDN filer の service tick(`palw_memory_routing_court_v2` 未満と challenge しない node では何もしない)。
