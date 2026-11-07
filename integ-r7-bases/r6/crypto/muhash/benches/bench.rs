use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rand_chacha::{
    ChaCha8Rng,
    rand_core::{RngCore, SeedableRng},
};

use kaspa_muhash::{MuHash, SERIALIZED_MUHASH_SIZE};

fn bench_muhash(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut rand_set = MuHash::new();

    let mut data = [0u8; 100];
    // Set the numerator and denominators.
    rng.fill_bytes(&mut data);
    rand_set.add_element(&data);
    rng.fill_bytes(&mut data);
    rand_set.remove_element(&data);

    rng.fill_bytes(&mut data);

    c.bench_function("MuHash::add_element", |b| {
        let mut muhash = MuHash::new();
        b.iter(|| {
            black_box(&mut data);
            muhash.add_element(&data);
        });
        black_box(muhash);
    });

    c.bench_function("MuHash::remove_element", |b| {
        let mut muhash = MuHash::new();
        b.iter(|| {
            black_box(&mut data);
            muhash.remove_element(&data);
        });
        black_box(muhash);
    });
    c.bench_function("MuHash::combine", |b| {
        let mut muhash = MuHash::new();
        b.iter(|| {
            black_box((&mut rand_set, &mut muhash));
            muhash.combine(&rand_set);
        });
        black_box(muhash);
    });

    c.bench_function("MuHash::clone", |b| {
        b.iter(|| {
            black_box(&mut rand_set);
            rand_set.clone()
        });
    });

    c.bench_function("MuHash::serialize worst", |b| {
        // PR-9.5e: LtHash16_1024 serialized state is SERIALIZED_MUHASH_SIZE (2048)
        // bytes; every byte sequence of that length is a valid state (no prime
        // bound to stay under, unlike the previous Uint3072 multiplicative MuHash),
        // so the prior 384-byte fixture and prime-tuning offsets are dropped.
        let muhash_serialized = [255u8; SERIALIZED_MUHASH_SIZE];
        let muhash = MuHash::deserialize(muhash_serialized).unwrap();
        b.iter(|| black_box(muhash.clone()).serialize());
    });

    c.bench_function("MuHash::serialize best", |b| {
        let muhash = MuHash::new();
        b.iter(|| black_box(muhash.clone()).serialize())
    });

    c.bench_function("MuHash::serialize rand", |b| b.iter(|| black_box(rand_set.clone()).serialize()));

    c.bench_function("MuHash::finalize", |b| {
        b.iter(|| black_box(rand_set.clone()).finalize());
    });
}

criterion_group!(benches, bench_muhash);
criterion_main!(benches);
