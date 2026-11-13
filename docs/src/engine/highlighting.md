# Syntax Highlighting

`ui-highlight` turns source text into byte ranges with a class, and holds no
colour. A theme is the caller's `TokenClass → ink` match, which is why the
crate has no design tokens and can serve a code block, a diff and a log view
equally.

## Output

`tokens(language, source)` returns `Result<Arc<[Token]>, Error>`, each token a
range in UTF-8 bytes plus its class. Ranges are in source order and do not
overlap, so a caller cuts the text it already has instead of copying it, and a
run's range indexes the same bytes a caret or an edit would.

A range the grammar did not capture is absent rather than classed `Other`: the
caller paints it in the base ink, and an unstyled gap costs nothing to merge.
Nested captures are resolved by a stack — a source event takes the innermost
capture around it — because `function.call` and `string` can both enclose the
same bytes and the inner one is the more specific claim.

## Classes and captures

`CAPTURES` pairs each recognised tree-sitter capture name with a
`TokenClass`: attribute, comment, constant, function, keyword, label, number,
operator, property, punctuation, string, type, variable. The names go to
`HighlightConfiguration::configure` once, which matches a capture to the
longest recognised name it contains; `function.method` therefore lands on
`function`, and an unknown capture is unstyled rather than an error.

## Grammars

One grammar is one C library compiled by `cc`, so `Language` is a `match` and
not a trait: a second language is a dependency and an arm. This build ships
Rust and JSON, both MIT and both compiled in unconditionally. The curated set
the [rich text survey](../reference/rich-text-survey.md) fixed — C/C++, Python,
JS/TS, JSON, TOML, YAML, Bash, Go, HTML/CSS — grows as each is needed, and each
grammar's licence is audited on arrival, since a few upstream grammars are
copyleft and belong behind their own opt-in feature.

`tree-sitter-rust` exports no locals query, so the locals argument is empty;
its highlight query and injections query are the whole grammar data.
`tree-sitter-json` ships no injections or locals query either, and captures
neither punctuation nor object keys as properties: a key is a string, and
braces and commas stay in the base ink.

`Language::from_info` is the one place a fence's info string becomes a grammar:
the first comma-separated part names it, matched case-insensitively, with `rs`
as Rust's alias. A name this build has no grammar for is `None`, which the
caller paints as plain text rather than an error.

The configuration is built once per language behind a `OnceLock`, because
`HighlightConfiguration::new` compiles the query on first use: a later parse
pays for one tree, not one query compile.

## Limits

`tokens` walks the whole source through `tree_sitter_highlight::Highlighter`,
which runs the query over the entire document. That fits a code block in a
page; an editor wants `Tree::edit` plus a reparse of the changed ranges and a
splice of the spans, which is the incremental iterator the survey leaves to
us.

Each thread keeps the sixteen most recent `(language, source)` parses, so a
rebuild — which a theme tween performs every frame — re-tokenises only a block
whose text changed. The entry is exact-match and move-to-front; a document with
more blocks than the window falls back to a full parse for the ones that fell
out, and sharing is visible as `Arc::ptr_eq` rather than promised by a cache
hit count.

## Traps

| Change | Symptom |
| --- | --- |
| Slice with a token range without checking the boundary | A panic at the first multi-byte character inside a node |
| Keep the outermost capture | A whole function body takes the keyword's ink |
| Style inside this crate | A second theme needs a fork |
| Build the configuration per call | The query recompiles every frame |

## Status

Implemented: `Language::Rust` and `Language::Json`, the thirteen classes, the
nested capture stack, a lazily built shared configuration per grammar, a
thread-local cache of recent parses, `Language::from_info`, and `tokens()`. The gallery's Markdown
page highlights its Rust fence through it — each class gets an ink, comments
also go italic — and leaves the text plain when the fence names a language
this build has no grammar for.

Not implemented: incremental reparse, further grammars from the survey's set,
and a theme object holding the class → style map.

## See also

- [Block Document](./document-model.md): the tree a highlighted fence sits in.
- [Inline Content](./layout-inline-content.md): runs, ink and decorations the
  tokens become.
