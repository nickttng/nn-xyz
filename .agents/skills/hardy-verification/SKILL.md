---
name: hardy-verification
description: How to verify and validate writing about Thomas Hardy's novels and Wessex. Use when a note tagged thomas-hardy is added or changed, when dating a novel or filling in the Wessex as a World reading plan, or when checking a quotation, plot detail, place name, publication date, or scholarly claim about Hardy. Pairs with the hardy-scholar role.
---

# Verifying writing about Hardy

The rule behind every check here: **a claim about Hardy is only verified if you have the source in front of you.** Memory is not a source, and that includes an AI model's memory. Fluent recall of Hardy produces plausible misquotations and invented citations. If a source can't be opened, the claim is unverified, and the report says so.

Check facts, not taste. A reading of a novel can be strained or disputed without being wrong. See "Readings" below.

## Which source settles what
Use the highest source that applies, and name it in the report.

1. **Hardy's text in a named edition:** settles quotations, plot, names, and what the narrator says.
2. **Hardy's own framing:** prefaces, postscripts, and dated section headings. Settles what Hardy said about a book, not necessarily what is true of it.
3. **Scholarly editions and standard references** (listed below): settle publication history, textual variants, and accepted chronology.
4. **Peer-reviewed articles:** settle what a named scholar argued, not whether they are right.
5. **Reputable secondary sites** (Victorian Web, Thomas Hardy Society, Wikipedia): good leads. Follow them to something higher before calling a claim verified.

## The checks, in order

### 1. Quotations
Run the checker first. It reads the note and doesn't change it:

```sh
python3 .agents/skills/hardy-verification/check_quotes.py content/the-trumpet-major.md
python3 .agents/skills/hardy-verification/check_quotes.py --quiet content/*.md   # problems only
```

It checks blockquotes and inline quotes of four or more words against all fourteen novels, and treats each side of an ellipsis separately.

| Result | Means | Do this |
|---|---|---|
| `EXACT` | Words and punctuation match; the chapter is shown | Done. Compare the chapter with any location the note claims |
| `WORDS` | Words match; punctuation or capitals differ | Compare by hand. Often the note's own punctuation, or an edition difference |
| `NOT FOUND` | Not in these texts; the nearest passage is shown | Read the next list before calling it an error |

A `NOT FOUND` can be any of these, so identify which:
- **A misquotation.** The nearest passage is 80%+ alike and differs by a word or two.
- **An edition variant.** Hardy revised heavily. Check the owner's edition before proposing a change.
- **Someone else's words.** The owner's own speech ("what an exasperating cad"), or another author (Bespaloff, Weil). Expected; note it and move on.
- **A poem, story, preface, or *The Dynasts*.** Only the fourteen novels are loaded.

What it can't do: it skips inline quotes under four words, single-quoted phrases, and anything paraphrased. A match proves the words exist, not that they mean what the note says or are spoken by who the note says. Read the surrounding paragraph for every quotation that carries an argument.

### 2. Plot, people, and sequence
Search the text rather than remembering it. For each claim in the note, find the passage and read around it. Check who does what **to whom**, and in what order: inverted direction is an easy error, and it survives a quotation check. Example from *The Trumpet-Major*: at the farewell the soldiers do wish Anne well, but the "wishes" in Hardy's sentence about the seven are hers to them.

If a search finds nothing, try other words before concluding (`fire`, `burnt`, `flames`, `thatch`), then say what you searched. "I couldn't find it" is a valid result. "It isn't there" needs a full read of the likely chapters.

*The Trumpet-Major* has a second Hardy: Captain Hardy of the *Victory*. A search for "Hardy" isn't a search for the author.

### 3. Dates in the story
Hardy gave no master timeline, so every date is an argument. Build it from evidence, strongest first, and write the evidence down:

1. **Dated headings or letters in the text.** *Desperate Remedies* dates its own sections.
2. **Hardy's stated date.** The 1895 preface to *The Return of the Native* puts the events "between 1840 and 1850".
3. **Fixed history the action touches.** Trafalgar (October 1805) in *The Trumpet-Major*; the Royal visit in *The Mayor of Casterbridge*. Search for battles, reigns, railways, comets, and new laws. Gutenberg files can lack a preface: the Corn Laws appear nowhere in `#143`, so confirm that anchor in an edition with Hardy's preface.
4. **Calendar evidence.** Does a stated weekday fit a date? Test it, don't reason it:
   ```sh
   python3 -c "import datetime; print([y for y in range(1830,1880) if datetime.date(y,10,9).weekday()==6])"
   ```
   Calendars repeat, so a match rules years out but doesn't pick one.
5. **Cross-references between novels:** shared characters and their ages.
6. **A scholar's chronology.** Quote whose, and whether anyone disagrees.

Rules for writing dates down:
- Separate *when the opening is set* from *when the main action is*, and say which you mean.
- Hardy contradicts himself. Report the conflict rather than resolving it silently.
- A "fifty or sixty years ago" in a preface counts from the date of that preface, and describes customs, not plot.
- Use a railway in a novel as evidence only with the actual local date in hand.

**The same date must appear everywhere.** Compare the `era:` field of each note with its row in `wessex-as-a-world.md` and with what its prose says.

### 4. Publication history
Keep three dates apart: **written**, **serialised**, **published as a book**. Then check any "Hardy wrote this in…" against a reference, not against the publication date. Titles change too (*The Well-Beloved* was serialised in 1892 as *The Pursuit of the Well-Beloved*).

### 5. Places
Wessex names are not a simple code. Hardy moved and merged features, and said so in his prefaces.
- Say "Hardy's X, based on Y", and give the source for anything beyond the well-known pairs.
- Wessex and Egdon Heath have no exact real equivalent.
- Use only the table below as a starting point. Check `Melcombe Regis`/`Weymouth` wherever the exact town matters.

### 6. Claims about scholars
Confirm the article or book exists, and that it says what the note says. Open it, or say you only saw an abstract or a summary. A scholar's argument is written as theirs ("Weber dates…"), never as fact. Check the detail of what they cite against Hardy's text, since a mismatch in a name or a rank can undo the argument.

Never supply a citation you haven't opened, and never fill in a page number from memory.

### 7. Hardy's vocabulary and period usage
Period spellings and dialect in quotations stay exactly as they are ("quire", "'twas"). Check a Hardy term against the text before correcting it, and check any word that may have shifted meaning: `engaged` in *The Trumpet-Major* can mean busy.

## Readings
Interpretation isn't a fact to verify. Grade it on the evidence:
- **Grounded:** the text supports it. Say where.
- **Open:** the text allows it and allows others. Fine as written, especially with "perhaps" or "I".
- **Strained:** a passage works against it. Quote the passage and let the owner decide.

Never mark a reading wrong because critics prefer another. Disagreement with the note goes in as a suggestion for further thought.

## Editions
Name the edition behind any textual claim. The main versions of most novels are the serial, the first book edition, the 1895–6 Osgood, McIlvaine collected edition (revised), and the 1912 Macmillan Wessex Edition (revised again). Variants across these are real, so a quotation can be right in one and wrong in another.

Project Gutenberg is the working text here because it's free and searchable. It isn't a critical edition, and each file names its source in the header or not at all. Two points worth knowing:
- `#107` *Far from the Madding Crowd* is the 1895 revised text; `#27` is the 1874 first edition. Compare them for a free look at Hardy revising.
- *The Trumpet-Major* `#2864` is from a 1920 Macmillan printing, *Ethelberta* `#3469` from 1907, *Two on a Tower* `#3146` from 1923. *The Return of the Native* also has a second file, `#17500`.

For anything that matters, confirm in the **Cambridge Edition of the Novels and Stories of Thomas Hardy** (general editor Richard Nemesvari), which has the fullest apparatus. As listed on Cambridge Core at October 2026:

| Volume | Editor | Print |
|---|---|---|
| *Desperate Remedies* | Richard Nemesvari | 2019 |
| *Under the Greenwood Tree* | Simon Gatrell | 2019 |
| *The Woodlanders* | Alan Manford | 2019 |
| *The Return of the Native* | Tim Dolin | 2021 |
| *Life's Little Ironies* | Alan Manford | 2023 |
| *Wessex Tales* | N. H. Reeve, Richard Nemesvari | 2025 |
| *Far from the Madding Crowd* | Simon Gatrell | 2025 |
| *A Laodicean* | Dennis Taylor | 2026 |

Other novels aren't listed there yet, so use another annotated edition and say which. Recheck [the series page](https://www.cambridge.org/core/series/cambridge-edition-of-the-novels-and-stories-of-thomas-hardy/BCDB18CCD33A51E124D72DA23431CA70) before saying a volume doesn't exist.

## Reference shelf
Each of these was confirmed to exist, but its contents weren't read for this skill. Open it before citing it.
- Richard Little Purdy, *Thomas Hardy: A Bibliographical Study* (1954): publication history.
- Purdy and Michael Millgate (eds.), *The Collected Letters of Thomas Hardy*, 7 vols. (Clarendon, 1978–88); supplement *Further Letters* (Millgate and Keith Wilson).
- Millgate (ed.), *The Life and Work of Thomas Hardy* (Macmillan, 1984): Hardy's disguised autobiography, so treat it as his own account.
- Millgate, *Thomas Hardy: A Biography* (1982), later revised as *A Biography Revisited*.
- Simon Gatrell, *Hardy the Creator: A Textual Biography* (Clarendon, 1988): how the texts changed.
- Carl J. Weber, "Chronology in Hardy's Novels", *PMLA* 53.1 (1938), 314–20. Full text needs library access.
- Adrian Room, "The Case for Casterbridge: Thomas Hardy as Placename Creator", *Names* 37.1 (1989): [doi.org/10.1179/nam.1989.37.1.1](https://doi.org/10.1179/nam.1989.37.1.1).
- Hermann Lea's *Hardy Guides* and the Thomas Hardy Society's tour pamphlets, for topography.

The 1912 General Preface sorts the fiction under headings including "Novels of Character and Environment", "Romances and Fantasies", and "Novels of Ingenuity". Take the placement of a specific novel from the preface itself.

## Fixed points
Checked against the texts unless marked.

| Novel | Book | Gutenberg | Dating evidence in the text |
|---|---|---|---|
| *Desperate Remedies* | 1871 | 3044 | Dated headings and letters, 1835–36 through 1860s |
| *Under the Greenwood Tree* | 1872 | 2662 | 1896 preface: quires of "fifty or sixty years ago" |
| *A Pair of Blue Eyes* | 1873† | 224 | No year stated; railway terminus 14 miles off |
| *Far from the Madding Crowd* | 1874 | 107 | Tombstone: October 9, 18—, which fell on a Sunday in both 1836 and 1870 |
| *The Hand of Ethelberta* | 1876† | 3469 | No year stated in the story |
| *The Return of the Native* | 1878† | 122 | Preface: "between 1840 and 1850" |
| *The Trumpet-Major* | 1880 | 2864 | Trafalgar and its news; then "the following year", so the action reaches spring 1806 |
| *A Laodicean* | 1881† | 3258 | Subtitled "A Story of To-day" |
| *Two on a Tower* | 1882† | 3146 | Column built 1782; "comet of 1811" recalled |
| *The Mayor of Casterbridge* | 1886† | 143 | Opens "before the nineteenth century had reached one-third of its span" |
| *The Woodlanders* | 1887† | 482 | Aside on the thirties and 1840; needs a closer read |
| *Tess of the d'Urbervilles* | 1891 | 110 | No year stated in the story; needs a closer read |
| *Jude the Obscure* | 1895† | 153 | Preface dates the writing, not the action |
| *The Well-Beloved* | 1897 | 3326 | Spans decades; recalls the gale of 1824 |

† Year from general knowledge, not from the text; confirm in Purdy before relying on it.

| Hardy's name | Commonly identified with | Note |
|---|---|---|
| Casterbridge | Dorchester | |
| Budmouth | Weymouth | Budmouth-Regis is more exactly Melcombe Regis, across the river |
| Christminster | Oxford | Outside Wessex proper |
| Melchester | Salisbury | |
| Mellstock | Stinsford and the Bockhamptons | A composite |
| Overcombe | Sutton Poyntz | |
| Port Bredy | Bridport | Confirmed in Room's abstract |
| Anglebury | Wareham | Confirmed in Room's abstract |
| Sandbourne | Bournemouth | |
| Sherton Abbas | Sherborne | |
| Shaston | Shaftesbury | |
| Knollsea | Swanage | |

Unless noted, these come from Wikipedia's table, so confirm in Room's article before relying on one.

## Keeping this current
When a check settles something new, add it here with its evidence, and remove any † it clears. If a volume appears in the Cambridge Edition, update the table. The checker's texts live in a temp folder and never enter the repo.
