# PALW 内部閾値 beacon の独立参照モデル

**状態: `PROTOCOL_MODEL_ONLY`**  
**実暗号backend: `BGWVSS_EXTERNAL_UNIMPLEMENTED`**

この実装は、閾値再構成の算術と、2種類の未来乱数を扱う状態遷移の小さい実行可能モデルである。BGW VSSの完全なネットワークプロトコル、DKG、private channel、ML-DSA、PQ暗号、native合意は実装していない。

## 実行

Python 3の標準ライブラリだけで動作する。

```bash
python3 test_beacon_model.py
```

`results.json` と `test_vectors.json` を更新する。前者は各試験名、件数、失敗・エラー、実装と試験コードのSHA-256を記録する。後者はEXECとPANELの状態、canonical encoding、全dealer秘密とshare、出力preimageとseedの公開fixtureである。**fixtureの秘密は全て公開しており、本番鍵・本番entropyではない。** これは主設計書の暗号証明や本番実装を検証するテストではない。

## 固定したtoyパラメーター

| 項目 | 値・意味 |
| --- | --- |
| share holder | `n = 7` |
| fault仮定の例 | `f = 2` |
| 私的な秘密復元 | `t = f+1 = 3` 点 |
| 公開certificate | `q = n-f = 5` 人以上の、同じ次数2以下の多項式に一致する認証済みshare |
| 固定qualified dealer集合 | 既定fixtureは5人、許可範囲は5〜7人 |
| 有限体 | `2^127−1`。本番のsecurity parameterではない |
| 乱数purpose | `EXEC`、`PANEL`だけ |
| 時計 | 整数のlogical step。native clockのfixture |
| 出力 | 固定descriptorと固定Q順の`(dealer_id, secret)`全列をcanonicalに符号化し、SHAKE256で32byteを導出。toy fieldのentropyが256bitになる意味ではない |

`t=3`と`q=5`は別の条件である。3点で秘密を知ることはできるが、公開certificateとして受理されるには5点以上を要求する。最大2人の不正という仮定の下で5点には3点以上の正直な点が含まれるため、次数2以下の別多項式で正直な点を置き換えられない、という主設計の境界をモデル化する。

## qualificationと認証のfixture

`make_qualification_fixture()` は全ての秘密係数を知る単一のローカル生成器である。各dealerのShamir多項式を生成し、全shareを直接計算する。整合性は全3点subsetの復元を列挙して検査する。

この処理はBGW VSSの配布、complaint、qualificationを実行していない。各shareの公開digestを事前に知るfixtureによって、提出shareのauthenticityを代用する。これをML-DSA検証、暗号学的PVSS、秘密通信、分散生成、外部ノードの正直性の証明として扱ってはならない。

このfixtureは真のshare以外を認証不成立として拒否するため、不正holderが偽の値に**有効な本人署名**を付ける場合のpublic certificate定理を単独では検査しない。その境界は別script `../reviews/bgw_certificate_check.py` が補足する。有限体17、n=7/f=2において不正holderが任意の偽点を認証できるものとして、103,152通りの不正多項式と腐敗集合を列挙し、誤多項式の最大supportは4でq=5に届かないことを確認する。これは署名の実装試験ではない。この件数を本モデルの29テストへ合算しない。

`ciphertext_digest` は、暗号化share transcriptを束縛する欄の**placeholder**であり、暗号文ではない。`salt`、このdigest、全share commitments、secret commitment、roundがqualification rootへ結合される。Pythonの`random.Random`は再現用の決定論的生成器であり、本番entropy源ではない。

公開salt付きsecret commitmentとshare digestの秘匿性は本モデルでは証明しない。2点だけからのShamir算術上の曖昧さの検査は、公開transcriptを含む完全なprivacy証明ではない。

## 実装した状態遷移

- `SETUP → FROZEN → RELEASING → READY`。
- 固定期限までに成功しない場合は`TERMINAL_UNAVAILABLE`。元roundの再開は拒否する。
- freezeはnativeのstable判定fixtureと固定TimingPlanを必要とする。
- EXEC freezeはregistration rootを束縛する。
- PANEL freezeはregistration root、original root、pool root、先行EXEC round、先行releaseのlogical stepを束縛し、EXEC公開より後のcloseを要求する。
- roundはchain、epoch、purpose、ordinal、roster、暗号profile、scheduleをcanonicalに束縛する。
- Qはrelease前から固定し、全Qの復元certificateが揃うまでseedを返さない。
- 公開certificateは署名/proof/集約subsetのbyte表現をseedへ混ぜない。現在のsignature/proof欄は認証を実装しておらず、長さ・型だけを検査する。
- 出力には用途、RoundRef、QualRoot、FreezeId、toy suite/codecと固定Q順の全`(dealer_id, secret)`列を使う。QualRootはsalt・qualification・placeholder ciphertext transcriptを束縛する。秘密の剰余和やXORへ圧縮してからhashする方式は使わない。主稿第11章と同じ全値列の導出原則であり、production wire形式の割当ではない。
- 失敗後の別seed、別round、別freeze、pool縮小での救済を拒否する。
- AttemptLedgerは同じattemptのround変更と通常失敗後の復活を拒否する。DB並行性・reorgは実装していない。

`verify_certificate()`は証拠の数学的・fixture上の検査関数であり、公開時刻の判定はしない。native受理をモデル化する`accept_certificate()`と`submit_share()`がrelease gateとdeadlineを検査する。これも、秘密shareを既に保有する共謀者の私的復元を防ぐものではない。

## 検査結果

**29テスト、失敗0、エラー0。**

主な検査は次のとおり。

| 検査 | 結果 |
| --- | --- |
| 5dealer × 全35組の3share subset | 175通りで元secretへ一致 |
| 7人から5人を選ぶ全21certificate subset | 同じ出力へ一致 |
| holder2人が停止し、残る5人が全Qを復元 | 成功。dealerの公開協力は不要 |
| 3shareだけの私的復元 | secretは復元できるが公開certificateは拒否 |
| 別多項式、偽share、誤index、別purpose・round・dealer | 拒否 |
| 少ないshare、重複index、Qの一部欠落 | 拒否 |
| stable freeze前・release前の公開 | 拒否 |
| Panelのpool/original root欠落、EXEC以前のclose | 拒否 |
| 署名・proofの表現変更、shareの到着順変更 | seed不変 |
| 公開後の別freeze、Q/pool縮小、fallback、別round救済 | 拒否 |
| salt / qualification / ciphertext transcriptの書換え | 拒否 |
| 異なる全秘密列が同じ剰余和を持つ例 | 別preimage・別SHAKE256出力となることを確認 |
| 期限切れ | 有限の未成功終端、成功へ昇格しない |

加えて、次の**防げない境界を再現した**。

1. `t=3`のshareを共謀者が持てば、公開gateの前でも私的にsecretを復元できる。閾値未満という仮定は不可欠である。
2. 公開済みEXEC seedをhashして作るだけのPanel seedは誰でも計算できる。domain分離を二つ目の未来未知性として扱えない。

175通りと21通りは29テスト内部の算術検査であり、別の独立テスト件数として合算しない。第29テストは、旧参照モデルの剰余和をhashする導出から、主稿と同じ全秘密列をSHAKE256へ渡す導出へ修正した際の回帰検査である。

## 本モデルの範囲外

BGW VSS完全実装、悪意dealerのqualification処理、PQ private channel、ML-DSA認証、暗号suiteの具体パラメーターと証明、secure erasure、adaptive corruption、Sybil耐性、roster選出、実ネットワーク包含、native stable freeze、共通prefix、永続DB、reorg、資源容量・攻撃下性能・本番有効化は検証していない。

また、Pythonオブジェクトの直接改変を防ぐセキュリティ境界ではない。定義された公開APIとtrusted fixtureの範囲を検査する。テストPASSを予測不能性・完全活性・本番準備完了の証明にしてはならない。
