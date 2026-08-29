# PALW マイニングプール — ノードを立てずに base LLM クラスで採掘する

**これは何か。** `misaka-palw-pool` は、**ノードを持たないマイナー**が PALW の
BASE-0（floor / base LLM）クラスでブロックを生成できるようにする**ボンド必須のリレープール**です。
`kaspad/src/palw_producer.rs` は冒頭で自らこう書いています —
*「Third-party mining over RPC needs those facts on the wire, which is a protocol change and a
separate piece of work.」*。本稿はその「separate piece of work」の運用手順です。

プロデューサのループは 8 段階です。プールはそれを 2 つに割ります:

| 段階 | 必要なもの | どちらが持つか |
|---|---|:-:|
| 1. チェーン事実（class target・pwu・bond 登録・epoch budget） | チェーン状態 | **プール** |
| 2. ブロックテンプレート | mining manager | **プール** |
| 3. アンカー導出 | 上の 2 つ | 両方（マイナーは自分で再計算して検証） |
| 4. 推論（1 回） | CPU のみ | **マイナー** |
| 5. nonce 探索 | CPU のみ | **マイナー** |
| 6. 署名 | **ボンドの秘密鍵** | **マイナー** |
| 7. material の保持と gossip | P2P の口 | **プール** |
| 8. ブロック投函 | ノード | **プール** |

**プールは鍵を持たず、報酬も抜きません。** テンプレートの coinbase は
マイナー自身のアドレスに支払われ、マイナーはそれを **merkle root の再計算で検証** できます
（§4）。スラッシュされるのもマイナー自身のボンドです — 責任が仕事と同じ場所にあります。

---

## 0. なぜ参加にボンドが必須なのか（方針ではなく算術）

理由は 2 つあり、どちらも運用ポリシーではありません。

**(1) ボンドの無い attempt は存在できない。**
admission は attempt の `executor_bond` をチェーンが保持するボンドと、`executor_pubkey` を
そのボンドが登録した鍵と突き合わせます。ボンドが無いマイナーはそのフィールドに入れるものが無く、
どれだけ計算しても**そのブロックは mount 不能**です。プールは入口でチェーンに問い合わせるので、
マイナーは推論を 1 回も無駄にせず一文で理由を知ります。

**(2) 1 ボンド = 1 ジョブ。**
`base0_rc_job_anchor_v1` はアンカーを `(network domain, pre-pow, class, **bond**)` から導きます。
同一ボンドの 2 台は同一テンプレートで**同一アンカー・同一推論・同一探索空間**になります —
2 台目は追加の仕事ではなく複製です。ボンドが 1 台ごとに要るのは、2 台目が 2 つ目のジョブに
なるための条件です。プールは同一ボンドの 2 セッション目を理由付きで拒否します。

### これは分散共有（variance sharing）プールではない

正直に書きます。ハッシュプールは 1 つの探索空間を分割して share で山分けしますが、
PALW ではアンカーがボンドに束縛されるため**共有できる探索空間がありません**。
このプールが取り除くのは**ノード要件**であって分散ではありません。
マイナーが得るのは「チェーンも 30 GiB の状態も開放ポートも無いノート PC で floor クラスを
採掘できる」ことです（floor の重みは pinned seed から導出されるのでダウンロードもゼロ）。

---

## 1. マイナー側の準備（3 つだけ）

### 1a. 鍵を作る

```bash
misaka key gen > miner-seed.hex     # 32 バイト hex の ML-DSA-87 seed
chmod 600 miner-seed.hex            # 群/他ユーザ可読だと読み込みが fail-closed で拒否されます
```

### 1b. ボンドを登録する（一度だけ・オンチェーン）

ボンド登録にはノードが要りますが、**一度きり**です。自分のノードでも、信頼できる誰かの
ノードでも構いません:

```bash
kaspad --testnet --netsuffix=11 \
  --palw-register-bond --palw-bond-collateral=<sompi> \
  --palw-producer-key=miner-seed.hex \
  --palw-producer-pay-address=<自分の misakatest... アドレス>
```

キャリアが投函されると **bond outpoint（`<txid>:<index>`）が 1 行だけ印字されます**。
これが以後の `--bond` です（この行が唯一の出所 — outpoint はキャリア自身の id です）。
登録後、ボンドはチェーン状態なので**どのプールからでも参照できます**。

### 1c. 支払先アドレス

ML-DSA-87 P2PKH（`misakatest…`）であること。プールはネットワーク接頭辞が違うアドレスを
拒否します — 使えない coinbase を掴まされて「なぜか支払われない」を防ぐためです。

## 2. マイナーを走らせる（ノード不要）

```bash
cargo build --release -p misaka-palw-pool --bin misaka-palw-pool-miner

./target/release/misaka-palw-pool-miner \
  --pool pool.example.com:26350 \
  --bond <txid>:<index> \
  --key miner-seed.hex \
  --pay-address misakatest:...
```

必要なのはこれだけです。**同期済みノードも、開放ポートも、モデルファイルも要りません** —
floor クラスの artifact は起動時にメモリ上で導出され、チェーンが登録した root と一致しなければ
拒否されます（`resolve_class_v1`）。

出力の読み方:

```
[miner] admitted on misaka-testnet-11; class <128-hex> (the derived floor — nothing to download)
[miner] job j12: no winner in the assigned range (3.4s)     ← 通常。次のジョブへ
[miner] job j13: WON at nonce 918273 after 918274 tries      ← 両抽選を通過
[miner] block <hash> accepted — the coinbase pays this miner's address
```

## 3. プールを走らせる（運用者向け）

プールは **kaspad のサービス**です。別デーモンではありません — material の gossip
(`broadcast_palw_material`) にも、テンプレートを事実と同じ chain point で作ることにも
ノード内部が要るからです。

```bash
kaspad --testnet --netsuffix=11 --utxoindex \
  --palw-pool-listen=0.0.0.0:26350 \
  [--palw-pool-max-miners=256]
```

**プールに鍵・ボンド・支払先アドレスは要りません。** それがプロデューサを走らせることとの違いです。
`--palw-produce` と併用もできます（同じノードが自分でも掘りつつプールも提供する）。
クラスは既定で floor（`base_class_id`）— 全マイナーが無ダウンロードで解決できる唯一のクラスだからです。
`--palw-producer-class` を与えるとプールもそのクラスに移ります。

**プールが引き受ける義務**: マイナーが送ってきた material を `palw-retention/` に保存し、
パネルへ gossip し、その**後で**ブロックを投函します（プロデューサと同じ順序・同じ理由 —
material を誰も配らない claim は license されません）。保存先はプロデューサと同一ディレクトリで、
再ブロードキャストの掃除も 1 つです。

## 4. マイナーがプールを信頼している範囲（明示）

チェーンを持たない以上、事実の一部はプールの言い分です。**検証できるものは検証しています**:

| 項目 | 検証可能か | どうやって |
|---|:-:|---|
| テンプレートの pre-pow | ✓ | ヘッダから自分で `pre_pow_hash_64` を再計算（アンカーの根拠なので絶対に受け売りにしない） |
| **coinbase が自分に支払うか** | ✓ | 全 tx から `hash_merkle_root` を再計算してヘッダと一致を確認し、coinbase の出力に自分の script があるか確認 |
| クラスの重み | ✓ | floor を自分で導出し、`artifact_root` と一致しなければ拒否 |
| class target / retention window | ✗ | プールの言い分（どのプールでも同じ） |

merkle 再計算が無いと「coinbase はあなたに払います」はプールが**言うだけ**になります
（見せた coinbase と入れる coinbase を変えられる）。両方あって初めて算術になります。

**プール側に残る唯一の実質的リスクは material の不配です。** material を受け取って gossip しない
プールは、誰も license できない claim を作ります → **void して報酬ゼロ**。
ただしこれは**流動性リスクであってスラッシュリスクではありません**: 正直に実行された execution は
誰が配り損ねても convict されません。

## 5. 拒否メッセージの読み方

| 拒否 | 意味 | 出口 |
|---|---|---|
| `this chain holds no bond at <outpoint>` | **ボンド未登録**（本稿の主要件） | §1b を実行する |
| `the key this miner holds is not the one that bond registered` | 他人のボンドを名乗った | 自分のボンドを使う |
| `the auth signature did not verify…` | ボンドは合っているが鍵を持っていない | 正しい seed を `--key` に |
| `the chain refuses this bond for now: …` | epoch budget 切れ・exposure 上限など | チェーンの言い分どおり待つ／担保を増やす |
| `another session already holds bond …` | 同一ボンドで 2 重接続 | §0(2)。1 ボンド 1 台 |
| `'…' is a mainnet address and this pool is on testnet` | 支払先の接頭辞違い | ネットワークに合ったアドレスに |
| `the coinbase of this template pays another script` | **プールが自分に払っていない** | そのプールを使わない |

## 6. 何がテストされているか

* `misaka-palw-pool` の単体テスト（24 本）— 認証メッセージの束縛、セッション/ネットワーク/支払先を
  跨いだ署名の再利用不能性、ボンド無し・鍵違い・なりすましの各拒否、ジョブ発行と solution の mount、
  他人のボンドでの mount 拒否。
* `tests/end_to_end.rs`（3 本）— **実ソケット越しに、チェーンだけをフェイクにした全経路**:
  hello → ボンド証明 → floor をゼロから導出 → 実アンカーでの実推論 → 実 grind → 実署名 →
  プールが mount して material と共に publish。ボンド無しが署名を求められる前に拒否されること、
  鍵を持たないなりすましが auth で拒否されることも同じ経路で。
* 署名ドメイン分離 — プール認証は `misaka-palw/pool-auth/mldsa87/v1`、attempt は
  `misaka-palw/attempt-v2/mldsa87/v1`。**別コンテキストなので、悪意あるプールが
  「認証チャレンジ」と称して attempt 署名を集めることは原理的にできません**（これはテストで固定）。

## 7. この文書が言っていないこと

* **本番ネットワークでの実測値はまだありません。** 上の 3 本は実ソケット・実推論ですが、
  チェーンはフェイクで、class target は探索が終わる値に開けてあります。
  実 testnet-11 での 1 ジョブあたりの所要時間と受理率は、走らせた人が測るべき数字です。
* 手数料機構はありません（意図的）。coinbase を付け替えられるプールは、
  マイナーが唯一自力で検証できる項目を検証できなくします。運用者が手数料を取るなら別建てで。
