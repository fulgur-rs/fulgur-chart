## Summary

<!-- Describe the user-visible change and any notable implementation details. -->

## Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Updated and reviewed `insta` snapshots when SVG output changed
- [ ] Regenerated and visually verified golden PNGs when raster output changed
- [ ] Not applicable / docs-only change

## Checklist

- [ ] The change preserves deterministic output for identical inputs
- [ ] New or changed chart behavior is covered by tests
- [ ] User-facing behavior is documented where appropriate
- [ ] Security-sensitive reports were handled privately, not in a public issue
