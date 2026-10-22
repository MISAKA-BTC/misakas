# RFC-0038 — free-prompt の回答トークンを支払済み計算として記録する(FPA1 carriage)

Status: **Draft** (2026-10-22, compute-claim audit lane compute-exec)。[ADR-0205](../adr/0205-a-decoded-answer-is-not-paid-twice.md)。
fence: `palw_fp_answer_accounting_v1`(dormant、testnet-12 full-activation list の entry、arming record が pin されるまで
`validate_palw_v2` が arming を拒否)。

## 動機

ADR-0145 §6 は「class が既に支払った prefix は再び支払わない」と定めるが、chain が記録するのは **prompt** だけで、
支払済み claim が **decode した回答** は記録されない。決定的な因果モデルでは位置 `t` の計算は `0..=t` のトークンの関数
なので、

* 支払済み C0 =(P, D)の回答 g を続けた C1 =(P ‖ g, d)は C0 が計算済みの位置を「prefill」として再度受け取る
  (testnet-12 で live: `palw_fp_derived_work` は genesis、decode rules は dormant)。decode rules 下でも
  C_k =(P ‖ g[..k], D − k)の梯子で Σ(D − k) ≈ D²/2 の credit を P + D の計算で得る。
* 同じ(P, D)を別 bond で commit すると(`fp_work_id_v1` は bond ごと)decode 全体が再度 credit される。leaf の値は
  bond 非依存で、別 bond の context で再 hash するだけ(hash コスト)。

いずれも正直な実行なので SEAT-R の全再実行も court も拒否できない。価格だけが拒否できる。

## 仕様

1. **carriage**: `FPA1 ‖ borsh(Vec<u8> inner) ‖ borsh(Vec<u32> answer)`。`inner` は回答なしで載る payload
   (plain V3、`TFP1`、`HFP1`)。先頭 magic なので plain payload(先頭 2 byte は version)や他の envelope と衝突しない。
   入れ子の FPA1 は拒否。fence を ruleset に持たない network では isolation が拒否(PayloadUndecodable)。
2. **fold**(fence 以降、claim の受理 DAA で判定): compute-priced commitment は `decode_tokens_executed` 個の回答を
   native ingress から受け取る。無い/長さ違いは `FreePromptAnswerNotAccountable`(報酬・weight・credit なし、carrier は
   通常 fee)。
3. **記録**: prompt row の横に `prompt ‖ answer` row を別 key(`palw_fp_answer_row_key_v1(claim)`)で書く。expiry は
   prompt row と同じ(100 epoch、epoch 境界で sweep)。
4. **価格**: `c` = 有効な支払済み row のうち、同じトークンで既に計算された先頭位置数の最大
   (prompt row は全位置、sequence row は最後の回答トークンを除く位置)。`c ≤ P` なら `c` が accounted prefix、
   `P < c < P + D − 1` なら credit は位置 `c..` のみ(prompt `c`、decode `P + D − c` として導出)、
   `c ≥ P + D − 1` は `ZeroQuanta`。
5. **回答の拘束**(arming 前提): seat は replay で claim の回答を知る。carried answer が claim の `output_root` を
   claim の context で再計算しない claim には licence を出さない。嘘の回答の claim は licence されないので、
   嘘で後続の coverage を縮めることはできず、植え付けた coverage は実際に計算した列にしか一致しない。

## 互換性

fence 未満は byte-identical(answer row は書かれず、価格関数は `answer = None` で従来と同一、FPA1 は undecodable)。
fence は Some-only で両 fingerprint と fork id に入る。

## arming 前提(未実装)

* processor の native ingress: FPA1 を剥がして `extras.fp_claim_answers` を claim id で埋める(TFP1/HFP1/TFO1 内側も)。
* isolation door: ruleset に fence があるときだけ FPA1 を受け、inner を従来どおり検査。
* seat の回答照合(仕様 5)。
* producer(worker / fp-submit / gateway / CLI)が FPA1 を出す。
