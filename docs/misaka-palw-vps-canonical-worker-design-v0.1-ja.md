# MISAKA PALW VPS Canonical Worker — 不採用理由

Status: **Withdrawn / 不採用。**

llama.cpp / GGUF の float runtime を build・ISA・CPU class ごとに固定する worker 採用案は撤回した。異種 CPU で reduction 順序と trace が一致せず、runtime の固定だけでは公開の客観的裁定を保証できないため。整数 runtime で同じモデルを実行・裁定できる [ADR-0053](adr/0053-palw-one-execution-family.md) を採用し、専用 float class の導入計画は継続しない。
