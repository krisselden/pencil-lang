// What makes a figure's controls work: its defaults in `<pencil-controls>`, or a page's own in the
// `controls` slot. Either way, each control is marked with what it sets and each output with what
// it shows, and this sets their ranges, steps and values from the figure, follows them each frame,
// and hands what a reader does to the figure, through its public API only:
//
//   input[type=range][data-param="q"]   a slider: its min, max, step and value are the figure's
//   select[data-param="along"]          a choice's options, filled in if it has none
//   input[type=radio][data-param="along"][value="1"]   one option, by its place from 0
//   input[type=checkbox][data-param="glow"]            a choice of off and on: checked is on
//   output[data-param="q"]              the param's value, as its control says it
//   [data-readout="ratio"]              what a readout says, laid out as the figure lays it out
//   button[data-play]                   plays and pauses; input[type=range][data-time] and
//                                       output[data-time] are the time
//   button[data-step="1"], [data-step="-1"], output[data-beat], [data-captions]   beats
//
// A choice's `data-param` is the name its value has, `along`, or its param, `along.index`.
import { paramNamed, valueText, timeText, beatAt, steps, readoutHtml } from './controls.js';

export class Wiring {
  #figure;
  #roots = [];
  #off = null;
  #parts = null;

  constructor(figure) {
    this.#figure = figure;
  }

  /** Listens on these roots, a shadow root or a page's own element, and looks for what's in them
   * again. */
  attach(roots) {
    this.#off?.abort();
    this.#off = new AbortController();
    this.#roots = roots;
    this.#parts = null;
    const signal = this.#off.signal;
    for (const root of roots) {
      root.addEventListener('input', (e) => this.#input(e.target), { signal });
      root.addEventListener('change', (e) => this.#change(e.target), { signal });
      root.addEventListener('click', (e) => this.#click(e.target.closest?.('button')), { signal });
    }
  }

  get roots() {
    return this.#roots;
  }

  /** What's in them changed: look again. */
  forget() {
    this.#parts = null;
  }

  #all(selector) {
    return this.#roots.flatMap((root) => [...root.querySelectorAll(selector)]);
  }

  #find() {
    const info = this.#figure.info;
    return (this.#parts ??= {
      play: this.#all('[data-play]'),
      times: this.#all('input[data-time]'),
      timeOuts: this.#all('[data-time]:not(input)'),
      beatOuts: this.#all('[data-beat]'),
      back: this.#all('[data-step="-1"]'),
      next: this.#all('[data-step="1"]'),
      captions: this.#all('[data-captions]').map((list) => [...list.children]),
      params: this.#all('[data-param]')
        .map((el) => ({ el, param: paramNamed(info, el.dataset.param), said: null }))
        .filter((each) => each.param),
      readouts: this.#all('[data-readout]').map((el) => ({ el, id: el.dataset.readout, said: null })),
    });
  }

  /** The figure has loaded: everything can be worked, and shows the figure as it stands. */
  enable() {
    for (const root of this.#roots) {
      // Only what's marked as the figure's: a page's other controls are the page's.
      for (const control of root.querySelectorAll('[data-param], [data-time], [data-play], [data-step]')) {
        if ('disabled' in control) control.disabled = false;
      }
    }
    this.#parts = null;
    const info = this.#figure.info;
    for (const { el, param } of this.#find().params) {
      if (el.localName === 'select' && el.options.length === 0 && param.choice) {
        param.choice.options.forEach((text, i) => el.append(new Option(text, String(i))));
      }
      if (el.localName === 'input' && el.type === 'range') {
        const [min, max] = this.#figure.range(param.id);
        Object.assign(el, { min, max, step: param.step ?? ((param.max - param.min) / 1000 || 0.001) });
      }
    }
    for (const input of this.#find().times) Object.assign(input, { min: 0, max: this.#figure.duration, step: 0.001 });
    if (info) this.paintPlay();
    this.read();
  }

  #input(target) {
    const figure = this.#figure;
    if (!figure.info || !target?.matches) return;
    if (target.matches('input[type=range][data-time]')) {
      figure.pause();
      figure.time = Number(target.value);
    } else if (target.matches('input[type=range][data-param]')) {
      const param = paramNamed(figure.info, target.dataset.param);
      if (!param) return;
      figure.pause();
      figure.hold(param.id, Number(target.value));
    }
  }

  #change(target) {
    const figure = this.#figure;
    if (!figure.info || !target?.matches?.('select[data-param], input[type=radio][data-param], input[type=checkbox][data-param]')) return;
    const param = paramNamed(figure.info, target.dataset.param);
    if (!param) return;
    if (target.type === 'checkbox') figure.hold(param.id, target.checked ? 1 : 0);
    else if (target.type !== 'radio' || target.checked) figure.hold(param.id, Number(target.value));
  }

  #click(button) {
    if (!button || button.disabled || !this.#figure.info) return;
    const figure = this.#figure;
    if (button.matches('[data-play]')) figure.paused ? figure.play() : figure.pause();
    else if (button.dataset.step) figure.step(Number(button.dataset.step));
  }

  /** Says Play or Pause. A button with a word for each shows the one that applies. */
  paintPlay() {
    const paused = this.#figure.paused;
    for (const button of this.#find().play) {
      const words = button.querySelectorAll(':scope > span');
      if (words.length === 2) {
        words[0].toggleAttribute('data-on', paused);
        words[1].toggleAttribute('data-on', !paused);
      } else {
        const text = paused ? 'Play' : 'Pause';
        if (button.textContent !== text) button.textContent = text;
      }
      button.setAttribute('aria-label', paused ? 'Play' : 'Pause');
    }
  }

  /** Reads the figure back, so the controls show what's drawn. Only what changed is written. */
  read() {
    const figure = this.#figure;
    const info = figure.info;
    if (!info || !this.#roots.length) return;
    const parts = this.#find();
    const say = (el, text) => {
      if (el.textContent !== text) el.textContent = text;
    };
    const time = figure.time;
    for (const input of parts.times) {
      if (!input.matches(':focus')) input.value = String(time);
      input.setAttribute('aria-valuetext', timeText(time));
    }
    for (const out of parts.timeOuts) say(out, timeText(time));
    for (const each of parts.params) {
      const { el, param } = each;
      const value = figure.value(param.id);
      if (el.localName === 'input' && el.type === 'range') {
        // A range that follows another param is read as it stands, so the control stops where
        // the figure does.
        if (param.follows?.length) [el.min, el.max] = figure.range(param.id);
        if (!el.matches(':focus') && Number.isFinite(value)) el.value = String(value);
        const text = valueText(param, value);
        if (each.said !== text) el.setAttribute('aria-valuetext', text);
        each.said = text;
      } else if (el.localName === 'input' && el.type === 'radio') {
        el.checked = Number(el.value) === Math.round(value);
      } else if (el.localName === 'input' && el.type === 'checkbox') {
        el.checked = Math.round(value) === 1;
      } else if (el.localName === 'select') {
        if (!el.matches(':focus') && Number.isFinite(value)) el.value = String(Math.round(value));
      } else if (el.localName !== 'input') {
        say(el, valueText(param, value));
      }
    }
    if (steps(info) && (parts.beatOuts.length || parts.captions.length)) {
      const range = info.params.find((p) => p.id === 'beat');
      const beat = Number.isFinite(figure.beat) ? figure.beat : 0;
      const at = beatAt(info, beat);
      for (const out of parts.beatOuts) say(out, at.text);
      for (const list of parts.captions) list.forEach((caption, i) => caption.toggleAttribute('data-on', i === at.caption));
      for (const button of parts.back) button.disabled = beat <= range.min;
      for (const button of parts.next) button.disabled = beat >= range.max;
    }
    if (parts.readouts.length) {
      const said = new Map(figure.readouts.map((readout) => [readout.id, readout]));
      for (const each of parts.readouts) {
        const readout = said.get(each.id);
        const key = readout ? readout.text : '';
        if (each.said === key) continue;
        each.said = key;
        each.el.innerHTML = readoutHtml(readout);
      }
    }
  }

  detach() {
    this.#off?.abort();
    this.#off = null;
    this.#roots = [];
    this.#parts = null;
  }
}
