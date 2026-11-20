// The editor's undo and redo: a history for each file, kept by the page.
//
// One textarea shows each of a figure's files in turn, and its text is set from script when the
// file changes. The browser keeps one stack of edits for the whole page, and a script-set value
// leaves the old file's edits on it: on iOS Safari an undo or a redo then put the `.pen`'s edit
// into the sheet being shown (docs/goals.md, 2026-11-20). So every undo and redo is the page's:
// the Undo and Redo keys, Cmd+Z and Shift+Cmd+Z (Ctrl elsewhere, and Ctrl+Y off a Mac), and the
// system's own undo (the Edit menu, iOS's gestures), a `beforeinput` of `historyUndo`, which is
// stopped. An undo or redo the browser does anyway, when it couldn't be stopped, is put back and
// done again from here.
//
// The browser's own stack still records each edit and is never applied. It's what makes the
// browser offer its undo at all, and since its undo is always stopped it never has anything to
// redo: the system's redo isn't offered, and redo is the Redo key or the shortcut. A
// `historyRedo` is still handled, as a guard.
//
// Nothing is undone while a modal dialog is open or the editor is hidden: the browser sends its
// undo to the textarea's last edit wherever focus is, even from a dialog's empty field.
//
// An edit is recorded as the browser makes it, from its `input`. A run of typed characters, or of
// deletes, each where the last left the caret and within two seconds of it, is one step, and so is
// one composition from an input method, or a run of them each where the last ended. Anything else
// is a step of its own: a word finished from the row, a symbol written for its name, a paste, a
// selection replaced. A text dragged within the editor is one step.

const histories = new WeakMap();

/** The history a textarea has, if any, for code that edits it (a paste of LaTeX). */
export const historyOf = (textarea) => histories.get(textarea);

/** How long typing can pause and still be the same step. */
const RUN_MS = 2000;
/** The most steps a file keeps. */
const LIMIT = 500;
const MAC = /Mac|iPhone|iPad/.test(navigator.platform ?? '');

/**
 * Keeps `textarea`'s history, a file at a time (`show`). `within`: other elements where focus is
 * still the editor's, such as its keys, whose Cmd/Ctrl+Z are the editor's too.
 */
export function editHistory(textarea, { within = [] } = {}) {
  const files = new WeakMap();
  const fresh = (text) => ({ undo: [], redo: [], now: text });
  let entry = fresh(textarea.value);
  /** The selection as it was before the edit being made, from its `beforeinput`, until it's used
   * or the task ends: a `beforeinput` the browser makes nothing of (a Backspace at the start)
   * has no `input` after it. */
  let pending = null;
  /** The selection after the last edit or move. */
  let sel = [textarea.selectionStart, textarea.selectionEnd];
  /** The next edit starts a step of its own. */
  let separated = true;
  /** While the page itself sets the text, nothing is recorded. */
  let applying = false;
  /** The input method's composition: a count of them, and whether one is going on. It counts as
   * going on until the task after its end, so the `input` that commits it is part of it. */
  let composition = 0;
  let composing = false;
  const listeners = new Set();
  const changed = () => listeners.forEach((f) => f());
  const selection = () => [textarea.selectionStart, textarea.selectionEnd];
  const clamp = ([a, b], text) => [Math.min(a, text.length), Math.min(b, text.length)];

  document.addEventListener('selectionchange', () => {
    if (document.activeElement === textarea && !applying) sel = selection();
  });
  textarea.addEventListener('compositionstart', () => {
    composition += 1;
    composing = true;
  });
  textarea.addEventListener('compositionend', () => {
    const was = composition;
    setTimeout(() => {
      if (composition === was) composing = false;
    });
  });

  /** Whether an undo that didn't come from the keys may go to the editor: it's shown, and no modal
   * dialog is over it. With focus elsewhere in the page, on the figure, the browser has always sent
   * its undo to the editor's last edit, and still does. */
  function mine() {
    if ([...document.querySelectorAll('dialog[open]')].some((d) => { try { return d.matches(':modal'); } catch { return true; } })) return false;
    return !textarea.closest('[inert]') && textarea.getClientRects().length > 0;
  }

  /** Whether `old` became `text` by replacing the selection `[a, b]`. */
  const fits = (old, text, [a, b]) =>
    a <= b && b <= old.length && text.length >= a + old.length - b && text.startsWith(old.slice(0, a)) && text.endsWith(old.slice(b));

  /** The selection before an edit: from its `beforeinput`, or the last one known, if either
   * explains the change; otherwise the part of the text that changed. */
  function before(old, text) {
    for (const s of [pending, sel]) if (s && fits(old, text, s)) return s;
    let p = 0;
    while (p < old.length && p < text.length && old[p] === text[p]) p++;
    let s = 0;
    while (s < old.length - p && s < text.length - p && old[old.length - 1 - s] === text[text.length - 1 - s]) s++;
    return [p, old.length - s];
  }

  /** What kind of edit made `text` from `old`. */
  function kindOf(old, text, [a, b], e) {
    if (composing || e.isComposing || /Composition/.test(e.inputType ?? '')) return 'compose';
    if (e.inputType === 'deleteByDrag' || e.inputType === 'insertFromDrop') return 'drag';
    if (/^insertFrom/.test(e.inputType ?? '')) return 'other';
    if (a === b && text.length > old.length) return 'insert';
    if (text.length < old.length && /^delete/.test(e.inputType ?? 'delete') && text === old.slice(0, a) + old.slice(b)) return 'delete';
    return 'other';
  }

  function record(e) {
    const old = entry.now;
    if (e.inputType === 'historyUndo' || e.inputType === 'historyRedo') {
      // The browser undid or redid from its own stack, which may hold another file's edits: put
      // the text back, and do it from this file's history instead, if it's the editor's.
      pending = null;
      put(old, sel);
      if (mine()) step(e.inputType === 'historyUndo' ? 'undo' : 'redo');
      return;
    }
    const text = textarea.value;
    const from = before(old, text);
    pending = null;
    const after = selection();
    sel = after;
    if (text === old) return;
    const kind = kindOf(old, text, from, e);
    const top = entry.undo.at(-1);
    const at = performance.now();
    const soon = top && at - top.at < RUN_MS;
    const where = top && (top.selAfter[0] === from[1] || top.selAfter[0] === from[0]);
    const goesOn = top && !separated && top.kind === kind && (
      (kind === 'compose' && (top.composition === composition || (soon && where)))
      || (kind === 'drag' && at - top.at < 1000 && top.drags === 1)
      || ((kind === 'insert' || kind === 'delete') && soon && where));
    if (goesOn) Object.assign(top, { after: text, selAfter: after, at, drags: top.drags + 1 });
    else {
      entry.undo.push({ before: old, after: text, selBefore: from, selAfter: after, kind, at, composition, drags: 1 });
      if (entry.undo.length > LIMIT) entry.undo.shift();
    }
    separated = false;
    entry.redo = [];
    entry.now = text;
    changed();
  }

  /** Sets the text from here, keeping the scroll, without recording it. */
  function put(text, selection) {
    applying = true;
    const scroll = [textarea.scrollTop, textarea.scrollLeft];
    if (textarea.value !== text) textarea.value = text;
    textarea.setSelectionRange(...clamp(selection, text));
    [textarea.scrollTop, textarea.scrollLeft] = scroll;
    sel = selection;
    applying = false;
  }

  /** Undoes or redoes one step of the file shown. Says whether there was one. */
  function step(way) {
    const from = way === 'undo' ? entry.undo : entry.redo;
    const s = from.pop();
    if (!s) return false;
    (way === 'undo' ? entry.redo : entry.undo).push(s);
    const [text, selection] = way === 'undo' ? [s.before, s.selBefore] : [s.after, s.selAfter];
    put(text, selection);
    entry.now = text;
    separated = true;
    // As the browser's own undo says it, so the page keeps the file, compiles it and brings the
    // caret into view.
    applying = true;
    textarea.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: way === 'undo' ? 'historyUndo' : 'historyRedo' }));
    applying = false;
    changed();
    return true;
  }

  // First, before any other listener sees the edit.
  addEventListener('input', (e) => {
    if (e.target === textarea && !applying) record(e);
  }, true);
  textarea.addEventListener('beforeinput', (e) => {
    if (e.inputType === 'historyUndo' || e.inputType === 'historyRedo') {
      // One that can't be stopped goes on to the browser's own undo, whose `input` is put back
      // and done from here (`record`): stepping here as well would take two steps.
      if (!e.cancelable) return;
      e.preventDefault();
      if (mine()) step(e.inputType === 'historyUndo' ? 'undo' : 'redo');
      return;
    }
    pending = selection();
    const was = pending;
    setTimeout(() => {
      if (pending === was) pending = null;
    });
  });
  // The shortcuts, here rather than through the browser, which has nothing to redo once its own
  // undo was stopped. On a Mac they're Cmd's, and Ctrl+Y is Cocoa's yank.
  const shortcut = (e) => {
    const mod = MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
    if (!mod || e.altKey || e.isComposing) return;
    const key = e.key.toLowerCase();
    const way = key === 'z' ? (e.shiftKey ? 'redo' : 'undo') : key === 'y' && !MAC && !e.shiftKey ? 'redo' : null;
    if (!way) return;
    e.preventDefault();
    if (mine()) step(way);
  };
  for (const el of [textarea, ...within]) el.addEventListener('keydown', shortcut);

  const history = {
    /** Shows the file `key` with `text`, from its own history. A text changed while it was away
     * (Revert all) is a step of that history, so undo takes the change back. */
    show(key, text) {
      let next = files.get(key);
      if (!next) files.set(key, (next = fresh(text)));
      else if (next.now !== text) {
        let p = 0;
        while (p < text.length && text[p] === next.now[p]) p++;
        next.undo.push({ before: next.now, after: text, selBefore: [p, p], selAfter: [p, p], kind: 'other', at: 0 });
        next.redo = [];
        next.now = text;
      }
      entry = next;
      applying = true;
      textarea.value = text;
      applying = false;
      separated = true;
      sel = selection();
      changed();
    },
    /** Hands the history of the file `from` to `to`, a copy of it that takes its place. */
    rekey(from, to) {
      const kept = files.get(from);
      if (!kept || from === to) return;
      files.set(to, kept);
      files.delete(from);
    },
    undo: () => step('undo'),
    redo: () => step('redo'),
    get canUndo() {
      return entry.undo.length > 0;
    },
    get canRedo() {
      return entry.redo.length > 0;
    },
    /** The next edit is a step of its own, as a word finished from the row is. */
    separate() {
      separated = true;
    },
    /** Calls `f` when what can be undone or redone may have changed. */
    onChange(f) {
      listeners.add(f);
    },
  };
  histories.set(textarea, history);
  return history;
}
