# crabsnap-ng Documentation

Engineering documentation for the `ui/` workspace: the engine, its contracts, the three control
sets, the decisions behind them, and the mistakes that produced them.

The book is built with [mdBook](https://rust-lang.github.io/mdBook/) from `docs/src/`. Chapters are
listed in [`src/SUMMARY.md`](./src/SUMMARY.md); a chapter written as `[Title]()` is planned and not
written yet.

To preview:

```sh
cargo install mdbook
mdbook serve docs
```

## Where the rules live

| File | Governs |
| --- | --- |
| [`AGENTS.md`](./AGENTS.md) | What an agent must do when changing docs: summary entries, placement, evidence, safety |
| [`.rules`](./.rules) | Voice, page patterns, terminology, status vocabulary |
| [`.conventions/CONVENTIONS.md`](./.conventions/CONVENTIONS.md) | What gets documented, page versus section, page anatomy, quality checklist |
| [`../AGENTS.md`](../AGENTS.md) | Repository-wide traps; not part of the book |

## Policy

- No binary assets in the repository. Reference an external location, or record the measurement.
- `old/` is a staging area for material being ported. It is not part of the book, and no page links
  into it.
- A page that contradicts the code is a bug in one of them, not a known issue.
