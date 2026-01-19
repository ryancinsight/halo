use halo::{GhostToken, SharedGhostToken, BrandedHashMap};
use std::sync::Arc;
use std::thread;

#[test]
fn test_shared_token_concurrency() {
    GhostToken::new(|token| {
        let mut map = BrandedHashMap::new();
        map.insert("key", 1);
        map.insert("key2", 2);

        let map = Arc::new(map);
        let shared_token = Arc::new(SharedGhostToken::new(token));

        thread::scope(|s| {
            for _ in 0..10 {
                let map_clone = map.clone();
                let token_clone = shared_token.clone();
                s.spawn(move || {
                    let guard = token_clone.read();
                    let val = map_clone.get(&guard, &"key");
                    assert_eq!(val, Some(&1));

                    let val2 = map_clone.get(&guard, &"key2");
                    assert_eq!(val2, Some(&2));
                });
            }
        });
    });
}

#[test]
fn test_shared_token_write_access() {
    GhostToken::new(|token| {
        let mut map = BrandedHashMap::new();
        map.insert("key", 0);

        let map = Arc::new(map);
        let shared_token = Arc::new(SharedGhostToken::new(token));

        {
            let mut guard = shared_token.write();
            if let Some(val) = map.get_mut(&mut guard, &"key") {
                *val = 100;
            }
        }

        {
            let guard = shared_token.read();
            let val = map.get(&guard, &"key");
            assert_eq!(val, Some(&100));
        }
    });
}

#[test]
fn test_shared_token_mixed_access() {
    GhostToken::new(|token| {
        let mut map = BrandedHashMap::new();
        map.insert("counter", 0);

        let map = Arc::new(map);
        let shared_token = Arc::new(SharedGhostToken::new(token));

        thread::scope(|s| {
            // Writer thread
            let map_clone = map.clone();
            let token_clone = shared_token.clone();
            s.spawn(move || {
                for _ in 0..100 {
                    let mut guard = token_clone.write();
                    if let Some(val) = map_clone.get_mut(&mut guard, &"counter") {
                        *val += 1;
                    }
                }
            });

            // Reader threads
            for _ in 0..5 {
                let map_clone = map.clone();
                let token_clone = shared_token.clone();
                s.spawn(move || {
                    for _ in 0..100 {
                        let guard = token_clone.read();
                        let _val = map_clone.get(&guard, &"counter");
                    }
                });
            }
        });

        let guard = shared_token.read();
        let val = map.get(&guard, &"counter");
        assert_eq!(val, Some(&100));
    });
}
