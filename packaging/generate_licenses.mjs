// Generates THIRD_PARTY_LICENSES.html: license notices for the Rust crates compiled into the
// LiveAudio binaries, rendered by cargo-about from about.toml and packaging/about.hbs.
// Requires cargo-about at the pinned version (CI installs it in .github/workflows/release.yml).
import { spawnSync } from 'node:child_process';
import { statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const CARGO_ABOUT_VERSION = '0.9.3';
const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const output = join(root, 'THIRD_PARTY_LICENSES.html');

const probe = spawnSync('cargo', ['about', '--version'], { cwd: root, encoding: 'utf8' });
if (probe.error || probe.status !== 0 || !probe.stdout.includes(` ${CARGO_ABOUT_VERSION}`)) {
  console.error(
    `cargo-about ${CARGO_ABOUT_VERSION} is required. Install it with:\n` +
      `  cargo install cargo-about --locked --version ${CARGO_ABOUT_VERSION} --features cli`,
  );
  process.exit(1);
}

const generate = spawnSync(
  'cargo',
  ['about', 'generate', '--locked', '--workspace', 'packaging/about.hbs', '-o', output],
  { cwd: root, stdio: 'inherit' },
);
if (generate.status !== 0) {
  console.error('cargo-about failed to generate THIRD_PARTY_LICENSES.html');
  process.exit(generate.status ?? 1);
}

console.log(`Wrote ${output} (${(statSync(output).size / 1024).toFixed(0)} KiB)`);
