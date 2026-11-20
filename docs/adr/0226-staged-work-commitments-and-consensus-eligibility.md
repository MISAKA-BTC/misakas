# ADR-0226 — 実行commit・段階別抽選・仕事の一回性を設計案として分離する

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED**
- 対応RFC: [RFC-0058](../rfc/0058-staged-work-commitments-and-consensus-eligibility.md)
- 同時改訂: [RFC-0056](../rfc/0056-global-claim-admission-difficulty.md) §16、[RFC-0057](../rfc/0057-court-nonce-and-panel-challenges.md) §16
- 依存する容量契約: [RFC-0055](../rfc/0055-global-admission-court-calendar-and-one-bps.md) / [ADR-0223](0223-global-admission-court-calendar-and-one-bps.md)
- 本記録は提案の判断と境界を記す。設計責任者の採択、native実装、公開activationを代行しない。

## 背景

ユーザーはPALW白書の監査を提示し、ticket先読み、計算の非償却性、Panel抽選の選別、資料取得とcourt活性、Final後の取消、DAG安全性の問題に対処するRFC修正を依頼した。

入力はdocs(3).zip、貼付けた設計メモ、この会話で作成済みのRFC-0055〜0057、および固定commitのsourceである。今回の添付に白書PDF原本はなく、原PDFの式・ページを直接監査したと扱わない。

source上のC_attempt_zeroは、challenge欄をゼロ化したattempt全体であり、trace・output・execution rootを含む。したがって「rootが一切ticketに入らない」という批判は現在mainに対する確定事実ではない。既存WorkId重複拒否、FPの未来beacon、認証後の成熟規則も存在する。

一方、未検証rootの安価な候補作成、有効実行からのheader表現選別、機会の別名化、beaconの偏り、正しい計算のcache／簡約、全consumerの合意資格接続は、それぞれ独立して検討が必要である。

## 提案する判断

1. **仕事と経済試行を分ける。** TrialSlotは資本機会、ComputationalWorkKeyはcanonicalな実計算記述、OriginalIdは結果rootと責任を含む元commitとする。native WorkIdを保持し、一意なadapterで対応付ける。
2. **ownerを先に固定する。** 資金付き登録をR_jobより先に閉じ、owner／beneficiaryを後から差し替えられなくする。無資金の偽WorkKey予約や公開traceの先取りを権利にしない。
3. **三つの因果stageを分ける。** 登録閉鎖→R_job→実job→Original閉鎖→R_ticket→受理集合閉鎖→R_panel。既知seedのdomain変更を未知乱数として扱わない。
4. **全候補にfull court責任を作らない。** presealの有限control予約と、ticket後の全工程受理を分ける。claim時計をPanel抽選後にリセットしない。
5. **既存のDと容量を使う。** 120秒のCAD1 controller、0055のhardな資源予約、全carrier合計1 BPS、既受理negativeの保護を維持する。新Dを二重に掛けない。
6. **Court contextを仕事へ結合する。** 新版CPN contextにOriginal／WorkKey／Panel freezeを結合する。共有opening、actorの承認、支払い・制裁の一回性は別に保持する。
7. **未検証候補の合意寄与を閉じる。** 新profileのclock／DAA work標本／weight／成功報酬／EXEC発行を用途別資格に接続する。元の認証・成熟・同batch反証優先・現在状態再検査を全consumerで行う。
8. **成熟とsettlementを分ける。** MatureEligibleは新profileの導出viewであり、旧Finalや台帳settledの改名ではない。reorgと通常の取消も別に扱う。
9. **証明できないものを残す。** nonceやroot bindingで独立した物理労働を証明しない。任意モデルの費用下界、資料の客観的終結、DAG common-prefix／収束は独立の未完了項目とする。

## 凍結原則との境界

整数完全一致、モデル非保有Node、局所裁定、計算正当性の暗号学的証明を使わない方針、モデルの無審査1.0／0.3、登録モデル全体の発行上限、claimごとの検証を維持する。

今回のBFT乱数案は**DISABLEDな参照候補**とする。DNS-BFT廃止、別の特権的委員会を設けない既存方針、PQ-onlyに対する例外を、本ADRの提出だけで承認しない。BLSの一意性はその古典暗号前提の下での性質であり、外側ML-DSA署名では耐量子化しない。

この候補を具体化する場合は、committee形成・DKG・key rotation・fault model・freeze・clock・障害終端までの完全な採択案と、方針上の明示的判断が必要である。既存native beaconで能力契約を満たす別の根拠が得られる場合も、その根拠を独立にレビューする。方式未確立のまま「BFTで安全」として新profileを有効化しない。

## 比較した代案と判断

| 案 | 判断と理由 |
| --- | --- |
| ticketへtrace rootを一つ足せば完成 | 不十分。現attemptには既にrootがあり、正規化・時系列・再抽選・費用下界が別問題 |
| Court nonceだけで全Panelの仕事を証明 | 不採用。署名封筒のreplayは防げても、共有資料・局所計算・外注を区別できない |
| Dを生の到着件数で上げる | 不採用。無資金spamで正直者のdifficultyを操作でき、前段負荷も消えない |
| 異議にも新D／新乱数を要求 | 不採用。受理済みの正直な反証を止めるため |
| BFT署名／ACKを資料到達の証拠にする | 不十分。その申立人の全scope取得・残検査時間・包含機会を示さない |
| BFT randomnessをDAG finalityとみなす | 不採用。別の安全性命題と信頼前提である |
| 正しい高速化traceをslashする | 不採用。正しい計算の実費と規範workの差は算術不正ではない |
| モデル別benchmark合格を登録条件にする | 不採用。凍結したモデル無審査方針を変更しない |
| 旧claimや旧試験結果を新版に読み替える | 不採用。旧意味・過去証拠・元期限を保持する |

## 帰結

提案の順序・責務・失敗終端は実装とレビューが可能な形になる。新しい登録・Original・Panel段階、nullifier、snapshot／journal、保護されたcontrol容量が必要であり、待ち時間と保持費用は増える。

一方、任意モデルの計算費用が規範演算量に比例する保証は得られない。実行結果の検証可能性とPoWとしての仕事下界を分けて評価する。既存の採択済み報酬条件を未証明の下界だけで遡及無効化しないが、新しいPoW成立の宣言にも使わない。

120秒はdifficultyの計画epochであり、1 BPSは外側搬送の目標である。全modelで一件だけ、courtを固定五件、必ず120秒Final、1秒settlementという変更はしない。

## 必要な証拠と実装

[実装計画](../design/palw/integration/staged-work-implementation-plan-2026-11-17.md)と[脅威表](../maintenance/revisions/2026-11-17-work-binding/THREAT_MATRIX.md)の全経路・正常系・敵対的系を追跡する。

抽象モデルは仕事一回性、stage順序、有限局所反証、成熟・権利消費の契約を検査する。native全route、実署名・DKG、実LLM、資料取得、最低Nodeでの負荷と回復、実1 BPS、PoW下界、DAG定理を代替しない。

[今回の改訂記録](../maintenance/revisions/2026-11-17-work-binding/README.md)に入力保全、旧試験の保持、現在の試験結果と未完了事項を記録する。RFC-v2は20冊、ADR-v2は50冊を維持し、新番号は統合後の変更記録として登録する。
