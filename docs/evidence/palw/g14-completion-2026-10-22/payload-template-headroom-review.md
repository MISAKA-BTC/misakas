# Read-only template/headroom review (live run k10ph1lg)

No source/node/transaction operation performed. Source inspection plus a read-only live gzip snapshot; live observations are incomplete and not a final measurement report.

## Observed facts at this review snapshot

- Sender trace: root split 1 tx, compute 42,908 / storage 40,936 / transient 40,752; scalar 42,908.
- Second depth: 32 RPC-matching-accepted split txs; compute 31,750–32,547, storage 431,442–455,555, transient 35,880–36,228. Borsh DTO bytes 8,923–9,006, not P2P/network bytes.
- In the read-only raw getBlocks snapshot, DAA 4/5/6 blocks were LEGACY_FLOOR with two body transactions: coinbase plus one of those second-depth split transactions. The RPC storage commitment for that split was 455,555. This is carriage/header observation; selected-chain acceptance is separately recorded by the observer.
- Sender output polling had advanced to DAA 6 and 38/600 observed final outputs at elapsed ~150 s. Parent later observed DAA 8 and 57/600. Thus complete non-inclusion due to an invariant floor reservation is not established; there is actual progress.
- Temporary snapshot: /tmp/g14-payload-headroom-observations.json. It is metadata extracted from an unfinished live stream, not a pinned final artifact.

## Source budget path

1. kaspad/src/palw_producer.rs:1674–1680 obtains the ordinary MiningManager get_block_template. It does not append a separate floor body transaction after selection. At 2198–2201 it adds nonce and palw_commitment to the HEADER, finalizes, and submits the template body.
2. mining/src/block_template/builder.rs:100–117 passes the mempool selector factory into the consensus builder. consensus/src/pipeline/virtual_processor/processor/template.rs:328–331 derives the snapshot-dependent selector, obtains transactions, then at 414 onward validates actual transactions against the virtual UTXO view and rejects/refills invalid or deferred candidates.
3. mining/src/mempool/model/transactions_pool.rs:361–369: ordinary build_selector reserves the bounded H1 carrier lane first only when the carrier index has entries, then gives remaining block mass to the normal frontier. Empty lane gives the entire configured budget to normal selection. build_selector_within at 658 onward builds Policy(block_mass).
4. H1 priority is controlled by actual R-core+ policy (mining/src/mempool/config.rs:48), not by the block being a floor attempt. A separate native-carriage reservation path also exists (transactions_pool.rs:469 onward) when a native snapshot and eligible contract exist. Neither source path establishes a generic unconditional floor-body reservation.
5. mining/src/mempool/model/frontier.rs:203–228 selects TakeAll when everything fits; a large frontier uses SequenceSelector; the middle uses fee-weighted rebalancing. SequenceSelector (frontier/selectors.rs:193) skips candidates exceeding remaining scalar mass and looks for smaller ones. RebalancingWeightedTransactionSelector (block_template/selector.rs:149) stops the current episode if the selected candidate exceeds remaining scalar mass. There is no FIFO latency guarantee for any particular ordinary split tx.
6. Actual consensus mass checks are PER DIMENSION sums over all body transactions including coinbase: consensus/src/pipeline/body_processor/body_validation_in_isolation.rs:188–218. Each compute/transient/storage total must fit max_block_mass; header palw_commitment is not iterated as a body transaction by this function. Params devnet max_block_mass is 500,000, standard individual mass ceiling is 480,000 (mining/src/mempool/check_transaction_standard.rs:51).

## Consequences and limits

- Any two second-depth split transactions exceed 500,000 storage mass (minimum sum 862,884), so they cannot share one block under the existing rule regardless of fee or floor traffic. At best one such split can be carried in a block. The source and observed single-split blocks are consistent with a preparation queue serviced roughly one large split per block; this is not a measured lower-bound rate or deadline guarantee.
- A single split leaves only 44,445–68,558 STORAGE mass before accounting for other body transactions. Normal selection may put smaller transactions into other remaining dimensions, and ordinary fee-weighting can delay a particular split. We did not derive full background compute/transient mass from RPC bodies, so this is not a complete independent block mass audit.
- Earlier reported 111,927-byte background observations cannot be subtracted from a 500,000 mass budget, nor compared with current Borsh DTO bytes. Current DAA4–6 raw bodies did not contain that amount of extra floor lifecycle traffic; later Panel traffic can change the composition.
- Confirmation polling reads DAG and UTXO RPCs separately; an RPC success is not a confirmation. No cap, fee, timeout, fence or funding change is proposed. Keep the run and original records intact; final throughput and queue analysis must use the full actual trace.
