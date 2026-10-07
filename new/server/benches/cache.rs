use std::collections::HashMap;
use std::hint::black_box;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aegisx::core::cache::Cache;
use criterion::{Criterion, criterion_group, criterion_main};
use dashmap::DashMap;
use quick_cache::sync::Cache as Quick;

const KEYS: u64 = 10_000;
const OPS: u64 = 200_000;

type Key = [u8; 32];

fn key ( index: u64 ) -> Key {

    let mut key = [0u8; 32];

    key[..8].copy_from_slice(&index.to_le_bytes());
    key[8..16].copy_from_slice(&index.wrapping_mul(0x9E37_79B9_7F4A_7C15).to_le_bytes());

    key

}

trait Store: Send + Sync + 'static {

    fn read ( &self, key: &Key ) -> Option<u64>;

    fn write ( &self, key: Key, value: u64 );

}

impl Store for Cache<Key, u64> {

    fn read ( &self, key: &Key ) -> Option<u64> { self.get(key) }

    fn write ( &self, key: Key, value: u64 ) { self.put(key, value); }

}

impl Store for DashMap<Key, ( u64, u64 )> {

    fn read ( &self, key: &Key ) -> Option<u64> { self.get(key).filter(|entry| entry.1 > 0).map(|entry| entry.0) }

    fn write ( &self, key: Key, value: u64 ) { self.insert(key, ( value, u64::MAX )); }

}

impl Store for Quick<Key, ( u64, u64 )> {

    fn read ( &self, key: &Key ) -> Option<u64> { self.get(key).filter(|entry| entry.1 > 0).map(|entry| entry.0) }

    fn write ( &self, key: Key, value: u64 ) { self.insert(key, ( value, u64::MAX )); }

}

impl Store for Mutex<HashMap<Key, ( u64, u64 )>> {

    fn read ( &self, key: &Key ) -> Option<u64> { self.lock().ok()?.get(key).filter(|entry| entry.1 > 0).map(|entry| entry.0) }

    fn write ( &self, key: Key, value: u64 ) { if let Ok(mut map) = self.lock() { map.insert(key, ( value, u64::MAX )); } }

}

type Runner = Box<dyn Fn(usize, u64, u64) -> Duration>;

type Maker = Box<dyn Fn() -> Runner>;

fn filled <S: Store> ( store: S ) -> Arc<S> {

    for index in 0..KEYS { store.write(key(index), index); }

    Arc::new(store)

}

fn run <S: Store> ( store: &Arc<S>, threads: usize, write_every: u64, iterations: u64 ) -> Duration {

    let mut total = Duration::ZERO;

    for _ in 0..iterations {

        let started = Instant::now();

        let handles: Vec<_> = (0..threads).map(|thread| {

            let store = store.clone();

            thread::spawn(move || {

                let mut state = 0x2545_F491_4F6C_DD1Du64 ^ (thread as u64 + 1);
                let mut hits = 0u64;

                for op in 0..OPS {

                    state ^= state << 13; state ^= state >> 7; state ^= state << 17;

                    let index = state % KEYS;

                    if write_every > 0 && op.is_multiple_of(write_every) { store.write(key(index), op); } else if store.read(&key(index)).is_some() { hits += 1; }

                }

                black_box(hits)

            })

        }).collect();

        for handle in handles { let _ = handle.join(); }

        total += started.elapsed();

    }

    total

}

fn bench ( criterion: &mut Criterion ) {

    let stores: Vec<( &str, Maker )> = vec![
        ( "aegisx-papaya", Box::new(|| { let store = filled(Cache::<Key, u64>::new(KEYS as usize * 2, u64::MAX)); Box::new(move |threads, writes, iterations| run(&store, threads, writes, iterations)) }) ),
        ( "dashmap", Box::new(|| { let store = filled(DashMap::<Key, ( u64, u64 )>::with_capacity(KEYS as usize * 2)); Box::new(move |threads, writes, iterations| run(&store, threads, writes, iterations)) }) ),
        ( "quick_cache", Box::new(|| { let store = filled(Quick::<Key, ( u64, u64 )>::new(KEYS as usize * 2)); Box::new(move |threads, writes, iterations| run(&store, threads, writes, iterations)) }) ),
        ( "mutex-hashmap", Box::new(|| { let store = filled(Mutex::new(HashMap::<Key, ( u64, u64 )>::with_capacity(KEYS as usize * 2))); Box::new(move |threads, writes, iterations| run(&store, threads, writes, iterations)) }) ),
    ];

    for ( threads, writes, label ) in [( 1, 0, "read-1t" ), ( 4, 20, "mixed95-4t" ), ( 8, 20, "mixed95-8t" ), ( 4, 0, "read-4t" )] {

        let mut group = criterion.benchmark_group(label);

        group.sample_size(10).measurement_time(Duration::from_secs(8)).throughput(criterion::Throughput::Elements(OPS * threads as u64));

        for ( name, make ) in &stores {

            let runner = make();

            group.bench_function(*name, |bencher| bencher.iter_custom(|iterations| runner(threads, writes, iterations)));

        }

        group.finish();

    }

}

criterion_group!(cache, bench);
criterion_main!(cache);
