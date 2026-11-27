// A figure's default controls, as HTML: a play button and a time slider, beat stepping with its
// caption, a control for each param a handle doesn't set, and the readouts. One function writes
// them, for a build, which writes them into the page as a declarative shadow root of
// `<pencil-controls>` before any script runs, and for `<pencil-figure controls>` on a page that gave
// none. Nothing here touches the DOM, so a build can call it.
//
// Nothing moves when a value changes. An `<output>` is as wide as the widest value its param's
// range can show, a readout's box is as big as the most it says at the moments a build tries, every
// caption lies in one place, so the row is as tall as the longest of them, and the play button is
// as wide as the wider of its words. On a phone one row stays, `BAR` says which, and the rest open
// from a menu over the page, so the figure keeps the height. That row, too, goes behind the menu if
// it doesn't fit (the element checks), so nothing in the bar is ever cut off.
//
// Every control is marked with what it sets (`data-param`, `data-time`, `data-play`, `data-step`),
// and each output with what it shows (`data-param`, `data-time`, `data-beat`, `data-readout`), as a
// page's own controls are. The element wires them the same way either way.
import { FAMILY } from './labels.js';

const escape = (text) => String(text).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const attrs = (pairs) =>
  Object.entries(pairs)
    .filter(([, value]) => value !== undefined && value !== null && value !== false)
    .map(([name, value]) => (value === true ? ` ${name}` : ` ${name}="${escape(value)}"`))
    .join('');

/** How long time runs before it loops, for a figure that declares no `duration`, as the element has it. */
const LOOP = 10;

/**
 * Which row a phone's bar keeps, of the controls' one line, with the rest behind its menu: `'first'`,
 * the first of them (time, beats, or the first param's control), or `'readouts'`, the readouts when
 * a figure has any, and the first row when it has none.
 */
export const BAR = 'first';

/** When the controls are a phone's: one row, and a menu. The styles below say the same. */
export const PHONE = '(max-width: 40rem), (max-height: 32rem)';

/** As many decimals as a param's step has, so a whole-number param reads as one. */
export const decimals = (param) => (param.step == null ? 2 : (String(param.step).split('.')[1] ?? '').length);

/** A param's value as its control says it: a choice's option, or the number. One that doesn't exist
 * this frame is a dash. */
export function valueText(param, value) {
  if (!Number.isFinite(value)) return '—';
  if (param.choice) return param.choice.options[Math.round(value)] ?? '—';
  return value.toFixed(decimals(param)).replace('-', '−');
}

export const timeText = (seconds) => `${seconds.toFixed(1)} s`;

/** Whether a handle sets this param. Its handle is its control: it takes focus and the arrow
 * keys, and says its value to a screen reader, so the param gets no control of its own. */
export const handled = (info, param) => (info.handles ?? []).some((handle) => handle.params.includes(param.id));

/** Whether the figure has beats to step through. */
export const steps = (info) => (info.beats?.length ?? 0) > 0 && info.params.some((p) => p.id === 'beat');

/** The params that get a control: each one a handle doesn't set, and not `beat` when it's stepped. */
export const controlled = (info) => info.params.filter((p) => !(p.id === 'beat' && steps(info)) && !handled(info, p));

/** Whether a figure has anything for its controls to show: time, beats, a param, or a readout. */
export const hasControls = (info) =>
  !!info && (info.usesTime || steps(info) || controlled(info).length > 0 || (info.readouts?.length ?? 0) > 0);

/** The param a control or an output is for: `data-param` names it, or for a choice, its value. */
export const paramNamed = (info, name) => info?.params.find((p) => p.id === name || p.choice?.of === name);

/** Beats with lengths count the beats done, from 0 to n. Ones without say which beat it's on. */
const timed = (info) => (info.beatLengths?.length ?? 0) > 0;

/** Where a figure is in its beats: what the counter says, and which caption shows. */
export function beatAt(info, beat) {
  const n = info.beats.length;
  const at = Number.isFinite(beat) ? beat : 0;
  const shown = timed(info) ? at : at + 1;
  const whole = Math.abs(shown - Math.round(shown)) < 1e-6;
  return {
    text: `${whole ? Math.round(shown) : shown.toFixed(1)} / ${n}`,
    caption: timed(info) ? Math.min(n - 1, Math.max(0, Math.ceil(at) - 1)) : Math.round(at),
  };
}

/** What a param's control says it is, as its sheets' `text` picks: its caption, or its name, as
 * parts joined by ` = `, each `{ text, math }`. Its value isn't one of them: it's said after the
 * control. A control with no caption says its name, which is mathematics. */
export function controlParts(param) {
  const name = param.control?.name ?? param.choice?.of ?? param.id;
  const said = [];
  for (const part of param.control?.text ?? ['caption', 'value']) {
    const piece =
      part === 'caption' ? (param.caption ? { text: param.caption, math: false } : { text: name, math: true })
      : part === 'name' || part === 'expr' ? { text: name, math: true }
      : null;
    if (piece && !said.some((each) => each.text === piece.text)) said.push(piece);
  }
  return said;
}

/** What a control is called, for a screen reader: what it says, or its caption or name when it
 * says only its value. */
export function controlName(param) {
  const parts = controlParts(param);
  if (parts.length) return parts.map((part) => part.text).join(' = ');
  return param.caption ?? param.control?.name ?? param.choice?.of ?? param.id;
}

/** Whether a control says its value after it: a slider does unless its sheets leave `value` out. */
export const saysValue = (param) => (param.control?.text ?? ['caption', 'value']).includes('value');

/** Which control a param gets, as the core works it out from the figure and its sheets. */
export const kindOf = (param) => param.control?.kind ?? (param.choice ? 'select' : 'slider');

const labelHtml = (param) =>
  controlParts(param)
    .map((part) => (part.math ? `<var>${escape(part.text)}</var>` : escape(part.text)))
    .join(' = ');

/** The room an output needs: as many digits wide as the longest thing it will say. With
 * tabular figures a digit is a `ch`, and a sign, a point and a space are no wider. */
const room = (...texts) => `--room: ${Math.max(...texts.map((text) => String(text).length))}ch`;

/**
 * What kind of controls a figure has, as a short string. Controls made for one figure fit another
 * with the same one, so the element keeps what a build wrote and only turns it on.
 */
export function shape(info) {
  const text = JSON.stringify([
    info.usesTime,
    info.duration,
    info.beats,
    timed(info),
    info.params.map((p) => [p.id, p.min, p.max, p.step, p.choice, handled(info, p), p.caption, p.control]),
    (info.readouts ?? []).map((r) => r.id),
  ]);
  let hash = 5381;
  for (let i = 0; i < text.length; i++) hash = ((hash * 33) ^ text.charCodeAt(i)) >>> 0;
  return hash.toString(36);
}

const px = (v) => `${Math.round(v * 1e4) / 1e4}em`;

/**
 * What a readout says, as HTML to put in its box: what the core laid out, each run and rule placed
 * in ems from the left of its line and its middle, as a label over the figure is, in the same
 * fonts. Every style is inline, so it reads the same in a page's own box as in the defaults. Its
 * words are there for a screen reader, and the runs aren't.
 *
 * `readout` is one of the element's `readouts`, or of what a build says of the poster's moment:
 * `{ id, text, words, math, width, height, laid }`.
 */
export function readoutHtml(readout) {
  if (!readout) return '';
  const box = `position:relative;display:inline-block;vertical-align:middle;width:${px(readout.width)};height:${px(readout.height)}`;
  const run = `position:absolute;white-space:pre;line-height:1;font-kerning:none;font-variant-ligatures:none;font-feature-settings:'calt' 0`;
  const sans = `font-family:${FAMILY.sans};font-style:normal`;
  const italic = `font-family:${FAMILY.math};font-style:italic`;
  const formula = `font-family:'Pencil Math', ${FAMILY.math};font-style:normal`;
  const said = `<span style="position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap">${escape(readout.words)}</span>`;
  let drawn;
  if (!readout.laid) {
    drawn = `<span style="${run};left:0;top:calc(50% - 0.5em);font-size:1em;${readout.math ? italic : sans}">${escape(readout.words)}</span>`;
  } else {
    const words = readout.math ? italic : sans;
    drawn = readout.laid.runs
      .map((r) => {
        const stretch = r.stretch !== 1 ? `;transform:scaleY(${r.stretch});transform-origin:0 ${(0.5 + 0.324) * 100}%` : '';
        return `<span style="${run};left:${px(r.x)};top:calc(50% + ${px(r.top)});font-size:${px(r.size)};${r.math ? formula : words}${stretch}">${escape(r.text)}</span>`;
      })
      .concat(
        readout.laid.rules.map(
          (r) => `<span style="position:absolute;left:${px(r.x)};top:calc(50% + ${px(r.y)});width:${px(r.width)};height:${px(r.height)};background:currentColor"></span>`,
        ),
      )
      .join('');
  }
  return `<span aria-hidden="true" style="${box}">${drawn}</span>${said}`;
}

/** The defaults' styles, in their shadow root. A page styles them through `::part` and these custom
 * properties, which inherit into it:
 *
 * - `--pencil-controls-font`: their text, as the `font` shorthand.
 * - `--pencil-ink`: values, and a name, which is mathematics. `--pencil-muted`: what controls say
 *   they are, and captions.
 * - `--pencil-accent`: sliders, radios and checkboxes. `--pencil-rule` and `--pencil-surface`: a
 *   button's border and background, and the menu's on a phone.
 * - `--pencil-math-font`: a param's name, as the `font` shorthand.
 * - `--pencil-focus`: the focus ring, as on the figure's handles.
 * - `--pencil-readout-size`: how big a readout's text is.
 */
export const CONTROLS_CSS = `
:host { display: block; }
:host([hidden]) { display: none; }
/* One grid, so every slider starts and ends where the others do: what a control says, the control,
   and its value. What a control says wraps past two fifths of the width, so a long caption doesn't
   squeeze its slider. A row of anything else takes the whole width. */
.controls, .rest { display: grid; grid-template-columns: fit-content(40%) minmax(0, 1fr) max-content; gap: 0.375rem 0.75rem; align-items: center; }
.controls { margin-top: 0.625rem; font: var(--pencil-controls-font, 0.875rem/1.4 ui-sans-serif, system-ui, sans-serif); color: var(--pencil-muted, color-mix(in srgb, currentColor 68%, transparent)); }
.bar, .rest { display: contents; }
.rest { position: static; inset: auto; margin: 0; padding: 0; border: 0; width: auto; height: auto; max-width: none; max-height: none; overflow: visible; background: none; color: inherit; }
.more, .more-words, .row.phone { display: none; }
.row { grid-column: 1 / -1; display: flex; align-items: center; gap: 0.5rem 0.75rem; flex-wrap: wrap; margin: 0; padding: 0; border: 0; min-width: 0; }
.row.param, .row.time { display: contents; }
/* The readouts come after the controls, wherever the bar's switch puts them, and what a page put
   in after them. */
.row.readouts { order: 1; }
slot { display: contents; }
::slotted(*) { grid-column: 1 / -1; order: 2; }
fieldset.row { min-inline-size: 0; }
legend { float: left; padding: 0; margin-right: 0.25rem; }
label { display: inline-flex; align-items: center; gap: 0.375rem; }
.option { gap: 0.25rem; }
input[type='range'] { width: 100%; min-width: 4rem; flex: 1 1 8rem; height: 1.25rem; margin: 0; contain: size layout style; accent-color: var(--pencil-accent, auto); }
input[type='radio'], input[type='checkbox'] { margin: 0; accent-color: var(--pencil-accent, auto); }
select { justify-self: start; }
output { display: inline-block; flex: none; justify-self: end; width: var(--room, 3.5rem); text-align: right; white-space: nowrap; overflow: hidden; font-variant-numeric: tabular-nums; color: var(--pencil-ink, currentColor); contain: layout paint style; }
.beats output { text-align: center; }
var { font: var(--pencil-math-font, italic 1.07em Charter, 'Bitstream Charter', 'Sitka Text', Cambria, Georgia, serif); color: var(--pencil-ink, currentColor); }
/* Every caption lies in the same place and only the beat's own shows, so the row is as tall as the
   longest from the start and stepping moves nothing below it. */
.captions { flex-basis: 100%; display: grid; font-style: italic; min-width: 0; }
.caption { grid-area: 1 / 1; visibility: hidden; }
.caption[data-on] { visibility: visible; }
.readouts { gap: 0.25rem 1.25rem; -webkit-user-select: text; user-select: text; }
.readout { display: inline-block; width: calc(var(--width, 4) * 1em); height: calc(var(--height, 1.4) * 1em); flex: none; text-align: left; font-size: var(--pencil-readout-size, 1.0625rem); line-height: 1; color: var(--pencil-ink, currentColor); contain: strict; overflow: clip; }
button, select { font: inherit; color: var(--pencil-ink, currentColor); background: var(--pencil-surface, transparent); border: 1px solid var(--pencil-rule, color-mix(in srgb, currentColor 25%, transparent)); border-radius: 4px; padding: 0.25rem 0.625rem; cursor: pointer; }
button:disabled { opacity: 0.4; cursor: default; }
input:disabled, select:disabled { cursor: default; }
/* The play button is as wide as the wider of its words, so it doesn't move what's beside it. */
.play { display: inline-grid; flex: none; justify-self: start; }
.play > span { grid-area: 1 / 1; }
.play > span:not([data-on]) { visibility: hidden; }
:focus-visible { outline: 2px solid var(--pencil-focus, currentColor); outline-offset: 2px; }
input[type='range']:focus-visible { outline-offset: 1px; }
.said { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
/* On a phone, or a screen with little height, one row stays, on one line, in a bar of fixed height,
   and the rest open from a menu over the page, as a sheet from the bottom: the figure keeps the
   height. Each thing in the bar keeps its own width, so the element can tell when the row doesn't
   fit, and then it goes behind the menu too. The beats' caption is a line of its own under the bar,
   as tall as the longest caption at that width, wherever the stepper is. */
@media (max-width: 40rem), (max-height: 32rem) {
  .controls { display: block; }
  .bar { display: flex; align-items: center; gap: 0.5rem; height: 2.25rem; min-width: 0; }
  .bar > .row, .bar > .row.param, .bar > .row.time { display: flex; flex: 1 1 auto; min-width: 0; flex-wrap: nowrap; overflow: hidden; gap: 0.5rem 0.75rem; }
  .bar > .row > *, .bar > .row label { flex: none; white-space: nowrap; }
  .bar > .row > input[type='range'] { flex: 1 1 4rem; min-width: 4rem; }
  .beats .captions { display: none; }
  .row.phone { display: flex; margin-top: 0.25rem; }
  .row.phone .caption { white-space: normal; }
  .controls[data-more] .more { display: inline-flex; align-items: center; gap: 0.375rem; flex: none; margin-left: auto; padding: 0.25rem 0.5rem; line-height: 1; }
  .bar[data-crowded] .more-words { display: inline; }
  .rest { display: grid; }
  .rest[popover]:not(:popover-open) { display: none; }
  .rest:popover-open { position: fixed; inset: auto 0 0 0; margin: 0; width: auto; max-height: 70vh; overflow: auto; padding: 1rem 1rem 1.25rem; row-gap: 0.75rem; background: var(--pencil-surface-menu, Canvas); color: var(--pencil-muted, CanvasText); border-top: 1px solid var(--pencil-rule, color-mix(in srgb, currentColor 25%, transparent)); box-shadow: 0 -0.5rem 1.5rem rgb(0 0 0 / 0.15); font: var(--pencil-controls-font, 0.875rem/1.4 ui-sans-serif, system-ui, sans-serif); }
}
`;

/** The rows, in order: time, beats, each param's control, then the readouts. */
function rows({ info, moment, paused, disabled }) {
  const { params = [], beats = [], usesTime } = info;
  const { time = 0, values = {}, ranges = {}, readouts = [] } = moment ?? {};
  const out = [];

  if (usesTime) {
    const duration = info.duration ?? LOOP;
    out.push(
      `<div class="row time" part="row time">` +
        `<button${attrs({ type: 'button', class: 'play', part: 'button play', 'data-play': true, disabled })}>` +
        `<span${attrs({ 'data-on': paused })}>Play</span><span${attrs({ 'data-on': !paused })}>Pause</span></button>` +
        `<input${attrs({ type: 'range', id: 'time', part: 'slider time', 'data-time': true, min: 0, max: duration, step: 0.001, value: time, 'aria-label': 'Time', 'aria-valuetext': timeText(time), disabled })}>` +
        `<output${attrs({ for: 'time', part: 'value time', 'data-time': true, 'aria-live': 'off', style: room(timeText(duration)) })}>${timeText(time)}</output>` +
        `</div>`,
    );
  }

  if (steps(info)) {
    const range = params.find((p) => p.id === 'beat');
    const beat = Number.isFinite(values.beat) ? values.beat : 0;
    const at = beatAt(info, beat);
    // The widest it says is a beat part done, as `9.5 / 10`, when beats have lengths.
    const widest = timed(info) ? `${beats.length}.5 / ${beats.length}` : `${beats.length} / ${beats.length}`;
    const captions = beats
      .map((caption, i) => `<span${attrs({ class: 'caption', part: 'caption', 'data-on': i === at.caption })}>${escape(caption ?? '')}</span>`)
      .join('');
    out.push(
      `<div class="row beats" part="row beats">` +
        `<button${attrs({ type: 'button', part: 'button step', 'data-step': -1, title: 'Previous beat', 'aria-label': 'Previous beat', disabled: disabled || beat <= range.min })}>←</button>` +
        `<output${attrs({ part: 'value beat', 'data-beat': true, 'aria-live': 'off', style: room(widest) })}>${at.text}</output>` +
        `<button${attrs({ type: 'button', part: 'button step', 'data-step': 1, title: 'Next beat', 'aria-label': 'Next beat', disabled: disabled || beat >= range.max })}>→</button>` +
        `<span class="captions" part="captions" data-captions>${captions}</span>` +
        `</div>`,
    );
  }

  for (const param of controlled(info)) {
    const id = `param-${param.id}`;
    const name = param.control?.name ?? param.choice?.of ?? param.id;
    const value = values[param.id];
    const index = Math.round(value);
    const kind = kindOf(param);
    const label = labelHtml(param);
    const called = controlName(param);
    if (kind === 'radios') {
      const options = param.choice.options
        .map(
          (text, i) =>
            `<label class="option" part="option"><input${attrs({ type: 'radio', part: 'radio', name: id, 'data-param': name, value: i, checked: i === index, disabled })}>${escape(text)}</label>`,
        )
        .join('');
      out.push(
        `<fieldset class="row choice" part="row choice">` +
          `<legend${attrs({ part: 'label', class: label ? undefined : 'said' })}>${label || escape(called)}</legend>${options}</fieldset>`,
      );
      continue;
    }
    if (kind === 'checkbox') {
      out.push(
        `<div class="row choice" part="row choice"><label part="label">` +
          `<input${attrs({ type: 'checkbox', id, part: 'checkbox', 'data-param': name, checked: index === 1, 'aria-label': label ? undefined : called, disabled })}>${label}</label></div>`,
      );
      continue;
    }
    let input;
    if (kind === 'select') {
      const options = param.choice.options
        .map((text, i) => `<option${attrs({ value: i, selected: i === index })}>${escape(text)}</option>`)
        .join('');
      input = `<select${attrs({ id, part: 'select', 'data-param': name, 'aria-label': label ? undefined : called, disabled })}>${options}</select>`;
    } else {
      const [min, max] = ranges[param.id] ?? [param.min, param.max];
      const step = param.step ?? ((param.max - param.min) / 1000 || 0.001);
      input = `<input${attrs({
        type: 'range',
        id,
        part: 'slider',
        'data-param': name,
        min,
        max,
        step,
        value: Number.isFinite(value) ? value : undefined,
        'aria-label': label ? undefined : called,
        'aria-valuetext': valueText(param, value),
        disabled,
      })}>`;
    }
    // A choice says itself, so only a slider says its value after it.
    const widest = param.choice ? param.choice.options : [valueText(param, param.min), valueText(param, param.max)];
    const out_ =
      kind === 'slider' && saysValue(param)
        ? `<output${attrs({ for: id, part: 'value', 'data-param': name, 'aria-live': 'off', style: room(...widest) })}>${escape(valueText(param, value))}</output>`
        : '';
    // Three cells, always, so every row keeps to the grid's columns.
    const said = label ? `<label${attrs({ for: id, part: 'label', class: 'param' })}>${label}</label>` : '<span></span>';
    out.push(`<div class="row param" part="row param">${said}${input}${out_ || '<span></span>'}</div>`);
  }

  const said = info.readouts ?? [];
  if (said.length) {
    out.readouts = out.length;
    const boxes = said
      .map((readout, i) => {
        const [width, height] = readout.room ?? [readouts[i]?.width ?? 4, readouts[i]?.height ?? 1.4];
        return `<output${attrs({ class: 'readout', part: 'readout', 'data-readout': readout.id, 'aria-live': 'off', style: `--width: ${width}; --height: ${height}` })}>${readoutHtml(readouts[i])}</output>`;
      })
      .join('');
    out.push(`<div class="row readouts" part="row readouts">${boxes}</div>`);
  }
  return out;
}

/**
 * The controls for a figure, as the inside of their shadow root, after its styles.
 *
 * - `info`: what the figure says of itself, with each readout's `room` from a build or the element.
 * - `moment`: where it stands: `{ time, values, ranges, readouts }`, the first three by param, and
 *   what each readout says.
 * - `paused`: whether it's paused, for the play button.
 * - `disabled`: the figure isn't ready, so nothing can be worked yet.
 * - `extra`: the page puts something of its own in `<pencil-controls>`, as a choice of looks,
 *   which comes after the defaults, and on a phone in their menu.
 */
export function markup({ info, moment, paused = false, disabled = false, extra = false }) {
  const all = rows({ info, moment, paused, disabled });
  const at = BAR === 'readouts' && all.readouts !== undefined ? all.readouts : 0;
  const first = all[at];
  const rest = all.filter((_, i) => i !== at);
  // On a phone the beats' caption is a line of its own with the bar, wherever the stepper is: it's
  // the figure's own words. Every caption lies in its one place, so it's as tall as the longest.
  const beats = all.find((row) => row.startsWith('<div class="row beats"'));
  const captions = beats
    ? `<div class="row captions-row phone" part="row captions">${beats.slice(beats.indexOf('<span class="captions"'), beats.lastIndexOf('</div>'))}</div>`
    : '';
  // Where it is in the beats and what the caption says, said when a beat changes, wherever the
  // stepper is, so Next beat is heard.
  const said = beats ? '<span class="said" data-said role="status" aria-live="polite"></span>' : '';
  // On a phone the button opens the menu, and shows whenever anything is in it: the rows that aren't
  // the bar's, what the page put in, or the bar's own row when it doesn't fit.
  const more =
    `<button${attrs({ type: 'button', class: 'more', part: 'button more', popovertarget: 'rest', 'aria-label': 'More controls', title: 'More controls' })}>` +
    `<span class="more-words">Controls</span> ⋯</button>`;
  const menu = rest.length > 0 || extra;
  return (
    `<div${attrs({ class: 'controls', part: 'controls', 'data-more': first && menu, 'data-menu': first && menu })}>` +
    (first ? `<div class="bar" part="bar">${first}${more}</div>` : '') +
    captions +
    `<div class="rest" id="rest" part="rest"${first ? ' popover' : ''}>${rest.join('')}<slot></slot></div>` +
    said +
    `</div>`
  );
}

/** The controls as a declarative shadow root, for a build to write inside `<pencil-controls>`, so
 * they have their place and their size before any script runs. */
export function declarative(options) {
  return `<template shadowrootmode="open"><style>${CONTROLS_CSS}</style>${markup(options)}</template>`;
}
