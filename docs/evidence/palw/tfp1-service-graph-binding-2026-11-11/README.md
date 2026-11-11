# 専用TFP1の予約graph固定

2026-11-11。基点 `cc7f7e81b`。**実装中・Rust検証未実行**。

## 接続

専用frameの明示第2版に、成功した保護予約のclaim単位graph identityを追加する。元Original、obligation、原受理DAA/blue score、canonical順のpacket ID、bytes/work/io/controls、releaseと期限、因果辺をallocation-freeでhashする。物理slot割当と観測供給は復旧時に変わるためidentityへ入れない。frame identityをgraphへ戻してhashする循環も作らない。

第2版のfactoryは実署名準備とbookingのOriginal・obligation・受付点を照合し、deep copy前にgraph IDを導く。旧第1版のwireとidentityは維持し、旧factoryは互換fixtureに限定する。新版はpins後に固定64byteを追加し、Borshとnative scannerの双方で欠落・zero・不明版を拒否する。

保存候補、5件連続のadmission journal、snapshot、service-only delta、および通常native service writerでframeに固定したgraphを照合する。同scopeでもpacketの省略・費用や期限の変更・因果辺の削除を許さない。旧版と旧claimのwireの意味は変更しない。

旧compact releaseはpacketと因果辺を消し、scopeと件数しか残さない。第2版のframeにはその経路を適用せず、専用の有限dispositionが接続されるまでlive graphを保持する。このholdを有限終端の完成と数えない。Final、担保解放、escrow決済の新しい許可も追加しない。

## 検証と残件

実署名・UTXO handoff由来の保存fixtureを第2版へ移し、構造上正しい同scopeの予約について費用削減、期限変更、packet省略、因果辺削除を用いたsnapshot/delta/writer拒否の回帰を追加する。旧wire互換、新版codec往復・改ざん拒否、観測時計をhard後へ進めても旧releaseで消せないことも記述した。ローカルfixtureだけterminal窓を4 DAAにして、最大slotの未供給packetを正規の因果順repairで移し、同じframeを保持してnative writer・delta適用/逆適用が成立する成功対照も追加した。**WRITTEN_NOT_RUN**。

ここで固定するのは実装済みの保護予約graphの内容であり、全工程plannerの完成を意味しない。完全fee・全非Batch義務・資源/期限・初回profile解決、受付coordinator、専用の有限disposition/settlement、FpTokenizer routeとRPC/Studio接続は残る。

公開T12へ配備・quote・署名送信はしていない。同一claimのPanel・Final・当選権利消費・対応block生成は未証明。モデル取得証明・モデル重み配信は追加しない。hashは予約内容の同一性のためであり、モデル計算の正しさや取得可能性の証明ではない。

採択済み[実装計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md) §1に従いCargo check/build/testは縦接続完成後。静的レビュー・整形・差分確認と文書検査の結果は [validation.json](validation.json)。
