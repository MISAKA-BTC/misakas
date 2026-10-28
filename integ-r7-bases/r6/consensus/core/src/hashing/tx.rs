use super::HasherExtensions;
use crate::TransactionHash;
use crate::tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_hashes::HasherBase;

bitflags::bitflags! {
    /// A bitmask defining which transaction fields we want to encode and which to ignore.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TxEncodingFlags: u8 {
        const FULL = 0;
        const EXCLUDE_SIGNATURE_SCRIPT = 1 << 0;
        const EXCLUDE_MASS_COMMIT = 1 << 1;
    }
}

/// Returns the transaction hash. Note that this is different than the transaction ID.
///
/// PR-9.5c: widened to [`TransactionHash`] (=[`kaspa_hashes::Hash64`])
/// per ADR-0008. The underlying digest now flows through the keyed
/// BLAKE2b-512 [`kaspa_hashes::TransactionHash64`] hasher.
pub fn hash(tx: &Transaction) -> TransactionHash {
    let mut hasher = kaspa_hashes::TransactionHash64::new();
    write_transaction(&mut hasher, tx, TxEncodingFlags::FULL);
    hasher.finalize()
}

/// Returns the transaction hash pre-crescendo (which excludes the mass commitment)
///
/// PR-9.5c: widened to [`TransactionHash`] (Hash64). Used by
/// pre-crescendo merkle-root computation paths only; current
/// merkle leaves go through [`hash`] above.
pub fn hash_pre_crescendo(tx: &Transaction) -> TransactionHash {
    let mut hasher = kaspa_hashes::TransactionHash64::new();
    write_transaction(&mut hasher, tx, TxEncodingFlags::EXCLUDE_MASS_COMMIT);
    hasher.finalize()
}

/// Not intended for direct use by clients. Instead use `tx.id()`
///
/// PR-9.5c: widened to [`TransactionId`] (Hash64). The underlying
/// digest is the keyed BLAKE2b-512 [`kaspa_hashes::TransactionId64`]
/// hasher; its key is `b"TransactionID64"` which is domain-
/// separated from the 32-byte legacy `TransactionID` key.
pub(crate) fn id(tx: &Transaction) -> TransactionId {
    // Encode the transaction, replace signature script with an empty array, skip
    // sigop counts and mass commitment and hash the result.

    let encoding_flags = if tx.is_coinbase() {
        TxEncodingFlags::FULL
    } else {
        TxEncodingFlags::EXCLUDE_SIGNATURE_SCRIPT | TxEncodingFlags::EXCLUDE_MASS_COMMIT
    };
    let mut hasher = kaspa_hashes::TransactionId64::new();
    write_transaction(&mut hasher, tx, encoding_flags);
    hasher.finalize()
}

/// Write the transaction into the provided hasher according to the encoding flags.
///
/// PR-9.5c: generic bound relaxed from `T: Hasher` (whose `finalize`
/// returns `Hash32`) to `T: HasherBase` so the function composes
/// against both 32-byte and 64-byte hashers. The inherent
/// `finalize` on the concrete `TransactionHash64` / `TransactionId64`
/// hashers (returning `Hash64`) is called at the outer site, not
/// through this generic.
fn write_transaction<T: HasherBase>(hasher: &mut T, tx: &Transaction, encoding_flags: TxEncodingFlags) {
    hasher.update(tx.version.to_le_bytes()).write_len(tx.inputs.len());
    for input in tx.inputs.iter() {
        // Write the tx input
        write_input(hasher, input, encoding_flags);
    }

    hasher.write_len(tx.outputs.len());
    for output in tx.outputs.iter() {
        // Write the tx output
        write_output(hasher, output);
    }

    hasher.update(tx.lock_time.to_le_bytes()).update(&tx.subnetwork_id).update(tx.gas.to_le_bytes()).write_var_bytes(&tx.payload);

    /*
       Design principles (mostly related to the new mass commitment field; see KIP-0009):
           1. The new mass field should not modify tx::id (since it is essentially a commitment by the miner re block space usage
              so there is no need to modify the id definition which will require wide-spread changes in ecosystem software).
           2. Coinbase tx hash should ideally remain unchanged

       Solution:
           1. Hash the mass field only for tx::hash
           2. Hash the mass field only if mass > 0
           3. Require in consensus that coinbase mass == 0

       This way we have:
           - Unique commitment for tx::hash per any possible mass value (with only zero being a no-op)
           - tx::id remains unmodified
           - Coinbase tx hash remains unchanged
    */

    if !encoding_flags.contains(TxEncodingFlags::EXCLUDE_MASS_COMMIT) {
        let mass = tx.mass();
        if mass > 0 {
            hasher.update(mass.to_le_bytes());
        }
    }
}

#[inline(always)]
fn write_input<T: HasherBase>(hasher: &mut T, input: &TransactionInput, encoding_flags: TxEncodingFlags) {
    write_outpoint(hasher, &input.previous_outpoint);
    if !encoding_flags.contains(TxEncodingFlags::EXCLUDE_SIGNATURE_SCRIPT) {
        hasher.write_var_bytes(input.signature_script.as_slice()).update([input.sig_op_count]);
    } else {
        hasher.write_var_bytes(&[]);
    }
    hasher.update(input.sequence.to_le_bytes());
}

#[inline(always)]
fn write_outpoint<T: HasherBase>(hasher: &mut T, outpoint: &TransactionOutpoint) {
    hasher.update(outpoint.transaction_id).update(outpoint.index.to_le_bytes());
}

#[inline(always)]
fn write_output<T: HasherBase>(hasher: &mut T, output: &TransactionOutput) {
    hasher
        .update(output.value.to_le_bytes())
        .update(output.script_public_key.version().to_le_bytes())
        .write_var_bytes(output.script_public_key.script());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        subnets::{self, SubnetworkId},
        tx::{ScriptPublicKey, scriptvec},
    };
    use kaspa_hashes::Hash64;
    use std::str::FromStr;

    // kaspa-pq Phase 9: TransactionHash / TransactionId widened to Hash64 per
    // ADR-0008. The pinned vectors below are the regenerated 64-byte (128 hex)
    // ids/hashes produced by the keyed BLAKE2b-512 `TransactionId64` /
    // `TransactionHash64` hashers.
    #[test]
    fn test_transaction_hashing() {
        struct Test {
            tx: Transaction,
            expected_id: &'static str,
            expected_hash: &'static str,
        }

        // Test #4's id is reused below as the spent-outpoint id for Test #5,
        // so it is hoisted into a single source of truth.
        const TX4_ID: &str = "727d3917c2abf660ed55b9b5c68da2938b8f94227b4b9e605b5e736cdb9d60fc36775b5097b44afc909f4a1bff7cc36c0821dbc0fa36a4b300165124854bfe51";

        let mut tests = vec![
            // Test #1
            Test {
                tx: Transaction::new(0, Vec::new(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
                expected_id: "de3dff94f79e131bec78363ee62be24fd960127034c8286f0efcbfd7b4d130ae3419970442a0e84532eec6fa67ce01793ce2569e23ba129ab033273c9e65142d",
                expected_hash: "c8f926e2f3d8f3844902f24e615424753130b9965e617187c1ee201adb786fb6cd9af1892a469888ef03411c2af9bf7b8ce6cfcf04855cae1abaa44e394039d7",
            },
        ];

        let inputs = vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(0), 2), vec![1, 2], 7, 5)];

        // Test #2
        tests.push(Test {
            tx: Transaction::new(1, inputs.clone(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
            expected_id: "48cf43b5bdb7e834e13d1d55bd1f8dfbefd96b160d78d08ec8879a6b80d56db7e466cd83d6e5895025351536348aab444ab10e6fc2028e5ad81bd4bcc9bbf8fb",
            expected_hash: "f4a9d3114996c9068bf68104f4b18b0463c44c834a5e7f26e602d070637076a1582fb1a57fecc281c4ff3b0d084ed7532bfdc9527c00ed12e141398e96004e57",
        });

        let outputs = vec![TransactionOutput::new(1564, ScriptPublicKey::new(7, scriptvec![1, 2, 3, 4, 5]))];

        // Test #3
        tests.push(Test {
            tx: Transaction::new(1, inputs.clone(), outputs.clone(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
            expected_id: "5e3ad73a2349bd603be1a131e64a8bb85388bf0e4dccc3a61e95b9047db6413ca8c8db52655e24eb59e723561fe95bd9a86ff94decbafa33add864cc75045f3b",
            expected_hash: "2c5ca3710d316556efc0ad77dbb87b8f8814b77fa03d1704342b5288e66a2bb21dd1b1b5c82094df51aefd745a1ba959cc589672179fa5f0b405671a17dd8c0f",
        });

        // Test #4
        tests.push(Test {
            tx: Transaction::new(2, inputs, outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new()),
            expected_id: TX4_ID,
            expected_hash: "5b5c98847adf6a062d3f9df3736d2c3c0aa31bfe1f64dc186091cdc3f5707e3b3eaae2ec80fcd113d28ec660e58e99c372baf433f02a1b74612cdc9e647de6fe",
        });

        let inputs = vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_str(TX4_ID).unwrap(), 2), vec![1, 2], 7, 5)];

        // Test #5
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new()),
            expected_id: "324217e88ab1b9b7a05af658f2893f40049e3413691b9afb6d70b8e868b53eec849139b1de1056234f73827bd156536e679c8542300613c7f27b0834f6384b70",
            expected_hash: "2ba8151a584c0d1a98a1eae8f3634db8ded5342f0429e92364ac7860104c06f5168f70176e09ee6d4b8cb8ddcb0706ed5c1d80e27c6541c50e72863c2728ce31",
        });

        // Test #6
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_COINBASE, 3, Vec::new()),
            expected_id: "eb1e942adede8e62f099be9e1b318b01b44223038f9b81b3f4dfcbe823f69d1b53abc6a03d972c384930d4164b3fb48962d51f3e3ae9074916a6d6e25b31e995",
            expected_hash: "df856bceab2d672ca5be654489f098ca0f7ae3208ecf26f1a321c2a39d7bd9c7f3dbb7ca508688161e30ca2ba1f190cbb132e32c99a1708bec997412a06911d1",
        });

        // Test #7
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, Vec::new()),
            expected_id: "fc8c0604292eb191b42210abaa3666e15f88d132caa30b970e90c54ef7e7f7055c7d0d586ee038a5156c18317051aacf0e51d352d038311c7e14938198a77547",
            expected_hash: "21f7f17dda8f72d137e0a4803553c6b913c043857e6ca3ad5d30d02551ada1472b7109cb7e00e7e404a08b89a35f49d73ab498534e3ef4041f5e0be4be29f268",
        });

        // Test #8, same as 7 but with a non-zero payload. The test checks id and hash are affected by payload change
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, vec![1, 2, 3]),
            expected_id: "53071aabb76fc87b99e4ad15d24d46276b81528b48534d8c315e30183019e0f2a9e25675c1410960be2656572d5ff8c302555b7b1fe0b835a192d11bfc1dd3b0",
            expected_hash: "e4a731388de5a26ff731685f629132ac0226a5f039cddee9c1bb8bb1ec23c88a138b21438916f673a20c539d27bb477e7466ab466bf07992084b987468a117b7",
        });

        for (i, test) in tests.iter().enumerate() {
            assert_eq!(test.tx.id(), Hash64::from_str(test.expected_id).unwrap(), "transaction id failed for test {}", i + 1);
            assert_eq!(hash(&test.tx), Hash64::from_str(test.expected_hash).unwrap(), "transaction hash failed for test {}", i + 1);
        }

        // Avoid compiler warnings on the last clone
        drop(inputs);
        drop(outputs);
    }
}
