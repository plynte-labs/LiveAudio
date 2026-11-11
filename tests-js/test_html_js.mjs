// SPDX-License-Identifier: MIT
/**
 * Tests for JavaScript and CSS subtitle rendering logic in assets/subtitulos_obs.html.
 * Ported from tests/test_html_js.py to native Node.js test runner (node:test).
 */

import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

const htmlPath = path.resolve(import.meta.dirname, '..', 'assets', 'subtitulos_obs.html');
const htmlContent = fs.readFileSync(htmlPath, 'utf8');

describe('RGB Per-Word Color Animation', () => {
  it('splits text into word spans with HSL colors when style is rgb', () => {
    const hasRgbSplit = (htmlContent.includes("'rgb'") || htmlContent.includes('"rgb"'))
      && htmlContent.includes('split')
      && htmlContent.toLowerCase().includes('hsl');
    assert.ok(hasRgbSplit, 'RGB style should split text into words with HSL colors');
  });

  it('generates HSL colors with minimum luminance of 40%', () => {
    const hasHsl = htmlContent.toLowerCase().includes('hsl(');
    assert.ok(hasHsl, 'RGB style should generate HSL colors');

    const luminanceMatch = htmlContent.match(/luminance\s*=\s*(\d+)\s*\+\s*Math\.floor/);
    assert.ok(luminanceMatch, 'RGB style should calculate luminance with a floor');
    const minLuminance = parseInt(luminanceMatch[1], 10);
    assert.ok(minLuminance >= 40, `RGB minimum luminance ${minLuminance}% must be at least 40%`);
  });

  it('provides rgbFadeIn animation for entry', () => {
    const hasRgbFadeIn = htmlContent.includes('rgbFadeIn');
    assert.ok(hasRgbFadeIn, 'RGB style should define rgbFadeIn animation');
  });
});

describe('Typewriter Sequential Reveal Animation', () => {
  it('splits text into word spans with staggered animation delays', () => {
    const hasTypewriterSplit = (htmlContent.includes("'typewriter'") || htmlContent.includes('"typewriter"'))
      && htmlContent.includes('animationDelay');
    assert.ok(hasTypewriterSplit, 'Typewriter should split text with staggered delays');
  });

  it('defines blinking cursor keyframes animation', () => {
    const hasBlink = htmlContent.toLowerCase().includes('blink') && htmlContent.includes('@keyframes');
    assert.ok(hasBlink, 'Typewriter should have blink keyframes');
  });

  it('clamps 40ms base delay against REVEAL_BUDGET_MS', () => {
    assert.ok(
      htmlContent.includes('REVEAL_BUDGET_MS'),
      'Typewriter stagger must reference the REVEAL_BUDGET_MS cap constant'
    );

    const clampPattern = /Math\.min\(\s*40\s*,\s*REVEAL_BUDGET_MS\s*\//;
    assert.match(
      htmlContent,
      clampPattern,
      'Typewriter reveal must clamp 40ms base delay via Math.min(40, REVEAL_BUDGET_MS / ...)'
    );

    assert.ok(
      htmlContent.includes('animationDelay'),
      'Typewriter must assign the capped delay to animationDelay'
    );
  });
});

describe('Entry and Exit Animation Mirroring', () => {
  it('defines .hiding state for exit animation across themes', () => {
    const hasHiding = htmlContent.includes('.hiding') && htmlContent.includes("classList.add('hiding')");
    assert.ok(hasHiding, 'All themes should use .hiding class for exit');
  });

  it('uses mirrored translateY distance between entry and exit', () => {
    const hasEntry = htmlContent.includes('translateY(20px)') || htmlContent.includes('translateY(10px)');
    const hasExit = htmlContent.includes('translateY(12px)') || htmlContent.includes('translateY(10px)');
    assert.ok(hasEntry && hasExit, 'Entry and exit should use mirrored translateY');
  });
});

describe('Valid Styles and URL Parameters', () => {
  it('includes all 7 preset styles in VALID_STYLES', () => {
    const requiredStyles = ['default', 'karaoke', 'neon', 'minimal', 'bold', 'rgb', 'typewriter'];
    for (const style of requiredStyles) {
      assert.ok(htmlContent.includes(`'${style}'`), `VALID_STYLES should include '${style}'`);
    }
  });

  it('parses ?style= URL parameter on load', () => {
    const hasUrlParser = htmlContent.includes('URLSearchParams') && htmlContent.toLowerCase().includes('style');
    assert.ok(hasUrlParser, 'JS should parse ?style= URL parameter');
  });

  it('handles style parameter with fallback', () => {
    const hasLegacy = htmlContent.includes("get('style')") || htmlContent.includes('get("style")');
    assert.ok(hasLegacy, 'JS should read style URL parameter');
  });

  it('parses ?mode= URL parameter', () => {
    assert.match(
      htmlContent,
      /urlParams\.get\(\s*['"]mode['"]\s*\)/,
      'JS should parse ?mode= via urlParams.get("mode")'
    );
  });

  it('parses ?lines= URL parameter', () => {
    assert.match(
      htmlContent,
      /urlParams\.get\(\s*['"]lines['"]\s*\)/,
      'JS should parse ?lines= via urlParams.get("lines")'
    );
  });

  it('resolves mode to "adaptive" by default', () => {
    assert.match(
      htmlContent,
      /resolvedMode\s*=[^;]*['"]adaptive['"]/,
      'Resolved mode must default to "adaptive", not "single"'
    );
  });
});

describe('Animation Duration Sync', () => {
  it('matches CSS :root and JS token default animation duration', () => {
    const rootMatch = htmlContent.match(/--sub-animation-duration:\s*([0-9.]+s)\s*;/);
    assert.ok(rootMatch, 'CSS :root must declare --sub-animation-duration with an s value');

    const tokenMatch = htmlContent.match(/'--sub-animation-duration':\s*\{[^}]*default:\s*'([0-9.]+s)'/);
    assert.ok(tokenMatch, 'JS VALID_THEME_TOKENS must declare a default for --sub-animation-duration');

    assert.equal(
      rootMatch[1],
      tokenMatch[1],
      'CSS :root and JS token default for --sub-animation-duration must be identical'
    );
  });

  it('stays within the [0.1, 2.0] validator bounds', () => {
    const rootMatch = htmlContent.match(/--sub-animation-duration:\s*([0-9.]+)s\s*;/);
    assert.ok(rootMatch);
    const value = parseFloat(rootMatch[1]);
    assert.ok(value >= 0.1, 'Duration below validator min 0.1');
    assert.ok(value <= 2.0, 'Duration above validator max 2.0');
  });
});

describe('Reveal Budget Cap', () => {
  it('defines REVEAL_BUDGET_MS exactly once as a named constant', () => {
    const defs = [...htmlContent.matchAll(/const\s+REVEAL_BUDGET_MS\s*=\s*(\d+)\s*;/g)];
    assert.equal(defs.length, 1, 'REVEAL_BUDGET_MS must be defined exactly once as a named constant');
    assert.equal(defs[0][1], '400', 'REVEAL_BUDGET_MS must default to 400 (OD-4)');
  });

  it('clamps base delays for karaoke (25ms), rgb (50ms), and typewriter (40ms)', () => {
    for (const base of ['25', '50', '40']) {
      const pattern = new RegExp(`Math\\.min\\(\\s*${base}\\s*,\\s*REVEAL_BUDGET_MS\\s*\\/`);
      assert.match(
        htmlContent,
        pattern,
        `A style must clamp its ${base}ms base delay via Math.min(${base}, REVEAL_BUDGET_MS / ...)`
      );
    }
  });

  it('guards single word in divisor using Math.max(words.length - 1, 1)', () => {
    assert.match(
      htmlContent,
      /Math\.max\(\s*words\.length\s*-\s*1\s*,\s*1\s*\)/,
      'Per-word delay divisor must use Math.max(words.length - 1, 1) to avoid div-by-zero'
    );
  });

  it('applies cap to both reveal and hide cascades', () => {
    const occurrences = [...htmlContent.matchAll(/Math\.min\(\s*40\s*,\s*REVEAL_BUDGET_MS\s*\//g)].length;
    assert.ok(
      occurrences >= 2,
      'Typewriter clamp must be applied on both reveal and hide (>=2 occurrences)'
    );
  });

  it('ensures DOM cleanup timeout is >= 600ms to outlast budget and exit animation', () => {
    const cleanupTimeouts = [...htmlContent.matchAll(/setTimeout\(\s*cleanupCb\s*,\s*exitCleanupMs\(\)\s*\)/g)];
    assert.ok(cleanupTimeouts.length >= 1, 'Could not locate any cleanupCb setTimeout schedule');
    const floor = htmlContent.match(/function exitCleanupMs\(\)\s*\{\s*return Math\.max\(\s*(\d+)\s*,/);
    assert.ok(floor && parseInt(floor[1], 10) >= 600, 'Cleanup delay must keep a floor >= 600ms so tail words finish');
  });
});

describe('Style Size and Weight Consistency', () => {
  it('does not apply per-style font-size multipliers', () => {
    assert.doesNotMatch(
      htmlContent,
      /font-size:\s*calc\(\s*var\(--sub-font-size\)\s*\*/,
      'Styles must not apply per-style font-size multipliers; use var(--sub-font-size)'
    );
  });

  it('does not hardcode numeric font-weight in CSS rules', () => {
    assert.doesNotMatch(
      htmlContent,
      /\n\s*font-weight:\s*\d{3}\s*;/,
      'Styles must not hardcode a numeric font-weight; use var(--sub-font-weight)'
    );
  });

  it('ensures all 7 styles reference font-weight: var(--sub-font-weight)', () => {
    const count = (htmlContent.match(/font-weight:\s*var\(--sub-font-weight\)/g) || []).length;
    assert.equal(count, 7, 'All 7 styles must use font-weight: var(--sub-font-weight)');
  });
});

describe('All 7 Subtitle Styles CSS Rules', () => {
  const styles = ['default', 'karaoke', 'neon', 'minimal', 'bold', 'rgb', 'typewriter'];

  for (const style of styles) {
    it(`defines CSS rule for .style-${style}`, () => {
      const pattern = new RegExp(`\\.style-${style}\\s*\\{`);
      assert.match(htmlContent, pattern, `CSS must define .style-${style} class`);
    });
  }
});

describe('XSS Prevention and Text Sanitization', () => {
  it('uses textContent or createTextNode for rendering text to avoid raw HTML injection', () => {
    assert.match(
      htmlContent,
      /sub\.textContent\s*=\s*safeText/,
      'Plain subtitle rendering must assign to textContent, never innerHTML'
    );
    assert.match(
      htmlContent,
      /span\.textContent\s*=\s*word/,
      'Word span rendering must assign to textContent, never innerHTML'
    );
  });

  it('never assigns untrusted user text directly to innerHTML in overlay script', () => {
    const scriptMatch = htmlContent.match(/<script>([\s\S]*?)<\/script>/);
    assert.ok(scriptMatch, 'Script block must be present');
    const scriptCode = scriptMatch[1];
    assert.doesNotMatch(
      scriptCode,
      /\.innerHTML\s*=\s*(text|safeText|word|data\.)/,
      'Script must not assign dynamic text to innerHTML'
    );
  });

  it('strips dangerous Unicode including bidirectional overrides and control chars', () => {
    assert.match(
      htmlContent,
      /\[\\u202E\\u202D\\u200E\\u200F\\u0000\\u001b\]/,
      'safeText sanitization must strip bidi override characters and control characters'
    );
  });
});

describe('Word Spacing, Margin-Right Elimination, and Sub-box Enforcements', () => {
  it('declares --sub-word-spacing in :root with default normal', () => {
    assert.match(
      htmlContent,
      /--sub-word-spacing:\s*normal\s*;/,
      ':root must declare --sub-word-spacing: normal;'
    );
  });

  it('enforces text-align: center !important and word-spacing in .sub-box', () => {
    const subBoxMatch = htmlContent.match(/\.sub-box\s*\{([^}]*)\}/);
    assert.ok(subBoxMatch, '.sub-box rule not found');
    const css = subBoxMatch[1];
    assert.match(
      css,
      /text-align:\s*center\s*!important\s*;/,
      '.sub-box must enforce text-align: center !important'
    );
    assert.match(
      css,
      /word-spacing:\s*var\(--sub-word-spacing,\s*normal\)\s*;/,
      '.sub-box must set word-spacing: var(--sub-word-spacing, normal)'
    );
  });

  it('does not apply hardcoded margin-right on word spans in animated presets', () => {
    assert.doesNotMatch(
      htmlContent,
      /\.style-karaoke\s+\.word\s*\{[^}]*margin-right/,
      '.style-karaoke .word must not have hardcoded margin-right'
    );
    assert.doesNotMatch(
      htmlContent,
      /\.style-rgb\s+\.word\s*\{[^}]*margin-right/,
      '.style-rgb .word must not have hardcoded margin-right'
    );
    assert.doesNotMatch(
      htmlContent,
      /\.style-typewriter\s+\.word\s*\{[^}]*margin-right/,
      '.style-typewriter .word must not have hardcoded margin-right'
    );
  });

  it('preserves display: inline-block on word spans across animated presets', () => {
    for (const style of ['karaoke', 'rgb', 'typewriter']) {
      const match = htmlContent.match(new RegExp(`\\.style-${style}\\s+\\.word\\s*\\{([^}]*)\\}`));
      assert.ok(match, `.style-${style} .word rule must exist`);
      assert.match(
        match[1],
        /display:\s*inline-block/,
        `.style-${style} .word must retain display: inline-block for transforms`
      );
    }
  });

  it('registers --sub-word-spacing and --sub-letter-spacing in VALID_THEME_TOKENS', () => {
    assert.match(
      htmlContent,
      /'--sub-word-spacing':\s*\{\s*type:\s*'spacing'\s*\}/,
      'VALID_THEME_TOKENS must register --sub-word-spacing as spacing'
    );
    assert.match(
      htmlContent,
      /'--sub-letter-spacing':\s*\{\s*type:\s*'spacing'\s*\}/,
      'VALID_THEME_TOKENS must register --sub-letter-spacing as spacing'
    );
  });

  it('ensures all 7 preset styles honor user overrides for spacing, padding, radius, border, and transform', () => {
    const styles = ['default', 'karaoke', 'neon', 'minimal', 'bold', 'rgb', 'typewriter'];
    for (const style of styles) {
      const match = htmlContent.match(new RegExp(`\\.style-${style}\\s*\\{([^}]*)\\}`));
      assert.ok(match, `.style-${style} rule must exist`);
      const body = match[1];

      assert.match(
        body,
        /letter-spacing:\s*var\(--sub-user-letter-spacing,/,
        `.style-${style} must use var(--sub-user-letter-spacing, <preset-default>)`
      );
      assert.match(
        body,
        /word-spacing:\s*var\(--sub-word-spacing,\s*normal\)/,
        `.style-${style} must use var(--sub-word-spacing, normal)`
      );
      assert.match(
        body,
        /padding:\s*var\(--sub-user-padding,/,
        `.style-${style} must use var(--sub-user-padding, <preset-default>)`
      );
      assert.match(
        body,
        /border-radius:\s*var\(--sub-user-radius,/,
        `.style-${style} must use var(--sub-user-radius, <preset-default>)`
      );
      assert.match(
        body,
        /border:\s*var\(--sub-user-border,/,
        `.style-${style} must use var(--sub-user-border, <preset-default>)`
      );
      assert.match(
        body,
        /text-transform:\s*var\(--sub-user-text-transform,/,
        `.style-${style} must use var(--sub-user-text-transform, <preset-default>)`
      );
    }
  });

  it('applies --sub-word-spacing and --sub-letter-spacing dynamically across presets via applyTheme', () => {
    const themeEngineStart = htmlContent.indexOf('/* THEME-ENGINE:BEGIN */');
    const themeEngineEnd = htmlContent.indexOf('/* THEME-ENGINE:END */');
    assert.ok(themeEngineStart >= 0 && themeEngineEnd > themeEngineStart, 'Theme engine markers not found');
    const themeEngineCode = htmlContent.slice(themeEngineStart, themeEngineEnd);

    const setProps = {};
    const removedProps = new Set();
    const mockRoot = {
      style: {
        setProperty(k, v) { setProps[k] = v; removedProps.delete(k); },
        removeProperty(k) { delete setProps[k]; removedProps.add(k); },
      },
      dataset: {},
    };
    const context = {
      document: { documentElement: mockRoot },
      console: { warn() {} },
    };
    vm.createContext(context);
    vm.runInContext(themeEngineCode, context);

    context.applyTheme({
      '--sub-word-spacing': '6px',
      '--sub-letter-spacing': '2px',
      '--sub-padding': '14px 28px',
      '--sub-radius': '12px',
      '--sub-border': '2px solid #ff0000',
      '--sub-text-transform': 'uppercase',
    });

    assert.equal(setProps['--sub-word-spacing'], '6px');
    assert.equal(setProps['--sub-letter-spacing'], '2px');
    assert.equal(setProps['--sub-user-letter-spacing'], '2px');
    assert.equal(setProps['--sub-padding'], '14px 28px');
    assert.equal(setProps['--sub-user-padding'], '14px 28px');
    assert.equal(setProps['--sub-radius'], '12px');
    assert.equal(setProps['--sub-user-radius'], '12px');
    assert.equal(setProps['--sub-border'], '2px solid #ff0000');
    assert.equal(setProps['--sub-user-border'], '2px solid #ff0000');
    assert.equal(setProps['--sub-text-transform'], 'uppercase');
    assert.equal(setProps['--sub-user-text-transform'], 'uppercase');

    context.applyTheme({});
    assert.ok(removedProps.has('--sub-word-spacing'));
    assert.ok(removedProps.has('--sub-letter-spacing'));
    assert.ok(removedProps.has('--sub-user-letter-spacing'));
    assert.ok(removedProps.has('--sub-user-padding'));
    assert.ok(removedProps.has('--sub-user-radius'));
    assert.ok(removedProps.has('--sub-user-border'));
    assert.ok(removedProps.has('--sub-user-text-transform'));
  });
});

describe('Box animation and time on screen', () => {
  const themeEngine = htmlContent.match(/\/\* THEME-ENGINE:BEGIN \*\/([\s\S]*?)\/\* THEME-ENGINE:END \*\//)[1];
  const engine = () => {
    const context = { document: { documentElement: { style: { setProperty() {}, removeProperty() {} }, dataset: {} } }, console: { warn() {} } };
    vm.createContext(context);
    vm.runInContext(themeEngine, context);
    return context;
  };

  it('reads seconds tokens from a clean theme in ms, null when absent or invalid', () => {
    const { themeSeconds } = engine();
    assert.equal(typeof themeSeconds, 'function');
    assert.equal(themeSeconds({ '--sub-lifetime': '8s' }, '--sub-lifetime'), 8000);
    assert.equal(themeSeconds({ '--sub-lifetime': '1.5s' }, '--sub-lifetime'), 1500);
    assert.equal(themeSeconds({}, '--sub-lifetime'), null);
    assert.equal(themeSeconds({ '--sub-lifetime': '99s' }, '--sub-lifetime'), null);
    assert.equal(themeSeconds({ '--sub-lifetime': '0.2s' }, '--sub-lifetime'), null);
  });

  it('box enter/exit follow --sub-animation and --sub-animation-duration for every style', () => {
    assert.match(htmlContent, /html\[data-anim="fade"\] \.sub-box/);
    assert.match(htmlContent, /html\[data-anim="none"\] \.sub-box/);
    assert.doesNotMatch(htmlContent, /--sub-slide-duration/);
    assert.match(htmlContent, /transition:\s*opacity var\(--sub-animation-duration\)/);
  });

  it('per-line and single expiry use the configured time on screen and exit duration', () => {
    assert.match(htmlContent, /subLifetimeMs = themeSeconds\(data\.tokens, '--sub-lifetime'\) \?\? SUB_LIFETIME_MS/);
    assert.match(htmlContent, /armPerLineExpiry\(sub, subLifetimeMs\)/);
    assert.match(htmlContent, /runExpiry\(sub, safeStyle, false\), subLifetimeMs\)/);
    assert.match(htmlContent, /setTimeout\(cleanupCb, exitCleanupMs\(\)\)/);
  });
});

describe('Whitespace Collapsing, Word Span Splitting, and URL Style Override', () => {
  it('collapses consecutive whitespace using /\\s+/g in safeText normalization', () => {
    assert.match(
      htmlContent,
      /safeText\s*=\s*String\(text\s*\|\|\s*''\)\.replace\(\s*\/\\s\+\/g\s*,\s*' '\s*\)/,
      'safeText must collapse consecutive whitespace with /\\s+/g'
    );
  });

  it('filters empty strings with .filter(Boolean) in karaoke, rgb, and typewriter', () => {
    for (const style of ['karaoke', 'rgb', 'typewriter']) {
      const pattern = new RegExp(`safeStyle\\s*===\\s*'${style}'[\\s\\S]*?const words = safeText\\.split\\(' '\\)\\.filter\\(Boolean\\);`);
      assert.match(
        htmlContent,
        pattern,
        `${style} style must split words using .split(' ').filter(Boolean)`
      );
    }
  });

  it('produces zero empty .word spans when text has consecutive whitespace or leading/trailing spaces', () => {
    const rawAsrText = '   Hello    multispace   ASR   words   stream   ';
    const safeText = String(rawAsrText || '').replace(/\s+/g, ' ').trim().slice(0, 600);
    const words = safeText.split(' ').filter(Boolean);

    assert.deepEqual(words, ['Hello', 'multispace', 'ASR', 'words', 'stream']);
    assert.equal(words.filter((w) => w.length === 0).length, 0);

    const spans = words.map((word) => ({ className: 'word', textContent: word }));
    assert.equal(spans.length, 5);
    for (const span of spans) {
      assert.ok(span.textContent.length > 0, 'Word span must never be empty string');
    }
  });

  it('respects urlStyle over message style when urlStyle is valid', () => {
    assert.match(
      htmlContent,
      /const\s+chosenStyle\s*=\s*\(urlStyle\s*&&\s*VALID_STYLES\.has\(urlStyle\)\)\s*\?\s*urlStyle\s*:\s*style;/,
      'showSubtitle must prioritize valid urlStyle over message style'
    );
  });
});

describe('Animation token overrides preset word transforms', () => {
  // Specificity (ids, classes/attributes, elements) of a simple descendant selector.
  const specificity = (selector) => {
    const ids = (selector.match(/#[\w-]+/g) || []).length;
    const classes = (selector.match(/\.[\w-]+|\[[^\]]+\]/g) || []).length;
    const elements = selector.replace(/\[[^\]]+\]/g, '').split(/\s+/).filter((part) => /^[a-z]/i.test(part)).length;
    return ids * 10000 + classes * 100 + elements;
  };
  const ruleBody = (selector) => {
    const start = htmlContent.indexOf(`${selector} {`);
    assert.ok(start >= 0, `${selector} rule must exist`);
    return htmlContent.slice(start, htmlContent.indexOf('}', start));
  };

  it('fade keeps animating opacity only', () => {
    assert.match(ruleBody('html[data-anim="fade"] .word'), /animation:\s*subFade\b/);
    assert.doesNotMatch(ruleBody('@keyframes subFade'), /transform/);
  });

  for (const anim of ['fade', 'none']) {
    it(`resets karaoke/rgb base transform under data-anim="${anim}" with winning specificity`, () => {
      const selector = `html[data-anim="${anim}"] .word`;
      assert.match(ruleBody(selector), /transform:\s*none\s*;/, `${selector} must reset transform to none`);
      for (const style of ['karaoke', 'rgb']) {
        const preset = `.style-${style} .word`;
        assert.match(ruleBody(preset), /transform:/, `${preset} declares a base transform`);
        assert.ok(specificity(selector) > specificity(preset), `${selector} must out-rank ${preset}`);
      }
    });
  }
});
