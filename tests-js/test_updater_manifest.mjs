// SPDX-License-Identifier: MIT
import { describe, it, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

import {
  find_bundle_artifacts,
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
        /Could not locate both installer \.exe and \.sig file/
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
