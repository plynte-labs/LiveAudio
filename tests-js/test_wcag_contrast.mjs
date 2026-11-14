// SPDX-License-Identifier: MIT
/**
 * WCAG 2.1 AA relative luminance and contrast ratio verification for all 7 subtitle presets.
 * Ported from tests/test_wcag_contrast.py to native Node.js test runner (node:test).
 */

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

/**
 * Converts a hex color string (#rgb or #rrggbb) to an RGB tuple [r, g, b].
 * @param {string} hexColor
 * @returns {[number, number, number]}
 */
export function hexToRgb(hexColor) {
  let hex = hexColor.replace(/^#/, '');
  if (hex.length === 3) {
    hex = hex.split('').map((c) => c + c).join('');
  }
  if (hex.length !== 6) {
    throw new Error(`Invalid hex color: ${hexColor}`);
  }
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  return [r, g, b];
}

/**
 * Calculates relative luminance per WCAG 2.1 formula.
 * @param {[number, number, number]} rgb
 * @returns {number}
 */
export function relativeLuminance(rgb) {
  const linearize = (c) => {
    const v = c / 255.0;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  };
  const [r, g, b] = rgb;
  return 0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b);
}

/**
 * Calculates contrast ratio between two hex colors per WCAG 2.1.
 * Ratio formula: (L1 + 0.05) / (L2 + 0.05), where L1 is lighter and L2 darker.
 * @param {string} color1
 * @param {string} color2
 * @returns {number}
 */
export function contrastRatio(color1, color2) {
  const l1 = relativeLuminance(hexToRgb(color1));
  const l2 = relativeLuminance(hexToRgb(color2));
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

// Preset color definitions extracted from subtitulos_obs.html CSS
export const PRESETS = {
  default: { color: '#ffffff', bg: '#000000' },     // white on rgba(0,0,0,0.7) ~ black
  karaoke: { color: '#ffcc00', bg: '#000000' },     // yellow on rgba(0,0,0,0.7) ~ black
  neon: { color: '#ffffff', bg: '#001414' },        // white on rgba(0,20,20,0.6)
  minimal: { color: '#ffffff', bg: '#000000' },     // white on rgba(0,0,0,0.55)
  bold: { color: '#ffffff', bg: '#000000' },        // white on rgba(0,0,0,0.85)
  rgb: { color: '#ffffff', bg: '#000000' },         // white on rgba(0,0,0,0.7)
  typewriter: { color: '#ffffff', bg: '#000000' },  // white on rgba(0,0,0,0.75)
};

describe('WCAG 2.1 Math Functions', () => {
  it('hexToRgb handles 3-digit and 6-digit hex values', () => {
    assert.deepEqual(hexToRgb('#fff'), [255, 255, 255]);
    assert.deepEqual(hexToRgb('#000'), [0, 0, 0]);
    assert.deepEqual(hexToRgb('#ffcc00'), [255, 204, 0]);
    assert.deepEqual(hexToRgb('#001414'), [0, 20, 20]);
  });

  it('relativeLuminance calculates exact bounds for black and white', () => {
    const blackLum = relativeLuminance([0, 0, 0]);
    const whiteLum = relativeLuminance([255, 255, 255]);
    assert.equal(blackLum, 0);
    assert.equal(whiteLum, 1);
  });

  it('contrastRatio computes 21:1 for white against black', () => {
    const ratio = contrastRatio('#ffffff', '#000000');
    assert.equal(Math.round(ratio * 100) / 100, 21);
  });

  it('contrastRatio computes 1:1 for identical colors', () => {
    const ratio = contrastRatio('#ffcc00', '#ffcc00');
    assert.equal(ratio, 1.0);
  });
});

describe('WCAG AA Contrast: Large Text (>= 3.0:1)', () => {
  function checkContrast(presetName, fg, bg, minRatio = 3.0) {
    const ratio = contrastRatio(fg, bg);
    assert.ok(
      ratio >= minRatio,
      `Preset '${presetName}' contrast ratio ${ratio.toFixed(2)}:1 < ${minRatio}:1 (WCAG AA large text)`
    );
    return ratio;
  }

  it('default preset: white on black exceeds 20:1', () => {
    const ratio = checkContrast('default', PRESETS.default.color, PRESETS.default.bg);
    assert.ok(ratio > 20.0, `Ratio ${ratio} should exceed 20:1`);
  });

  it('karaoke preset: yellow (#ffcc00) on black exceeds 3:1', () => {
    const ratio = checkContrast('karaoke', PRESETS.karaoke.color, PRESETS.karaoke.bg);
    assert.ok(ratio >= 3.0, `Ratio ${ratio} should be at least 3:1`);
  });

  it('neon preset: white on dark teal (#001414) exceeds 3:1', () => {
    const ratio = checkContrast('neon', PRESETS.neon.color, PRESETS.neon.bg);
    assert.ok(ratio >= 3.0, `Ratio ${ratio} should be at least 3:1`);
  });

  it('minimal preset: white on dark bg exceeds 3:1', () => {
    const ratio = checkContrast('minimal', PRESETS.minimal.color, PRESETS.minimal.bg);
    assert.ok(ratio >= 3.0, `Ratio ${ratio} should be at least 3:1`);
  });

  it('bold preset: white on near-black exceeds 20:1', () => {
    const ratio = checkContrast('bold', PRESETS.bold.color, PRESETS.bold.bg);
    assert.ok(ratio > 20.0, `Ratio ${ratio} should exceed 20:1`);
  });

  it('rgb preset: white on black base exceeds 20:1', () => {
    const ratio = checkContrast('rgb', PRESETS.rgb.color, PRESETS.rgb.bg);
    assert.ok(ratio > 20.0, `Ratio ${ratio} should exceed 20:1`);
  });

  it('typewriter preset: white on dark bg exceeds 20:1', () => {
    const ratio = checkContrast('typewriter', PRESETS.typewriter.color, PRESETS.typewriter.bg);
    assert.ok(ratio > 20.0, `Ratio ${ratio} should exceed 20:1`);
  });
});

describe('WCAG AA Contrast: Normal Text (>= 4.5:1)', () => {
  for (const [name, colors] of Object.entries(PRESETS)) {
    it(`preset '${name}' meets WCAG AA normal text threshold (>= 4.5:1)`, () => {
      const ratio = contrastRatio(colors.color, colors.bg);
      assert.ok(
        ratio >= 4.5,
        `Preset '${name}' contrast ratio ${ratio.toFixed(2)}:1 must be >= 4.5:1 for normal text`
      );
    });
  }
});
