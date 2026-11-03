# Attempt の claim 受理を表示へ接続する

追加修正の実装・局所検証記録。node / consensus / params は変更しない。
frontend 配備、exporter の実 CLI smoke、実 claim の受理・panel=1・Final は **pending**。

## 表示と API

algo-6 の PAV2 / PAV3 header を読むだけでは claim の受理を意味しない。
LLM jobs は executor の `getPalwClaims` を読み、accepting block・class・executor bond・既知の claim ID
が正確に一致する場合だけ Accepted と phase を表示する。必要なら generic claim read
`getPalwFreePromptClaim` の found / identity / accepting block も照合する。
API unavailable、found=false、newest 200 rows の外、retired、RPC error は「acceptance unconfirmed」。
拒否確定とは表示しない。claim↔blocks の各 block にこの状態を表示し、decoded submissions の数と受理の数を分ける。

1 席という値は claim row の実 seats を読む。Final は SecurityCertified と分ける。
旧 claim の panel 条件や席数を変えず、共通の説明文で固定 3-of-5 を新 claim に要求しない。
algo-10 の Model execution フィルタ・round grouping / counter は変更しない。
Attempt が受理・Final になっても、それだけで algo-10 の数は増やさない。
block 検証欄も feed の有無だけに依存せず、同じ三つの identity を照合する。

古い cold sweep が存在しない `header.parents` を見て sink に止まる問題は、
既存 `recentChainAnchor` の selected-parent walker の再利用で解消する。
この箇所の同等修正は公開 UI / original main の dirty 作業にも存在する。

## exporter

旧二つの class ID だけを出力する filter を除去し、floor 以外の全 native Attempt に
block・claim・class_id・executor_bond・execution_root 等の公開 identity metadata を出力する。
PAV2 / PAV3 は既存 core decoder、claim ID は既存 `attempt_id_v2` を使う。
未知 profile に旧 Qwen の job shape を適用せず、prompt_ids / prefill / decode は null、
prompt_derived=false とする。未知 prompt / output text は推測・公開しない。
既知の二つの旧 profile の anchor-derived prompt は従来どおり。
exporter の metadata は受理・承認・Final の証拠ではない。

## 検証

- [表示テスト](explorer-attempt-admission-tests.log) **16/16 PASS**。
  制御された positive fixture の 1-seat Final、実 block 481352 の未受理 negative、
  block / class / executor / claim の不一致、found=false / limit / retired / RPC unavailable、
  available=false の stray rows、PAV3 base offset、同じ carrier block 内の 2 FP claim の Final / unknown cache 隔離を確認。新規 8 件と既存 cards / classification 8 件を含む。
- [実公開 API の negative 確認](explorer-attempt-admission-negative-live.json)：
  DAA 18,734 時点で 481352 の candidate claim は found=false / 未確認を維持。
  synthetic positive fixture をライブ受理の証拠として扱わない。
- exporter main と新 unit 2 件は既存 warm rlib を使う `rustc --test --emit=metadata` で型検査 PASS。
  ローカルのリンク・unit runtime は未実施。Cargo が別 feature variants を作り始めたため、
  空き容量低下時に自分の process group だけを停止した。他者の compiler / cache は変更しない。
- JavaScript syntax / diff checks PASS。
  [source SHA と検証範囲](explorer-attempt-admission-checks.json)を参照。

## remote CLI smoke の手順（まだ実行していない）

[入力 JSON](jobs-export-481352-input.json) は `{ "blocks": [ actual_rpc_response.params.block ] }`。
root が公開 node で保存した FULL REAL block 481352 の header / body をそのまま使う。
[期待値](jobs-export-481352-expected.json)の claim は exact unsigned Borsh bytes と既存 attempt-id domain の
独立 BLAKE2b identity 計算でも `aafbaf3e…883cc` と一致した。
これは計算の正しさや claim の受理の証拠ではない。

```sh
target/release/misaka-palw-jobs-export \
  --blocks jobs-export-481352-input.json --retention /dev/null \
  --network testnet-12 --out jobs-export-481352-output.json
```

期待は rows=1、hash mismatch=0、class_id=ebf44d0a…、claim=期待 JSON の全 128 hex と一致、
prompt_derived=false、prompt_ids / prefill / decode=null、prompt_text / output_ids は未提供。
stdout / stderr と生成 output は新しい evidence file に保存する。
retention 引数は CLI 互換用で、保持 material / answer を読む必要はない。

## original main と配備

`contrib/misakascan-t12/app.js` は original main の多数の user dirty の一つ。
この WT のファイル全体を original / live に上書きしない。
親が path 限定の三方向 merge をレビューし、元の dirty 差分を保持する必要がある。
ローカル `/tmp` に before/base/candidate を保存し、既存 policy / lifecycle 文を保持する候補で
admission / cards テスト 12/12 を確認した。original file 自体と本番サービスは変更していない。

## 独立した LIVE UI への最小適用候補

公開 app.js は isolated main とは別の既存修正を含む。
read-only download の before SHA は
`e08a95eef65dab68c549177916352ef71d9b0a9c7b2018921ec4a9f13c5850c1`。
今回の変更だけを三方向適用し、既存 static catalog・protocol note・lifecycle、
header/body hydration、coldFill / updateRecent、arrival order、round classification を保持した。
5 箇所の競合（policy 2・class lookup 2・cold-walk comment 1）は元の LIVE 機能を残して解消。
候補は `/tmp/misaka-chain-20261031/explorer-live-claim-join-candidate.js`、SHA は
`13176ce4caae3ff41ef34973605ced825809e1f680eb8b7379387e4c6d59ea3b`。
本番へはまだ適用していない。配備直前に before SHA 一致を再確認し、不一致なら再 merge する。

[候補テスト](explorer-attempt-admission-live-candidate-tests.log)は **16/16 PASS**。
これは新規 admission 8 + cards 4 + LIVE baseline 保持 4。
isolated の 16 は新規 admission 8 + cards 4 + isolated classification 4 なので、同じ内訳ではない。
元 main の dirty を保存する候補の 12 は admission 8 + cards 4 のみ。
LIVE 固有の round classification は以前から isolated と異なるため、既存 main の分類へ
勝手に置換して合格させていない。
[保持テストの再現用ソース](explorer-live-preservation.test.mjs.txt)を保存した。

親レビューで見つかった同一 carrier block 内の複数 claim の cache 混同は、
全 cache get / set / keep / count を block + class + executor + candidate claim + isFreePrompt の
composite identity に変更して防ぐ。header 単独の Attempt は従来どおり一つの identity を持つ。

## 候補 merge の cold initialization 抜けを修正

親レビューで、original-main 用の候補に新しい LLM cold initialization の接続が抜け、
旧 `header.parents` walk が残っていたことを確認した。
shipped source `3c998c366` 自体は `recentChainAnchor(dag,12)` を正しく呼んでおり、
今回の訂正は `/tmp` の候補と、その SHA / 検証 metadata のみ。production source は変更しない。

両候補の cold initialization block を shipped source と同じ呼出・説明 comment に揃え、
composite cache-key の説明 comment も復元した。
main 候補の新 SHA は
`6c599167c0c6957405a49963908e7724b7f75b5a01b733ccb36b567aa950b56b`、
LIVE 候補の新 SHA は
`13176ce4caae3ff41ef34973605ced825809e1f680eb8b7379387e4c6d59ea3b`。
それ以外の merged prefix / suffix が全 byte 不変であることを、訂正だけを逆変換して確認した。
[訂正前後と backup の記録](explorer-candidate-coldwalk-correction.json)を参照。

- 構文検査は両候補 PASS。
- [main 候補 12/12 再実行](explorer-candidate-coldwalk-main-tests.log)。
- [LIVE 候補 16/16 再実行](explorer-candidate-coldwalk-live-tests.log)。
- [cold initialization control 4/4](explorer-candidate-coldwalk-tests.log)。
  実際の初期化 block を controlled RPC / walker で実行し、shipped / main / LIVE の三つで
  selected-parent walker を呼ぶことを確認。訂正前 main が walker を呼ばず sink に留まる control も確認。
  [再現用ソース](explorer-candidate-coldwalk.test.mjs.txt)を保存した。

従来の raw test logs は保存し、今回の再実行は別 file に記録した。
配備時の public before SHA guard は引き続き `e08a95ee…5850c1`。
元 main と本番サービスは、この候補訂正でも変更していない。
