# testnet-200 real Qwen PALW proof — 2026-07-29

This public bundle records a Mac Studio Metal inference, its independently
signed k=2 Receipt-v3 pair, the testnet-200 bonded-provider DA object, the
registered public leaf, and the final consensus/reward reports. No ticket
authority seed, raw ticket nullifier, or TicketSecretStore is included.

## Real inference

- Model: `Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf`
- GGUF SHA-256: `1dc494614bee8a3bc00e79fe5a49da0fc1c36b3b118c4156e223e98e5a0a671b`
- Runtime: `qi35-int-v1`, Metal v3, Mac Studio M1 Max
- Prompt: `PALW testnet-200の実GPU推論証明として、2たす3の答えを数字だけで返してください。`
- Decoded answer ends in: `5`
- Replica A: slot 0, `macstudio-provider-a`, 9.235115875 seconds
- Replica B: slot 1, `macstudio-provider-b`, 7.587924292 seconds
- Prompt tokens: 27
- Output tokens: 38
- Total committed tokens: 65
- Canonical compute units: 387444
- External k=2 pair id:
  `f89190aec9d1a79b5e2c3cdebe9ea132d037452a064399095b644233605dc96daf36dbccf23a8144f5361dccd63f45a9cc49626602d4f63779ff2a1f619f2392`
- Output commitment:
  `e83cf2e6d10c8d56a297d7e75d8f670f2c363abeb9cca724b47bead43025a461bf6ef30e60eff6b2f0779900f46c8676b0ec1c5e23dc32f7c32d009f9434081e`

The verifier reconstructed both canonical Receipt-v3 bodies, verified both
ML-DSA-87 envelopes, required distinct credentials/slots, required an exact k=2
projection match, required byte-identical worker results, and recomputed the
output-token commitment.

## Inference-bound ticket and on-chain leaf

- Proof commitment:
  `a25fdf869e5e3a9c76ca93b61bb5034788842897cfeb3952cb6d24cfa06a1aca67a6c2d7d8c77b5a3f103d0f87016a4e799ee4fa543d7b55e310a2b218cde8f4`
- Ticket nullifier commitment:
  `c54f34863c4c2988de3aec9e34c3533493b09883abd368d913f531978fba5174ac8da519541beefb2f7aeebc846846c7be3d0df2e38c454251562ae42013ed71`
- Ticket derivation: ticket-authority secret + verified proof commitment +
  verified external k=2 pair id
- Model profile id:
  `951be0881cfa64bdd8242630d8173c9aee08086fa01a342ad4620e94b167ec209b57dfcbe6284ceeea301c5ec405689200b63246e6bba2c7df7e106d86efce15`
- Batch id:
  `264ecc41631bd937fded02ae8116f9fae5583288a2023a281c2023ebd12d3a28ba83f82dbf00cefa16bf72d92e188efb96230576b0237a07d1097dac9c5160bb`
- DA object root:
  `1db1c555eabfc82ed84e664082c8734f9335b832da6f72073b3abdc356f34643ca755dab5c3522f4732885513a6a93e1b593849e1ac9ee40461289c3a8f9272d`
- DA object: Receipt v3, 31,288 bytes, 2 chunks
- DA obligations: provider A and B both challenged and satisfied
- Epochs: registration 4202, activation 4210, expiry 4226
- Batch status: `active` on node A, node B, and the 160 VPS node

The bonded-provider DA object re-signs the real projection under the registered
provider A/B owner-authorized session keys. Offline verification confirms both
owner-to-session ML-DSA-87 authorizations, both Receipt-v3 ML-DSA-87
signatures, distinct slots 0/1, and exact equality of output/schedule/execution/
route/state roots, compute units, token count, and stop reason with the Mac
proof. The public leaf keeps runtime class `44…44` because that is the
pre-registered testnet-200 provider capacity class; the exact Metal runtime and
artifact hashes are authenticated in `remote/real-provider/proof.json`.

## Algo-4 consensus and settlement

- Algo-4 block:
  `c7ffe7678dce891dd4a5679985033c8d74e0587336c5f1dbddb8e98afd621bc8b49553a2284f00c394b8a6fb081594f30c2fea9c535c2f7fdf329f35584c2e70`
- Source batch/leaf: the batch above, leaf 0
- Source subsidy: 205972571 sompi
- Blue settlement block:
  `e50000927cb322bf976ff1e2bae4ee5243ff3ea8aa24319a4b22b38a93febd71be9194aaf74a77fb90f6ea9eeeb94a28a1ce6e656832fd9900f6b7c03e36d70e`
- Provider A paid: 79299440 sompi to its exact registered reward SPK
- Provider B paid: 79299441 sompi to its exact registered reward SPK
- Verification: `PASS_SETTLED`

The same block and blue settlement were fetched independently from node A
(`95:27220`), node B (`95:27230`), and the remote VPS node
(`160:27231`). All three returned the same batch id, block hash, settlement
block, amounts, and reward scripts.

## File integrity

- `remote/real-provider/proof.json` SHA-256:
  `9a1479f89469c48d39d43461235aea4f7c3c87fcb7cdd6de8310706d5df775b5`
- `remote/da/1db1…9272d.palwobj` SHA-256:
  `a4a41bf8a7b4f66f16d7379109b246eb60f690d6feb1591cf76ffefcf820260c`
- `remote/leaves.batch.json` SHA-256:
  `c7cd3e3b80d72b9d6ef799cb9f22c1f90dc4a21976f8079c1c17dd54e544ce2a`
- Replica A public receipt JSON SHA-256:
  `de689276961833364e50ba31ae7b24bafcb585490c4c31173b33b32c9ef395ce`
- Replica B public receipt JSON SHA-256:
  `621573cb16b9b7acf22049a402e7746d73d586984ae37fcaa8031c9b6c976497`
