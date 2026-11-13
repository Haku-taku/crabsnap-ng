# Design Tokens

An MD3 control's numbers are not written by hand. A token export supplies
every value, a generator turns it into a spec function, and the control
resolves that function for the state it is in. The generator is
`ui/crates/ui-widgets-md3/tokens/gen_spec.py`; its output is
`src/spec/<control>_tokens.rs`, marked "generated".

## The pipeline

| Step | Command or file |
| --- | --- |
| Export | `ui/crates/ui-widgets-md3/tokens/<control>.json`, checked in |
| Inspect | `python3 tokens/gen_spec.py inspect tokens/<control>.json` |
| Generate | `python3 tokens/gen_spec.py gen tokens/<control>.json --out src/spec/<control>_tokens.rs` |
| Check | the generate command with `--check`; exits non-zero when the file is stale |
| Audit | `python3 tokens/gen_spec.py audit`; regenerates every table listed in `tokens/tables.json` |
| Selftest | `python3 tokens/gen_spec.py selftest` |
| Consume | `use crate::spec::<control>_spec;` from the control module |

`tokens/tables.json` records the export and prefixes behind each table; the file
names its own control, so the manifest holds only where the values came from.
`audit` is the whole-book check: it prints the stale tables and exits non-zero.

`inspect` prints states, objects, properties and the full table; use it
before writing a control, and again when a token seems missing.

## What the generated file contains

One struct per object (`NavigationRailStateLayer`, `ButtonContainer`, …), a
top-level struct holding them, a flags struct naming the interaction switches,
and one `const fn` taking those flags plus one value per variant axis:

```rust
#[derive(Clone, Copy, Debug)]
pub struct NavigationRailFlags {
    pub active: bool,
    pub no_label: bool,
    pub hovered: bool,
    pub focused: bool,
    pub pressed: bool,
}

impl NavigationRailFlags {
    /// Every switch off: the base state.
    pub const NONE: Self = Self {
        active: false,
        no_label: false,
        hovered: false,
        focused: false,
        pressed: false,
    };
}

pub const fn navigation_rail_spec(
    flags: NavigationRailFlags,
) -> NavigationRailSpec
```

A caller names the switches it sets and leaves the rest off, so a parameter
never binds by position:

```rust
navigation_rail_spec(NavigationRailFlags {
    active: true,
    ..NavigationRailFlags::NONE
})
```

`NONE` is an associated constant rather than `Default`: a control's
preset specs are `const` items, and a trait method cannot be called from
one.

A table with one switch or none keeps a plain signature (`badge_spec(true)`):
there is no order to confuse, and a one-field struct would only add noise.

The body starts from the default values and overlays each active state in
priority order, so the last matching state wins. Values stay raw: colour
roles are `&'static str` names (`"secondaryContainer"`), shapes are
strings, sizes are `f32`. The control maps a role name to a colour through the
pack; nothing is resolved at generation time.

## Token names

A name is `md.comp.<control>.<states…>.<object>.<property>`. The object
may contain dots (`track.outline`, `focus.indicator`) or be absent
(`md.comp.divider.color`, where the control is its own object). Objects
are matched longest-first against a vocabulary declared in the generator;
anything left over is a state.

Unknown states are reported in the file header, not dropped silently.
Deprecated tokens are skipped with a count. `gen` prints the two other losses to
stderr — tokens whose export value is blank, and tokens under an excluded
subtree — because a value dropped that way leaves the control at `0.0`/`""`.

A prefix with no entry in `OBJECTS_BY_CONTROL` borrows every registered
control's objects and says so on stderr. The borrow keeps an unregistered
export readable, but another control's vocabulary can claim a segment that is
not an object here (`horizontal.container` in a menu). Register the control
before trusting its table.

## Adding a control or a token

1. Drop the export next to the others and run `inspect`; confirm the
   objects appear under the control's vocabulary, and add missing objects to
   `OBJECTS_BY_CONTROL` in the generator. A section of the export's tree that
   is not an appearance goes in `SECTION_SEGMENTS`; a subtree this build does
   not paint goes in `EXCLUDED_SUBTREES`.
2. Run `gen` and read the header and stderr: unknown states, deprecated
   tokens, blank values and excluded subtrees are the work list.
3. Never edit a generated file. If a value is wrong, the export or the
   vocabulary is wrong. `--check` proves the file is current: it regenerates in
   memory, formats both sides with `rustfmt`, and compares everything but the
   comment lines. A table generated before an export or a vocabulary changed
   fails the check until it is regenerated.
4. A table being rewritten names its own control, so a prefix with a section
   suffix keeps its function: `md.comp.date-picker.docked` still generates
   `date_picker_spec`. `--name` overrides that, and is what a table with no
   file yet needs when the prefix's last segment is not the control.
5. If the export splits one control into sections (collapsed/expanded,
   docked/full-screen), the generator flattens them into a single state
   vocabulary. Check the result before relying on it; a section that is not a
   state must not become one.

## Where hand-written values live

Only MD3 has a generated pipeline. Fluent control envelopes are a hand-written
table (`ui-widgets-fluent/src/spec.rs`, `MotionTable::winui`); Glass
values are Cupertino literals in its `pack.rs` and per-control constants. The
semantic layer (`ui-design`) supplies the common colours and metrics those packs
adapt.

## Traps

| Change | Symptom |
| --- | --- |
| Edit a `*_tokens.rs` by hand | The next generation silently reverts it |
| Add an object without updating the vocabulary | Its segments read as states; values land on the wrong field |
| Treat a section name as a state | Two unrelated variants collapse into one table |
| Resolve a colour role in the generator | The pack cannot theme it; a literal is baked in |
| Assume a token exists because the spec names it | The export may omit it; `inspect` decides |
| Leave a control out of `OBJECTS_BY_CONTROL` | Another control's object claims its segments; the table compiles with the wrong fields and parameters |
| Assume two axes are independent | Where both write one field, the axis declared later wins; order them by that rule |
| Pass two interaction switches positionally | Both are `bool`s, so a swapped pair compiles and paints the other state; the flags struct names each switch |
| Register one half's qualifier as a section | Its value lands on the shared field, and both halves inherit the open menu's corner |

## Status

Generated: the MD3 control token tables under `src/spec/`. Hand-written:
Fluent envelopes and motion, Glass constants, and the token values the
export does not carry (see each control's source comment for the reference
it came from).

The generator is still weak on an export that splits one control into several
token sets: it flattens them into one state vocabulary, and a section that is
not an appearance must be named in `SECTION_SEGMENTS` by hand. The decided
direction is to stop flattening by hand and give each section that *is* a real
appearance its own table, as `date-picker.docked`/`modal` and `toolbar.fab`
already do; that is recorded, not implemented. `selftest` pins the parse rules
on small fixtures — an object between states, an object split around a state,
a section segment, an excluded subtree, a base state with no axis to negate, a
base state guarded by the primary axis it is the opposite of, and a variant
axis — so a change to the matcher shows up as a failed line instead of a wrong
table.

Every registered table is current. Regenerating the date picker's table was a
behaviour fix, not only a vocabulary one: its default (unselected) state used
to assign unconditionally, so it overwrote `onPrimary` with `onSurface` in the
selected day's label. The generated file now guards that block with
`!selected`.

The rich tooltip's table carries an action's label and state layers, but the
control draws its actions as text buttons, which resolve those values from
`md.comp.button`. The tooltip's own table supplies the surface, the subhead and
the supporting text; the action part documents what the buttons must match.

A subtree can be a control of its own. The floating toolbar's `fab` group is
excluded from the bar's table and generated as `toolbar_fab_spec` from its own
prefix, so its theme axis and sizes do not become parameters of the bar.

A form can be a table of its own too. The date picker's modal prefix repeats
the docked body but adds a taller container and a two-line header, so it
generates `date_picker_modal_spec` from the modal prefix alone; the shared body
stays in the docked table. Its `range-selection` and `year-selection` subtrees
are excluded: they are drawing this build does not have yet, and letting them
through would add states no widget reads.

Axis order is precedence, not presentation. A table applies its pure-axis
blocks in `VARIANT_GROUPS` order and its keys carrying an interaction switch
after them, most specific last, so a later axis overrides an earlier one on a
shared field. The FAB needs this: its `surface` family re-declares the base
box, so colour must be applied before size or a large surface FAB would shrink
back to 56 dp. Before the rule was written down, the order came from the shared
priority list, where `small`/`medium`/`large` happen to sit early — correct for
the FAB only by accident.

The FAB export also spells one interaction two ways: the `surface` family
writes `hover`/`focus` while every other family writes `hovered`/`focused`, and
the deprecated `focus.container` tokens are dropped before this matters.
`STATE_ALIASES` folds the two spellings into one state, so the table has one
hover and one focus parameter rather than a pair that can disagree.

An export can omit its own base. Every split-button value sits under a size
segment, so no token states an unconditional appearance; taking the
alphabetically first state as the base made `hovered` the unguarded block and
a resting button would have drawn its hover corner. `DEFAULT_BY_CONTROL`
names the base, and an empty name says the table has none. The same export
uses `trailing-button` as both a half (its paddings) and a qualifier
(`trailing-button.inner-corner.selected`), so the vocabulary registers the
qualified object: the open menu's 50% corner is the trailing half's alone and
must not land on the shared `inner-corner`, or the leading half would turn
into a pill too.

Two kinds of segment in one export need opposite treatment. The progress
table's `wave` names the active indicator's amplitude and wavelength, so it is
a section and those values stay fields of `active-indicator`; `with-wave` is a
form axis, because it changes the container's height and size. Left unregistered
they became two booleans that can disagree: one would hold the amplitude of a
flat bar, the other a height with no wave.

The switches used to be positional `bool`s, and a call could pass them in the
wrong order and still compile: the split button's `(selected, hovered, pressed)`
was first called as `(false, false, selected)`, and the date and time pickers
each kept a hand-written `Flags` struct that mirrored the argument list field
for field. A table with two or more switches now emits an `XFlags` struct with a
`NONE` constant and takes it as one argument; one switch or none keeps a plain
parameter. Both hand-written mirrors are gone, and `selftest` pins both shapes.

## See also

- [Style Objects](../engine/style-objects.md): how a role name becomes a colour.
- [External Sources](./external.md): where an exported value can be checked.
