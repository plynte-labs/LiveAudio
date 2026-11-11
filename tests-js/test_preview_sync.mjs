// SPDX-License-Identifier: MIT
/**
 * The Style tab preview must render what the OBS overlay renders. The overlay HTML is the
 * single source: its marked CSS regions are generated into desktop/src/overlay-preview.css,
 * and the preview derives its element properties with the same rules as the overlay's applyTheme.
 */
import fs from 'node:fs';
import path from 'node:path';
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..');
const read = (...parts) => fs.readFileSync(path.join(root, ...parts), 'utf8');
const overlay = read('assets', 'subtitulos_obs.html');
const normalize = (text) => text.replace(/\r\n/g, '\n');

const { buildPreviewCss } = await import('../packaging/sync_overlay_preview_css.mjs').catch(() => ({}));
const { previewProperties } = await import('../desktop/src/subtitle-style.js');

const STYLES = ['default', 'karaoke', 'neon', 'minimal', 'bold', 'rgb', 'typewriter'];

describe('preview CSS comes from the overlay', () => {
  it('overlay-preview.css is the generated output of the overlay regions', () => {
    assert.equal(typeof buildPreviewCss, 'function', 'packaging/sync_overlay_preview_css.mjs exports buildPreviewCss');
    assert.equal(normalize(read('desktop', 'src', 'overlay-preview.css')), normalize(buildPreviewCss(overlay)),
      'run: node packaging/sync_overlay_preview_css.mjs');
  });

  it('covers every style class and the animation overrides of the overlay', () => {
    const css = normalize(read('desktop', 'src', 'overlay-preview.css'));
    for (const style of STYLES) assert.ok(css.includes(`.sub-style-preview .style-${style} {`), `style-${style}`);
    assert.ok(css.includes('.sub-style-preview[data-anim="fade"] .word'), 'fade override');
    assert.ok(css.includes('.sub-style-preview[data-anim="none"] .word'), 'none override');
    assert.ok(css.includes('.sub-style-preview[data-anim="fade"] .sub-box'), 'fade box override');
    assert.ok(css.includes('.sub-style-preview[data-anim="none"] .sub-box'), 'none box override');
    assert.ok(css.includes('--sub-lifetime: 5s'), 'time on screen default');
    assert.ok(css.includes('.sub-style-preview[data-font="custom"] .style-typewriter'), 'custom font override');
    assert.ok(css.includes('--sub-user-color') && css.includes('--sub-word-spacing: normal'), 'tokens and user vars');
  });

  it('styles.css keeps no second copy of the overlay style rules', () => {
    const css = normalize(read('desktop', 'src', 'styles.css'));
    assert.doesNotMatch(css, /^\.style-(default|karaoke|neon|minimal|bold|rgb|typewriter)\b/m);
    assert.doesNotMatch(css, /^\.sub-box\b/m);
    assert.doesNotMatch(css, /@keyframes (popIn|rgbFadeIn|typeIn)\b/);
  });

  it('index.html loads the generated stylesheet after styles.css', () => {
    const html = read('desktop', 'src', 'index.html');
    const styles = html.indexOf('href="styles.css"');
    const generated = html.indexOf('href="overlay-preview.css"');
    assert.ok(styles !== -1 && generated > styles);
  });
});

describe('preview properties match overlay applyTheme', () => {
  const themeBlock = overlay.match(/\/\* THEME-ENGINE:BEGIN \*\/([\s\S]*?)\/\* THEME-ENGINE:END \*\//)[1];
  const overlayResult = (theme) => {
    const props = new Map();
    const dataset = {};
    const document = { documentElement: { style: { setProperty: (k, v) => props.set(k, v), removeProperty: (k) => props.delete(k) }, dataset } };
    new Function('document', `${themeBlock}\napplyTheme(${JSON.stringify(theme)});`)(document);
    return { props: Object.fromEntries([...props].sort()), anim: dataset.anim, font: dataset.font };
  };
  const themes = [
    {},
    { '--sub-color': '#ff0000' },
    { '--sub-bg': 'rgba(10, 20, 30, 0.4)' },
    { '--sub-letter-spacing': '3px' },
    { '--sub-word-spacing': '12px' },
    { '--sub-font-family': 'Georgia', '--sub-font-size': '60px', '--sub-font-weight': '400' },
    { '--sub-radius': '4px', '--sub-padding': '2px 4px', '--sub-border': '2px solid #ff0000', '--sub-text-transform': 'uppercase' },
    { '--sub-animation': 'fade', '--sub-animation-duration': '1.2s' },
    { '--sub-animation': 'none', '--sub-color': '#00ff00' },
    { '--sub-lifetime': '8s' },
    { '--sub-shadow': 'none', '--sub-letter-spacing': '-1.5px', '--sub-word-spacing': '-2px' },
  ];
  themes.forEach((theme, index) => {
    it(`theme ${index} ${JSON.stringify(theme)}`, () => {
      const expected = overlayResult(theme);
      const actual = previewProperties(theme);
      assert.deepEqual(Object.fromEntries(Object.entries(actual.props).sort()), expected.props);
      assert.equal(actual.anim, expected.anim);
      assert.equal(actual.font, expected.font);
    });
  });
});
