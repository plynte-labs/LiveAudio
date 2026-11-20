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
 *     },
 *     "linux-x86_64": {
 *       "signature": "<raw-minisign-signature-contents>",
 *       "url": "https://github.com/<repo>/releases/download/<tag>/<installer>.AppImage"
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
 * Infer Tauri platform key from an installer filename.
 * @param {string} installerPath
 * @returns {"windows-x86_64" | "linux-x86_64"}
 */
function inferPlatformKey(installerPath) {
  const lower = path.basename(installerPath).toLowerCase();
  if (lower.endsWith(".appimage")) {
    return "linux-x86_64";
  }
  if (lower.endsWith(".exe")) {
    return "windows-x86_64";
  }
  throw new Error(`Unsupported installer format for updater manifest: ${installerPath}`);
}

/**
 * Locate signed platform artifacts across one or more candidate bundle directories.
 * Discovers:
 *   - windows-x86_64: *.exe paired with *.exe.sig (or matching *.sig)
 *   - linux-x86_64: *.AppImage paired with *.AppImage.sig
 *
 * @param {string[]} bundleDirs
 * @returns {Record<string, { installerPath: string, sigPath: string }>}
 */
export function find_platform_artifacts(bundleDirs) {
  const searched = [];
  const platforms = {};
  const unsignedWindows = [];
  const unsignedLinux = [];

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

    const files = entries.filter((e) => e.isFile()).map((e) => e.name);
    const sigNames = files.filter((name) => name.endsWith(".sig"));

    const exeNames = files.filter(
      (name) =>
        name.toLowerCase().endsWith(".exe") &&
        !name.toLowerCase().startsWith("uninstall")
    );
    const appImageNames = files.filter((name) =>
      name.toLowerCase().endsWith(".appimage")
    );

    if (!platforms["windows-x86_64"] && exeNames.length > 0) {
      let matched = null;

      for (const sigName of sigNames) {
        if (sigName.toLowerCase().endsWith(".exe.sig")) {
          const exeName = sigName.slice(0, -4);
          if (files.includes(exeName)) {
            matched = {
              installerPath: path.join(bDir, exeName),
              sigPath: path.join(bDir, sigName),
            };
            break;
          }
        }
      }

      if (!matched) {
        for (const sigName of sigNames) {
          if (sigName.toLowerCase().endsWith(".appimage.sig")) {
            continue;
          }
          const exeName = path.parse(sigName).name + ".exe";
          if (files.includes(exeName)) {
            matched = {
              installerPath: path.join(bDir, exeName),
              sigPath: path.join(bDir, sigName),
            };
            break;
          }
        }
      }

      if (!matched) {
        const nonAppImageSigs = sigNames.filter(
          (name) => !name.toLowerCase().endsWith(".appimage.sig")
        );
        if (nonAppImageSigs.length > 0) {
          matched = {
            installerPath: path.join(bDir, exeNames[0]),
            sigPath: path.join(bDir, nonAppImageSigs[0]),
          };
        }
      }

      if (matched) {
        platforms["windows-x86_64"] = matched;
      } else {
        unsignedWindows.push(path.join(bDir, exeNames[0]));
      }
    }

    if (!platforms["linux-x86_64"] && appImageNames.length > 0) {
      let matched = null;

      for (const appImageName of appImageNames) {
        const exactSigName = `${appImageName}.sig`;
        if (files.includes(exactSigName)) {
          matched = {
            installerPath: path.join(bDir, appImageName),
            sigPath: path.join(bDir, exactSigName),
          };
          break;
        }
      }

      if (matched) {
        platforms["linux-x86_64"] = matched;
      } else {
        unsignedLinux.push(path.join(bDir, appImageNames[0]));
      }
    }
  }

  if (!platforms["linux-x86_64"] && unsignedLinux.length > 0) {
    throw new Error(
      `Missing .AppImage.sig signature for Linux AppImage: ${unsignedLinux[0]}\n` +
        "Please ensure TAURI_SIGNING_PRIVATE_KEY is provisioned and valid."
    );
  }

  if (!platforms["windows-x86_64"] && unsignedWindows.length > 0) {
    throw new Error(
      `Missing .sig signature for Windows installer: ${unsignedWindows[0]}\n` +
        "Please ensure TAURI_SIGNING_PRIVATE_KEY is provisioned and valid."
    );
  }

  if (Object.keys(platforms).length === 0) {
    let errMsg =
      "Could not locate signed installer (.exe + .sig or .AppImage + .AppImage.sig) in candidate directories:\n" +
      bundleDirs.map((d) => `  - ${d}`).join("\n");
    if (searched.length > 0) {
      errMsg += `\nFound directories: ${searched.join(", ")}, but no signed updater artifacts were present.`;
      errMsg += "\nPlease ensure TAURI_SIGNING_PRIVATE_KEY is provisioned and valid.";
    }
    throw new Error(errMsg);
  }

  return platforms;
}

/**
 * Locate Windows installer .exe and matching .sig in candidate bundle directories.
 * @param {string[]} bundleDirs
 * @returns {{ installerPath: string, sigPath: string }}
 */
export function find_bundle_artifacts(bundleDirs) {
  const platforms = find_platform_artifacts(bundleDirs);
  if (platforms["windows-x86_64"]) {
    return platforms["windows-x86_64"];
  }
  throw new Error(
    "Could not locate both installer .exe and .sig file in candidate directories:\n" +
      bundleDirs.map((d) => `  - ${d}`).join("\n")
  );
}

/**
 * Build latest.json and stage platform installers, .sig files, and latest.json in outDir.
 * Accepts either a `platforms` map (`{ "windows-x86_64": { installerPath, sigPath }, ... }`)
 * or legacy single-platform `installerPath` + `sigPath`.
 *
 * @param {object} params
 * @param {string} params.version
 * @param {Record<string, { installerPath: string, sigPath: string }>} [params.platforms]
 * @param {string} [params.installerPath]
 * @param {string} [params.sigPath]
 * @param {string} params.outDir
 * @param {string} [params.repo]
 * @param {string} [params.tag]
 * @param {string} [params.notes]
 * @returns {{
 *   manifestPath: string,
 *   manifestData: object,
 *   stagedFiles: Record<string, { destInstaller: string, destSig: string }>,
 *   destInstaller: string,
 *   destSig: string
 * }}
 */
export function assemble_manifest({
  version,
  platforms = null,
  installerPath = null,
  sigPath = null,
  outDir,
  repo = process.env.GITHUB_REPOSITORY || "plynte-labs/LiveAudio",
  tag = "",
  notes = null,
}) {
  const resolvedPlatforms = platforms
    ? { ...platforms }
    : installerPath && sigPath
      ? { [inferPlatformKey(installerPath)]: { installerPath, sigPath } }
      : {};

  const platformEntries = Object.entries(resolvedPlatforms);
  if (platformEntries.length === 0) {
    throw new Error("At least one signed platform artifact is required to assemble latest.json");
  }

  const cleanVersion = version.startsWith("v") ? version.slice(1) : version;
  const tagName = tag ? tag : version.startsWith("v") ? version : `v${version}`;
  const pubDate = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
  const releaseNotes = notes ?? `LiveAudio v${cleanVersion}`;

  const manifestPlatforms = {};
  const stagedFiles = {};

  fs.mkdirSync(outDir, { recursive: true });

  for (const [platformKey, artifact] of platformEntries) {
    if (!artifact?.installerPath || !fs.existsSync(artifact.installerPath)) {
      throw new Error(`Installer file does not exist for ${platformKey}: ${artifact?.installerPath}`);
    }
    if (!artifact?.sigPath || !fs.existsSync(artifact.sigPath)) {
      throw new Error(`Signature file does not exist for ${platformKey}: ${artifact?.sigPath}`);
    }

    const rawSignature = fs.readFileSync(artifact.sigPath, "utf-8").trim();
    if (!rawSignature) {
      throw new Error(`Signature file ${artifact.sigPath} is empty`);
    }

    const installerFilename = path.basename(artifact.installerPath);
    const downloadUrl = `https://github.com/${repo}/releases/download/${tagName}/${installerFilename}`;
    const parsedUrl = new URL(downloadUrl);
    if (parsedUrl.protocol !== "https:") {
      throw new Error(`Updater URL must use HTTPS: ${downloadUrl}`);
    }

    manifestPlatforms[platformKey] = {
      signature: rawSignature,
      url: downloadUrl,
    };

    const destInstaller = path.join(outDir, installerFilename);
    if (path.resolve(destInstaller) !== path.resolve(artifact.installerPath)) {
      fs.copyFileSync(artifact.installerPath, destInstaller);
    }

    const destSig = path.join(outDir, path.basename(artifact.sigPath));
    if (path.resolve(destSig) !== path.resolve(artifact.sigPath)) {
      fs.copyFileSync(artifact.sigPath, destSig);
    }

    stagedFiles[platformKey] = { destInstaller, destSig };

    const installerSize = fs.existsSync(destInstaller)
      ? fs.statSync(destInstaller).size
      : 0;
    console.log(
      `Staged ${platformKey} installer: ${destInstaller} (${installerSize.toLocaleString()} bytes)`
    );
    console.log(`Staged ${platformKey} signature: ${destSig}`);
  }

  const manifestData = {
    version: cleanVersion,
    notes: releaseNotes,
    pub_date: pubDate,
    platforms: manifestPlatforms,
  };

  const manifestPath = path.join(outDir, "latest.json");
  fs.writeFileSync(manifestPath, JSON.stringify(manifestData, null, 2) + "\n", "utf-8");

  console.log(`Assembled updater manifest: ${manifestPath}`);
  console.log(`Manifest contents:\n${JSON.stringify(manifestData, null, 2)}`);

  const firstStaged = Object.values(stagedFiles)[0];
  return {
    manifestPath,
    manifestData,
    stagedFiles,
    destInstaller: firstStaged.destInstaller,
    destSig: firstStaged.destSig,
  };
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
  const candidateBundleDirs =
    values["bundle-dir"] && values["bundle-dir"].length > 0
      ? [...values["bundle-dir"]]
      : [
          path.join(root, "target", "release", "bundle", "nsis"),
          path.join(root, "desktop", "src-tauri", "target", "release", "bundle", "nsis"),
          path.join(root, "target", "release", "bundle", "appimage"),
          path.join(root, "desktop", "src-tauri", "target", "release", "bundle", "appimage"),
          path.join(root, "target", "release", "bundle"),
          path.join(root, "desktop", "src-tauri", "target", "release", "bundle"),
        ];

  let platforms;
  if (values.installer && values.sig) {
    platforms = {
      [inferPlatformKey(values.installer)]: {
        installerPath: values.installer,
        sigPath: values.sig,
      },
    };
  } else {
    platforms = find_platform_artifacts(candidateBundleDirs);
  }

  assemble_manifest({
    version: values.version,
    platforms,
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
