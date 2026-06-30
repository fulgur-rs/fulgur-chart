# WASM Browser Smoke Test Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a headless-Chromium smoke test that exercises `crates/bindings/wasm`'s published npm package (`index.js` + `pkg/`) through its real browser load path — `fetch()`-based `init()` — which today is never executed in CI (Node tests bypass `fetch()` by passing wasm bytes directly).

**Architecture:** A small `node:http` static file server serves the package directory with correct `Content-Type` headers (notably `application/wasm`). Playwright launches headless Chromium, navigates to that server, and inside the page dynamically imports `/index.js`, calls `init()` with no arguments (the browser `fetch()` path), then renders SVG and PNG and returns the results to the Node test process for assertion. The new test file is named so it is excluded from `node --test`'s default discovery (verified empirically — see Task 2), and is run only via an explicit `test:browser` npm script, which CI invokes as a new step in the existing `wasm-binding` job after `npm test`.

**Tech Stack:** Node.js built-in test runner (`node:test`), `node:http`, Playwright (Chromium), existing `wasm-pack --target web` build output.

**Beads issue:** `fulgur-chart-de9` (design and acceptance criteria already recorded there — this plan operationalizes that design).

---

### Task 1: Add Playwright as a devDependency

**Files:**
- Modify: `crates/bindings/wasm/package.json`
- Modify (generated): `crates/bindings/wasm/package-lock.json`

**Step 1: Install playwright**

```bash
cd crates/bindings/wasm
npm install --save-dev playwright
```

**Step 2: Verify**

```bash
grep -A2 '"devDependencies"' package.json
```
Expected: `playwright` listed alongside the existing `@types/node` and `typescript` entries.

**Step 3: Commit**

```bash
git add crates/bindings/wasm/package.json crates/bindings/wasm/package-lock.json
git commit -m "chore(wasm-binding): add playwright devDependency for browser smoke test"
```

---

### Task 2: Write the static file server + browser smoke test

**Files:**
- Create: `crates/bindings/wasm/__test__/browser-smoke.mjs`

**Why this filename:** `node --test` (which `npm test` runs) auto-discovers files by *filename* pattern (`*.test.{js,mjs,cjs}`, `*-test.*`, `*_test.*`, `test-*.*`), not by directory. A file named `browser-smoke.test.mjs` would be auto-discovered by `npm test` even outside `__test__/` (confirmed empirically during design). Naming it `browser-smoke.mjs` (no `.test.` infix) keeps it out of `npm test`'s default run while still living alongside the other tests in `__test__/`. It is run explicitly via `node --test __test__/browser-smoke.mjs`.

**Step 1: Write the file with one intentionally-wrong assertion**

This proves the harness actually drives a real assertion failure (not a vacuously-passing smoke test) before we trust a later green run.

```javascript
// crates/bindings/wasm/__test__/browser-smoke.mjs
//
// Smoke-tests the published npm package in a real headless browser, exercising the
// fetch()-based init() path that __test__/*.test.mjs cannot reach (Node has no `fetch`
// loader for local files, so those tests pass wasm bytes directly via the object form).
// Named without `.test.` so `node --test` (npm test's default discovery) does not pick
// it up; run explicitly via `npm run test:browser`.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { chromium } from 'playwright'

const PKG_ROOT = fileURLToPath(new URL('..', import.meta.url))

const CONTENT_TYPES = {
  '.js': 'text/javascript',
  '.mjs': 'text/javascript',
  '.wasm': 'application/wasm',
}

// Minimal static file server: only ever needs to serve index.js and pkg/*, so
// node:http + node:fs covers it without a new npm dependency.
function startServer() {
  const server = createServer(async (req, res) => {
    const reqPath = decodeURIComponent(new URL(req.url, 'http://localhost').pathname)
    const filePath = path.join(PKG_ROOT, reqPath === '/' ? 'index.js' : reqPath)
    if (!filePath.startsWith(PKG_ROOT)) {
      res.writeHead(403)
      res.end()
      return
    }
    try {
      const body = await readFile(filePath)
      const ext = path.extname(filePath)
      res.writeHead(200, { 'Content-Type': CONTENT_TYPES[ext] ?? 'application/octet-stream' })
      res.end(body)
    } catch {
      res.writeHead(404)
      res.end()
    }
  })
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => resolve(server))
  })
}

const BAR = '{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}'

test('browser (chromium): fetch()-based init() + render() smoke', async () => {
  const server = await startServer()
  const { port } = server.address()
  const browser = await chromium.launch()
  try {
    const page = await browser.newPage()
    await page.goto(`http://127.0.0.1:${port}/`)
    const result = await page.evaluate(async (spec) => {
      const mod = await import('/index.js')
      await mod.default() // init(): no args -> browser fetch() path
      const svg = mod.build(spec).render('svg')
      const png = mod.render(spec, 'png')
      return {
        version: mod.version(),
        svgPrefix: svg.slice(0, 5),
        pngMagic: Array.from(png.subarray(0, 4)),
      }
    }, BAR)

    assert.match(result.version, /^\d+\.\d+\.\d+/)
    assert.equal(result.svgPrefix, 'WRONG') // intentionally wrong; fixed in Step 3
    assert.deepEqual(result.pngMagic, [0x89, 0x50, 0x4e, 0x47])
  } finally {
    await browser.close()
    await new Promise((resolve) => server.close(resolve))
  }
})
```

**Step 2: Install the Chromium browser binary Playwright needs**

```bash
npx playwright install --with-deps chromium
```
Expected: downloads Chromium (and OS deps); exits 0. This is a one-time local setup cost (CI does this fresh every run — see Task 4).

**Step 3: Run the test and confirm it fails at the intentional assertion**

```bash
node --test __test__/browser-smoke.mjs
```
Expected: FAIL, with the assertion error showing `actual: '<svg '` vs `expected: 'WRONG'` — this confirms `init()`, the real `fetch()` load, and `render()` all executed successfully in the browser, and only the deliberately-wrong expectation failed.

**Step 4: Fix the intentional wrong assertion**

```diff
-    assert.equal(result.svgPrefix, 'WRONG') // intentionally wrong; fixed in Step 3
+    assert.equal(result.svgPrefix, '<svg ')
```

**Step 5: Run again, confirm it passes**

```bash
node --test __test__/browser-smoke.mjs
```
Expected: PASS, 1 test, 0 failures.

**Step 6: Confirm `npm test` is unaffected (file not auto-discovered)**

```bash
npm test 2>&1 | tail -8
```
Expected: `tests 36`, `pass 36`, `fail 0` — same count as the worktree baseline (Task before Task 1), proving `browser-smoke.mjs` was not swept into the default run.

**Step 7: Commit**

```bash
git add crates/bindings/wasm/__test__/browser-smoke.mjs
git commit -m "test(wasm-binding): add headless-Chromium smoke test for fetch()-based init()"
```

---

### Task 3: Add the `test:browser` npm script

**Files:**
- Modify: `crates/bindings/wasm/package.json`

**Step 1: Add the script**

In the `"scripts"` block, alongside the existing `"test": "node --test"`:

```diff
   "scripts": {
     "build": "wasm-pack build --target web --release --locked",
     "test": "node --test",
+    "test:browser": "node --test __test__/browser-smoke.mjs",
     "typecheck": "tsc --noEmit -p tsconfig.json",
     "prepack": "npm run build && rm -f pkg/.gitignore"
   },
```

**Step 2: Verify the script runs**

```bash
npm run test:browser
```
Expected: PASS, 1 test, 0 failures (same as Task 2 Step 5, now via the script).

**Step 3: Commit**

```bash
git add crates/bindings/wasm/package.json
git commit -m "chore(wasm-binding): add test:browser npm script"
```

---

### Task 4: Wire the browser smoke test into CI

**Files:**
- Modify: `.github/workflows/ci.yml` (the `wasm-binding` job, after its existing `Test` step — `npm test`, around line 426-427)

**Step 1: Add the two new steps**

Insert immediately after the existing:
```yaml
      - name: Test
        run: npm test
```
the following:
```yaml

      - name: Install Playwright Chromium
        run: npx playwright install --with-deps chromium

      - name: Browser smoke test (Chromium headless)
        run: npm run test:browser
```
(Both run with the job's existing `working-directory: crates/bindings/wasm` default, same as the other steps in this job.)

**Step 2: Validate YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('OK')"
```
Expected: `OK`.

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(wasm-binding): run headless-Chromium smoke test after npm test"
```

---

### Task 5: Final verification

**Step 1: Run the full existing wasm-binding check sequence locally, mirroring CI**

```bash
cd crates/bindings/wasm
cargo fmt --manifest-path Cargo.toml -- --check
cargo clippy --manifest-path Cargo.toml --target wasm32-unknown-unknown --all-targets --locked -- -D warnings
npm run build
npm run typecheck
npm test
npm run test:browser
```
Expected: every command exits 0; `npm test` reports `pass 36, fail 0`; `npm run test:browser` reports `pass 1, fail 0`.

**Step 2: Confirm no unrelated files changed**

```bash
git status --porcelain
git log --oneline main..HEAD
```
Expected: working tree clean (everything committed across Tasks 1-4), and the commit log shows exactly the 4 commits from this plan.

This task has no separate commit — it's a verification gate before handing off to code review / PR.
