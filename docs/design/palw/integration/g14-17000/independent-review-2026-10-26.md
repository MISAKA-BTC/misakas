# kernel route Panel=1 slice の独立レビュー(2026-10-26)

対象: `2119c943b`(`claude/g14-panel1-17000`)、`PALW_T12_KERNEL_SLICE_FENCES_V1`(`consensus/core/src/config/params/full_activation_v1.rs:302`)。
担当: Claude agent(実装 lane とは別に読んだ。build も試験も実行していない。読んだ範囲は末尾に書く)。

## 指摘

### MAJOR-1: interim seat の抽選と challenge seed が claim id だけで決まる(grinding できる)
- 根拠: `consensus/core/src/palw_kernel_route_fold_v1.rs:1463-1503`(`draw_seats`)、`consensus/core/src/palw_kernel_route_v1.rs:667-696`。
  seed は `H(domain; claim id)`。`challenge_anchor` と `sample_seed` も `H(claim id)` で、後から出る beacon や block hash を含まない。
  抽選から外されるのは、producer の bond と、それと同じ `operator_id` の bond だけ。
- 攻撃の筋書き: producer は claim の可変な部分(nonce、prompt の空白など)を変えながら、手元で seed を計算する。自分の Sybil operator(別の `operator_id` を持ち、担保は floor 以上)が Panel=1 の 1 席を取る claim id が出るまで続ける。その Sybil が 1 回の抽選で席を取る確率を p とすると、必要な hash は期待値で 1/p 回なので、費用はほぼ 0 である。こうして嘘の claim が `Final` まで届く。sample_seed も事前に分かるので、標本に入らない位置だけを偽ることもできる。
- どこまで影響するか: Gate C(`palw_security_certification_fold_v1.rs:150`、`sweep_security_certification_v1`)は、commit より後の `round_seed_anchor` で抽選する。Final 報酬は `hold_kernel_final_reward_v1` で保留される(`palw_kernel_route_fold_v1.rs:1412-1416`)。したがって合意 credit と報酬は、Gate C が守っているという前提に立っている。ただし次の 2 点は未確認である。
  - (a) Gate C が成立しなかったとき(`NoPopulation` を含む)に poster へ返金するだけで producer が slash されないなら、grinding に伴う損失は「Gate C の抽選で正直な検証者に当たる」場合だけになる。producer の期待損失は bond が拘束される時間だけになる。
  - (b) Gate C より前の Final が frontier・settlement・Rule E・anchor・beacon source に 0 しか寄与しないことを、今回の読みでは全経路について確かめていない。
- 必要な修正または試験: seat seed と sample_seed に、commit より後の beacon(RFC-0010 / `round_seed_anchor`。Gate C と同じく commit より後の anchor)を入れる。これが fence の変更になる場合でも、最低限、次の試験が要る。producer の Sybil が Panel の席を取った嘘の claim について、(1) Gate C が成立せず、報酬 0・weight 0・beacon source に採用されないこと、(2) Panel 外の bonded verifier 1 人が `FileProof` を出し、期限内に slash まで到達すること。当選確率は stake に比例する(`clamp(1, 1_000_000)` MSK)ので、operator を分割しても有利にも不利にもならない。しかし grinding を防ぐ効果はない。

### MAJOR-2: G14 の値に「not wired into any preset; not measured」と書かれている
- 根拠: `consensus/core/src/palw_g14_values_v1.rs:32`(`PALW_G14_VALUES_STATUS_V1`)。court の容量、ingress fee(1 session 1.1M sompi)、turn deadline 42 DAA、ρ_court などは目安の値で、preset に入っていない。
- 影響: 公開告発(却下 fee、seat 課金、block 予算、期限内の conviction)の経済不等式が、実際に武装される値で成り立つかを確かめられない。2026-10-22 のユーザー方針は「目安で pin してよい。根拠と見直し条件は台帳に書く」である。値が pin されていない今の状態は、この方針から見て武装の前提を満たしていない。
- 必要な修正: 武装する preset に値を配線する。台帳には目安の値であることを書く。「junk filing で block 予算が飽和した状態でも、外部者 1 人が期限内に slash まで届く」drill を、出荷する binary で実行する。

### MINOR-1: interim challenge の `challenge_anchor` も claim id だけで決まる
原因は MAJOR-1 と同じ。名前どおり暫定の仕組みなら、slice の武装時に本番の seed に置き換える。置き換えないなら、Gate C 以外の判定に使わないことを明記する。

## 確認した範囲 / 見ていない範囲
- 確認した範囲
  - fence の一覧。
  - kernel fold の seat 抽選。
  - Final 報酬の財源が escrow だけであること(GAP-5、`:1042-1060`、`:1360`)と、Gate C までの保留。
  - Gate C の m/k=75/41(`palw_security_certification_v1.rs:103-104`)と、commit より後の anchor を使う抽選。
  - paid claim の sample seed が beacon から作られること(`misaka/palw/kernel/src/claim_paid_v2.rs:418-440, 505-530`)。
- 禁止事項
  - 読んだ範囲では、ZK・succinct proof・計算の正しさの暗号学的証明に依存する箇所はなかった。
  - モデル単位の審査も、読んだ範囲には入っていなかった。ただし `palw_model_eligibility_fold_v1`(RFC-0017)の sweep は tick から呼ばれている。コードの記述では、fence より前は何もしない。この仕組みは PRINCIPLES §9 と矛盾するので、この slice でその fence が武装されないことを確認する必要がある。
- 見ていない範囲(結論は出していない)
  - `FileProof` の seal/reveal、却下 fee、G-K1 の詳細。
  - F02 の duty と default。
  - X3 の hard deadline。
  - settlement once の reorg・restart・IBD・pruned import の経路。
  - ADR-0192 の共通 frontier。
  - beacon source を producer が操作できるかどうか。
  - Panel route との shadow comparison で判定が設計上一致するか。
  - e2e 試験(`g14_kernel_route_e2e/canonical/`)。

## 総合判定
条件付きで、今は武装しない方がよい。この slice を武装してよいのは、次の 2 つがそろってからである。
- MAJOR-1(seat seed と sample seed の grinding)について、commit より後の beacon で塞ぐか、無害であること(報酬・weight・credit が 0 で、外部者が slash まで届くこと)を試験で示す。
- MAJOR-2 の値を preset に pin し、drill を通す。

見ていない範囲には、別のレビューが必要である。
