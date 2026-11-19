# Keyboard lab

A page for trying ways of typing a Pencil document on a phone. It has a live `<pencil-figure>` above a plain editor, and
the figure is compiled again as the text changes, as the playground does it. The page itself never scrolls: the app fills
what's visible, the figure is as big as the room left allows and sits in the middle of it, and the editor is the only
thing that scrolls.

**The playground took variant C on 2026-11-18** (`docs/goals.md`, The playground). The keys, the words and the hold on
the viewport moved to `web/keys.js`, `web/keyboard.js`, `web/keyboard.css` and `web/viewport.js`, and this page loads those
same files, so it stays a bench for what the playground has. It opens on C now, and the keys come up when the editor takes
focus and go when focus leaves it and them, as in the playground. ▾ closes them for the visit, and so does typing on a
hardware keyboard; **keys** in the bar brings them back. The lab keeps its choices apart from the playground's.

## Opening it

With `scripts/serve.py` running, open `http://localhost:8741/lab/keyboard/`. The address carries everything, so a link
opens the same page:

| In the address | What it does |
|---|---|
| `?keys=a`, `b` or `c` | Which way of typing (below), C when it's left out. The Keys menu in the bar switches it too. |
| `&figure=circle`, `tangents` or `limacon` | Which corpus figure. The ☰ menu switches it too. The File menu picks which of its files to edit. |
| `&numbers` | Shows what the browser says about its viewports over the figure (also in ☰). Use it on a phone to see what happens when a keyboard comes up. |
| `&widget=resizes-visual`, `resizes-content` or `overlays-content` | Tries another `interactive-widget` (below). The page asks for `resizes-content`. |
| `&pin=off` | Stops the page from scrolling itself back to the top when iOS scrolls it (below), to compare. |
| `&gl` | WebGL2, as on the other pages. |

Edits are kept in this browser, per file. ☰ → Revert drops them.

## The three ways of typing

**A. The system keyboard, with a row of symbols riding just above it.** This uses the phone's own keyboard: its
letters, its autocorrect (off here), its spacebar caret drag and its dictation. The row holds what that keyboard lacks or
hides two pages down. It scrolls sideways, and its ‹ › caret keys stay at the end.

**B. Pencil keys.** The editor has `inputmode="none"`, so the system keyboard stays down while the editor keeps focus,
its caret and its selection. The keys come up when the editor is touched, and ▾ puts them away. The bottom row switches
between four pages:

- **123:** numbers and operators, laid out as on Desmos's keypad.
- **abc:** letters.
- **f(x):** Pencil's words and functions.
- **αβγ:** Greek and symbols.

**system** hands over to the system keyboard, and the symbol row from A comes up over it. Its **Pencil keys** button
hands back.

**C. B, with a row of words above the keys.** The row shows words that finish what's being typed, chosen by where the
caret is:

| Where the caret is | What the row offers |
|---|---|
| At the start of a line | Statements (`let`, `style`, `curve`, …). In a `.style` file: `style`, `let`, `fn` and `use`. |
| After `style` | The document's marks and tags, and the type selectors (`figure`, `paper`, `curve`, …). |
| Inside a rule, before its `:` | Properties, most used first. Tapping one writes `width: `. |
| After a property's `:` | The sheets' own names (`ink`, `blue`), then `self.dist`, `px` and the other values, functions and constants. |
| Anywhere else | The names the figure uses, most used first, then the functions (tapping one writes `sin(`), the constants and words such as `for` and `in`. |

It shows nothing inside a comment or a label's text. Words are the slow part on a custom keyboard, and this is the
answer to that. It also stays over the system keyboard after a hand-over.

In every variant:
- A hardware keyboard types into the editor as usual.
- Typing `\theta` and a space writes θ, as in the playground, using `web/symbols.js` itself.
- Undo takes back what a key typed, because keys type through the browser's own editing (`execCommand('insertText')`).
- Every key is a real button with a name a screen reader says ("times" for `·`, "theta" for θ).
- The keys are one tab stop: the arrow keys move between them, and Enter or Space presses one.
- A key acts as it's pressed, so a held ⌫, ‹, › or space repeats.
- Keys in a row that scrolls act on release, so a swipe through them types nothing.

## What's on which key, and why

These counts are from every `.pen` and `.style` file in `corpus/` and `lab/explainer/`. The characters Pencil accepts in
an expression come from `docs/language.md` (Expressions; Mathematics in labels; Typing the symbols) and the lexer:

- ASCII operators, plus `·` for times.
- `²` and `³`.
- Subscript digits in a name.
- Letters of any script, Greek included.
- The constant glyphs `π ℯ ⅈ ⅇ`.

Arrows, relations and set symbols (`→ ≤ ∈ ℝ`) are only for a label's text or a `$…$` span.

| Character | Times used | How hard it is now |
|---|---|---|
| `.` | 1262 | |
| `,` | 1234 | |
| `(` `)` | 958 each | |
| `:` | 951 | |
| `/` | 668 | |
| `_` | 661 | Third page of iOS's keyboard. |
| `=` | 552 | Third page of iOS's keyboard. |
| `;` | 424 | |
| `{` `}` | 370 each | Third page of iOS's keyboard. |
| `·` | 317 | On no page of either system keyboard. |
| `-` | 230 | |
| `"` | 216 | |
| `+` | 164 | |
| `π` | 73 | On no page of either system keyboard. |
| `²` | 40 | A long press on 2 on iOS. |
| `^` | 22 | |

Greek, by use: τ 14, θ 13, σ 12, ω 10, α 3, β 3, φ 2, μ 1, ν 1.

The words, by use:

| Group | Words and counts |
|---|---|
| Statements | `style` 299, `let` 196, `curve` 80, `label` 48, `dot` 44, `fn` 55, `beat` 34, `use` 30, `pause` 23, `viewport` 22, `param` 22, `segment` 21 |
| Properties | `width` 132, `stroke` 129, `fill` 122, `opacity` 95, `radius` 52 |
| Functions | `mix` 47, `smoothstep` 25, `cos` 24, `exp` 23, `oklch` 23, `sin` 21, `intersect` 19 |

The units `px` and `em` are used 260 and 30 times.

- **123** follows Desmos's keypad:
  - On the left, what an expression needs: `x y t ²`, `( ) ^ ,`, `{ } : "`, `π _ ; ..`.
  - In the middle, the digits, with an operator at the end of each row: `/`, `·`, `-`, `+`. As on Desmos the last row is
    `0 . = +`.
  - On the right, ⌫, the caret keys ‹ ›, and new line.
  - It uses Pencil's own operators `/ · -`, where Desmos shows ÷ × −, so the key types what the file holds.
  - `t` takes Desmos's third variable key because it is the commonest curve parameter (121 uses).
  - `..` is one key because every range has it.
- **abc** is QWERTY, with ⌫ and the caret keys in place of the last letters' neighbours.
  - Its fourth row is `{ } ( ) _ . , : ; =` and new line, so `style c { fill: ink; }` and `let r_1 = 2` can be typed on
    one page.
  - ⇧ is once for the next letter; tapped twice quickly it stays on.
- **f(x)** is a list that scrolls, beside the editing keys. It holds the statements in order of use, then:
  - the functions, as the core lists them through `functions()`, so a new built-in shows up without an edit here;
  - the words inside a statement;
  - the constants;
  - the properties;
  - a sheet's values (`self.dist`, `px`, …).
- **αβγ** has the whole lowercase Greek alphabet, as GeoGebra's keyboard has it, but without omicron, which is an o.
  - Its last row is the label symbols a figure's captions use: `→ ↦ ≤ ≥ ≠ ± ∞ °`.
  - ⇧ gives the capitals that aren't Latin letters (`Γ Δ Θ Λ Ξ Π Σ Υ Φ Ψ Ω`) and the constant glyphs `ℯ ⅈ`.
  - It also gives `∂ ∇ √ ∑ ∫`, the number sets `ℝ ℂ ℤ ℕ ℚ`, and `∈ ∉ ⊂ ∪ ∩ × ÷ ≈`.
  - A key's spoken name comes from the core's own table (`symbols()`), so θ says "theta".
- **The symbol row (A, and B and C after a hand-over)** has the characters the system keyboard lacks or buries, most used
  first: `· ² π ( ) { } = : ; , . .. _ " + - / ^ ³`. Then the Greek by use (`θ τ σ ω α β φ μ ν λ ρ ε δ`), then the label
  symbols.
- The caret keys are ‹ and ›, not ← and →, because → is a symbol a label can hold and has its own key.

## How the page keeps still, per browser

What the page does:

1. **The viewport meta asks for `interactive-widget=resizes-content`.**
2. **The app is sized to the visual viewport.** It is `position: fixed`, its height is `visualViewport.height`, and it
   is moved down by `visualViewport.offsetTop`, all updated on the visual viewport's `resize` and `scroll`.
3. **A page that scrolled itself is scrolled back to 0.**
4. **A one-finger drag is cancelled unless it starts in something that scrolls:** the editor, a key row or the words
   list. Two fingers still zoom.
5. **`html` and `body` have `overflow: hidden` and `overscroll-behavior: none`,** and the editor has
   `overscroll-behavior: contain`.
6. **On a touch screen the editor's text is 16px,** because below that iOS zooms the page when a field takes focus.
   The playground's editor is 13px (`0.8125rem`), so it zooms there.

### What each browser does

- **iOS Safari (26.x)**
  - The keyboard shrinks only the visual viewport. The layout viewport, and with it `dvh`, `svh` and `lvh`, stay the
    same size ([web.dev](https://web.dev/blog/viewport-units),
    [explainer](https://github.com/bramus/viewport-resize-behavior/blob/main/explainer.md)).
  - Safari scrolls the page to the focused field even with `overflow: hidden`. This is deliberate
    ([WebKit 207425](https://bugs.webkit.org/show_bug.cgi?id=207425)).
  - Fixed elements stay with the layout viewport, so a keyboard covers them. That leaves the empty space Kris could
    scroll into, and it is why steps 2–4 exist.
  - It ignores `interactive-widget` before iOS 27.3. 27.3 is in beta with it
    ([WebKit 259770](https://bugs.webkit.org/show_bug.cgi?id=259770)): `overlays-content` and `resizes-visual` are
    reported working, and `resizes-content` reported broken.
  - Known bugs this may hit:
    - `visualViewport` scroll events come only when scrolling stops ([WebKit 218465](https://bugs.webkit.org/show_bug.cgi?id=218465)).
    - In iOS 26, `overflow: hidden` on html/body stopped preventing scroll ([WebKit 299084](https://bugs.webkit.org/show_bug.cgi?id=299084)).
    - After the keyboard is dismissed, `offsetTop` can stay non-zero ([WebKit 297779](https://bugs.webkit.org/show_bug.cgi?id=297779),
      [Apple forums](https://developer.apple.com/forums/thread/800125)).
  - The fix in step 2 is community practice, not something Apple documents.
- **Chrome on Android**
  - Since Chrome 108 the default is `resizes-visual`, which behaves as Safari does
    ([Chrome blog](https://developer.chrome.com/blog/viewport-resize-behavior)).
  - With `resizes-content`, which this page asks for, the layout viewport and `dvh` shrink with the keyboard, so the
    app fits on its own.
  - The VirtualKeyboard API (`navigator.virtualKeyboard`, `env(keyboard-inset-height)`) exists only in Chromium 94+
    ([MDN data](https://bcd.developer.mozilla.org/bcd/api/v0/current/api.VirtualKeyboard.json),
    [Chrome docs](https://developer.chrome.com/docs/web-platform/virtual-keyboard)). It has known bugs in its geometry
    ([write-up](https://zouhir-blog.pages.dev/blog/virtual-keyboard-api)), so the page doesn't use it.
- **Firefox on Android** reads `interactive-widget` from about 132/133
  ([Bugzilla 1916002](https://bugzilla.mozilla.org/show_bug.cgi?id=1916002)).
- **`overscroll-behavior`** works in Safari from iOS 16, with caveats for containers that don't overflow
  ([MDN data](https://bcd.developer.mozilla.org/bcd/api/v0/current/css.properties.overscroll-behavior.json)).
  `touch-action: none`, `pan-x` and `pan-y` work from iOS 13.

### What `inputmode="none"` costs

- **Support:** Safari 13 and later, Chrome Android 66 and later, Firefox Android 79 and later
  ([MDN data](https://bcd.developer.mozilla.org/bcd/api/v0/current/html.global_attributes.json)).
- **Hardware keyboard:** WebKit made it keep the keyboard down only when no hardware keyboard is attached
  ([r244220](https://trac.webkit.org/r244220), [WebKit 203061](https://bugs.webkit.org/show_bug.cgi?id=203061)). So a
  hardware keyboard still types, and the globe key still switches.
- **Switching while focused:** WebKit has a test that a focused field switched from `none` to `text` brings the
  keyboard up after the tap ([test](https://raw.githubusercontent.com/WebKit/WebKit/main/LayoutTests/pointerevents/ios/inputmode-change-update-keyboard-after-pointerup.html)).
  The hand-over still blurs and refocuses in the tap's own click, to be sure on both browsers.
- **What it costs: unconfirmed.** No source says whether the caret, long-press selection handles, the
  Copy/Paste/Select menu and iOS's bar over the keyboard still appear under `inputmode="none"`. They are separate from
  the keyboard, so they probably do, but this needs a phone.
- **What it costs in the lab's own design:**
  - The system keyboard's autocorrect, dictation and spacebar caret drag are gone while the Pencil keys are up. **system**
    gets them back for a while.
  - A paste needs the long-press menu, since the Pencil keys have no paste key.

## What was checked, and what wasn't

**Checked, in the desktop app's browser pane (Chromium) at 375×812 and 1633×919:**

- The page loads with no console errors.
- The page can't scroll: its scroll height is its height.
- At 375×812:
  - Every variant edits the document, and the figure follows: `0.25` became `0.5·…·2` and the circle grew.
  - A broken edit shows its error under the figure, and the last figure stays.
  - Keys type without taking focus from the editor.
  - The hand-over in C keeps the caret, and swaps the keys for the symbol row.
- At 1633×919, the figure is drawn at 1064×665 beside the editor and keys.
- With a hardware keyboard:
  - Text types into the editor.
  - Arrows move between keys, Enter presses one, and focus stays on that key.
  - `\theta` becomes θ.
  - Undo steps back through what keys and typing did.
- C's words in `circle` and `tangents`, at a statement, a selector, a property, a value and an expression.

**Not checked in the pane: anything about a real on-screen keyboard.** The pane has none; what Kris found on his
iPhone is in the next section. The iOS Simulator has Safari's real one,
but Kris had declined Claude's access to it, so this wasn't tried. On his phone:

1. Open `?keys=a&numbers`, tap the editor, and read the numbers.
   - Does `visual` shrink, and does `at` (the offset) go back to 0?
   - Does the symbol row sit right on the keyboard, or under iOS's own bar?
   - Can the page be dragged into empty space?
   - Repeat with `&pin=off`, and on iOS 27.3 with `&widget=overlays-content`.
2. Open `?keys=b` and `?keys=c`, and tap the editor. Check each of these:
   - The system keyboard stays down.
   - The caret shows.
   - A long press selects, with handles and the Copy/Paste menu.
   - Paste works.
   - A held ⌫ repeats.
   - **system** brings the keyboard up in one tap, and **Pencil keys** puts it away.
3. In B, type a whole line, `let r = 1 + 0.25·sin(time)`, and time it against A.
4. In C, try a style rule: `style c { width: 2px }`.

## On a real phone (iPhone, through a tunnel)

Kris tried the lab on his iPhone on 2026-11-18, through a tunnel to `serve.py`.

- **The shell:** the lab fixes the usability problems he had editing on the phone.
- **The Pencil keys** are much better than the system keyboard. He doesn't see why he'd use the system one.
- **C's words** are good.
- **One defect:** the words row changed height when it had nothing to suggest. The grid gave the difference to the figure
  and the editor, so the editor's text jumped and its scroll position moved.

**The fix.** The words row, and the symbol row like it, now have a set height (45px: a 36px key and the row's padding and
rule). An empty row says "No words to suggest here", in the muted colour. All four key pages are now declared the same
height, `4 × --key-h` plus their gaps. Before, the f(x) page took its height from its list and the others from their rows,
and the two differed by 0.02px.

**Measured** in the browser pane at 375×812, in C on `circle.pen`, typing with the Pencil keys at the end of the file:

| | Before | After |
| --- | --- | --- |
| Words row, full and empty | 45px, then 25.8px | 45px both |
| Editor top, full and empty | 227.0px, then 234.7px | 227.02px both |
| Editor height, full and empty | 287.8px, then 299.3px | 287.74px both |
| Editor `scrollTop`, full and empty | 76, then 64.5 | 76.5 both |

- **The words row:** in 30 steps, the row emptied and filled 15 times. It went from 24 words to empty, back to 24, to 3,
  and to empty, as `z`, ⌫, `s`, `q` and ⌫⌫ were typed. The editor's box, its `scrollTop` and the caret's place on
  screen didn't change.
- **The key pages:** 123, abc, f(x) and αβγ, with Greek and letters shifted, were turned through three times. The keys
  stayed 252.23px tall, and the editor's top, height and `scrollTop` didn't change. `scrollTop` changed only when typing
  scrolled to the caret.
- **Hiding the keys with ▾:** this changes the room, by design. The keys and the words row go, the figure and the editor
  share the 297px freed, and the editor's top moves from 227px to 346px. Showing the keys again puts everything back
  exactly. Whether the figure should keep its size, so that nothing above the editor moves, is Kris's call.
- **The hand-over to the system keyboard** swaps the keys for the symbol row. That changes the room too, under a keyboard
  that is itself changing it.

**Still unconfirmed on the phone.** Kris hasn't reported on these yet:

- The selection handles under `inputmode="none"`.
- Copy and paste: the long-press menu.
- The **system** key: whether it brings the keyboard up in one tap, and whether **Pencil keys** puts it away.
- The checklist's numbers: the viewport readings, `&pin=off`, and `widget=overlays-content` on iOS 27.3.

## Reaching the page from a phone

`scripts/serve.py` listens on 127.0.0.1 only, so a phone can't reach it as it is. The ways:

- **A tunnel**, as `serve.py`'s own notes say: `cloudflared tunnel --url http://localhost:8741`.
  - It gives an https address, which also works for things that need https.
  - Anyone with the address can read the whole repo while it runs.
- **The local network.** Serve on `0.0.0.0` and open `http://<the Mac's address>:8741/lab/keyboard/` on the same Wi-Fi.
  - That needs `serve.py` to take a bind address, or another server, and it exposes the repo to everyone on that network.
- **Tailscale or another private network** between the Mac and the phone. It's like the local network, but only his
  own devices can reach it.
- **Android only:** USB and `adb reverse tcp:8741 tcp:8741`, or Chrome's `chrome://inspect` port forwarding.
  `localhost` on the phone is then the Mac, and nothing listens on a network. iOS has no equivalent.
- **The iOS Simulator** on the Mac reaches `localhost` directly and has Safari's real keyboard (⇧⌘K turns off the Mac's
  keyboard). It's close to a phone for layout, but it isn't his phone or his hands.
