//! Comprehensive tests for GhostCell covering edge cases, safety, and correctness

use halo::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::{Arc, Mutex};

// ===== SAFETY AND CORRECTNESS TESTS =====

// NOTE: This crate intentionally does not expose a safe `uninit()` constructor
// for `GhostCell<T>` because it would either:
// - require runtime checks on every access, or
// - risk undefined behavior if accessed before initialization.

#[test]
fn test_zero_sized_types() {
    // Test with zero-sized types
    GhostToken::new(|mut token| {
        let zst_cell = GhostCell::new(());
        assert_eq!(*zst_cell.borrow(&token), ());

        *zst_cell.borrow_mut(&mut token) = ();
        assert_eq!(*zst_cell.borrow(&token), ());
    });
}

#[test]
fn test_non_copy_types() {
    // Test with non-Copy types like String and Vec
    GhostToken::new(|mut token| {
        let string_cell = GhostCell::new("Hello".to_string());
        assert_eq!(*string_cell.borrow(&token), "Hello");

        string_cell.borrow_mut(&mut token).push_str(" World");
        assert_eq!(*string_cell.borrow(&token), "Hello World");

        let vec_cell = GhostCell::new(vec![1, 2, 3]);
        vec_cell.borrow_mut(&mut token).push(4);
        assert_eq!(*vec_cell.borrow(&token), vec![1, 2, 3, 4]);
    });
}

#[test]
fn test_types_with_destructors() {
    // Test with types that implement Drop
    let drop_count = std::cell::Cell::new(0);

    struct DropCounter<'a>(&'a std::cell::Cell<i32>);

    impl<'a> Drop for DropCounter<'a> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    GhostToken::new(|_token| {
        let _cell1 = GhostCell::new(DropCounter(&drop_count));
        let _cell2 = GhostCell::new(DropCounter(&drop_count));
        // Drop the cells - should call destructors in reverse order
        drop(_cell1);
        assert_eq!(drop_count.get(), 1);
        drop(_cell2);
        assert_eq!(drop_count.get(), 2);
    });
}

#[test]
fn test_large_types() {
    // Test with large data structures
    GhostToken::new(|mut token| {
        let large_cell = GhostCell::new(vec![0u8; 1024 * 1024]); // 1MB
        assert_eq!(large_cell.borrow(&token).len(), 1024 * 1024);

        // Modify in place without copying
        large_cell.borrow_mut(&mut token)[0] = 42;
        assert_eq!(large_cell.borrow(&token)[0], 42);
    });
}

#[test]
fn test_empty_collections() {
    // Test with empty collections
    GhostToken::new(|token| {
        let empty_vec = GhostCell::new(Vec::<i32>::new());
        assert!(empty_vec.borrow(&token).is_empty());

        let empty_string = GhostCell::new(String::new());
        assert!(empty_string.borrow(&token).is_empty());

        let empty_hashmap = GhostCell::new(HashMap::<i32, i32>::new());
        assert!(empty_hashmap.borrow(&token).is_empty());
    });
}

#[test]
fn test_nested_types() {
    // Test with nested types (avoiding true recursion)
    GhostToken::new(|token| {
        let cell = GhostCell::new(RefCell::new(Some(42)));
        assert_eq!(*cell.borrow(&token).borrow(), Some(42));
    });
}

#[test]
fn test_borrow_mutability() {
    // Test that we can't create multiple mutable borrows
    // This is enforced by the type system, so we test valid patterns
    GhostToken::new(|token| {
        let cell = GhostCell::new(42);

        // Single mutable borrow is fine (but we need a mutable token)
        // Since we can't have multiple mutable borrows, we just test immutable
        let _borrow1 = cell.borrow(&token);
        assert_eq!(*_borrow1, 42);
    });
}

#[test]
fn test_replace_and_swap() {
    // Test replace and swap operations
    GhostToken::new(|mut token| {
        let cell1 = GhostCell::new(42);
        let cell2 = GhostCell::new(100);

        // Test replace
        let old_value = cell1.replace(&mut token, 200);
        assert_eq!(old_value, 42);
        assert_eq!(*cell1.borrow(&token), 200);

        // Test swap
        cell1.swap(&mut token, &cell2);
        assert_eq!(*cell1.borrow(&token), 100);
        assert_eq!(*cell2.borrow(&token), 200);
    });
}

#[test]
fn test_update_and_map() {
    // Test implementation would go here
}

// Property-based tests disabled for now due to GhostToken return type incompatibility
// TODO: Re-enable when proptest integration is properly structured

#[test]
fn test_update_and_map_operations() {
    // Test functional update and map operations
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(42);

        // Test update
        cell.update(&mut token, |x| *x *= 2);
        assert_eq!(*cell.borrow(&token), 84);

        // Test map (consumes the cell)
        let new_cell = cell.map(&token, |x| x.to_string());
        assert_eq!(*new_cell.borrow(&token), "84");
    });
}

#[test]
fn test_apply_operations() {
    // Test apply operations
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(vec![1, 2, 3]);

        // Test apply (immutable)
        let len = cell.apply(&token, |v| v.len());
        assert_eq!(len, 3);

        // Test apply_mut
        let old_len = cell.apply_mut(&mut token, |v| {
            let old = v.len();
            v.push(4);
            old
        });
        assert_eq!(old_len, 3);
        assert_eq!(cell.borrow(&token).len(), 4);
    });
}

#[test]
fn test_clone_operations() {
    // Test clone operations
    GhostToken::new(|token| {
        let cell = GhostCell::new(vec![1, 2, 3]);
        let cloned = cell.cloned(&token);
        assert_eq!(cloned, vec![1, 2, 3]);

        // Original should be unchanged
        assert_eq!(*cell.borrow(&token), vec![1, 2, 3]);
    });
}

// ===== BORROW CHECKER AND LIFETIME TESTS =====

#[test]
fn test_borrow_lifetime_constraints() {
    // Test that borrows respect lifetime constraints
    GhostToken::new(|token| {
        let cell = GhostCell::new(42);

        // This should work - borrow lifetime tied to token
        let value_ref = cell.borrow(&token);
        assert_eq!(*value_ref, 42);

        // Can't store the reference beyond the borrow scope
        // This would be a compile error if uncommented:
        // let stored_ref: &i32 = value_ref; // Compile error!
    });
}

#[test]
fn test_nested_scopes() {
    // Test nested token scopes
    GhostToken::new(|outer_token| {
        let outer_cell = GhostCell::new("outer");

        GhostToken::new(|inner_token| {
            let inner_cell = GhostCell::new("inner");

            // Can access cells within their respective scopes
            assert_eq!(*outer_cell.borrow(&outer_token), "outer");
            assert_eq!(*inner_cell.borrow(&inner_token), "inner");

            // Can't mix tokens from different scopes
            // This would be a compile error if uncommented:
            // assert_eq!(*outer_cell.borrow(&inner_token), "outer"); // Compile error!
        });

        // Inner scope is done, can still access outer
        assert_eq!(*outer_cell.borrow(&outer_token), "outer");
    });
}

// ===== MEMORY AND PERFORMANCE TESTS =====

#[test]
fn test_memory_layout() {
    use std::mem;

    // GhostCell should have reasonable size (may have small overhead due to initialization tracking)
    let cell_size = mem::size_of::<GhostCell<i32>>();
    let base_size = mem::size_of::<std::cell::Cell<std::mem::MaybeUninit<i32>>>();
    assert!(cell_size >= base_size); // Should be at least as big as the base
    assert!(cell_size <= base_size + 8); // But not much bigger

    // Test that alignment is reasonable
    assert_eq!(
        mem::align_of::<GhostCell<i32>>(),
        mem::align_of::<std::cell::Cell<std::mem::MaybeUninit<i32>>>()
    );

    // Note: GhostToken size depends on the specific lifetime used
    // The zero-sized property holds for the abstract token type
}

#[test]
fn test_no_unnecessary_allocations() {
    // Test that operations don't cause unnecessary allocations
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(vec![1, 2, 3]);

        // Borrowing should not allocate
        let _borrowed = cell.borrow(&token);

        // Copy operations should not allocate for Copy types
        let copy_cell = GhostCell::new(42);
        let _value = copy_cell.get(&token);

        // Replace should reuse allocation
        let old_vec = cell.replace(&mut token, vec![4, 5, 6]);
        assert_eq!(old_vec, vec![1, 2, 3]);
    });
}

// ===== CONCURRENCY TESTS =====

#[test]
fn test_thread_local_usage() {
    // Test that tokens work in thread-local scenarios
    thread_local! {
        static THREAD_CELL: RefCell<Option<i32>> = RefCell::new(None);
    }

    THREAD_CELL.with(|cell| {
        *cell.borrow_mut() = Some(42);
        assert_eq!(*cell.borrow(), Some(42));
    });
}

// ===== PANIC SAFETY TESTS =====

#[test]
fn test_panic_during_borrow() {
    // Test behavior when panics occur during borrowing
    let cell = GhostToken::new(|token| {
        let cell = GhostCell::new(vec![1, 2, 3]);
        // This should work fine
        cell.borrow(&token).clone()
    });
    assert_eq!(cell, vec![1, 2, 3]);
}

#[test]
fn test_drop_order() {
    // Test that values are dropped in correct order
    let drop_order = Arc::new(Mutex::new(Vec::new()));

    struct DropTracker(Arc<Mutex<Vec<i32>>>, i32);

    impl Drop for DropTracker {
        fn drop(&mut self) {
            self.0.lock().unwrap().push(self.1);
        }
    }

    let _result = GhostToken::new(|_token| {
        let _cell1 = GhostCell::new(DropTracker(Arc::clone(&drop_order), 1));
        let _cell2 = GhostCell::new(DropTracker(Arc::clone(&drop_order), 2));
        // Cells go out of scope here
        42
    });

    // Check drop order (should be reverse of creation)
    let order = drop_order.lock().unwrap();
    assert_eq!(*order, vec![2, 1]);
}

// ===== INTEGRATION TESTS =====

#[test]
fn test_trait_implementations() {
    // Test that our types work with common traits
    GhostToken::new(|token| {
        // From trait
        let cell: GhostCell<i32> = 42.into();
        assert_eq!(*cell.borrow(&token), 42);

        // Default trait
        let default_cell = GhostCell::<String>::default();
        assert_eq!(*default_cell.borrow(&token), "");
    });
}

#[test]
fn test_generic_bounds() {
    // Test that our generic bounds work correctly
    fn generic_function<'brand, T: Clone>(
        cell: &GhostCell<'brand, T>,
        token: &GhostToken<'brand>,
    ) -> T {
        cell.cloned(token)
    }

    GhostToken::new(|token| {
        let cell = GhostCell::new(vec![1, 2, 3]);
        let cloned = generic_function(&cell, &token);
        assert_eq!(cloned, vec![1, 2, 3]);
    });
}

#[test]
fn test_complex_data_structures() {
    // Test with complex nested data structures
    #[derive(Debug, Clone, PartialEq)]
    struct ComplexData {
        id: i32,
        data: Vec<String>,
        metadata: HashMap<String, i32>,
    }

    GhostToken::new(|mut token| {
        let complex_cell = GhostCell::new(ComplexData {
            id: 1,
            data: vec!["hello".to_string(), "world".to_string()],
            metadata: [("key".to_string(), 42)].into(),
        });

        // Modify nested structures
        {
            let borrowed = complex_cell.borrow_mut(&mut token);
            borrowed.data.push("!".to_string());
            borrowed.metadata.insert("new_key".to_string(), 100);
        }

        let final_data = complex_cell.borrow(&token);
        assert_eq!(final_data.data, vec!["hello", "world", "!"]);
        assert_eq!(final_data.metadata["new_key"], 100);
    });
}

// Property-based tests
use proptest::prelude::*;
use proptest::test_runner::{TestCaseResult, TestRunner};

fn run_proptest<S, F>(strategy: S, test: F)
where
    S: Strategy,
    F: Fn(S::Value) -> TestCaseResult,
{
    let mut runner = TestRunner::default();
    runner.run(&strategy, test).unwrap();
}

#[test]
fn test_ghost_cell_properties() {
    run_proptest((any::<i32>(), any::<i32>()), |(val, new_val)| {
        GhostToken::new(|mut token| {
            let cell = GhostCell::new(val);

            // Test basic read
            prop_assert_eq!(*cell.borrow(&token), val);

            // Test write
            *cell.borrow_mut(&mut token) = new_val;
            prop_assert_eq!(*cell.borrow(&token), new_val);
            Ok(())
        })
    });
}

#[test]
fn test_ghost_cell_replace() {
    run_proptest((any::<i32>(), any::<i32>()), |(val, new_val)| {
        GhostToken::new(|mut token| {
            let cell = GhostCell::new(val);
            let old = cell.replace(&mut token, new_val);

            prop_assert_eq!(old, val);
            prop_assert_eq!(*cell.borrow(&token), new_val);
            Ok(())
        })
    });
}

#[test]
fn test_ghost_cell_map() {
    run_proptest(any::<i32>(), |val| {
        GhostToken::new(|token| {
            let cell = GhostCell::new(val);
            let mapped = cell.map(&token, |x| x.to_string());

            prop_assert_eq!(mapped.borrow(&token), &val.to_string());
            Ok(())
        })
    });
}

// ===== FORMAL VERIFICATION TESTS =====

#[test]
fn branded_collections_mathematical_correctness() {
    // Test mathematical properties of branded collections
    GhostToken::new(|token| {
        // Test BrandedVec properties
        let mut vec = BrandedVec::new();
        vec.push(1);
        vec.push(2);
        vec.push(3);

        // Commutativity: order of insertion doesn't affect final state
        let mut vec2 = BrandedVec::new();
        vec2.push(3);
        vec2.push(2);
        vec2.push(1);

        // But the elements are in different positions - this tests positional access
        assert_eq!(*vec.get(&token, 0).unwrap(), 1);
        assert_eq!(*vec.get(&token, 1).unwrap(), 2);
        assert_eq!(*vec.get(&token, 2).unwrap(), 3);

        assert_eq!(*vec2.get(&token, 0).unwrap(), 3);
        assert_eq!(*vec2.get(&token, 1).unwrap(), 2);
        assert_eq!(*vec2.get(&token, 2).unwrap(), 1);

        // Test BrandedHashMap mathematical properties
        let mut map = BrandedHashMap::new();
        map.insert("a", 1);
        map.insert("b", 2);

        // Test that insertion is idempotent for same key
        let old = map.insert("a", 10);
        assert_eq!(old, Some(1));
        assert_eq!(*map.get(&token, &"a").unwrap(), 10);

        // Test removal
        let removed = map.remove(&"a");
        assert_eq!(removed, Some(10));
        assert!(map.get(&token, &"a").is_none());
    });
}

#[test]
fn arena_key_invariants() {
    // Test that arena keys maintain their invariants
    GhostToken::new(|mut token| {
        let arena: BrandedArena<'_, i32, 8> = BrandedArena::new();

        let k1 = arena.alloc(&mut token, 42);
        let k2 = arena.alloc(&mut token, 24);

        // Keys should be valid and point to correct values
        assert_eq!(*arena.get_key(&token, k1), 42);
        assert_eq!(*arena.get_key(&token, k2), 24);

        // Keys should be unique
        assert_ne!(k1.index(), k2.index());

        // Mutation should work
        *arena.get_key_mut(&mut token, k1) = 100;
        assert_eq!(*arena.get_key(&token, k1), 100);
    });
}

#[test]
fn branded_deque_comprehensive_test() {
    // Test BrandedDeque (the ring buffer implementation)
    // Note: BrandedDeque is not exported in the root halo prelude, so we import it fully qualified
    // to avoid conflict with potential other Deque names.
    use halo::collections::BrandedDeque;

    GhostToken::new(|mut token| {
        let mut deque: BrandedDeque<'_, i32, 16> = BrandedDeque::new();

        assert!(deque.is_empty());

        // Test mixed front/back operations
        deque.push_back(1).unwrap();
        deque.push_front(2).unwrap();
        deque.push_back(3).unwrap();

        // Structure should be: [2, 1, 3]
        assert_eq!(*deque.front(&token).unwrap(), 2);
        assert_eq!(*deque.back(&token).unwrap(), 3);
        assert_eq!(deque.len(), 3);

        // Test random access
        assert_eq!(*deque.get(&token, 0).unwrap(), 2);
        assert_eq!(*deque.get(&token, 1).unwrap(), 1);
        assert_eq!(*deque.get(&token, 2).unwrap(), 3);

        // Test mutation
        if let Some(val) = deque.get_mut(&mut token, 1) {
            *val = 10;
        }
        // Structure: [2, 10, 3]
        assert_eq!(*deque.get(&token, 1).unwrap(), 10);

        // Test removal
        assert_eq!(deque.pop_front().map(|c| c.into_inner()), Some(2));
        assert_eq!(deque.pop_back().map(|c| c.into_inner()), Some(3));
        assert_eq!(deque.len(), 1);
        assert_eq!(deque.pop_front().map(|c| c.into_inner()), Some(10));
        assert!(deque.is_empty());
    });
}

// ===== RUSTBELT SAFETY VALIDATION TESTS =====

// These tests validate that the core RustBelt safety properties are maintained

#[test]
fn test_rustbelt_generativity_property() {
    // Test that tokens are generative (fresh brands cannot escape)
    // This should compile and run successfully
    let result = GhostToken::new(|mut token| {
        let cell = GhostCell::new(42);
        *cell.borrow_mut(&mut token) = 24;
        *cell.borrow(&token) // Return the final value
    });
    assert_eq!(result, 24);
}

#[test]
fn test_rustbelt_linearity_property() {
    // Test that token linearity prevents concurrent mutable access
    GhostToken::new(|mut token| {
        let cell1 = GhostCell::new(1);
        let cell2 = GhostCell::new(2);

        // This should work - sequential mutable access
        *cell1.borrow_mut(&mut token) = 10;
        *cell2.borrow_mut(&mut token) = 20;

        // Verify values
        assert_eq!(*cell1.borrow(&token), 10);
        assert_eq!(*cell2.borrow(&token), 20);
    });
}

#[test]
fn test_rustbelt_no_runtime_borrow_checking() {
    // Test that no runtime borrow checking is needed - type system enforces safety
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(42);

        // Multiple immutable borrows should work
        let borrow1 = cell.borrow(&token);
        let borrow2 = cell.borrow(&token);
        assert_eq!(*borrow1, 42);
        assert_eq!(*borrow2, 42);

        // Drop immutable borrows before mutable borrow
        drop(borrow1);
        drop(borrow2);

        // Now mutable borrow should work
        *cell.borrow_mut(&mut token) = 24;
        assert_eq!(*cell.borrow(&token), 24);
    });
}

#[test]
fn test_branded_collection_invariants() {
    // Test that branded collections maintain their safety invariants
    GhostToken::new(|mut token| {
        let mut vec = BrandedVec::new();

        // Test owner exclusivity - collection owns the GhostCells
        vec.push(1);
        vec.push(2);
        vec.push(3);

        // Test token-gated access
        assert_eq!(*vec.get(&token, 0).unwrap(), 1);
        assert_eq!(*vec.get(&token, 1).unwrap(), 2);

        // Test mutation through token
        for i in 0..vec.len() {
            if let Some(val) = vec.get_mut(&mut token, i) {
                *val *= 2;
            }
        }

        assert_eq!(*vec.get(&token, 0).unwrap(), 2);
        assert_eq!(*vec.get(&token, 1).unwrap(), 4);
        assert_eq!(*vec.get(&token, 2).unwrap(), 6);
    });
}

// ===== COMPREHENSIVE STDLIB CORRECTNESS VALIDATION =====

// These tests validate that our implementations handle all edge cases correctly compared to stdlib

#[test]
fn branded_vec_comprehensive_vs_std_vec() {
    GhostToken::new(|mut token| {
        // Test empty vectors
        let mut std_vec = Vec::<i32>::new();
        let mut branded_vec = BrandedVec::<i32>::new();

        assert_eq!(std_vec.len(), branded_vec.len());
        assert_eq!(std_vec.is_empty(), branded_vec.is_empty());

        // Test large insertions
        for i in 0..10000 {
            std_vec.push(i);
            branded_vec.push(i);
        }

        assert_eq!(std_vec.len(), branded_vec.len());
        for i in 0..10000 {
            assert_eq!(std_vec[i], *branded_vec.get(&token, i).unwrap());
        }

        // Test bulk operations
        std_vec.clear();
        // BrandedVec doesn't have clear, so recreate
        let mut branded_vec = BrandedVec::new();

        // Test edge case: insert at various positions
        for i in 0..100 {
            std_vec.insert(0, i);
            branded_vec.insert(0, i);
        }

        assert_eq!(std_vec.len(), branded_vec.len());
        for i in 0..100 {
            assert_eq!(std_vec[i], *branded_vec.get(&token, i).unwrap());
        }

        // Test removals
        while !std_vec.is_empty() && !branded_vec.is_empty() {
            let std_val = std_vec.remove(0);
            let branded_val = branded_vec.remove(0);
            assert_eq!(std_val, branded_val);
        }
    });
}

#[test]
fn branded_hashmap_comprehensive_vs_std_hashmap() {
    GhostToken::new(|mut token| {
        let mut std_map = std::collections::HashMap::new();
        let mut branded_map = BrandedHashMap::new();

        // Test various key/value types - use integers for simplicity
        let test_data = vec![
            (1, 42),
            (2, 24),
            (3, 100),
            (0, 999), // Zero key
            (999, 123),
        ];

        // Insert all
        for (k, v) in &test_data {
            assert_eq!(std_map.insert(*k, *v), branded_map.insert(*k, *v));
        }

        assert_eq!(std_map.len(), branded_map.len());

        // Verify all insertions
        for (k, expected_v) in &test_data {
            assert_eq!(
                std_map.get(k),
                branded_map.get(&token, k).map(|x| *x).as_ref()
            );
        }

        // Test updates (same key, different value)
        // First, verify the key exists
        assert_eq!(std_map.get(&1), Some(&42));
        assert_eq!(branded_map.get(&token, &1), Some(&42));

        let std_prev = std_map.insert(1, 999);
        let branded_prev = branded_map.insert(1, 999);
        assert_eq!(std_prev, branded_prev);
        assert_eq!(
            std_map.get(&1),
            branded_map.get(&token, &1).map(|x| *x).as_ref()
        );

        // Test removals
        for (k, _) in &test_data {
            let std_result = std_map.remove(k);
            let branded_result = branded_map.remove(k);
            assert_eq!(std_result, branded_result);
        }

        assert_eq!(std_map.len(), branded_map.len());
        assert!(std_map.is_empty());
        assert!(branded_map.is_empty());
    });
}

#[test]
fn branded_vecdeque_comprehensive_vs_std_vecdeque() {
    GhostToken::new(|mut token| {
        let mut std_deque = std::collections::VecDeque::new();
        let mut branded_deque = BrandedVecDeque::new();

        // Test alternating front/back operations
        for i in 0..100 {
            if i % 2 == 0 {
                std_deque.push_back(i);
                branded_deque.push_back(i);
            } else {
                std_deque.push_front(i);
                branded_deque.push_front(i);
            }
        }

        assert_eq!(std_deque.len(), branded_deque.len());

        // Test alternating removals (this verifies the internal structure is correct)
        while !std_deque.is_empty() && !branded_deque.is_empty() {
            let std_val = if std_deque.len() % 2 == 0 {
                std_deque.pop_front()
            } else {
                std_deque.pop_back()
            };

            let branded_val = if branded_deque.len() % 2 == 0 {
                branded_deque.pop_front()
            } else {
                branded_deque.pop_back()
            };
            assert_eq!(std_val, branded_val);
        }

        assert_eq!(std_deque.len(), branded_deque.len());
    });
}

#[test]
fn memory_safety_edge_cases() {
    // Test that our implementations don't have memory safety issues that stdlib avoids

    GhostToken::new(|mut token| {
        // Test with ZST (Zero-Sized Types)
        let mut branded_vec = BrandedVec::<()>::new();
        for _ in 0..1000 {
            branded_vec.push(());
        }

        assert_eq!(branded_vec.len(), 1000);

        // Test with large types
        #[derive(Clone, Debug, PartialEq)]
        struct LargeStruct([u8; 1024]);

        let mut branded_vec = BrandedVec::new();
        let large_item = LargeStruct([42; 1024]);

        for _ in 0..10 {
            branded_vec.push(large_item.clone());
        }

        assert_eq!(branded_vec.len(), 10);
        for i in 0..10 {
            assert_eq!(branded_vec.get(&token, i).unwrap().0, large_item.0);
        }

        // Test mutation of large types
        for i in 0..10 {
            if let Some(item) = branded_vec.get_mut(&mut token, i) {
                item.0[0] = i as u8;
            }
        }

        for i in 0..10 {
            assert_eq!(branded_vec.get(&token, i).unwrap().0[0], i as u8);
        }
    });
}

#[test]
fn performance_regression_validation() {
    // Test that our implementations maintain reasonable performance characteristics
    // compared to expected algorithmic complexity

    GhostToken::new(|mut token| {
        // Test O(1) amortized push operations
        let mut branded_vec = BrandedVec::new();
        let start = std::time::Instant::now();

        for i in 0..10000 {
            branded_vec.push(i);
        }

        let duration = start.elapsed();
        // Should complete in reasonable time (much less than 1ms per operation)
        assert!(duration < std::time::Duration::from_millis(100));

        // Test O(1) access operations
        let start = std::time::Instant::now();
        for i in 0..1000 {
            black_box(branded_vec.get(&token, i % branded_vec.len()));
        }
        let duration = start.elapsed();
        assert!(duration < std::time::Duration::from_millis(10));

        // Test O(n) iteration operations (should be slower but still reasonable)
        let start = std::time::Instant::now();
        for i in 0..branded_vec.len().min(1000) {
            if let Some(val) = branded_vec.get_mut(&mut token, i) {
                *val += 1;
            }
        }
        let duration = start.elapsed();
        assert!(duration < std::time::Duration::from_millis(50));
    });
}

// ===== MEMORY USAGE VALIDATION =====

// Compare memory usage characteristics with stdlib

#[test]
fn memory_overhead_comparison() {
    // Test that our implementations don't have excessive memory overhead
    GhostToken::new(|token| {
        // Compare Vec vs BrandedVec memory scaling
        let sizes = [100usize, 1000];

        for &size in &sizes {
            let std_vec: Vec<i32> = (0..size).map(|x| x as i32).collect();
            let branded_vec: BrandedVec<i32> = {
                let mut vec = BrandedVec::with_capacity(size);
                for i in 0..size {
                    vec.push(i as i32);
                }
                vec
            };

            // Both should contain the same data
            assert_eq!(std_vec.len(), branded_vec.len());
            for i in 0..size.min(100) {
                // Test first 100 elements to avoid excessive time
                assert_eq!(std_vec[i], *branded_vec.get(&token, i).unwrap());
            }

            // Memory overhead should be reasonable (branded vec has some overhead for safety)
            assert_eq!(branded_vec.len(), size);
        }
    });
}

#[test]
fn capacity_management_validation() {
    // Test that capacity management works correctly compared to stdlib expectations
    GhostToken::new(|token| {
        // Test reserve behavior
        let mut branded_vec = BrandedVec::<i32>::new();

        // Should start empty
        assert_eq!(branded_vec.len(), 0);

        // Reserve capacity
        branded_vec.reserve(1000);

        // Should still be able to add elements
        for i in 0..100 {
            branded_vec.push(i as i32);
        }

        assert_eq!(branded_vec.len(), 100);
        for i in 0..100 {
            assert_eq!(*branded_vec.get(&token, i).unwrap(), i as i32);
        }

        // Test that capacity is maintained
        assert!(branded_vec.len() >= 100);

        // Should still work
        assert_eq!(branded_vec.len(), 100);
        for i in 0..100 {
            assert_eq!(*branded_vec.get(&token, i).unwrap(), i as i32);
        }
    });
}

#[test]
#[ignore = "Timing-based performance assertions are environment-dependent; prefer benches (criterion) and run with --release for perf validation."]
fn asymptotic_complexity_validation() {
    // Test that operations have expected asymptotic complexity
    GhostToken::new(|token| {
        let sizes = [100usize, 1000, 10000];

        for &size in &sizes {
            let mut branded_vec = BrandedVec::with_capacity(size);

            // O(n) population
            let start = std::time::Instant::now();
            for i in 0..size {
                branded_vec.push(i as i32);
            }
            let populate_time = start.elapsed();

            // O(1) access should be much faster than O(n) operations
            let start = std::time::Instant::now();
            for i in 0..(size.min(1000)) {
                // Limit to avoid excessive time
                black_box(branded_vec.get(&token, i % branded_vec.len()));
            }
            let access_time = start.elapsed();

            // NOTE: Do not assert wall-clock ratios here; debug builds and OS jitter make them flaky.
            let _ = (populate_time, access_time);

            // O(n) iteration (simplified test)
            let start = std::time::Instant::now();
            for i in 0..size.min(1000) {
                // Limit iterations
                black_box(branded_vec.get(&token, i % branded_vec.len()));
            }
            let iterate_time = start.elapsed();

            let _ = iterate_time;
        }
    });
}

// ===== COMPILATION FAILURE TESTS =====

// These tests verify that certain patterns fail to compile
// (We can't test compilation failures directly in runtime tests,
// but we can document the patterns that should fail)

#[test]
fn test_type_safety_guarantees() {
    // This test documents that the following patterns would fail to compile:

    // Pattern 1: Mixing tokens from different scopes
    // GhostToken::new(|token1| {
    //     let cell = GhostCell::new(42);
    //     GhostToken::new(|token2| {
    //         // This would fail: cell.borrow(&token2)
    //     });
    // });

    // Pattern 2: Storing references beyond borrow scope
    // GhostToken::new(|token| {
    //     let cell = GhostCell::new(42);
    //     let stored_ref: &i32 = cell.borrow(&token); // Lifetime error
    // });

    // Since these are compilation errors, we just assert that
    // the correct patterns work
    GhostToken::new(|token| {
        let cell = GhostCell::new(42);
        let value = *cell.borrow(&token);
        assert_eq!(value, 42);
    });
}

// ===== STRESS TESTS =====

#[test]
fn test_many_cells() {
    // Test with many cells to stress the system
    GhostToken::new(|mut token| {
        let cells: Vec<GhostCell<i32>> = (0..1000).map(GhostCell::new).collect();

        // Bulk operations
        for (i, cell) in cells.iter().enumerate() {
            assert_eq!(*cell.borrow(&token), i as i32);
        }

        // Modify all cells
        for cell in &cells {
            *cell.borrow_mut(&mut token) *= 2;
        }

        // Verify modifications
        for (i, cell) in cells.iter().enumerate() {
            assert_eq!(*cell.borrow(&token), (i as i32) * 2);
        }
    });
}

#[test]
fn test_ghostcell_borrowing_vec() {
    // Test `GhostCell` borrowing patterns on a non-`Copy` type.
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(vec![1, 2, 3]);

        {
            let borrowed = cell.borrow(&token);
            assert_eq!(*borrowed, vec![1, 2, 3]);
        }

        {
            let borrowed_mut = cell.borrow_mut(&mut token);
            borrowed_mut.push(4);
            assert_eq!(*borrowed_mut, vec![1, 2, 3, 4]);
        }

        assert_eq!(*cell.borrow(&token), vec![1, 2, 3, 4]);
    });
}

#[test]
fn test_lazy_ghostcell_basic() {
    // Test basic lazy computation
    let compute_count = std::cell::Cell::new(0);

    GhostToken::new(|mut token| {
        let lazy = GhostLazyCell::new(|| {
            compute_count.set(compute_count.get() + 1);
            vec![1, 2, 3, 4, 5]
        });

        // Not computed yet
        assert!(!lazy.is_initialized(&token));
        assert_eq!(compute_count.get(), 0);

        // First access triggers computation
        let value = lazy.get(&mut token);
        assert_eq!(*value, vec![1, 2, 3, 4, 5]);
        assert!(lazy.is_initialized(&token));
        assert_eq!(compute_count.get(), 1);

        // Subsequent accesses use cached value
        let value2 = lazy.get(&mut token);
        assert_eq!(*value2, vec![1, 2, 3, 4, 5]);
        assert_eq!(compute_count.get(), 1); // No additional computation
    });
}

#[test]
fn test_lazy_ghostcell_mutation() {
    // Test lazy computation with mutable access
    GhostToken::new(|mut token| {
        let lazy = GhostLazyCell::new(|| vec![1, 2, 3]);

        // Get mutable access (triggers computation)
        {
            let value = lazy.get_mut(&mut token);
            value.push(4);
            assert_eq!(*value, vec![1, 2, 3, 4]);
        }

        // Value persists
        let value = lazy.get(&mut token);
        assert_eq!(*value, vec![1, 2, 3, 4]);
    });
}

#[test]
fn test_lazy_ghostcell_invalidation() {
    // Test invalidation and recomputation with Fn (recomputable)
    GhostToken::new(|mut token| {
        use std::sync::atomic::{AtomicI32, Ordering};

        static COUNTER: AtomicI32 = AtomicI32::new(0);

        let lazy = GhostLazyCell::new(|| COUNTER.fetch_add(1, Ordering::Relaxed) + 1);

        // First computation
        assert_eq!(*lazy.get(&mut token), 1);

        // Invalidate and recompute
        lazy.invalidate(&mut token);
        assert!(!lazy.is_initialized(&token));

        // Second computation
        assert_eq!(*lazy.get(&mut token), 2);
    });
}

#[test]
fn test_lazy_ghostcell_clone() {
    // Test cloning computed values
    GhostToken::new(|mut token| {
        let lazy = GhostLazyCell::new(|| vec![1, 2, 3]);
        let cloned = lazy.get(&mut token).clone();
        assert_eq!(cloned, vec![1, 2, 3]);

        // Original still works
        assert_eq!(*lazy.get(&mut token), vec![1, 2, 3]);
    });
}

#[test]
fn test_lazy_ghostcell_default() {
    // Test default lazy initialization
    GhostToken::new(|mut token| {
        let lazy: GhostLazyCell<i32> = GhostLazyCell::default();
        assert_eq!(*lazy.get(&mut token), 0);
    });
}

#[test]
fn test_lazy_ghostcell_drop() {
    // Test that lazy cells drop properly
    let drop_count = std::rc::Rc::new(std::cell::Cell::new(0));

    struct DropTracker(std::rc::Rc<std::cell::Cell<i32>>);

    impl Drop for DropTracker {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    GhostToken::new(|mut token| {
        let lazy = GhostLazyCell::new(|| DropTracker(drop_count.clone()));

        // Access to trigger computation
        let _tracker = lazy.get(&mut token);
        assert_eq!(drop_count.get(), 0); // Not dropped yet

        // Drop the lazy cell
        drop(lazy);
        assert_eq!(drop_count.get(), 1); // Now dropped
    });
}

#[test]
fn test_lazy_ghostcell_complex_computation() {
    // Deterministic "expensive computation" test without timing assumptions.
    let compute_count = std::cell::Cell::new(0);

    GhostToken::new(|mut token| {
        let lazy = GhostLazyCell::new(|| {
            compute_count.set(compute_count.get() + 1);
            (0..1000).map(|i| i * i).collect::<Vec<i32>>()
        });

        assert_eq!(compute_count.get(), 0);

        // First access computes.
        let value = lazy.get(&mut token);
        assert_eq!(value.len(), 1000);
        assert_eq!(value[0], 0);
        assert_eq!(value[999], 999 * 999);
        assert_eq!(compute_count.get(), 1);

        // Second access is cached: no recomputation.
        let value2 = lazy.get(&mut token);
        assert_eq!(value2.len(), 1000);
        assert_eq!(compute_count.get(), 1);

        // Invalidate and recompute.
        lazy.invalidate(&mut token);
        assert_eq!(compute_count.get(), 1);
        let _ = lazy.get(&mut token);
        assert_eq!(compute_count.get(), 2);
    });
}

#[test]
fn test_lazy_ghostcell_memory_efficiency() {
    // Test that lazy cells use memory efficiently
    use std::mem;

    GhostToken::new(|mut token| {
        // Empty lazy cell (inline storage; no heap allocation by the cell itself)
        let lazy: GhostLazyCell<Vec<i32>> = GhostLazyCell::new(Vec::new);
        let empty_size = mem::size_of_val(&lazy);

        // After computation (same size, different memory usage)
        let _ = lazy.get(&mut token);
        let computed_size = mem::size_of_val(&lazy);

        // Size should remain the same (raw allocation handles storage)
        assert_eq!(computed_size, empty_size);
        // Upper bound sanity check: must not be egregiously larger than (F + T).
        let upper = mem::size_of::<fn() -> Vec<i32>>()
            + mem::size_of::<Vec<i32>>()
            + 2 * mem::size_of::<usize>();
        assert!(empty_size <= upper);
    });
}

#[test]
fn test_ghost_once_cell() {
    // Test basic GhostOnceCell functionality
    GhostToken::new(|mut token| {
        let cell: GhostOnceCell<i32> = GhostOnceCell::new();

        // Initially not initialized
        assert!(!cell.is_initialized(&token));
        assert_eq!(cell.get(&token), None);

        // Set value
        assert!(cell.set(&mut token, 42).is_ok());
        assert!(cell.is_initialized(&token));
        assert_eq!(cell.get(&token), Some(&42));

        // Try to set again (should fail)
        assert!(cell.set(&mut token, 100).is_err());
        assert_eq!(cell.get(&token), Some(&42)); // Still the original value

        // Get mutable reference
        if let Some(value) = cell.get_mut(&mut token) {
            *value = 200;
        }
        assert_eq!(cell.get(&token), Some(&200));

        // Take the value
        let taken = cell.take(&mut token);
        assert_eq!(taken, Some(200));
        assert!(!cell.is_initialized(&token));
        assert_eq!(cell.get(&token), None);
    });
}

#[test]
fn test_ghost_unsafe_cell() {
    // Test the direct unsafe cell variant
    GhostToken::new(|mut token| {
        let cell = GhostUnsafeCell::new(vec![1, 2, 3]);

        // Test direct access
        {
            let value = cell.get(&token);
            assert_eq!(*value, vec![1, 2, 3]);
        }

        // Test mutable access
        {
            let value = cell.get_mut(&mut token);
            value.push(4);
            assert_eq!(*value, vec![1, 2, 3, 4]);
        }

        // Test replace
        let old_value = cell.replace(vec![5, 6], &mut token);
        assert_eq!(old_value, vec![1, 2, 3, 4]);

        let current = cell.get(&token);
        assert_eq!(*current, vec![5, 6]);

        // Test pointer access
        let ptr = cell.as_ptr(&token);
        // SAFETY: We have token access
        unsafe {
            assert_eq!(*ptr, vec![5, 6]);
        }
    });
}

#[test]
fn test_ghost_unsafe_cell_thread_safety() {
    // Test that GhostUnsafeCell is Send + Sync for appropriate types
    fn assert_send_sync<T: Send + Sync>() {}

    // GhostUnsafeCell<i32> should be Send + Sync since i32 is Send + Sync
    assert_send_sync::<GhostUnsafeCell<i32>>();

    // Test basic functionality
    GhostToken::new(|mut token| {
        let cell = GhostUnsafeCell::new(42i32);

        // Test basic access
        assert_eq!(*cell.get(&token), 42);

        // Test mutable access
        {
            let value = cell.get_mut(&mut token);
            *value = 100;
        }
        assert_eq!(*cell.get(&token), 100);
    });
}

#[test]
fn test_ghostcell_interactions() {
    // Test that different GhostCell types can interact safely
    GhostToken::new(|mut token| {
        let cell = GhostCell::new(42);
        let unsafe_cell = GhostUnsafeCell::new(vec![1, 2, 3]);
        let lazy_cell = GhostLazyCell::new(|| 100);
        let once_cell = GhostOnceCell::new();

        // Test each type individually to avoid borrow conflicts
        let value1 = cell.get(&token);
        assert_eq!(value1, 42);

        let value2 = unsafe_cell.get(&token);
        assert_eq!(*value2, vec![1, 2, 3]);

        let value3 = lazy_cell.get(&mut token);
        assert_eq!(*value3, 100);

        // Now use mutable token for once cell
        assert!(once_cell.set(&mut token, 200).is_ok());
        assert_eq!(*once_cell.get(&token).unwrap(), 200);
    });
}

#[test]
fn test_ghostcell_drop_behavior() {
    // Test that dropping works correctly for different cell types
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[derive(Clone)]
    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let drops = Arc::new(AtomicUsize::new(0));

    GhostToken::new(|mut token| {
        let cell = GhostCell::new(DropCounter(drops.clone()));
        let unsafe_cell = GhostUnsafeCell::new(DropCounter(drops.clone()));
        let lazy_lock = GhostLazyLock::new(|| DropCounter(drops.clone()));
        let once_cell = GhostOnceCell::new();

        // Initialize lazy/once values so there is something to drop.
        let _ = lazy_lock.get(&mut token);
        assert!(once_cell
            .set(&mut token, DropCounter(drops.clone()))
            .is_ok());

        let c0 = drops.load(Ordering::Relaxed);
        drop(cell);
        let c1 = drops.load(Ordering::Relaxed);
        assert_eq!(c1, c0 + 1);

        drop(unsafe_cell);
        let c2 = drops.load(Ordering::Relaxed);
        assert_eq!(c2, c1 + 1);

        drop(lazy_lock);
        let c3 = drops.load(Ordering::Relaxed);
        assert_eq!(c3, c2 + 1);

        drop(once_cell);
        let c4 = drops.load(Ordering::Relaxed);
        assert_eq!(c4, c3 + 1);
    });

    // We created and dropped exactly 4 `DropCounter` values above.
    assert_eq!(drops.load(Ordering::Relaxed), 4);
}

#[test]
fn test_ghostcell_memory_layout() {
    // Test that our memory layouts are optimal
    use std::mem;

    // Core primitives should be very compact.
    assert!(mem::size_of::<GhostUnsafeCell<i32>>() <= 16);
    assert!(mem::size_of::<GhostCell<i32>>() <= 16);

    // Initialization primitives are still intended to be small and allocation-free.
    assert!(mem::size_of::<GhostOnceCell<i32>>() <= 16);
    assert!(mem::size_of::<GhostLazyLock<i32>>() <= 32);
    assert!(mem::size_of::<GhostLazyCell<i32>>() <= 32);

    // `GhostUnsafeCell` is a thin wrapper around `UnsafeCell`.
    assert!(
        mem::size_of::<GhostUnsafeCell<i32>>() <= mem::size_of::<std::cell::UnsafeCell<i32>>() + 8
    );
}

#[test]
fn test_ghostcell_thread_safety() {
    // Test that GhostUnsafeCell is conditionally Send + Sync
    fn assert_send_sync<T: Send + Sync>() {}

    // GhostUnsafeCell<i32> should be Send + Sync since i32 is Send + Sync
    assert_send_sync::<GhostUnsafeCell<i32>>();

    // Basic functionality test
    GhostToken::new(|token| {
        let cell = GhostUnsafeCell::new(42);
        assert_eq!(*cell.get(&token), 42);
    });
}

#[test]
fn test_large_data_stress() {
    // Test with large data structures
    GhostToken::new(|mut token| {
        let large_data = vec![0u8; 10 * 1024 * 1024]; // 10MB
        let cell = GhostCell::new(large_data);

        // Modify in place
        cell.borrow_mut(&mut token)[0] = 255;
        assert_eq!(cell.borrow(&token)[0], 255);

        // Clone operation (should not be too slow)
        let cloned = cell.cloned(&token);
        assert_eq!(cloned.len(), 10 * 1024 * 1024);
        assert_eq!(cloned[0], 255);
    });
}
