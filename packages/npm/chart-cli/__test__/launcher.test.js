'use strict';
const { describe, it } = require('node:test');
const assert = require('node:assert');
const { spawnSync } = require('child_process');
const path = require('path');
const { detectPlatformKey } = require('../bin/fulgur-chart');

const LAUNCHER = path.join(__dirname, '..', 'bin', 'fulgur-chart');

describe('detectPlatformKey', () => {
  it('detects linux x64 glibc', () => {
    assert.strictEqual(detectPlatformKey('linux', 'x64', () => false), 'linux-x64');
  });

  it('detects linux x64 musl', () => {
    assert.strictEqual(detectPlatformKey('linux', 'x64', () => true), 'linux-x64-musl');
  });

  it('detects linux arm64', () => {
    assert.strictEqual(detectPlatformKey('linux', 'arm64', () => false), 'linux-arm64');
  });

  it('detects darwin arm64', () => {
    assert.strictEqual(detectPlatformKey('darwin', 'arm64', () => false), 'darwin-arm64');
  });

  it('detects darwin x64', () => {
    assert.strictEqual(detectPlatformKey('darwin', 'x64', () => false), 'darwin-x64');
  });

  it('detects win32 x64', () => {
    assert.strictEqual(detectPlatformKey('win32', 'x64', () => false), 'win32-x64');
  });

  it('returns null for unsupported platform', () => {
    assert.strictEqual(detectPlatformKey('freebsd', 'x64', () => false), null);
  });
});

describe('launcher errors', () => {
  it('exits 1 when platform package is missing', () => {
    // Copy the launcher to an isolated temp directory so require.resolve
    // cannot find any optional platform package installed in the repo.
    const tmpdir = require('fs').mkdtempSync(require('os').tmpdir() + '/chart-cli-test-');
    const isolatedLauncher = path.join(tmpdir, 'fulgur-chart');
    require('fs').copyFileSync(LAUNCHER, isolatedLauncher);
    require('fs').chmodSync(isolatedLauncher, 0o755);
    const r = spawnSync(process.execPath, [isolatedLauncher, 'render', '-', '-o', '-'], {
      input: '{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}',
      encoding: 'utf8',
    });
    assert.notStrictEqual(r.status, 0, `expected non-zero exit, got ${r.status}`);
    assert.ok(
      /platform package @fulgur-rs\/chart-cli-/.test(r.stderr),
      `expected missing platform package error, got: ${r.stderr}`
    );
  });
});
