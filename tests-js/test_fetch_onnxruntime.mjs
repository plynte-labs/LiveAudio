// SPDX-License-Identifier: MIT
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import zlib from "node:zlib";

import {
  ORT_VERSION,
  PLATFORMS,
  selectPlatform,
  sha256,
  verifySha256,
  readZipEntries,
  readTgzEntries,
} from "../packaging/fetch_onnxruntime.mjs";

function tarHeader(name, size) {
  const h = Buffer.alloc(512);
  h.write(name, 0);
  h.write(size.toString(8).padStart(11, "0") + "\0", 124);
  h.write("0", 156);
  return h;
}

describe("fetch_onnxruntime pins", () => {
  it("pins ORT 1.24.4 with a 64-hex SHA-256 for every archive", () => {
    assert.equal(ORT_VERSION, "1.24.4");
    for (const [name, p] of Object.entries(PLATFORMS)) {
      assert.ok(p.archives.length > 0, name);
      for (const a of p.archives) {
        assert.match(a.sha256, /^[0-9a-f]{64}$/, `${name}/${a.url}`);
        assert.ok(a.url.startsWith("https://"), a.url);
        assert.ok(a.files.length > 0);
      }
    }
  });

  it("selects platform by key and rejects unknown", () => {
    assert.equal(selectPlatform("windows-x64"), PLATFORMS["windows-x64"]);
    assert.throws(() => selectPlatform("macos-arm64"), /unsupported/i);
  });
});

describe("fetch_onnxruntime hashing", () => {
  it("accepts a matching hash and rejects a mismatch", () => {
    const buf = Buffer.from("hello");
    const good = sha256(buf);
    assert.doesNotThrow(() => verifySha256(buf, good, "x"));
    assert.throws(() => verifySha256(buf, "0".repeat(64), "x"), /SHA-256 mismatch/);
  });
});

describe("fetch_onnxruntime archive readers", () => {
  it("reads a tgz entry by path", () => {
    const body = Buffer.from("so-bytes");
    const pad = Buffer.alloc(512 - body.length);
    const tar = Buffer.concat([tarHeader("pkg/lib/a.so", body.length), body, pad, Buffer.alloc(1024)]);
    const entries = readTgzEntries(zlib.gzipSync(tar));
    assert.equal(entries.get("pkg/lib/a.so").toString(), "so-bytes");
  });

  it("reads a stored and a deflated zip entry", () => {
    const mk = (name, data, method) => ({ name, data, method });
    const files = [mk("a.txt", Buffer.from("stored"), 0), mk("b/c.txt", Buffer.from("deflated deflated deflated"), 8)];
    const parts = [];
    const central = [];
    let off = 0;
    for (const f of files) {
      const comp = f.method === 8 ? zlib.deflateRawSync(f.data) : f.data;
      const n = Buffer.from(f.name);
      const lh = Buffer.alloc(30);
      lh.writeUInt32LE(0x04034b50, 0); lh.writeUInt16LE(f.method, 8);
      lh.writeUInt32LE(comp.length, 18); lh.writeUInt32LE(f.data.length, 22); lh.writeUInt16LE(n.length, 26);
      const ch = Buffer.alloc(46);
      ch.writeUInt32LE(0x02014b50, 0); ch.writeUInt16LE(f.method, 10);
      ch.writeUInt32LE(comp.length, 20); ch.writeUInt32LE(f.data.length, 24); ch.writeUInt16LE(n.length, 28);
      ch.writeUInt32LE(off, 42);
      central.push(Buffer.concat([ch, n]));
      const rec = Buffer.concat([lh, n, comp]);
      parts.push(rec); off += rec.length;
    }
    const cd = Buffer.concat(central);
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054b50, 0); eocd.writeUInt16LE(files.length, 10);
    eocd.writeUInt32LE(cd.length, 12); eocd.writeUInt32LE(off, 16);
    const zip = Buffer.concat([...parts, cd, eocd]);
    const entries = readZipEntries(zip);
    assert.equal(entries.get("a.txt").toString(), "stored");
    assert.equal(entries.get("b/c.txt").toString(), "deflated deflated deflated");
  });
});

describe("app-local VC++ runtime", () => {
  const dlls = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll"];

  it("fetch script declares the four VC++ runtime DLLs", async () => {
    const mod = await import("../packaging/fetch_onnxruntime.mjs");
    assert.deepEqual(mod.VCRUNTIME_DLLS, dlls);
    assert.equal(typeof mod.findVcRedistDir, "function");
  });

  it("findVcRedistDir picks the newest Microsoft.VC14*.CRT x64 dir and errors when absent", async () => {
    const { findVcRedistDir } = await import("../packaging/fetch_onnxruntime.mjs");
    const fs = await import("node:fs");
    const os = await import("node:os");
    const path = await import("node:path");
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "vsroot-"));
    assert.throws(() => findVcRedistDir(root), /Visual C\+\+ Redistributable/);
    for (const v of ["14.38.1", "14.50.2"]) fs.mkdirSync(path.join(root, "VC/Redist/MSVC", v, "x64/Microsoft.VC143.CRT"), { recursive: true });
    assert.ok(findVcRedistDir(root).includes("14.50.2"));
    fs.rmSync(root, { recursive: true });
  });

  it("tauri.windows.conf.json ships them app-locally in the install root", async () => {
    const fs = await import("node:fs");
    const conf = JSON.parse(fs.readFileSync(new URL("../desktop/src-tauri/tauri.windows.conf.json", import.meta.url), "utf8"));
    for (const d of dlls) assert.equal(conf.bundle.resources[`../../packaging/onnxruntime/windows-x64/${d}`], d);
  });

  it("release workflow stages them in the CLI zip", async () => {
    const fs = await import("node:fs");
    const yml = fs.readFileSync(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
    for (const d of dlls) assert.ok(yml.includes(d), d);
  });
});
