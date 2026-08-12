//! The cache promises four things. Each one is a test here.
//!
//! 1. What goes in comes out, keyed by content rather than by path.
//! 2. It never exceeds its budget, and it evicts the least recently used first.
//! 3. A corrupted entry heals instead of serving wrong pixels.
//! 4. Bumping `pipeline_ver` invalidates every proxy without deleting anything.

use aura_cache::lru::CacheIndex;
use aura_cache::{
    decode_linear, encode_linear, Artefact, CacheBudget, CacheKey, CacheStore, LinearHeader,
    MIN_BUDGET_BYTES,
};

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn store(budget: u64) -> (tempfile::TempDir, CacheStore) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let store = CacheStore::open(directory.path(), CacheBudget { max_bytes: budget })
        .expect("open the cache");
    (directory, store)
}

// ------------------------------------------------------------------- basics

#[test]
fn what_goes_in_comes_out() {
    let (_directory, store) = store(MIN_BUDGET_BYTES);
    let key = CacheKey::new(HASH_A, 1, Artefact::Thumb);
    store.put(&key, b"thumbnail bytes").expect("put");

    assert!(store.contains(&key));
    assert_eq!(
        store.get(&key).as_deref(),
        Some(b"thumbnail bytes".as_ref())
    );
    let stats = store.stats();
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.bytes_used, 15);
    assert_eq!(stats.hits, 1);
}

#[test]
fn a_miss_is_a_miss_and_is_counted() {
    let (_directory, store) = store(MIN_BUDGET_BYTES);
    let key = CacheKey::new(HASH_A, 1, Artefact::Proxy);
    assert!(store.get(&key).is_none());
    assert_eq!(store.stats().misses, 1);
    assert_eq!(store.stats().hits, 0);
}

#[test]
fn entries_are_sharded_by_the_first_two_characters_of_the_hash() {
    let (directory, store) = store(MIN_BUDGET_BYTES);
    let key = CacheKey::new(HASH_A, 1, Artefact::Thumb);
    store.put(&key, b"x").expect("put");

    let expected = directory.path().join("aa").join(key.file_name());
    assert!(
        expected.exists(),
        "expected {} to exist",
        expected.display()
    );
}

#[test]
fn the_pipeline_version_is_part_of_the_key() {
    let (_directory, store) = store(MIN_BUDGET_BYTES);
    let old = CacheKey::new(HASH_A, 1, Artefact::Proxy);
    let new = CacheKey::new(HASH_A, 2, Artefact::Proxy);
    store.put(&old, b"rendered by pipeline 1").expect("put");

    // A pipeline bump is a clean miss, and the old entry is still there to be
    // evicted in its own time rather than deleted by hand.
    assert!(store.get(&new).is_none());
    assert_eq!(
        store.get(&old).as_deref(),
        Some(b"rendered by pipeline 1".as_ref())
    );
}

#[test]
fn the_four_artefacts_of_one_image_do_not_collide() {
    let (_directory, store) = store(MIN_BUDGET_BYTES);
    for artefact in Artefact::all() {
        let key = CacheKey::new(HASH_A, 1, artefact);
        store.put(&key, artefact.as_str().as_bytes()).expect("put");
    }
    for artefact in Artefact::all() {
        let key = CacheKey::new(HASH_A, 1, artefact);
        assert_eq!(
            store.get(&key).as_deref(),
            Some(artefact.as_str().as_bytes())
        );
    }
    assert_eq!(store.stats().entries, 4);
}

// ------------------------------------------------------------------- budget

#[test]
fn the_budget_is_never_exceeded_and_the_oldest_entry_goes_first() {
    let (_directory, store) = store(MIN_BUDGET_BYTES);
    // A tiny budget, set through the clamping constructor's raw form so the
    // test can exercise eviction without writing 256 MB.
    store.set_budget(CacheBudget { max_bytes: 300 });

    let payload = vec![0u8; 100];
    for hash in [HASH_A, HASH_B, HASH_C] {
        store
            .put(&CacheKey::new(hash, 1, Artefact::Thumb), &payload)
            .expect("put");
    }
    assert_eq!(store.stats().entries, 3);

    // Touch A so that B becomes the least recently used, then overflow.
    assert!(store
        .get(&CacheKey::new(HASH_A, 1, Artefact::Thumb))
        .is_some());
    store
        .put(&CacheKey::new(HASH_A, 1, Artefact::Proxy), &payload)
        .expect("put");

    let stats = store.stats();
    assert!(
        stats.bytes_used <= 300,
        "cache exceeded its budget: {} bytes",
        stats.bytes_used
    );
    assert!(stats.evictions >= 1);
    assert!(
        !store.contains(&CacheKey::new(HASH_B, 1, Artefact::Thumb)),
        "the least recently used entry should have gone first"
    );
    assert!(store.contains(&CacheKey::new(HASH_A, 1, Artefact::Thumb)));
}

#[test]
fn an_absurdly_small_budget_is_clamped_rather_than_thrashing() {
    let budget = CacheBudget::clamped(10);
    assert_eq!(budget.max_bytes, MIN_BUDGET_BYTES);
}

#[test]
fn purging_empties_the_cache_but_keeps_the_lifetime_counters() {
    let (_directory, store) = store(MIN_BUDGET_BYTES);
    let key = CacheKey::new(HASH_A, 1, Artefact::Thumb);
    store.put(&key, b"bytes").expect("put");
    assert!(store.get(&key).is_some());

    store.purge().expect("purge");
    let stats = store.stats();
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.bytes_used, 0);
    assert_eq!(stats.hits, 1, "lifetime statistics survive a purge");
    assert!(store.get(&key).is_none());
}

#[test]
fn statistics_report_a_usable_hit_rate() {
    let (_directory, store) = store(MIN_BUDGET_BYTES);
    let key = CacheKey::new(HASH_A, 1, Artefact::Thumb);
    assert_eq!(store.stats().hit_rate(), 0.0, "no requests, no claims");

    store.put(&key, b"bytes").expect("put");
    let _ = store.get(&key);
    let _ = store.get(&CacheKey::new(HASH_B, 1, Artefact::Thumb));
    let stats = store.stats();
    assert!((stats.hit_rate() - 0.5).abs() < 1e-9);
    assert!(stats.fill_fraction() > 0.0);
}

// ------------------------------------------------------------------ healing

#[test]
fn a_corrupted_entry_is_rebuilt_rather_than_served() {
    let (directory, store) = store(MIN_BUDGET_BYTES);
    let key = CacheKey::new(HASH_A, 1, Artefact::Proxy);
    store.put(&key, b"the real pixels").expect("put");

    // Something else on the machine rewrites the file: a sync client, a backup
    // tool, a failing disk.
    let path = directory.path().join(key.shard()).join(key.file_name());
    std::fs::write(&path, b"corrupted!!!!!!").expect("corrupt the entry");

    assert!(
        store.get(&key).is_none(),
        "a digest mismatch must be a miss, never wrong pixels"
    );
    assert!(!store.contains(&key), "and the bad entry must be forgotten");
}

#[test]
fn an_entry_deleted_behind_our_back_is_a_miss_not_an_error() {
    let (directory, store) = store(MIN_BUDGET_BYTES);
    let key = CacheKey::new(HASH_A, 1, Artefact::Thumb);
    store.put(&key, b"bytes").expect("put");
    std::fs::remove_file(directory.path().join(key.shard()).join(key.file_name()))
        .expect("delete the file");

    assert!(store.get(&key).is_none());
    assert!(!store.contains(&key));
}

#[test]
fn the_index_is_rebuilt_by_scanning_when_it_is_missing() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let key = CacheKey::new(HASH_A, 1, Artefact::Thumb);
    {
        let store = CacheStore::open(directory.path(), CacheBudget::default()).expect("open");
        store
            .put(&key, b"bytes that outlive the index")
            .expect("put");
    }
    std::fs::remove_file(directory.path().join("index.json")).expect("delete the index");

    let reopened = CacheStore::open(directory.path(), CacheBudget::default()).expect("reopen");
    assert!(reopened.contains(&key), "the entry survived the index");
    assert_eq!(
        reopened.get(&key).as_deref(),
        Some(b"bytes that outlive the index".as_ref())
    );
}

#[test]
fn a_stray_file_in_the_cache_directory_is_ignored() {
    let directory = tempfile::tempdir().expect("temporary directory");
    std::fs::create_dir_all(directory.path().join("zz")).expect("shard");
    std::fs::write(directory.path().join("zz").join("notes.txt"), b"hello").expect("stray file");

    let store = CacheStore::open(directory.path(), CacheBudget::default()).expect("open");
    assert_eq!(store.stats().entries, 0);
}

// ------------------------------------------------------------ linear buffers

#[test]
fn the_sixteen_bit_buffer_round_trips_through_its_container() {
    let samples: Vec<u16> = (0..(4 * 3 * 3)).map(|value| value * 1_000).collect();
    let encoded = encode_linear(
        LinearHeader {
            width: 4,
            height: 3,
        },
        &samples,
    );
    let (header, decoded) = decode_linear(&encoded).expect("decode");
    assert_eq!((header.width, header.height), (4, 3));
    assert_eq!(decoded, samples);
}

#[test]
fn a_linear_buffer_with_the_wrong_length_is_refused() {
    let mut encoded = encode_linear(
        LinearHeader {
            width: 2,
            height: 2,
        },
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
    );
    encoded.truncate(encoded.len() - 4);
    assert!(decode_linear(&encoded).is_none());
    assert!(decode_linear(b"not ours at all").is_none());
}

// -------------------------------------------------------------- index logic

#[test]
fn eviction_takes_the_least_recently_used_first() {
    let mut index = CacheIndex::default();
    index.insert("a".into(), 100, "da".into());
    index.insert("b".into(), 100, "db".into());
    index.insert("c".into(), 100, "dc".into());
    assert!(index.touch("a"));

    assert_eq!(
        index.eviction_order(150),
        vec!["b".to_string(), "c".to_string()]
    );
}

#[test]
fn a_cache_inside_its_budget_evicts_nothing() {
    let mut index = CacheIndex::default();
    index.insert("a".into(), 10, "da".into());
    assert!(index.eviction_order(1_000).is_empty());
}

#[test]
fn a_miss_does_not_create_an_index_entry() {
    let mut index = CacheIndex::default();
    assert!(!index.touch("absent"));
    assert_eq!(index.misses, 1);
    assert!(index.entries.is_empty());
}
