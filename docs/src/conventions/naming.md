# Naming

A name in this codebase encodes what a thing is allowed to do, not only what
it holds. Read a signature before reading the implementation: the suffix
usually answers the question the reader would otherwise have to trace.

## Vocabulary

| Name | Means |
| --- | --- |
| `Decl` | One frame's description of a node; immutable once built |
| `Element` | Something that can build a `Decl` and report a `Change` |
| `Node` | The engine's retained copy of a `Decl`, with layout and interaction state |
| `Widget` | A control: `decl(&self, cx)` plus an optional `change(..)` |
| `Params` | The facts a control is constructed from; plain data |
| `Change` | A semantic report from a control to the application |
| `Cx` | The context a build reads: scope, routing, geometry, animation |
| `Styled` | The adapter that turns a `Widget` into an `Element` |

A widget holds no state and keeps nothing between frames. If a name suggests
otherwise (`…State`, `…Cache` inside a control), it is either engine-owned or
misplaced.

## Suffixes that are load-bearing

| Suffix | Contract |
| --- | --- |
| `resolve` | Pure: builds a `Decl` from the receiver alone |
| `resolve_in` | Needs `Cx`: reads scroll geometry, scope, or routing |
| `colour_or`, `metric_or`, `get_or` | Returns the fallback when the value is missing |
| `…_at`, `…_of` | Answers about a point, id, or node rather than about the receiver |
| `…_by` | Takes an explicit anchor or owner instead of searching for one |

`resolve` and `resolve_in` must agree: the context-free form is the same
declaration minus the parts that need a frame. A control whose `resolve` draws
less than `resolve_in` should say so in the doc comment, as `Scroll` does for
its scrollbar.

## Identifiers

- Full words. `cfg_`, `tmp`, `q`, `n` are not names; `config`, `scratch`,
  `query`, `count` are.
- Ids are stable strings: `Id::str("nav")` for a root,
  `id.with("title")` or `id.with(index)` for parts. Two nodes must never share
  an id; the tree keys by it and silently merges.
- A part id names the role (`"leading"`, `"state-layer"`), not its index in
  the declaration list, so reordering children does not rename anything.
- Acronyms follow the crate's existing spelling: `Md3`, `Gpu`, `Ime`.

## Files and crates

- One control per file, singular: `button.rs`, `nav_rail.rs`, `date_picker.rs`.
- A logical unit gets a file, not a folder of helpers. No new `mod.rs`: the
  module lives in `src/name.rs`, and a new crate names its library root in
  `Cargo.toml` (`[lib] path = ...`).
- Crate names carry the layer: `ui-core`, `ui-style`, `ui-widgets-md3`. A
  facade crate contains `pub use` only.

## Traps

| Change | Symptom |
| --- | --- |
| Name a fallback getter `colour` and return a default | Callers cannot tell a real role from a substitute |
| Add a `resolve_in` path to one control only | The context-free `resolve` draws a different tree |
| Name a part id after its index | Reordering the children changes every id and loses interaction state |
| Introduce `tmp`/`n` in a hot path "because it is short" | The next reader cannot tell what is being counted |
| Put state in a `Widget` | It persists across frames and disagrees with the engine |

## See also

- [Comments](./comments.md): what a name does not say, and a comment may.
- [Tests](./tests.md): names are specifications there too.
