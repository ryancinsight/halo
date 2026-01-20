/// A macro helper for creating a new GhostToken scope.
///
/// This provides a slightly more ergonomic syntax for `GhostToken::new`.
///
/// # Example
///
/// ```rust
/// use halo::scope;
///
/// scope! { |token|
///     // use token
/// }
/// ```
///
/// You can also create a nested scope from an existing token:
///
/// ```rust
/// use halo::{scope, GhostToken};
///
/// scope! { |token|
///     scope! { |sub_token| in token {
///         // use sub_token
///     }}
/// }
/// ```
#[macro_export]
macro_rules! scope {
    (|$sub_token:ident| in $token:ident, $body:expr) => {
        $token.with_scoped(|$sub_token| $body)
    };
    (|$sub_token:ident| in $token:ident $body:block) => {
        $token.with_scoped(|$sub_token| $body)
    };
    (|$token:ident| $body:expr) => {
        $crate::GhostToken::new(|$token| $body)
    };
    (|$token:ident| $body:block) => {
        $crate::GhostToken::new(|$token| $body)
    };
}

#[cfg(test)]
mod tests {
    use crate::GhostToken;

    #[test]
    fn test_scope_macro_basic() {
        let res = scope!(|token| {
            10
        });
        assert_eq!(res, 10);
    }

    #[test]
    fn test_scope_macro_nested() {
        scope!(|token| {
            let res = scope!(|sub| in token {
                20
            });
            assert_eq!(res, 20);
        });
    }

    #[test]
    fn test_scope_macro_nested_expr() {
        scope!(|token| {
            let res = scope!(|sub| in token, 30);
            assert_eq!(res, 30);
        });
    }
}
