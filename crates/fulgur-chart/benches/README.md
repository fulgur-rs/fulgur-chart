# Benchmarks

Two bench targets measure rendering performance for representative chart cases
(small + synthetic-large), generated in `cases.rs`.

## Speed (`render`) — report-only

criterion times the E2E pipeline (JSON → SVG, JSON → PNG). It never gates CI:
wall-clock on shared runners is too noisy. CI archives `target/criterion`.

```bash
cargo bench -p fulgur-chart --bench render            # full run
cargo bench -p fulgur-chart --bench render -- --test  # quick smoke (each case once)
```

## Memory (`membench`) — deterministic gate

dhat measures per-case allocation bytes (deterministic), compared against the
committed `membench_baseline.json`. CI fails if any case exceeds the baseline by
more than the threshold (default +25%).

```bash
# Print current numbers
cargo bench -p fulgur-chart --bench membench --features dhat-heap

# Gate against the baseline (what CI runs)
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check

# Custom threshold
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --check --threshold 30
```

### Updating the baseline

When an intentional change alters allocations (including adding/removing a case),
regenerate and commit the baseline:

```bash
cargo bench -p fulgur-chart --bench membench --features dhat-heap -- --update
git add crates/fulgur-chart/benches/membench_baseline.json
```

The `dhat-heap` feature is required for `membench` (it installs the dhat global
allocator); `required-features` keeps dhat out of normal builds and tests.
