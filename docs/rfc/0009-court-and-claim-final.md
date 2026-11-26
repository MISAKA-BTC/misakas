# RFC-0009 — Court・単独反証・claim Final

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0009](../adr/0009-bounded-court-and-verification-gated-final.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 元claimに束縛された裁定

Courtは元Original、model/trace commitment、規範入力、対象position、演算、weight index、境界、担当義務を確認する。Nodeは全モデルを保持せず、認証された必要部分と有界局所再計算で判断する。

weightの値だけを受け入れず、登録model rootへのopeningとindex/type/shapeを検査する。別モデルの小区間、別trace、未認証境界で作った反証を拒否する。Merkle openingは同一性であり、関係の正しさはcheckerが別に評価する。

## 2. 反証の完結性

Init、各Step、出力、Stop、ReadoutEndの全必須関係を局所checkerで覆う。不正な規範traceがあれば、資料を取得した能動的検証者が少なくとも一つの有界不一致を構成できる契約を持つ。

適格noticeを元期限内に包含でき、証拠を資源・時間上限内で搬送・裁定できることが必要である。全資料の永久秘匿・包含不能まで無条件に反証完了できるとはしない。その場合の成功停止と客観的義務違反を分ける。

## 3. 時間契約

時計の原点はclaim受付DAAのaである。対象の既存Batch公開profileでは最初のnotice期限は`a+3120`、係争hardは`a+10000`、最遅notice後の公開経路予算は6880 DAAとする。これらを秒、1 BPS、600秒決済目標へ読み替えない。

collect_until、material_until、責任終端、reorg保持を別欄にする。後から受理したreceipt、再掲Original、Panel対話、追加質問、再起動で原点をresetしない。保存済み署名期限を後から切り詰めて受付可能にしない。

受付時に最悪の公開依存グラフ、局所化、filing/collectionの重なり、各資源の具体予約が期限内に収まることを確認する。短い正常経路だけを見て受理し、紛争後に長いfallbackへ移さない。

## 4. 単独反証と肯定資料の優先関係

適格反証はPanel又はGate Cの肯定承認より優先する。多数票で算術不一致を正常化しない。複数の矛盾したclaimがある場合は、それぞれの元commitmentに対する客観的違反を判断する。

不一致箇所の対話局所化は任意である。すでに局所化された反証を直接受理し、相手の協力を必須にしない。単なるRootの相違だけではどちらが不正かを断定しない。

## 5. 資料不足と帰責

| 状態 | 成功・制裁 |
| --- | --- |
| 認証済み局所関係の違反 | 正規conviction、claim成功拒否、対象責任へ個別処理 |
| 署名した提供義務と包含機会が成立した不履行 | 当該契約に基づくPartyDefault |
| 検閲・包含機会不明・local資源故障 | NoSlashHold等の未検証処理。有罪へ自動変換しない |
| 元期限まで検証未成立 | 未成功終端。Final・creditを与えない |
| 不正な告発 | 認証された虚偽の内容・義務の範囲で責任を裁定 |

URL、ACK、署名一件だけで正直なPanelが全資料を取得したことにはしない。自分が取得できないという申告だけで相手の有罪や返金を確定しない。

## 6. 共有要求と供給者の競合

同じ資料は共有できるが、各claimの同意、原資、元期限、参照、責任を保持する。ownerの排他的な直列キューで公開反証を塞がず、正規の第三者供給を同じ認証規則で受理する。

供給者の初回notice、ready digest opening、終端material、対象範囲、費用は元caseから導く。予約、ACK、proof-of-retention風の表示を実際の取得・検査の代替にしない。

## 7. Final述語

対象claimの必要な肯定検証が成立し、元通知境界を満たし、適格な未解決case・不供給・有効反証・依存取消がないときにのみ成功Finalとする。署名数や未告発だけから成功にしない。

同batchではnotice、反証、失効、依存取消を成功効果より先に処理する。別claimに無関係な責任が残るだけで全チェーンを止めず、依存関係を認証して分離する。

## 8. 終端と責任保存

case終端、金融責任の適格解消、資料保持終了、prune可能を別に記録する。archived closeという表示だけで原資を解放せず、正規受付、同意、支払条件、元scopeへの結合を検査する。

未解決liabilityと必要なresponse eligibility履歴は、元Originalが再掲されたりhot cacheから落ちたりしても保持する。時刻経過だけで有罪・無罪・返金・成功を推定しない。

## 9. 保護容量と受入試験

既受理negative、資料応答、Court終端は新規Ticketや新しい乱数を待たず搬送できる容量を持つ。新claimや大量の合法異議がこれを二重予約しない。容量が足りない場合は新規受付を制限する。

偽weight、別モデル、偽readout、最後の合法notice、資料秘匿、owner共謀、公共供給、期限超過、任意対話なし、再掲、restart、reorg、cold importを同じprofileで検証する。小さいfixtureの収束を最大実モデルのCourt成立保証と扱わない。

## 10. Court容量・保持・時計の適用

既存court-capacity profileでは、claim寿命中に最初の一行分の予約poolを一回だけ与える。hard-deadline bucketの新規受理上限は`floor(reserve/2)`とし、超過は`CourtClaimAdmissionFull`として受理しない。最終時刻より所定service window以上前に受理された行が未裁定なら、対象profileの未成功・無報酬処理を行い、無視して成功Finalを残さない。

G-13の既存seat下限を適用するprofileでは240,000 BILIをdraw/bind/room/activation/seating共通関数から読む。この値を新しい全profileの自動下限にせず、profileと発効条件に束縛する。fused-attentionの既存Attemptで要求するtrace保持はlicenceから`4W + W_disclose`以上とし、保持約束と予算を受付時に確認する。

native claimの責任horizonは元署名入力と受付rulesetから`Final下限 + window_court`として導く。後続EXEC等が責任を延ばすprofileでは、短い別lifetimeで上書きしない。保持中の未払fee、資本、pending caseをhorizonまで追跡し、条件成立時の支払い、契約に従うrefund、個別slashを別遷移にする。

Panel-licensed Finalの記録専用eventは、対象profileが認めない限りbeacon sourceや合意creditへ計上しない。記録専用行の再読出しを、独立Gate Cの成功として扱わない。

## 11. 直列witness filingの時計【提案】

既存accepted cursorが各partの受理後にしか進まないpipelineを対象に、clock version 2では`max(2, maximum_moves) * inclusion_margin`と全byte inclusionを加える案とする。実planでは`max(collection_daa, filing_daa)`等の既存合成を適用し、collect/material/bondとresource identityへ反映する。

旧versionの式・serialization・plan IDを新意味に読み替えない。実搬送上限を越えたplanは拒否し、本文だけでdoor上限を広げない。将来のbatch/prefetch実装を現行の時計の前提にしない。これは提案であり、有効化前に実carrier、全依存、供給、包含、最遅時刻、資源を確認する。

## 12. 共通caseの原子性

共有要求は元root・対象範囲・codecから導くcanonical identityを使い、owner任意のlabelで別義務へ割らない。同じ実資料とdecode済み回答に重ねて払わず、別の合法choiceや応答枝は単に先行ownerが選ばなかったことを理由に消さない。

response eligibilityは当該前提が実際に受理された時刻を保持する。Original再掲、case archived化、別の選択親からの履歴によって期限や資格を作り直さない。初回noticeの等号、terminal material、ready-root opening、金融解消は対象codecの正規境界で検査する。

caseの終端と金融責任の解消は別である。未解決責任、資料保持、返金可能残額、既成立役務を保存し、無関係なclaimの進行は依存関係を検査して継続できる。
