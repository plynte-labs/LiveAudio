// Fails the release when the pinned CUDA worker is published but its download does not match the pin.
// A placeholder (all-zero SHA-256) is accepted: the app then reports the worker as not published.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const pin = JSON.parse(readFileSync(new URL('./ct2-worker.pin.json', import.meta.url), 'utf8'));
if (/^0+$/.test(pin.sha256)) {
  console.log('ct2-worker.pin.json is the unpublished placeholder; nothing to verify.');
  process.exit(0);
}
const res = await fetch(pin.url, { redirect: 'follow' });
if (!res.ok) throw new Error(`Pinned worker URL answered HTTP ${res.status}: ${pin.url}`);
const bytes = Buffer.from(await res.arrayBuffer());
const sha256 = createHash('sha256').update(bytes).digest('hex');
if (bytes.length !== pin.size) throw new Error(`Pinned worker size mismatch: pin ${pin.size}, got ${bytes.length}`);
if (sha256 !== pin.sha256) throw new Error(`Pinned worker SHA-256 mismatch: pin ${pin.sha256}, got ${sha256}`);
console.log(`Pinned worker verified: ${pin.url} (${bytes.length} bytes)`);
