# Vega-Lite strict モード encoding allow-list mark 別化 実装プラン

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `check_unknown_keys`(strict モード)の encoding allow-list を mark 別にし、`VlXxxEncoding`(deny_unknown_fields)の schema 制約と runtime strict を一致させる。

**Architecture:** `check_unknown_keys` の中で top.mark を軽く peek し、mark 名に応じた encoding channel allow-list を選ぶ。string / object 両形の mark に対応する小さなヘルパ `read_mark_name` を追加。非 strict 挙動は変更しない。

**Tech Stack:** Rust, serde_json::Value, cargo test。

---

## Baseline

- Worktree: `/home/ubuntu/fulgur-chart/.worktrees/vl-strict-mark-encoding`
- Branch: `feat/vl-strict-mark-encoding`(base main @ 9d0935b)
- 事前確認: `cargo test -p fulgur-chart --test frontend_vegalite` = 22 pass

## Task 1: mark 別 allow-list 対応(TDD)

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`(:483-530 `check_unknown_keys` / `check_object`)
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`(末尾に追加)

### Step 1: 失敗テストを追加

`crates/fulgur-chart/tests/frontend_vegalite.rs` 末尾に以下を追加。既存 `strict_circle_rejects_shape_encoding` (line 269) パターンをミラー。

```rust
#[test]
fn strict_bar_rejects_theta_encoding() {
    let json = r#"{
        "mark": "bar",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    // strict では VlBarEncoding が受理しない theta を拒否する。
    assert!(vegalite::parse(json, true).is_err());
    // 非 strict では現状通り黙って許容(挙動維持)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_line_rejects_theta_encoding() {
    let json = r#"{
        "mark": "line",
        "data": {"values": [{"cat":"A","val":3}]},
        "encoding": {"x": {"field":"cat"}, "y": {"field":"val"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_point_rejects_theta_encoding() {
    let json = r#"{
        "mark": "point",
        "data": {"values": [{"x":1,"y":2}]},
        "encoding": {"x": {"field":"x","type":"quantitative"}, "y": {"field":"y","type":"quantitative"}, "theta": {"field":"c"}}
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_arc_accepts_x_encoding() {
    // arc の allow-list は [theta, color, x, y] を含むので strict でも OK。
    let json = r#"{
        "mark": "arc",
        "data": {"values": [{"cat":"A","val":3},{"cat":"B","val":5}]},
        "encoding": {"theta": {"field":"val"}, "color": {"field":"cat"}, "x": {"field":"cat"}}
    }"#;
    assert!(vegalite::parse(json, true).is_ok());
}
```

### Step 2: 失敗を確認

```bash
cargo test -p fulgur-chart --test frontend_vegalite strict_ 2>&1 | tail -15
```

Expected:
- `strict_bar_rejects_theta_encoding` → fail(現状 theta は全 mark 共通 allow-list で通ってしまう)
- `strict_line_rejects_theta_encoding` → fail
- `strict_point_rejects_theta_encoding` → fail
- `strict_arc_accepts_x_encoding` → 現状すでに pass(既存 allow-list に x/y/color/theta 全部入っている)

### Step 3: `check_unknown_keys` を mark 別化

`crates/fulgur-chart/src/frontend/vegalite.rs:486-515` の `check_unknown_keys` を以下で置き換える(既存 top-level check はそのまま流用):

```rust
/// strict 用: 既知キーのホワイトリストに照らし、最初の未知キーをパス付き Err で返す。
/// 防御的に走査し、ノードが欠落/想定外の形なら Ok を返す（後段の通常パースに委ねる）。
fn check_unknown_keys(json: &str) -> Result<(), String> {
    let value: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()), // 不正 JSON は後段パースに委ねる
    };
    let Some(top) = value.as_object() else {
        return Ok(()); // object でなければ後段パースに委ねる
    };

    check_object(
        top,
        &[
            "mark", "data", "encoding", "$schema", "width", "height", "title",
        ],
        "",
    )?;

    if let Some(encoding) = top.get("encoding").and_then(Value::as_object) {
        // mark 別 encoding allow-list を選ぶ。mark 名が読めない/未対応なら
        // 現状挙動(全キー拒否せずスルー)を保つ = 後段パースに委ねる。
        let allowed: &[&str] = match read_mark_name(top).as_deref() {
            Some("bar") | Some("line") | Some("point") | Some("circle") => &["x", "y", "color"],
            Some("arc") => &["theta", "color", "x", "y"],
            _ => return Ok(()),
        };
        check_object(encoding, allowed, "encoding")?;
        for channel in allowed {
            if let Some(ch) = encoding.get(*channel).and_then(Value::as_object) {
                // aggregate は未実装(本体は単純合計しかしない)。strict では
                // 誤った集計結果を黙って返さないよう、未対応キーとして拒否する。
                check_object(ch, &["field", "type"], &format!("encoding.{channel}"))?;
            }
        }
    }

    Ok(())
}

/// top.mark の名前を string / object 両形で取り出す。取れなければ None。
fn read_mark_name(top: &Map<String, Value>) -> Option<String> {
    match top.get("mark")? {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("type").and_then(Value::as_str).map(str::to_owned),
        _ => None,
    }
}
```

### Step 4: pass を確認

```bash
cargo test -p fulgur-chart --test frontend_vegalite strict_ 2>&1 | tail -15
```

Expected: 4 新規テスト + 既存 strict_* テスト全て pass。

### Step 5: 全 vegalite テスト回帰なし

```bash
cargo test -p fulgur-chart --test frontend_vegalite 2>&1 | tail -3
```

Expected: `test result: ok. 26 passed; 0 failed`(既存 22 + 新規 4)

### Step 6: 全 fulgur-chart スイート回帰なし

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "^test result" | awk 'BEGIN {ok=0; fail=0} {ok+=$4; fail+=$6} END {print "PASS:", ok, "FAIL:", fail}'
```

Expected: `PASS: 641 FAIL: 0`(前ベース 639 + 新規 4 に若干差分)

### Step 7: commit

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
fix(vegalite): apply mark-specific encoding allow-list in strict mode

check_unknown_keys used a global encoding allow-list ["x","y","color","theta"]
regardless of mark, so `--strict` accepted specs that would fail Vl*Encoding
schema validation (e.g. `mark: "bar"` with `encoding.theta`), then silently
ignored the ignored channels at parse time.

Peek at top.mark and pick the allow-list to match each Vl*Encoding:
  - bar / line / point / circle: [x, y, color]
  - arc: [theta, color, x, y]
Unknown / missing mark falls through as before (defer to the normal parse
for a mark-name error).

refs: fulgur-chart-cw3
EOF
)"
```

## Task 2: 最終検証

### Step 1: 全テスト

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "^test result" | awk 'BEGIN {ok=0; fail=0} {ok+=$4; fail+=$6} END {print "PASS:", ok, "FAIL:", fail}'
```

### Step 2: clippy

```bash
cargo clippy -p fulgur-chart --tests -- -D warnings 2>&1 | tail -5
```

Expected: warnings 0。

### Step 3: fmt

```bash
cargo fmt -p fulgur-chart -- --check
```

Expected: 差分なし。

---

## YAGNI(この plan では扱わない)

- エラーメッセージのメッセージング改善(e.g. mark に応じたヒント)
- 各 channel 内の未対応キー(scale/axis/legend/aggregate 等)拒否 → 既存 per-channel check で対応済み
- 新規 mark(area/rect 等)対応

## 受け入れ基準(from bd issue)

- [x] bar/line/point/circle with `encoding.theta` → strict で Err
- [x] arc with `encoding.x`(または y/theta/color)→ strict で OK
- [x] non-strict モード挙動は変更なし
- [x] 既存 vegalite テスト・スナップショット全通過
