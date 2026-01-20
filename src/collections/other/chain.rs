//! `BrandedChain` — a combinator that chains two branded collections.
//!
//! This collection allows treating two token-gated collections as a single sequence
//! without materializing a new collection. It implements `ZeroCopyOps` to allow
//! searching across both collections efficiently.

use crate::collections::{BrandedCollection, ZeroCopyOps};
use crate::GhostToken;

/// A collection that chains two other branded collections.
pub struct BrandedChain<C1, C2> {
    first: C1,
    second: C2,
}

impl<C1, C2> BrandedChain<C1, C2> {
    /// Creates a new chained collection.
    pub fn new(first: C1, second: C2) -> Self {
        Self { first, second }
    }

    /// Consumes the chain and returns the inner collections.
    pub fn into_inner(self) -> (C1, C2) {
        (self.first, self.second)
    }
}

impl<'brand, C1, C2> BrandedCollection<'brand> for BrandedChain<C1, C2>
where
    C1: BrandedCollection<'brand>,
    C2: BrandedCollection<'brand>,
{
    fn is_empty(&self) -> bool {
        self.first.is_empty() && self.second.is_empty()
    }

    fn len(&self) -> usize {
        self.first.len() + self.second.len()
    }
}

impl<'brand, T, C1, C2> ZeroCopyOps<'brand, T> for BrandedChain<C1, C2>
where
    C1: ZeroCopyOps<'brand, T>,
    C2: ZeroCopyOps<'brand, T>,
{
    fn find_ref<'a, F>(&'a self, token: &'a GhostToken<'brand>, f: F) -> Option<&'a T>
    where
        F: Fn(&T) -> bool,
    {
        self.first
            .find_ref(token, &f)
            .or_else(|| self.second.find_ref(token, f))
    }

    fn any_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.first.any_ref(token, &f) || self.second.any_ref(token, f)
    }

    fn all_ref<F>(&self, token: &GhostToken<'brand>, f: F) -> bool
    where
        F: Fn(&T) -> bool,
    {
        self.first.all_ref(token, &f) && self.second.all_ref(token, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::BrandedVec;
    use crate::GhostToken;

    #[test]
    fn test_branded_chain_basic() {
        GhostToken::new(|token| {
            let mut v1 = BrandedVec::new();
            v1.push(1);
            v1.push(2);

            let mut v2 = BrandedVec::new();
            v2.push(3);
            v2.push(4);

            let chain = BrandedChain::new(v1, v2);

            assert_eq!(chain.len(), 4);
            assert!(!chain.is_empty());

            // Test find_ref
            assert_eq!(chain.find_ref(&token, |&x| x == 2), Some(&2));
            assert_eq!(chain.find_ref(&token, |&x| x == 3), Some(&3));
            assert_eq!(chain.find_ref(&token, |&x| x == 5), None);

            // Test any_ref
            assert!(chain.any_ref(&token, |&x| x == 4));
            assert!(!chain.any_ref(&token, |&x| x == 9));

            // Test all_ref
            assert!(chain.all_ref(&token, |&x| x > 0));
            assert!(!chain.all_ref(&token, |&x| x < 3));
        });
    }
}
