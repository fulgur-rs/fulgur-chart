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
    assert.equal(result.svgPrefix, '<svg ')
    assert.deepEqual(result.pngMagic, [0x89, 0x50, 0x4e, 0x47])
  } finally {
    await browser.close()
    await new Promise((resolve) => server.close(resolve))
  }
})
