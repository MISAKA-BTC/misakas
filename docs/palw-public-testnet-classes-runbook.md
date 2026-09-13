# PALW public Testnet-11 classes

Verified against current `main` on 2026-09-13. The chain is authoritative for class ID, status, share, budget and artifact root.

## Inspect the current registry

```bash
misaka --network testnet-11 model list
misaka --network testnet-11 model status --help
```

The Relaunch 5f genesis class allocation is:

| class | share | local artifact for a full node | artifact for producer/panel |
|---|---:|---|---|
| PALW-BASE-0/Floor | 22‰ | none | none |
| QWEN25-A16 | 489‰ | none | required |
| QWEN36 | 489‰ | none | required |

Do not copy a class ID from a prior relaunch. The current A16 graph/profile and older A16 rows have different IDs even when the model name looks similar.

## Full nodes

A full node validates class commitments and chain transitions without loading model weights. It needs an artifact only when acting as a producer or panel/verifier for that class.

## Floor

Floor is an embedded deterministic integer class:

```bash
misaka --network testnet-11 mining setup --model floor
```

It needs no GPU, tokenizer or model file.

## A16 and QWEN36

Use the current catalog and on-chain facts:

```bash
misaka --network testnet-11 model list
misaka --network testnet-11 mining setup --model <model-name> --artifact <file>
```

The setup checks:

- artifact container and root
- class/profile identity
- tokenizer binding where applicable
- host resources
- Bond capability
- class-specific collateral
- fee output

For a large artifact, `--verify-artifact` performs a full read and root verification.

## Adding or certifying a model

```bash
misaka --network testnet-11 model add
misaka --network testnet-11 model add <catalog-model> --artifact <file>
```

No argument lists the current catalog. The command resumes registration/family/lane certification from chain state. A model absent from the catalog requires a reviewed canonical profile, deterministic converter/runtime, complete reachable-kernel adjudication and conformance tests; copying a GGUF or safetensors file into the node does not register a class.

See [palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md).

## Panel/verifier

```bash
misaka --network testnet-11 verifier setup
misaka --network testnet-11 verifier status
```

A panel seat must be able to replay the selected class and therefore needs the matching artifact/capability. The current verifier role is unpaid.

## Collateral

Class collateral is derived from current PWU and whole claim lifetime. Use the wizard or:

```bash
misaka --network testnet-11 bond status \
  --bond <txid>:<index> \
  --class-id <current-128-hex-class-id>
```

Collateral is fixed at registration, cannot be topped up and does not become a reusable key slot after retirement.

## Free-prompt lane

The free-prompt gateway is a separate service path (gateway/worker/rail) whose receipts can become PALW work. It is not required for ordinary attempt production and should be configured only after the class producer/panel path is healthy.
