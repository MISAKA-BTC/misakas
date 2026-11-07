// kaspa-pq Phase 7 (PR-7.5) — TypeScript SDK example.
//
// Mirror of `rpc/wrpc/examples/kaspa_pq_send/src/main.rs`, expressed
// through the kaspa-pq WASM bindings. Assumes the package has been
// built with one of `--features wasm32-sdk|wasm32-core|wasm32-keygen`
// and the resulting npm package is reachable as `kaspa-pq`.
//
// Demonstrates:
//
//   1. BIP39 mnemonic + kaspa-pq path -> ML-DSA-65 keypair
//      via KaspaPqKeyPair.fromMnemonic.
//   2. kaspa-pq P2PKH address via KaspaPqKeyPair.address.
//   3. Sign a 32-byte sighash digest with the kaspa-pq tx context.
//   4. MlDsa65Signature.verify against the public key.
//
// Submission against a live node is a Phase 5' follow-up (the wRPC
// client surface is part of the same npm package).
//
// Run with (e.g. Node):
//     ts-node derivation.ts

import {
    KaspaPqKeyPair,
    MlDsa65PublicKey,
    MlDsa65Signature,
    // The `Address` class comes from the same kaspa-pq SDK build.
    Address,
} from "kaspa-pq";

// 12 / 24-word phrases work; the kaspa-pq spec recommends 24.
const TEST_MNEMONIC =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

function bytesToHex(bytes: Uint8Array): string {
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function main(): void {
    console.log("=== kaspa-pq Phase 7 PR-7.5 TS SDK example ===\n");

    // 1. Derive keypair.
    const keypair: KaspaPqKeyPair = KaspaPqKeyPair.fromMnemonic(
        TEST_MNEMONIC,
        /* passphrase  */ "",
        /* networkId   */ "simnet",
        /* account     */ 0,
        /* change      */ 0,
        /* index       */ 0,
    );

    // 2. Public key + kaspa-pq address.
    const publicKey: MlDsa65PublicKey = keypair.publicKey();
    const publicKeyBytes: Uint8Array = publicKey.toBytes();
    console.log(`Step 1-2: derived ML-DSA-65 keypair`);
    console.log(`          public_key.length  = ${publicKeyBytes.length}`);

    const address: Address = keypair.address("kaspapqsim");
    // Address has a toString() method coming from the kaspa-addresses WASM
    // class. Output looks like `kaspapqsim:qgr5dcr8yeq0wpe59...`.
    console.log(`          kaspa-pq simnet address = ${address.toString()}`);

    // 3. Sign a placeholder 32-byte sighash. Real-world: pass the
    //    calc_schnorr_signature_hash output from the kaspa-pq consensus
    //    sighash path here.
    const sighash = new Uint8Array(32).fill(0xa5);
    const randomness = new Uint8Array(32).fill(0x77);
    const signature: MlDsa65Signature = keypair.sign(sighash, randomness);
    const sigBytes: Uint8Array = signature.toBytes();
    console.log(
        `\nStep 3: signed placeholder sighash, signature.length = ${sigBytes.length}`,
    );
    console.log(`        signature.toHex().slice(0, 32) = ${bytesToHex(sigBytes).slice(0, 32)}...`);

    // 4. Local verify. The MlDsa65Signature.verify method binds the kaspa-pq
    //    tx context internally — callers do not pass a context string.
    const ok: boolean = signature.verify(publicKey, sighash);
    if (!ok) {
        throw new Error("kaspa-pq verify failed under MLDSA65_TX_CONTEXT");
    }
    console.log(`Step 4: local verify OK under MLDSA65_TX_CONTEXT.`);

    // 5. Demonstrate that a tampered message verifies as false.
    const tampered = new Uint8Array(32).fill(0xa6); // differs by one byte
    const tamperedOk: boolean = signature.verify(publicKey, tampered);
    if (tamperedOk) {
        throw new Error("kaspa-pq verify should have rejected the tampered message");
    }
    console.log(`Step 5: tampered-message verify correctly returned false.`);

    console.log(
        `\n(Submitting a real signed transaction against live UTXOs is a Phase 5'`,
    );
    console.log(`continuation — see docs/adr/0006-rpc-wasm-sdk-types.md §1.)`);
}

main();
