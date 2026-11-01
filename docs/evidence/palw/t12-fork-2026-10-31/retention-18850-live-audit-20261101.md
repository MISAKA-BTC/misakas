# 未配備の DAA 18,850 規則と公開履歴の衝突監査

2026-11-01 22:31:36 JST、95.111.236.186 の既存 loopback JSON wRPC を読み取った。
本監査ではソース・node・launcher・dropin・DB・peer 設定を変更していない。

## 観測した履歴

virtual DAA は18,930、sink は `5219e8ec3faf8ecd7e17e308d4a7d198f2cd57ffe247c019838b621b48f94cacc86f239d1c90dcb9b084a4b53d6272aba803ba1ba432ca9f12ffbe1e56edf667`。
sink 自身の header DAA は18,929。`getInfo.isSynced=false` のため、この高さをネットワーク全体の先端とは扱わない。

selected-parent を18,849まで160ブロックさかのぼり、そこから `getBlocks` 2ページで
merge集合とsink anticoneも取得した。重複排除した232ブロックはすべて本文あり。
203件はalgo-8 heartbeat、29件はalgo-6のPAV2/PAV3 Attemptだった。
Attemptはすべて既存Qwen class `ebf44d0a…2246d01`、BASEはこの範囲では0件。
26件が選択チェーンのblue merge、2件がred merge、1件が現在sink自身である。

全Attemptのraw signed retentionは、自分のheader DAA +20,608だった。
最初の例はheader DAA18,848の `1968bd823…553ac`。
選択チェーン上の `44714a618…38eaa`（DAA18,851）のblue mergeに入り、
raw期限39,456から受理側DAAを引くと20,605で、20,608より3短い。
次の `f678bf353…0d956` はheader18,851、blue merge先18,853、期限39,459で2短い。
対応する全hash・class・claim ID・merge先・期限差は[監査JSON](retention-18850-live-audit.json)に保存した。

## 新旧規則の差と限界

[未配備候補の記録](../model-panel-one-2026-10-31/retention-18850.md)の規則は、
受理DAA18,850以後にstored obligationを `max(raw, accepted DAA + canonical span)` とする。
coreの `palw_state_v2::apply_attempt` は `ctx.daa_score` を保持義務計算とcourt admissionに渡し、
merged Attemptも受理ブロックの同じctxで処理する。
したがって本履歴には、過去の保持期間判定・stored obligationが変わり得る実入力が存在する。
「18,850を越えたが影響するAttemptがない」という前提で配備してはならない。
境界を独断で変更したり、既存DBの受理状態を手書きしたりしていない。

署名の対象であるunsigned Borsh bytesの長さとbytesを、既存attempt-id domainをkeyとした
BLAKE2b-512でハッシュし、claim IDを算出した（計算の正しさの証明ではなくidentity）。
この算出は既存の481352実ブロックfixtureの期待ID `aafbaf3e…883cc` と完全一致した。
95のgeneric claim RPCで、sinkとmerged `1968bd`・`f678bf`・`eb2e7a` の4件はすべて `found=false`。
[生応答](retention-18850-live-claims.json)。

**新規則による全履歴の再実行はしていない。** 保持期間の比較は変わるが、他のadmission gate、
mergeset non-DAA判定、予約・経済状態まで含めて全claimが新たに通るとは断言しない。
`found=false`だけから特定の拒否理由を断定もしない。ライブ生成・単独承認・Finalの成功証跡でもない。

## 外部peer

95のpeerInfoはprotocol108とuser agentを返すが、params/schedule IDを返さない。
これだけで未配備候補BBB系への更新を判定することはできない。
直近の95拒否ログでは、外部旧peerが18640を持たず `next=MAX` を広告する例を確認した。
これは18640境界での不一致を示すが、全外部peerの状態や最初の実分岐点は証明しない。

依頼された広告IP `217.76.57.217` / `85.131.213.182` に対しては、公開wRPC候補26314だけを
5秒timeoutで照会した。22:34:36 JST、両方とも接続timeoutでInfo・DAA・paramsは取得できなかった。
[接続結果](retention-18850-public-rpc.json)。他portの走査、新規node起動、接続設定の変更はしていない。
RPC未取得をnode停止・特定版・分岐の証拠にはしない。

## 再現資料

- [read-only走査script](retention-18850-readonly.py)。既存nodeのloopback26314でstdin実行する。
  getBlock/getBlocks/getConnectedPeerInfoのみ。request/page/selected-walkに上限がある。
- [取得metadataと公開commitmentのgzip](retention-18850-live-headers.json.gz)。展開JSON SHA-256:
  `def720ed4c9aca8685d8444ea60103d924c2cba3b3412080de8aadd29033e13a`。
- audit scriptのclaim decoderは既知fixtureと、保存29Attemptのclaim ID・class・retentionを照合した。

運用所有者へ影響候補を連絡し、未配備18,850候補を現チェーンへそのまま配備する案は中止する旨を受領した。
今後の境界・配布・接続・安全な復旧は、その担当が外部チェーンも確認して進める。

## 検査

既知実ブロックfixtureと保存29件のidentity/class/deadline照合 PASS。
ドキュメントの no-new-regressions は PASS（既知43件、新規0件）、RFC-v2 integrity も PASS。
ADR-v2 integrity は既存の別作業RFC0044/ADR0212の未登録・inventory/count整合エラー5件でFAIL。
当監査でその文書やbaselineは変更していない。

13:31:36 UTCの95には `121.81.248.189:45472` の接続済みinboundもあった一方、
同IPの別source portは18640不足で拒否されていた。
同一IPを複数nodeが共有している可能性があり、IP単独では個々のnodeを識別できない。
