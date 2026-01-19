//! RustBelt / GhostCell paper style example: branded vector (Section 2 pattern).
//!
//! This demonstrates the core idea: a single linear token grants permission to
//! access many independently mutable elements stored in a branded collection.

use halo::{BrandedVec, GhostToken};

fn main() {
    GhostToken::new(|mut token| {
        let mut v: BrandedVec<'_, i32> = BrandedVec::new();
        v.push(1);
        v.push(2);
        v.push(3);

        // Shared reads: require only `&GhostToken`.
        let sum: i32 = v.iter(&token).copied().sum();
        assert_eq!(sum, 6);

        // Exclusive writes: require `&mut GhostToken`.
        v.for_each_mut(&mut token, |x| *x *= 10);
        let out: Vec<i32> = v.iter(&token).copied().collect();
        assert_eq!(out, vec![10, 20, 30]);
    });
}
