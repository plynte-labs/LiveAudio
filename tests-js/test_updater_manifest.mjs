// SPDX-License-Identifier: MIT
import { describe, it, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

import {
  find_bundle_artifacts,
  find_platform_artifacts,
  assemble_manifest,
  main,
} from "../packaging/assemble_latest_manifest.mjs";

describe("Tauri 2 Updater Manifest Assembler", () => {
  let tmpDir;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "liveaudio-manifest-test-"));
  });

  afterEach(() => {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  });

  describe("find_bundle_artifacts", () => {
    it("finds pair matching pattern: <name>.exe.sig and <name>.exe", () => {
      const bundleDir = path.join(tmpDir, "bundle-nsis");
      fs.mkdirSync(bundleDir, { recursive: true });

      const exePath = path.join(bundleDir, "LiveAudio-Setup-1.6.0.exe");
      const sigPath = path.join(bundleDir, "LiveAudio-Setup-1.6.0.exe.sig");

      fs.writeFileSync(exePath, "dummy-binary-payload");
      fs.writeFileSync(sigPath, "untrusted comment: signature\nRWFakeSignatureBase64String==");

      const found = find_bundle_artifacts([bundleDir]);
      assert.equal(path.resolve(found.installerPath), path.resolve(exePath));
      assert.equal(path.resolve(found.sigPath), path.resolve(sigPath));
    });

    it("finds pair matching pattern: <name>.sig and <name>.exe", () => {
      const bundleDir = path.join(tmpDir, "bundle");
      fs.mkdirSync(bundleDir, { recursive: true });

      const exePath = path.join(bundleDir, "LiveAudio_1.6.0_x64-setup.exe");
      const sigPath = path.join(bundleDir, "LiveAudio_1.6.0_x64-setup.sig");

      fs.writeFileSync(exePath, "dummy-binary-payload");
      fs.writeFileSync(sigPath, "RWFakeSignatureBase64String==");

      const found = find_bundle_artifacts([bundleDir]);
      assert.equal(path.resolve(found.installerPath), path.resolve(exePath));
      assert.equal(path.resolve(found.sigPath), path.resolve(sigPath));
    });

    it("ignores uninstaller executables in fallback search", () => {
      const bundleDir = path.join(tmpDir, "bundle");
      fs.mkdirSync(bundleDir, { recursive: true });

      const uninstallExe = path.join(bundleDir, "uninstall.exe");
      const appExe = path.join(bundleDir, "LiveAudio.exe");
      const sigPath = path.join(bundleDir, "custom.sig");

      fs.writeFileSync(uninstallExe, "uninstaller");
      fs.writeFileSync(appExe, "app");
      fs.writeFileSync(sigPath, "signature");

      const found = find_bundle_artifacts([bundleDir]);
      assert.equal(path.resolve(found.installerPath), path.resolve(appExe));
      assert.equal(path.resolve(found.sigPath), path.resolve(sigPath));
    });

    it("skips non-existent candidate directories gracefully", () => {
      const nonExistent1 = path.join(tmpDir, "missing-1");
      const nonExistent2 = path.join(tmpDir, "missing-2");
      const validDir = path.join(tmpDir, "valid");
      fs.mkdirSync(validDir, { recursive: true });

      const exePath = path.join(validDir, "App.exe");
      const sigPath = path.join(validDir, "App.exe.sig");
      fs.writeFileSync(exePath, "bin");
      fs.writeFileSync(sigPath, "sig");

      const found = find_bundle_artifacts([nonExistent1, nonExistent2, validDir]);
      assert.equal(path.resolve(found.installerPath), path.resolve(exePath));
      assert.equal(path.resolve(found.sigPath), path.resolve(sigPath));
    });

    it("throws clear error when artifacts are not found", () => {
      const emptyDir = path.join(tmpDir, "empty");
      fs.mkdirSync(emptyDir, { recursive: true });

      assert.throws(
        () => find_bundle_artifacts([emptyDir]),
        /Could not locate signed installer/
      );
    });
  });

  describe("find_platform_artifacts", () => {
    it("discovers Linux AppImage-only artifacts", () => {
      const appImageDir = path.join(tmpDir, "bundle-appimage");
      fs.mkdirSync(appImageDir, { recursive: true });

      const appImagePath = path.join(appImageDir, "LiveAudio_1.6.0_amd64.AppImage");
      const sigPath = path.join(appImageDir, "LiveAudio_1.6.0_amd64.AppImage.sig");
      fs.writeFileSync(appImagePath, "ELF-appimage-payload");
      fs.writeFileSync(sigPath, "RWFakeLinuxSignature==");

      const platforms = find_platform_artifacts([appImageDir]);
      assert.deepEqual(Object.keys(platforms), ["linux-x86_64"]);
      assert.equal(path.resolve(platforms["linux-x86_64"].installerPath), path.resolve(appImagePath));
      assert.equal(path.resolve(platforms["linux-x86_64"].sigPath), path.resolve(sigPath));
    });

    it("discovers both Windows and Linux artifacts across multiple directories", () => {
      const winDir = path.join(tmpDir, "win");
      const linuxDir = path.join(tmpDir, "linux");
      fs.mkdirSync(winDir, { recursive: true });
      fs.mkdirSync(linuxDir, { recursive: true });

      const exePath = path.join(winDir, "LiveAudio_1.6.0_x64-setup.exe");
      const exeSigPath = path.join(winDir, "LiveAudio_1.6.0_x64-setup.exe.sig");
      const appImagePath = path.join(linuxDir, "LiveAudio_1.6.0_amd64.AppImage");
      const appImageSigPath = path.join(linuxDir, "LiveAudio_1.6.0_amd64.AppImage.sig");

      fs.writeFileSync(exePath, "exe-payload");
      fs.writeFileSync(exeSigPath, "win-sig");
      fs.writeFileSync(appImagePath, "appimage-payload");
      fs.writeFileSync(appImageSigPath, "linux-sig");

      const platforms = find_platform_artifacts([winDir, linuxDir]);
      assert.deepEqual(Object.keys(platforms).sort(), ["linux-x86_64", "windows-x86_64"]);
      assert.equal(path.resolve(platforms["windows-x86_64"].installerPath), path.resolve(exePath));
      assert.equal(path.resolve(platforms["linux-x86_64"].installerPath), path.resolve(appImagePath));
    });

    it("discovers both Windows and Linux artifacts in a single combined directory", () => {
      const combinedDir = path.join(tmpDir, "combined");
      fs.mkdirSync(combinedDir, { recursive: true });

      const exePath = path.join(combinedDir, "LiveAudio_1.6.0_x64-setup.exe");
      const exeSigPath = path.join(combinedDir, "LiveAudio_1.6.0_x64-setup.exe.sig");
      const appImagePath = path.join(combinedDir, "LiveAudio_1.6.0_amd64.AppImage");
      const appImageSigPath = path.join(combinedDir, "LiveAudio_1.6.0_amd64.AppImage.sig");
      const debPath = path.join(combinedDir, "LiveAudio_1.6.0_amd64.deb");

      fs.writeFileSync(exePath, "exe-payload");
      fs.writeFileSync(exeSigPath, "win-sig");
      fs.writeFileSync(appImagePath, "appimage-payload");
      fs.writeFileSync(appImageSigPath, "linux-sig");
      fs.writeFileSync(debPath, "deb-payload");

      const platforms = find_platform_artifacts([combinedDir]);
      assert.deepEqual(Object.keys(platforms).sort(), ["linux-x86_64", "windows-x86_64"]);
      assert.equal(path.resolve(platforms["windows-x86_64"].installerPath), path.resolve(exePath));
      assert.equal(path.resolve(platforms["linux-x86_64"].installerPath), path.resolve(appImagePath));
    });

    it("pairs the stable-named Windows installer published in release_assets", () => {
      const releaseDir = path.join(tmpDir, "release_assets");
      const outDir = path.join(tmpDir, "out");
      fs.mkdirSync(releaseDir, { recursive: true });

      fs.writeFileSync(path.join(releaseDir, "LiveAudio-Setup-1.6.0.exe"), "exe-payload");
      fs.writeFileSync(path.join(releaseDir, "LiveAudio-Setup-1.6.0.exe.sig"), "win-sig");
      fs.writeFileSync(path.join(releaseDir, "LiveAudio_1.6.0_amd64.AppImage"), "appimage-payload");
      fs.writeFileSync(path.join(releaseDir, "LiveAudio_1.6.0_amd64.AppImage.sig"), "linux-sig");
      fs.writeFileSync(path.join(releaseDir, "liveaudio-cli-1.6.0-windows-x64.zip"), "zip");

      const platforms = find_platform_artifacts([releaseDir]);
      assert.equal(
        path.basename(platforms["windows-x86_64"].installerPath),
        "LiveAudio-Setup-1.6.0.exe"
      );
      assert.equal(
        path.basename(platforms["windows-x86_64"].sigPath),
        "LiveAudio-Setup-1.6.0.exe.sig"
      );

      const { manifestData } = assemble_manifest({
        version: "1.6.0",
        platforms,
        outDir,
        repo: "plynte-labs/LiveAudio",
        tag: "v1.6.0",
      });
      assert.equal(
        manifestData.platforms["windows-x86_64"].url,
        "https://github.com/plynte-labs/LiveAudio/releases/download/v1.6.0/LiveAudio-Setup-1.6.0.exe"
      );
    });

    it("rejects Linux AppImage when .AppImage.sig is missing", () => {
      const linuxDir = path.join(tmpDir, "unsigned-linux");
      fs.mkdirSync(linuxDir, { recursive: true });

      fs.writeFileSync(path.join(linuxDir, "LiveAudio_1.6.0_amd64.AppImage"), "appimage-payload");

      assert.throws(
        () => find_platform_artifacts([linuxDir]),
        /Missing \.AppImage\.sig signature/
      );
    });

    it("rejects missing .AppImage.sig even when Windows artifact is signed in same directory", () => {
      const combinedDir = path.join(tmpDir, "partial-signed");
      fs.mkdirSync(combinedDir, { recursive: true });

      fs.writeFileSync(path.join(combinedDir, "LiveAudio_1.6.0_x64-setup.exe"), "exe");
      fs.writeFileSync(path.join(combinedDir, "LiveAudio_1.6.0_x64-setup.exe.sig"), "win-sig");
      fs.writeFileSync(path.join(combinedDir, "LiveAudio_1.6.0_amd64.AppImage"), "appimage");

      assert.throws(
        () => find_platform_artifacts([combinedDir]),
        /Missing \.AppImage\.sig signature/
      );
    });
  });

  describe("assemble_manifest", () => {
    it("assembles valid Tauri 2 manifest and stages files", () => {
      const srcDir = path.join(tmpDir, "src");
      const outDir = path.join(tmpDir, "dist");
      fs.mkdirSync(srcDir, { recursive: true });

      const installerPath = path.join(srcDir, "LiveAudio-Setup-1.6.0.exe");
      const sigPath = path.join(srcDir, "LiveAudio-Setup-1.6.0.exe.sig");
      const rawSig = "untrusted comment: minisign signature\nRWQ+sampleSig123456789==";

      fs.writeFileSync(installerPath, "PE32+ windows installer binary content");
      fs.writeFileSync(sigPath, rawSig);

      const result = assemble_manifest({
        version: "v1.6.0",
        installerPath,
        sigPath,
        outDir,
        repo: "plynte-labs/LiveAudio",
        tag: "v1.6.0",
        notes: "Release notes for v1.6.0",
      });

      assert.ok(fs.existsSync(result.manifestPath));
      assert.ok(fs.existsSync(result.destInstaller));
      assert.ok(fs.existsSync(result.destSig));

      const manifestContent = JSON.parse(fs.readFileSync(result.manifestPath, "utf-8"));
      assert.equal(manifestContent.version, "1.6.0"); // Leading 'v' stripped
      assert.equal(manifestContent.notes, "Release notes for v1.6.0");
      assert.match(manifestContent.pub_date, /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);

      const platformData = manifestContent.platforms["windows-x86_64"];
      assert.ok(platformData);
      assert.equal(platformData.signature, rawSig);
      assert.equal(
        platformData.url,
        "https://github.com/plynte-labs/LiveAudio/releases/download/v1.6.0/LiveAudio-Setup-1.6.0.exe"
      );
    });

    it("synthesizes tag name and default notes when omitted", () => {
      const srcDir = path.join(tmpDir, "src2");
      const outDir = path.join(tmpDir, "dist2");
      fs.mkdirSync(srcDir, { recursive: true });

      const installerPath = path.join(srcDir, "Setup.exe");
      const sigPath = path.join(srcDir, "Setup.exe.sig");
      fs.writeFileSync(installerPath, "bytes");
      fs.writeFileSync(sigPath, "sig-data");

      const result = assemble_manifest({
        version: "1.6.0", // bare version without 'v'
        installerPath,
        sigPath,
        outDir,
        repo: "test/repo",
      });

      const manifest = JSON.parse(fs.readFileSync(result.manifestPath, "utf-8"));
      assert.equal(manifest.version, "1.6.0");
      assert.equal(manifest.notes, "LiveAudio v1.6.0");
      assert.equal(
        manifest.platforms["windows-x86_64"].url,
        "https://github.com/test/repo/releases/download/v1.6.0/Setup.exe"
      );
    });

    it("throws if signature file is empty", () => {
      const srcDir = path.join(tmpDir, "src3");
      const outDir = path.join(tmpDir, "dist3");
      fs.mkdirSync(srcDir, { recursive: true });

      const installerPath = path.join(srcDir, "Setup.exe");
      const sigPath = path.join(srcDir, "Setup.exe.sig");
      fs.writeFileSync(installerPath, "bytes");
      fs.writeFileSync(sigPath, "   \n\t "); // Whitespace only

      assert.throws(
        () =>
          assemble_manifest({
            version: "1.6.0",
            installerPath,
            sigPath,
            outDir,
          }),
        /is empty/
      );
    });

    it("rejects assembly when zero platforms are provided", () => {
      const outDir = path.join(tmpDir, "dist-empty");
      assert.throws(
        () =>
          assemble_manifest({
            version: "1.6.0",
            platforms: {},
            outDir,
          }),
        /At least one signed platform artifact is required/
      );
    });

    it("assembles Linux AppImage-only manifest", () => {
      const srcDir = path.join(tmpDir, "src-linux");
      const outDir = path.join(tmpDir, "dist-linux");
      fs.mkdirSync(srcDir, { recursive: true });

      const appImagePath = path.join(srcDir, "LiveAudio_1.6.0_amd64.AppImage");
      const sigPath = path.join(srcDir, "LiveAudio_1.6.0_amd64.AppImage.sig");
      fs.writeFileSync(appImagePath, "ELF-appimage");
      fs.writeFileSync(sigPath, "linux-minisign-sig");

      const result = assemble_manifest({
        version: "1.6.0",
        platforms: {
          "linux-x86_64": { installerPath: appImagePath, sigPath },
        },
        outDir,
        repo: "plynte-labs/LiveAudio",
        tag: "v1.6.0",
      });

      const manifest = JSON.parse(fs.readFileSync(result.manifestPath, "utf-8"));
      assert.deepEqual(Object.keys(manifest.platforms), ["linux-x86_64"]);
      assert.equal(manifest.platforms["linux-x86_64"].signature, "linux-minisign-sig");
      assert.equal(
        manifest.platforms["linux-x86_64"].url,
        "https://github.com/plynte-labs/LiveAudio/releases/download/v1.6.0/LiveAudio_1.6.0_amd64.AppImage"
      );
    });

    it("assembles combined Windows + Linux multi-platform manifest via CLI", () => {
      const combinedDir = path.join(tmpDir, "combined-cli");
      const outDir = path.join(tmpDir, "combined-out");
      fs.mkdirSync(combinedDir, { recursive: true });

      fs.writeFileSync(path.join(combinedDir, "LiveAudio_1.6.0_x64-setup.exe"), "exe-bytes");
      fs.writeFileSync(path.join(combinedDir, "LiveAudio_1.6.0_x64-setup.exe.sig"), "win-sig-123");
      fs.writeFileSync(path.join(combinedDir, "LiveAudio_1.6.0_amd64.AppImage"), "appimage-bytes");
      fs.writeFileSync(path.join(combinedDir, "LiveAudio_1.6.0_amd64.AppImage.sig"), "linux-sig-456");

      const code = main([
        "--version",
        "1.6.0",
        "--bundle-dir",
        combinedDir,
        "--out-dir",
        outDir,
        "--repo",
        "plynte-labs/LiveAudio",
        "--tag",
        "v1.6.0",
      ]);

      assert.equal(code, 0);
      const manifest = JSON.parse(fs.readFileSync(path.join(outDir, "latest.json"), "utf-8"));
      assert.equal(manifest.version, "1.6.0");
      assert.deepEqual(Object.keys(manifest.platforms).sort(), ["linux-x86_64", "windows-x86_64"]);
      assert.equal(manifest.platforms["windows-x86_64"].signature, "win-sig-123");
      assert.equal(
        manifest.platforms["windows-x86_64"].url,
        "https://github.com/plynte-labs/LiveAudio/releases/download/v1.6.0/LiveAudio_1.6.0_x64-setup.exe"
      );
      assert.equal(manifest.platforms["linux-x86_64"].signature, "linux-sig-456");
      assert.equal(
        manifest.platforms["linux-x86_64"].url,
        "https://github.com/plynte-labs/LiveAudio/releases/download/v1.6.0/LiveAudio_1.6.0_amd64.AppImage"
      );
    });
  });

  describe("main CLI wrapper", () => {
    it("runs successfully with explicit --installer and --sig flags", () => {
      const bundleDir = path.join(tmpDir, "cli-test");
      const outDir = path.join(tmpDir, "cli-out");
      fs.mkdirSync(bundleDir, { recursive: true });

      const installer = path.join(bundleDir, "LiveAudio.exe");
      const sig = path.join(bundleDir, "LiveAudio.exe.sig");
      fs.writeFileSync(installer, "exe-content");
      fs.writeFileSync(sig, "minisign-sig");

      const code = main([
        "--version",
        "1.6.0",
        "--installer",
        installer,
        "--sig",
        sig,
        "--out-dir",
        outDir,
        "--repo",
        "owner/repo",
        "--tag",
        "v1.6.0",
      ]);

      assert.equal(code, 0);
      assert.ok(fs.existsSync(path.join(outDir, "latest.json")));
      assert.ok(fs.existsSync(path.join(outDir, "LiveAudio.exe")));
      assert.ok(fs.existsSync(path.join(outDir, "LiveAudio.exe.sig")));
    });

    it("returns 1 if --version is missing", () => {
      const code = main(["--out-dir", tmpDir]);
      assert.equal(code, 1);
    });
  });
});
