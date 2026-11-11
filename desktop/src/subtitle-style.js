// SPDX-License-Identifier: MIT

/**
 * Subtitle style helpers for the Style tab.
 *
 * The font picker lists the installed families the backend reports (Windows, DirectWrite).
 * These curated lists are the fallback when it reports none. Names must stay inside the
 * font charset that the Rust and overlay validators accept.
 */
const WINDOWS_FONT_FAMILIES = Object.freeze([
  'Segoe UI',
  'Arial',
  'Verdana',
  'Tahoma',
  'Calibri',
  'Trebuchet MS',
  'Georgia',
  'Times New Roman',
  'Courier New',
  'Consolas',
  'Impact',
]);

const LINUX_FONT_FAMILIES = Object.freeze([
  'DejaVu Sans',
  'Liberation Sans',
  'Ubuntu',
  'Helvetica',
  'Arial',
  'Verdana',
  'Georgia',
  'Times New Roman',
  'Courier New',
]);

export const SUBTITLE_FONT_FAMILIES = Object.freeze([...new Set([...WINDOWS_FONT_FAMILIES, ...LINUX_FONT_FAMILIES])]);

/** Families to offer on one platform. An unknown platform gets the full list. */
export function fontFamiliesForPlatform(platform = '') {
  if (/windows|^win/i.test(platform)) return WINDOWS_FONT_FAMILIES;
  if (/linux/i.test(platform)) return LINUX_FONT_FAMILIES;
  return SUBTITLE_FONT_FAMILIES;
}

/** Font choices: installed families when known, else the platform list; a saved font is always kept. */
export function fontChoices(installed, platform, saved) {
  const base = installed.length > 0 ? installed : platform;
  return !saved || base.includes(saved) ? base : [...base, saved];
}

const FONT_NAME = /^[A-Za-z0-9 -]{1,200}$/;

export const MAX_SUBTITLE_PRESETS = 20;
export const MAX_PRESET_NAME_CHARS = 40;

// Control defaults mirror the overlay's :root tokens. A control at its default is not
// written to the theme, so the overlay keeps its own value. A null background means no
// custom background: each preset keeps its own background.
export const SUBTITLE_STYLE_DEFAULTS = Object.freeze({
  fontFamily: 'Segoe UI',
  fontSize: 42,
  fontWeight: '900',
  color: '#ffffff',
  background: null,
  backgroundOpacity: 0.7,
  animation: 'pop',
  animationDuration: 0.3,
  lifetime: 5, // s on screen
  letterSpacing: 0, // px; 0 is the token's `normal`
  wordSpacing: 0, // px; 0 is the token's `normal`
});

// Slider range in px. Inside the Rust clamp (-5..20) and the overlay's spacing check.
export const SUBTITLE_LETTER_SPACING = Object.freeze({ min: -2, max: 10, step: 0.5 });
export const SUBTITLE_WORD_SPACING = Object.freeze({ min: -2, max: 20, step: 0.5 });

export const SUBTITLE_LIFETIME = Object.freeze({ min: 1.5, max: 15, step: 0.5 });

export const SUBTITLE_WEIGHT_OPTIONS = Object.freeze(['400', '700', '900']);
export const SUBTITLE_ANIMATION_OPTIONS = Object.freeze(['pop', 'fade', 'none']);

// Tokens the Style tab edits. Any other token in the theme is kept as it is.
const CONTROLLED_TOKENS = Object.freeze([
  '--sub-font-family',
  '--sub-font-size',
  '--sub-font-weight',
  '--sub-color',
  '--sub-bg',
  '--sub-animation',
  '--sub-animation-duration',
  '--sub-lifetime',
  '--sub-letter-spacing',
  '--sub-word-spacing',
]);

const HEX_COLOR = /^#[0-9a-f]{6}$/i;

const clamp = (value, min, max) => Math.min(max, Math.max(min, value));

function parseUnit(value, unit) {
  if (typeof value !== 'string') return null;
  const match = value.match(new RegExp(`^(\\d+(?:\\.\\d+)?)${unit}$`));
  return match ? Number(match[1]) : null;
}

/** Reads #rgb, #rgba, #rrggbb, #rrggbbaa, rgb() and rgba() into a hex color and an alpha. */
function parseColor(value) {
  if (typeof value !== 'string') return null;
  const text = value.trim().toLowerCase();
  const hexMatch = text.match(/^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/);
  if (hexMatch) {
    let digits = hexMatch[1];
    if (digits.length <= 4) digits = [...digits].map((c) => c + c).join('');
    const alpha = digits.length === 8 ? parseInt(digits.slice(6, 8), 16) / 255 : 1;
    return { hex: `#${digits.slice(0, 6)}`, alpha };
  }
  const rgbMatch = text.match(/^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*(\d*\.?\d+)\s*)?\)$/);
  if (!rgbMatch) return null;
  const channels = rgbMatch.slice(1, 4).map(Number);
  const alpha = rgbMatch[4] === undefined ? 1 : Number(rgbMatch[4]);
  if (channels.some((channel) => channel > 255) || alpha > 1) return null;
  const hex = `#${channels.map((channel) => channel.toString(16).padStart(2, '0')).join('')}`;
  return { hex, alpha };
}

/**
 * Maps the Style tab controls to the CSS tokens sent to OBS. Controls at their default are
 * not written. A user-set background is always written, with its opacity. Tokens in
 * `currentTokens` that this tab does not edit are kept.
 */
export function themeFromControls(controls, currentTokens = {}) {
  const defaults = SUBTITLE_STYLE_DEFAULTS;
  const values = { ...defaults, ...controls };
  const theme = {};
  for (const [name, value] of Object.entries(currentTokens ?? {})) {
    if (!CONTROLLED_TOKENS.includes(name)) theme[name] = value;
  }
  if (values.fontFamily !== defaults.fontFamily && FONT_NAME.test(values.fontFamily)) {
    theme['--sub-font-family'] = values.fontFamily;
  }
  const size = clamp(Math.round(Number(values.fontSize)), 12, 120);
  if (Number.isFinite(size) && size !== defaults.fontSize) theme['--sub-font-size'] = `${size}px`;
  if (values.fontWeight !== defaults.fontWeight && SUBTITLE_WEIGHT_OPTIONS.includes(values.fontWeight)) {
    theme['--sub-font-weight'] = values.fontWeight;
  }
  if (HEX_COLOR.test(values.color) && values.color.toLowerCase() !== defaults.color) {
    theme['--sub-color'] = values.color.toLowerCase();
  }
  if (HEX_COLOR.test(values.background ?? '')) {
    const opacity = Math.round(clamp(Number(values.backgroundOpacity), 0, 1) * 100) / 100;
    const [red, green, blue] = [1, 3, 5].map((offset) => parseInt(values.background.slice(offset, offset + 2), 16));
    const alpha = Number.isFinite(opacity) ? opacity : defaults.backgroundOpacity;
    theme['--sub-bg'] = `rgba(${red}, ${green}, ${blue}, ${alpha})`;
  }
  if (SUBTITLE_ANIMATION_OPTIONS.includes(values.animation) && values.animation !== defaults.animation) {
    theme['--sub-animation'] = values.animation;
  }
  const duration = clamp(Math.round(Number(values.animationDuration) * 10) / 10, 0.1, 2.0);
  if (Number.isFinite(duration) && duration !== defaults.animationDuration) {
    theme['--sub-animation-duration'] = `${duration}s`;
  }
  const life = clamp(Math.round(Number(values.lifetime) / SUBTITLE_LIFETIME.step) * SUBTITLE_LIFETIME.step, SUBTITLE_LIFETIME.min, SUBTITLE_LIFETIME.max);
  if (Number.isFinite(life) && life !== defaults.lifetime) theme['--sub-lifetime'] = `${life}s`;
  const { min, max, step } = SUBTITLE_LETTER_SPACING;
  const spacing = clamp(Math.round(Number(values.letterSpacing) / step) * step, min, max);
  if (Number.isFinite(spacing) && spacing !== defaults.letterSpacing) theme['--sub-letter-spacing'] = `${spacing}px`;
  const wordRange = SUBTITLE_WORD_SPACING;
  const wordSpacing = clamp(Math.round(Number(values.wordSpacing) / wordRange.step) * wordRange.step, wordRange.min, wordRange.max);
  if (Number.isFinite(wordSpacing) && wordSpacing !== defaults.wordSpacing) theme['--sub-word-spacing'] = `${wordSpacing}px`;
  return theme;
}

/** Reads CSS tokens back into control values; anything missing or invalid uses the default. */
export function controlsFromTheme(tokens = {}) {
  const defaults = SUBTITLE_STYLE_DEFAULTS;
  const pick = (value, allowed, fallback) => (allowed.includes(value) ? value : fallback);
  const hex = (value) => (HEX_COLOR.test(value ?? '') ? value.toLowerCase() : null);
  const background = parseColor(tokens['--sub-bg']);
  return {
    fontFamily: FONT_NAME.test(tokens['--sub-font-family'] ?? '') ? tokens['--sub-font-family'] : defaults.fontFamily,
    fontSize: parseUnit(tokens['--sub-font-size'], 'px') ?? defaults.fontSize,
    fontWeight: pick(tokens['--sub-font-weight'], SUBTITLE_WEIGHT_OPTIONS, defaults.fontWeight),
    color: hex(tokens['--sub-color']) ?? defaults.color,
    background: background ? background.hex : defaults.background,
    backgroundOpacity: background ? background.alpha : defaults.backgroundOpacity,
    animation: pick(tokens['--sub-animation'], SUBTITLE_ANIMATION_OPTIONS, defaults.animation),
    animationDuration: parseUnit(tokens['--sub-animation-duration'], 's') ?? defaults.animationDuration,
    lifetime: clamp(parseUnit(tokens['--sub-lifetime'], 's') ?? defaults.lifetime, SUBTITLE_LIFETIME.min, SUBTITLE_LIFETIME.max),
    letterSpacing: parseLetterSpacing(tokens['--sub-letter-spacing']) ?? defaults.letterSpacing,
    wordSpacing: parseWordSpacing(tokens['--sub-word-spacing']) ?? defaults.wordSpacing,
  };
}

/** Reads a px spacing token into the slider range; `normal` and anything unreadable give null. */
function parseLetterSpacing(value) {
  const match = typeof value === 'string' ? value.trim().match(/^(-?\d+(?:\.\d+)?)px$/) : null;
  return match ? clamp(Number(match[1]), SUBTITLE_LETTER_SPACING.min, SUBTITLE_LETTER_SPACING.max) : null;
}

/** Reads a px word spacing token into the slider range; `normal` and anything unreadable give null. */
function parseWordSpacing(value) {
  const match = typeof value === 'string' ? value.trim().match(/^(-?\d+(?:\.\d+)?)px$/) : null;
  return match ? clamp(Number(match[1]), SUBTITLE_WORD_SPACING.min, SUBTITLE_WORD_SPACING.max) : null;
}

/** Saves a named preset with its base style (when given); an existing name is replaced. Errors: 'name' or 'limit'. */
export function savePreset(presets, rawName, tokens, style) {
  const name = String(rawName ?? '').trim();
  if (name.length === 0 || [...name].length > MAX_PRESET_NAME_CHARS) return { presets, error: 'name' };
  const index = presets.findIndex((preset) => preset.name === name);
  if (index === -1 && presets.length >= MAX_SUBTITLE_PRESETS) return { presets, error: 'limit' };
  const entry = { name, ...(style ? { style } : {}), tokens: { ...tokens } };
  const next = index === -1
    ? [...presets, entry]
    : presets.map((preset, position) => (position === index ? entry : preset));
  return { presets: next };
}

export function deletePreset(presets, name) {
  return presets.filter((preset) => preset.name !== name);
}

// Loading a preset replaces the whole current theme with its tokens, so tokens the
// controls do not edit come back and tokens from the previous theme do not carry over.
// `style` is its built-in base style, or null for a preset saved without one.
export function loadPresetTheme(preset) {
  const theme = { ...preset.tokens };
  return { theme, controls: controlsFromTheme(theme), style: preset.style ?? null };
}

// Saved styles sit in the style selector next to the built-in ones, told apart by a prefix.
const PRESET_OPTION_PREFIX = 'preset:';
export const presetOptionValue = (name) => `${PRESET_OPTION_PREFIX}${name}`;
export const presetNameFromOption = (value) => (String(value).startsWith(PRESET_OPTION_PREFIX) ? String(value).slice(PRESET_OPTION_PREFIX.length) : null);

// Preview Play: per-word reveal delays use the overlay's stagger (max ms per word, 400 ms budget).
const WORD_STAGGER_MAX_MS = Object.freeze({ karaoke: 25, rgb: 50, typewriter: 40 });
const REVEAL_BUDGET_MS = 400;
export function previewWordDelays(style, count) {
  const max = WORD_STAGGER_MAX_MS[style];
  if (!max) return [];
  const step = Math.min(max, REVEAL_BUDGET_MS / Math.max(count - 1, 1));
  return Array.from({ length: count }, (_, index) => index * step);
}

/** How long Play holds the line: the time on screen, capped so a preview never drags. */
export const previewHoldMs = (lifetimeSeconds) => Math.min(lifetimeSeconds * 1000, 3000);

// The preview applies a theme to its container the way the overlay's applyTheme applies it to
// <html>, so the shared overlay CSS (desktop/src/overlay-preview.css) resolves identically.
// tests-js/test_preview_sync.mjs runs the overlay's own applyTheme and compares the results.
const OVERLAY_TOKENS = Object.freeze([
  '--sub-bg', '--sub-color', '--sub-font-size', '--sub-font-weight', '--sub-radius', '--sub-shadow',
  '--sub-border', '--sub-padding', '--sub-animation', '--sub-animation-duration', '--sub-lifetime', '--sub-font-family',
  '--sub-text-transform', '--sub-letter-spacing', '--sub-word-spacing',
]);

// Presets with their own properties read these `--sub-user-*` variables, so a user choice wins.
const USER_OVERRIDES = Object.freeze({
  '--sub-user-color': '--sub-color',
  '--sub-user-bg': '--sub-bg',
  '--sub-user-letter-spacing': '--sub-letter-spacing',
  '--sub-user-padding': '--sub-padding',
  '--sub-user-radius': '--sub-radius',
  '--sub-user-border': '--sub-border',
  '--sub-user-text-transform': '--sub-text-transform',
});

/** Custom properties, `data-anim` and `data-font` the preview container needs for a theme. */
export function previewProperties(tokens = {}) {
  const has = (name) => OVERLAY_TOKENS.includes(name) && typeof tokens?.[name] === 'string';
  const props = {};
  for (const name of OVERLAY_TOKENS) if (has(name)) props[name] = tokens[name].trim();
  for (const [userName, source] of Object.entries(USER_OVERRIDES)) if (has(source)) props[userName] = props[source];
  return { props, anim: props['--sub-animation'] || 'pop', font: has('--sub-font-family') ? 'custom' : 'default' };
}
