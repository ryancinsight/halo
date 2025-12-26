# UB checking workflow

This crate contains `unsafe` code (by design) in small, audited areas. To increase confidence that refactors did not introduce undefined behavior, use:

## Miri (interpreter + UB detector)

If you have the Miri component installed:

```bash
cargo +nightly miri test
```

If Miri is not installed yet:

```bash
rustup +nightly component add miri
```

Notes:
- Miri is slower; use it as a periodic audit step, not on every edit.
- Some concurrency-heavy tests may need adjustments for Miri limitations.







