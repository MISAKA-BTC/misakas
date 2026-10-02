# PALW classes on testnet-12

The chain is authoritative for class ID, status, share, budget and artifact root. This page is a map
of the operator commands per class; the step-by-step join guide is
[testnet12-join-mining.md](testnet12-join-mining.md).

## Inspect the current registry

```bash
misaka --network testnet-12 model list            # every class: name, status, share, budget
misaka --network testnet-12 model status <model>  # base, a model's name, or a class id
```

`misaka` defaults to testnet-12 when no network is named (`--network` > `MISAKA_NETWORK` >
`~/.misaka/config.toml`).

## Genesis classes

testnet-12's genesis registers the floor and two dense Qwen2.5-1.5B A16 held rows
(`PALW_T12_GENESIS_HELD_ROWS` in `consensus/core/src/config/params.rs`):

| class | context | local artifact for a full node | artifact for producer/panel |
|---|---:|---|---|
| PALW-BASE-0/Floor | — | none | none |
| `Qwen/Qwen2.5-1.5B/graph-v7@8192` (`ebf44d0a…`) | 8,192 | none | `qwen25-1.5b-a16-8k.palwart` |
| Qwen2.5-1.5B A16 graph-v7 at 2,097,152 (`74c67e63…`) | 2,097,152 | none | [qwen25-a16-2m-held-artifact.md](qwen25-a16-2m-held-artifact.md) |

There is no hybrid (Qwen3.6) genesis row on testnet-12; a hybrid row is a registration transaction
(see [qwen36-2m-held-artifact.md](qwen36-2m-held-artifact.md)). The held rows declare no intended
share: each registers at the bundle's `min_grantable_share_permille` and the floor holds the residual
(ADR-0137). Block rights come from verified work, so read the live figures with `model list`.

The genesis rows start in `Prefetching` and need 7 ready seats before they admit claims; the
lifecycle is in [testnet12-join-mining.md §8](testnet12-join-mining.md#8-the-model-lifecycle).

Do not copy a class ID from a retired network. A class is its graph and width: rows with similar
model names have different IDs.

## Full nodes

A full node validates class commitments and chain transitions without loading model weights. It
needs an artifact only when acting as a producer or panel seat for that class.

## Floor

Floor is an embedded deterministic integer class:

```bash
misaka --network testnet-12 mining setup --model floor
```

It needs no GPU, tokenizer or model file.

## Model classes

```bash
misaka --network testnet-12 model list
misaka --network testnet-12 mining setup --model <model-or-class-id> --artifact <file>
```

The setup walks node, model, key, funds, bond, artifact, seats and fee output, and resumes where it
stopped. `--verify-artifact` performs a full read and root verification of a large artifact. Getting
and sizing the 8k artifact is covered in
[testnet12-join-mining.md §6](testnet12-join-mining.md#6-start).

## Adding a model

```bash
misaka --network testnet-12 model add                                   # lists the catalog
misaka --network testnet-12 model add <catalog-model> --artifact <file>
misaka --network testnet-12 model add --manifest <class-manifest.json>  # a model the catalog lacks
```

The command resumes registration and family/lane certification from chain state. Copying a GGUF or
safetensors file into the node does not register a class. Start from
[palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md); the operator walkthrough is
[palw-add-a-model-runbook.md](palw-add-a-model-runbook.md).

## Panel/verifier

```bash
misaka --network testnet-12 verifier setup
misaka --network testnet-12 verifier status
```

A panel seat must be able to replay the selected class and therefore needs the matching artifact.
`misaka palw panel join --class <id> --artifact <file>` declares the class for a bond.

## Collateral

```bash
misaka --network testnet-12 bond status \
  --bond <txid>:<index> \
  --class-id <current-128-hex-class-id>
```

`--class-id` defaults to the floor class. Collateral sizing is in
[testnet12-join-mining.md §5](testnet12-join-mining.md#5-register-a-bond).

## Free-prompt lane

The free-prompt gateway is a separate service path ([palw-freeprompt-gateway.md](palw-freeprompt-gateway.md))
whose receipts can become PALW work. It is not required for ordinary attempt production; configure it
only after the class producer/panel path is healthy.
