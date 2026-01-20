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
#[macro_export]
macro_rules! scope {
    (|$token:ident| $body:expr) => {
        $crate::GhostToken::new(|$token| $body)
    };
    (|$token:ident| $body:block) => {
        $crate::GhostToken::new(|$token| $body)
    };
}
