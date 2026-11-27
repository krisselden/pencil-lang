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
import { PHONE, paramNamed, valueText, timeText, beatAt, steps, readoutHtml } from './controls.js';

const ATTRIBUTES = ['data-play', 'data-time', 'data-step', 'data-param', 'data-readout'];

/** What a control is, as a selector that finds the same control in controls written again: its
 * `id`, or what it's marked as being for, or the menu's button. None for anything else. */
export function identity(el) {
  if (el.id) return `#${CSS.escape(el.id)}`;
  for (const name of ATTRIBUTES) {
    if (!el.hasAttribute(name)) continue;
    const value = el.getAttribute(name);
    let key = `${el.localName}[${name}${value ? `="${CSS.escape(value)}"` : ''}]`;
    if (el.type === 'radio') key += `[value="${CSS.escape(el.value)}"]`;
    return key;
  }
  return el.classList.contains('more') ? '.more' : null;
}

export class Wiring {
  #figure;
  #roots = [];
  #off = null;
  #parts = null;
  #sized = null; // watches the bars, which hold one row on a phone
  #beatSaid = undefined; // the beat last said, by its caption's place

  constructor(figure) {
    this.#figure = figure;
  }

  /** Listens on these roots, a shadow root or a page's own element, and looks for what's in them
   * again. */
  attach(roots) {
    this.#beatSaid = undefined;
    this.#off?.abort();
    this.#off = new AbortController();
    this.#roots = roots;
    this.#parts = null;
    const signal = this.#off.signal;
    for (const root of roots) {
      root.addEventListener('input', (e) => this.#input(e.target), { signal });
      root.addEventListener('change', (e) => this.#change(e.target), { signal });
      root.addEventListener('click', (e) => this.#click(e.target.closest?.('button')), { signal });
      this.#menus(root, signal);
    }
    this.#sized?.disconnect();
    this.#sized = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => this.fit());
    for (const root of roots) for (const bar of root.querySelectorAll('.bar')) this.#sized?.observe(bar);
  }

  /** A menu that closes with the focus in it, on Escape, gives the focus back to its button, as a
   * menu should, where a browser can leave it nowhere. */
  #menus(root, signal) {
    const scope = root.getRootNode?.() ?? document;
    for (const menu of root.querySelectorAll('[popover][id]')) {
      const button = root.querySelector(`[popovertarget="${CSS.escape(menu.id)}"]`);
      let inside = false;
      menu.addEventListener(
        'beforetoggle',
        (e) => {
          if (e.newState === 'closed') inside = menu.contains(scope.activeElement);
        },
        { signal },
      );
      menu.addEventListener(
        'toggle',
        (e) => {
          if (e.newState !== 'closed' || !inside) return;
          inside = false;
          const now = scope.activeElement;
          if (!now || now === document.body || menu.contains(now)) button?.focus();
        },
        { signal },
      );
    }
  }

  /**
   * On a phone a bar keeps one row on one line, and one that doesn't fit, as radios with long
   * words or a long caption, goes behind the menu with the rest, so nothing in the bar is cut off.
   * The bar is as tall either way, so nothing moves.
   */
  fit() {
    const phone = typeof matchMedia !== 'undefined' && matchMedia(PHONE).matches;
    for (const root of this.#roots) {
      const bar = root.querySelector('.bar');
      const menu = root.querySelector('.rest');
      const controls = root.querySelector('.controls');
      if (!bar || !menu || !controls) continue;
      const scope = root.getRootNode?.() ?? document;
      const focused = scope.activeElement;
      // A row the bar gave up comes back to see whether it fits now, as when a phone turns.
      const moved = menu.querySelector(':scope > .row[data-moved]');
      if (moved) {
        moved.removeAttribute('data-moved');
        bar.insertBefore(moved, bar.querySelector(':scope > .more'));
        bar.removeAttribute('data-crowded');
        controls.toggleAttribute('data-more', controls.hasAttribute('data-menu'));
        this.#parts = null;
      }
      const row = bar.querySelector(':scope > .row');
      if (phone && row && row.scrollWidth > row.clientWidth + 0.5) {
        menu.prepend(row);
        row.setAttribute('data-moved', '');
        bar.setAttribute('data-crowded', '');
        controls.setAttribute('data-more', '');
        this.#parts = null;
      }
      // Moving what has the focus takes it away, so it's given back.
      if (focused && focused !== scope.activeElement && focused.isConnected) focused.focus({ preventScroll: true });
    }
  }

  get roots() {
    return this.#roots;
  }

  /** Whether the controls say each new beat themselves, so the figure needn't say a key's. */
  get speaks() {
    return this.#roots.length > 0 && !!this.#figure.info && this.#find().said.length > 0;
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
      said: this.#all('[data-said]'),
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
    this.fit();
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
      // A beat that changes is said, however it changed: which it is, and its caption, in the
      // words the element says a key's step in, which it leaves to this. Not the first, which is
      // where the figure starts and isn't news. Beats with lengths are at their start before the
      // first is done.
      const step = (info.beatLengths?.length ?? 0) > 0 && beat < 1e-6 ? 0 : at.caption + 1;
      if (this.#beatSaid !== undefined && this.#beatSaid !== step) {
        const caption = info.beats[step - 1];
        const words = step < 1 ? 'Start' : `Step ${step} of ${info.beats.length}${caption ? `: ${caption}` : ''}`;
        for (const said of parts.said) say(said, words);
      }
      this.#beatSaid = step;
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
    this.#sized?.disconnect();
    this.#sized = null;
    this.#roots = [];
    this.#parts = null;
  }
}
