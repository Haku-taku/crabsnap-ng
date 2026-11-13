# GPUI Architecture Notes — for a New Immediate-Mode-Flavored Framework

Source: `/home/ollie/.cargo/git/checkouts/zed-a70e2ad075855582/79fe6a6/crates/gpui/src/` (and `gpui_wgpu`, `gpui_platform`, `gpui_linux`). All line numbers are from the files as read.

---

## 1. Element tree / render model

GPUI is **retained per frame, discarded after** (`element.rs:1-17`): elements are built by `render()`, laid out, painted, and "Before the start of the next frame, the entire element tree and any callbacks they have registered with GPUI are dropped." The `Element` trait (`element.rs:52-139`):

```rust
pub trait Element: 'static + IntoElement {
    type RequestLayoutState: 'static;
    type PrepaintState: 'static;

    fn id(&self) -> Option<ElementId>;
    fn source_location(&self) -> Option<&'static panic::Location<'static>>;
    fn request_layout(&mut self, id: Option<&GlobalElementId>, inspector_id: Option<&InspectorElementId>,
                      window: &mut Window, cx: &mut App) -> (LayoutId, Self::RequestLayoutState);
    fn prepaint(&mut self, id: Option<&GlobalElementId>, inspector_id: Option<&InspectorElementId>,
                bounds: Bounds<Pixels>, request_layout: &mut Self::RequestLayoutState,
                window: &mut Window, cx: &mut App) -> Self::PrepaintState;
    fn paint(&mut self, id: Option<&GlobalElementId>, inspector_id: Option<&InspectorElementId>,
             bounds: Bounds<Pixels>, request_layout: &mut Self::RequestLayoutState,
             prepaint: &mut Self::PrepaintState, window: &mut Window, cx: &mut App);
    // provided:
    fn a11y_role(&self) -> Option<accesskit::Role> { None }
    fn write_a11y_info(&self, _node: &mut accesskit::Node) {}
    fn a11y_synthetic_children(&mut self, _p: &mut Self::PrepaintState, _b: &mut A11ySubtreeBuilder) {}
    fn into_any(self) -> AnyElement { ... }
}
```

`RenderOnce` is far smaller (`element.rs:180-186`): `fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement`; `Render` takes `&mut self` (`element.rs:164-167`). `IntoElement` (`element.rs:146-157`) is only `type Element: Element` + `into_element(self)`. `ParentElement` (`element.rs:189-210`) supplies `.child()`/`.children()` by wrapping each in `AnyElement` (`element.rs:649-655`) — an `ArenaBox<dyn ElementObject>` erasing the three phases (`element.rs:298-315`), so heterogeneous children allocate nothing per frame.

`ElementDrawPhase` (`element.rs:322-347`) is a typestate enum driven by `Drawable<E>`: `Start → RequestLayout → LayoutComputed → Prepaint → Painted`. Each step pushes the element id onto `window.element_id_stack`, computes a `GlobalElementId`, calls the element, pops; `prepaint` (`element.rs:406-518`) additionally calls `layout_bounds(...)`, builds the accesskit node and allocates a `DispatchNodeId` (`dispatch_tree.push_node()`). All three `mem::take` and `panic!` on out-of-order calls — illegal phase order is a runtime, not type, error.

**State persistence keyed by id.** `GlobalElementId` (`element.rs:218-222`) is `Arc<[ElementId]>` plus a precomputed `hash`, folded incrementally by `extend_path_hash` (`element.rs:231-237`) so the window's `ElementIdStack` (`window.rs:989-1020`) never re-hashes a path (`element.rs:213-219`: hashing alone was "a visible share of a frame"). `with_element_state` (`window.rs:4399-4470`) keys `Frame::element_states: FxHashMap<(GlobalElementId, TypeId), ElementStateBox>` (`window.rs:1048`) and records each access in `accessed_element_states` (`window.rs:1049`). `Frame::finish` (`window.rs:1203-1214`) **migrates only accessed states**:

```rust
pub(crate) fn finish(&mut self, prev_frame: &mut Self) {
    for element_state_key in &self.accessed_element_states {
        if let Some((element_state_key, element_state)) =
            prev_frame.element_states.remove_entry(element_state_key)
        { self.element_states.insert(element_state_key, element_state); }
    }
    self.scene.finish();
}
```

Unaccessed state is thus GC'd implicitly. `PrepaintStateIndex`/`PaintIndex` (`window.rs:1071-1092`) with `reuse_prepaint`/`reuse_paint` (`window.rs:4014`, `4079`) apply the same trick to whole view subtrees.

**`Div`** (`elements/div.rs:1855-1861`) is `{ interactivity: Interactivity, children: SmallVec<[StackSafe<AnyElement>; 2]>, … }`; its `Element` impl (`div.rs:1943-2145`) is pure delegation:

```rust
impl Element for Div {
    type RequestLayoutState = DivFrameState;        // SmallVec<[LayoutId; 2]>
    type PrepaintState = Option<Hitbox>;            // the div's own hitbox, if it made one
    fn request_layout(...) -> (LayoutId, DivFrameState) {
        let layout_id = window.with_image_cache(image_cache, |window| {
            self.interactivity.request_layout(global_id, inspector_id, window, cx,
                |style, window, cx| {
                    child_layout_ids = self.children.iter_mut()
                        .map(|child| child.request_layout(window, cx)).collect();
                    window.request_layout(style, child_layout_ids.iter().copied(), cx)
                })
        });
        (layout_id, DivFrameState { child_layout_ids })
    }
    // prepaint: compute content_size from child bounds, then Interactivity::prepaint
    // paint:    Interactivity::paint → style.paint(bounds, ..., |w,c| children.paint(w,c))
}
```

**Between "build" and "paint"** (`Window::draw`, `window.rs:3441-3530`; `draw_roots`, `window.rs:3687-3790`):
1. Enter arena scope; `prepare_frame()`; invalidate dirty entities.
2. `request_layout(...)` — Taffy nodes created recursively, **no layout computed yet**.
3. `stretch_auto_size_to_fill(...)` + `prepaint_as_root(...)`: `layout_as_root` runs `compute_layout(...)` **once** mid-way (`element.rs:562-613`), then walks the tree giving each element real `Bounds` and registering hitboxes, dispatch nodes, listeners, cursors.
4. Deferred draws + prompt/drag/tooltip overlays, then `mouse_hit_test = hit_test(...)` — hover resolves *between* prepaint and paint.
5. `paint(...)` — a second full walk emitting scene primitives.
6. `next_frame.finish(&mut rendered_frame)`, swap, `platform_window.draw(&scene)` (`window.rs:3622-3632`).

So: **two complete tree walks per frame plus one for layout request.** That is the price of separating hitbox registration from painting.

---

## 2. Layout

`TaffyLayoutEngine` (`taffy.rs:31-38`) wraps `TaffyTree<NodeContext>` with `disable_rounding()` (`taffy.rs:43-51`) — GPUI snaps itself. `NodeContext` is a `Box<dyn FnMut(Size<Option<Pixels>>, Size<AvailableSpace>, &mut Window, &mut App) -> Size<Pixels>>` measure fn (`taffy.rs:27-32`).

- `request_layout(style, rem_size, scale_factor, children)` (`taffy.rs:62-83`): `style.to_taffy(...)` then `new_leaf`/`new_with_children`. `LayoutId` is `#[repr(transparent)]` over `taffy::NodeId` (`taffy.rs:394-420`), so slice casts are free.
- `request_measured_layout` (`taffy.rs:85-110`): `new_leaf_with_context` + measure closure — text.
- `compute_layout` (`taffy.rs:187-274`): **evicts** cached bounds for an already-computed subtree, scales `available_space` by `scale_factor` (`taffy.rs:219-228`), un-scales `known_dimensions`/`available_space` around each `measure`, re-snaps via `snap_measured_size_to_device_pixels` (`taffy.rs:255-272`). Engine in **device px**, app in logical.
- `layout_bounds(id, scale_factor)` (`taffy.rs:356-393`): walks to the parent (memoized `absolute_outer_origins`), snaps both corners with `round_half_toward_zero`, divides by `scale_factor`. `taffy.rs:276-355` explains the closure/translation-stability tradeoff behind absolute-edge rounding.
- `Window::layout_bounds` (`window.rs:5293-5306`) adds `pixel_snap_point(element_offset())` — the scroll offset.

**Style resolution.** `Style` (`style.rs:177-183`) derives `Refineable`; `StyleRefinement` is its all-`Option` mirror. `compute_style_internal` (`div.rs:3465-3587`) is the entire cascade — `Style::default()` → `base_style` → `in_focus_style` → `focus_style` → `focus_visible_style` (only when `last_input_was_keyboard`) → `group_hover_style` → `hover_style` → drag-over → `group_active_style` → `active_style` — each a field-wise `style.refine(...)`. No selector engine: "hover" is `hitbox.is_hovered(window)` at paint time (`div.rs:3517-3534`).

Overflow clipping becomes a `ContentMask` in `Style::overflow_mask` (`style.rs:638-694`), pushed via `window.with_content_mask` (`window.rs:4181-4196`), which intersects the parent mask (`window.rs:4187-4189`).

**Units.** `Pixels(pub(crate) f32)` — logical (`geometry.rs:2689`), built by `pub const fn px(f32)` (`geometry.rs:3748`); `ScaledPixels(pub f32)` — device (`geometry.rs:3087`). Conversion is explicit via `.scale(scale_factor)` on `Point`/`Size`/`Bounds`/`Edges`/`Corners` (`geometry.rs:202`, `497`, `1698`, `2129`, `2445`); the `pub(crate)` field stops user code conflating them. Paint snaps through `cover_bounds` (`window.rs:3331-3341`), flooring origin, ceiling the far edge.

---

## 3. Painting / primitives

`Scene` (`scene.rs:41-52`) is **parallel typed vectors plus an ordered op log**:

```rust
pub struct Scene {
    pub(crate) paint_operations: Vec<PaintOperation>,
    primitive_bounds: BoundsTree<ScaledPixels>,   // R-tree, for occlusion / bounds queries
    layer_stack: Vec<DrawOrder>,
    pub shadows: Vec<Shadow>, pub quads: Vec<Quad>, pub paths: Vec<Path<ScaledPixels>>,
    pub underlines: Vec<Underline>, pub monochrome_sprites: Vec<MonochromeSprite>,
    pub subpixel_sprites: Vec<SubpixelSprite>, pub polychrome_sprites: Vec<PolychromeSprite>,
    pub surfaces: Vec<PaintSurface>,
}
```

`Primitive` (`scene.rs:222-231`) has those eight variants; `PaintOperation` (`scene.rs:215-219`) is `Primitive | StartLayer | EndLayer`. `insert_primitive` (`scene.rs:87-140`) culls against `content_mask`, assigns `order` from the enclosing layer (or a fresh `BoundsTree` insert), and appends the typed value plus the op. `finish` (`scene.rs:151-163`) sorts each typed vector by `order` (sprites by `(order, texture_id)`); `batches()` (`scene.rs:172-185`) k-way-merges them back into painting order as `PrimitiveBatch`es (`scene.rs:276-460`) so the renderer replays painter's order in instanced bursts. `replay(range, prev_scene)` (`scene.rs:141-149`) re-inserts a cached op range — what `reuse_paint` uses.

Layers: `push_layer(bounds)` inserts into the R-tree and pushes `StartLayer`; that index is the shared `order` for everything inside. There is **no general z-index** — `paint_layer` (`window.rs:4551-4567`) exists only "for performance reasons." Real stacking is `defer_draw` (`window.rs:4522-4541`): it captures the current `DispatchNodeId`, element-id stack, text style, content mask, rem size and absolute offset, then prepaints/paints after the root by `priority` (`window.rs:3996ff`) — popovers (`elements/deferred.rs`), tooltips, prompts, drag ghost (`window.rs:3749-3754`).

Order *within* an element is hand-written in `Style::paint` (`style.rs:688-765`): drop shadows → background → inset shadows → children → border. Border-only quads split into four strips to avoid shading transparent interiors (`window.rs:4722-4750`).

GPU side: `PlatformWindow::draw(&self, scene: &Scene)` (`platform.rs:1307`) is the only sink. `WgpuRenderer::draw` (`gpui_wgpu/src/wgpu_renderer.rs:1117-1206`) acquires the surface texture and calls `record_frame` (`wgpu_renderer.rs:1472-1613`), which writes instances into a flat buffer with per-type bind groups, then per batch:

```rust
for batch in scene.batches() {
    match batch {
        PrimitiveBatch::Quads(range)   => self.draw_instances(&instance_bindings.quads,
                                          &self.resources().pipelines.quads, instance_range(range), &mut pass),
        PrimitiveBatch::Shadows(range) => ... pipelinesshadows ...,
        PrimitiveBatch::Paths(range) => { drop(pass); let rasterized = self.draw_paths_to_intermediate(...)?;
            pass = encoder.begin_render_pass(/* load: Load */); if rasterized { self.draw_paths_from_intermediate(...)?; } }
        PrimitiveBatch::MonochromeSprites { texture_id, range } => self.draw_sprites(...),
        ...
        PrimitiveBatch::Surfaces(_) => {}   // macOS-only video surfaces
    }
}
```

Every primitive is a **unit quad, drawn instanced**: `pass.draw(0..4, first_instance + range)` (`wgpu_renderer.rs:1729-1746`). Rounded rects, borders and shadows are fragment-shader SDF work (`quad_sdf`, `shaders.wgsl:345-370`; `fs_shadow`, `shaders.wgsl:1005-1053`). Paths are two-pass: rasterize to intermediate, then resolve.

---

## 4. Effects / backdrop blur

**This GPUI has no `BackdropFilter`, no `blur(element)`, no vibrancy in the element tree.** A crate-wide grep: `backdrop_filter`/`BackdropFilter` match nothing; `blur_radius` matches only `style.rs:355,370,377` (`BoxShadow`), `scene.rs:576` (`Shadow`) and `window.rs:4596,4641` (scaling it). "Vibrancy" is purely **window-level**: `WindowBackgroundAppearance` (`platform.rs:2876` `Blurred`, …), implemented with `CABackdropLayer` on macOS (`gpui_macos/src/window.rs:3840-3870`), `org_kde_kwin_blur` on Wayland (`gpui_linux/src/linux/wayland/window.rs:2322-2333`), Mica/Acrylic on Windows (`gpui_windows/src/window.rs:932-936`).

The only blur is **shadow blur**, and it is a single-pass analytic approximation, not a separable convolution:

- `paint_drop_shadows` (`window.rs:4576-4606`) dilates by spread, scales `blur_radius` by `scale_factor`, inserts `Shadow { inset: 0 }`; `paint_inset_shadows` (`window.rs:4612-4650`) uses the negative-spread "hole" rect with `inset: 1` and must run *after* the background.
- In the shader (`shaders.wgsl:985-991`) the quad is expanded by `margin = 3.0 * blur_radius`; `fs_shadow` (`shaders.wgsl:1005-1053`) takes **4 samples** along y, each a closed-form 1-D box-of-erf integral (`blur_along_x`, `shaders.wgsl:333-338`) times a Gaussian — "we can get away with surprisingly few samples."

The ordering constraint a real backdrop blur needs is **not expressed anywhere**: the scene is a flat painter's-order list of instanced quads with no framebuffer sampling. A backdrop-filter primitive would need (a) a render-pass break at the layer boundary capturing the region below to an intermediate texture, then resume with `LoadOp::Load` — machinery the `Path` batch already demonstrates (`wgpu_renderer.rs:1532-1567`) — and (b) an explicit "capture below" op in `Scene` with a hard ordering contract, since unlike a shadow it can never be independent of what was already emitted.

---

## 5. Hit testing and interaction

Hitboxes are `{ id, bounds, content_mask, behavior }` (`window.rs:840-850`), created **only during prepaint** by `insert_hitbox` (`window.rs:5313-5327`), which snapshots `content_mask` and pushes to `Frame::hitboxes`. Hit testing (`window.rs:1173-1194`) is a reverse linear scan:

```rust
pub(crate) fn hit_test(&self, position: Point<Pixels>) -> HitTest {
    let mut set_hover_hitbox_count = false;
    let mut hit_test = HitTest::default();
    for hitbox in self.hitboxes.iter().rev() {
        let bounds = hitbox.bounds.intersect(&hitbox.content_mask.bounds);
        if bounds.contains(&position) {
            hit_test.ids.push(hitbox.id);
            if !set_hover_hitbox_count && hitbox.behavior == HitboxBehavior::BlockMouseExceptScroll {
                hit_test.hover_hitbox_count = hit_test.ids.len(); set_hover_hitbox_count = true;
            }
            if hitbox.behavior == HitboxBehavior::BlockMouse { break; }
        }
    }
    if !set_hover_hitbox_count { hit_test.hover_hitbox_count = hit_test.ids.len(); }
    hit_test
}
```

Insertion order == paint order, so "reverse" == front-to-back. `HitboxBehavior::{Normal, BlockMouse, BlockMouseExceptScroll}` (`window.rs:898-948`) backs `occlude()`/`block_mouse_except_scroll()`. `Hitbox::is_hovered` (`window.rs:870-873`) delegates to `HitboxId::is_hovered` (`window.rs:792-806`), which is `false` under keyboard modality unless the hitbox holds **pointer capture**; `should_handle_scroll` uses the full id list, so overlays block hover without blocking scroll (`window.rs:855-895`).

**`InteractiveElement`** (`div.rs:792-1345`) is builder-only: every method assigns `self.interactivity().<field>`. `id()` returns `Stateful<Self>` (`div.rs:802-807`) purely so `.on_click()` exists only on identified elements *at the type level*. `Interactivity::prepaint` (`div.rs:2353-2456`) calls `should_insert_hitbox` (`div.rs:2458-2484`), inserting one iff anything needs it — behavior flag, cursor, group, scroll, focus, hover style, or any listener.

Listeners are **not** attached to hitboxes: they are global closures pushed onto `Frame::mouse_listeners` at paint time (`div.rs:2810-2830`), each closing over its `Hitbox` and re-checking `hitbox.is_hovered(window)` — so every listener runs for every mouse event, filter *inside* the closure. `on_click` (`div.rs:2997-3188`) is a state machine: Bubble mouse-down records `pending_mouse_down`; Capture mouse-up clears it unconditionally (an intervening `stop_propagation` can't strand it); Bubble mouse-up fires only if still hovered. Drag: a non-capture `MouseMoveEvent` past `DRAG_THRESHOLD` (2.0, `div.rs:52`) takes the `drag_listener`, builds the preview, sets `cx.active_drag = Some(AnyDrag { … })` (`div.rs:3057-3085`) and stops propagation. `on_drag<T,W>(value, |&T, Point, …| -> Entity<W>)` is `div.rs:639-659`; `AnyDrag` at `app.rs:3556-3572`. Drop compares `value.type_id()` to `drop_listeners` and downcasts (`div.rs:2962-2994`). `on_drag_move<T>` registers in **Capture**, so it fires regardless of hover (`div.rs:360-381`).

Hover: (1) style — `hover`/`group_hover`/`active`/`focus` refine in `compute_style_internal`; (2) callback — `on_hover` (`div.rs:699-708`) is a *transition* detector in the **Bubble** phase (`div.rs:3168-3205`) diffing `element_state.hover_listener_state`, plus a `MouseExitEvent` handler since "the pointer can leave the window without a final MouseMove."

Cursor: `set_cursor_style(style, hitbox)` during paint (`window.rs:4145-4151`) records a *request*; `Frame::cursor_style` (`window.rs:1158-1171`) folds requests in reverse skipping non-hovered hitboxes, and `reset_cursor_style` (`window.rs:5578-5587`) applies it. A drag overrides via `set_window_cursor_style` (`div.rs:2621-2625`).

---

## 6. Focus & keyboard

`FocusHandle` (`window.rs:542-552`) is `{ id, handles: Arc<RwLock<FocusMap>>, tab_index, tab_stop }`, refcounted so it survives frames and can live in view state (`Drop` queues release, `window.rs:669-681`). Focusable elements get one lazily in `Interactivity::request_layout` (`div.rs:2307-2325`). Registration is *prepaint*: `set_focus_handle` (`window.rs:5350-5358`) sets `next_frame.focus` when focused and `dispatch_tree.set_focus_id(id)`. `Window::focus` (`window.rs:2418-2428`) sets `self.focus`, bumps `focus_generation`, clears pending keystrokes, refreshes.

**Dispatch tree.** `DispatchTree` (`key_dispatch.rs:71-80`) is rebuilt each frame in lockstep with the element walk: `push_node` on prepaint entry, `set_active_node` on paint, `pop_node` on exit. Each `DispatchNode` (`key_dispatch.rs:83-94`) holds `key_listeners`, `action_listeners`, `context`, `focus_id`, `view_id`, `parent`; `set_active_node` (`key_dispatch.rs:178-210`) rewinds/pushes the stack to rebuild `context_stack` — which is what makes paint-order == listener scope. `dispatch_path(focus)` (`key_dispatch.rs:563-571`) returns **root → focused**.

Keyboard dispatch (`Window::dispatch_key_event`, `window.rs:6053-6210`): force a synchronous `draw` if dirty (so the dispatch tree is fresh), build `dispatch_path`, extract the `Keystroke`, check `input_preference` (character input vs bindings, `window.rs:6035-6051`), run **interceptors**, then `dispatch_key(pending, keystroke, &path)`. `dispatch_key` (`key_dispatch.rs:483-519`) concatenates pending keystrokes, asks `Keymap::bindings_for_input`, and returns one of: more input needed, bindings to run, or "no match at length n → pop and replay" (`replay_prefix`). A `PENDING_INPUT_TIMEOUT` flushes pending input as replayed keystrokes (`key_dispatch.rs:521-536`, `window.rs:6240-6260`).

**Keymap matching** (`keymap.rs:165-260`): bindings are filtered by `binding_enabled` (`keymap.rs:274-280`) = `predicate.depth_of(contexts)`, then sorted by `(depth desc, keymap index desc)` — *deeper context wins; later registration breaks ties*. `KeyContext` (`keymap/context.rs:10-18`) is a list of `key` / `key=value` entries parsed from strings like `"Editor mode = normal"`.

`Action` (`action.rs:117-160`) requires `Any + Send`: `boxed_clone`, `partial_eq`, `name`, `name_for_type`, `build(serde_json::Value)`, `action_json_schema`, `deprecated_aliases`, `deprecation_message`, `documentation`. Derives need `Clone + PartialEq + Deserialize + JsonSchema` — which is what lets keymaps be JSON.

Action dispatch (`dispatch_action_on_node_inner`, `window.rs:6533-6645`) runs four passes: global capture → **per-node capture (root → leaf)** → **per-node bubble (leaf → root)** → global bubble. In the bubble phase it sets `cx.propagate_event = false` *before* each handler, so **actions stop propagation by default** (`window.rs:6605`), while mouse events propagate by default. `on_action` handlers are `(TypeId, closure)` pairs on the active dispatch node (`div.rs:1157`).

IME is **pull-based; there is no `set_ime_position`**. The focused element registers an `InputHandler` during **paint** via `handle_input(&FocusHandle, impl InputHandler, cx)` (`window.rs:5409-5425`), effective only if the handle is focused. The trait (`platform.rs:2313-2400`) mirrors Cocoa's `NSTextInputClient`: `selected_text_range`, `marked_text_range`, `text_for_range`, `replace_text_in_range`, `replace_and_mark_text_in_range`, `unmark_text`, `paste`, `bounds_for_range`, plus `accepts_text_input` and `text_input_configuration`. `PlatformInputHandler` (`platform.rs:2014-2060`) wraps it with an `AsyncWindowContext` and a `focus_id` so "backends can detect a field change without restarting the keyboard session." Candidate placement comes from `bounds_for_range` — macOS's `firstRectForCharacterRange` calls it and flips y (`gpui_macos/src/window.rs:3396-3422`). `ElementInputHandler<V: EntityInputHandler>` (`input.rs:152-167`) adapts ordinary views. Changes push via `text_input_state_changed(...)` (`window.rs:5429-5449`, enum `platform.rs:1212-1222`); configuration is re-sent only on change, "because reconfiguring a live input session can restart the IME connection" (`platform.rs:1259-1262`).

---

## 7. Event bubbling

Yes — for mouse and actions, over a flat listener list. `DispatchPhase` is `{ Bubble (default), Capture }` (`window.rs:93-122`); per its docs *capture* runs mouse listeners back-to-front and keyboard root→focused, *bubble* the reverse. All of it hangs on one `bool`:

```rust
pub(crate) fn propagate_event: bool,          // App (app.rs:921)
pub fn stop_propagation(&mut self) { self.propagate_event = false; }   // app.rs:2780-2782
pub fn propagate(&mut self) { self.propagate_event = true; }           // app.rs:2788-2790
```

`stop_propagation` lives on **`App`**, not a context; `propagate()` lets a handler opt back in after someone else stopped.

`dispatch_mouse_event` (`window.rs:5974-6030`) is the whole machine:

```rust
let mut mouse_listeners = mem::take(&mut self.rendered_frame.mouse_listeners);
for listener in &mut mouse_listeners {                 // Capture: back → front
    listener.as_mut().unwrap()(event, DispatchPhase::Capture, self, cx);
    if !cx.propagate_event { break; }
}
if cx.propagate_event {
    for listener in mouse_listeners.iter_mut().rev() { // Bubble: front → back
        listener.as_mut().unwrap()(event, DispatchPhase::Bubble, self, cx);
        if !cx.propagate_event { break; }
    }
}
```

("Front" == last painted == topmost, since listeners are pushed in paint order.) `cx.propagate_event` is reset per event (`window.rs:5652`) and reported back as `DispatchEventResult { propagate, default_prevented }` (`window.rs:2233-2236`, `window.rs:5802`). The second, orthogonal flag is `window.prevent_default()` (`default_prevented`) — used by focus-on-click so only the nearest focusable acts (`div.rs:2823-2835`).

`EventEmitter<E: Any>` (`gpui.rs:359`) is **not** propagation — it is a marker declaring which events an entity type emits; `Context::emit` (`app/context.rs:780`) fans out to observers. It composes with, but does not replace, the dispatch tree.

---

## 8. Animation

`Animation { duration, oneshot, synced, easing: Rc<dyn Fn(f32)->f32>, max_fps }` (`elements/animation.rs:15-27`), with builders `with_easing`/`repeat`/`repeat_synced`/`with_max_fps`. `with_animation(id, animation, |el, delta| el)` (`animation.rs:82-100`) wraps the element in `AnimationElement<E>` — an `Element` whose `RequestLayoutState` *is* the animated child.

State lives in `with_element_state` keyed by the wrapper's own `id` (`animation.rs:161-166` sets `Element::id()` = the supplied id): `AnimationState { start: Instant, animation_ix: usize, delayed_frame_pending }`. `request_layout` (`animation.rs:394-479`) updates the clock, advances through the animation chain, applies easing, re-invokes the animator, and requests a frame:

```rust
let mut delta = elapsed.as_secs_f32() / duration.as_secs_f32();
...
let delta = (self.animations[animation_ix].easing)(delta);
let element = self.element.take().expect("should only be called once");
let mut element = (self.animator)(element, animation_ix, delta).into_any_element();
if !done {
    match self.animations[animation_ix].max_fps {
        Some(max_fps) if ... => { /* spawn timer → cx.notify(view) */ }
        _ => window.request_animation_frame(),
    }
}
((element.request_layout(window, cx), element), state)
```

Because the animator takes the element builder by value, animation re-runs the *entire builder chain* each frame — `.with_animation()` is a retained-mode escape hatch, not an immediate-mode one. Springs (`spring.rs:12-24`, `elements/animation.rs:124-166`, `254-355`) store `SpringState { position, velocity }` + `updated_at` in element state; `SpringConfig::step` is an analytic frame-rate-independent stepper (`spring.rs:41-53`), `step_ramp` handles moving targets (`spring.rs:55-77`). Both honor `cx.reduce_motion()`.

**Repaint scheduling.** `cx.notify(entity)` (`app.rs:3187-3210`) dirties every window that actually renders that entity; `Window::refresh` (`window.rs:2388-2394`) sets `invalidator.set_dirty(true)` only when not already drawing. `request_animation_frame` (`window.rs:2742-2746`) is sugar for `on_next_frame(|_, cx| cx.notify(current_view))`; `on_next_frame` (`window.rs:2722-2731`) pushes to `next_frame_callbacks` and calls `schedule_frame()` + `invalidator.wake_platform()` because "next-frame callbacks create frame demand without dirtying the window." `present` (`window.rs:3622-3632`) then hands `&scene` to the platform.

---

## 9. The platform abstraction

`trait Platform: 'static` (`platform.rs:366-470`): `platform_info`, executors, `text_system`, `run(on_finish_launching)`, `quit`, `restart`, `activate`, `displays`, `primary_display`, `active_window`, `open_window(handle, WindowParams) -> Box<dyn PlatformWindow>`, `window_appearance`, `open_url`, prompts, lifecycle callbacks (`on_quit`, `on_reopen`, `on_system_sleep/wake`), plus mobile ones.

`trait PlatformWindow: HasWindowHandle + HasDisplayHandle` (`platform.rs:1224+`) is the real contract, ~80 methods; the frame-critical subset:

```rust
fn bounds(&self) -> Bounds<Pixels>;
fn content_size(&self) -> Size<Pixels>;
fn prepare_frame(&self) -> bool;                 // sample geometry; true => invalidate view caches
fn resize(&mut self, size: Size<Pixels>);
fn scale_factor(&self) -> f32;
fn set_input_handler(&mut self, input_handler: PlatformInputHandler);
fn take_input_handler(&mut self) -> Option<PlatformInputHandler>;
fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>);
fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>);
fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>);
fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>);
fn draw(&self, scene: &Scene);
fn schedule_frame(&self) {}
fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas>;
fn is_subpixel_rendering_supported(&self) -> bool;
```

**The renderer/presenter boundary is exactly `scene: &Scene`.** Everything above it (layout, hit testing, text shaping, scene assembly) is platform-free; everything below is a `dyn PlatformWindow` whose job on `draw` is to hand the scene to *some* renderer. Wayland (`gpui_linux/src/linux/wayland/window.rs:2011-2059`) holds `renderer: Option<WgpuRenderer>` (`window.rs:118`), does GPU recovery and frame-callback bookkeeping, then `renderer.draw(scene) -> bool`. No wgpu type leaks *up* into `Scene`, but `Scene` leaks the renderer's ABI *down*: `#[repr(C)]` instance structs (`scene.rs:535-552`), `PaddedBool32` to avoid padding UB when reinterpreted as `&[u8]` (`scene.rs:29-39`), `AtlasTextureId` in `PrimitiveBatch`. Swapping the GPU backend is therefore bound to that ABI.

Backends plug in via one `cfg` ladder: `current_platform(headless)` (`gpui_platform/src/gpui_platform.rs:67-96`) matches `target_os` and returns `Rc<dyn Platform>` for iOS, macOS, Windows, Linux/FreeBSD (x11/wayland inside `gpui_linux`), wasm.

---

## 10. What is deliberately **not** immediate-mode, and the Rust ergonomics you'd inherit

**Not immediate-mode:**

1. **Three phases with per-element state types.** An egui frame is `|ui|`; a GPUI element is a typestate machine threading two associated state types through `request_layout → prepaint → paint`, panicking on out-of-order calls (`element.rs:402`, `515`, `557`).
2. **Cross-frame storage keyed by a path hash.** Hover, `pending_mouse_down`, `clicked_state`, tooltips, animation clocks, spring velocity, scroll offsets live in `(GlobalElementId, TypeId)` maps (`window.rs:1048`, `4399-4470`), migrated only if re-accessed (`window.rs:1204-1214`).
3. **Retained callback registration.** Listeners are `'static` closures pushed at paint time (`div.rs:2810-2830`), dropped next frame — outliving the render function by exactly one frame. Every interaction is a `DispatchPhase` state machine (`on_click`, `div.rs:2997-3188`).
4. **Entity/model layer above elements.** `Entity<T>` + `Context<T>` + observers + `cx.notify()` (`app.rs:3187`): a retained reactive graph *behind* the "pure function of state" tree. Clean views are spliced from the previous scene rather than re-rendered (`view.rs:544-580`).
5. **Draw-order lists that outlive the frame.** `mouse_listeners`, `hitboxes`, `cursor_styles`, `deferred_draws`, `dispatch_tree.nodes` are frame-scoped retained vectors; correctness requires prepaint and paint to visit elements in the *same* order (index bookkeeping at `window.rs:3890-3900`).
6. **Mutable builder trees.** `Div::prepaint(&mut self)` drains listener vectors with `mem::take`; a `Div` renders once.

**Ergonomics you'd inherit if you copied this:**

- **`impl IntoElement` fever.** 24 impls in the crate. Each adds inference work; `impl IntoElement` in return position is opaque `impl Trait`, breaking `?Sized` composition and yielding diagnostics that name the opaque type.
- **Associated-type explosion.** Every `Element` names two state types in its header; `AnyElement` erases them, and inspecting a tree means `downcast_mut::<T>()` (`element.rs:664-666`).
- **`&mut Window, &mut App` split.** Two mutable contexts per call; "paint-only vs prepaint-only" is enforced by `debug_assert_paint()`/`debug_assert_prepaint()` at runtime (`window.rs:5227-5228`, `4406`, `5295`), not by types.
- **Closure lifetime taxes.** Listeners must be `'static + Fn`; capturing view state needs `cx.listener(...)` or `Rc<RefCell<…>>`; `on_hover` is a `debug_assert!(...is_none())` because a builder cannot express "at most one" (`div.rs:703-707`).
- **Panics as type errors**: wrong phase order, double `.id()`, duplicate hover, reentrant `with_element_state`.
- **`stacker` as mitigation** for recursive paint blowing the stack (`taffy.rs:19-23`, `div.rs:44-47`) — the design is deep-recursion-bound.

---

## 5 things worth copying

1. **`AnyElement` over a per-frame bump arena** (`element.rs:649-655`, `arena.rs:81-218`). Heterogeneous children, zero per-frame heap allocation, one bulk free. `Box<dyn Widget>` per frame is the default trap.
2. **`GlobalElementId` as an *incrementally hashed path*** (`element.rs:218-300`, `window.rs:989-1020`). O(1) per frame at any depth, cheap `Eq`/`Hash`, and the same hash doubles as the accesskit `NodeId` — the cheapest answer to "which widget is this across frames."
3. **Phase-typed draw state (`ElementDrawPhase`)** (`element.rs:322-347`, `359-612`). Declaration / layout-resolution / paint ordering is what hitbox-vs-paint correctness needs, and it hosts "measure text once, then paint." Encode it in types, not debug asserts.
4. **Sorted slices + k-way-merge `Scene`** (`scene.rs:41-185`, `271-460`). Painter's-order semantics for correctness, long homogeneous runs for the GPU (`draw(0..4, instances)` per batch) — the one decision that makes "one quad per call" viable.
5. **`DispatchPhase` + one boolean for propagation** (`window.rs:93-122`, `5974-6030`; `app.rs:2780-2790`). Capture/bubble via `stop_propagation`/`propagate` is ~40 lines, needs no per-event target resolution, and handles "button inside a scrollable inside a pane." Copy the mechanism and the `default_prevented` flag, not the listener model.

## 5 things worth NOT copying

1. **Two full tree walks per frame plus a third for layout request.** `request_layout` → `prepaint` → `paint` runs every builder chain three times (`window.rs:3719-3760`). Fuse request and prepaint unless you genuinely need Taffy-style measured leaves.
2. **A global `'static` closure per listener, filtered inside.** Every mouse event runs every listener in the window, each doing `hitbox.is_hovered(window)` (`div.rs:2810-2830`, `window.rs:5974-6010`). Carry one hit-path (`SmallVec<Id>` from a single reverse scan, as `hit_test` computes at `window.rs:1173`) and dispatch to it.
3. **`cx.listener()` / `Rc<RefCell<>>` for handler state.** The unavoidable cost of `'static + Fn`. If your framework owns the frame, handlers can borrow `&mut State` directly — deleting a whole class of borrow panics.
4. **The `Entity`/observer layer for element-local state.** `with_element_state` already gives id-keyed persistence (`window.rs:4399-4470`); a reactive entity graph on top for "is this hovered" is enormous complexity for an `egui::Memory`-style map.
5. **`RequestLayoutState` + `PrepaintState` associated types.** They force every custom `Element` to name two types and thread them through three signatures, and force `AnyElement` to erase them. Prefer one `Bounds` plus one opaque per-id state slot.
