// Symbols as they're typed, for a keyboard that lacks them. Pencil's text is the symbol itself,
// `→` and `θ` and `ℝ`, and never a backslash, so an editor writes one for its LaTeX name: `\to`
// and then a space is `→ `, `\theta` is `θ`, `\R` is `ℝ`. Inside a label's text, where they can't
// be operators, `->`, `|->`, `<=`, `>=`, `!=`, `~=` and `+-` are their symbols too. The lists are
// the core's (`formula::symbols`), which only names what the math font draws.
import { ready as init, symbols } from '@pencil-lang/figure/host';

let names = null;
let digraphs = [];
export const ready = init().then(() => {
  const table = JSON.parse(symbols());
  names = new Map(table.names);
  digraphs = table.digraphs;
});

/**
 * What the text before the caret ends in that should be a symbol: `[typed, written]`, the
 * characters to take back and what to write for them. A name is done when something that can't
 * continue it is typed, which stays after the symbol, or when `asked` (Tab) says it's done.
 */
export function completion(before, asked = false) {
  if (!names) return null;
  const named = (asked ? /\\([A-Za-z]+)$/ : /\\([A-Za-z]+)([^A-Za-z])$/).exec(before);
  if (named) {
    const symbol = names.get(named[1]);
    return symbol ? [named[0], symbol + (named[2] ?? '')] : null;
  }
  if (asked) return null;
  // Inside a label's text: an odd number of quotes so far on this line.
  const line = before.slice(before.lastIndexOf('\n') + 1);
  if ((line.match(/"/g) ?? []).length % 2 === 0) return null;
  const found = digraphs.find(([typed]) => before.endsWith(typed));
  return found ?? null;
}

/** Writes the symbol for what was just typed, through the browser's own editing so undo gives
 * back what was typed. Says whether it did. */
function write(textarea, asked, note) {
  if (textarea.selectionStart !== textarea.selectionEnd) return false;
  const at = textarea.selectionStart;
  const found = completion(textarea.value.slice(0, at), asked);
  if (!found) return false;
  const [typed, written] = found;
  textarea.setSelectionRange(at - typed.length, at);
  document.execCommand('insertText', false, written);
  note(`Wrote ${written.trim() || written} for ${typed.trim() || typed}`);
  return true;
}

/** Makes a textarea write symbols as they're typed. `note(text)` says what was written. */
export function typeSymbols(textarea, note) {
  textarea.addEventListener('input', (e) => {
    if (e.inputType === 'insertText' && !e.isComposing) write(textarea, false, note);
  });
}

/** Finishes a name at the caret, as Tab does. Says whether there was one. */
export const finishSymbol = (textarea, note) => write(textarea, true, note);
