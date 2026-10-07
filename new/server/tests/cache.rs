use std::sync::Arc;
use std::thread;
use std::time::Duration;

use aegisx::core::cache::Cache;

#[test]
fn entries_expire_and_sweep_reclaims_them () {

    let cache: Cache<[u8; 32], u32> = Cache::new(8, 50);
    let key = [7u8; 32];

    assert!(cache.put(key, 1));
    assert_eq!(cache.get(&key), Some(1));
    assert!(cache.put_for([9u8; 32], 2, 10_000));

    thread::sleep(Duration::from_millis(80));

    assert_eq!(cache.get(&key), None);
    assert_eq!(cache.get(&[9u8; 32]), Some(2));
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.sweep(), 1);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.remove(&[9u8; 32]), Some(2));
    assert!(cache.is_empty());

}

#[test]
fn capacity_rejects_new_keys_but_allows_updates () {

    let cache: Cache<u64, &'static str> = Cache::new(2, 1_000);

    assert!(cache.put(1, "a"));
    assert!(cache.put(2, "b"));
    assert!(!cache.put(3, "c"));
    assert!(cache.put(2, "bb"));
    assert_eq!(cache.get(&2), Some("bb"));
    assert_eq!(cache.get(&3), None);

    cache.clear();

    assert!(cache.put(3, "c"));
    assert_eq!(cache.capacity(), 2);

}

#[test]
fn concurrent_readers_and_writers_stay_consistent () {

    let cache: Arc<Cache<u64, u64>> = Arc::new(Cache::new(100_000, 60_000));

    for key in 0..1_000 { assert!(cache.put(key, key * 2)); }

    let readers: Vec<_> = (0..4).map(|_| {

        let cache = cache.clone();

        thread::spawn(move || {

            let mut hits = 0u64;

            for round in 0..200_000u64 {

                let key = round % 1_000;

                if let Some(value) = cache.get(&key) { assert!(value == key * 2 || value == key * 3); hits += 1; }

            }

            hits

        })

    }).collect();

    let writer = {

        let cache = cache.clone();

        thread::spawn(move || { for key in 0..1_000 { assert!(cache.put(key, key * 3)); } })

    };

    writer.join().expect("writer");

    for reader in readers { assert_eq!(reader.join().expect("reader"), 200_000); }

    assert_eq!(cache.get(&999), Some(2_997));

}
