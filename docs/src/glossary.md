# Glossary

The words this codebase uses in a specific sense. When a term here conflicts
with a general UI term, this page wins.

| Term | Means |
| --- | --- |
| App | The application state and its `view()` method, which returns this frame's live control tree. `changed()` receives semantic reports from controls. |
| AppLoop | The engine loop. Owns the tree, focus, hit index, gestures, animation clocks, menus, and editing state. |
| Declaration (`Decl`) | The per-frame tree the app returns: containers, text, controls, each with an `Id` and styling. |
| Element | The live control tree (`Box<dyn Element>`) returned by `App::view` before it becomes declarations. |
| Node | The retained tree entry. Carries layout, presence, style, and parent/child links across frames. |
| `Id` | A stable identity across frames. Focus, selection, and animation state are claimed by `Id`, not by position. |
| `Event` | An input event, from a platform backend or a test harness. |
| `Change` | A semantic report from a control (`Toggled`, `Activated`, `Moved`, ...). Not an input event. |
| Frame | One build-and-present cycle: view, declarations, layout, text, damage, hit, paint, present. |
| `FrameClock` | Engine time. `now()` is the last ticked frame; `live()` reads the wall clock for event timestamps. |
| `Presence` | An enter or exit declaration: opacity, offset, scale, and optionally a separate scale clock. |
| `AnimSpec` | A duration plus an easing curve. |
| `AnimStore` | Per-id animation state. Retargeting starts from the current value, not the previous endpoint. |
| Damage | The set of rectangles that changed this frame, and therefore must be presented. |
| `DrawList` | Resolved primitives, clips, layers, and text runs, ready for a backend. |
| Primitive | A typed paint command: rect, rounded rect, glyph run, image, shadow, arc. |
| `TextPass` | Shaping, line breaking, and glyph resolution for the frame. One instance serves layout and paint. |
| Style | Resolved visual properties. A style object is computed from the design system, not written per call site. |
| Spec | The design system's constants for one control, generated and checked against the published values. |
| Capabilities | Host facts detected at runtime (protocols, pixel formats, input devices) and passed down. |
| Present target | A backend's presentation contract: `PresentTarget` for CPU pixels, `SceneTarget` for an engine-built scene. |
| Control set | One complete design system's controls: MD3, Fluent, or Liquid Glass. |
| Nested session | An isolated Hyprland launched by `scripts/nested-session.sh`, used for graphical tests. |
| Legacy project | `/home/ollie/git/crabsnap`, the pre-rewrite implementation. Unverified input, never a dependency. |

## See also

- [Architecture](./architecture.md): how these parts are split across crates.
- [Frame Pipeline](./engine/frame-pipeline.md): the order a frame runs in.
- [Workflow](./development/workflow.md): how to change any of the above.
