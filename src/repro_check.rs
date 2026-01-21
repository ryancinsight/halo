
#[cfg(test)]
mod tests {
    use crate::collections::vec::vec::BrandedArray;
    use crate::token::GhostToken;

    struct NoDefault {
        val: i32,
    }

    #[test]
    fn test_no_default() {
        GhostToken::new(|mut token| {
            let mut arr: BrandedArray<'_, NoDefault, 4> = BrandedArray::new();
            arr.push(NoDefault { val: 1 });
            let popped = arr.pop();
            assert!(popped.is_some());
            assert_eq!(popped.unwrap().val, 1);
        });
    }
}
