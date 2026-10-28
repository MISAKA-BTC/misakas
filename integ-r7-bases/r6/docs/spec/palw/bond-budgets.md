# bond の予算・拘束・責任

> 採用済み条項の読解用統合版。新規規則・実装検証・本番有効化ではありません。[状態](../../STATUS.md) / [凍結](../../governance/design-freeze.md)。

適格bondがQ（claim）、B（報酬block）、R（報酬）、F（Final weight）の最大枠を制限します。同額元本・同mode・同期間なら高速偽造や計算省略で最大枠を増やしません。予約と消費を二重計上せず、mode・model・キーを変えても履歴と残存責任をresetしません。

後続決定の拘束基準は **Panel=0が7日相当DAA、Panel=1が1日相当DAA**。日当たり上限・共通長期会計・burst・予約を一体評価します。通常の最早再利用と担保解放は別で、未解決の裁定・責任を保持します。早期Valid/Finalで期間を短縮しません。

30日平均は配分履歴で、担保の30日拘束という意味ではありません。RFC-0016の通常Kernel claimはB=F=0、利用者払いescrowのみで、V2coinbase採掘経路と混同しません。利用者escrowをモデル配分で受理後に削りません。

## 出典と適用条件

[ADR-0176](../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) / [RFC-0016](../../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)。

[パラメーター台帳](../../governance/parameter-register.md) / [mainnet後への延期](../../governance/post-mainnet-scope.md) / [仕様索引](../README.md)。
