# egui 0.36.2 — Architecture for a New IMGUI Framework

Sources: `/home/ollie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{egui,epaint,emath}-0.36.2/src/`. Paths below are relative to each crate's `src/`. Every claim cites lines I read.

Target: a GPU IMGUI for a screen-capture/annotation tool (overlay toolbar, context menus, save dialog, settings). These surfaces are floating, translucent, sometimes frosted, often rebuilt on the fly — exactly where egui is weakest.

---

## 1. Frame loop / `Context`

`Context` is `Arc<RwLock<ContextImpl>>` (`context.rs:722-723`); all methods are `&self` (recursive locking deadlocks, `context.rs:680-697`). `run_ui` builds a root `Ui` and calls `run_dyn`, a pass loop (`context.rs:798-890`):

```rust
// context.rs:836-876 (abridged)
loop {
    self.begin_pass(new_input.take());
    run_ui(self);                          // user code re-executed, once per pass
    output.append(self.end_pass());
    if !output.platform_output.requested_discard() { break; }
    if max_passes <= output.platform_output.num_completed_passes { break; }
}
```

`RawInput::take` (`data/input/raw_input.rs:124-143`) makes multi-pass correct: events and `screen_rect` are *moved*, `time`/`focused`/`system_theme` are copied, so pass #2 sees identical input with no duplicate events. `request_discard` (`context.rs:1992-2004`) is the "my layout guess was visibly wrong" escape hatch; only `Grid` (`grid.rs:465-472`) and `Resize` (`containers/resize.rs:286`) use it.

`ContextImpl::begin_pass` (`context.rs:418-540`) is where last frame becomes this frame:

```rust
// context.rs:461-506 (abridged)
self.memory.begin_pass(&new_raw_input, &all_viewport_ids);
viewport.input = core::mem::take(&mut viewport.input).begin_pass(...);
viewport.this_pass.begin_pass();                 // clears used_ids / widgets
let mut layers: Vec<LayerId> = viewport.prev_pass.widgets.layer_ids()
    .filter(|l| self.memory.areas().is_interactable(*l)).collect();
layers.sort_by(|&a, &b| self.memory.areas().compare_order(a, b));
viewport.hits = crate::hit_test::hit_test(&viewport.prev_pass.widgets, &layers,
    &self.memory.to_global, pos, interact_radius);
viewport.interact_widgets = crate::interaction::interact(&viewport.interact_widgets,
    &viewport.prev_pass.widgets, &viewport.hits, &viewport.input,
    self.memory.interaction_mut());
```

Hit-testing and the click/drag state machine run **before** user code, against **last pass's** `WidgetRect`s — egui's hidden one-frame latency (§4, pitfall 1). Data flow: `RawInput → InputState → hit_test(prev_pass) → InteractionSnapshot →` user code (`Ui::allocate_space → create_widget → this_pass.widgets`; `Painter::add → GraphicLayers → Vec<ClippedShape>`) `→ end_pass → memory.end_pass(used_ids), fonts delta, textures_delta, graphics.drain(order), swap(this,prev) → FullOutput`.

```rust
// data/output.rs:12-40
pub struct FullOutput {
    pub platform_output: PlatformOutput,
    pub textures_delta: epaint::textures::TexturesDelta,
    pub shapes: Vec<epaint::ClippedShape>,
    pub pixels_per_point: f32,
    pub viewport_output: OrderedViewportIdMap<ViewportOutput>,
}
```

`PlatformOutput` (`data/output.rs:139-193`): `commands`, `cursor_icon`, `cursor_image`, a11y `events`, `mutable_text_under_cursor`, `ime: Option<IMEOutput>`, `accesskit_update`, `num_completed_passes`, `request_discard_reasons`. `take` (`data/output.rs:244-249`) is stateful — `cursor_icon` is sticky across frames.

`ContextImpl::end_pass` (`context.rs:2664-2850`): fonts delta, `memory.end_pass(&this_pass.used_ids)`, AccessKit tree, `graphics.drain(areas.order(), to_global)`, optional "repaint if any `WidgetRect` changed" (`2725-2731`), then `swap(&mut prev_pass, &mut this_pass)` (`2746`). "Immediate mode" literally re-executes at `context.rs:856`; the framework's job is to keep *identity* (`Id`) and *results* (`WidgetRect`, `Region`, `AreaState`) outside the closure.

---

## 2. Layout

```rust
// layout.rs:13-48
pub(crate) struct Region {
    pub min_rect: Rect,      // bbox of children so far
    pub max_rect: Rect,      // SOFT max — children may overflow
    pub(crate) cursor: Rect, // one side ±INFINITY along the main axis
}
```

`initial_cursor` (`layout.rs:380-399`) sets the main-axis side to `±INFINITY`, making the cursor "unbounded remaining row". `region_from_max_rect` (`layout.rs:401-411`) seeds `min_rect` as a zero-size rect at the next widget position, so an empty `Ui` reports a valid point. `available_from_cursor_max_rect` (`layout.rs:434-493`) intersects `max_rect` with the cursor and collapses inverted rects to a midpoint.

The exact allocation (`ui.rs:1239-1251`):

```rust
fn allocate_space_impl(&mut self, desired_size: Vec2) -> Rect {
    let item_spacing = self.spacing().item_spacing;
    let frame_rect  = self.placer.next_space(desired_size, item_spacing);      // 1
    let widget_rect = self.placer.justify_and_align(frame_rect, desired_size); // 2
    self.placer.advance_after_rects(frame_rect, widget_rect, item_spacing);    // 3
    register_rect(self, widget_rect);
    widget_rect
}
```

1. `Placer::next_space` (`placer.rs:113-125`) → `Layout::next_frame` (`layout.rs:499-580`): handle wrapping, then `next_frame_ignore_wrap` (`layout.rs:582-636`): `frame_size = child_size`, inflated to full available width when `cross_justify` or (vertical + center), then `align2.align_size_within_rect(frame_size, available_rect)` — the frame fills the cross axis and is pinned to the main-axis start.
2. `justify_and_align` (`layout.rs:639-653`) inflates/centers the child inside the frame.
3. `advance_after_rects` (`layout.rs:699-765`): expand the cursor's cross extent to the frame, then `cursor.min.y = widget_rect.max.y + spacing.y` (TopDown), symmetrical for the other three directions.

`frame_rect` (consumed) vs `widget_rect` (drawn) is the key split: `min_rect` grows by the *frame* ("pretend we used whole frame", `placer.rs:189`), so a centered button doesn't shrink its parent. `Placer` (`placer.rs:7-299`) is a thin `Option<GridLayout>` adapter around `Layout`; there is no `next_widget_space_rect` (`next_space` and `next_widget_space_ignore_wrap_justify`, `226-229`, are the equivalents).

**Wrapping/columns** (`layout.rs:506-580`): if `available_size.x < child_size.x` and we're not at the row start, jump to a new row at `min_rect.bottom() + spacing.y`. Wrapped advance detects a new row by `cursor.intersects(frame_rect.shrink(1.0))` and rebuilds the cursor with a `NaN` on the unknown side (`707-749`). **Columns = wrapping with `main_dir = TopDown`.**

**Unknown desired size.** There is no content-size query. `allocate_ui_with_layout` (`ui.rs:1331-1348`) allocates a guessed frame; the child overflows and `scope_dyn` (`ui.rs:2203-2215`) later does `advance_cursor_after_rect(child_ui.min_rect())` — the parent cursor ends correct, but earlier-painted siblings may shift. `Area` instead uses a `sizing_pass` (`containers/area.rs:444-487`, `area.rs:625-627`): children are built invisible with `cross_justify=false, cross_align=Min` (`ui.rs:240-247`) to measure minimums; the area is non-interactable that pass and requests a repaint (`area.rs:684-687`). **`WidgetInfo` is not layout** — a11y/debug only (`data/output.rs:563-735`, `context.rs:1572-1584`).

**Expand-to-available** is expressed as `available_size()` (`layout.rs:419-430`), `*_justified` layouts (`layout.rs:184-186`, `220-228`), or `set_min_size` (`placer.rs:233-281`); egui never returns a rect smaller than requested (`ui.rs:1130`).

---

## 3. State across frames

```rust
// widget_rect.rs:8-49
pub struct WidgetRect { pub id: Id, pub parent_id: Id, pub layer_id: LayerId,
    pub rect: Rect, pub interact_rect: Rect, pub sense: Sense, pub enabled: bool }
```

`WidgetRects` (`widget_rect.rs:93-106`) holds both orders: `by_layer: HashMap<LayerId, Vec<WidgetRect>>` (insertion order = paint order = z-order in a layer) and `by_id: IdMap<(usize, WidgetRect)>`. `insert` (`widget_rect.rs:166-224`) is an upsert: rect "last wins", `sense |=`, `enabled |=`, optional `move_to_top`; a layer change for the same id in one frame **panics** in debug (`widget_rect.rs:206-211`). `Ui::new` registers itself as a widget with `Rect::NOTHING` *before* its children (`ui.rs:170-184`) and `remember_min_rect` updates it after (`ui.rs:970-996`) — containers are hit-testable for free.

**Ids** are deterministic ahash (`id.rs:67-88`): `Id::new` hashes anything, `Id::with(salt)` hashes `(parent, salt)`. Auto-ids come from `next_auto_id_salt` seeded by `id.with("auto")` (`ui.rs:154`, `1231-1232`); `scope_dyn` rewinds the salt so a `scope` consumes one id like `allocate_space` (`2208-2210`); child ids are `stable_id` and `unique_id = stable_id.with(auto)` (`252-259`). Collisions are **diagnostic only**: `check_for_id_clash` (`context.rs:1163-1234`) paints a red warning — so filtered/reordered context menus must carry explicit salts or state hops between rows.

**Superficial state** is `IdTypeMap` (`util/id_type_map.rs:405-420`): `RawKey = id XOR hash(Type)`, nohash so one probe; values must be `Clone + Send + Sync` and **every read clones** (`444-451`), hence "wrap big data in `Arc`" (`memory/mod.rs:39-42`). `TextEditState::load/store` is literally `get_persisted`/`insert_persisted` (`widgets/text_edit/state.rs:62-68`).

**`Response`** (`response.rs:23-76`) = `Context` clone + `layer_id/id/rect/interact_rect/sense` + a private `Flags` bitset (`response.rs:92-138`: `ENABLED, CONTAINS_POINTER, HOVERED, HIGHLIGHTED, CLICKED, FAKE_PRIMARY_CLICKED, LONG_TOUCHED, DRAG_STARTED, DRAGGED, DRAG_STOPPED, IS_POINTER_BUTTON_DOWN_ON`). Flags are filled in `get_response` (`context.rs:1425-1566`) by reading the pass-start `InteractionSnapshot` — a `Response` is a snapshot of a decision already made, which is why reading it inside an open `Ui` gives last frame's rect (`context.rs:1348-1355`).

---

## 4. Interaction & hit testing

`hit_test` (`hit_test.rs:42-199`) is, functionally, a **z-ordered linear scan of nearby widgets**:

1. Flatten `widgets.get_layer(l)` for layers sorted back-to-front by `Areas::compare_order` (`memory/mod.rs:1226-1234`: `Order` enum, then previous frame's index; `None < Some`).
2. Keep widgets within `interact_radius`; `dist_sq <= closest` so ties go to the **last = topmost** (`hit_test.rs:77-80`).
3. Layer blocking (`hit_test.rs:120-129`): walk hits top→bottom, collect layer ids, stop when any hit's `interact_rect` contains the whole search circle — everything lower is dropped.
4. Disabled widgets get `sense -= CLICK|DRAG` (`hit_test.rs:134-139`); cross-layer "hidden behind" pruning (`hit_test.rs:145-156`).
5. `hit_test_on_close` (`hit_test.rs:206-330`) yields one `click` + one `drag`, preferring a small clickable thing on a big draggable background.

Click/drag arbitration is a 200-line state machine over `InteractionState` (`memory/mod.rs:486-498`: `potential_click_id`, `potential_drag_id`):

```rust
// interaction.rs:162-189 (abridged)
PointerEvent::Pressed { .. } => {
    if interaction.potential_click_id.is_none() { interaction.potential_click_id = hits.click.map(|w| w.id); }
    if interaction.potential_drag_id.is_none()  { interaction.potential_drag_id  = hits.drag.map(|w| w.id); }
}
PointerEvent::Released { click, .. } => {
    if click.is_some() && !input.pointer.is_decidedly_dragging()
        && let Some(w) = interaction.potential_click_id.and_then(|id| widgets.get(id)) { clicked = Some(w.id); }
    interaction.potential_drag_id = None; interaction.potential_click_id = None; dragged = None;
}
```

Hover is computed last (`interaction.rs:257-301`): while clicking/dragging only that widget is hovered; otherwise non-interactive widgets *above* the topmost interactive hit are hovered too (so labels in windows get tooltips).

**Areas/layers:** `Order` is five flat buckets `Background < Middle < Foreground < Tooltip < Debug` (`layers.rs:10-27`); `LayerId { order, id }` (`layers.rs:65-68`). `Area::show` = `begin` → `content_ui` → `end` (`containers/area.rs:406-416`): positions from `AreaState{pivot_pos, pivot, size}`, registers an immediate move/drag response, calls `move_to_top` on click (`area.rs:553`). `Areas::end_pass` (`memory/mod.rs:1358-1400`) re-sorts by `(order, wants_to_be_on_top)` and splices sublayers above their parent (one level only, `1304-1331`). Modality is not geometric: `is_above_modal_layer` compares `compare_order` (`941-950`); `allows_interaction` (`956-960`) gates focus and hit layers.

---

## 5. Focus, keyboard, IME

```rust
// memory/mod.rs:501-534 (abridged)
struct Focus { focused_widget: Option<FocusWidget /* {id, filter: EventFilter} */>,
    id_previous_frame: Option<Id>, id_two_frames_ago: Option<Id>, id_next_frame: Option<Id>,
    give_to_next: bool, last_interested: Option<Id>, focus_direction: FocusDirection,
    top_modal_layer: Option<LayerId>, top_modal_layer_current_frame: Option<LayerId> }
```

`begin_pass` (`memory/mod.rs:570-621`) consumes Tab/Shift+Tab/arrows/Escape **only if the current `EventFilter` doesn't claim them**; traversal resolves at `end_pass` (`623-641`) via a ±45° cone scored by `distance / cos²θ` (`703-778`). Tab order = registration order (`647-689`), i.e. code order, including widgets scrolled out of view. Dead-man's switch (`630-638`): focus drops if the id wasn't in `used_ids` and didn't just gain focus. `EventFilter` (`data/input/event_filter.rs:10-79`) is four booleans — a focus lock, not a capture model.

`Event` (`data/input/event.rs:17-189`): `Text(String)`, `Key{key, physical_key, pressed, repeat, modifiers}`, `Ime(ImeEvent)`, `Copy/Cut/Paste`, `PointerButton`, `MouseWheel{unit,…}`, `Zoom`, `Touch`, `WindowFocused`, `Screenshot`, `AccessKitActionRequest`; integrations must not send `Key`s already consumed by an IME (`event.rs:36`).

`TextEdit` consumes IME only if `owns_ime_events(id) == has_focus(id)` (`memory/mod.rs:1037-1048`); the match arm is `Event::Ime(e) if owns_ime_events` (`widgets/text_edit/builder.rs:1238`). The preedit is written into the buffer as real text, tracked via `cursor_purpose = ImeComposition{active_range}` (`builder.rs:1306-1324`); `Commit` clears then inserts (`1325-1334`); `DeleteSurrounding` becomes cursor arithmetic (`1335-1340`); the galley re-lays out in the same frame (`1347-1352`). The candidate window is positioned purely by output:

```rust
// widgets/text_edit/builder.rs:945-963
if ui.memory(|mem| mem.owns_ime_events(id)) {
    let to_global = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
    ui.output_mut(|o| { o.ime = Some(crate::output::IMEOutput {
        purpose: if password { IMEPurpose::Password } else { IMEPurpose::Normal },
        rect: to_global * inner_rect,
        cursor_rect: to_global * primary_cursor_rect,
        should_interrupt_composition: false }); });
}
```

`IMEOutput { purpose, rect, cursor_rect, should_interrupt_composition }` (`data/output.rs:100-114`) is per-frame state: if the widget stops being drawn, nothing is set — there is no explicit "IME off" transition, which is a real hazard for transiently rebuilt overlay fields.

---

## 6. Drag & drop

The two-slot design is deliberate (`memory/mod.rs:479-498`): a press sets a click target *and* a drag target, so a button atop a scroll area can be either.

```rust
// interaction.rs:191-226 (abridged)
if dragged.is_none() {
    if let Some(widget) = interaction.potential_drag_id.and_then(|id| widgets.get(id)) && widget.enabled {
        let is_dragged = if widget.sense.senses_click() && widget.sense.senses_drag() {
            let pointer_is_inside = hits.contains_pointer.iter().any(|w| w.id == widget.id);
            let could_still_be_clicked = pointer_is_inside || hits.click.is_some_and(|h| h.id == widget.id);
            input.pointer.is_decidedly_dragging() || !could_still_be_clicked
        } else { widget.sense.senses_drag() };
        if is_dragged { dragged = Some(widget.id); }
    }
}
```

Thresholds are `InputOptions` (`input_state/mod.rs:54-122`): `max_click_dist = 6.0`, `max_click_duration = 0.8 s`, `max_double_click_delay = 0.3 s`. `could_any_button_be_click` (`1488-1504`) is false once `has_moved_too_much_for_a_click` (from the **press origin**, `1120-1123`) or the press outlasts the duration; `is_decidedly_dragging` (`1515-1520`) = down/released, not just pressed, not clickable, no click. So `Sense::click_and_drag()` (`sense.rs:77-84`) has intrinsic latency; `Escape` aborts drags (`interaction.rs:137-141`); long-press emits click + `long_touched` (`143-155`).

DnD is not widget state but a plugin holding `Arc<dyn Any + Send + Sync>` (`drag_and_drop.rs:22-47`), cleared on Escape at `on_begin_pass` and on release at `on_end_pass`; `Response::dnd_set_drag_payload` writes it on `drag_started` (`response.rs:499-509`), drop zones read it gated on `contains_pointer` (`511-545`). Plugins (`context.rs:746-752`) are egui's only sanctioned global hook.

---

## 7. Animation

`AnimationManager` (`animation_manager.rs:1-140`) keeps `IdMap<BoolAnim{last_value,last_tick}>` and `IdMap<ValueAnim{from_value,to_value,toggle_time}>`:

```rust
// context.rs:3231-3257 (abridged)
pub fn animate_bool_with_time_and_easing(&self, id, target_value, animation_time, easing) -> f32 {
    let animated_value = self.write(|ctx| ctx.animation_manager.animate_bool(&input, animation_time, id, target_value));
    let animation_in_progress = 0.0 < animated_value && animated_value < 1.0;
    if animation_in_progress { self.request_repaint(); }
    if target_value { easing(animated_value) } else { 1.0 - easing(1.0 - animated_value) }
}
```

`animate_value` extrapolates `+predicted_dt/2` on the toggle frame (`animation_manager.rs:100-104`); `animate_bool` clamps elapsed to `stable_dt` (`57`) so a stall doesn't jump. `Area::fade_in` (`area.rs:632-644`) applies the same idea to `predicted_dt` and repaints while `opacity < 1.0`. The "not rebuilding the world" answer is: the animated value is just a *parameter* of the closure — same `Id`, same entry, nothing to rebuild. The price is that any animation re-runs layout and tessellation of the entire frame at 60 fps; there is no retained-tree animation, no property interpolation, no style transition.

---

## 8. Painting

```rust
// epaint/lib.rs:117-156
pub struct ClippedShape { pub clip_rect: emath::Rect, pub shape: Shape }
pub struct ClippedPrimitive { pub clip_rect: emath::Rect, pub primitive: Primitive }
pub enum Primitive { Mesh(Mesh), Callback(PaintCallback) }
// epaint/mesh.rs:12-74
pub struct Vertex { pub pos: Pos2, pub uv: Pos2, pub color: Color32 /* premultiplied sRGBA */ }
pub struct Mesh { pub indices: Vec<u32>, pub vertices: Vec<Vertex>, pub texture_id: TextureId }
```

`Shape` (`shapes/shape.rs:27-71`) is a closed 64-byte enum: `Noop | Vec | Circle | Ellipse | LineSegment | Path | Rect | Text | Mesh(Arc<Mesh>) | QuadraticBezier | CubicBezier | Callback`. `Shape::Callback` is the only custom-GPU escape hatch; it is never tessellated (`tessellator.rs:1476-1478`, passed through at `1377-1383`) but still carries a clip rect.

`Painter::add` (`painter.rs:213-221`) applies opacity/fade then pushes `ClippedShape` into `GraphicLayers[layer.order][layer.id]` (`layers.rs:113-139`). `ShapeIdx` + `PaintList::set` (`layers.rs:146-156`) lets `Frame` push `Shape::Noop` (`containers/frame.rs:379`) and fill it after children measure (`473-481`). `GraphicLayers::drain` (`layers.rs:213-262`) walks `Order::ALL` then `Areas::order()`, applies layer `TSTransform`s, concatenates to one flat back-to-front list.

Tessellation (`tessellator.rs:1359-1414`) **merges consecutive shapes while `(clip_rect, texture_id)` is unchanged** — draw-call count is governed by clip/texture switches, not shape count. Small filled circles are replaced by pre-rasterized atlas discs (`tessellator.rs:1507-1529`; radii `2^(i/2-1)` up to 8 px baked at atlas creation, `texture_atlas.rs:103-137`).

Anti-aliasing is feathering, not MSAA (`tessellator.rs:767-820`):

```rust
// tessellator.rs:796-810
let mut i0 = n - 1;
for i1 in 0..n {
    let p1 = &path[i1 as usize];
    let dm = 0.5 * feathering * p1.normal;          // default feathering = 1 physical px
    let pos_inner = p1.pos - dm;  let pos_outer = p1.pos + dm;
    out.colored_vertex(pos_inner, fill_color);
    out.colored_vertex(pos_outer, Color32::TRANSPARENT);
    out.add_triangle(idx_inner + i1*2, idx_inner + i0*2, idx_outer + 2*i0);
    out.add_triangle(idx_outer + i0*2, idx_outer + i1*2, idx_inner + 2*i1);
    i0 = i1;
}
```

Every boundary vertex is doubled ±`feathering/2` along its normal with a transparent outer copy: a one-physical-pixel alpha ramp in geometry (`TessellationOptions.feathering_size_in_pixels = 1.0`, `tessellator.rs:669-675`). Rounded rects are just closed paths with circular corner arcs (`tessellator.rs:606`). The scheme's failure modes are visible in the code: sub-feather strokes become "thin line" cases (`1008-1031`, `1248-1252`), thin filled rects become line segments (`1810-1827`), and rects are pixel-snapped per `StrokeKind` so tiled `Inside` strokes don't double-blend (`1829-1862`).

`RectShape::blur_width` (`shapes/rect_shape.rs:50`) is *larger feathering* (`tessellator.rs:1864-1877`) — a soft edge, not a backdrop blur. Text is `TextShape{pos, Arc<Galley>, …}` (`shapes/text_shape.rs:12-34`); per-row vertex meshes are copied with UV normalisation at tessellation (`tessellator.rs:1993-2080`), glyphs baked into `TextureAtlas` with a `dirty` rect for partial uploads (`texture_atlas.rs:60-79`, `202-209`). Backends receive `Vec<ClippedPrimitive>` (`context.rs:2859-2897`); note the warning at `context.rs:2866-2868` that comparing shapes for reuse costs ~50% of tessellating them.

---

## 9. Repaint scheduling / damage

**No damage rectangles exist.** `grep -rin damage` over `egui/src` and `epaint/src` returns zero hits. Scheduling is a per-viewport countdown plus delay:

```rust
// context.rs:130-170 (abridged)
fn request_repaint_after(&mut self, mut delay: Duration, viewport_id, cause) {
    if delay == Duration::ZERO { viewport.repaint.outstanding = 1; }  // 2 repaints: "let things settle"
    if let Ok(predicted) = Duration::try_from_secs_f32(viewport.input.predicted_dt) {
        delay = delay.saturating_sub(predicted);
    }
    viewport.repaint.causes.push(cause);
    if delay < viewport.repaint.repaint_delay {
        viewport.repaint.repaint_delay = delay;
        if let Some(cb) = &self.request_repaint_callback {
            (cb)(RequestRepaintInfo { viewport_id, delay, current_cumulative_pass_nr });
        }
    }
}
```

Granularity is the **whole viewport** (`ViewportOutput::repaint_delay`, `context.rs:2821`). `RepaintCause` (`context.rs:255-304`) is `#[track_caller]` file+line+reason via `ctx.repaint_causes()` (`1947`) — excellent for debugging "why 60 fps". Other sources: `begin_pass_repaint_logic` (`99-124`), `repaint_on_widget_change` (deep-compares `WidgetRects`, `2725-2731`), animations, `Area::fade_in`, sizing passes. Dirty regions exist only as render-time culling: clip rects + `coarse_tessellation_culling` (`tessellator.rs:1758-1762`, `1445-1449`, per text row `2044-2051`), `Ui::is_rect_visible` (`ui.rs:667-669`).

To add region damage you would need: (1) previous-frame paint bounds per widget (egui half-has this in `prev_pass.widgets`); (2) a per-frame dirty set from structural diffs, expanded by old ∪ new rects of moved/dying widgets; (3) tessellation with per-widget mesh identity (egui flattens and merges by clip/texture, so an unchanged widget's meshes are not recoverable); (4) scissored partial clears with framebuffer preservation across the swap; (5) rules that expand damage by filter radius for shadows/blur, and that disable damage where translucent stacking makes it unsound.

---

## 10. What egui does not have

| Missing | Evidence | Why it hurts a capture/annotation tool |
|---|---|---|
| Backdrop blur / filters | `blur_width` = bigger feathering (`tessellator.rs:1864-1877`); no filter nodes in `Shape`/`Primitive` | Frosted toolbar over the screenshot needs a real backdrop pass |
| Region damage | §9: zero `damage` hits | A 3 px brush stroke shouldn't re-tessellate the canvas |
| Real animation state | `AnimationManager` animates `f32`s by `Id` and always by re-running the frame (`animation_manager.rs`) | Springs, velocity-preserving interruption, non-drawn animation |
| Nested stacking contexts / opacity groups | 5 flat `Order` buckets (`layers.rs:10-27`), 1-level sublayers (`memory/mod.rs:1304-1331`), per-shape opacity multiply (`painter.rs:100-108`) | Translucent groups with overlapping children composite wrong |
| Real focus tree / capture | Single `Option<FocusWidget>` (`memory/mod.rs:501-534`), 4-bool `EventFilter`, modal = layer compare | Nested menus/dialogs and focus traps fight each other |
| Zero-lag hit testing | `context.rs:485-505` uses `prev_pass.widgets` | Fresh context menu isn't hoverable for one frame |
| Text-native input / IME lifecycle | `Event::Text(String)` only (`event.rs:27-30`); `IMEOutput` is per-frame state (`data/output.rs:100-114`) | Composition handled per widget; no explicit IME off |
| Spatial index | Linear scan per layer (`hit_test.rs:63-85`) | Thousands of annotation handles need a BVH |
| Shader/effect authoring | Only `Shape::Callback` (`shape.rs:69-70`) | Marching ants, magnifier lens = unmanaged GPU state |

---

## 8 lessons to copy

1. **Resolve input before user code with two slots.** `potential_click_id`/`potential_drag_id` (`memory/mod.rs:486-498`) + `interaction.rs` correctly decides "button on a draggable window on a scroll area" with no widget cooperation. Cheapest, highest-value 200 lines in egui.
2. **Separate `frame_rect` from `widget_rect`.** Consumed vs drawn space (`ui.rs:1239-1251`, `placer.rs:165-192`) makes alignment, justification and spacing fall out of one algorithm.
3. **Soft `max_rect` + infinite cursor.** `Region` (`layout.rs:13-48`) turns a wrong size guess into mild misalignment instead of clipped content.
4. **Id = deterministic hash; state = `(Id, Type)`.** `id.rs:67-88` + `IdTypeMap` (`util/id_type_map.rs:405-420`) gives free persistence and fast probes; ship the `check_for_id_clash` debug painting (`context.rs:1163-1234`) too.
5. **Feathering over MSAA.** A half-pixel transparent fringe along normals (`tessellator.rs:667-674`, `796-810`) is resolution-independent, needs one shader, and works when compositing over unknown pixels.
6. **Merge by `(clip_rect, texture_id)`.** `tessellator.rs:1385-1403` gives few draw calls with no scene graph; a flat `Vec<ClippedPrimitive>` is the smallest sane backend contract (`output.rs:25-33`).
7. **Repaint = delay + cause.** `context.rs:130-170`, `255-304`: a debuggable power model where idle costs nothing. Add damage on top; don't replace it.
8. **Pre-rasterized primitives in the atlas.** Discs as textured quads (`tessellator.rs:1507-1529`, `texture_atlas.rs:103-137`) generalize to shadow LUTs, blur kernels and brush stamps.

## 6 pitfalls to avoid

1. **Hit-testing last frame's rects** (`context.rs:485-505`, `1348-1355`): fresh popups are non-interactive for a frame and in-body `Response`s carry stale geometry. Use this frame's rects (two-phase allocate→interact) or pre-measure every popup.
2. **`request_discard` as a layout fix** (`grid.rs:468`, `containers/resize.rs:286`): doubles frame cost, and the `Area` sizing-pass pattern still shows a glitchy first frame. Build a real intrinsic-size pass.
3. **Single-id focus + layer-compared modality** (`memory/mod.rs:501-534`, `941-960`): focus dies silently when an id isn't in `used_ids`; modality is one layer comparison. Need a focus tree with capture.
4. **Preedit as real text, IME as per-frame state** (`builder.rs:1306-1334`, `data/output.rs:100-114`): undo/validation get murky and "IME off" is implied by omission. Use a composition layer + explicit IME enable/disable/cursor commands.
5. **Per-frame `Response` flags as the only signal** (`context.rs:1425-1566`): the click-vs-drag decision window leaks to user code (`response.rs:408-435`). Deliver press/drag/click once; keep arbitration in the framework.
6. **Flat `Order` + 1-level sublayers + per-shape opacity** (`layers.rs:10-27`, `memory/mod.rs:1304-1331`, `painter.rs:100-108`): frosted overlays need render-to-texture groups, backdrop sampling and group opacity from day one.
