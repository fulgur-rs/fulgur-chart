# @fulgur-rs/chart-cli

Zero-install `npx` distribution of the `fulgur-chart` Rust CLI.

## Usage

```bash
npx @fulgur-rs/chart-cli render spec.json -o out.svg
npx @fulgur-rs/chart-cli render spec.json -o out.png --format png
npx @fulgur-rs/chart-cli schema
```

## Supported platforms

- Linux x64 (glibc)
- Linux x64 (musl)
- Linux arm64
- macOS arm64
- macOS x64
- Windows x64

Unsupported platforms exit with code 1 and a clear error message.
