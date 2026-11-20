# RFC-0057 — 独立設計レビュー

日付: 2026-11-17（Asia/Tokyo）  
状態: **文書レビュー完了。PROPOSED_DESIGN_NOT_ACTIVATEDを維持。**

## 1. 対象と判定

対象は[RFC-0057](../../../rfc/0057-court-nonce-and-panel-challenges.md)の335行版、SHA-256 `4dac02f381f65d467cfad39d3bd061093eafd5c949895f5f24b93d96fd301875`である。

root/docsの作業規約、PRINCIPLES、既存RFC-v2-0017/0018、RFC-0049/0050、先行RFC-0055/0056との整合を確認した。初期318行版を精読して4件の具体的な仕様欠落を指摘し、修正版の該当箇所を再確認した。

**以下の是正後、レビュー対象範囲に追加の重大な文書上の矛盾は検出していない。** これは設計の記述・前提・状態遷移のレビューであり、native実装、乱数の安全性、署名実装、実機性能、公開activationの認証ではない。

## 2. 指摘と確認した是正

| 指摘 | 修正が必要だった理由 | 確認した是正 |
| --- | --- | --- |
| native seedの幅 | 初期式の`seed[32]`と、既存challengeの64-byte Digestとの接続が未定義だった | §4.2でnative versionと全seed bytesをNRで符号化し、新しい32-byte nonceと元seedを区別。切詰め・任意変換を禁止 |
| 計画からnonceへの循環 | Context内のplanに選択後targetやnonceを含めると、Context→nonce→target→Contextになる | §4.1でPlanSpecを抽出前の対象空間・selector・k/R・共通coverage規則・費用上界に限定。選択結果はTargetSetDigestへ別結合 |
| 未消費条件と受理後の計数 | 義務を消費した後に未消費を再要求すると、受理済み承認をFinalで数えられなくなる | §7でAdmissibleNewPositiveとCountablePositiveRecordを分離。既受理recordと対応するonce消費を原子的に保存し、そのrecordを計数 |
| 分割chunkの認証 | flat BodyDigestだけでは、部分chunkのMerkle openingを認証するrootが存在しなかった | §4.3で署名subjectへChunkRootを追加。chunk幅・index・長さ・padding・親hash・厳密なopening高さを固定し、完成時に全長とflat BodyDigestも照合 |

seed幅の確認根拠は固定mainの[challenge/hash.rs](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/challenge/src/hash.rs)と[challenge/seed.rs](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/misaka/palw/challenge/src/seed.rs)である。詳しい既存入口・署名・共有資料・経済効果の監査は[SOURCE_AUDIT.md](SOURCE_AUDIT.md)に分けて記録する。

## 3. 維持される安全性・生存性の境界

### 3.1 nonceと共有資料

ObligationKey、CourtSubmissionId、EvidenceMaterialId、nativeの支払い/制裁onceを区別した。同じ元rootへの正しい資料を共有することと、他人の署名付き封筒・責任・支払先を移植することを同一にしていない。

nonceは外側のbindingとchallenge導出に使い、元Minerのtrace木、Merkle leaf、モデルの整数演算を変更しない。対象集合の重複やk=Nを合法とし、同じbytesからコピー違反・独立実行の欠如を推定しない。

既存の共有source unionには、Panel identity・relay・nonceを変えてquotaを増やせない先例がある。RFC-0057は主体別nonceを共有service identityへ入れて新しい債権・予算に変換しない。旧prototypeの受信meterを、未認証packetが他人の保護予約を消費できる許可として一般化しない。

### 3.2 Panel固有challengeとPublicOneHonest

個別challengeは本人の肯定承認を数える条件であり、全員の提出を待つ新しいFinal条件ではない。claim共通の必須coverage、全選出者の完全検証責務、最低一件の承認、全体検査・異議機会、Q解消を別条件として保持している。

無回答のPanelだけを新たなQへ入れず、最初の肯定を得たことで正直な一人の検査期間を短縮しない。全actorが合法に提出する場合の潜在費用は事前予約し、一人分の予算しか確保しない構成を避けている。

これは能動的な正直者が実際に存在・検証・提出できるという既存前提を維持するものである。BFT、nonce、局所sample、署名数から、その前提を新たに証明したとは扱っていない。

### 3.3 単独反証とscope別責任

negative・資料要求・応答にはActionNonceを使い、新しいseed待ちを追加しない。positive sample外の不正、適格第三者の反証、肯定後の訂正noticeも既存の有限なcase経路で扱う。

全体肯定への客観的反証と、別対象のLocalHoldsに対する反証を区別している。SlotConflictだけで二重署名罪にせず、相互排他的な署名命題と事前責任契約を必要とする。資料不足、seed障害、service不足を算術有罪・コピー罪へ変換しない。

既存[0018](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)と[0020](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の反証/Q優先を維持し、positive再送や多数署名でRefutedを取り消せない。訂正を受け付けることと、過去の肯定署名責任を消すことも分けている。

### 3.4 有限な処理・復旧・移行

selectorは有限Rのrange抽出と疎なFisher–Yatesを使い、hash試行kR、map、sort、対象導出、opening、搬送、保持を予算化する。Rを超えた場合は同seedで再現するSelectionUnavailableとなり、無料のround変更や有罪判定にしない。

署名・native義務・費用認可を確認した完成bundleだけが義務を消費し、部分提出は有限stagingとして管理する。nonce/台帳を再送、pruning、policy更新で復活させず、正規reorgでは同じcheckpointのundo/replayへ従う。snapshot欠損を未消費・空Qへ読み替えない。

CPN1の費用をRFC-0055の元予約へ含め、既受理の救済へRFC-0056の新Dや新ExecCreditを要求しない。モデル数、actor、nonce、fragmentが増えたことから全体の資源枠や1 BPS枠を増設しない。

## 4. 検証結果の扱い

本レビュー担当は抽象モデル・native試験を実行していない。この記録に未確定の試験件数やPASSを記載しない。モデルの実行結果、対象hash、文書保全、元資料との比較、patch適用は[VALIDATION.md](VALIDATION.md)で別に確認する。

文書レビューの完了は、native署名・draw・beacon・資金・料金・全工程plannerのadapterが実装されたことを意味しない。抽象モデルがfixtureで扱う箇所も、実系の保証へ拡大しない。

## 5. 実装・有効化に残る条件

1. 全Court入口とrelay/fragment経路が同一のnative義務・署名scope・nonce・once台帳へ接続され、旧版downgradeで迂回できないこと。
2. post-commit seed、draw、PlanSpec、対象indexの固定順序と予測不能性・偏り・可用性が、実際のnative beaconで成立すること。
3. 完成bundle、部分保持、承認record、資金、責任担保、共有service receipt、case、Finalを同じ永続化・undo境界で処理できること。
4. 必要資料の公開取得、能動的な正直者の完全検証、適格な第三者と最遅notice、実包含・裁定が元時計と全工程予算内で成立すること。既存の未確立な公開資料手順を、nonce導入だけで解決済みにしないこと。
5. 全体肯定のscopeを客観的な反証・有限担保・既存offence onceへ接続し、局所一致や資源障害を誤って全体有罪へ変換しないこと。
6. 最低Nodeで未認証洪水、合法な最大需要、fork、partial bundle、復旧、保持、IBDを混在させても、測定したCPU/RAM/I/O等の上界に収まること。
7. 実LLMの同一claimから正常承認・単独反証・Final・権利消費までを確認し、実1 BPSや本人の物理的再計算を文書・抽象試験から推定しないこと。

これらは[実装接続表](../../../design/palw/integration/court-nonce-implementation-plan-2026-11-17.md)の完了証拠で判断する。新しい暗号学的計算証明、モデル別審査、Nodeの全モデル保持を補助手段として追加していない。
