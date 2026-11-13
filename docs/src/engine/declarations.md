# Incremental Declarations

The app builds a fresh declaration tree every frame. The engine can skip
rebuilding parts of it when a control proves that nothing it reads has
changed. This page states the proof, the app's side of the contract, and why
the cache is worth less than it looks.

## The mechanism

| Piece | Role |
| --- | --- |
| `Decl.children: Vec<Rc<Decl>>` | A node records the declaration it came from |
| `CachedKey { id, props }` | Identity plus a fingerprint of the control's own inputs |
| `Element::cache_key()` | A control's own fingerprint; `Container` answers with a marker key and proves its shell by comparison instead |
| `Cx` `Dep` recording | Every read through `Cx` during build becomes a dependency |
| `DeclCache` entry | Holds the `Rc<Decl>` and the props fingerprint |
| `Tree::apply` fast path | `Rc::ptr_eq` skips the deep compare and clone of `NodeData` |

An entry is valid when every recorded dependency still holds and every child
entry is valid too. Animation calls are replayed at validation time: the call
is re-run, and if its value is unchanged the entry hits.

## The control author's contract

A control may return a `CachedKey` only if `decl()` reads nothing outside
`cx` and its own fields. The fingerprint must cover every field `decl()`
reads. One missed field means pixels that never update, with no error and no
warning.

- Prefer collecting the inputs into one `Opts` struct and deriving `Hash`,
  so the fingerprint has a single source.
- `Container` returns `None` on purpose: its shell is built by the app, and
  the engine cannot prove the shell did not change.

## The app's contract

1. **State the engine cannot see has changed** → call `cx.invalidate_decls()` in
   `view()`. A call to `App::changed` already drops every entry.
2. **Reading engine facts outside element construction** (for example
   `cx.at(id).animate(...)` and then passing the value into a control)
   cannot be attributed to an entry, so the whole frame's cache is dropped.
   This is automatic.
3. The replay assumption: a control's animation call is a function of its
   recorded dependencies and its own fields. If the target changes without a
   dependency changing, the animation freezes at the old value, which means
   rule 1 was broken.

## What it is worth

Measured on 2026-11-04, release build, slider page, scripted sway for 10–20 s:

- Widget-level cache: 94% hit rate, and `decl` time unchanged (116.6 µs with
  cache, 115.7 µs without). The reason is the split:
  `decl 111 µs = view 79 µs + root_decl 32 µs`. The cache only touches
  `root_decl`.
- Container-level cache plus whole-subtree skip: 683 vs 669 µs per frame,
  inside noise. Interactive frames invalidate the cache every frame anyway,
  and a dragged control changes the `Rc` on every ancestor from the control
  to the root.

The conclusion is uncomfortable but useful: **the expensive part is
`App::view`, which the engine cannot see into.** Do not attempt another
declaration cache layer expecting double-digit gains. If subtree reuse is
ever worth doing, it needs a different signal (layout dirty marks plus
unchanged constraints), and it must be honoured by layout, hit, and paint as
well.

## Corrections

**Reads outside element construction invalidated the whole cache.** An
unattributed read was treated as "anything may have changed", so the table was
dropped every frame and never hit. Unattributed reads are now collected per
frame and compared with the previous frame; only a difference invalidates the
cache, and an equal reading proves the derived state did not move.

**The cache existed with zero keys.** `element_decl` only consults the cache
when an element returns a key, and the only element returning one forwarded
to a `Container`, which cannot prove its shell. Every frame rebuilt the whole
tree while the cache reported `0 hits`. The mechanism was complete; the missing
piece was the `Widget` fingerprint, now implemented.

**A vanished child entry left its parent valid.** Validation checked only the
children that still had entries, so a parent whose child stopped reporting an
entry kept a frozen subtree. Entries now carry a `whole` flag: a whole entry
requires every child entry to be valid. The flag also gates
`cached_decl_matching` and the `Rc` fast path.

**A declaration-built subtree could never be reused.** Validation demanded an
entry for every child id of a cached declaration. A control that builds its own
rows has children that are declarations, never entries, so every lookup walked
into "no entry" and failed: the cache reported zero hits while the tree rebuilt
every frame, and the first control to opt in built exactly such a tree. The
recorder now lists the element children a build created (`note_element_child`),
and validation recurses into exactly those; a declaration child is covered by
the entry's own fingerprint. `Markdown` measured 3098 → 2373 allocations per
frame, 1555 → 1366 kB, once it was placed as a cached element.

**Two kinds of child are live state, not projections.** A declaration that
declares an input's bytes owns a buffer the engine edits in place; replaying the
declaration restored the bytes from build time, so an editor lost characters
and re-revealed old caret states. Such an entry is stored unusable (`whole`
cleared) and rebuilt every frame. A child that hands over a finished
declaration (`decl_shared`, what `child_decl` and `from_decl` place) is app
input built outside the recorded build; reusing its parent embedded last
frame's handover. `element_decl` now marks that child rebuilt, so its parent is
rebuilt too. Both rules came from the gallery self-test: with either missing,
the Markdown editor, the math page's error popover, the positioning sample and
the selection wash all failed against the same run with the cache disabled.

## Traps

| Change | Symptom |
| --- | --- |
| Fingerprint misses a field `decl()` reads | That field never updates on screen |
| Put the props fingerprint into `Dep` | Validation never sees it; entries hit on stale props |
| Return a key from a `Container` | The app's shell can change under a valid-looking entry |
| Add another cache layer above this one | Buys nothing; cost is in `view()`, which has no key |
| Assume a hit avoids layout or paint | Both still walk every node |

## Status

Implemented: the cache, widget keys, container matching, whole-subtree
validation, the `Rc` fast path, and the invalidation contract. Controls that
return a key today: `Markdown`, `Md3Slider` and `Md3NavDrawer`. A control whose
`decl()` builds its rows from its own resolved fields and reads nothing through
`cx` is the cheapest kind to cache: the gallery's navigation drawer (22 slots,
each a text block) took the markdown page from 2258 to 1574 allocations per
frame, 1363 to 989 kB, once it answered with a key — it was about half of that
page's frame.

Not planned: app-side `memo` for subtrees, and subtree-level layout/paint
skipping. Both need a dirty-mark design of their own.

## See also

- [Frame Pipeline](./frame-pipeline.md): where `view`, `apply`, and the cache
  sit in a frame.
- [Style Objects](./style-objects.md): the other place per-frame work is cached.
