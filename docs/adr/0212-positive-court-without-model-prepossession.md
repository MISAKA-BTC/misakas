> Local adoption 2026-10-31: ユーザーの実装指示と番号確認により、Positive Court R2をRFC-0044 / ADR-0212として追加。既存0042 / 0210は保持。Implementation in this branch: A0 shared contract r1; the prior EXACT prototype remains in the source checkout and is not integrated. Independent review / native carrier / public activation: incomplete, inactive. 元ZIP内のDraft表示・未測定事項は履歴として下に保持。現行実装範囲は [status](../design/palw/positive-court/IMPLEMENTATION-STATUS.md) を参照。

# ADR-0212 — Panelが正常・不正の証拠生成を担い、Minerは結果commitと上限付き対話応答に限定する

- **Status:** Proposed / Review Required / R2。Rust未実装・実LLM未計測・未有効化。
- **Date:** 2026-10-31。
- **Spec:** [RFC-0044 R2](../rfc/0044-positive-court-full-coverage.md)。前案の同一判断案を改訂。
- **利用者指定:** Miner＝LLM推論・結果commit・必要data応答、Panel＝検証計算・証拠構築・Positive Court提出、Node＝独立検査・裁定・報酬/信用制御。
- **統合先:** ADR-v2-0038。凍結例外の状態はADR-v2-0050へ参照。現行入口を51本に増やさない。

## 背景と既存実装の位置

既存のkernel verifierは自分のmodelで再計算し、不一致ならFileProofを組み立て、必要dataが不足すればDemandを行う。producerにはRespondの処理がある。一方、旧producerのclaim生成にはbuild_evidenceもあり、完全な結果commitだけの軽量Minerではない。[ソース確認](../design/palw/positive-court/IMPLEMENTATION-ALIGNMENT.md)

RFC0042の前案もPanelを提出者にしていたが、全traceのcommitをMinerに要求していた。そのままでは、正常証明の重い計算をPanelへ移す目的を明確に満たさない。今回、結果commitと証拠commit・署名責任を分離する。

## 決定案

1. **Minerは通常推論の結果を固定する。** 新profileでは全trace/witness作成を必須にしない。data応答は署名済みの有限policy内に限る。旧claimの必須fieldは別versionのまま残す。
2. **Panelは計算するproverである。** model/inputを取得し、中間値を再計算し、証拠用trace・Merkle tree・opening束・provenanceを構築し、正常/不正Courtを提出する。単なる署名者・転送者ではない。
3. **Nodeは検証する。** Panel witnessの全relationと接続、登録model/inputへのbinding、終端とMiner outputの一致を独立に検査する。署名数や未告発を代替にしない。
4. **全Panel合計でMiner負担を制限する。** data key/rangeをcanonical化して要求を集約し、bytes/work/round/保持/費用の累積上限を設ける。full witnessや検査用projectionをMinerへ要求しない。
5. **witnessの作者に責任を帰属する。** Panelの偽traceはPanel witness拒否であり、Miner有罪ではない。Minerの結果不正は署名scopeに結び付くexact反証で裁く。data応答完了と下流Court完成は別台帳にする。
6. **確率型はPanel witnessを先に固定する。** 後続seedへbindし、交代Panel・候補変更・reorg・abortもsoundness予算に含む。Miner全traceを外した代わりに事前固定まで外さない。
7. **対話は通信・資料準備の最適化であり、被覆の省略ではない。** 基準はEXACT_STREAM、承認済み整数profileのみ全relation直接検査。Node計算・bytesが少ないとは未測定で主張しない。
8. **モデルの事前保有は不要、必要weight開示は必要。** sole holderがMinerなら冷起動時のdata供給は大きくなり得る。有限上限に入らなければprovider契約/事前共有または受付拒否。秘匿計算の新証明方式は導入しない。
9. **未検証は無信用。** PositivePassedに加えGate C、依存、責任windowを維持。正常Court資料の搬送権を未検証claim自体の信用に依存させない。
10. **三者の実費を測る。** Miner追加負担、Panel証拠生成、Node検査を分離し、費用の支払者も明示同意する。Panelの再計算を二重採掘として発行しない。

## 不採用の設計

| 案 | 理由 |
|---|---|
| Minerが全Positive証拠を作りPanelが転送だけする | 今回の指定する作業分担に反する |
| MinerがPanel witness全体を再署名することを必須化 | 責任転嫁と証拠再計算負担を招く |
| Panelが正常と署名したらNode検査を省略 | 全員共謀条件を満たさない |
| 対話で数箇所を確認したら全計算合格 | 未検査不正を隠せる。被覆条件を維持する |
| Panelの不正witnessや放棄をMiner有罪にする | 値の作者・署名責任が異なる |
| 私的な無応答だけでMinerをslash | 送信・検閲・Panel隠匿を客観識別できない |
| Node負荷をPanelの計算へ単純転嫁して0とする | 独立検査は残る。性能は別の実測条件 |

## 互換性・凍結・採択

[POLICY-DELTA](../design/palw/positive-court/POLICY-DELTA.md)の限定変更を採択時に明示する。現行PRINCIPLES、wire、runtime、fenceをこの成果物では編集していない。利用者指定の責務分担を文書へ反映したことと、全protocolの採択/mainnet承認は別である。

年次カタログ・1.0/0.3・モデル審査不採用・Panel=0休眠・暗号学的validity proof不採用は変更しない。RFC A01〜A12が同じ候補で満たされるまで有効化しない。
