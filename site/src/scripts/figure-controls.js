// <figure-controls for="id">: a play button, a time slider, beat stepping and a control for each
// param of a `<pencil-figure>`. It only calls the element's public API (`info`, `play`, `pause`,
// `time`, `step`, `hold`, `value`, `range`), as every page in `web/` does, so a figure needs no
// controls of its own written for it.
//
// The build writes the controls into the page already, as the figure will stand when it starts
// and disabled (`controls-markup.js`, through `Figure.astro`), so they have their place before
// the figure does. This turns them on when the figure loads, and writes them again only if the
// figure isn't the kind they were made for, as when a reader picks another look.
//
// `bare` holds nothing, and only says why a figure couldn't be drawn, if it couldn't.
// `looks` on the element, a JSON list of `{ name, compiled }` or `{ name, src, sheets }`, adds a
// choice between them, for a figure shown in more than one style.
import { markup, shape, steps, beatAt, valueText, timeText } from './controls-markup.js';

class FigureControls extends HTMLElement {
  #figure = null;
  #off = new AbortController();
  #parts = null; // the controls now in it, found once after each time they're written

  connectedCallback() {
    this.#figure = document.getElementById(this.getAttribute('for'));
    if (!this.#figure) return;
    const signal = this.#off.signal;
    this.#figure.addEventListener('load', () => this.#ready(), { signal });
    this.#figure.addEventListener('draw', () => this.#read(), { signal });
    this.#figure.addEventListener('play', () => this.#paintPlay(), { signal });
    this.#figure.addEventListener('pause', () => this.#paintPlay(), { signal });
    this.#figure.addEventListener('error', (e) => this.#fail(e.detail?.error), { signal });
    // One listener for each kind of thing a reader does, whatever controls are written in.
    this.addEventListener('input', (e) => this.#input(e.target), { signal });
    this.addEventListener('change', (e) => this.#change(e.target), { signal });
    this.addEventListener('click', (e) => this.#click(e.target.closest('button')), { signal });
    if (this.#figure.info) this.#ready();
  }

  disconnectedCallback() {
    this.#off.abort();
    this.#off = new AbortController();
  }

  get #looks() {
    return JSON.parse(this.getAttribute('looks') ?? '[]');
  }

  /** The figure has loaded: turn the controls on, writing them first if they aren't its. */
  #ready() {
    const figure = this.#figure;
    const info = figure.info;
    if (!info || this.hasAttribute('bare')) return;
    const looks = this.#looks;
    const made = shape(info, looks);
    if (this.dataset.shape !== made) {
      const values = Object.fromEntries(info.params.map((p) => [p.id, figure.value(p.id)]));
      const ranges = Object.fromEntries(info.params.map((p) => [p.id, figure.range(p.id)]));
      const note = this.querySelector('.error');
      // A reader who picked a look with the keyboard is still on that control afterwards.
      const focused = this.contains(document.activeElement) ? document.activeElement.id : '';
      this.innerHTML = markup({
        id: figure.id,
        info,
        moment: { time: figure.time, values, ranges },
        paused: figure.paused,
        looks,
        look: Number(this.dataset.look ?? 0),
        looksLabel: this.getAttribute('looks-label') ?? undefined,
      });
      if (note) this.append(note);
      this.dataset.shape = made;
      if (focused) document.getElementById(focused)?.focus();
    } else {
      for (const control of this.querySelectorAll('input, select, button')) control.disabled = false;
    }
    this.#parts = null;
    this.hidden = this.children.length === 0;
    this.#paintPlay();
    this.#read();
  }

  #find() {
    const one = (selector) => this.querySelector(selector);
    return (this.#parts ??= {
      play: one('[data-play]'),
      time: one('[data-time]'),
      timeOut: one('[data-time-out]'),
      beatOut: one('[data-beat-out]'),
      back: one('[data-step="-1"]'),
      next: one('[data-step="1"]'),
      captions: [...this.querySelectorAll('[data-captions] > .caption')],
      params: [...this.querySelectorAll('[data-param]')].map((input) => ({
        input,
        out: this.querySelector(`[data-out="${CSS.escape(input.dataset.param)}"]`),
        param: this.#figure.info.params.find((p) => p.id === input.dataset.param),
      })),
    });
  }

  #input(target) {
    const figure = this.#figure;
    if (target.matches('[data-time]')) {
      figure.pause();
      figure.time = Number(target.value);
    } else if (target.matches('input[data-param]')) {
      figure.pause();
      figure.hold(target.dataset.param, Number(target.value));
    }
  }

  #change(target) {
    const figure = this.#figure;
    if (target.matches('select[data-param]')) {
      figure.hold(target.dataset.param, Number(target.value));
    } else if (target.matches('[data-look]')) {
      this.dataset.look = target.value;
      const look = this.#looks[Number(target.value)];
      // A build gives each look as a compiled figure, and `astro dev` as its sources.
      for (const name of ['src', 'sheets', 'compiled']) {
        if (look[name] == null) figure.removeAttribute(name);
        else figure.setAttribute(name, look[name]);
      }
      this.dispatchEvent(new CustomEvent('look', { detail: look, bubbles: true }));
    }
  }

  #click(button) {
    if (!button || button.disabled) return;
    const figure = this.#figure;
    if (button.matches('[data-play]')) figure.paused ? figure.play() : figure.pause();
    else if (button.dataset.step) figure.step(Number(button.dataset.step));
  }

  #paintPlay() {
    if (!this.#figure.info || this.hasAttribute('bare')) return;
    const { play } = this.#find();
    if (!play) return;
    const text = this.#figure.paused ? 'Play' : 'Pause';
    if (play.textContent !== text) play.textContent = text;
  }

  /** Reads the figure back, so the controls show what's drawn. */
  #read() {
    const figure = this.#figure;
    const info = figure.info;
    if (!info || this.hasAttribute('bare') || this.dataset.shape !== shape(info, this.#looks)) return;
    const parts = this.#find();
    const say = (el, text) => {
      if (el.textContent !== text) el.textContent = text;
    };
    if (parts.time) {
      parts.time.max = figure.duration;
      if (document.activeElement !== parts.time) parts.time.value = figure.time;
      say(parts.timeOut, timeText(figure.time));
    }
    for (const { param, input, out } of parts.params) {
      if (!param) continue;
      // A range that follows another param, as `0..q` does, is read as it stands, so the
      // control stops where the figure does.
      if (param.follows?.length) [input.min, input.max] = figure.range(param.id);
      const value = figure.value(param.id);
      if (document.activeElement !== input && Number.isFinite(value)) input.value = String(value);
      if (!param.choice) say(out, valueText(param, value));
    }
    if (parts.beatOut && steps(info)) {
      const range = info.params.find((p) => p.id === 'beat');
      const beat = Number.isFinite(figure.beat) ? figure.beat : 0;
      const at = beatAt(info, beat);
      say(parts.beatOut, at.text);
      parts.captions.forEach((caption, i) => caption.toggleAttribute('data-on', i === at.caption));
      parts.back.disabled = beat <= range.min;
      parts.next.disabled = beat >= range.max;
    }
  }

  #fail(error) {
    let note = this.querySelector('.error');
    if (!note) {
      note = Object.assign(document.createElement('p'), { className: 'error' });
      note.setAttribute('role', 'alert');
      this.append(note);
    }
    this.hidden = false;
    note.textContent = `This figure couldn’t be drawn here: ${error?.message ?? error}`;
  }
}

customElements.define('figure-controls', FigureControls);

// `?gl` on the page's address draws every figure with WebGL2, as on the pages in `web/`.
if (new URLSearchParams(location.search).has('gl')) {
  for (const figure of document.querySelectorAll('pencil-figure')) figure.setAttribute('gl', '');
}

// A tall figure fits the window's height on a wide screen rather than running off it, as in the
// playground. On a narrow one it takes the width, since a sheet like the lattice's narrow one is
// drawn tall on purpose. The stylesheet says when `--fit-width` applies.
for (const figure of document.querySelectorAll('pencil-figure')) {
  figure.addEventListener('load', () => {
    const [w, h] = figure.designSize ?? [0, 0];
    if (w && h) figure.style.setProperty('--fit-width', `calc(85vh * ${w / h})`);
    else figure.style.removeProperty('--fit-width');
  });
}

// A figure whose page starts it at a moment, as Möbius's starts at the graph's own map, gets
// `data-start` in seconds, set once when it first loads.
for (const figure of document.querySelectorAll('pencil-figure[data-start]')) {
  figure.addEventListener('load', () => (figure.time = Number(figure.dataset.start)), { once: true });
}
