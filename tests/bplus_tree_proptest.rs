use halo::collections::btree::bplus_tree::BrandedBPlusTree;
use halo::GhostToken;
use proptest::prelude::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
enum Operation {
    Insert(u8, u16),
    Get(u8),
    // Remove(u8), // TODO: Implement remove in BrandedBPlusTree first if not present
}

proptest! {
    #[test]
    fn test_bplus_tree_matches_std_map(ops in proptest::collection::vec(
        prop_oneof![
            (any::<u8>(), any::<u16>()).prop_map(|(k, v)| Operation::Insert(k, v)),
            any::<u8>().prop_map(Operation::Get),
        ],
        1..100
    )) {
        let mut std_map = BTreeMap::new();
        
        GhostToken::new(|mut token| {
            let mut tree = BrandedBPlusTree::new();
            
            for op in ops {
                match op {
                    Operation::Insert(k, v) => {
                        let std_res = std_map.insert(k, v);
                        let tree_res = tree.insert(&mut token, k, v);
                        assert_eq!(std_res, tree_res, "Insert result mismatch for key {}", k);
                    }
                    Operation::Get(k) => {
                        let std_res = std_map.get(&k);
                        let tree_res = tree.get(&token, &k);
                        assert_eq!(std_res, tree_res, "Get result mismatch for key {}", k);
                    }
                }
            }
            
            // Final consistency check
            assert_eq!(tree.len(), std_map.len(), "Length mismatch");
            
            for (k, v) in &std_map {
                assert_eq!(tree.get(&token, k), Some(v), "Final content mismatch for key {}", k);
            }
        });
    }
}
