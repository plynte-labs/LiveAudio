#!/usr/bin/env node
// SPDX-License-Identifier: MIT
/**
 * Assemble latest.json updater manifest and stage release assets for Tauri 2.
 * Pure Node 22 ESM implementation.
 *
 * Manifest schema (Tauri 2):
 * {
 *   "version": "1.6.0",
 *   "notes": "LiveAudio v1.6.0",
 *   "pub_date": "2026-10-29T18:00:00Z",
 *   "platforms": {
 *     "windows-x86_64": {
 *       "signature": "<raw-minisign-signature-contents>",
 *       "url": "https://github.com/<repo>/releases/download/<tag>/<installer>.exe"
 *     }
 *   }
 * }
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

/**
 * Locate installer .exe and matching .sig in candidate bundle directories.
 * @param {string[]} bundleDirs
 * @returns {{ installerPath: string, sigPath: string }}
 */
export function find_bundle_artifacts(bundleDirs) {
  const searched = [];

  for (const bDir of bundleDirs) {
    if (!fs.existsSync(bDir)) {
      continue;
    }
    searched.push(bDir);

    let entries = [];
    try {
      entries = fs.readdirSync(bDir, { withFileTypes: true });
    } catch {
      continue;
    }

    const sigFiles = entries
      .filter((e) => e.isFile() && e.name.endsWith(".sig"))
      .map((e) => path.join(bDir, e.name));

    for (const sig of sigFiles) {
      const sigName = path.basename(sig);

      // Pattern 1: <name>.exe.sig -> <name>.exe
      if (sigName.endsWith(".exe.sig")) {
        const exeName = sigName.slice(0, -4);
        const exePath = path.join(bDir, exeName);
        if (fs.existsSync(exePath)) {
          return { installerPath: exePath, sigPath: sig };
        }
      }

      // Pattern 2: <name>.sig where <name>.exe exists
      const exeName = path.parse(sig).name + ".exe";
      const exePath = path.join(bDir, exeName);
      if (fs.existsSync(exePath)) {
        return { installerPath: exePath, sigPath: sig };
      }
    }

    // Fallback: any .sig and any non-uninstall .exe in the directory
    const exeFiles = entries
      .filter(
        (e) =>
          e.isFile() &&
          e.name.toLowerCase().endsWith(".exe") &&
          !e.name.toLowerCase().startsWith("uninstall")
      )
      .map((e) => path.join(bDir, e.name));

    if (sigFiles.length > 0 && exeFiles.length > 0) {
      return { installerPath: exeFiles[0], sigPath: sigFiles[0] };
    }
  }

  let errMsg =
    "Could not locate both installer .exe and .sig file in candidate directories:\n" +
    bundleDirs.map((d) => `  - ${d}`).join("\n");
  if (searched.length > 0) {
    errMsg += `\nFound directories: ${searched.join(", ")}, but .sig signature was missing.`;
    errMsg += "\nPlease ensure TAURI_SIGNING_PRIVATE_KEY is provisioned and valid.";
  }
  throw new Error(errMsg);
}

/**
 * Build latest.json and stage installer, .sig, and latest.json in outDir.
 * @param {object} params
 * @param {string} params.version
 * @param {string} params.installerPath
 * @param {string} params.sigPath
 * @param {string} params.outDir
 * @param {string} [params.repo]
 * @param {string} [params.tag]
 * @param {string} [params.notes]
 * @returns {{ manifestPath: string, manifestData: object, destInstaller: string, destSig: string }}
 */
export function assemble_manifest({
  version,
  installerPath,
  sigPath,
  outDir,
  repo = process.env.GITHUB_REPOSITORY || "plynte-labs/LiveAudio",
  tag = "",
  notes = null,
}) {
  if (!fs.existsSync(sigPath)) {
    throw new Error(`Signature file does not exist: ${sigPath}`);
  }

  const rawSignature = fs.readFileSync(sigPath, "utf-8").trim();
  if (!rawSignature) {
    throw new Error(`Signature file ${sigPath} is empty`);
  }

  const cleanVersion = version.startsWith("v") ? version.slice(1) : version;
  const tagName = tag ? tag : (version.startsWith("v") ? version : `v${version}`);
  const installerFilename = path.basename(installerPath);
  const downloadUrl = `https://github.com/${repo}/releases/download/${tagName}/${installerFilename}`;

  const pubDate = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
  const releaseNotes = notes ?? `LiveAudio v${cleanVersion}`;

  const manifestData = {
    version: cleanVersion,
    notes: releaseNotes,
    pub_date: pubDate,
    platforms: {
      "windows-x86_64": {
        signature: rawSignature,
        url: downloadUrl,
      },
    },
  };

  fs.mkdirSync(outDir, { recursive: true });

  const destInstaller = path.join(outDir, installerFilename);
  if (path.resolve(destInstaller) !== path.resolve(installerPath)) {
    fs.copyFileSync(installerPath, destInstaller);
  }

  const destSig = path.join(outDir, path.basename(sigPath));
  if (path.resolve(destSig) !== path.resolve(sigPath)) {
    fs.copyFileSync(sigPath, destSig);
  }

  const manifestPath = path.join(outDir, "latest.json");
  fs.writeFileSync(manifestPath, JSON.stringify(manifestData, null, 2) + "\n", "utf-8");

  const installerSize = fs.existsSync(destInstaller)
    ? fs.statSync(destInstaller).size
    : 0;

  console.log(`Staged installer: ${destInstaller} (${installerSize.toLocaleString()} bytes)`);
  console.log(`Staged signature: ${destSig}`);
  console.log(`Assembled updater manifest: ${manifestPath}`);
  console.log(`Manifest contents:\n${JSON.stringify(manifestData, null, 2)}`);

  return { manifestPath, manifestData, destInstaller, destSig };
}

/**
 * Main entry point for CLI usage.
 * @param {string[]} [argv]
 * @returns {number}
 */
export function main(argv = process.argv.slice(2)) {
  const options = {
    version: { type: "string" },
    "bundle-dir": { type: "string", multiple: true },
    installer: { type: "string" },
    sig: { type: "string" },
    "out-dir": { type: "string", default: "release_assets" },
    repo: {
      type: "string",
      default: process.env.GITHUB_REPOSITORY || "plynte-labs/LiveAudio",
    },
    tag: {
      type: "string",
      default: process.env.GITHUB_REF_NAME || "",
    },
    notes: { type: "string" },
  };

  const { values } = parseArgs({ args: argv, options, allowPositionals: true });

  if (!values.version) {
    console.error("Error: --version is required");
    return 1;
  }

  const root = process.cwd();
  const candidateBundleDirs = values["bundle-dir"] ? [...values["bundle-dir"]] : [];
  candidateBundleDirs.push(
    path.join(root, "target", "release", "bundle", "nsis"),
    path.join(root, "desktop", "src-tauri", "target", "release", "bundle", "nsis"),
    path.join(root, "target", "release", "bundle"),
    path.join(root, "desktop", "src-tauri", "target", "release", "bundle")
  );

  let installer = values.installer;
  let sig = values.sig;

  if (installer && sig) {
    // Both explicitly provided
  } else {
    const found = find_bundle_artifacts(candidateBundleDirs);
    installer = found.installerPath;
    sig = found.sigPath;
  }

  assemble_manifest({
    version: values.version,
    installerPath: installer,
    sigPath: sig,
    outDir: values["out-dir"],
    repo: values.repo,
    tag: values.tag,
    notes: values.notes,
  });

  return 0;
}

// Auto-run if executed directly
const isDirectExecution =
  process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isDirectExecution) {
  try {
    const exitCode = main();
    if (exitCode !== 0) {
      process.exit(exitCode);
    }
  } catch (err) {
    console.error(err.message || err);
    process.exit(1);
  }
}
