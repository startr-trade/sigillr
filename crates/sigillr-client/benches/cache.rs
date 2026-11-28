//! Cache-hit cost (FTR-004, B-064): `Client::key` served from a cache filled to its 8 MiB
//! default budget, with the crate's fake transport and clock, so no network. Budget: p95 at
//! most 5 µs. It exercises the hit path of `client_serves_cached_answer_within_lifetime` (R-2).
//!
//! Compile under `heavy`, run outside it:
//! `nice -n 19 timeout 30m cargo bench -p sigillr-client --bench cache`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use sigillr_client::testing::{FakeClock, FakeTransport};
use sigillr_client::{Client, HttpResponse};

const KEY: &str = include_str!("../tests/fixtures/key.json");
const INSTANCE: &str = "https://sigillr.provr.dev";

fn fingerprint(i: usize) -> String {
    format!("sha256:{i:064x}")
}

fn client_cache_hit(c: &mut Criterion) {
    let transport = FakeTransport::new();
    transport.push(Ok(HttpResponse::new(200, vec![], KEY.as_bytes().to_vec())));
    let client = Client::builder(INSTANCE)
        .transport(transport.clone())
        .clock(FakeClock::new())
        .build()
        .expect("client");
    // More answers than the 8 MiB budget holds, so the cache is full and has evicted.
    let mut i = 0;
    while transport.calls() < 20_000 {
        client.key(&fingerprint(i)).expect("lookup");
        i += 1;
    }
    let hit = fingerprint(i - 1);
    let before = transport.calls();
    assert!(client.key(&hit).expect("lookup").from_cache, "a hit");
    c.bench_function("client_cache_hit", |b| {
        b.iter(|| black_box(client.key(black_box(&hit)).expect("lookup")))
    });
    assert_eq!(transport.calls(), before, "every lookup was a cache hit");
}

criterion_group!(benches, client_cache_hit);
criterion_main!(benches);
