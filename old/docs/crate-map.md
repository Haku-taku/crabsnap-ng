# crate 映射表（旧模块 → 新 crate）

> 配套文档：[`architecture.md`](architecture.md)（分层与决策）、[`migration.md`](migration.md)（步骤与验收）
> 数据来源：对 `/home/ollie/git/crabsnap` 的静态扫描（2026-10-25，HEAD `6391f01`）

---

## 1. 顶层映射一览

`src/` 下 28 个顶层模块（18 个目录 + 10 个单文件模块）+ `crates/settings` 的 9 个模块
→ 43 个 crate。
「LOC」含测试；「外部依赖」是该 crate 实际 `use` 到的第三方 crate（不是全部可用依赖）。

| # | 新 crate | 层 | 旧路径 | 文件 | LOC | 外部依赖 |
|---|---|---|---|---|---|---|
| 1 | `crabsnap-core` | L0 | `core/{error,logging,i18n,testing}`、`paths`、`parallel`、`select` | 9 | 1257 | thiserror, log, env_logger, fluent-bundle, unic-langid, libc |
| 2 | `crabsnap-geometry` | L0 | `core/geometry`、`ui/control/width::quantize` | 1 | 153 | kurbo |
| 3 | `crabsnap-color` | L0 | `core/hdr/*`、`core/color/*`、`settings/color.rs`、`wayland/color_proto::OutputColour` | 13 | 4189 | half, log |
| 4 | `crabsnap-keymap` | L0 | `crates/settings/keymap.rs`、`wayland/xkb.rs` 的 `KEY_*`、`input/bridge.rs` 的键表 | 1+ | 608+ | — |
| 5 | `crabsnap-text` | L0 | `text/*` | 3 | 1555 | parley, skrifa, tiny-skia, kurbo, log |
| 6 | `crabsnap-input` | L0 | 新增：`wayland/pointer.rs` 事件类型、`wayland/cursor::CursorShape`、`input/mod.rs::BTN_*` | 新增 | ~500 | keymap |
| 7 | `crabsnap-selection` | L0 | `selection.rs` | 1 | 565 | kurbo, geometry |
| 8 | `crabsnap-ffmpeg` | L1 | `ffmpeg/*`、`build.rs`(still.c) | 1 | 565 | （原始 FFI） |
| 9 | `crabsnap-image` | L1 | `model/*` | 17 | 4341 | png, half, log |
| 10 | `crabsnap-gl` | L1 | `gl/*` | 7 | 1339 | glow, khronos-egl, libloading(新), libc, log |
| 11 | `crabsnap-portal` | L1 | `platform/portal.rs`、`platform/probe.rs`、`platform/mod.rs`、`platform/linux/mod.rs` | 4 | 220 | zbus |
| 12 | `crabsnap-daemon` | L1 | `platform/linux/daemon/{mod,protocol,client,discovery}` | 4 | 927 | serde, serde_json, libc, log |
| 13 | `crabsnap-wayland` | L2 | `platform/linux/wayland/*`、`platform/linux/swcursor.rs`、`protocols/*.xml` | 19 | 5646 | wayland-client/-protocols/-wlr/-backend/-scanner, rustix, memmap2, libc, log |
| 14 | `crabsnap-capture` | L2 | `platform/linux/capture/*` | 13 | 2829 | rustix, zbus, serde_json, wayland-client/-protocols/-wlr, log |
| 15 | `crabsnap-inject` | L2 | `platform/linux/inject.rs` | 1 | 737 | libc, serde, zbus, wayland-client/-wlr |
| 16 | `crabsnap-hotkey` | L2 | `platform/linux/hotkey/*` | 3 | 789 | zbus, serde, wayland-client, wayland-scanner, log |
| 17 | `crabsnap-element` | L2 | `platform/linux/element/*` | 3 | 1171 | zbus |
| 18 | `crabsnap-config` | L3 | `settings.rs`（去 annotate 部分）、`crates/settings/store.rs` | 1+1 | ~1100 | serde, toml, log |
| 19 | `crabsnap-scene` | L3 | `scene/*` | 4 | 1656 | kurbo |
| 20 | `crabsnap-live` | L3 | `live/*` | 2 | 309 | — |
| 21 | `crabsnap-record` | L3 | `record/*`、`build.rs`(shim.c/audio.c) | 9 | 4003 | libc, serde, serde_json, log |
| 22 | `crabsnap-ocr` | L3 | `ocr.rs` | 1 | 391 | serde_json |
| 23 | `crabsnap-translate` | L3 | `translate.rs` | 1 | 301 | serde_json |
| 24 | `crabsnap-stitch` | L3 | `stitch/*` | 2 | 1191 | — |
| 25 | `crabsnap-longshot` | L3 | `longshot.rs` | 1 | 1156 | log |
| 26 | `crabsnap-notify` | L3 | `notify.rs` | 1 | 253 | zbus |
| 27 | `crabsnap-clipboard` | L3 | `clipboard.rs` | 1 | 607 | rustix, wayland-client/-wlr, log |
| 28 | `crabsnap-actions` | L3 | `input/{actions,gestures,keymap,bridge}` | 4 | 1013 | — |
| 29 | `crabsnap-render` | L4 | `render/*` 除 `gpu`；+ `layer/stack::device_damage`、`ui/engine/damage::Dirty`、`ui/chrome/magnifier::{window_span,loupe_center}` | 20+ | 9651 | tiny-skia, kurbo, half, log, hotpath |
| 30 | `crabsnap-render-gpu` | L4 | `render/gpu/*` | 13 | 8552 | glow, half, kurbo, log, hotpath |
| 31 | `crabsnap-annotate` | L5 | `annotate/*`、`settings.rs` 工具状态持久化、`ui/chrome::node_grips` | 24+ | 13074 | kurbo, log |
| 32 | `crabsnap-card` | L5 | `card/*` | 5 | 1660 | kurbo, pulldown-cmark |
| 33 | `crabsnap-ui-base` | L5 | `ui/{base,widgets,engine}/*`、`ui/chrome/icons.rs`、`ui/panel/hit.rs` 的命中词汇、`ui/chrome` 的 `MARK_BOX_*`/`grip`、`ui/control/settings::SliderRange`、`ui/panel/toolbar::{PANEL_BG,anchor::snap_zone_rects}`、`ui/mod.rs::FROST_SIGMA` | 18+ | 7840 | kurbo, log |
| 34 | `crabsnap-ui-chrome` | L5 | `ui/chrome/*`（`icons` 962 行已下沉到 `ui-base`） | 3 | 1450 | kurbo |
| 35 | `crabsnap-ui-control` | L5 | `ui/control/*` | 17 | 6981 | kurbo, log |
| 36 | `crabsnap-ui-panel` | L5 | `ui/panel/*` | 44 | 15305 | kurbo, log, hotpath |
| 37 | `crabsnap-ui` | L5 | `ui/mod.rs`（只留再导出与 `toolbar_blur_kind`） | 1 | 105 | — |
| 38 | `crabsnap-settings-ui` | L5 | `crates/settings/src/{app,color_picker,sidebar,theme,wake,widgets,lib}.rs` + example | 8 | 9148− | egui |
| 39 | `crabsnap-compositor` | L6 | `layer/*`、`layers/debug_overlay/*` | 26 | 9291 | glow, wayland-client/-protocols/-wlr, kurbo, log, hotpath |
| 40 | `crabsnap-layers` | L6 | `layers/{freeze.rs,sticker/*}` | 13 | 4793 | wayland-client/-wlr, kurbo, log |
| 41 | `crabsnap-app` | L7 | `app/*`、`input/mod.rs`、`platform/linux/daemon/server.rs` | 35 | 13975 | clap, clap_complete, serde_json, libc, log, hotpath |
| 42 | `crabsnap-settings-host` | L7 | `app/cli/commands/settings/{mod,annotate}.rs` | 2 | 2167 | eframe, egui |
| 43 | `crabsnap` | L7 | `lib.rs`、`main.rs`、`examples/*` | 2+34 | 111 + 5.5k | hotpath |
| — | `xtask` | 工具 | 新增（移植 `scripts/check-notes-index.sh` 等） | 新增 | — | cargo_metadata, toml |

**校验**：`142103 + 0.5k ≈ 142.6k` 行（旧 `src/` + `crates/settings/src` 共 142,103 行）。
LOC 按「旧文件归属」统计；从 `ui/chrome`、`ui/panel`、`ui/control` 下沉到 `ui-base` 的
碎片（约 1.6k 行）没有重复计入，因此 `ui-base` 的实际行数会比表中略高。

---

## 2. 逐文件映射（迁移执行清单）

> 每行 = 旧文件(行数) → 新位置。**内部目录结构保持不变**（`mod.rs` → `lib.rs`，
> 其余文件名与相对层级不变），这样 crate 内部的所有 `super::`/`self::` 引用无需改动。

### 2.1 `crabsnap-core`
```
core/mod.rs(14)            → crates/core/src/lib.rs
core/error/mod.rs(63)      → crates/core/src/error/mod.rs
core/logging/mod.rs(117)   → crates/core/src/logging/mod.rs
core/logging/rules.rs(164) → crates/core/src/logging/rules.rs
core/i18n/mod.rs(339)      → crates/core/src/i18n/mod.rs
core/i18n/locales/*.ftl    → crates/core/src/i18n/locales/*.ftl
core/testing.rs(74)        → crates/core/src/testing.rs
paths.rs(337)              → crates/core/src/paths.rs
parallel.rs(40)            → crates/core/src/parallel.rs
select.rs(109)             → crates/core/src/select.rs
```

### 2.2 `crabsnap-geometry`
```
core/geometry/mod.rs(153)         → crates/geometry/src/lib.rs
ui/control/width.rs::quantize(28) → crates/geometry/src/quantize.rs（新增，函数体原样）
```

### 2.3 `crabsnap-color`
```
core/hdr/mod.rs(33)          → crates/color/src/hdr/mod.rs
core/hdr/{blur,colour,constants,detect,primaries,tonemap,transfer}.rs → 同名
core/hdr/frame/mod.rs(1685)  → crates/color/src/hdr/frame/mod.rs
core/hdr/frame/tests.rs(175) → crates/color/src/hdr/frame/tests.rs
core/color/mod.rs(194)       → crates/color/src/color/mod.rs
core/color/output.rs(210)    → crates/color/src/color/output.rs
crates/settings/src/color.rs(426) → crates/color/src/format.rs
wayland/color_proto.rs::OutputColour(约 130) → crates/color/src/output_colour.rs
```

### 2.4 `crabsnap-keymap`
```
crates/settings/src/keymap.rs(608) → crates/keymap/src/lib.rs（`egui::Key` → 自有 `Key`）
wayland/xkb.rs 的 KEY_* 常量     → crates/keymap/src/keysym.rs
input/bridge.rs 的 key_from_evdev/key_from_keysym/combo_of → crates/keymap/src/evdev.rs
```

### 2.5 `crabsnap-text`
```
text/mod.rs(925) → crates/text/src/lib.rs
text/edit.rs(570) → crates/text/src/edit.rs
text/wrap.rs(60)  → crates/text/src/wrap.rs
```

### 2.6 `crabsnap-input`（新）
```
wayland/pointer.rs 的 PointerEvent/KeyEvent/KeyMods/ModsSource → crates/input/src/event.rs
wayland/cursor.rs::CursorShape → crates/input/src/cursor.rs
input/mod.rs 的 BTN_LEFT/BTN_RIGHT/BTN_MIDDLE → crates/input/src/button.rs
```

### 2.7 `crabsnap-selection`
```
selection.rs(565) → crates/selection/src/lib.rs
```

### 2.8 `crabsnap-ffmpeg`
```
ffmpeg/mod.rs(565)   → crates/ffmpeg/src/lib.rs
build.rs 的 still.c 段 → crates/ffmpeg/build.rs
src/ffmpeg/still.c   → crates/ffmpeg/src/still.c
```

### 2.9 `crabsnap-image`
```
model/mod.rs(18)         → crates/image/src/lib.rs
model/{color,document,encoded,frame,picture,pixels}.rs → 同名
model/codec/mod.rs(419)  → crates/image/src/codec/mod.rs
model/codec/{avif,ffmpeg_still,jpeg,jxl,params,png,radiance,scale,webp}.rs → 同名
```

### 2.10 `crabsnap-gl`
```
gl/mod.rs(131)      → crates/gl/src/lib.rs（去掉 `#[cfg(all(crabsnap_egl,crabsnap_gbm))]`）
gl/context.rs(379)  → crates/gl/src/context.rs（`renders_a_known_pixel` 移出，见 2.30）
gl/dmabuf.rs(542)   → crates/gl/src/dmabuf.rs
gl/egl.rs(108)      → crates/gl/src/egl.rs
gl/gbm.rs(132)      → crates/gl/src/gbm.rs（`#[link(name="gbm")]` → `libloading`）
gl/fourcc.rs(16)    → crates/gl/src/fourcc.rs
gl/probe.rs(31)     → crates/gl/src/probe.rs
```

### 2.11 `crabsnap-portal`
```
platform/portal.rs(156)  → crates/portal/src/lib.rs
platform/probe.rs(41)    → crates/portal/src/probe.rs
platform/mod.rs(7)       → 删除（内容并入 lib.rs 的 `pub use`）
platform/linux/mod.rs(16) → 删除（同上）
```

### 2.12 `crabsnap-daemon`
```
platform/linux/daemon/mod.rs(36)      → crates/daemon/src/lib.rs
platform/linux/daemon/protocol.rs(554) → crates/daemon/src/protocol.rs
platform/linux/daemon/client.rs(204)  → crates/daemon/src/client.rs
platform/linux/daemon/discovery.rs(133) → crates/daemon/src/discovery.rs
platform/linux/daemon/server.rs(958)  → **crates/app/src/daemon.rs**（见 2.41）
```

### 2.13 `crabsnap-wayland`
```
platform/linux/wayland/mod.rs(1078) → crates/wayland/src/lib.rs
其余 wayland/*.rs（16 个）→ crates/wayland/src/同名
platform/linux/swcursor.rs(161) → crates/wayland/src/swcursor.rs
protocols/hyprland-global-shortcuts-v1.xml → crates/wayland/protocols/
```

### 2.14 `crabsnap-capture`
```
platform/linux/capture/mod.rs(136) → crates/capture/src/lib.rs
capture/{all,ext_copy,follow,fourcc,frame,kwin,portal,probe,request,shm_source,window,wlr_screencopy}.rs → 同名
```

### 2.15 `crabsnap-inject`
```
platform/linux/inject.rs(737) → crates/inject/src/lib.rs
```

### 2.16 `crabsnap-hotkey`
```
platform/linux/hotkey/mod.rs(223)      → crates/hotkey/src/lib.rs（`action_description` 改收 `ShortcutSpec`）
platform/linux/hotkey/hyprland.rs(223) → crates/hotkey/src/hyprland.rs
platform/linux/hotkey/portal.rs(343)   → crates/hotkey/src/portal.rs
```

### 2.17 `crabsnap-element`
```
platform/linux/element/mod.rs(102)            → crates/element/src/lib.rs
platform/linux/element/accessibility.rs(389)  → crates/element/src/accessibility.rs
platform/linux/element/pixels.rs(680)         → crates/element/src/pixels.rs
```

### 2.18 `crabsnap-config`
```
settings.rs(518) 的 Settings/load/config_path/各 *Settings 表 → crates/config/src/lib.rs
settings.rs 的 annotate_state*/save_annotate_state/encode_annotate_settings/tool_*_from（约 300 行）→ crates/annotate/src/persist.rs
crates/settings/src/store.rs(916) → crates/config/src/store.rs（去掉 egui 相关）
```

### 2.19 `crabsnap-scene`
```
scene/mod.rs(1139) → crates/scene/src/lib.rs
scene/builder.rs(329) → crates/scene/src/builder.rs
scene/hit.rs(59)      → crates/scene/src/hit.rs
scene/image.rs(129)   → crates/scene/src/image.rs
```

### 2.20 `crabsnap-live`
```
live/mod.rs(230) → crates/live/src/lib.rs
live/monitor.rs(79) → crates/live/src/monitor.rs
```

### 2.21 `crabsnap-record`
```
record/mod.rs(811)     → crates/record/src/lib.rs
record/{audio,avcodec,fit,params,plan,replay,session,source}.rs → 同名
src/record/shim.c(—)   → crates/record/src/shim.c
src/record/audio.c(—)  → crates/record/src/audio.c
build.rs 的 record/pipewire 段 → crates/record/build.rs
```

### 2.22–2.27 单文件 crate
```
ocr.rs(391)        → crates/ocr/src/lib.rs
translate.rs(301)  → crates/translate/src/lib.rs
stitch/mod.rs(994) → crates/stitch/src/lib.rs ；stitch/tests.rs(197) → crates/stitch/src/tests.rs
longshot.rs(1156)  → crates/longshot/src/lib.rs
notify.rs(253)     → crates/notify/src/lib.rs
clipboard.rs(607)  → crates/clipboard/src/lib.rs
```

### 2.28 `crabsnap-actions`
```
input/actions.rs(225)  → crates/actions/src/registry.rs
input/gestures.rs(154) → crates/actions/src/gestures.rs
input/keymap.rs(197)   → crates/actions/src/dispatch.rs
input/bridge.rs(437)   → crates/actions/src/bridge.rs（键表部分已下沉到 keymap）
```

### 2.29 `crabsnap-render`
```
render/mod.rs(949)          → crates/render/src/lib.rs
render/canvas/{mod,tests}.rs(2043) → crates/render/src/canvas/{mod,tests}.rs
render/cpu/{mod,compositor,resample}.rs(2621) → crates/render/src/cpu/同名
render/effect/{mod,corners,solver}.rs(729) → crates/render/src/effect/同名
render/{effect_cache,effect_diag,effect_pipeline,ink,kernel,linear,pixels,probe,raster,surface,target}.rs → 同名
layer/stack/mod.rs::device_damage/logical_rect → crates/render/src/damage.rs
ui/engine/damage.rs(327)::Dirty → crates/render/src/damage.rs（`ui-base` 保留再导出）
ui/chrome/magnifier.rs::{window_span,loupe_center} → crates/render/src/loupe.rs
```

### 2.30 `crabsnap-render-gpu`
```
render/gpu/mod.rs(2478) → crates/render-gpu/src/lib.rs
render/gpu/compositor/{mod,tests}.rs(2852) → crates/render-gpu/src/compositor/{mod,tests}.rs
render/gpu/{effect_cache,effects,geom,geom_gl,row_order,shaders,state,surface,texture}.rs → 同名
gl/context.rs::renders_a_known_pixel → crates/render-gpu/src/selftest.rs（新增）
```

### 2.31 `crabsnap-annotate`
```
annotate/mod.rs(589)          → crates/annotate/src/lib.rs
annotate/{menu,pattern,resize,tool}.rs → 同名
annotate/document/{mod,tests}.rs → crates/annotate/src/document/同名
annotate/manager/{mod,nested,tests}.rs → crates/annotate/src/manager/同名
annotate/tools/{mod,arrow,bezier,ellipse,eraser,highlighter,line,mosaic,number,pen,rect,wave}.rs → 同名
annotate/tools/text/{mod,tests}.rs → crates/annotate/src/tools/text/同名
settings.rs 的工具状态持久化 → crates/annotate/src/persist.rs
ui/chrome/mod.rs::node_grips + NODE_GRIP_LINE_WIDTH → crates/annotate/src/chrome.rs
```

### 2.32 `crabsnap-card`
```
card/mod.rs(171) → crates/card/src/lib.rs
card/{color,color_card,markdown,text_card}.rs → 同名
```

### 2.33 `crabsnap-ui-base`
```
ui/base/{mod,hit,layout,view}.rs → crates/ui-base/src/base/同名
ui/widgets/{mod,annotation,components,container,handles,slider,text_edit,text_view,valuebox}.rs → crates/ui-base/src/widgets/同名
ui/engine/{mod,anim}.rs → crates/ui-base/src/engine/同名
ui/engine/damage.rs → crates/render/src/damage.rs（本 crate 只 `pub use`）
ui/engine/widget/{mod,tests}.rs → crates/ui-base/src/engine/widget/同名
ui/chrome/icons.rs(962) → crates/ui-base/src/icons.rs
ui/panel/hit.rs 的 CursorShapeLite/HitTarget/Modal/Dispatching/dispatch_modal → crates/ui-base/src/base/hit_target.rs
ui/chrome/mod.rs 的 MARK_BOX_WIDTH/MARK_BOX_DASH/MARK_BOX_LINE/grip → crates/ui-base/src/markers.rs
ui/control/settings/mod.rs::SliderRange → crates/ui-base/src/widgets/slider.rs
ui/panel/toolbar/render.rs::PANEL_BG → crates/ui-base/src/theme.rs
ui/panel/toolbar/anchor.rs::snap_zone_rects → crates/ui-base/src/base/anchor.rs
ui/mod.rs::FROST_SIGMA / toolbar_blur_kind / blur_kind_for → crates/ui-base/src/blur.rs
```

### 2.34 `crabsnap-ui-chrome`
```
ui/chrome/mod.rs(1147 − 已下沉部分) → crates/ui-chrome/src/lib.rs
ui/chrome/magnifier.rs(165 − 已下沉部分) → crates/ui-chrome/src/magnifier.rs
ui/chrome/tooltip.rs(138) → crates/ui-chrome/src/tooltip.rs
```

### 2.35 `crabsnap-ui-control`
```
ui/control/{mod,browser_fs,browser_layer,context_menu,mosaic,param,places,save_dialog,text_size,width}.rs → 同名
ui/control/palette/{mod,formats,layout}.rs → crates/ui-control/src/palette/同名
ui/control/settings/{mod,hit,layout,render}.rs → crates/ui-control/src/settings/同名
```

### 2.36 `crabsnap-ui-panel`
```
ui/panel/{mod,annotate,hit}.rs → crates/ui-panel/src/同名（`hit.rs` 保留面板级 `HitProbe`）
ui/panel/toolbar/{mod,anchor,layout,render,scrollbar}.rs → 同名
ui/panel/overlay/{mod,anim,compose,damage,dispatch,hit,input,longshot,menu,palette,record,save,scene,settings_row,toolbar}.rs → 同名
ui/panel/overlay/tests/**（19 个文件）→ crates/ui-panel/src/overlay/tests/同名
```

### 2.37 `crabsnap-ui`
```
ui/mod.rs(105) → crates/ui/src/lib.rs（只剩 `pub use` 与 `toolbar_blur_kind`）
```

### 2.38 `crabsnap-settings-ui`
```
crates/settings/src/lib.rs(44)         → crates/settings-ui/src/lib.rs
crates/settings/src/{app,color_picker,sidebar,theme,wake,widgets}.rs → 同名
crates/settings/examples/visual_spike.rs(852) → crates/settings-ui/examples/visual_spike.rs
crates/settings/src/{keymap,color,store}.rs → 已拆到 keymap / color / config
```

### 2.39 `crabsnap-compositor`
```
layer/mod.rs(25)        → crates/compositor/src/lib.rs
layer/{consts,host,probe,process,surface_stack}.rs → 同名
layer/stack/{mod,browser_kind_tests}.rs → crates/compositor/src/stack/同名
layer/compositor/{mod,tests}.rs → crates/compositor/src/compositor/同名（tests 见 §5）
layer/backends/{mod,cpu,gpu}.rs → crates/compositor/src/backends/同名
layer/present/{mod,chain,release,target}.rs → crates/compositor/src/present/同名
layer/present/{dmabuf,shm}/{mod,backend,slots}.rs → 同名
layers/debug_overlay.rs(1612) → crates/compositor/src/debug/mod.rs
layers/debug_overlay/{surface,thread}.rs → crates/compositor/src/debug/同名
```

### 2.40 `crabsnap-layers`
```
layers/mod.rs(18) → crates/layers/src/lib.rs
layers/freeze.rs(902) → crates/layers/src/freeze.rs
layers/sticker/{mod,consts,badge,density,export,hit,look,menu,stack}.rs → crates/layers/src/sticker/同名
layers/sticker/layer/{mod,tests}.rs → crates/layers/src/sticker/layer/同名
```

### 2.41 `crabsnap-app`
```
app/mod.rs(11)     → crates/app/src/lib.rs
app/cli/mod.rs(1026) → crates/app/src/cli/mod.rs
app/cli/commands/{mod,annotate,completions,config,daemon,elements,export,formats,info,ocr,pin,record,replay,shot,translate}.rs → 同名
app/cli/commands/settings/{mod,annotate}.rs → **crates/settings-host/src/**（见 2.42）
app/output.rs(292) → crates/app/src/output.rs
app/state/{mod,annotate,browser,deliver,frames,longshot,output,paste,pick,pins,place,record,session}.rs → crates/app/src/state/同名
input/mod.rs(174) → crates/app/src/input_router.rs（`ToolOutcome`/`CaptureOverlay` 的装配点）
platform/linux/daemon/server.rs(958) → crates/app/src/daemon.rs
```

### 2.42 `crabsnap-settings-host`
```
app/cli/commands/settings/mod.rs(1721)    → crates/settings-host/src/lib.rs
app/cli/commands/settings/annotate.rs(446) → crates/settings-host/src/annotate.rs
（新增）crates/settings-host/src/main.rs  → `fn main()` 只调 `crabsnap_settings_host::run_from_argv()`
```

### 2.43 `crabsnap`
```
lib.rs(59)  → crates/crabsnap/src/lib.rs（facade：`pub use crabsnap_core as core;` …）
main.rs(52) → crates/crabsnap/src/main.rs（内容不变，路径改 crate 名）
examples/*(34 个，5.5k 行) → crates/crabsnap/examples/同名
```

---

## 3. 路径改写规则

### 3.1 绝对路径（`crate::…`）

对每个文件，把 `crate::<旧模块路径>` 重写成 `<新 crate 标识>::<新内部路径>`。
规则由「旧模块前缀 → 新 crate + 新内部前缀」表驱动，**最长前缀优先**：

| 旧前缀 | 新写法 | 例 |
|---|---|---|
| `crate::core::error` | `crabsnap_core::error` | `crate::core::error::Result` → `crabsnap_core::error::Result` |
| `crate::core::geometry` | `crabsnap_geometry::` | `crate::core::geometry::PixelRect` → `crabsnap_geometry::PixelRect` |
| `crate::core::hdr` | `crabsnap_color::hdr` | `crate::core::hdr::Cicp` → `crabsnap_color::hdr::Cicp` |
| `crate::core::color` | `crabsnap_color::color` | `crate::core::color::prepare_for_output` → `crabsnap_color::color::prepare_for_output` |
| `crate::core::i18n` | `crabsnap_core::i18n` | `crate::core::i18n::t` → `crabsnap_core::i18n::t` |
| `crate::core::logging` | `crabsnap_core::logging` | |
| `crate::core::testing` | `crabsnap_core::testing` | |
| `crate::select` | `crabsnap_core::select` | |
| `crate::parallel` / `crate::paths` | `crabsnap_core::{parallel,paths}` | |
| `crate::model` | `crabsnap_image` | `crate::model::Frame` → `crabsnap_image::Frame` |
| `crate::ffmpeg` | `crabsnap_ffmpeg` | |
| `crate::text` | `crabsnap_text` | |
| `crate::scene` | `crabsnap_scene` | |
| `crate::selection` | `crabsnap_selection` | |
| `crate::card` | `crabsnap_card` | |
| `crate::gl` | `crabsnap_gl` | |
| `crate::platform::linux::wayland` | `crabsnap_wayland` | |
| `crate::platform::linux::capture` | `crabsnap_capture` | |
| `crate::platform::linux::inject` | `crabsnap_inject` | |
| `crate::platform::linux::hotkey` | `crabsnap_hotkey` | |
| `crate::platform::linux::element` | `crabsnap_element` | |
| `crate::platform::linux::daemon::server` | `crabsnap_app::daemon` | 文件已搬进 app |
| `crate::platform::linux::daemon` | `crabsnap_daemon` | |
| `crate::platform::portal` / `crate::platform::probe` | `crabsnap_portal` | |
| `crate::live` | `crabsnap_live` | |
| `crate::record` | `crabsnap_record` | |
| `crate::ocr` / `crate::translate` / `crate::stitch` / `crate::longshot` / `crate::notify` / `crate::clipboard` | 同名 `crabsnap_*` | |
| `crate::settings` | `crabsnap_config`（工具状态部分 → `crabsnap_annotate::persist`） | |
| `crate::input::actions` / `::gestures` / `::keymap` / `::bridge` | `crabsnap_actions::{registry,gestures,dispatch,bridge}` | |
| `crate::input::{BTN_*,GestureMap,Dispatcher}` | `crabsnap_input::*` / `crabsnap_actions::*` | `BTN_*` 进 input，`GestureMap` 进 actions |
| `crate::input`（其余：InputRouter） | `crabsnap_app::input_router` | |
| `crate::render::gpu` | `crabsnap_render_gpu` | |
| `crate::render` | `crabsnap_render` | |
| `crate::annotate` | `crabsnap_annotate` | |
| `crate::ui::base` / `::widgets` / `::engine` / `::icons` / `::hit` / `::damage`(再导出) | `crabsnap_ui_base::{base,widgets,engine,icons,hit}` / `crabsnap_render::damage` | |
| `crate::ui::chrome` / `::magnifier` / `::tooltip` | `crabsnap_ui_chrome::{…}` | |
| `crate::ui::control` / `::palette` / `::save_dialog` / `::context_menu` / `::settings` / `::width` … | `crabsnap_ui_control::{…}` | |
| `crate::ui::panel` / `::overlay` / `::toolbar` / `::hit`(HitProbe) / `::annotate` | `crabsnap_ui_panel::{…}` | |
| `crate::ui::FROST_SIGMA` / `::toolbar_blur_kind` | `crabsnap_ui_base::{FROST_SIGMA,toolbar_blur_kind}` | |
| `crate::ui::CaptureOverlay` 等 | `crabsnap_ui_panel::overlay::CaptureOverlay` | |
| `crate::layer` | `crabsnap_compositor` | |
| `crate::layers::debug_overlay` | `crabsnap_compositor::debug` | |
| `crate::layers` | `crabsnap_layers` | |
| `crate::app` | `crabsnap_app` | |

**同一个 crate 内部**的引用统一改写成 `crate::…`（例：`crabsnap_render` 内的
`crate::core::hdr::X` → `crate::damage::…` 或 `crabsnap_color::hdr::X`）。
判断依据：目标文件是否落在同一个新 crate 里。

### 3.2 相对路径（`super::` / `self::`）

**内部布局不变 ⇒ 绝大多数不用改。** 只有三类需要处理：

1. **切分点上的 `super::`**：目标模块被搬到了别的 crate。
   例：`annotate/tools/bezier.rs` 里的 `super::super::…` 如果指向 `ui::chrome`，
   改为 `crabsnap_ui_chrome::…`（本次已通过把 `node_grips` 搬进 annotate 消除）。
2. **`pub(super)` / `pub(crate)` 跨切分点**：改成 `pub`（+ `#[doc(hidden)]`）或收紧到
   `pub(crate)`。清单见 [`migration.md` §4](migration.md)。
3. **`mod.rs` 变 `lib.rs`**：`crate::<模块>` 在 crate 内变成 `crate::<子模块>`；
   顶层 `pub use` 保持不变（旧 `mod.rs` 的再导出原样搬进 `lib.rs`）。

### 3.3 文档内链

`/// … [`crate::x::y`] …` 里跨 crate 的链接要改成 `[`crabsnap_x::y`]`，
否则 `cargo doc` 报 unresolved link（只是警告，但会掩盖真问题）。
459 处中约 300 处跨 crate。批量改写规则同上表，只处理 `[`…`]` 内的路径。

### 3.4 其它需要人工确认的引用

| 形态 | 处理 |
|---|---|
| `env!("CARGO_PKG_VERSION")` | `app`/`daemon` 里的版本号改由 `workspace.package.version` 统一（全部 0.1.0），行为不变 |
| `include_str!("locales/*.ftl")` | 随 `i18n` 目录搬进 `crates/core/src/i18n/`，相对路径不变 |
| `include_bytes!` / `#[path]` / `include!` | 扫描结果为 0，无需处理 |
| `#[cfg(crabsnap_egl)]` 等 | 见 `architecture.md` ADR-007；拆完用 `grep -rn 'cfg(crabsnap_' crates/` 对账 |
| `#[hotpath::measure]` | 所在 crate 必须保留非 optional 的 `hotpath` 依赖 |
| `env_logger`/`log` 初始化 | 只在 `crabsnap` 二进制里调用一次（不变） |

---

## 4. 依赖矩阵（新 crate 图，43×43 的非零项）

行 = 依赖方，列 = 被依赖方。`*` 表示同层依赖（`arch.toml` 必须显式列出）。

```
core        ← （无）
geometry    ← （无）
keymap      ← （无）
text        ← （无）
color       → geometry
input       → keymap
selection   → geometry
ffmpeg      → core
image       → core color geometry ffmpeg
gl          → core
portal      → core
daemon      → core
wayland     → core color geometry input keymap
capture     → wayland image color geometry core portal
inject      → wayland portal core
hotkey      → wayland portal core keymap
element     → wayland capture(*) geometry color image
config      → core keymap color geometry
scene       → color geometry image text
live        → capture wayland core geometry
record      → capture live wayland image color core geometry ffmpeg
ocr         → image geometry core
translate   → core
stitch      → image core
longshot    → stitch image core
notify      → config core
clipboard   → wayland image core
actions     → input keymap config core
render      → core geometry color image scene text gl render-gpu(*)
render-gpu  → render gl color geometry image scene core
annotate    → color geometry image scene text selection render
card        → core image scene text render
ui-base     → geometry image scene text selection render annotate(*) color core
ui-chrome   → ui-base(*) image scene text selection render annotate color core
ui-control  → ui-base(*) annotate image scene text color geometry core render
ui-panel    → ui-base(*) ui-chrome(*) ui-control(*) annotate image scene text selection
              color geometry core render actions wayland input
ui          → ui-base ui-chrome ui-control ui-panel
settings-ui → config keymap color
compositor  → render render-gpu(*) gl wayland portal scene image color core geometry text
layers      → compositor(*) ui-base ui-chrome ui-control ui-panel annotate image scene text
              color geometry core render wayland config input
app         → 其余全部
settings-host → settings-ui config keymap annotate ui-control daemon actions core image color
crabsnap    → app settings-host 与全部（仅再导出）
```

**边数**：231（含 `crabsnap` facade 的 9 条再导出边）。`cargo xtask arch --matrix` 输出机器可读版本。

---

## 5. 测试的归属调整

| 旧位置 | 新位置 | 理由 |
|---|---|---|
| `layer/compositor/tests.rs`（624 行，用到 `ui::context_menu`） | `crates/crabsnap/tests/compositor_ui.rs` | 测的是「合成器 × UI 场景」两层契约，合成器 crate 不能依赖 UI |
| `render/canvas/tests.rs` 里用到 `layers::sticker::PinEntry` 的那部分 | `crates/crabsnap/tests/canvas_pin.rs` | 同上 |
| `render/gpu/compositor/tests.rs` 里的 `device_damage` 用例 | 留在 `render-gpu`，改为调 `crabsnap_render::damage::device_damage` | 函数已下沉，测试不用动 |
| 其余 253 个 `#[cfg(test)]` 模块 | 原地 | 内部布局不变，`super::` 仍然有效 |

**上移测试的写法**：在 `crates/crabsnap/tests/` 里用 `use crabsnap::{compositor, ui_panel, …};`
（facade 再导出），并在文件头写一行说明「为什么这条测试必须跨层」。

---

## 6. `ui/` workspace 里没有旧对应物的 crate

> §1–§5 是旧 `crabsnap` 43 crate 的迁移映射；`ui/` 是独立 workspace，其中的新 crate
> 只要不是「照搬旧代码」，就不在上面任何一张表里。本节登记这些 crate，免得读者拿
> §1 的表当全集。设计依据见 [`research/i18n-fluent.md`](research/i18n-fluent.md)。

| crate | 层 | 旧对应 | 文件 | LOC | 外部依赖 |
|---|---|---|---|---|---|
| `ui-i18n` | L0（`PORTABLE`） | 旧 `src/core/i18n/`（339 行，`include_str!` + 手写回退）只算同问题域的参考，**不是迁移源**：新 crate 按 Fluent 重新设计 | 5 + `build.rs` | 762 | fluent-bundle, unic-langid, log；build-dep fluent-syntax |

**`ui-i18n` 的接线**（改它之前先确认这几处还在）：

| 环节 | 位置 |
|---|---|
| key 表与 locale 原文由构建期生成：`Msg` / `ALL` / `LOCALES` | `ui-i18n/build.rs` ← `locales/{en,zh-CN}/*.ftl` |
| 回退链 `请求 locale → 同语言 → en`，装载即校验 | `ui-i18n/src/catalog.rs`（`Catalog::builtin`） |
| 消费入口 `Cx::t` / `Cx::t_args`，目录以 `Arc<Catalog>` 挂在 `Env` 链上 | `ui-widgets/src/cx.rs`、`Env::with(parent, catalog)` |
| 直接依赖它的 crate | `ui-widgets`、`ui-widgets-md3`（控件文案）、`ui-gallery`（页面文案） |
| 不译的东西 | `Id` 的字符串、tag、图标名、令牌名、十六进制、`format!` 的调试读数 |
| 可移植性边界 | `ui-platform/tests/platform_boundary.rs` 的 `PORTABLE`（crate 内不读 `LANG`/`sys-locale`，locale 由宿主传入） |
| 防漏翻 | `ui-i18n/tests/catalog.rs::every_locale_defines_every_generated_message` 遍历 `ALL × LOCALES`，新增 key 自动进检查 |
