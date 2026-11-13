# Test corpora for a math stack

Can one existing suite certify that an implementation "fully satisfies LaTeX"?
No. This page says why, lists what exists with sizes and licences, and defines
the layered harness that replaces the idea of a single conformance suite.

## There is no LaTeX conformance standard for rendering

Three separate obstacles, each fatal on its own:

1. **No standard defines rendered math.** TeX's behaviour is defined by an
   implementation (`tex.web`) plus a font's metrics; engines implement subsets
   and deliberately disagree. The same 16-formula corpus measured here differs
   by −22% to +45% in width between mathtex and formulary on the same font.
   A suite with expected output must therefore name its engine, and cannot be
   the standard for another.
2. **Completeness is distributional, not enumerable.** TeX's macro language is
   Turing-complete; there is no finite set of formulas whose success proves the
   next one. Coverage can only be measured over a corpus and a construct list.
3. **Most published expectations are tied to a renderer.** KaTeX's are CSS/HTML
   from its bundled fonts; MathJax's are MathML/CHTML; Typst's are its own
   frames. None is directly comparable to a third engine's geometry.

What can be certified is narrower and honest: *this engine parses and lays out
this declared subset on this corpus, within these tolerances, with no panics,
and the subset is listed construct by construct.*

## Existing suites, with what each is good for

| Suite | Licence | Size (counted 2026-11-07) | Good for | Not good for |
| --- | --- | --- | --- | --- |
| KaTeX tests | MIT | `katex-spec.ts` 190 KB, ~649 `it`/`it.each` blocks; `mathml-spec.ts` 35; 128 screenshot groups (`test/screenshotter/ss_data.yaml`); `docs/supported.md` ~530 `\command` tokens | Construct checklist, parser edge cases, function coverage table | Expected HTML/CSS and KaTeX fonts; no geometry for our engine |
| MathJax tests (`mathjax/mathjax-tests`) | Apache-2.0 | 100 JSON files, 1,371 `input`/`expected` cases (3 entries unreadable) | Second, broader syntax checklist; AMS, colour, cancel, bussproofs packages | Expectations are MathML/CHTML |
| Typst math tests (`tests/suite/math`) | Apache-2.0 | 26 `.typ` files, ~331 cases, including a port of the Mozilla MathML tests | How a modern native engine defines each construct; stretch/alignment cases | Typst syntax, not LaTeX; frames, not shared geometry |
| web-platform-tests `mathml/` | BSD-3-Clause | 755 HTML + 90 XHTML reftests | MathML Core rendering and stretch structure as a structural oracle | No LaTeX input at all |
| texmath tests | **GPL-2.0** | large; AMS extensions and macros | — | Licence is incompatible with copying into an MIT/Apache project; do not use as a data source |

## Corpora for scale and robustness

| Corpus | Size | Licence / access | Use |
| --- | --- | --- | --- |
| im2latex-100k | ~100k formulas (HF export 67.9k rows) | CC0-1.0; arXiv/Cornell KDD Cup lineage | Public, redistributable regression corpus |
| ARQMath (CLEF) | ~28M LaTeX formulas advertised, with LaTeX/Presentation MathML/Content MathML indices | registration; MSE content CC BY-SA | Parse-rate measurement; do not redistribute |
| arXMLiv / arXiv sources | millions of documents | per-paper arXiv licences | CI-time sampling only; never vendored |
| UniMERNet and similar formula sets | ~1M formulas | check each card | Optional breadth; licence audit first |

Rule: corpora that cannot be redistributed are fetched in CI, never committed;
committed samples carry provenance and a licence note per source.

## The harness we should build

Five gates, each with an explicit number. The numbers below are proposals to be
ratified at M2, not measurements.

| Gate | What it runs | Proposed threshold |
| --- | --- | --- |
| L1 parse | Every function in KaTeX `supported.md`; MathJax JSON inputs; im2latex-100k | 100% of the declared subset parses; MathJax inputs ≥95% with no error node; im2latex ≥99.5% with zero diagnostics; 100% no panics |
| L2 structure | Round-trip `parse(write(parse(s)))`; span invariants | Structural equality on the supported subset; every byte covered by a leaf span, a diagnostic, or a macro-expansion mark |
| L3 geometry | 16-formula corpus plus a 1k sample, laid out by this stack and formulary on the same font bytes | Width within 1% or 0.5 pt, height within 1 pt; every divergence listed with its rule choice |
| L4 visual | Self-owned screenshots per construct, display and text styles | No regression beyond a stated pixel tolerance; not compared against another engine |
| L5 robustness | Fuzz over arbitrary bytes and every prefix of valid formulas; macro/expansion budgets | No panic, no hang, bounded memory; a diagnostic for every failure |

The construct table is the completeness claim. It starts from KaTeX's function
list, and every entry is marked `supported`, `partial` or `unsupported`; the
unsupported list is published with the library. "Complete" then means: the
published table, the declared thresholds, and no silent gaps between them.

## What the harness cannot prove

It cannot prove conformance to a standard that does not exist, and it cannot
prove the next formula from an unseen paper will work. If the product goal is
"papers typeset correctly", the acceptance metric is a parse-and-layout success
rate on a sampled arXiv corpus plus human spot checks of rendered output — which
is what L1 and L3 measure, made measurable.

## See also

- [Target architecture](./architecture.md): the validation section this expands.
- `raw/algorithm.md`: the rule tables the geometry gate compares against.
