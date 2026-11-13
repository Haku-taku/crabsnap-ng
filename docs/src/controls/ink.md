# State Layers and Ripples

An MD3 control paints three things on one surface: the hover/focus state
layer, the selection fill, and the press ripple. They must agree on the same
rect and corner curve, including when the control paints no fill at all this
frame. That slot is the control's *effective background*.

## The slot exists whether or not it paints

A navigation destination that is not selected still owns a capsule; a
drawer row with no background still owns its row rect. If the ripple clips to
whatever was painted, an unselected destination has a zero-width target and
a press shows no ripple.

The rule follows: declare the slot's geometry, not the shape that happens to
be in the declaration list this frame.

## Ink targets

`InkTarget` says where a ripple takes its rect and curve from:

| Target | Use |
| --- | --- |
| `Node` | The node's own box; correct when the box is the background and its curve moves |
| `Rect { rect, corners }` | A fixed slot in the node's own space |
| `Descendant(id)` | A slot that is a node, placed by layout (`getRectCallback`) |
| `Shape(n)` | The nth painted shape, in declaration order |

Prefer `Rect` or `Descendant`. `Shape(n)` is an index into the paint order,
and conditional shapes shift that order, so the same control can clip its
ripple to a different shape when a state appears.

## Building one

The MD3 kit's `layer::Background` produces the state layer, the focus ring
and the press target from one geometry:

```rust
let background = Background::stadium(width, height);
shapes.push(background.fill(state_layer));
shapes.push(background.ring(focus_ink, width));
// .press_ink(background.press(ink, InkRadius::Cover))
```

`Background::new(rect, corners)` is the general form; `stadium` is the
capsule case. Because all three values come from one struct, they cannot
disagree.

The engine owns the splat's origin and lifetime: `Widget::decl` is a pure
function and cannot keep the press point across frames. A control declares
colour, radius and target only.

## Traps

| Change | Symptom |
| --- | --- |
| Clip the ripple to the painted background | An unselected item has no ripple |
| Use `Shape(0)` for a slot that may not paint | The target drifts to a conditional shape |
| Compute the capsule rect from the current width | A collapsed capsule clips the ripple to nothing |
| Do the state layer and the ripple separately | Their curves disagree at the corners |
| Capture a `Rect` for a background whose curve animates | The splash keeps the corner it had at the press |
| Infer the curve from a shape that comes and goes | A press before the first hover cuts the splash square |
| Keep the press point in the widget | It is gone next frame; the engine owns it |

## Corrections

**Three clipping mechanisms coexisted.** `InkTarget::Node`, `Shape(n)` and
`Descendant(id)` were each used somewhere, so the same control could cut its
state layer and its ripple to different boxes. One effective background
replaced the choice per feature.

**`Shape(n)` depended on declaration order.** Conditional pushes changed the
index, and one control relied on an unwritten "the background is always the
first shape" convention. Explicit geometry or a dedicated node removed the
index from the contract.

**An unselected destination had no ripple.** Its indicator was zero width, so
`Shape(0)` resolved to a zero rect and the press ink had nowhere to go. The
slot now exists at rest, and a test pins that an unselected destination still
offers a capsule.

## Status

Migrated to an explicit background: the MD3 navigation bar, rail, drawer and
toolbar (fixed rect or indicator node). Buttons, chips, cards, lists, tabs,
segmented controls, the FAB menu and the pickers still use
`InkTarget::Node`, where the node box is the background.

## See also

- [UI Checklist](./checklist.md): the press, focus and state checks in context.
- [Pointer Model](../engine/pointer-model.md): who owns a press.
- [Style Objects](../engine/style-objects.md): where the layer's ink comes from.
