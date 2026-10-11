//! Verifier-owned tokenizer operand transport. No producer model-file demand is created.
//! The original raw hash is checked with bounded reads before packets are generated; the
//! core parser independently rechecks it at EOF. A compressed sparse tree retains at most
//! two fixed-size nodes per unique key and never retains decoded JSON strings.
use kaspa_consensus_core::{
    Hash64,
    palw_json_key_set_v1::{PalwJsonKeyCompactProofV1, PalwJsonKeyInsertProofV1, palw_json_key_path_v1},
    palw_tokenizer_json_stream_v1::{PalwTokenizerJsonStreamV1, PalwTokenizerJsonVerdictV1},
    palw_tokenizer_rendering_v1::PALW_TOKENIZER_RAW_DOMAIN_V1,
    palw_tokenizer_table_stream_v1::PalwTokenizerStreamPacketV1,
};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
};

fn hash(domain: &[u8], fields: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for field in fields {
        h.update(field);
    }
    Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
}
fn node_hash(height: usize, left: Hash64, right: Hash64) -> Hash64 {
    hash(b"misaka-palw/json/object-key-node/v1", &[&(height as u16).to_le_bytes(), left.as_byte_slice(), right.as_byte_slice()])
}
fn bit(path: Hash64, at: usize) -> bool {
    path.as_byte_slice()[at / 8] & (1 << (7 - at % 8)) != 0
}
fn divergence(a: Hash64, b: Hash64) -> usize {
    (0..512).find(|i| bit(a, *i) != bit(b, *i)).unwrap_or(512)
}
#[derive(Clone)]
struct Node {
    path: Hash64,
    height: u16,
    hash: Hash64,
    children: Option<[usize; 2]>,
}
struct Book {
    nodes: Vec<Node>,
    root: Option<usize>,
    defaults: Vec<Hash64>,
    maximum: usize,
}
impl Book {
    fn new(keys: u64) -> Result<Self, String> {
        let maximum = usize::try_from(keys).ok().and_then(|n| n.checked_mul(2)).ok_or("tokenizer key book overflows")?;
        let mut nodes = Vec::new();
        nodes.try_reserve_exact(maximum).map_err(|e| e.to_string())?;
        if nodes.capacity() > maximum.saturating_mul(2).max(1) {
            return Err("tokenizer allocator exceeds reserved node envelope".into());
        }
        let mut defaults = Vec::with_capacity(513);
        defaults.push(hash(b"misaka-palw/json/object-key-empty/v1", &[]));
        for h in 1..=512 {
            let d = defaults[h - 1];
            defaults.push(node_hash(h, d, d));
        }
        Ok(Self { nodes, root: None, defaults, maximum })
    }
    fn lift(&self, index: usize, height: usize) -> Hash64 {
        let n = &self.nodes[index];
        let mut h = n.hash;
        for level in usize::from(n.height) + 1..=height {
            let d = self.defaults[level - 1];
            h = if bit(n.path, 512 - level) { node_hash(level, d, h) } else { node_hash(level, h, d) };
        }
        h
    }
    fn root_hash(&self) -> Hash64 {
        self.root.map_or(self.defaults[512], |r| self.lift(r, 512))
    }
    fn proof(&self, namespace: u64, key_hash: Hash64) -> PalwJsonKeyInsertProofV1 {
        let path = palw_json_key_path_v1(namespace, key_hash);
        let mut siblings = self.defaults[..512].to_vec();
        let mut cursor = self.root;
        while let Some(index) = cursor {
            let n = &self.nodes[index];
            let d = divergence(path, n.path);
            if d < 512 - usize::from(n.height) {
                siblings[511 - d] = self.lift(index, 511 - d);
                break;
            }
            let Some(children) = n.children else { break };
            let side = usize::from(bit(path, 512 - usize::from(n.height)));
            siblings[usize::from(n.height) - 1] = self.lift(children[1 - side], usize::from(n.height) - 1);
            cursor = Some(children[side]);
        }
        PalwJsonKeyInsertProofV1 { namespace, key_hash, siblings }
    }
    fn insert(&mut self, namespace: u64, key_hash: Hash64) -> Result<(), String> {
        let path = palw_json_key_path_v1(namespace, key_hash);
        let mut ancestors = Vec::with_capacity(512);
        let mut cursor = self.root;
        let mut replace = None;
        while let Some(index) = cursor {
            let n = &self.nodes[index];
            let d = divergence(path, n.path);
            if d < 512 - usize::from(n.height) || n.children.is_none() {
                if d == 512 {
                    return Ok(());
                }
                replace = Some((index, d));
                break;
            }
            let side = usize::from(bit(path, 512 - usize::from(n.height)));
            ancestors.push((index, side));
            cursor = Some(n.children.unwrap()[side]);
        }
        let needed = if replace.is_some() { 2 } else { 1 };
        if self.nodes.len().checked_add(needed).is_none_or(|n| n > self.maximum) {
            return Err("tokenizer key book exceeds admitted event budget".into());
        }
        let leaf = self.nodes.len();
        self.nodes.push(Node {
            path,
            height: 0,
            hash: hash(b"misaka-palw/json/object-key-present/v1", &[path.as_byte_slice()]),
            children: None,
        });
        let replacement = if let Some((old, d)) = replace {
            let side = usize::from(bit(path, d));
            let mut children = [old, old];
            children[side] = leaf;
            let height = 512 - d;
            let hash = node_hash(height, self.lift(children[0], height - 1), self.lift(children[1], height - 1));
            let branch = self.nodes.len();
            self.nodes.push(Node { path, height: height as u16, hash, children: Some(children) });
            branch
        } else {
            leaf
        };
        if let Some((parent, side)) = ancestors.last().copied() {
            self.nodes[parent].children.as_mut().unwrap()[side] = replacement;
        } else {
            self.root = Some(replacement);
        }
        for (index, _) in ancestors.into_iter().rev() {
            let n = &self.nodes[index];
            let children = n.children.unwrap();
            let height = usize::from(n.height);
            self.nodes[index].hash = node_hash(height, self.lift(children[0], height - 1), self.lift(children[1], height - 1));
        }
        Ok(())
    }
}

pub(crate) struct TokenizerWitnessPacketV1 {
    pub(crate) packet: PalwTokenizerStreamPacketV1,
    _reservation: Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}
pub(crate) enum TokenizerWitnessMoveV1 {
    Packet(TokenizerWitnessPacketV1),
    Seal,
}
pub(crate) struct TokenizerWitnessV1 {
    file: std::fs::File,
    parser: PalwTokenizerJsonStreamV1,
    book: Book,
    context: Hash64,
    length: u64,
    signed_keys: u64,
    _reservation: Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}
impl TokenizerWitnessV1 {
    /// `actual_length` comes from the authenticated held operand, not the producer's signed
    /// length declaration. A false signed length is adjudicated only by the rooted EOF court.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        path: &std::path::Path,
        context: Hash64,
        tokenizer: Hash64,
        actual_length: u64,
        signed_keys: u64,
        token: u32,
        class: Hash64,
        claim: Hash64,
        ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        let node_bytes = signed_keys
            .checked_mul(4)
            .and_then(|n| n.checked_mul(std::mem::size_of::<Node>() as u64))
            .ok_or("tokenizer book byte bound overflows")?;
        // Covers allocator doubling, two parser copies/stack, defaults, full proof/expansion,
        // packet/raw/serialized copies, and the bounded ancestor work list. No strings persist.
        let need = node_bytes.checked_add(1_048_576).ok_or("tokenizer scratch overflows")?;
        let reservation = Arc::new(
            ledger
                .reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-tokenizer-staged-witness",
                        class_id: class,
                        job: claim,
                    },
                    need,
                )
                .map_err(|e| e.to_string())?,
        );
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != actual_length {
            return Err("held tokenizer length changed".into());
        }
        let mut raw_hash = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKENIZER_RAW_DOMAIN_V1).to_state();
        raw_hash.update(&actual_length.to_le_bytes());
        let mut buffer = vec![0; 65536];
        let mut read = 0u64;
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            read = read.checked_add(count as u64).ok_or("held tokenizer read overflows")?;
            if read > actual_length {
                return Err("held tokenizer grew while authenticating".into());
            }
            raw_hash.update(&buffer[..count]);
        }
        if read != actual_length || raw_hash.finalize().as_bytes() != tokenizer.as_byte_slice() {
            return Err("held tokenizer is not the registered raw operand".into());
        }
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        Ok(Self {
            file,
            parser: PalwTokenizerJsonStreamV1::new(context, tokenizer, actual_length, token),
            book: Book::new(signed_keys)?,
            context,
            length: actual_length,
            signed_keys,
            _reservation: reservation,
        })
    }
    pub(crate) fn offset(&self) -> u64 {
        self.parser.offset()
    }
    pub(crate) fn verdict(&self) -> Result<PalwTokenizerJsonVerdictV1, String> {
        self.parser.finish()
    }
    /// Deterministic packets permit a fresh worker to replay this own file up to the rooted
    /// cursor. The caller must publish/acknowledge one move before asking for another.
    pub(crate) fn next(&mut self) -> Result<TokenizerWitnessMoveV1, String> {
        let offset = self.parser.offset();
        self.file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut raw = vec![0; usize::try_from((self.length - offset).min(65536)).unwrap()];
        self.file.read_exact(&mut raw).map_err(|e| e.to_string())?;
        let mut packet = PalwTokenizerStreamPacketV1 { offset, bytes: Vec::with_capacity(raw.len()), proofs: Vec::new() };
        let mut used = 0;
        let mut encoded = 16usize;
        loop {
            if let Some((namespace, key)) = self.parser.pending_key() {
                if self.parser.key_events() == self.signed_keys {
                    self.parser.mark_work_overrun(self.context, self.signed_keys)?;
                    continue;
                }
                let proof = self.book.proof(namespace, key);
                let compact = PalwJsonKeyCompactProofV1::compress(&proof)?;
                let size = borsh::to_vec(&compact).map_err(|e| e.to_string())?.len();
                if packet.proofs.len() == 64 || encoded + size > 90000 {
                    break;
                }
                self.parser.prove_key(self.context, &proof)?;
                self.book.insert(namespace, key)?;
                if self.parser.key_set_root() != self.book.root_hash() {
                    return Err("local tokenizer sparse book diverges from the core parser".into());
                }
                packet.proofs.push(compact);
                encoded += size;
            } else {
                if used == raw.len() || encoded == 90000 {
                    break;
                }
                let end = raw.len().min(used + 90000 - encoded);
                let count = self.parser.push(self.context, self.parser.offset(), &raw[used..end])?;
                if count == 0 {
                    return Err("tokenizer witness made no bounded progress".into());
                }
                packet.bytes.extend_from_slice(&raw[used..used + count]);
                used += count;
                encoded += count;
            }
        }
        if !packet.bytes.is_empty() || !packet.proofs.is_empty() {
            if borsh::to_vec(&packet).map_err(|e| e.to_string())?.len() > 90000 {
                return Err("tokenizer packet exceeds carrier bound".into());
            }
            return Ok(TokenizerWitnessMoveV1::Packet(TokenizerWitnessPacketV1 { packet, _reservation: self._reservation.clone() }));
        }
        self.parser.seal()?;
        self.parser.finish()?;
        Ok(TokenizerWitnessMoveV1::Seal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
    use kaspa_consensus_core::palw_json_key_set_v1::{PalwJsonKeySetV1, palw_json_key_hash_v1};
    use kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1;

    #[test]
    fn compressed_book_matches_full_512_bit_core_membership_and_retains_only_two_nodes_per_key() {
        let mut book = Book::new(100).unwrap();
        let mut core = PalwJsonKeySetV1::default();
        for i in 0..100u64 {
            let namespace = i % 7;
            let key = palw_json_key_hash_v1(&i.to_le_bytes());
            let proof = book.proof(namespace, key);
            assert_eq!(core.contains(namespace, key, &proof), Ok(false));
            core.insert(namespace, key, &proof).unwrap();
            book.insert(namespace, key).unwrap();
            assert_eq!(core.root(), book.root_hash());
            assert!(book.nodes.len() <= 2 * (i as usize + 1));
            let member = book.proof(namespace, key);
            assert_eq!(core.contains(namespace, key, &member), Ok(true));
            book.insert(namespace, key).unwrap();
            assert_eq!(core.root(), book.root_hash());
        }
        for i in 0..100u64 {
            let p = book.proof(i % 7, palw_json_key_hash_v1(&i.to_le_bytes()));
            assert_eq!(core.contains(p.namespace, p.key_hash, &p), Ok(true));
            let compact = PalwJsonKeyCompactProofV1::compress(&p).unwrap();
            assert_eq!(compact.expand().unwrap(), p);
        }
    }

    fn replay(core: &mut PalwTokenizerJsonStreamV1, context: Hash64, packet: &PalwTokenizerStreamPacketV1, bound: u64) {
        assert_eq!(core.offset(), packet.offset);
        assert!(packet.bytes.len() <= 65536 && packet.proofs.len() <= 64);
        assert!(borsh::to_vec(packet).unwrap().len() <= 90000);
        let mut bytes = 0;
        let mut proofs = 0;
        loop {
            if core.pending_key().is_some() && core.key_events() == bound {
                core.mark_work_overrun(context, bound).unwrap();
            }
            if core.pending_key().is_some() {
                if proofs == packet.proofs.len() {
                    break;
                }
                core.prove_key(context, &packet.proofs[proofs].expand().unwrap()).unwrap();
                proofs += 1;
            } else {
                if bytes == packet.bytes.len() {
                    break;
                }
                bytes += core.push(context, core.offset(), &packet.bytes[bytes..]).unwrap();
            }
        }
        assert_eq!(bytes, packet.bytes.len());
        assert_eq!(proofs, packet.proofs.len());
    }

    #[test]
    fn own_large_raw_file_packets_replay_after_restart_and_signed_work_overrun_hashes_to_eof() {
        let vocab = (0..140).map(|i| format!("\"t{i}\":{i}")).collect::<Vec<_>>().join(",");
        let raw = format!("{{\"model\":{{\"vocab\":{{{vocab}}}}},\"padding\":\"{}\"}}", "x".repeat(140000)).into_bytes();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tokenizer.json");
        std::fs::write(&path, &raw).unwrap();
        let context = Hash64::from_u64_word(19);
        let tokenizer = palw_tokenizer_raw_commitment_v1(&raw);
        for bound in [1000, 1] {
            let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(2_000_000), || None);
            let open = || {
                TokenizerWitnessV1::open(
                    &path,
                    context,
                    tokenizer,
                    raw.len() as u64,
                    bound,
                    1,
                    Hash64::from_u64_word(1),
                    Hash64::from_u64_word(2),
                    &ledger,
                )
                .unwrap()
            };
            let mut generator = open();
            let mut core = PalwTokenizerJsonStreamV1::new(context, tokenizer, raw.len() as u64, 1);
            let first = match generator.next().unwrap() {
                TokenizerWitnessMoveV1::Packet(p) => p,
                _ => panic!("nonempty raw operand"),
            };
            replay(&mut core, context, &first.packet, bound);
            let first_bytes = borsh::to_vec(&first.packet).unwrap();
            drop(generator);
            assert!(ledger.reserved_bytes() > 0, "the emitted carrier retains its RAM guard after the worker drops");
            drop(first);
            assert_eq!(ledger.reserved_bytes(), 0);
            let mut generator = open();
            let repeated = match generator.next().unwrap() {
                TokenizerWitnessMoveV1::Packet(p) => p,
                _ => panic!(),
            };
            assert_eq!(
                borsh::to_vec(&repeated.packet).unwrap(),
                first_bytes,
                "empty local restart deterministically rebuilds the rooted prefix"
            );
            drop(repeated);
            loop {
                match generator.next().unwrap() {
                    TokenizerWitnessMoveV1::Packet(p) => replay(&mut core, context, &p.packet, bound),
                    TokenizerWitnessMoveV1::Seal => {
                        core.seal().unwrap();
                        break;
                    }
                }
            }
            assert_eq!(generator.offset(), raw.len() as u64);
            assert_eq!(core.finish().unwrap(), generator.verdict().unwrap());
            if bound == 1 {
                assert_eq!(core.finish().unwrap(), PalwTokenizerJsonVerdictV1::WorkBoundExceeded);
            }
            drop(generator);
            assert_eq!(ledger.reserved_bytes(), 0);
        }
        let short = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(1), || None);
        assert!(
            TokenizerWitnessV1::open(
                &path,
                context,
                tokenizer,
                raw.len() as u64,
                1000,
                1,
                Hash64::default(),
                Hash64::default(),
                &short
            )
            .is_err()
        );
        assert_eq!(short.reserved_bytes(), 0);
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(2_000_000), || None);
        assert!(
            TokenizerWitnessV1::open(
                &path,
                context,
                Hash64::default(),
                raw.len() as u64,
                1000,
                1,
                Hash64::default(),
                Hash64::default(),
                &ledger
            )
            .is_err()
        );
        assert_eq!(ledger.reserved_bytes(), 0, "wrong own model material is refusal, never a conviction");
    }
}
