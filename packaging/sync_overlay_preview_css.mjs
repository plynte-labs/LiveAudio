// SPDX-License-Identifier: MIT
/**
 * Generates desktop/src/overlay-preview.css from the SUBTITLE-STYLE regions of the OBS overlay
 * (assets/subtitulos_obs.html), so the desktop Style tab preview uses the overlay's own rules.
 * Selectors are scoped under `.sub-style-preview` (`:root` -> the container, `html[...]` -> the
 * container's attributes). Run `node packaging/sync_overlay_preview_css.mjs` after editing a region
 * and `--check` to verify without writing.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCOPE = '.sub-style-preview';
const REGION = /\/\* SUBTITLE-STYLE:BEGIN[^*]*\*\/([\s\S]*?)\/\* SUBTITLE-STYLE:END \*\//g;

function scopeSelector(selector) {
  const sel = selector.trim();
  if (sel === ':root') return SCOPE;
  if (sel.startsWith('html[')) return SCOPE + sel.slice('html'.length);
  return `${SCOPE} ${sel}`;
}

// Trimmed non-empty lines, indented two spaces per brace depth (keyframe steps nest one level).
function lines(body) {
  let depth = 1;
  return body.split('\n').map((line) => line.trim()).filter(Boolean).map((line) => {
    if (line.startsWith('}')) depth -= 1;
    const text = '  '.repeat(depth) + line;
    if (line.endsWith('{')) depth += 1;
    return text;
  });
}

export function buildPreviewCss(overlayHtml) {
  const source = overlayHtml.replace(/\r\n/g, '\n');
  const regions = [...source.matchAll(REGION)].map((match) => match[1].replace(/\/\*[\s\S]*?\*\//g, ''));
  if (regions.length === 0) throw new Error('No SUBTITLE-STYLE regions found in the overlay');
  const out = ['/* GENERATED from assets/subtitulos_obs.html by packaging/sync_overlay_preview_css.mjs. Do not edit. */'];
  for (const region of regions) {
    let rest = region;
    for (;;) {
      const open = rest.indexOf('{');
      if (open === -1) break;
      let depth = 0;
      let close = open;
      for (; close < rest.length; close += 1) {
        if (rest[close] === '{') depth += 1;
        else if (rest[close] === '}' && --depth === 0) break;
      }
      const prelude = rest.slice(0, open).trim();
      const body = rest.slice(open + 1, close).replace(/\{/g, '{\n').replace(/\}/g, '\n}').replace(/;\s*/g, ';\n');
      rest = rest.slice(close + 1);
      out.push('');
      if (prelude.startsWith('@keyframes')) {
        out.push(`${prelude} {`, ...lines(body), '}');
      } else {
        const selectors = prelude.split(',').map(scopeSelector);
        out.push(`${selectors.join(',\n')} {`, ...lines(body), '}');
      }
    }
  }
  return `${out.join('\n')}\n`;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const target = path.join(root, 'desktop', 'src', 'overlay-preview.css');
  const css = buildPreviewCss(fs.readFileSync(path.join(root, 'assets', 'subtitulos_obs.html'), 'utf8')).replace(/\n/g, '\r\n');
  if (process.argv.includes('--check')) {
    const current = fs.existsSync(target) ? fs.readFileSync(target, 'utf8') : '';
    if (current !== css) { console.error('overlay-preview.css is out of date'); process.exit(1); }
  } else {
    fs.writeFileSync(target, css);
  }
}
