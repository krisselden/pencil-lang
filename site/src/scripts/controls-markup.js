// What `<figure-controls>` holds, as HTML: a choice of looks, a play button and a time slider,
// beat stepping with its caption, and a control for each param. One function writes it, for the
// build and for the page. The build writes it into the page from what `pencil build` says of the
// figure, as the figure will stand when it starts and with everything disabled, so the controls
// have their place and their size before any script runs. The page writes it again only when a
// figure turns out to be another shape of thing, as when a reader picks another look.
//
// Nothing here moves when a value changes. An `<output>` is as wide as the widest value it can
// show, worked out from the param's range, and every caption of a figure is laid in the same
// place, so the row is as tall as the longest of them from the start.

const escape = (text) => String(text).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const attrs = (pairs) =>
  Object.entries(pairs)
    .filter(([, value]) => value !== undefined && value !== null && value !== false)
    .map(([name, value]) => (value === true ? ` ${name}` : ` ${name}="${escape(value)}"`))
    .join('');

/** How long time runs before it loops, for a figure that declares no `duration`, as the element has it. */
const LOOP = 10;

/** As many decimals as a param's step has, so a whole-number param reads as one. */
export const decimals = (param) => (param.step == null ? 2 : (String(param.step).split('.')[1] ?? '').length);

/** A param's value as its control says it. One that doesn't exist this frame is a dash. */
export const valueText = (param, value) => (Number.isFinite(value) ? value.toFixed(decimals(param)) : '—');

export const timeText = (seconds) => `${seconds.toFixed(1)} s`;

/** Whether the figure has beats to step through. */
export const steps = (info) => info.beats.length > 0 && info.params.some((p) => p.id === 'beat');

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

/** The room an output needs: as many digits wide as the longest thing it will say. With
 * tabular figures a digit is a `ch`, and a sign, a point and a space are no wider. */
const room = (...texts) => `--room: ${Math.max(...texts.map((text) => text.length))}ch`;

/**
 * What kind of controls a figure has, as a short string. Controls made for one figure fit
 * another with the same one, so the page keeps what the build wrote and only turns it on.
 */
export function shape(info, looks = []) {
  const text = JSON.stringify([
    info.usesTime,
    info.duration,
    info.beats,
    timed(info),
    info.params.map((p) => [p.id, p.min, p.max, p.step, p.choice]),
    looks.map((look) => look.name),
  ]);
  let hash = 5381;
  for (let i = 0; i < text.length; i++) hash = ((hash * 33) ^ text.charCodeAt(i)) >>> 0;
  return hash.toString(36);
}

/**
 * The controls for a figure.
 *
 * - `id`: the figure's, which the controls' own ids start with.
 * - `info`: what the figure says of itself.
 * - `moment`: where it stands: `{ time, values, ranges }`, each of the last two by param.
 * - `paused`: whether it's paused, for the play button.
 * - `looks`, `look`, `looksLabel`: the choice of looks, which is picked, and what it's called.
 * - `disabled`: the figure isn't ready, so nothing can be worked yet.
 */
export function markup({ id, info, moment, paused = false, looks = [], look = 0, looksLabel = 'Style', disabled = false }) {
  const { params = [], beats = [], usesTime } = info;
  const { time = 0, values = {}, ranges = {} } = moment ?? {};
  const rows = [];

  if (looks.length > 1) {
    const options = looks.map((each, i) => `<option${attrs({ value: i, selected: i === look })}>${escape(each.name)}</option>`).join('');
    rows.push(
      `<div class="row"><label for="${escape(id)}-look">${escape(looksLabel)}</label>` +
        `<select${attrs({ id: `${id}-look`, 'data-look': true, disabled })}>${options}</select></div>`,
    );
  }

  if (usesTime) {
    const duration = info.duration ?? LOOP;
    rows.push(
      `<div class="row time">` +
        `<button${attrs({ type: 'button', class: 'play', 'data-play': true, disabled })}>${paused ? 'Play' : 'Pause'}</button>` +
        `<input${attrs({ type: 'range', id: `${id}-time`, 'data-time': true, min: 0, max: duration, step: 0.001, value: time, 'aria-label': 'Time', disabled })}>` +
        `<output${attrs({ for: `${id}-time`, 'data-time-out': true, style: room(timeText(duration)) })}>${timeText(time)}</output>` +
        `</div>`,
    );
  }

  const stepped = steps(info);
  if (stepped) {
    const range = params.find((p) => p.id === 'beat');
    const beat = Number.isFinite(values.beat) ? values.beat : 0;
    const at = beatAt(info, beat);
    // The widest it says is a beat part done, as `9.5 / 10`, when beats have lengths.
    const widest = timed(info) ? `${beats.length}.5 / ${beats.length}` : `${beats.length} / ${beats.length}`;
    const captions = beats
      .map((caption, i) => `<span${attrs({ class: 'caption', 'data-on': i === at.caption })}>${escape(caption ?? '')}</span>`)
      .join('');
    rows.push(
      `<div class="row beats">` +
        `<button${attrs({ type: 'button', 'data-step': -1, title: 'Previous beat', 'aria-label': 'Previous beat', disabled: disabled || beat <= range.min })}>←</button>` +
        `<output${attrs({ 'data-beat-out': true, style: room(widest) })}>${at.text}</output>` +
        `<button${attrs({ type: 'button', 'data-step': 1, title: 'Next beat', 'aria-label': 'Next beat', disabled: disabled || beat >= range.max })}>→</button>` +
        `<span class="captions" data-captions>${captions}</span>` +
        `</div>`,
    );
  }

  for (const param of params) {
    if (param.id === 'beat' && stepped) continue;
    const inputId = `${id}-param-${param.id}`;
    const value = values[param.id];
    const [min, max] = ranges[param.id] ?? [param.min, param.max];
    let input;
    if (param.choice) {
      const options = param.choice.options
        .map((text, i) => `<option${attrs({ value: i, selected: i === Math.round(value) })}>${escape(text)}</option>`)
        .join('');
      input = `<select${attrs({ id: inputId, 'data-param': param.id, disabled })}>${options}</select>`;
    } else {
      const step = param.step ?? ((param.max - param.min) / 1000 || 0.001);
      input = `<input${attrs({ type: 'range', id: inputId, 'data-param': param.id, min, max, step, value: Number.isFinite(value) ? value : undefined, disabled })}>`;
    }
    // A choice says itself, so its output is empty and takes no room.
    const out = param.choice
      ? `<output${attrs({ for: inputId, 'data-out': param.id, hidden: true })}></output>`
      : `<output${attrs({ for: inputId, 'data-out': param.id, style: room(valueText(param, param.min), valueText(param, param.max)) })}>${valueText(param, value)}</output>`;
    rows.push(
      `<div class="row"><label class="param" for="${escape(inputId)}"><var>${escape(param.choice?.of ?? param.id)}</var></label>${input}${out}</div>`,
    );
  }

  return rows.join('');
}
