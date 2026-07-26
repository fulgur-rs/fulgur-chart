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

dhat measures allocation bytes for the E2E JSON-to-SVG and JSON-to-PNG paths
of every representative case, compared against the committed
`membench_baseline.json`. SVG entries keep the case name; `_png` identifies PNG
entries. CI fails if any target exceeds the baseline by more than the threshold
(default +25%).

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

The numbers are deterministic for a fixed compiler, but `std`'s allocation
patterns can shift across Rust releases, and CI runs on a floating `stable`
toolchain. The default +25% threshold absorbs normal drift; if a toolchain bump
ever pushes a case over it without a real regression, regenerate the baseline
with `--update` and commit.

The `dhat-heap` feature is required for `membench` (it installs the dhat global
allocator); `required-features` keeps dhat out of normal builds and tests.
