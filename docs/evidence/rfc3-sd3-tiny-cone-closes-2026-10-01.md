# RFC-0003 step 6 — the reduced SD3 class: every commit-point kind's cone close

Measured by the worker on the class's own run (`misaka-palw-sdk/tests/gen_sd3_class.rs`, `the_sd3_class_passes_the_gate_and_every_cone_close_fits_one_carrier`):
the cone close at the FIRST leaf of each kind, serialized as the one-move accusation carries it (`GenCone`, tag 10), against the
one-move carrier (`palw_gen_one_move_max_proof_bytes_v1` = 95,037 B). Layout `tile_len = 16` (step-leaf lanes and weight rows per tile).

    sd3-tiny: class e76d562e41105fa5b1c313f6a0d469ef599fd312588102e088aa6de578c4868e2348b5480c04635f51c15ec2d96353233cee1ae2db9a52b32071ef33f2944e59 — 15738 canonical step leaves, 15738 at the widest job
    15570 step leaves in 63 commit-point kinds (one-move carrier 95037 B)
    largest cone close: 73676 B (vae.up0.us / post / Reshape); 2 dissected kinds: ["text_pool / attn.nope+mlp / Reshape", "text_rows / attn.nope+mlp / Reshape"]

| kind (stage / block / primitive) | leaves | close B | margin B |
| --- | ---: | ---: | ---: |
| denoise / post / Clamp | 352 | 27615 | 67422 |
| denoise / post / StateWrite | 64 | 9659 | 85378 |
| denoise / pre / Clamp | 432 | 24016 | 71021 |
| denoise / transformer_blocks.0.attn / Clamp | 1848 | 29140 | 65897 |
| denoise / transformer_blocks.0.attn / ReduceMax | 24 | 26176 | 68861 |
| denoise / transformer_blocks.0.attn / Reshape | 16 | 3590 | 91447 |
| denoise / transformer_blocks.0.attn / Transpose | 1152 | 26812 | 68225 |
| denoise / transformer_blocks.0.mlp / Clamp | 864 | 29121 | 65916 |
| denoise / transformer_blocks.0.mlp / Reshape | 16 | 3590 | 91447 |
| denoise / transformer_blocks.1.attn / Clamp | 1704 | 29140 | 65897 |
| denoise / transformer_blocks.1.attn / ReduceMax | 24 | 26176 | 68861 |
| denoise / transformer_blocks.1.attn / Reshape | 144 | 3590 | 91447 |
| denoise / transformer_blocks.1.attn / Transpose | 1152 | 26812 | 68225 |
| denoise / transformer_blocks.1.mlp / Clamp | 560 | 29121 | 65916 |
| denoise / transformer_blocks.1.mlp / Reshape | 144 | 3590 | 91447 |
| text_pool / attn.nope+mlp / Clamp | 180 | 7922 | 87115 |
| text_pool / attn.nope+mlp / Gather | 40 | 70376 | 24661 |
| text_pool / attn.nope+mlp / Reshape | 20 | dissected | - |
| text_pool / post / Clamp | 20 | 7922 | 87115 |
| text_pool / post / Reshape | 10 | 2962 | 92075 |
| text_pool / pre / Clamp | 10 | 8762 | 86275 |
| text_rows / attn.nope+mlp / Clamp | 288 | 7922 | 87115 |
| text_rows / attn.nope+mlp / Gather | 64 | 70376 | 24661 |
| text_rows / attn.nope+mlp / Reshape | 32 | dissected | - |
| text_rows / post / Clamp | 16 | 7922 | 87115 |
| text_rows / pre / Clamp | 16 | 8762 | 86275 |
| vae.in / post / Reshape | 64 | 27896 | 67141 |
| vae.in / pre / Reshape | 16 | 2694 | 92343 |
| vae.mid.at / post / Clamp | 708 | 10352 | 84685 |
| vae.mid.at / post / Concat | 6 | 13968 | 81069 |
| vae.mid.at / post / ReduceMax | 4 | 48816 | 46221 |
| vae.mid.at / pre / Reshape | 64 | 3078 | 91959 |
| vae.mid.r0 / post / Clamp | 320 | 10328 | 84709 |
| vae.mid.r0 / post / Concat | 12 | 13968 | 81069 |
| vae.mid.r0 / post / Reshape | 128 | 54428 | 40609 |
| vae.mid.r0 / pre / Reshape | 64 | 2886 | 92151 |
| vae.mid.r1 / post / Clamp | 320 | 10328 | 84709 |
| vae.mid.r1 / post / Concat | 12 | 13968 | 81069 |
| vae.mid.r1 / post / Reshape | 128 | 54428 | 40609 |
| vae.mid.r1 / pre / Reshape | 64 | 3078 | 91959 |
| vae.out / post / Clamp | 256 | 13371 | 81666 |
| vae.out / post / Concat | 12 | 10232 | 84805 |
| vae.out / post / Reshape | 48 | 33943 | 61094 |
| vae.out / post / Transpose | 48 | 4274 | 90763 |
| vae.out / pre / Reshape | 128 | 3078 | 91959 |
| vae.up0.r0 / post / Clamp | 320 | 10334 | 84703 |
| vae.up0.r0 / post / Concat | 12 | 13968 | 81069 |
| vae.up0.r0 / post / Reshape | 128 | 54468 | 40569 |
| vae.up0.r0 / pre / Reshape | 64 | 3078 | 91959 |
| vae.up0.r1 / post / Clamp | 320 | 10334 | 84703 |
| vae.up0.r1 / post / Concat | 12 | 13968 | 81069 |
| vae.up0.r1 / post / Reshape | 128 | 54468 | 40569 |
| vae.up0.r1 / pre / Reshape | 64 | 3078 | 91959 |
| vae.up0.us / post / Reshape | 256 | 73676 | 21361 |
| vae.up0.us / pre / Reshape | 64 | 3014 | 92023 |
| vae.up1.r0 / post / Clamp | 896 | 15589 | 79448 |
| vae.up1.r0 / post / Concat | 24 | 21376 | 73661 |
| vae.up1.r0 / post / Reshape | 384 | 56464 | 38573 |
| vae.up1.r0 / pre / Reshape | 256 | 3078 | 91959 |
| vae.up1.r1 / post / Clamp | 640 | 15205 | 79832 |
| vae.up1.r1 / post / Concat | 24 | 11896 | 83141 |
| vae.up1.r1 / post / Reshape | 256 | 36584 | 58453 |
| vae.up1.r1 / pre / Reshape | 128 | 3206 | 91831 |
