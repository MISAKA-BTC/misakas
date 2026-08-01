//! PALW per-block ticket-authorization producer (ADR-0040 P1-6, AUTH-01/02/03).
//!
//! # Purpose
//!
//! Each algo-4 block carries an ML-DSA-87 authorization signed by the leaf's declared ticket authority.
//! It binds the parents, transaction set, timestamp, and ticket coordinates to prevent a disclosed
//! winning ticket from being transferred to another block.
//!
//! # Construction == validation
//!
//! The producer uses the same binding functions as consensus validation:
//!
//! | produced here | checked by clause 7 |
//! |---|---|
//! | [`TicketAuthority::pk_hash`] | `PalwBlockAuthorizationV1::binds_leaf_authority` (AUTH-03) |
//! | `header_preimage_commitment` via `palw_header_preimage_commitment` over the WHOLE header | `PalwBlockAuthorizationV1::binds_header` (AUTH-02) |
//! | `signature` under [`PALW_AUTHORIZATION_MLDSA87_CONTEXT`] over `signing_hash` | `verify_mldsa87_with_context` (AUTH-01) |
//! | [`BlockAuthorization::authorization_hash`] | `auth.hash() == header.palw_authorization_hash` |
//! | [`BlockAuthorization::payload`] on subnetwork `0x38` | the body's authorization-tx lookup + `validate_palw_overlay_payload` |
//!
//! The `ctx` matters as much as the digest: FIPS-204 binds the context into the signature, so signing
//! under anything but [`PALW_AUTHORIZATION_MLDSA87_CONTEXT`] yields an authorization consensus cannot
//! verify (and, by construction, one that can never be replayed as a beacon or an audit vote).
//!
//! # What the caller must hand over, and how final it must be
//!
//! ADR-0040 (AUTH-02) made the binding TOTAL: the commitment is the block's OWN canonical header
//! preimage — the exact bytes `write_header_preimage` emits — under the disjoint `PalwAuthPreimageHash64`
//! domain, with exactly two substitutions (`palw_authorization_hash := 0`, `hash_merkle_root :=
//! authed_root`). It replaced a hand-picked 9-scalar allowlist under which every unlisted header field
//! was a free variation axis, and this producer follows it: it passes the WHOLE [`Header`] through to
//! the consensus function rather than re-describing a subset of it. A subset is how the original hole
//! was created, and a parallel serializer here is exactly the drift the upstream fix removed.
//!
//! Three consequences for the assembler:
//!
//! 1. [`BlockAuthorizationBinding::authed_hash_merkle_root`] is the merkle root over the block's
//!    transactions **EXCLUDING the authorization transaction itself** — the authorization cannot commit
//!    to a root that contains the authorization, that is circular. The node assembling the block owns
//!    the transaction set, so it computes that root and passes it in; after appending the returned
//!    payload it recomputes the header's own `hash_merkle_root` over ALL transactions.
//! 2. EVERY OTHER HEADER FIELD MUST BE FINAL before [`TicketAuthority::authorize`] is called.
//!    Retargeting the coinbase, adding a parent at any level, or recomputing any virtual-derived
//!    commitment (`utxo_commitment`, `accepted_id_merkle_root`, `pruning_point`,
//!    `overlay_commitment_root`, `palw_beacon_seed`) after that point invalidates the signature and
//!    wastes the ticket draw. That is the intended cost of a total binding.
//! 3. THE CARRYING TRANSACTION HAS A CANONICAL SHAPE, and the assembler must NOT build it by hand.
//!    Use [`BlockAuthorization::carrying_transaction`], which delegates to the single consensus-core
//!    encoder `kaspa_consensus_core::palw::build_palw_authorization_transaction`. ADR-0040
//!    (AUTH-TXSHAPE) pins every field of that transaction: `version == TX_VERSION`, **zero** inputs,
//!    **zero** outputs, `lock_time == 0`, `gas == 0`, and a declared `mass()` of `0`, plus the payload
//!    must be the exact borsh encoding (a round-trip equality, so no trailing bytes). It is a CONSENSUS
//!    rule (`check_palw_block_authorization_shape`, in validation-in-isolation), not a convention, and
//!    it is checked before body validation runs. Why it exists: the bound merkle root above
//!    deliberately excludes this transaction, and `auth.hash()` covers only the payload, so any field
//!    left free here would be a variation axis that changes the block hash while clause 7 still passes
//!    — one authorization would again bind an unbounded class of blocks, which is the exact hole
//!    AUTH-02 closes. Routing every producer through one encoder is what makes that structural rather
//!    than a rule each assembler has to remember.

use kaspa_consensus_core::constants::{PALW_ANTISPAM_HEADER_VERSION, PALW_COMPUTE_SET_HEADER_VERSION, PALW_HEADER_VERSION};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw::{
    PALW_AUTHORIZATION_DOMAIN, PALW_AUTHORIZATION_MLDSA87_CONTEXT, PalwBlockAuthorizationV1, palw_header_preimage_commitment,
};
use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA;
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use kaspa_pq_validator_core::ValidatorKey;

/// The `0x38` subnetwork byte a ticket-authorization PALW TX carries (mirrors
/// `PalwTxKind::from_subnetwork_byte(0x38) == BlockAuthorization` and
/// `SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION`).
pub const BLOCK_AUTHORIZATION_SUBNETWORK_BYTE: u8 = 0x38;

/// The `ticket_authority_pk_hash` a leaf must publish for `public_key` to be able to authorize its
/// blocks — the exact preimage `PalwBlockAuthorizationV1::binds_leaf_authority` recomputes (AUTH-03).
///
/// A leaf whose `ticket_authority_pk_hash` is anything else (a placeholder, or the hash of a key the
/// miner does not hold) can never win a block: it may be admitted on-chain, but every algo-4 block
/// referencing it fails clause 7.
pub fn ticket_authority_pk_hash(public_key: &[u8]) -> Hash64 {
    blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, public_key)
}

/// Everything the authorization binds, resolved by the node from the block it is about to mint.
///
/// ADR-0040 AUTH-02: the binding carries the complete header rather than enumerating selected fields.
/// Any field included
/// in the block hash is therefore included in the authorization commitment.
#[derive(Clone, Debug)]
pub struct BlockAuthorizationBinding {
    /// The consensus PALW network number (`params.net.suffix()`), as used by every PALW preimage.
    pub network_id: u32,
    /// Complete header of the block being authorized, with every field final except the
    /// two the commitment necessarily substitutes: `palw_authorization_hash` (circular — it is the hash
    /// of the object being built) and `hash_merkle_root` (circular — the real root covers the
    /// authorization tx, whose payload carries this commitment). Both may hold any value here; the
    /// commitment ignores them in favour of zero and `authed_hash_merkle_root` respectively.
    ///
    /// The ticket coordinates the authorization declares — `batch_id`, `leaf_index`,
    /// `ticket_nullifier` — are read from this header rather than supplied separately, because
    /// `binds_header` compares the authorization's copies against the header's. Taking them from one
    /// source makes that comparison unfailable by construction.
    pub header: Header,
    /// Merkle root over the block's transactions **excluding** the authorization tx (see module docs).
    pub authed_hash_merkle_root: Hash64,
}

/// A signed authorization ready to ride in the block it authorizes.
#[derive(Clone, Debug)]
pub struct BlockAuthorization {
    /// [`BLOCK_AUTHORIZATION_SUBNETWORK_BYTE`] — the subnetwork the carrying transaction declares.
    pub subnetwork_byte: u8,
    /// `borsh(auth)` — the bytes the `0x38` transaction carries as its payload.
    pub payload: Vec<u8>,
    /// The value the minted header must stamp into `palw_authorization_hash`, so the header cannot
    /// name one authorization while the body carries another.
    pub authorization_hash: Hash64,
    pub auth: PalwBlockAuthorizationV1,
}

/// Why [`TicketAuthority::authorize`] can refuse.
///
/// ADR-0040 (AUTH-02): signing is the LAST irreversible step of a mint, and every one of these
/// conditions produces a signature over a header the node's own validator will reject. Returning them
/// as errors rather than signing anyway is the difference between "this miner declines to mint" and
/// "this miner mints blocks it then rejects", which on a shared network is indistinguishable from an
/// attack for anyone reading the logs.
///
/// Each variant corresponds to a rejection the block would earn: a non-v3/v4/v5 header or a non-algo-4
/// lane never reaches the authorization clause at all; a nonce not pinned to `low64(nullifier)` fails the
/// eligibility draw (I-3); and an authority that is not the leaf's declared one fails
/// `binds_leaf_authority` (AUTH-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AuthorizeError {
    #[error(
        "header version {0} is not an authorizable PALW header version — only v3, re-genesis v4, or \
         Compute-Set-registry v5 is supported"
    )]
    NotPalwHeader(u16),
    #[error("header pow_algo_id {0} is not the PALW replica lane — only algo-4 blocks carry a ticket authorization")]
    NotReplicaLane(u8),
    #[error(
        "header.palw_authorization_hash is already stamped — signing now would authorize a header that names a different authorization"
    )]
    AuthorizationHashAlreadyStamped,
    #[error("header.nonce {got} is not pinned to low64(ticket_nullifier) {expected} — the draw would fail (I-3)")]
    NonceNotPinnedToNullifier { expected: u64, got: u64 },
    #[error("this authority is not the leaf's declared ticket authority (AUTH-03)")]
    NotThisLeafsAuthority { expected: Hash64, got: Hash64 },
}

/// A ticket authority: the ML-DSA-87 key whose public-key hash a leaf publishes as
/// `ticket_authority_pk_hash`, and which alone can authorize blocks spending that leaf's ticket.
///
/// The key is the whole point of clause 7 — the observer of a winning block has the nullifier, the
/// leaf and the chain commit, but not this. Keep it where the mining loop runs.
pub struct TicketAuthority {
    key: ValidatorKey,
}

impl TicketAuthority {
    pub fn new(key: ValidatorKey) -> Self {
        Self { key }
    }

    /// Derive an authority from a 32-byte seed (same derivation as every other ML-DSA-87 role key).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self::new(ValidatorKey::from_seed(seed))
    }

    /// The raw ML-DSA-87 verification key carried verbatim in the authorization.
    pub fn public_key(&self) -> &[u8] {
        self.key.public_key()
    }

    /// The value to put in [`crate::ProviderRegistration::ticket_authority_pk_hash`] (and therefore in
    /// every minted leaf) so that this authority can authorize the leaf's blocks — AUTH-03.
    pub fn pk_hash(&self) -> Hash64 {
        ticket_authority_pk_hash(self.public_key())
    }

    /// Sign the per-block authorization for `binding`, refusing any binding whose header this authority
    /// must not sign — see [`AuthorizeError`].
    ///
    /// Pure over `(key, binding)` apart from ML-DSA's hedged signing randomness: the DIGEST is
    /// deterministic, so a verifier re-deriving it from the block gets the identical value.
    ///
    /// Prefer [`Self::authorize_for_leaf`] on any path that has the leaf in hand: it additionally
    /// enforces AUTH-03 *before* the signature, so a miner holding the wrong key finds out at the point
    /// of decision rather than after publishing a block the network rejects.
    pub fn authorize(&self, binding: &BlockAuthorizationBinding) -> Result<BlockAuthorization, AuthorizeError> {
        // Refuse before signing. Each check mirrors a rejection the minted block would earn, so a
        // signature is only ever produced over a header that can actually be accepted.
        // v5 (ADR-MA Compute Set registry) is authorizable for the same reason v4 was: the header
        // preimage grew by activation-gated fields that `write_header_preimage` already covers, so
        // the signed digest is still the block's own canonical header. testnet-20 is a v5 genesis —
        // gating at v4 here refused every algo-4 block the registry network can ever build.
        if !(PALW_HEADER_VERSION..=PALW_COMPUTE_SET_HEADER_VERSION).contains(&binding.header.version) {
            return Err(AuthorizeError::NotPalwHeader(binding.header.version));
        }
        if binding.header.pow_algo_id != POW_ALGO_ID_PALW_REPLICA {
            return Err(AuthorizeError::NotReplicaLane(binding.header.pow_algo_id));
        }
        // The commitment zeroes this field, so a stamped value does not change what is signed — but it
        // means the caller is holding a header that already names some OTHER authorization. Signing it
        // would produce two authorizations for one header, and only one of them can match clause 7.
        if binding.header.palw_authorization_hash != Hash64::default() {
            return Err(AuthorizeError::AuthorizationHashAlreadyStamped);
        }
        let expected_nonce = crate::mining::pinned_nonce(&binding.header.palw_ticket_nullifier);
        if binding.header.nonce != expected_nonce {
            return Err(AuthorizeError::NonceNotPinnedToNullifier { expected: expected_nonce, got: binding.header.nonce });
        }

        // Construction == validation: this is the SAME consensus function `binds_header` recomputes,
        // over the SAME header object. There is no second serializer here to drift from it.
        let header_preimage_commitment =
            palw_header_preimage_commitment(binding.network_id, &binding.header, &binding.authed_hash_merkle_root);
        let mut auth = PalwBlockAuthorizationV1 {
            version: 1,
            batch_id: binding.header.palw_batch_id,
            leaf_index: binding.header.palw_leaf_index,
            ticket_nullifier: binding.header.palw_ticket_nullifier,
            header_preimage_commitment,
            // INCLUDED in the signing preimage by `signing_hash`, so a signature cannot be
            // re-presented under a substituted key.
            authority_public_key: self.public_key().to_vec(),
            signature: Vec::new(),
        };
        let digest = auth.signing_hash(binding.network_id);
        auth.signature = self.key.sign_with_context(digest.as_bytes().as_slice(), PALW_AUTHORIZATION_MLDSA87_CONTEXT).to_vec();
        let authorization_hash = auth.hash();
        let payload = borsh::to_vec(&auth).expect("borsh encoding of a fixed-shape authorization is infallible");
        Ok(BlockAuthorization { subnetwork_byte: BLOCK_AUTHORIZATION_SUBNETWORK_BYTE, payload, authorization_hash, auth })
    }

    /// [`Self::authorize`] plus the AUTH-03 precondition: this authority MUST be the one the leaf
    /// published, checked against `leaf_ticket_authority_pk_hash` *before* the signature.
    ///
    /// Consensus enforces the same equality at body clause 7 (`binds_leaf_authority`), so skipping it
    /// here does not make an invalid block valid — it only wastes the ticket. A PALW ticket is one draw
    /// on one registered leaf and cannot be re-rolled (the nullifier commitment is fixed on-chain at
    /// registration), so minting with the wrong key does not cost a retry: it costs the ticket.
    pub fn authorize_for_leaf(
        &self,
        binding: &BlockAuthorizationBinding,
        leaf_ticket_authority_pk_hash: &Hash64,
    ) -> Result<BlockAuthorization, AuthorizeError> {
        let ours = self.pk_hash();
        if ours != *leaf_ticket_authority_pk_hash {
            return Err(AuthorizeError::NotThisLeafsAuthority { expected: *leaf_ticket_authority_pk_hash, got: ours });
        }
        self.authorize(binding)
    }
}

impl BlockAuthorization {
    /// The canonical carrying transaction for this authorization.
    ///
    /// Delegates to the single consensus-core encoder
    /// ([`kaspa_consensus_core::palw::build_palw_authorization_transaction`]) rather than assembling a
    /// transaction here, so the producer cannot drift from `check_palw_block_authorization_shape` or
    /// body clause 7. Append it to the block body as the LAST transaction and change nothing after.
    pub fn carrying_transaction(&self) -> kaspa_consensus_core::tx::Transaction {
        kaspa_consensus_core::palw::build_palw_authorization_transaction(&self.auth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mining::{EligibilityContext, grind_eligibility, nullifier_sequence};
    use crate::registration::restamp_leaves;
    use crate::{MiningJob, PalwMiner, ProviderRegistration};
    use kaspa_consensus_core::BlueWorkType;
    use kaspa_consensus_core::constants::PALW_HEADER_VERSION;
    use kaspa_consensus_core::header::PalwHeaderFields;
    use kaspa_consensus_core::palw::{PalwPublicLeafV1, validate_palw_overlay_payload};
    use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_txscript::verify_mldsa87_with_context;
    use misaka_palw::palw::{PalwRuntimeProfileV1, PalwSamplingParams, PalwTier};
    use misaka_palw::palw_replica::MockDeterministicRuntime;

    const NET: u32 = 0x9107;
    /// Max-easy genesis lane bits (`DEVNET_PALW`/`TESTNET_PALW`) — a handful of candidates wins.
    const EASY_BITS: u32 = 0x207fffff;
    const AUTHORITY_SEED: [u8; 32] = [0x9a; 32];

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn profile() -> PalwRuntimeProfileV1 {
        PalwRuntimeProfileV1 {
            version: 1,
            tier: PalwTier::Quality,
            model_id: PalwTier::Quality.model_id(),
            tokenizer_hash: h(1),
            quantization_manifest_hash: h(2),
            runtime_image_hash: h(3),
            kernel_graph_hash: h(4),
            operation_table_hash: h(5),
            shape_table_hash: h(6),
            gpu_arch_class: 100,
            tensor_parallel_degree: 1,
            pipeline_parallel_degree: 1,
            deterministic_reduction: true,
            batch_invariant: true,
            speculative_decode: false,
            sampling: PalwSamplingParams::greedy(),
        }
    }

    /// A miner whose registration names `authority` — the AUTH-03 link between the key held by the
    /// mining loop and the `ticket_authority_pk_hash` published in every leaf it mints.
    fn miner(authority: &TicketAuthority) -> PalwMiner<MockDeterministicRuntime, MockDeterministicRuntime> {
        // ADR-0040 P0-4 (ECON-01): a leaf's reward scripts are emitted VERBATIM as coinbase outputs, so
        // leaf admission requires the exact 69-byte P2PKH ML-DSA-87 template. An arbitrary script is not
        // coinbase-representable and the leaf chunk is rejected.
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0xa0; 64]);
        PalwMiner::new(
            MockDeterministicRuntime::new(profile(), 3, 2),
            MockDeterministicRuntime::new(profile(), 3, 2),
            ProviderRegistration {
                provider_a_bond: TransactionOutpoint::new(h(6), 0),
                provider_b_bond: TransactionOutpoint::new(h(7), 0),
                provider_a_reward_script: spk.clone(),
                provider_b_reward_script: spk,
                ticket_authority_pk_hash: authority.pk_hash(),
                registered_epoch: crate::registration::tests::FIXTURE_REGISTRATION_EPOCH,
                activation_epoch: 4,
                expiry_epoch: 1000,
                leaf_bond_sompi: 0,
            },
        )
    }

    fn base_leaf(authority: &TicketAuthority, batch_id: Hash64) -> PalwPublicLeafV1 {
        miner(authority)
            .produce_leaf(&MiningJob {
                batch_id,
                leaf_index: 0,
                job_set_descriptor: b"job".to_vec(),
                prompt: b"prompt".to_vec(),
                output_salt: [0x33; 32],
                job_nullifier: h(0x20),
                raw_ticket_nullifier: h(0),
            })
            .unwrap()
            .leaf
    }

    fn ctx() -> EligibilityContext {
        EligibilityContext {
            network_id: NET,
            beacon_seed: h(0xBE),
            chain_commit: h(0xCC),
            target_daa_interval: 42,
            replica_bits: EASY_BITS,
        }
    }

    /// A complete, realistic v3 algo-4 header — every field populated with a DISTINCT value so that any
    /// mutation in the reject test is genuinely observable in the preimage. Two parent levels, differing
    /// from each other (identical consecutive levels are run-length-collapsed by `CompressedParents`, so
    /// a level >= 1 is only separately mutable when it differs from level 0).
    fn header(batch_id: Hash64, nullifier: Hash64, c: &EligibilityContext) -> Header {
        Header::new_finalized(
            PALW_HEADER_VERSION,
            vec![vec![h(0x71), h(0x72)], vec![h(0x90)]].try_into().unwrap(),
            // `hash_merkle_root` is one of the two fields the commitment substitutes (it is replaced by
            // `authed_hash_merkle_root`), so its value here is deliberately irrelevant.
            h(0x7F),
            h(0xA1), // accepted_id_merkle_root
            h(0xA2), // utxo_commitment
            1_700_000_000_000,
            EASY_BITS,
            // PALW pins the nonce to low64(nullifier) so the fixture satisfies the clause-9 draw.
            crate::mining::pinned_nonce(&nullifier),
            POW_ALGO_ID_PALW_REPLICA,
            7_000, // daa_score
            BlueWorkType::from(11u64),
            5_000,   // blue_score
            h(0xA3), // pruning_point
        )
        .with_overlay_commitment(h(0xA4))
        .with_palw_fields(PalwHeaderFields {
            blue_hash_work: BlueWorkType::from(3u64),
            blue_compute_work: BlueWorkType::from(9u64),
            palw_batch_id: batch_id,
            palw_leaf_index: 0,
            palw_ticket_nullifier: nullifier,
            palw_epoch_certificate_hash: h(0xC7),
            palw_chain_commit: c.chain_commit,
            palw_target_daa_interval: c.target_daa_interval,
            // Zeroed by the commitment (it is the hash of the object being built), so any value works;
            // the assembler stamps the real one after `authorize` returns.
            palw_authorization_hash: Hash64::default(),
            palw_proof_type: 1,
            palw_beacon_seed: c.beacon_seed,
            palw_spam_accumulator_commitment: Hash64::default(),
            palw_spam_nonce: 0,
            palw_compute_set_id: Default::default(),
            palw_compute_policy_id: Default::default(),
            palw_allocation_plan_id: Default::default(),
        })
    }

    /// The full producer path, exactly as the node drives it: mint a leaf under the authority, grind a
    /// winning ticket, then authorize the block that would carry it.
    fn produce() -> (TicketAuthority, PalwPublicLeafV1, BlockAuthorizationBinding, BlockAuthorization) {
        let authority = TicketAuthority::from_seed(AUTHORITY_SEED);
        let batch_id = h(0x10);
        let leaf = restamp_leaves(batch_id, &[base_leaf(&authority, batch_id)]).remove(0);
        let c = ctx();
        let ticket = grind_eligibility(&c, &leaf, nullifier_sequence(b"authority-secret", 128)).expect("easy target wins");
        let binding = BlockAuthorizationBinding {
            network_id: NET,
            header: header(batch_id, ticket.raw_nullifier, &c),
            authed_hash_merkle_root: h(0x80),
        };
        // The fixture's header is a well-formed v3 algo-4 header with the nonce pinned to the winning
        // nullifier, so every `AuthorizeError` precondition holds; a failure here is a fixture bug.
        let authorized = authority.authorize(&binding).expect("the fixture header must be authorizable");
        (authority, ticket.leaf, binding, authorized)
    }

    /// Construction and validation: accepted input. Everything body clause 7 checks is checked here
    /// against the produced authorization — using the consensus functions themselves, not a
    /// re-implementation: the payload is isolation-valid on `0x38`, the header hash matches, the
    /// binding matches, the key is the leaf's declared authority, and the ML-DSA-87 signature verifies
    /// under the verifier's own context.
    #[test]
    fn miner_authorization_satisfies_every_clause_7_check() {
        let (authority, leaf, binding, authorized) = produce();

        // Isolation validity of the carrying 0x38 transaction payload.
        assert_eq!(authorized.subnetwork_byte, BLOCK_AUTHORIZATION_SUBNETWORK_BYTE);
        assert_eq!(validate_palw_overlay_payload(authorized.subnetwork_byte, &authorized.payload), Ok(()));
        // The payload really decodes back to the object we signed (the body decodes it, not us).
        let decoded = <PalwBlockAuthorizationV1 as borsh::BorshDeserialize>::try_from_slice(&authorized.payload).expect("decode");
        assert_eq!(decoded, authorized.auth);

        // `auth.hash() == header.palw_authorization_hash`.
        assert_eq!(decoded.hash(), authorized.authorization_hash);

        // AUTH-02: the authorization is ABOUT this block.
        assert!(decoded.binds_header(binding.network_id, &binding.header, &binding.authed_hash_merkle_root));

        // The commitment must survive the assembler stamping the authorization hash into the header —
        // that field is zeroed by the commitment precisely so this step is not circular.
        let mut stamped = binding.header.clone();
        stamped.palw_authorization_hash = authorized.authorization_hash;
        stamped.finalize();
        assert!(
            decoded.binds_header(binding.network_id, &stamped, &binding.authed_hash_merkle_root),
            "stamping palw_authorization_hash must not invalidate the authorization it names"
        );

        // AUTH-03: the signing key is the authority the LEAF declared — the link that makes
        // `ticket_authority_pk_hash` mean something.
        assert_eq!(leaf.ticket_authority_pk_hash, authority.pk_hash());
        assert!(decoded.binds_leaf_authority(&leaf.ticket_authority_pk_hash));

        // AUTH-01: the exact verification the body performs, same digest, same ctx.
        assert_eq!(
            verify_mldsa87_with_context(
                &decoded.authority_public_key,
                decoded.signing_hash(binding.network_id).as_bytes().as_slice(),
                &decoded.signature,
                PALW_AUTHORIZATION_MLDSA87_CONTEXT,
            ),
            Ok(true)
        );
    }

    /// Header-v5 (ADR-MA Compute Set registry) must be authorizable: `testnet-20` is a v5 genesis,
    /// so gating the producer at v4 refused every algo-4 block that network can build. The v5
    /// preimage extension (set id / policy id / plan id) is covered by `write_header_preimage`, so
    /// the commitment stays total.
    #[test]
    fn v5_compute_set_registry_header_is_authorizable() {
        let (authority, _leaf, mut binding, _) = produce();
        binding.header.version = PALW_COMPUTE_SET_HEADER_VERSION;
        binding.header.finalize();
        let authorized = authority.authorize(&binding).expect("v5 is an authorizable PALW version");

        // The signature must verify against the v5 header it was produced over.
        let mut minted = binding.header.clone();
        minted.palw_authorization_hash = authorized.authorization_hash;
        minted.finalize();
        let auth =
            <PalwBlockAuthorizationV1 as borsh::BorshDeserialize>::try_from_slice(&authorized.payload).expect("payload decodes");
        assert!(auth.binds_header(binding.network_id, &minted, &binding.authed_hash_merkle_root));

        // …and one version past v5 is still refused, so the gate stays a gate.
        binding.header.version = PALW_COMPUTE_SET_HEADER_VERSION + 1;
        binding.header.finalize();
        assert_eq!(authority.authorize(&binding).unwrap_err(), AuthorizeError::NotPalwHeader(PALW_COMPUTE_SET_HEADER_VERSION + 1));
    }

    /// Header-v4 deliberately permits one authorization signature to survive spam-nonce grinding.
    /// This is safe because each nonce is a different independently stamped final header, while the
    /// accumulator commitment and every other non-circular field remain authorization-bound.
    #[test]
    fn v4_authorization_reuse_is_limited_to_independently_stamped_nonce_variants() {
        let (authority, _leaf, mut binding, _) = produce();
        binding.header.version = PALW_ANTISPAM_HEADER_VERSION;
        binding.header.palw_spam_accumulator_commitment = h(0xD1);
        binding.header.palw_spam_nonce = 0;
        binding.header.finalize();
        let authorized = authority.authorize(&binding).expect("v4 is an explicit authorizable PALW version");

        let mut first = binding.header.clone();
        first.palw_authorization_hash = authorized.authorization_hash;
        first.palw_spam_nonce = 1;
        first.finalize();
        let mut second = first.clone();
        second.palw_spam_nonce = 2;
        second.finalize();

        assert!(authorized.auth.binds_header(binding.network_id, &first, &binding.authed_hash_merkle_root));
        assert!(authorized.auth.binds_header(binding.network_id, &second, &binding.authed_hash_merkle_root));
        assert_ne!(
            kaspa_consensus_core::hashing::header::palw_spam_hash(&first),
            kaspa_consensus_core::hashing::header::palw_spam_hash(&second),
            "each reusable-signature nonce is nevertheless independent objective work"
        );

        let mut changed_state = second;
        changed_state.palw_spam_accumulator_commitment = h(0xD2);
        changed_state.finalize();
        assert!(!authorized.auth.binds_header(binding.network_id, &changed_state, &binding.authed_hash_merkle_root));
    }

    /// Construction and validation: rejected input. Mutating any bound field breaks AUTH-02 and
    /// prevents an authorization from being transferred to a different block. The cases cover every
    /// bound field, including fields validated only at the virtual/UTXO stage.
    #[test]
    fn every_bound_field_is_load_bearing() {
        let (_authority, _leaf, binding, authorized) = produce();
        let auth = authorized.auth;

        // Each mutation is a block the authority did NOT authorize. Mutating the header in place and
        // re-finalizing is exactly what a re-minting observer would do.
        let mutate = |f: &dyn Fn(&mut Header)| -> Header {
            let mut hd = binding.header.clone();
            f(&mut hd);
            hd.finalize();
            hd
        };
        let cases: Vec<(&str, u32, Header, Hash64)> = vec![
            // --- ticket / block coordinates that the 9-scalar binding already covered ---
            ("parents", NET, mutate(&|hd| hd.parents_by_level.set_direct_parents(vec![h(0x71), h(0x73)])), h(0x80)),
            // Order-sensitivity: the same parent SET in a different order is a different block.
            ("parent order", NET, mutate(&|hd| hd.parents_by_level.set_direct_parents(vec![h(0x72), h(0x71)])), h(0x80)),
            ("tx set", NET, binding.header.clone(), h(0x81)),
            ("timestamp", NET, mutate(&|hd| hd.timestamp += 1), h(0x80)),
            ("chain commit", NET, mutate(&|hd| hd.palw_chain_commit = h(0xCD)), h(0x80)),
            ("target interval", NET, mutate(&|hd| hd.palw_target_daa_interval += 1), h(0x80)),
            ("batch id", NET, mutate(&|hd| hd.palw_batch_id = h(0x11)), h(0x80)),
            ("leaf index", NET, mutate(&|hd| hd.palw_leaf_index = 1), h(0x80)),
            ("nullifier", NET, mutate(&|hd| hd.palw_ticket_nullifier = h(0xEE)), h(0x80)),
            ("network id", NET + 1, binding.header.clone(), h(0x80)),
            // AUTH-02 fields validated at the virtual/UTXO stage.
            ("utxo commitment", NET, mutate(&|hd| hd.utxo_commitment = h(0xB2)), h(0x80)),
            ("accepted id merkle root", NET, mutate(&|hd| hd.accepted_id_merkle_root = h(0xB1)), h(0x80)),
            ("pruning point", NET, mutate(&|hd| hd.pruning_point = h(0xB3)), h(0x80)),
            ("overlay commitment root", NET, mutate(&|hd| hd.overlay_commitment_root = h(0xB4)), h(0x80)),
            ("beacon seed", NET, mutate(&|hd| hd.palw_beacon_seed = h(0xB5)), h(0x80)),
            ("epoch certificate hash", NET, mutate(&|hd| hd.palw_epoch_certificate_hash = h(0xB6)), h(0x80)),
            // A parent at level >= 1: invisible to the old binding, which committed to level 0 only.
            (
                "level-1 parent",
                NET,
                mutate(&|hd| {
                    let mut levels: Vec<Vec<Hash64>> = hd.parents_by_level.clone().into();
                    levels[1] = vec![h(0x91)];
                    hd.parents_by_level = levels.try_into().unwrap();
                }),
                h(0x80),
            ),
            // Remaining Header-v3 work and proof fields.
            ("bits", NET, mutate(&|hd| hd.bits = EASY_BITS - 1), h(0x80)),
            ("proof type", NET, mutate(&|hd| hd.palw_proof_type = 2), h(0x80)),
            ("blue compute work", NET, mutate(&|hd| hd.blue_compute_work = BlueWorkType::from(10u64)), h(0x80)),
        ];
        for (what, net, mutated_header, authed_root) in cases {
            // Guard against a vacuous case: a mutation that changed nothing would "pass" this test
            // while proving nothing. At least one of the three inputs must actually differ. `Header` has
            // no `PartialEq`, but every field mutated here is in the header preimage, so a real mutation
            // necessarily moves the (re-finalized) block-identity hash.
            assert!(
                net != binding.network_id
                    || authed_root != binding.authed_hash_merkle_root
                    || mutated_header.hash != binding.header.hash,
                "case {what} mutates nothing — it would assert vacuously"
            );
            assert!(!auth.binds_header(net, &mutated_header, &authed_root), "mutating {what} must break the authorization binding");
        }

        // A DIFFERENT authority's key does not satisfy the leaf's AUTH-03 declaration...
        let impostor = TicketAuthority::from_seed([0x5b; 32]);
        assert!(!auth.binds_leaf_authority(&impostor.pk_hash()));
        // ...and swapping the public key into the payload does not rescue the signature either, because
        // `signing_hash` covers the key.
        let mut swapped = auth.clone();
        swapped.authority_public_key = impostor.public_key().to_vec();
        assert_ne!(
            verify_mldsa87_with_context(
                &swapped.authority_public_key,
                swapped.signing_hash(binding.network_id).as_bytes().as_slice(),
                &swapped.signature,
                PALW_AUTHORIZATION_MLDSA87_CONTEXT,
            ),
            Ok(true)
        );
    }

    /// The signature is bound to the authorization's OWN context. Signing the same digest under a
    /// different PALW context (here the auditor-vote one) does not verify as an authorization — so an
    /// authorization can never be replayed as a vote or a beacon, and a producer that picks the wrong
    /// constant fails loudly rather than emitting silently-unverifiable blocks.
    #[test]
    fn authorization_signature_is_context_bound() {
        let (_authority, _leaf, binding, authorized) = produce();
        let digest = authorized.auth.signing_hash(binding.network_id);
        assert_ne!(
            verify_mldsa87_with_context(
                &authorized.auth.authority_public_key,
                digest.as_bytes().as_slice(),
                &authorized.auth.signature,
                kaspa_consensus_core::palw::PALW_AUDITOR_V2_MLDSA87_CONTEXT,
            ),
            Ok(true),
            "an authorization signature must not verify under another PALW context"
        );
    }

    /// AUTH-03 is only meaningful if the leaf actually carries the authority's hash — the registration
    /// field is the producer's single point of failure, so pin the derivation to the consensus one.
    #[test]
    fn pk_hash_matches_the_consensus_authority_derivation() {
        let authority = TicketAuthority::from_seed(AUTHORITY_SEED);
        assert_eq!(authority.pk_hash(), blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, authority.public_key()));
        // And a leaf minted under it declares exactly that.
        assert_eq!(base_leaf(&authority, h(0x10)).ticket_authority_pk_hash, authority.pk_hash());
    }

    /// The local `0x38` constant must BE the consensus subnetwork, not merely equal a literal that
    /// happens to match today. Asserting `authorized.subnetwork_byte == BLOCK_AUTHORIZATION_SUBNETWORK_BYTE`
    /// elsewhere is a tautology (the constructor copies the constant); this is the check that fails if
    /// consensus ever moves the band edge.
    #[test]
    fn subnetwork_byte_is_the_consensus_constant() {
        assert_eq!(
            kaspa_consensus_core::subnets::SubnetworkId::from_byte(BLOCK_AUTHORIZATION_SUBNETWORK_BYTE),
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION,
        );
    }

    /// The producer's carrying transaction IS the consensus canonical encoder's output — one encoder in
    /// the tree, so `construction == validation` holds structurally rather than by review.
    #[test]
    fn carrying_transaction_is_the_consensus_canonical_shape() {
        let (_authority, _leaf, _binding, authorized) = produce();
        let tx = authorized.carrying_transaction();
        assert_eq!(tx, kaspa_consensus_core::palw::build_palw_authorization_transaction(&authorized.auth));
        assert_eq!(tx.subnetwork_id, kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BLOCK_AUTHORIZATION);
        assert!(tx.inputs.is_empty() && tx.outputs.is_empty());
        assert_eq!((tx.lock_time, tx.gas, tx.mass()), (0, 0, 0));
        // The payload the transaction carries is the payload the authority signed over.
        assert_eq!(tx.payload, authorized.payload);
    }

    /// Pre-signing rejection cases. Each arm is a header this authority refuses because signing it would
    /// burn the ticket on a block this repository's own validator rejects. A PALW ticket is one draw on
    /// one registered leaf — there is no re-roll — so "refuse" and "sign anyway" are not equivalent.
    #[test]
    fn authorize_refuses_every_header_it_must_not_sign() {
        let authority = TicketAuthority::from_seed(AUTHORITY_SEED);
        let batch_id = h(0x10);
        let leaf = restamp_leaves(batch_id, &[base_leaf(&authority, batch_id)]).remove(0);
        let c = ctx();
        let ticket = grind_eligibility(&c, &leaf, nullifier_sequence(b"authority-secret", 128)).expect("easy target wins");
        let good = BlockAuthorizationBinding {
            network_id: NET,
            header: header(batch_id, ticket.raw_nullifier, &c),
            authed_hash_merkle_root: h(0x80),
        };
        // Control: the unmutated binding IS authorizable, so each rejection below is attributable to its
        // own mutation and not to a broken fixture.
        assert!(authority.authorize(&good).is_ok(), "the control binding must authorize");

        // A non-v3 header carries no PALW fields for the authorization to bind.
        let mut b = good.clone();
        b.header.version = PALW_HEADER_VERSION - 1;
        b.header.finalize();
        assert_eq!(authority.authorize(&b).unwrap_err(), AuthorizeError::NotPalwHeader(PALW_HEADER_VERSION - 1));

        // A hash-lane header is not a PALW block; authorizing one is meaningless.
        let mut b = good.clone();
        b.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_BLAKE2B_SHA3;
        b.header.finalize();
        assert_eq!(
            authority.authorize(&b).unwrap_err(),
            AuthorizeError::NotReplicaLane(kaspa_consensus_core::pow_layer0::POW_ALGO_ID_BLAKE2B_SHA3)
        );

        // Already naming some other authorization: the commitment zeroes the field, so signing would
        // silently produce a second authorization for a header that points at the first.
        let mut b = good.clone();
        b.header.palw_authorization_hash = h(0xDE);
        b.header.finalize();
        assert_eq!(authority.authorize(&b).unwrap_err(), AuthorizeError::AuthorizationHashAlreadyStamped);

        // I-3: an unpinned nonce fails the clause-9 draw, so the block could never be accepted.
        let mut b = good.clone();
        b.header.nonce = good.header.nonce ^ 1;
        b.header.finalize();
        assert_eq!(
            authority.authorize(&b).unwrap_err(),
            AuthorizeError::NonceNotPinnedToNullifier {
                expected: crate::mining::pinned_nonce(&good.header.palw_ticket_nullifier),
                got: good.header.nonce ^ 1,
            }
        );

        // AUTH-03: the right header, the wrong key. `authorize` alone cannot see this — only the
        // leaf-aware entry point can, which is why production must call it.
        let stranger = TicketAuthority::from_seed([0x5e; 32]);
        assert!(stranger.authorize(&good).is_ok(), "the plain entry point cannot detect a foreign authority");
        assert_eq!(
            stranger.authorize_for_leaf(&good, &leaf.ticket_authority_pk_hash).unwrap_err(),
            AuthorizeError::NotThisLeafsAuthority { expected: leaf.ticket_authority_pk_hash, got: stranger.pk_hash() }
        );
        // And the rightful holder passes the same gate.
        assert!(authority.authorize_for_leaf(&good, &leaf.ticket_authority_pk_hash).is_ok());
    }
}
