"""Times a figure's beats from its narration.

A narration script is plain text with anchors. `[mirror]` before a word means the beat named
`mirror` finishes arriving on that word's stressed syllable, and `[trace:start]` means the beat
`trace` starts there. Any recording of the script can time the beats: a synthesized draft while the
words are still changing, and a real recording at the end. Both are timed the same way, by forcing
an alignment of the known text to the audio, so the beats never depend on which voice spoke.

Each line of a script is a clip. Within a line the voice sets the time, and between lines the
figure can make it wait: a line starts late when a beat it anchors couldn't arrive in time, and
when the figure holds a landing. A landing's hold is the pause written after its beat in the `.pen`,
and the next line's first word waits for it.

    pixi run python narrate.py labels ../explainer/limacon.pen limacon.txt
    pixi run python narrate.py say limacon.txt out/draft.wav
    pixi run python narrate.py hear out/draft.wav limacon.txt
    pixi run python narrate.py align out/draft.wav limacon.txt out/draft.json
    pixi run python narrate.py retime ../explainer/limacon.pen out/draft.json out/limacon.pen

`retime` writes the narration as it's placed in the story to `out/limacon.wav`. To watch it, from
the repo's root:

    cargo run --release -p pencil-cli -- video lab/narration/out/limacon.pen --style corpus/theme.style \\
        --style corpus/dark.style --style lab/explainer/limacon.style --style lab/explainer/caption.style \\
        --size 1280 --out out/narrated.mp4
    ffmpeg -i out/narrated.mp4 -i lab/narration/out/limacon.wav -map 0:v -map 1:a -af apad -shortest \\
        -c:v copy -c:a aac out/narrated-audio.mp4

It also writes `out/limacon.prompt.json`, each word's time in the story, for a teleprompter that
plays the figure and lights the words as they're due, to read a take along with it:

    http://localhost:8741/web/prompter.html?take=limacon

The prompter records too. `scripts/serve.py` keeps each take in `out/takes`, with the figure's time
it started at and the `.pen` it was read with, so it's in step with the figure as recorded:

    pixi run python narrate.py video out/takes/limacon-20261112-091500.json

`labels` holds a figure to Kris's rule that any point a caption or the narration names is labeled in
the figure, which is how a reader knows which letter was meant.

A real take goes through the same steps from `hear` on, in any format ffmpeg reads, such as a
phone's m4a. `words.dict` says how the narrator says words the aligner's dictionary lacks, in
ARPAbet, and the draft voice says them the same way.
"""

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ANCHOR = re.compile(r"\[(\w+)(?::(start))?\]")
VOWEL = re.compile(r"^[A-Z]+([012])$")  # ARPAbet vowels carry a stress digit.
GAP = 0.3  # Seconds of silence between a draft's lines, which `retime` can stretch.


def lines_of(text):
    return [line for line in text.splitlines() if line.strip()]


def words_of(text):
    """The words a script says, normalized the way the aligner writes them."""
    return [w for w in (normal(t) for t in ANCHOR.sub(" ", text).split()) if w]


def normal(word):
    return re.sub(r"[^\w']", "", word.lower().replace("’", "'"))


def anchors_of(text):
    """Each anchor, with the line it's in and the index of the word it's on."""
    out, index = [], 0
    for number, line in enumerate(lines_of(text)):
        for token in re.split(r"(\[[^\]]+\])|\s+", line):
            if not token:
                continue
            m = ANCHOR.fullmatch(token)
            if m:
                out.append({"beat": m[1], "edge": m[2] or "end", "word": index, "line": number})
            elif normal(token):
                index += 1
    return out


def say(script, wav, voice="am_michael", speed=1.0):
    """Synthesizes a draft with Kokoro, a line at a time, and keeps Kokoro's own word times to
    compare."""
    import numpy as np
    import soundfile
    from kokoro import KPipeline

    pipeline = KPipeline(lang_code="a", repo_id="hexgrad/Kokoro-82M")
    rate, audio, words, offset = 24000, [], [], 0.0
    for line in lines_of(Path(script).read_text()):
        # Two spaces where an anchor was make Kokoro say the next spelled-out word twice.
        text = " ".join(ANCHOR.sub("", line).split())
        # Kokoro's own guesses read `sinh` as "sin", so the words the narrator has said how to
        # say are spelled out for it, from the same dictionary the aligner uses.
        for word, arpa in own_words().items():
            text = re.sub(rf"\b{re.escape(word)}\b", f"[{word}](/{kokoro_phonemes(arpa)}/)", text,
                          flags=re.IGNORECASE)
        for result in pipeline(text, voice=voice, speed=speed):
            chunk = result.audio.numpy()
            for token in result.tokens or []:
                if normal(token.text) and token.start_ts is not None:
                    words.append([offset + token.start_ts, offset + token.end_ts,
                                  normal(token.text)])
            audio.append(chunk)
            offset += len(chunk) / rate
        audio.append(np.zeros(int(GAP * rate), dtype=np.float32))
        offset += GAP
    Path(wav).parent.mkdir(parents=True, exist_ok=True)
    soundfile.write(wav, np.concatenate(audio), rate)
    Path(wav).with_suffix(".kokoro.json").write_text(json.dumps(words))
    print(f"{wav}: {offset:.2f} s, {len(words)} words timed by Kokoro")


# Words MFA's English dictionary doesn't have, said the way the narrator says them.
WORDS = Path(__file__).with_name("words.dict")


def own_words():
    words = {}
    for line in WORDS.read_text().splitlines():
        if line.strip():
            word, arpa = line.split("\t", 1)
            words[word] = arpa.split()
    return words


# ARPAbet to the phonemes Kokoro reads (misaki's American set), where a diphthong is one letter.
VOWELS = {"AA": "ɑ", "AE": "æ", "AO": "ɔ", "AW": "W", "AY": "I", "EH": "ɛ", "EY": "A", "IH": "ɪ",
          "IY": "i", "OW": "O", "OY": "Y", "UH": "ʊ", "UW": "u"}
CONSONANTS = {"B": "b", "CH": "ʧ", "D": "d", "DH": "ð", "F": "f", "G": "ɡ", "HH": "h", "JH": "ʤ",
              "K": "k", "L": "l", "M": "m", "N": "n", "NG": "ŋ", "P": "p", "R": "ɹ", "S": "s",
              "SH": "ʃ", "T": "t", "TH": "θ", "V": "v", "W": "w", "Y": "j", "Z": "z", "ZH": "ʒ"}


def kokoro_phonemes(arpa):
    out = ""
    for phone in arpa:
        base, stress = phone.rstrip("012"), phone[len(phone.rstrip("012")):]
        mark = {"1": "ˈ", "2": "ˌ"}.get(stress, "")
        if base == "AH":
            out += mark + ("ʌ" if mark else "ə")
        elif base == "ER":
            out += mark + ("ɜɹ" if mark else "əɹ")
        elif base in VOWELS:
            out += mark + VOWELS[base]
        else:
            out += CONSONANTS[base]
    return out


def english_dictionary():
    found = subprocess.run(["mfa", "model", "inspect", "dictionary", "english_us_arpa"],
                           capture_output=True, text=True).stdout
    for path in re.findall(r"(/\S+\.dict)", found):
        return Path(path)
    return Path.home() / "Documents/MFA/pretrained_models/dictionary/english_us_arpa.dict"


def align(take, script, out):
    """Aligns the script to a take with MFA, and times each anchor and each line."""
    import soundfile

    text = Path(script).read_text()
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    # The take as a WAV, which both the aligner and `retime` read. A phone's m4a, or anything
    # else ffmpeg reads, is converted.
    wav = Path(take) if Path(take).suffix.lower() == ".wav" else out.with_suffix(".take.wav")
    if wav != Path(take):
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", take, "-ac", "1", str(wav)],
                       check=True)
    with tempfile.TemporaryDirectory() as tmp:
        corpus, aligned = Path(tmp, "corpus"), Path(tmp, "aligned")
        corpus.mkdir()
        (corpus / "take.wav").symlink_to(wav.resolve())
        (corpus / "take.lab").write_text(" ".join(words_of(text)))
        dictionary = Path(tmp, "words.dict")
        dictionary.write_text(english_dictionary().read_text() + WORDS.read_text())
        subprocess.run(
            ["mfa", "align", "--clean", "--single_speaker", "--output_format", "json",
             str(corpus), str(dictionary), "english_us_arpa", str(aligned)],
            check=True, capture_output=True,
        )
        tiers = json.loads((aligned / "take.json").read_text())["tiers"]
        oovs = [p.read_text().split() for p in aligned.glob("**/oovs_found*.txt")]
    words = [w for w in tiers["words"]["entries"] if w[2] not in ("", "<eps>")]
    phones = tiers["phones"]["entries"]
    expected = words_of(text)
    said = [w[2] for w in words]
    # The aligner can split a last word that trails off into silence in two. Only what follows the
    # script's end is let go.
    if len(said) > len(expected) and said[: len(expected)] == expected:
        print(f"let go of {len(said) - len(expected)} word(s) past the end: {said[len(expected):]}")
        words, said = words[: len(expected)], expected
    if said != expected:
        sys.exit(f"the aligner heard different words:\n  script {expected}\n  aligned {said}")

    # Where each line's speech starts and ends.
    spans, index = [], 0
    for line in lines_of(text):
        count = len(words_of(line))
        spans.append([words[index][0], words[index + count - 1][1]])
        index += count
    anchors = []
    for anchor in anchors_of(text):
        start, end, word = words[anchor["word"]]
        anchors.append({**anchor, "text": word, "at": stressed(phones, start, end)})
    info = soundfile.info(str(wav))
    result = {"audio": str(wav.resolve()), "duration": info.frames / info.samplerate, "script": text,
              "lines": spans, "words": words, "anchors": anchors, "oov": sum(oovs, [])}
    out.write_text(json.dumps(result, indent=1))
    for a in anchors:
        print(f"{a['beat']:>13} {a['edge']:<5} {a['at']:6.3f} s  on '{a['text']}'")
    if result["oov"]:
        print("not in the dictionary:", ", ".join(result["oov"]))
    compare(take, words)


def stressed(phones, start, end):
    """When a word's stressed vowel starts: primary stress, else secondary, else any vowel. A
    beat lands on a vowel's onset, which is close to where a listener hears the syllable fall."""
    vowels = [(VOWEL.match(p[2])[1], p[0]) for p in phones
              if start - 1e-6 <= p[0] < end and VOWEL.match(p[2])]
    for rank in "120":
        for digit, at in vowels:
            if digit == rank:
                return at
    return start


def hear(wav, script):
    """What a recognizer hears, set against the script. The aligner can't do this: given the
    script, it finds a place for every word even when the take says something else, as when
    Kokoro said a word twice. A recognizer mishears math words, so read its differences, don't
    count them."""
    import difflib

    import whisper

    result = whisper.load_model("small.en").transcribe(wav, word_timestamps=True)
    heard = [normal(w["word"]) for s in result["segments"] for w in s["words"]]
    wanted = words_of(Path(script).read_text())
    for op, a1, a2, b1, b2 in difflib.SequenceMatcher(None, wanted, heard).get_opcodes():
        if op != "equal":
            print(f"script: {' '.join(wanted[a1:a2]) or '-':<30} heard: {' '.join(heard[b1:b2]) or '-'}")
    print(f"{len(heard)} words heard, {len(wanted)} in the script")


def compare(take, words):
    """How far Kokoro's own word times are from the aligner's, when there are any."""
    path = Path(take).with_suffix(".kokoro.json")
    if not path.exists():
        return
    kokoro = json.loads(path.read_text())
    if [w[2] for w in kokoro] != [w[2] for w in words]:
        print("Kokoro split the words differently, so its times weren't compared")
        return
    gaps = [abs(k[0] - a[0]) for k, a in zip(kokoro, words)]
    print(f"Kokoro's word starts vs the aligner's: mean {1000 * sum(gaps) / len(gaps):.0f} ms, "
          f"worst {1000 * max(gaps):.0f} ms")


# A point's name in words: a capital letter, maybe primed, or a number standing alone.
POINT = re.compile(r"(?<![\w'’])([A-Z][′']?|\d+)(?![\w′])")
NUMBERS = {"zero": "0", "one": "1", "two": "2", "three": "3"}


def unlabeled(pen, script):
    """Points a caption or the narration names that the figure doesn't label. What's said should
    be seen: a label is how a reader knows which letter was meant, and it settles letters that
    sound alike, as C and E do."""
    text = Path(pen).read_text()
    labels = set(re.findall(r'^\s*label\s+\w+\s*=\s*"([^"]*)"', text, re.M))
    said = [("caption", c) for c in re.findall(r'^beat\s+(?:\w+\s+)?"([^"]+)"', text, re.M)]
    said += [("narration", ANCHOR.sub("", line)) for line in lines_of(Path(script).read_text())]
    missing = []
    for where, words in said:
        names = []
        for m in POINT.finditer(words):
            # "A circle" and "I" are words, unless a capital follows nothing lowercase.
            after = words[m.end():].lstrip()
            if m[1] in ("A", "I") and after[:1].islower():
                continue
            # A number in an equation, as in `p = 0`, `θ/2` or `360°`, is a value, not a point.
            before = words[:m.start()].rstrip()
            if m[1][0].isdigit() and (before[-1:] in "=/(−-+·×^" and before or after[:1] in "°/)^·"):
                continue
            names.append(m[1])
        if where == "narration":
            # A number before a single letter is a coefficient, as in "two t", not a point.
            names += [NUMBERS[w] for w in re.findall(r"\b([a-z]+)\b(?!\s+[a-z]\b)", words.lower())
                      if w in NUMBERS]
        missing += [(where, name, words.strip()) for name in names if name not in labels]
    return missing


def labels(pen, script):
    missing = unlabeled(pen, script)
    for where, name, words in missing:
        print(f"{name!r} has no label, but the {where} names it: {words}")
    if not missing:
        print("every point a caption or the narration names has a label")
    return not missing


ITEM = re.compile(r"^(pause|beat)\s+(?:(\w+)\s+(?:\"[^\"]*\"\s+)?for\s+([\d.]+)|([\d.]+))")


def retime(pen, timing, out):
    """Places each line of a take in the story, and works out the waits that put each anchored
    beat on its word. Beat lengths stay as written, since they're how fast a move reads."""
    import numpy as np
    import soundfile

    take = json.loads(Path(timing).read_text())
    spans, anchors = take["lines"], take["anchors"]
    count = len(spans)
    # A line's clip runs from halfway through the silence before it to halfway through the one
    # after, so a take's own breaths and pauses stay as the least a gap can be.
    cuts = [0.0] + [(spans[i][1] + spans[i + 1][0]) / 2 for i in range(count - 1)]
    cuts.append(take["duration"])

    text = Path(pen).read_text().splitlines()
    items = []  # (line of the file, kind, name, seconds)
    for i, line in enumerate(text):
        m = ITEM.match(line)
        if m:
            items.append((i, m[1], m[2], float(m[3] or m[4])))
    index = {it[2]: k for k, it in enumerate(items) if it[1] == "beat"}
    missing = [a["beat"] for a in anchors if a["beat"] not in index]
    if missing:
        sys.exit(f"no beat named {', '.join(missing)}")
    by_beat = {a["beat"]: a for a in anchors}
    # How long the figure holds a landing: the pause written right after its beat.
    def hold(beat):
        k = index[beat] + 1
        return items[k][3] if k < len(items) and items[k][1] == "pause" else 0.0

    def play(starts):
        """The story with the lines placed so far: each anchored beat's wait, how late each line's
        beats are, and when each beat ends."""
        waits, late, ends, clock = {}, [0.0] * count, {}, 0.0
        for k, (_, kind, name, seconds) in enumerate(items):
            anchored = lambda it: it[1] == "beat" and it[2] in by_beat and \
                by_beat[it[2]]["line"] < len(starts)
            if kind == "pause":
                if k + 1 < len(items) and anchored(items[k + 1]):
                    continue  # The wait before an anchored beat replaces it.
                clock += seconds
                continue
            if anchored(items[k]):
                a = by_beat[name]
                start = starts[a["line"]] + a["at"] - cuts[a["line"]]
                if a["edge"] == "end":
                    start -= seconds
                wait = start - clock
                late[a["line"]] = max(late[a["line"]], -wait)
                waits[k] = max(wait, 0.0)
                clock += waits[k]
            clock += seconds
            ends[name] = clock
        return waits, late, ends

    starts = []
    for line in range(count):
        start = 0.0 if line == 0 else starts[-1] + cuts[line] - cuts[line - 1]
        if line > 0:
            # The figure holds each landing of the line before for as long as the `.pen` says,
            # before this line's first word.
            _, _, ends = play(starts)
            first_word = spans[line][0] - cuts[line]
            for a in anchors:
                if a["line"] == line - 1:
                    start = max(start, ends[a["beat"]] + hold(a["beat"]) - first_word)
        for _ in range(50):
            _, late, _ = play(starts + [start])
            if late[line] < 1e-6:
                break
            start += late[line]
        starts.append(start)

    waits, _, ends = play(starts)
    natural = [cuts[i] for i in range(count)]
    print(f"{'line':>4} {'starts':>7} {'held':>6}  first words")
    lines = lines_of_script(take)
    for i in range(count):
        print(f"{i + 1:>4} {starts[i]:7.2f} {starts[i] - natural[i]:6.2f}  {lines[i]}")
    print(f"\n{'beat':>13} {'lands':>7} {'pause was':>9} {'pause now':>9}")
    for k, wait in waits.items():
        _, _, name, seconds = items[k]
        was = items[k - 1][3] if k > 0 and items[k - 1][1] == "pause" else 0.0
        print(f"{name:>13} {ends[name] if by_beat[name]['edge'] == 'end' else ends[name] - seconds:7.2f} "
              f"{was:9.2f} {wait:9.2f}")

    # The `.pen` with those waits.
    replaced = {items[k - 1][0] for k in waits if k > 0 and items[k - 1][1] == "pause"}
    rewritten = []
    for i, line in enumerate(text):
        if i in replaced:
            continue
        k = next((k for k in waits if items[k][0] == i), None)
        if k is not None and waits[k] > 0.005:
            rewritten.append(f"pause {waits[k]:.2f}")
        rewritten.append(line)
    Path(out).write_text("\n".join(rewritten) + "\n")

    # The narration as placed in the story.
    audio, rate = soundfile.read(take["audio"], dtype="float32")
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    track = np.zeros(int((starts[-1] + cuts[-1] - cuts[-2]) * rate) + 1, dtype=np.float32)
    for i in range(count):
        clip = audio[int(cuts[i] * rate) : int(cuts[i + 1] * rate)]
        at = int(starts[i] * rate)
        track[at : at + len(clip)] += clip[: len(track) - at]
    soundfile.write(Path(out).with_suffix(".wav"), track, rate)

    # Each word as the script writes it, and when it's said in the story, for the prompter.
    prompt, k = [], 0
    for i, line in enumerate(lines_of(take["script"])):
        row = []
        for token in ANCHOR.sub(" ", line).split():
            if normal(token):
                start, end, _ = take["words"][k]
                k += 1
                row.append([token, round(starts[i] + start - cuts[i], 3),
                            round(starts[i] + end - cuts[i], 3)])
            elif row:
                row[-1][0] += " " + token  # A dash or the like goes with the word before it.
        prompt.append(row)
    Path(out).with_suffix(".prompt.json").write_text(
        json.dumps({"duration": round(len(track) / rate, 3), "lines": prompt}))
    print(f"\nwrote {out}, {Path(out).with_suffix('.wav')} ({len(track) / rate:.1f} s) and "
          f"{Path(out).with_suffix('.prompt.json')}")


REPO = Path(__file__).resolve().parents[2]


def video(*abouts):
    """Takes the prompter recorded, on the figure they were read with. Each starts at a known time
    in the figure, so it's placed there as it is, with nothing retimed. Several takes make one
    narration, as when a reader stops partway and starts again from a later word: each runs until
    the next one starts. An `.mp4` given last is where the video goes."""
    out = None
    if abouts and abouts[-1].endswith(".mp4"):
        *abouts, out = abouts
    abouts = sorted((Path(a).resolve() for a in abouts),
                    key=lambda a: json.loads(a.read_text())["start"])
    takes = [json.loads(a.read_text()) for a in abouts]
    pens = {a.with_suffix(".pen").read_text() for a in abouts if a.with_suffix(".pen").exists()}
    if len(pens) > 1:
        sys.exit("those takes were read with different timings, so they can't share a video")
    about, take = abouts[0], takes[0]
    pen = about.with_suffix(".pen")
    if not pen.exists():
        pen = Path(__file__).with_name("out") / f"{take['figure']}.pen"
    out = Path(out).resolve() if out else REPO / "out" / f"{about.stem}.mp4"
    out.parent.mkdir(parents=True, exist_ok=True)
    # The sheets are as the page named them, from web/.
    sheets = [str((REPO / "web" / s).resolve()) for s in take["sheets"].split()]
    silent = out.with_suffix(".silent.mp4")
    subprocess.run(["cargo", "run", "--release", "-q", "-p", "pencil-cli", "--", "video", str(pen),
                    *(a for s in sheets for a in ("--style", s)), "--size", "1280",
                    "--out", str(silent)], cwd=REPO, check=True)
    # Each take cut where the next begins, delayed to its start, and the lot mixed with no change
    # of level.
    inputs, parts = [], []
    for k, (a, t) in enumerate(zip(abouts, takes)):
        inputs += ["-i", str(a.with_name(t["recording"]))]
        cut = f"atrim=0:{takes[k + 1]['start'] - t['start']:.3f}," if k + 1 < len(takes) else ""
        parts.append(f"[{k + 1}:a]aformat=sample_rates=48000:channel_layouts=mono,{cut}"
                     f"adelay={round(1000 * t['start'])}:all=1[a{k}]")
    mix = "".join(f"[a{k}]" for k in range(len(takes)))
    graph = ";".join(parts) + f";{mix}amix=inputs={len(takes)}:normalize=0,apad[out]"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(silent), *inputs,
                    "-filter_complex", graph, "-map", "0:v", "-map", "[out]", "-shortest",
                    "-c:v", "copy", "-c:a", "aac", str(out)], check=True)
    silent.unlink()
    spans = ", ".join(f"{t['start']:.1f}–{t.get('end', 0):.1f} s" for t in takes)
    print(f"wrote {out}, with the takes at {spans} in the figure")


def lines_of_script(take):
    """Each line's first few words, from the aligned words, to name it."""
    out, words = [], take["words"]
    for start, _ in take["lines"]:
        k = next(i for i, w in enumerate(words) if w[0] == start)
        out.append(" ".join(w[2] for w in words[k : k + 5]))
    return out


if __name__ == "__main__":
    command, *args = sys.argv[1:]
    run = {"say": say, "hear": hear, "align": align, "retime": retime, "labels": labels,
           "video": video}[command]
    if run(*args) is False:
        sys.exit(1)
