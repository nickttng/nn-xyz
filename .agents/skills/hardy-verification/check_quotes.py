#!/usr/bin/env python3
"""Check the quotations in a note against the full text of Hardy's novels.

    python3 .agents/skills/hardy-verification/check_quotes.py content/the-trumpet-major.md

Reads the note, never writes to it. Texts come from Project Gutenberg and are
kept in a temp folder, outside the repo. Run with --quiet to list only problems.

Each quotation gets one result:
    EXACT       words and punctuation match, and the chapter is shown
    WORDS       the words match but punctuation or capitals differ
    NOT FOUND   the words aren't in any novel; the nearest passage is shown

A Gutenberg text is one edition, and Hardy revised heavily, so NOT FOUND means
"check a scholarly edition", not "wrong". See SKILL.md.
"""
import difflib
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# Project Gutenberg ebook numbers, checked against its catalogue.
NOVELS = {
    "Desperate Remedies": 3044,
    "Under the Greenwood Tree": 2662,
    "A Pair of Blue Eyes": 224,
    "Far from the Madding Crowd": 107,
    "The Hand of Ethelberta": 3469,
    "The Return of the Native": 122,
    "The Trumpet-Major": 2864,
    "A Laodicean": 3258,
    "Two on a Tower": 3146,
    "The Mayor of Casterbridge": 143,
    "The Woodlanders": 482,
    "Tess of the d'Urbervilles": 110,
    "Jude the Obscure": 153,
    "The Well-Beloved": 3326,
}

CACHE = Path(tempfile.gettempdir()) / "hardy-texts"
MIN_INLINE_WORDS = 4  # shorter inline quotes are too common to prove anything
CHAPTER = re.compile(r"^\s*(CHAPTER|Chapter|BOOK|PHASE)\b.*|^\s*[IVXLC]+\.?\s*(—.*|[A-Z ,'’-]+)?$")


def load(title, number):
    """Return a novel's text, downloading it the first time."""
    path = CACHE / f"pg{number}.txt"
    if not path.exists():
        CACHE.mkdir(exist_ok=True)
        url = f"https://www.gutenberg.org/cache/epub/{number}/pg{number}.txt"
        subprocess.run(["curl", "-sSfL", "--max-time", "60", url, "-o", str(path)], check=True)
    text = path.read_text(encoding="utf-8-sig")
    start = re.search(r"\*\*\* ?START OF.*", text)
    end = re.search(r"\*\*\* ?END OF", text)
    return text[start.end() if start else 0 : end.start() if end else len(text)]


def tidy(text):
    """Make typography comparable: curly quotes, dashes, italics, and line breaks."""
    text = text.replace("’", "'").replace("‘", "'").replace("“", '"').replace("”", '"')
    text = re.sub(r"\s*(—|–|--)\s*", " — ", text)
    text = text.replace("_", "").replace("*", "")
    return re.sub(r"\s+", " ", text).strip()


def words(text):
    """Just the words, for matching that ignores punctuation and capitals."""
    return re.findall(r"[a-z0-9æœ']+", tidy(text).lower().replace("-", " "))


def quotations(path):
    """Yield (line number, quotation) for blockquotes and longer inline quotes."""
    body_started = False
    in_front_matter = False
    block, block_line = [], 0
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if line.strip() == "---" and not body_started:
            in_front_matter = not in_front_matter
            body_started = not in_front_matter
            continue
        if not body_started:
            continue
        if line.startswith(">"):
            block_line = block_line or number
            block.append(line.lstrip("> "))
            continue
        if block:
            yield block_line, " ".join(block)
            block, block_line = [], 0
        for quote in re.findall(r'[“"]([^”"]+)[”"]', line):
            if len(words(quote)) >= MIN_INLINE_WORDS:
                yield number, quote
    if block:
        yield block_line, " ".join(block)


def fragments(quote):
    """Split on ellipses, since each side of a cut has to match on its own."""
    return [part.strip() for part in re.split(r"…|\.\.\.", quote) if len(words(part)) >= 2]


def chapter_at(raw, position):
    """The last chapter heading before a position in the text."""
    heading = ""
    for line in raw[:position].splitlines():
        if CHAPTER.match(line) and line.strip():
            heading = line.strip()
    return heading or "before the first chapter"


def nearest(fragment, library):
    """Find the closest passage, for a quotation that didn't match."""
    target = words(fragment)
    best = (0.0, "", "")
    size = min(5, len(target))
    for title, (_, _, book_words, starts) in library.items():
        seen = set()
        for offset in range(len(target) - size + 1):
            anchor = tuple(target[offset : offset + size])
            for hit in starts.get(anchor, []):
                begin = max(hit - offset, 0)
                if begin in seen:
                    continue
                seen.add(begin)
                window = book_words[begin : begin + len(target) + 5]
                score = difflib.SequenceMatcher(None, target, window).ratio()
                if score > best[0]:
                    best = (score, title, " ".join(window))
    return best


def check(fragment, library):
    """Return (result, detail) for one piece of a quotation."""
    # A closing full stop or comma is often the note's, not Hardy's.
    exact = tidy(fragment).rstrip(".,;:")
    plain = " " + " ".join(words(fragment)) + " "
    for title, (raw, flat, book_words, _) in library.items():
        if exact in flat[0]:
            # flat[1] maps back to the raw text so the chapter can be named.
            position = flat[1][flat[0].index(exact)]
            return "EXACT", f"{title} — {chapter_at(raw, position)}"
    for title, (raw, flat, book_words, _) in library.items():
        if plain in " " + " ".join(book_words) + " ":
            return "WORDS", title
    score, title, passage = nearest(fragment, library)
    if title:
        return "NOT FOUND", f"nearest ({score:.0%}), {title}: “{passage}”"
    return "NOT FOUND", "no similar passage in any novel"


def flatten(raw):
    """Tidy a whole book, keeping a map from tidied positions to raw positions."""
    out, positions = [], []
    last_space = True
    for index, char in enumerate(raw):
        char = {"’": "'", "‘": "'", "“": '"', "”": '"', "_": "", "*": ""}.get(char, char)
        if not char:
            continue
        if char.isspace():
            if last_space:
                continue
            char, last_space = " ", True
        else:
            last_space = False
        out.append(char)
        positions.append(index)
    flat = "".join(out)
    # Dash spacing can change length, so apply it through the position map too.
    spaced, spaced_positions = [], []
    for index, char in enumerate(flat):
        if char in "—–":
            while spaced and spaced[-1] == " ":
                spaced.pop(), spaced_positions.pop()
            spaced.extend(" — "), spaced_positions.extend([positions[index]] * 3)
        elif char == " " and spaced and spaced[-1] == " ":
            continue
        else:
            spaced.append(char), spaced_positions.append(positions[index])
    return "".join(spaced), spaced_positions


def build_library():
    library = {}
    for title, number in NOVELS.items():
        raw = load(title, number)
        book_words = words(raw)
        starts = {}
        for index in range(len(book_words) - 4):
            starts.setdefault(tuple(book_words[index : index + 5]), []).append(index)
        library[title] = (raw, flatten(raw), book_words, starts)
    return library


def main():
    arguments = sys.argv[1:]
    quiet = "--quiet" in arguments
    notes = [Path(a) for a in arguments if a != "--quiet"]
    if not notes:
        sys.exit(__doc__)
    library = build_library()
    problems = 0
    for note in notes:
        for line, quote in quotations(note):
            for fragment in fragments(quote):
                result, detail = check(fragment, library)
                problems += result != "EXACT"
                if quiet and result == "EXACT":
                    continue
                short = fragment if len(fragment) < 70 else fragment[:67] + "..."
                print(f"{note.name}:{line}  {result}  “{short}”\n    {detail}")
    print(f"\n{problems} to look at. Gutenberg is one edition: see SKILL.md before changing a quotation.")


if __name__ == "__main__":
    main()
