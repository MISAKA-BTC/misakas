# PALW-AN1 独立参照実装

状態: **DRAFT_NOT_ACTIVATED**。2026-11-18。

ユーザーが会話で提供したPALW-AN1の設計説明から、この改訂で独立に作成したPython実装です。リンク先の元HTML・ZIP・参照実装・テストベクトルは取得していません。元資料にある「11グループ」「7,226レコード」の再実行結果ではありません。

## 実行

Python 3の標準ライブラリだけを使用します。

~~~bash
python3 check_an1.py
~~~

`results.json`、`test_vectors.json`、`profile.json`を同じディレクトリへ出力します。`results.json`は検査時の二つのPythonファイルのSHA-256へ結合します。

## 実装した内容

- owner、nonce、model/job/profile、beacon指定、target、期限、Panel event、会計版を事前登録contextへ固定。
- `s = H_attempt(Enc(C), sigma)`から32byteすべてを別token IDへ写像し、`[BOS, POW] + 32 challenge tokens + [SEP] + job tokens`を実入力にする。
- vocabulary 259、内部次元2、重み1,060個、2token生成の小整数モデル。
- token embedding、Q/K/V、最後の位置からのcausal attention、residual、FFN、全vocabularyのlogitと最小index優先argmax。
- 全入力・出力・実行traceのcommitment。各実leaf・padding leaf・内部nodeへ試行ID、種別、位置、サイズを束縛。
- 同じcanonical contextを登録キーから一つのslotへ対応付け、別資金参照を使っても重複登録を拒否。元claimを後続Panel eventより前に一度だけ固定し、slot・attempt ID・実計算キーをFinal・credit発行と同じ遷移で消費。
- 元jobの一回creditを要求する別のtoy会計版では、nonce・ownerを含めないBaseWorkKeyも原子的に消費。別nonceを新しい有用需要とみなさない。
- Panelが実際に受け取った重み・trace・出力のbytesからRootを再構成して認証し、その後に全体を再実行・比較。
- Nodeの`CourtContext`はnative由来のRootと固定metadataだけを持ち、全モデル・program配列・trace配列を含まない。命令openingと必要な過去値・token・重みopeningから局所関係を判定。
- `ComputationRefuted`（認証済み元claimの誤計算・形式違反）、`InvalidEvidence`（不正なopeningや異なる文脈）、`NoRefutation`（当該局所関係では反証なし）の三分類。
- 一人の有効なnegativeを同batchのFinalより先に適用し、多数のpositiveで覆さない。
- bounded Courtへ載せられない資料は、位置と資料種別を伴う取得・構造未検証reportへ分ける。選出Panelの適格noticeをnative holdへ接続し、反証未成立を成功へ読み替えない。

## 独立toy profileの具体的な規則

全演算はchecked signed-64 integerです。浮動小数点、飽和、暗黙のoverflow、softmaxは使用しません。attention scoreは`max(0, Q・K)+1`、正のscore合計で整数floor divisionします。これは学習済みLLMとの数値互換を主張するものではありません。overflow・非正分母・未知命令等は正規実行のabort/拒否であり、成功へ丸めません。

固定命令DAGをinput長から構築します。Nodeが使用するprogram root、scope、出力参照位置は、その固定profileから認証済みのnative metadataとして与えます。Court payloadがこれらを選び直すことはできません。命令が指定する入力token・過去値から、必要な重みindexをNodeが再導出し、同じモデルの別indexのopeningも拒否します。

モデル重みの配置は、embedding 518、Q/K/V 12、FFN行列8、bias4、出力行列518の計1,060個です。基準jobの2tokenに対し、入力長37、trace 6,153レコード、padding後8,192 leaves、trace木のhash呼出16,383回です。元実装の7,226レコードとは異なります。hash数はこの木の構造上の計数であり、通常LLMの実費下界ではありません。

入力・trace・出力木は試行IDに束縛します。共通のmodel/program treeはそれぞれ登録model/profileの固定scopeへ束縛します。全treeを試行ごとに再hashするという主張ではありません。

## 認証・ネットワークの境界

署名とbeaconは、テストが与える認証済みfixtureとして扱います。`authenticated_owner`・`authenticated_actor`は実際の署名検証器の実装ではありません。beacon/panelイベントの対応表はnative環境fixtureで、乱数の未知性・偏り耐性を証明していません。

`materials_ready`、`closed`、`delivered`は、実際には取得・異議機会・原期限・納品契約の状態機械から導く必要がある条件です。このtoy Ledgerではfixtureで設定します。無条件に正しいと証明した状態ではありません。検査には、これらの前提が偽で正直な検査者が不在なら、誤ったclaimを止められない反例も含めます。

独立レビューで、(1) `6153.0`のような整数値floatの長さを受付が通す一方でCourtが拒否する型不一致、(2) 512byteのCourt上限を超える認証済みleafについてPanelが反証完了と誤分類する不一致を発見し、修正しました。Claim/context/位置/時計の整数型・範囲を厳密に揃え、過大leafは単なる正規生成器の拒否に頼らず、悪意あるraw hash生成器から作ったRootでも資料noticeによるholdで成功を止める回帰検査を追加しています。

`material_notice`はnative由来の元claim、選出actor、資料種別、期待位置の有界な取得caseをモデル化します。`apply_batch`はこのholdを先に適用します。実際のネットワークでのrequest/response、取得完了の客観判定、悪意あるnoticeの有限な終結は未実装であり、このtoyにはpending解除APIを置いていません。成功停止の接続を示すもので、正常claimの活性を完成したことにはなりません。

選出Panel数5は「四人の虚偽承認と一人の有効反証」を試すための固定fixtureです。RFCの全profileのPanel人数やcourt件数を五件へ固定する提案ではありません。時計引数はnative時計のfixtureであり、実ネットワークの時刻認証・不可逆な進行を実装していません。

`apply_batch`の順序と一回のPython遷移で原子的消費をモデル化しています。DB transaction、並行実行、実ネットワークの資料取得・反証包含、fork選択・stable anchor、不可逆な外部支払いを実装していません。rollbackはnative帳簿の簡略モデルです。

同じ正しい実行をcache・別アルゴリズムで作ることは不正と定義しません。零モデルも正しいtraceなら受理します。入力の相違やMerkle bindingから通常推論と同額のGPU費用・電力・金銭費用は導きません。

会計版は`AN1-TEST-UNIT-1`（一実計算一単位）と`AN1-TEST-USEFUL-ONCE-1`（同じ元jobに一度だけ）の二つのテスト用規則です。単位は金銭・FLOPs・合意weightではありません。元jobのcanonical同一性を検査するもので、人間の需要の存在・異なる任意programの意味的同値性を証明しません。残高・資金・成熟・EXEC全consumer・本番会計を完成したとは扱いません。

## ファイル

| ファイル | 内容 |
| --- | --- |
| `an1_reference.py` | kernel、canonical commitment、Panel、局所Court、簡略native state |
| `check_an1.py` | 正常例・反例・仮定を外した境界例の検査 |
| `profile.json` | 独立toy profileの規則 |
| `test_vectors.json` | 再現可能なcontext・実入力・Root・Ticket・命令位置 |
| `results.json` | 検査結果と検査対象scriptのhash |

これらはPanel＋Court方式の参照検査です。Groth16、ZK、succinctな計算validity proofは追加していません。
