#!/usr/bin/env node
// SPDX-License-Identifier: MIT
/**
 * Download ONNX Runtime (and DirectML on Windows) from official sources into
 * packaging/onnxruntime/<platform>/. Every archive is pinned by SHA-256;
 * a mismatch aborts. Usage: node packaging/fetch_onnxruntime.mjs [windows-x64|linux-x64]
 * Dependency-free: archives are parsed in-process (no tar/unzip needed).
 */
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

export const ORT_VERSION = "1.24.4";
const NUGET = "https://api.nuget.org/v3-flatcontainer";
const ORT_NOTICE_FILES = [
  { from: "LICENSE", to: "LICENSE" },
  { from: "ThirdPartyNotices.txt", to: "ThirdPartyNotices.txt" },
];

export const PLATFORMS = {
  "windows-x64": {
    archives: [
      {
        url: `${NUGET}/microsoft.ml.onnxruntime.directml/${ORT_VERSION}/microsoft.ml.onnxruntime.directml.${ORT_VERSION}.nupkg`,
        sha256: "57e9f11b73437bef7a309496135d4c1f96b1a8e9ddba60013fa27bfc1d788681",
        kind: "zip",
        files: [
          { from: "runtimes/win-x64/native/onnxruntime.dll", to: "onnxruntime.dll" },
          { from: "runtimes/win-x64/native/onnxruntime_providers_shared.dll", to: "onnxruntime_providers_shared.dll" },
          ...ORT_NOTICE_FILES,
        ],
      },
      {
        url: `${NUGET}/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg`,
        sha256: "4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9",
        kind: "zip",
        files: [
          { from: "bin/x64-win/DirectML.dll", to: "DirectML.dll" },
          { from: "LICENSE.txt", to: "DirectML-LICENSE.txt" },
        ],
      },
    ],
  },
  "linux-x64": {
    archives: [
      {
        url: `https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz`,
        sha256: "3a211fbea252c1e66290658f1b735b772056149f28321e71c308942cdb54b747",
        kind: "tgz",
        files: [
          { from: `onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION}`, to: `libonnxruntime.so.${ORT_VERSION}` },
          { from: `onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime_providers_shared.so`, to: "libonnxruntime_providers_shared.so" },
          { from: `onnxruntime-linux-x64-${ORT_VERSION}/LICENSE`, to: "LICENSE" },
          { from: `onnxruntime-linux-x64-${ORT_VERSION}/ThirdPartyNotices.txt`, to: "ThirdPartyNotices.txt" },
        ],
      },
    ],
  },
};

export function selectPlatform(key) {
  const p = PLATFORMS[key];
  if (!p) throw new Error(`unsupported platform "${key}" (expected one of ${Object.keys(PLATFORMS).join(", ")})`);
  return p;
}

export const sha256 = (buf) => crypto.createHash("sha256").update(buf).digest("hex");

export function verifySha256(buf, expected, label) {
  const actual = sha256(buf);
  if (actual !== expected) throw new Error(`SHA-256 mismatch for ${label}: expected ${expected}, got ${actual}`);
}

/** @returns {Map<string, Buffer>} entry path -> bytes (regular files only). */
export function readZipEntries(buf) {
  const entries = new Map();
  let eocd = buf.length - 22;
  while (eocd >= 0 && buf.readUInt32LE(eocd) !== 0x06054b50) eocd--;
  if (eocd < 0) throw new Error("zip: end of central directory not found");
  const count = buf.readUInt16LE(eocd + 10);
  let p = buf.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    if (buf.readUInt32LE(p) !== 0x02014b50) throw new Error("zip: bad central directory entry");
    const method = buf.readUInt16LE(p + 10);
    const csize = buf.readUInt32LE(p + 20);
    const nlen = buf.readUInt16LE(p + 28);
    const elen = buf.readUInt16LE(p + 30);
    const clen = buf.readUInt16LE(p + 32);
    const local = buf.readUInt32LE(p + 42);
    const name = buf.toString("utf8", p + 46, p + 46 + nlen);
    p += 46 + nlen + elen + clen;
    if (name.endsWith("/")) continue;
    const start = local + 30 + buf.readUInt16LE(local + 26) + buf.readUInt16LE(local + 28);
    const raw = buf.subarray(start, start + csize);
    if (method === 0) entries.set(name, Buffer.from(raw));
    else if (method === 8) entries.set(name, zlib.inflateRawSync(raw));
    else throw new Error(`zip: unsupported compression method ${method} for ${name}`);
  }
  return entries;
}

/** @returns {Map<string, Buffer>} entry path -> bytes (regular files only; symlinks skipped). */
export function readTgzEntries(gz) {
  const tar = zlib.gunzipSync(gz);
  const entries = new Map();
  let p = 0;
  while (p + 512 <= tar.length && tar[p] !== 0) {
    const name = tar.toString("utf8", p, p + 100).replace(/\0.*$/s, "");
    const size = parseInt(tar.toString("ascii", p + 124, p + 135).replace(/\0.*$/s, "").trim() || "0", 8);
    const type = String.fromCharCode(tar[p + 156] || 48);
    p += 512;
    if (type === "0") entries.set(name.replace(/^\.\//, ""), Buffer.from(tar.subarray(p, p + size)));
    p += Math.ceil(size / 512) * 512;
  }
  return entries;
}

/** VC++ runtime DLLs that prebuilt ORT and the Rust sidecars link dynamically. */
export const VCRUNTIME_DLLS = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll"];

/** Newest `VC/Redist/MSVC/<ver>/x64/Microsoft.VC14*.CRT` under a Visual Studio install root. */
export function findVcRedistDir(vsRoot) {
  const base = path.join(vsRoot, "VC", "Redist", "MSVC");
  const versions = fs.existsSync(base) ? fs.readdirSync(base).sort((a, b) => a.localeCompare(b, undefined, { numeric: true })) : [];
  for (const v of versions.reverse()) {
    const x64 = path.join(base, v, "x64");
    const crt = fs.existsSync(x64) && fs.readdirSync(x64).find((d) => /^Microsoft\.VC14\d\.CRT$/.test(d));
    if (crt) return path.join(x64, crt);
  }
  throw new Error(`Microsoft Visual C++ Redistributable (VC/Redist/MSVC/*/x64/Microsoft.VC14*.CRT) not found under ${vsRoot}`);
}

function stageVcRuntime(dir) {
  const vswhere = path.join(process.env["ProgramFiles(x86)"] || "C:\Program Files (x86)", "Microsoft Visual Studio", "Installer", "vswhere.exe");
  if (!fs.existsSync(vswhere)) throw new Error(`vswhere.exe not found at ${vswhere}; install Visual Studio Build Tools`);
  const vsRoot = execFileSync(vswhere, ["-latest", "-products", "*", "-property", "installationPath"], { encoding: "utf8" }).trim();
  if (!vsRoot) throw new Error("vswhere found no Visual Studio installation");
  const crt = findVcRedistDir(vsRoot);
  for (const f of VCRUNTIME_DLLS) fs.copyFileSync(path.join(crt, f), path.join(dir, f));
  const missing = VCRUNTIME_DLLS.filter((f) => !fs.existsSync(path.join(dir, f)));
  if (missing.length) throw new Error(`VC++ runtime copy incomplete: ${missing.join(", ")}`);
  console.log(`[ort] windows-x64: VC++ runtime staged from ${crt}`);
}

async function download(url) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`download failed: ${url} -> HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

export async function fetchPlatform(key, root) {
  const plat = selectPlatform(key);
  const dir = path.join(root, key);
  const stampFile = path.join(dir, ".pins");
  const stamp = plat.archives.map((a) => a.sha256).join("\n");
  const outputs = [...plat.archives.flatMap((a) => a.files.map((f) => f.to)), "VERSION_NUMBER"];
  if (key === "windows-x64") {
    fs.mkdirSync(dir, { recursive: true });
    stageVcRuntime(dir);
  }
  const present = outputs.every((f) => fs.existsSync(path.join(dir, f)));
  if (present && fs.existsSync(stampFile) && fs.readFileSync(stampFile, "utf8") === stamp) {
    console.log(`[ort] ${key}: up to date (${ORT_VERSION})`);
    return;
  }
  fs.mkdirSync(dir, { recursive: true });
  for (const a of plat.archives) {
    console.log(`[ort] ${key}: downloading ${a.url}`);
    const buf = await download(a.url);
    verifySha256(buf, a.sha256, a.url);
    const entries = a.kind === "zip" ? readZipEntries(buf) : readTgzEntries(buf);
    for (const f of a.files) {
      const data = entries.get(f.from);
      if (!data) throw new Error(`${a.url}: missing entry ${f.from}`);
      fs.writeFileSync(path.join(dir, f.to), data);
    }
  }
  fs.writeFileSync(path.join(dir, "VERSION_NUMBER"), `${ORT_VERSION}\n`);
  fs.writeFileSync(stampFile, stamp);
  console.log(`[ort] ${key}: ready (${ORT_VERSION})`);
}

async function main(argv) {
  const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "onnxruntime");
  const keys = argv.length ? argv : [process.platform === "win32" ? "windows-x64" : "linux-x64"];
  for (const k of keys) await fetchPlatform(k, root);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((e) => {
    console.error(`[ort] ERROR: ${e.message}`);
    process.exit(1);
  });
}
