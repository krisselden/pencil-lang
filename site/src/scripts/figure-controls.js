// <figure-controls for="id">: a play button, a time slider, beat stepping and a control for each
// param, built from what a `<pencil-figure>` says of itself once it loads. It only calls the
// element's public API (`info`, `play`, `pause`, `time`, `step`, `hold`, `value`), as every page in
// `web/` does, so a figure needs no controls of its own written for it.
//
// `bare` builds nothing, and only says why a figure couldn't be drawn, if it couldn't.
// `looks` on the element, a JSON list of `{ name, src, sheets }`, adds a choice of sheets, for a
// figure shown in more than one style.

const make = (tag, props = {}, ...children) => {
  const el = Object.assign(document.createElement(tag), props);
  el.append(...children);
  return el;
};

/** As many decimals as a param's step has, so a whole-number param reads as one. */
const decimals = (param) => (param.step == null ? 2 : (String(param.step).split('.')[1] ?? '').length);

class FigureControls extends HTMLElement {
  #figure = null;
  #inputs = new Map();
  #off = new AbortController();

  connectedCallback() {
    this.#figure = document.getElementById(this.getAttribute('for'));
    if (!this.#figure) return;
    const signal = this.#off.signal;
    this.#figure.addEventListener('load', () => this.#build(), { signal });
    this.#figure.addEventListener('draw', () => this.#read(), { signal });
    this.#figure.addEventListener('play', () => this.#paintPlay(), { signal });
    this.#figure.addEventListener('pause', () => this.#paintPlay(), { signal });
    this.#figure.addEventListener('error', (e) => this.#fail(e.detail?.error), { signal });
    if (this.#figure.info) this.#build();
  }

  disconnectedCallback() {
    this.#off.abort();
    this.#off = new AbortController();
  }

  #build() {
    const figure = this.#figure;
    const info = figure.info;
    if (!info || this.hasAttribute('bare')) return;
    const { params = [], beats = [], usesTime } = info;
    this.#inputs.clear();
    const rows = [];
    const id = figure.id;

    const looks = JSON.parse(this.getAttribute('looks') ?? '[]');
    if (looks.length > 1) {
      const select = make('select', { id: `${id}-look` });
      looks.forEach((look, i) => select.append(new Option(look.name, String(i))));
      select.value = this.dataset.look ?? '0';
      select.addEventListener('change', () => {
        this.dataset.look = select.value;
        const look = looks[Number(select.value)];
        figure.setAttribute('src', look.src);
        figure.setAttribute('sheets', look.sheets);
        this.dispatchEvent(new CustomEvent('look', { detail: look, bubbles: true }));
      });
      rows.push(make('div', { className: 'row' }, make('label', { htmlFor: select.id, textContent: this.getAttribute('looks-label') ?? 'Style' }), select));
    }

    if (usesTime) {
      this.play = make('button', { type: 'button', className: 'play' });
      this.play.addEventListener('click', () => (figure.paused ? figure.play() : figure.pause()));
      this.time = make('input', {
        type: 'range', min: 0, max: figure.duration, step: 0.001, value: figure.time, id: `${id}-time`,
      });
      this.time.setAttribute('aria-label', 'Time');
      this.time.addEventListener('input', () => {
        figure.pause();
        figure.time = Number(this.time.value);
      });
      this.timeOut = make('output', { htmlFor: this.time.id });
      rows.push(make('div', { className: 'row time' }, this.play, this.time, this.timeOut));
    } else {
      this.play = this.time = this.timeOut = null;
    }

    if (beats.length && params.some((p) => p.id === 'beat')) {
      const back = make('button', { type: 'button', textContent: '←', title: 'Previous beat' });
      const next = make('button', { type: 'button', textContent: '→', title: 'Next beat' });
      back.setAttribute('aria-label', 'Previous beat');
      next.setAttribute('aria-label', 'Next beat');
      back.addEventListener('click', () => figure.step(-1));
      next.addEventListener('click', () => figure.step(1));
      this.beatOut = make('output');
      this.caption = make('span', { className: 'caption' });
      this.back = back;
      this.next = next;
      rows.push(make('div', { className: 'row beats' }, back, this.beatOut, next, this.caption));
    } else {
      this.beatOut = null;
    }

    for (const param of params) {
      if (param.id === 'beat' && this.beatOut) continue;
      const inputId = `${id}-param-${param.id}`;
      let input;
      if (param.choice) {
        input = make('select', { id: inputId });
        param.choice.options.forEach((text, i) => input.append(new Option(text, String(i))));
        input.addEventListener('change', () => figure.hold(param.id, Number(input.value)));
      } else {
        input = make('input', {
          type: 'range', id: inputId, min: param.min, max: param.max,
          step: param.step ?? ((param.max - param.min) / 1000 || 0.001),
        });
        input.addEventListener('input', () => {
          figure.pause();
          figure.hold(param.id, Number(input.value));
        });
      }
      const out = make('output', { htmlFor: inputId });
      const label = make('label', { htmlFor: inputId, className: 'param' }, make('var', { textContent: param.choice?.of ?? param.id }));
      this.#inputs.set(param.id, { param, input, out });
      rows.push(make('div', { className: 'row' }, label, input, out));
    }

    this.replaceChildren(...rows);
    this.hidden = rows.length === 0;
    this.#paintPlay();
    this.#read();
  }

  #paintPlay() {
    if (this.play) this.play.textContent = this.#figure.paused ? 'Play' : 'Pause';
  }

  /** Reads the figure back, so the controls show what's drawn. */
  #read() {
    const figure = this.#figure;
    const info = figure.info;
    if (!info || this.hasAttribute('bare')) return;
    if (this.time) {
      this.time.max = figure.duration;
      if (document.activeElement !== this.time) this.time.value = figure.time;
      this.timeOut.textContent = `${figure.time.toFixed(1)} s`;
    }
    for (const { param, input, out } of this.#inputs.values()) {
      const value = figure.value(param.id);
      if (document.activeElement !== input && Number.isFinite(value)) input.value = String(value);
      out.textContent = param.choice ? '' : Number.isFinite(value) ? value.toFixed(decimals(param)) : '—';
    }
    if (this.beatOut) {
      const beats = info.beats;
      const range = info.params.find((p) => p.id === 'beat');
      // Beats with lengths count beats done, from 0 to n. Untimed ones say which beat it's on.
      const timed = (info.beatLengths?.length ?? 0) > 0;
      const beat = Number.isFinite(figure.beat) ? figure.beat : 0;
      const shown = timed ? beat : beat + 1;
      const whole = Math.abs(shown - Math.round(shown)) < 1e-6;
      this.beatOut.textContent = `${whole ? Math.round(shown) : shown.toFixed(1)} / ${beats.length}`;
      const index = timed ? Math.min(beats.length - 1, Math.max(0, Math.ceil(beat) - 1)) : Math.round(beat);
      this.caption.textContent = beats[index] ?? '';
      this.back.disabled = beat <= range.min;
      this.next.disabled = beat >= range.max;
    }
  }

  #fail(error) {
    let note = this.querySelector('.error');
    if (!note) {
      note = make('p', { className: 'error' });
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

// Sheets chosen by the reader's theme and width, as the lattice's, hearts' and Möbius's pages do:
// a figure with `data-sheets` (JSON `{ base, narrow, dark }`) gets `sheets` set from them.
const wantsDark = matchMedia('(prefers-color-scheme: dark)');
const wantsNarrow = matchMedia('(max-width: 600px)');
const pickSheets = () => {
  for (const figure of document.querySelectorAll('pencil-figure[data-sheets]')) {
    const { base = [], narrow = [], dark = [] } = JSON.parse(figure.dataset.sheets);
    const sheets = [...base, ...(wantsNarrow.matches ? narrow : []), ...(wantsDark.matches ? dark : [])].join(' ');
    if (figure.getAttribute('sheets') !== sheets) figure.setAttribute('sheets', sheets);
    // `src` waits for the sheets, so the figure is fetched once, with the right ones.
    if (!figure.hasAttribute('src')) figure.setAttribute('src', figure.dataset.src);
  }
};
pickSheets();
wantsDark.addEventListener('change', pickSheets);
wantsNarrow.addEventListener('change', pickSheets);
