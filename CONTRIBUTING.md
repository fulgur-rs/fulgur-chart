# Contributing to fulgur-chart

Thanks for your interest in contributing! `fulgur-chart` turns a chart.js v4–compatible
JSON spec into a **deterministic** static SVG/PNG chart. The most important rule below is
the determinism contract — please read that section even if you skip the rest.

By participating you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Scope

This project renders a **data-only, static subset** of chart.js v4. JavaScript-driven
features (callbacks, animation, interaction, plugin scripts) are intentionally out of
scope. When proposing a new option or chart type, please reference the corresponding
chart.js v4 behavior so we can keep the output faithful.

## Development setup

- Rust **1.85.0** or newer (the workspace MSRV; see `rust-version` in `Cargo.toml`).
- No other system dependencies — the renderer bundles its own font (Noto Sans JP) and
  does not read system fonts.

```sh
git clone https://github.com/fulgur-rs/fulgur-chart.git
cd fulgur-chart
cargo build --workspace
```

### Git hooks (recommended)

We use [lefthook](https://lefthook.dev) to manage git hooks. A `pre-commit` hook
runs `rustfmt` on the staged Rust files and re-stages the result, so your commits are
always formatted (and the CI `cargo fmt --check` gate stays green).

The `lefthook` binary is pinned via [mise](https://mise.jdx.dev) in
[`mise.toml`](mise.toml), so everyone uses the same version. With mise installed:

```sh
mise install        # provisions the pinned lefthook binary
mise run setup      # wires the git hooks into this clone (runs `lefthook install`)
```

(Not using mise? Install lefthook yourself — `brew install lefthook`,
`go install github.com/evilmartians/lefthook@latest`, … — then run `lefthook install`.)

The hook config lives in [`lefthook.yml`](lefthook.yml). It's optional but recommended;
CI enforces formatting regardless. Note that because the hook reformats and re-stages
the whole file, a partially-staged file (`git add -p`) will have its unstaged hunks
pulled into the commit too — stage the whole file when you rely on the hook.

## Before you open a pull request

Run the same checks CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. `clippy` warnings are treated as errors in CI.

## The determinism contract (please read)

The core guarantee of this project is that **the same input spec always produces
byte-identical output**. CI and downstream reports rely on this. When contributing,
avoid anything that breaks determinism:

- No system fonts, wall-clock time, randomness, or environment-dependent behavior in
  the render path.
- Beware of nondeterministic iteration order (e.g. `HashMap`); prefer ordered
  structures when iteration order affects output.
- Floating-point formatting and rounding must be stable across platforms.

If your change intentionally alters rendered output, the snapshot and golden tests will
fail — that is expected. Update them deliberately as described below and include the
regenerated artifacts in your PR.

### Snapshot tests (insta)

SVG output is covered by [`insta`](https://insta.rs) snapshot tests. When output changes
on purpose, review and accept the new snapshots:

```sh
cargo insta review                 # interactive review (needs: cargo install cargo-insta)
# or, to accept all without review:
INSTA_UPDATE=always cargo test --workspace
```

Commit the updated `*.snap` files together with the code change, and describe in the PR
why the output changed.

### Golden PNG tests

A few representative specs are rasterized to PNG and compared against committed golden
images (with a small pixel tolerance). Regenerate the goldens only when a rendering
change is intended, then **visually verify** the new PNGs before committing:

```sh
UPDATE_GOLDEN=1 cargo test -p fulgur-chart --test golden_png
```

## Commit messages

We follow [Conventional Commits](https://www.conventionalcommits.org/) (e.g. `fix:`,
`feat:`, `docs(examples):`). Keep commits focused and the working tree formatted.

## Reporting bugs and proposing features

Open a GitHub issue using the provided templates. For a bug, a **minimal reproducing
spec** plus the exact command and the `fulgur-chart --version` (or commit hash) makes
triage much faster. For security issues, do **not** open a public issue — see
[SECURITY.md](SECURITY.md).

## License

Unless you state otherwise, any contribution you submit is licensed under the project's
dual license, **MIT OR Apache-2.0**, matching the terms of the repository (inbound =
outbound). See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
