//! RFC7693 BLAKE2b-512 lowered to Boolean gates, including the key block,
//! 128-bit byte counter, final flag and little-endian words. Native hashing
//! update boundaries do not enter the circuit or its commitment preimage.
//! Primary specification: https://www.rfc-editor.org/rfc/rfc7693
use super::*;
pub type DigestWiresV5 = [ByteV5; 64];
type Word = [WireV5; 64];
const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];
const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];
/// Per compression: 96 G functions, each six 64-bit full adders
/// (3 XOR+2 AND per bit) and four 64-bit XORs; three counter/flag
/// XORs and two final XORs per state word. Rotations are wire permutations.
pub const BLAKE2B_XOR_GATES_PER_BLOCK_V5: u64 = 96 * (6 * 64 * 3 + 4 * 64) + 3 * 64 + 2 * 8 * 64;
pub const BLAKE2B_AND_GATES_PER_BLOCK_V5: u64 = 96 * 6 * 64 * 2;
pub const BLAKE2B_GATES_PER_BLOCK_V5: u64 = BLAKE2B_XOR_GATES_PER_BLOCK_V5 + BLAKE2B_AND_GATES_PER_BLOCK_V5;
pub fn compression_blocks_v5(message_bytes: u64, key_bytes: usize) -> Result<u64, String> {
    if key_bytes > 64 {
        return Err("BLAKE2b key exceeds64 bytes".into());
    }
    let data = message_bytes.div_ceil(128);
    if key_bytes == 0 { Ok(data.max(1)) } else { data.checked_add(1).ok_or("BLAKE2b block count overflow".into()) }
}
pub fn graph_cost_v5(input_bits: u64, blocks: u64, assertions: u64) -> Result<CircuitCostV5, String> {
    let xor = blocks.checked_mul(BLAKE2B_XOR_GATES_PER_BLOCK_V5).ok_or("hash circuit XOR count overflow")?;
    let and = blocks.checked_mul(BLAKE2B_AND_GATES_PER_BLOCK_V5).ok_or("hash circuit AND count overflow")?;
    let gates = xor
        .checked_add(and)
        .and_then(|n| n.checked_add(input_bits))
        .and_then(|n| n.checked_add(2))
        .ok_or("hash circuit total gates overflow")?;
    let graph_bytes = gates
        .checked_mul(std::mem::size_of::<BooleanGateV5>() as u64)
        .and_then(|n| n.checked_add(assertions.checked_mul(std::mem::size_of::<(WireV5, bool)>() as u64)?))
        .ok_or("hash circuit graph bytes overflow")?;
    Ok(CircuitCostV5 {
        gates,
        input_bits,
        assertions,
        graph_bytes,
        witness_bytes: gates,
        xor_gates: xor,
        and_gates: and,
        not_gates: 0,
    })
}
fn rotate(x: Word, n: usize) -> Word {
    std::array::from_fn(|i| x[(i + n) % 64])
}
fn mix(b: &mut CircuitBuilderV5, v: &mut [Word; 16], positions: [usize; 4], x: Word, y: Word) -> Result<(), String> {
    let [ai, bi, ci, di] = positions;
    let (mut a, mut bb, mut c, mut d) = (v[ai], v[bi], v[ci], v[di]);
    a = b.add_word(&a, &bb)?.0;
    a = b.add_word(&a, &x)?.0;
    d = rotate(b.xor_word(&d, &a)?, 32);
    c = b.add_word(&c, &d)?.0;
    bb = rotate(b.xor_word(&bb, &c)?, 24);
    a = b.add_word(&a, &bb)?.0;
    a = b.add_word(&a, &y)?.0;
    d = rotate(b.xor_word(&d, &a)?, 16);
    c = b.add_word(&c, &d)?.0;
    bb = rotate(b.xor_word(&bb, &c)?, 63);
    v[ai] = a;
    v[bi] = bb;
    v[ci] = c;
    v[di] = d;
    Ok(())
}
fn compress(b: &mut CircuitBuilderV5, h: &mut [Word; 8], block: &[ByteV5; 128], counter: u128, last: bool) -> Result<(), String> {
    let mut v = [[0; 64]; 16];
    v[..8].copy_from_slice(h);
    for i in 0..8 {
        v[i + 8] = b.constant_word(u128::from(IV[i]));
    }
    let low = b.constant_word(counter as u64 as u128);
    let high = b.constant_word(counter >> 64);
    let final_word = b.constant_word(if last { u64::MAX as u128 } else { 0 });
    v[12] = b.xor_word(&v[12], &low)?;
    v[13] = b.xor_word(&v[13], &high)?;
    v[14] = b.xor_word(&v[14], &final_word)?;
    let words: [Word; 16] = std::array::from_fn(|w| std::array::from_fn(|i| block[w * 8 + i / 8][i % 8]));
    for round in 0..12 {
        let s = SIGMA[round % 10];
        for (j, positions) in [
            [0, 4, 8, 12],
            [1, 5, 9, 13],
            [2, 6, 10, 14],
            [3, 7, 11, 15],
            [0, 5, 10, 15],
            [1, 6, 11, 12],
            [2, 7, 8, 13],
            [3, 4, 9, 14],
        ]
        .into_iter()
        .enumerate()
        {
            mix(b, &mut v, positions, words[s[j * 2]], words[s[j * 2 + 1]])?;
        }
    }
    for i in 0..8 {
        let x = b.xor_word(&h[i], &v[i])?;
        h[i] = b.xor_word(&x, &v[i + 8])?;
    }
    Ok(())
}
pub fn keyed_512_v5(b: &mut CircuitBuilderV5, key: &[u8], message: &dyn CircuitByteStreamV5) -> Result<DigestWiresV5, String> {
    let length = u64::try_from(message.len()).map_err(|_| "hash circuit message length overflow")?;
    compression_blocks_v5(length, key.len())?;
    let mut h: [Word; 8] = std::array::from_fn(|i| b.constant_word(IV[i] as u128));
    h[0] = b.constant_word((IV[0] ^ 0x01010000 ^ ((key.len() as u64) << 8) ^ 64) as u128);
    let zero = [b.constant(false); 8];
    let mut counter = 0u128;
    if !key.is_empty() {
        let mut block = [zero; 128];
        for (i, value) in key.iter().enumerate() {
            block[i] = std::array::from_fn(|j| b.constant((value >> j) & 1 != 0));
        }
        counter = 128;
        compress(b, &mut h, &block, counter, length == 0)?;
    }
    if length > 0 || key.is_empty() {
        let blocks = length.div_ceil(128).max(1);
        for index in 0..blocks {
            let start = index.checked_mul(128).ok_or("hash circuit block offset overflow")?;
            let used = length.saturating_sub(start).min(128);
            let mut block = [zero; 128];
            for i in 0..used {
                block[i as usize] = message.byte(usize::try_from(start + i).map_err(|_| "hash byte not addressable")?)?;
            }
            counter = counter.checked_add(u128::from(used)).ok_or("hash circuit counter overflow")?;
            compress(b, &mut h, &block, counter, index + 1 == blocks)?;
        }
    }
    Ok(std::array::from_fn(|i| std::array::from_fn(|bit| h[i / 8][(i % 8) * 8 + bit])))
}
/// Full explicit hash relation: public domain and digest, private message bits.
/// Gate/witness/graph sizes are computed before the first graph allocation.
pub fn compile_keyed_hash_v5(
    key: &[u8],
    bytes: usize,
    expected: Digest,
    limits: CircuitLimitsV5,
    memory: CircuitMemoryBudgetV5,
) -> Result<ReservedCircuitV5, String> {
    let input_bits = u64::try_from(bytes).map_err(|_| "hash byte count overflow")?.checked_mul(8).ok_or("hash input bits overflow")?;
    let blocks = compression_blocks_v5(bytes as u64, key.len())?;
    let plan = graph_cost_v5(input_bits, blocks, 512)?;
    let mut builder = CircuitBuilderV5::new_planned(limits, plan, memory)?;
    let message = builder.input_byte_span(bytes)?;
    let digest = keyed_512_v5(&mut builder, key, &message)?;
    builder.assert_bytes(&digest, &expected)?;
    builder.finish_reserved()
}
