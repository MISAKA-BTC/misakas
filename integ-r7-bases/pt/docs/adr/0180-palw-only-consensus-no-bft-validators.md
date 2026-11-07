# ADR-0180 — PALW のみの合意:BFT の validator を使わない

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* **Status:** Accepted(ユーザー決定 2026-10-16)。RFC-0012 の採用理由を記録する。
* **Related:**
  * [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md):PALW のみの合意と native EVM 決済。DNS validator・attestation・precommit・DNS-final・stake による reorg 拒否権を退役する。
  * fence `palw_dns_retirement_v1`:DNS-BFT の退役。RFC-0012 の方式 (b)。
  * [ADR-0178](0178-fork-choice-heals-a-partition-on-bonded-participation.md):bond 参加による fork choice。
  * [PRINCIPLES](../PRINCIPLES.md)。
* **Supersedes(合意の役割として):** ADR-0009〜0018 の DNS validator・確率的 finality・validator 報酬・stake attestation の系統。過去の記述は履歴として残す。

## 決定

**MISAKA は、BFT を使わない新しい proof として PALW を提唱する。** 合意は PoW(DAG)と PALW の有用計算・bond・公開証拠による裁定で成り立ち、BFT の validator 集合に finality や reorg の権限を与えない。

**validator を無くした理由:**
- **validator による先行者利益を無くすため。** BFT の validator 集合は、早く参加した者や初期の割当てを持つ者が、継続的に finality の権限と報酬を握る構造になりやすい。
- **運営母体を無くすため。** validator の選定・鍵・運用を担う主体は、実質的な運営母体になる。その主体に合意の権限が集まることを避ける。
- **より分散された構造を求めるため。** 参加は bond と実際の計算(PALW)によって誰にでも開かれる。権限は特定の集合ではなく、公開された規則と証拠に置く。

## 帰結

- DNS-BFT は `palw_dns_retirement_v1` で退役する。新しい規則は DNS-BFT に依存しない。t12 の履歴を読むための legacy reader は残す。
- 合意 credit の認証(RFC-0018 Gate C / `SecurityCertified`)では、抽選された検証者の承認を、正直な割合という明示した前提の下で用いる。抽選は claim ごとに行い、固定の validator 集合に権限を与えるものではない。
- 計算の正しさは、確率的な検出、公開証拠による局所化、決定論的な裁定、経済的な抑止で担保する(PRINCIPLES)。暗号学的な計算証明は使わない(PRINCIPLES §8)。
