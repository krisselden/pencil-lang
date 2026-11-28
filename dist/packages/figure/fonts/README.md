# The fonts labels are set in

Libertinus 7.051, unmodified, from the [Libertinus Fonts project](https://github.com/alerque/libertinus), under the
SIL Open Font License 1.1 (`OFL.txt`):

| File | What it sets |
|---|---|
| `LibertinusSans-Regular.otf` | Labels with `font: sans`, the default. |
| `LibertinusSerif-Italic.otf` | Labels with `font: math`: names and notes in italics. |
| `LibertinusMath-Regular.otf` | A label's mathematics, `$…$`, and any character the other two lack, such as `ℝ`, `∇` and `↦`. |

They are the only copies. `<pencil-figure>` loads them from here (`src/labels.js`), the `pencil` command has them built
in (`crates/pencil-cli/src/text.rs`), and the core's build reads the math font's metrics for laying mathematics out
(`crates/pencil-core/build.rs`). So a poster, a video and the live figure set the same glyphs at the same places.

Replacing one means all three change together, and the core's layout follows the new math font's own constants. The
license asks that a modified font not keep the names it reserves ("Linux Libertine", "Biolinum", "STIX Fonts"), so a
subset would need another name. These are the files as released.
