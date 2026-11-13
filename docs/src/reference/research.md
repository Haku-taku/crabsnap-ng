# Research Index

A subsystem page states a conclusion and the rule that follows from it. The
research notes behind those conclusions hold the raw comparison:
measurements, framework source readings, and the alternatives that lost.
When a page says "measured neutral" or "GPUI does X", this is where the
evidence for that sentence lives.

## Topics

| Topic | What the notes answer |
| --- | --- |
| Layout | Box model and positioning, the flex algorithm, grid track sizing, engine architecture, and the inventory of what each framework models |
| Declarations to rendering | Why the pipeline is typed primitives plus an operation log rather than one command enum; egui, GPUI, Qt Quick and Flutter compared |
| Text and i18n | Shaping and fallback, Fluent's capabilities and costs, the locale chain |
| Rich text | Markdown parsing, syntax highlighting, math layout and document models: what we take, what we own |
| Scroll | GPUI, egui and Qt scroll models; the constraints that shaped the container |
| Damage and presentation | GPU dirty rects, `damage_buffer` semantics, persistent scene targets |
| Backends | GPU/EGL zero-copy, the Vulkan question, CPU rasterizer choice, HDR and colour management |
| Controls | The three systems' component mappings, WinUI reference implementation, MD3 token and motion study, Liquid Glass pipelines |
| Audits | Per-subsystem audit ledger, clippy baseline, modifier mapping |

## How to use a note

- A note is evidence for the question it asked, not a specification. Its
  conclusion is only as wide as its measurement.
- Prefer the subsystem page for the rule and the note for the number. If they
  disagree, the code decides, then the page and the note are both updated.
- A note that is superseded is deleted, not archived; the subsystem's
  `## Corrections` records what changed and why.

## Status

The notes are being consolidated into the book. Until a topic has a page
that carries its conclusion, the note remains the only record, and a page
citing it should say how the fact was measured rather than only that
it was measured.

## See also

- [External Sources](./external.md): framework sources and where to read them.
- [Workflow](../development/workflow.md): how to check a claim
  before relying on it.
