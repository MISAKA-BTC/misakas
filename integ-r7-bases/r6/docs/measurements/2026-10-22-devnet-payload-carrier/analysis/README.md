# Narrow independent mass/fee rederivation candidate (offline only)

Prepared in /tmp while the run was live. No node, RPC, transaction send, native executable, source edit or completed live aggregation was performed. Eight synthetic Python tests pass. Those signatures are intentionally zero-filled and not cryptographically verified.

## Scope

recalc.py handles ONLY full txids listed in this sender's split/load submit trace, one ordinary input, native subnetwork, version0, locktime/gas0, sequence u64::MAX, sigop1, exact MLDSA87 PUSHDATA2 signature+sighash/pubkey grammar, and 1..32 ordinary v0 P2PKH-MLDSA87 outputs. It is neither a generic native-transaction auditor nor an authenticated UTXO/whole-node validator. No signature verification or mining/chain acceptance decision occurs.

Read final observer JSONL once (plain/gzip/xz/lzma supported), collect sender body observations by FULL txid from getMempoolEntries/getBlocks, recompute the native transaction ID, and require byte-identical full Borsh body on repeated observations of that ID. Signature-only or mass-only variants sharing txid become UNKNOWN. Missing body/input is UNKNOWN. Distinct sender transactions reusing one reconstructed input become UNKNOWN. No entry is manufactured from a fee/paid boolean.

Inputs begin with the launcher's preserved observed-funded-root.json output and are reconstructed through each observed split/load output keyed by (fulltxid,index). This is a candidate transaction graph anchored to separate RPC observations, not native UTXO provenance or an atomic state snapshot. Every input value and output sum is checked u64; actual in-out is independently compared with sender fee_sompi and any raw mempool fee observations. Reconstructed outputs are not a certification that those transactions were accepted by consensus.

## Exact source formulas

- consensus/core/src/tx.rs:177 Transaction derived Borsh field order, including mass AtomicU64 serialization (170) AND cached64-byte transaction ID. Vectors use u32 lengths. Input fields are outpoint64+u32, sigVec, sequenceu64, sigopu8. Output is valueu64 + SPK(versionu16, vec). SPK human RPC hex prefixes two version bytes BIG endian; Borsh version is LITTLE endian (tx/script_public_key.rs:81,358).
- consensus/core/src/hashing/tx.rs:44–127 plus hashing/mod.rs:48,84: keyed Blake2b512 TransactionID64, u64 vector/byte lengths. Ordinary ID excludes signature scripts, sigop and mass, unlike full Borsh body. These are identity hashes only.
- consensus/core/src/mass/mod.rs:13–60 native deterministic estimated_size, NOT Borsh length. It uses u64 collection lengths, payload_hash32 and no cached txid. Compute at273 = size*mass_per_tx_byte + sum(2+SPKscriptlength)*mass_per_script_pub_key_byte + sigops*mass_per_sig_op. Transient=size*4.
- mass/mod.rs:95 UTXO plurality = ceil((95+scriptlength)/100). Ordinary69-byte SPK has plurality2. calc_storage_mass at363–474 uses checked harmonic output terms C*p^2/value. For one-input/one-output PQ, relaxed subtraction is C*p_in^2/amount. For multiple outputs, arithmetic debit is p_in*(C//(inputamount//p_in)), preserving native integer rounding; saturation and zero-divisor refusal mirror source.
- Params devnet mass_per_tx_byte1, script_pub_key_byte10, sig_op10000, storage C=SOMPI_PER_KASPA*10000=10^12, transient factor4. params-source-candidate.json carries source-file SHA256; it must be compared to the run's pinned native source/build manifest. It is not proof of runtime config.

Exact signed Borsh DTO bytes remain distinct from P2P framing, block-network encoding and the native mass estimator. E.g. the synthetic one-in/one-out fixture has Borsh length7512 but estimated_size7491; they are deliberately tested as different.

## Run only AFTER final evidence is frozen and reviewed

python3 /tmp/g14-payload-independent-recalc/recalc.py --sender FINAL/sender-events.jsonl --observer FINAL/observer.jsonl.xz --funded-root FINAL/observed-funded-root.json --params /tmp/g14-payload-independent-recalc/params-source-candidate.json

No command above was executed against this real run. Lossless alternate compression preserves record content; never rewrite the original evidence. Corrupt/incomplete compressed data gives INCOMPLETE_EVIDENCE, never a successful partial audit. Required metadata: pinned native binary+source SHA, actual launch argv/network/params identity, observed root entry, full sender submit trace and final observer bodies. Unobserved failed/unknown submitted raw signed bodies and full original input snapshots are not in current sender trace; report those missing rather than completing them from predicted bytes.

## Missing outcomes and independent RPC comparisons

Durable submit_attempt identities are included even when no result exists. An unmatched attempt/retry remains UNKNOWN, including when its body later appears; the metric rederivation is never a submission/admission certificate. Repeated results for the same full txid must agree on sender metric fields or remain UNKNOWN, rather than silently keeping the first record. Positive native RPC verboseData.computeMass values are independently compared; absent/None/zero verbose compute mass is unavailable and is not compared as actual zero. The caller must separately retain original raw trace and observer records for each reported disagreement.

Parent verified one already-observed root as an independent live golden check (not final run aggregation): native ID, Borsh10085, compute42908, transient40752, storage40936, fee536350 matched. Its retained /tmp/g14-first-split-independent-check.json is external evidence; this candidate does not replace or modify it. Full-run aggregation remains deferred until final evidence freeze/review.
