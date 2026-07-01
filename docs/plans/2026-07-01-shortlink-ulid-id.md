# Shortlink ID: content-hash → ULID Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the shortlink id generation scheme in `chart-server` from a deterministic 48-bit content-hash (`SHA256(spec+params)[:6]`) to a server-generated, non-deterministic, time-based 128-bit ULID (Crockford base32, 26 chars).

**Architecture:** Remove the `compute_id` function in `crates/chart-server/src/handlers/shortlink.rs`. Replace its single call site in `post_create` with `Ulid::new().to_string()`. No other component (the `ShortlinkBackend` trait, `ShortlinkStore`, routing, ETag logic) needs to change because the id has always been treated as an opaque `String` end-to-end. Existing determinism-dependent unit tests are removed and replaced with tests that assert the new id shape (26-char Crockford base32) and non-determinism (identical specs now produce distinct ids/entries — dedup is intentionally lost, per the beads issue's accepted trade-off).

**Tech Stack:** Rust, `ulid` crate (new direct dependency, `ulid = "1.2"`, `rand` feature default-enabled), axum, existing `sha2`/`hex` (kept — still used by `response.rs::etag_value` for the unrelated content-based ETag on `/chart?{query}`).

**Related beads issue:** `fulgur-chart-8tr.5` (design + acceptance criteria stored on the issue).

---

## Task 1: Add the `ulid` dependency

**Files:**
- Modify: `crates/chart-server/Cargo.toml`

**Step 1: Add the dependency**

```bash
cd crates/chart-server && cargo add ulid@1.2
```

This adds a line under `[dependencies]`:
```toml
ulid = "1.2"
```

**Step 2: Verify the workspace still builds**

Run: `cargo check -p chart-server`
Expected: succeeds (the dependency is unused so far — no warning, Rust doesn't warn on unused crate deps by default).

**Step 3: Commit**

```bash
git add crates/chart-server/Cargo.toml Cargo.lock
git commit -m "chore(chart-server): add ulid dependency"
```

---

## Task 2: Replace `compute_id` with ULID generation (TDD)

**Files:**
- Modify: `crates/chart-server/src/handlers/shortlink.rs`

**Step 1: Write the failing tests**

Add two new tests to the `http_tests` module (after `create_succeeds_within_limits`, before the closing `}` of the module). Do **not** touch `compute_id` or `post_create` yet — these tests must fail against the current (hash-based) implementation:

```rust
    /// ULID は非決定的なので、同一 spec を連投しても別エントリ(別 URL)になる
    /// (content-hash 時代の dedup は意図的に失われる — 8tr.5 の受容済みトレードオフ)。
    #[tokio::test]
    async fn create_generates_distinct_ids_for_identical_specs() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let router = router_with_store(store);
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;

        let (status1, b1) = status_and_body(
            router.clone().oneshot(create_request(body)).await.unwrap(),
        )
        .await;
        assert_eq!(status1, StatusCode::OK, "body={b1}");

        let (status2, b2) =
            status_and_body(router.oneshot(create_request(body)).await.unwrap()).await;
        assert_eq!(status2, StatusCode::OK, "body={b2}");

        assert_ne!(
            b1, b2,
            "identical spec should produce distinct shortlink URLs (no dedup)"
        );
    }

    /// 返却される id は ULID の文字列表現: 26 文字の Crockford base32。
    #[tokio::test]
    async fn create_returns_url_with_26_char_ulid_id() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let resp = router_with_store(store)
            .oneshot(create_request(body))
            .await
            .unwrap();
        let (status, body) = status_and_body(resp).await;
        assert_eq!(status, StatusCode::OK, "body={body}");

        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let url = json["url"].as_str().unwrap();
        let id = url
            .strip_prefix("/chart/s/")
            .expect("url should be /chart/s/{id}");
        assert_eq!(id.len(), 26, "ULID string repr should be 26 chars, got: {id}");
        assert!(
            id.chars()
                .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c.to_ascii_uppercase())),
            "id should be valid Crockford base32: {id}"
        );
    }
```

Note: `http_tests` already imports `serde_json` transitively is NOT guaranteed — check the top of the `http_tests` module's `use` block. If `serde_json::Value` isn't in scope, add `use serde_json;` is unnecessary since it can be referenced via fully-qualified path `serde_json::Value` as written above (the crate is already a dependency of `chart-server`), no new `use` needed.

**Step 2: Run tests to verify the new two fail, everything else still passes**

Run: `cargo test -p chart-server create_generates_distinct_ids_for_identical_specs`
Run: `cargo test -p chart-server create_returns_url_with_26_char_ulid_id`
Expected: both FAIL —
- `create_generates_distinct_ids_for_identical_specs`: fails at `assert_ne!(b1, b2, ...)` because the current hash-based id is identical for identical bodies (dedup still active).
- `create_returns_url_with_26_char_ulid_id`: fails at `assert_eq!(id.len(), 26, ...)` because the current id is 12 hex chars.

**Step 3: Implement — replace `compute_id` with ULID generation**

In `crates/chart-server/src/handlers/shortlink.rs`:

1. Replace the import line:
```rust
use sha2::{Digest, Sha256};
```
with:
```rust
use ulid::Ulid;
```

2. Delete the entire `compute_id` function (lines 28-46).

3. In `post_create`, replace:
```rust
    let id = compute_id(
        &json,
        req.format.as_str(),
        req.width,
        req.height,
        // "_" を番兵として None と Some("") を区別する（Some("") は空文字列をそのまま使用）。
        req.background_color.as_deref(),
    );
```
with:
```rust
    let id = Ulid::new().to_string();
```

4. Delete the entire `mod tests { ... }` block (the unit tests `none_and_empty_string_background_produce_different_ids` and `same_params_produce_same_id`, lines 165-187 in the original) — both directly called `compute_id`, which no longer exists.

5. Update the now-stale comment in `http_tests::create_returns_503_when_store_full` (originally line 273):
```rust
        // 別スペック → 別 id → 新規挿入だが件数上限(1)に達しているため 503。
```
to:
```rust
        // ULID は非決定的なので spec に関わらず新規挿入だが、件数上限(1)に達しているため 503。
```

**Step 4: Run the full test suite to verify everything passes**

Run: `cargo test -p chart-server`
Expected: all tests pass, including the two new ones. `same_params_produce_same_id` and `none_and_empty_string_background_produce_different_ids` no longer appear (deleted, not skipped).

**Step 5: Commit**

```bash
git add crates/chart-server/src/handlers/shortlink.rs
git commit -m "feat(chart-server): generate shortlink ids as ULIDs instead of content-hash"
```

---

## Task 3: Update the stale determinism comment in `store.rs`

**Files:**
- Modify: `crates/chart-server/src/store.rs:56`

**Step 1: Update the comment**

Change:
```rust
                // 同一 id は決定的に同一 query になるため通常 old_len == query_len。
                // 一般化のためサイズ差分を正しく会計する。
```
to:
```rust
                // id は非決定的(ULID)なため通常この分岐(Occupied)は発生しない。
                // 衝突や将来の backend 実装差異に備え、サイズ差分を正しく会計する一般化ロジックとして保持。
```

This is a comment-only change; no test changes needed (no test asserts the comment content, and the store logic itself is already generic — see design notes on `fulgur-chart-8tr.5`).

**Step 2: Verify nothing broke**

Run: `cargo test -p chart-server store::tests`
Expected: all `store::tests::*` pass unchanged.

**Step 3: Commit**

```bash
git add crates/chart-server/src/store.rs
git commit -m "docs(chart-server): update stale determinism comment in ShortlinkStore::insert"
```

---

## Task 4: Full verification sweep

**Files:** none (verification only)

**Step 1: Run the full chart-server test suite**

Run: `cargo test -p chart-server`
Expected: all tests pass (unit tests in `handlers::shortlink`, `http_tests`, `store::tests`, and `tests/public_api.rs` integration tests — 35+ tests, 0 failures, matching or exceeding the pre-change baseline of 38 total).

**Step 2: Run clippy with the project's CI-matching flags**

Run: `cargo clippy -p chart-server -- -D warnings`
Expected: no warnings/errors.

Run: `cargo clippy -p chart-server --all-targets -- -D warnings`
Expected: no warnings/errors (covers the test targets too).

**Step 3: Run the workspace build to confirm nothing else references `compute_id`**

Run: `cargo build --workspace`
Expected: succeeds (no other crate references `chart_server::handlers::shortlink::compute_id` — it was a private `fn`, not `pub`, so this should be a no-op check, but confirms no accidental workspace breakage).

**Step 4: Re-check acceptance criteria against `fulgur-chart-8tr.5`**

Run: `bd show fulgur-chart-8tr.5` and manually confirm each of the 9 acceptance criteria items is satisfied (items 1-8 by code/tests above; item 9 — rate_limit is untouched — by inspection that `crates/chart-server/src/config.rs` was not modified in this plan).

No commit for this task (verification only, unless clippy/build surfaces something to fix — in that case, fix and commit as `fix(chart-server): <description>`).
