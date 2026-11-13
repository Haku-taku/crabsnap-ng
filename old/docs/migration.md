# 迁移计划（Phase 0–8）

> 配套：[`architecture.md`](architecture.md)（目标架构）、[`crate-map.md`](crate-map.md)（逐文件映射）
> 铁律：**每个 Phase 结束时 `cargo check --workspace --all-targets` + `cargo test --workspace`
> 必须绿。** 不接受「先红着，下个 Phase 再修」——一旦有两个 Phase 同时红，测试变红就无法归因。

---

## 1. 三条原则

1. **语义改动全部集中在 Phase 1。** Phase 2–7 只做「搬文件 + 改路径」的机械改写。
   这样 review 时只需认真读 Phase 1，其余按 diff 统计核对（文件数、行数、`crate::` 命中数）。
2. **内部布局不变。** crate 内部保持旧的目录结构（`mod.rs` → `lib.rs`，其余不变），
   于是 `super::`/`self::`/`pub(super)` 绝大多数不需要动。
3. **一次一个 Phase 一个提交。** 提交信息模板：
   `refactor(crate): Phase N —— <范围>（<搬移文件数>/<改写引用数>）`。

---

## 2. Phase 1：环破除清单（仍是单 crate，行为不变）

这一阶段**不新建任何 crate**，只在旧项目里做 15 处方向修正，目标是让「按顶层模块聚合的
依赖图」变成 DAG。为便于执行，下面把「同一动作、同一文件」的条目合并成 13 组
（2.1–2.13），2.14 是确认合法的边，2.15 是收尾复查。每处都给了证据行号与验证方式。

### 2.1 `core → platform`：`OutputColour` 下沉

- **证据**：`src/core/color/output.rs:17` `use crate::platform::linux::wayland::color_proto::OutputColour;`
  使用点：`output_reference_nits()`（同文件 ~74 行）、测试 ~190 行。
- **动作**：把 `OutputColour`（`color_proto.rs:18-130`，纯数据 + `to_description()`/`tf_name()`/
  `primaries_name()`）搬到 `core::color::output_colour`；`color_proto` 只负责从协议事件填充它，
  并 `pub use crate::core::color::output_colour::OutputColour;` 保持旧路径可用。
- **验证**：`grep -rn 'crate::platform' src/core/` 为 0；`output_reference_nits` 的
  三条断言（缺失/非正/正常）仍绿。

### 2.2 `gl → render/scene/image`：渲染自检上移

- **证据**：`src/gl/context.rs:328-378` 的 `renders_a_known_pixel()`，
  用到 `crate::render::RenderTarget`(334)、`crate::render::gpu::GpuRenderer`(335)、
  `crate::render::Renderer`(345)、`crate::scene::{Scene,builder,Shape}`(339-342)、
  `crate::model::Color`(343)。
- **动作**：
  1. 函数搬到 `render::gpu::selftest::renders_a_known_pixel(ctx)`；
  2. `gl::context` 暴露 `probe_with(check: impl FnOnce(&Rc<GpuContext>) -> bool)`，
     `GpuContext::probe()` 变成 `probe_with(|_| true)`；
  3. `gl::probe()` 的 `OnceLock` 缓存保留，但「能不能画」这一项由调用方注入；
  4. `render::probe()` 改为 `gl::probe_with(render::gpu::selftest::renders_a_known_pixel)`。
- **验证**：`grep -rn 'crate::render\|crate::scene\|crate::model' src/gl/` 为 0；
  `cargo test gl::` 与 `cargo test render::probe` 绿。

### 2.3 `render → layer`：`device_damage` 下沉

- **证据**：`src/render/gpu/compositor/tests.rs:1073,1549,1551` 调
  `crate::layer::stack::device_damage`；实现位于 `src/layer/stack/mod.rs:674`。
- **动作**：`device_damage` 与它的反向函数 `logical_rect`（同文件 691 行）搬到
  `render::damage`；`layer::stack` 改为 `pub use crate::render::damage::{device_damage, logical_rect};`
  保持调用点不变。
- **验证**：`grep -rn 'crate::layer' src/render/` 只剩注释；`cargo test render::gpu::compositor` 绿。

### 2.4 `render → ui`：`window_span` 下沉

- **证据**：`src/render/mod.rs:302` `crate::ui::magnifier::window_span(factor)`；
  实现位于 `src/ui/chrome/magnifier.rs`。
- **动作**：`window_span`/`loupe_center` 搬到 `render::loupe`；`ui::chrome::magnifier`
  改为 `pub use crate::render::loupe::{window_span, loupe_center};`。
- **验证**：`grep -rn 'crate::ui' src/render/` 为 0（注释里的文档链接同步改）。

### 2.5 `render → layers`：测试上移

- **证据**：`src/render/canvas/tests.rs:66` `use crate::layers::sticker::{PinEntry, entry_placement};`
- **动作**：把用到 `PinEntry` 的测试函数（`canvas/tests.rs` 内 1–2 个）搬到
  `tests/canvas_pin.rs`（Phase 1 时先放 `tests/` 顶层集成测试目录，因为旧项目没有；
  可先放进 `src/layers/sticker/layer/tests.rs` 旁边，Phase 6 再随 `layers` 走）。
- **验证**：`grep -rn 'crate::layers' src/render/` 为 0；被搬走的测试仍被执行
  （`cargo test -- --list | grep canvas_pin` 非空）。

### 2.6 `annotate → ui`：`node_grips` 上移

- **证据**：`src/annotate/tools/bezier.rs:881` `crate::ui::chrome::node_grips(...)`；
  实现位于 `src/ui/chrome/mod.rs:310`（依赖 `NODE_GRIP_LINE_WIDTH`）。
- **动作**：`node_grips` 与 `NODE_GRIP_LINE_WIDTH` 搬到 `annotate::chrome`；
  `ui::chrome` 删除这两个符号（`ui::chrome` 没有其它地方用它们——`grep -c` 确认）。
- **验证**：`grep -rn 'crate::ui' src/annotate/` 为 0；贝塞尔编辑的 3 个视觉诊断示例
  （`_diag_*`）仍能编译。

### 2.7 `layer → layers::debug_overlay`：调试浮层归入机制层

- **证据**：`src/layer/surface_stack.rs` 10 处（129,134,170,191,216,436,461,469,1421,1510），
  `src/layer/present/dmabuf/slots.rs:146`（`encode_srgb8_premultiplied`），
  `src/layer/probe.rs:28`（文档链接）。
- **动作**：
  1. `layers/debug_overlay*`（3 个文件，1998 行）整体搬到 `layer::debug`；
     `layers/mod.rs` 删掉 `pub mod debug_overlay;`；
  2. `present/dmabuf/slots.rs:146` 改为直接调 `render::canvas::encode_srgb8_premultiplied`
     （`layers::sticker::canvas` 本来就是 `render::canvas` 的再导出）；
  3. `layer/probe.rs:28` 的文档链接改成纯文本（跨 crate 后 `DMABUF_IMPLEMENTED` 不可链接）。
- **验证**：`grep -rn 'crate::layers' src/layer/` 为 0；`cargo test layer::` 绿；
  真机跑一次 `CRABSNAP_DEBUG_OVERLAY=1`（或对应环境变量）确认浮层仍出。

### 2.8 `layer → ui`：伤害类型下沉 + 测试上移

- **证据**：
  - `src/layer/stack/mod.rs:218,249,809,818-821,1025,1036,1040`（`ui::damage::Dirty`）
  - `src/layer/surface_stack.rs:37`（`ui::damage::Dirty`）
  - `src/layer/compositor/tests.rs:12,16,111,115,203,207`（`ui::context_menu::*`）
- **动作**：
  1. `ui/engine/damage.rs`（327 行）的 `Dirty` 搬到 `render::damage`；
     `ui::damage` 变成 `pub use crate::render::damage::Dirty;`（`ui` 侧只留辅助函数）；
  2. `layer/compositor/tests.rs` 整个文件上移到 `tests/compositor_ui.rs`（它测的是
     「合成器 × 真实 UI 场景」的契约，两边都不该依赖对方）。
- **验证**：`grep -rn 'crate::ui' src/layer/` 为 0；`cargo test` 全绿；
  `cargo test -- --list | grep -c compositor` 数量与搬移前一致。

### 2.9 `platform → app`：daemon 服务端上移

- **证据**：`src/platform/linux/daemon/server.rs` 全文 958 行，19 处 `crate::app::*`
  （19,380-390,662,760-802）、`crate::clipboard`(469)、`crate::longshot`(733-748)、
  `crate::record::session`(835)、`crate::settings`(74,784)。
- **动作**：`server.rs` 搬到 `src/app/daemon.rs`（模块名 `app::daemon`）；
  `platform/linux/daemon/mod.rs` 只留 `protocol`/`client`/`discovery`；
  `app/cli/commands/daemon.rs` 改为 `crate::app::daemon::run`。
- **理由**：daemon 服务端是**装配根**（它要拿 `AppState`、要发通知、要写剪贴板），
  放在协议层会让 `platform` 永远依赖 `app`。
- **验证**：`grep -rn 'crate::app' src/platform/` 为 0；`crabsnap daemon` 启动、
  一次 `shot` 走 daemon、`quit` 正常（真机）。

### 2.10 `platform → input/actions`：输入词汇表与键表下沉

- **证据**：
  - `src/platform/linux/wayland/pointer.rs:365-368` `use crate::input::bridge::key_from_evdev;`
  - `src/platform/linux/wayland/state.rs:962` `use crate::input::BTN_LEFT;`
  - `src/platform/linux/hotkey/mod.rs:30-31` `crate::input::actions::{ids,ACTIONS}`
- **动作**：
  1. 新建 `src/input/event.rs`：`PointerEvent`/`KeyEvent`/`KeyMods`/`ModsSource` 从
     `wayland/pointer.rs` 搬入；`CursorShape` 从 `wayland/cursor.rs` 搬入；
     `BTN_*` 从 `input/mod.rs` 搬入。`wayland` 侧 `pub use` 保持旧路径；
  2. `key_from_evdev`/`key_from_keysym`/`combo_of` 与 `KEY_*` 常量搬到
     `input::keysym`（Phase 2 之后它们进 `keymap` crate）；
  3. `hotkey` 的注册入口改为接收
     `&[ShortcutSpec { id: String, label_key: &'static str, trigger: String }]`，
     由 `app` 从动作表构造后传入。
- **验证**：`grep -rn 'crate::input' src/platform/` 为 0（Phase 1 结束时 `input` 是
  同 crate 的模块，改完后 `platform` 只依赖 `input::event`/`input::keysym`，
  这两者在 Phase 2 会被切成独立 crate，方向仍成立）。

### 2.11 `settings → annotate` / `ui`：配置三分

- **证据**：`src/settings.rs` 中
  `tool_settings_from`(101)、`encode_annotate_settings`(126)、`tool_snapshot_from`(160)、
  `annotate_state`(209)、`annotate_state_with_overrides`(218)、`annotate_state_from`(230)、
  `save_annotate_state`(248)、`replace_annotate_tables`(266)，共 30 处 `crate::annotate::*`；
  以及 `176` 行 `crate::ui::width::quantize`。
- **动作**：
  1. 工具状态持久化整体搬到 `annotate::persist`（新文件，函数名不变）；
     `config` 只保留 `Settings` 与各 `*Settings` 表的 schema/IO；
  2. `quantize` 搬到 `geometry`（纯函数，`ui/control/width.rs` 改为再导出）。
- **验证**：`grep -rn 'crate::annotate\|crate::ui' src/settings.rs` 为 0；
  `config.toml` 往返测试（`annotate_state_tests`、`tests`）仍绿。

### 2.12 `ui` 内部三处上行：词汇下沉到 `ui-base`

- **证据**：
  - `ui/widgets/components.rs:133`（`crate::ui::icons::draw`）
  - `ui/widgets/annotation.rs:139-146`（`crate::ui::chrome::MARK_BOX_*`）
  - `ui/widgets/handles.rs:111`（`crate::ui::chrome::grip`）
  - `ui/widgets/valuebox.rs:478`（`crate::ui::settings::SliderRange`）
  - `ui/engine/widget/mod.rs:550,1873,1943,1954`（`crate::ui::hit::CursorShapeLite`）
  - `ui/control/context_menu.rs:423,455`（同上）
  - `ui/control/palette/mod.rs:297`（`crate::ui::toolbar::render::PANEL_BG`）
  - `ui/control/context_menu.rs` 与 `ui/panel/overlay/mod.rs:1126-1127`（`SHADOW_PAD`）
  - `ui/chrome/mod.rs:268`（`pub use crate::ui::toolbar::anchor::snap_zone_rects`）
- **动作**：`icons`、`MARK_BOX_*`/`grip`、`SliderRange`、`CursorShapeLite` 与命中词汇、
  `PANEL_BG`、`snap_zone_rects`、`FROST_SIGMA`/`toolbar_blur_kind` 全部搬到
  `ui/base`（Phase 5 之后进 `ui-base` crate）；原位置改 `pub use` 保持调用点不变。
- **验证**：`cargo test ui::` 全绿；Phase 5 切 crate 时 `ui-base` 不再依赖
  `ui-chrome`/`ui-control`/`ui-panel`。

### 2.13 `ui → app` / `layers → app` / `wayland → app`：假象边

- **证据**：`crate::input::{BTN_LEFT,BTN_RIGHT,BTN_MIDDLE}`（`input/mod.rs:32-34`）与
  `crate::input::GestureMap`（`input/mod.rs:24` 再导出）——`input/mod.rs` 被划到 `app`，
  于是这些引用被误算成「依赖 app」。真实调用点：`layers/sticker/stack.rs:523,546,666`、
  `ui/panel/overlay/mod.rs:636,809,944`、`wayland/state.rs:962`。
- **动作**：随 2.10 完成（`BTN_*` 进 `input::event`，`GestureMap` 进 `input::gestures`）。
- **验证**：`grep -rn 'crate::app' src/layers/ src/ui/ src/platform/` 为 0。

### 2.14 `element → capture`：确认合法

- **证据**：`platform/linux/element/accessibility.rs:27`、`element/mod.rs:15`。
- **动作**：**不动**。`element`（元素点选）依赖 `capture`（取像素）是向下的，
  不是环；在 `arch.toml` 里作为同层允许边显式登记。

### 2.15 收尾：SCC 复查

Phase 1 结束时跑一次复查脚本（与设计期同款）：

```bash
# 按计划 crate 归属聚合模块，逐 crate 内部求 SCC；要求每个 SCC 都落在同一个 crate 内
python3 scripts/check_crate_scc.py --map docs/crate-map.md
```

同时检查「跨 crate 的 `crate::` 引用」是否只剩下**允许方向**：

```bash
# 期望输出为空
python3 scripts/check_crate_scc.py --violations
```

---

## 3. Phase 0–8 步骤

### Phase 0：workspace 骨架（1 个提交）

```bash
mkdir -p crabsnap-ng && cd crabsnap-ng
# 1) 根 Cargo.toml（architecture.md §7.1 全文）、arch.toml、rustfmt.toml
# 2) 43 个 crate 目录，每个只有：Cargo.toml（§7.2 模板）+ src/lib.rs（一行 //! 模块文档）
# 3) xtask：arch / disk / notes 三个子命令
cargo check --workspace --all-targets
cargo xtask arch          # 此时应当只有「空 crate」通过
```

**验收**：空壳全绿；`cargo xtask arch` 能列出 43 个 crate 与各自层号。

### Phase 1：环破除（见 §2，可拆成 3–4 个提交）

**验收**：
```bash
cargo test                       # 现有测试全绿（此阶段不动测试语义）
./scripts/run-tests.sh
python3 scripts/check_crate_scc.py --violations    # 空
```

### Phase 2：切 L0 + L1（12 个 crate）

顺序（每步一个提交，从叶子往上）：

1. `core`（含 `i18n` 与 `locales`）→ 2. `geometry` → 3. `color`（含 `OutputColour`、
   `format`）→ 4. `keymap`（`egui::Key` → 自有 `Key`）→ 5. `text` → 6. `input`
   → 7. `selection` → 8. `ffmpeg`（build.rs + `still.c`）→ 9. `image`
   → 10. `gl`（去 build.rs、`gbm` 改 dlopen）→ 11. `portal` → 12. `daemon`

**每步**：搬文件 → 改 `crate::` 路径 → `cargo check -p <新 crate>` →
`cargo check --workspace` → `cargo test`。

**验收**：`cargo tree -p crabsnap-image` 里不再出现 `wayland-*`；
`cargo check -p crabsnap-gl` 在**没有 EGL/GBM 头文件**的容器里也能过
（用 `--no-default-features` 的 CPU 路径验证，见 Phase 4）。

### Phase 3：切 L2 + L3（16 个 crate）

`wayland` → `capture` → `inject` → `hotkey`（`ShortcutSpec` 注入）→ `element` →
`config`（含 `store.rs`）→ `scene` → `live` → `record`（build.rs + 两个 `.c`）→
`ocr` → `translate` → `stitch` → `longshot` → `notify` → `clipboard` → `actions`

**验收**：`cargo xtask arch` 绿；`crabsnap`（此时还是旧 lib.rs）能 `cargo check`；
真机 `shot`/`pin`/`record` 各跑一次。

### Phase 4：切 L4（`render` + `render-gpu` + `gpu` feature）

1. 搬 `render`（含新 `damage.rs`/`loupe.rs`）；
2. 搬 `render-gpu`，`render` 的 `gpu` feature 接上；
3. `compositor`/`layers`/`app` 的 GL 相关 `#[cfg(all(crabsnap_egl, crabsnap_gbm))]`
   改成 `#[cfg(feature = "gpu")]`（Phase 6/7 时随 crate 落地，但 cfg 替换在 Phase 4 一起做）。

**验收**：
```bash
cargo check --workspace --all-targets
cargo check --workspace --all-targets --no-default-features   # 纯 CPU 构建必须绿
cargo test -p crabsnap-render -p crabsnap-render-gpu
grep -rn 'cfg(crabsnap_egl\|cfg(crabsnap_gbm)' crates/        # 期望 0
```

### Phase 5：切 L5（8 个 crate）

`annotate` → `card` → `ui-base` → `ui-chrome` → `ui-control` → `ui-panel` → `ui`（facade）
→ `settings-ui`（含 `visual_spike` 示例）

**验收**：
```bash
cargo test -p crabsnap-ui-base -p crabsnap-ui-chrome -p crabsnap-ui-control -p crabsnap-ui-panel
cargo run -p crabsnap-settings-ui --example visual_spike      # 嵌套 Hyprland 里看观感
grep -rn 'crate::ui::' crates/ui-base/src crates/ui-chrome/src | grep -v '^.*://'   # 期望 0
```

### Phase 6：切 L6（`compositor` + `layers`）

**验收**：`cargo test -p crabsnap-compositor -p crabsnap-layers`；
真机 `pin` 拖动、冻结层、工具栏毛玻璃三项手测。

### Phase 7：切 L7（`app` + `settings-host` + `crabsnap`）

1. `app`（含 `input_router.rs`、`daemon.rs`）；
2. `settings-host`（eframe 宿主 + `bin crabsnap-settings`）；
3. `crabsnap`（facade + `bin crabsnap`）；
4. 34 个 `examples/_diag_*` 搬进 `crates/crabsnap/examples/`；
5. `crabsnap settings` 改为 exec `crabsnap-settings`（找不到时打印
   `cargo build -p crabsnap-settings` 提示）；
6. `xtask arch` 接进 CI。

**验收**：
```bash
cargo build --workspace
./target/debug/crabsnap info
./target/debug/crabsnap settings --page save     # 能拉起独立进程
cargo build --examples -p crabsnap               # 34 个示例
readelf -S target/debug/crabsnap | grep -c '\.debug_'    # 0
```

### Phase 8：清理与实测

1. `cargo xtask disk` 记录体积与构建时间（与旧项目对照，填 §5 表格）；
2. `cargo doc --workspace --no-deps` 对账跨 crate 文档链接；
3. 评估 facade 是否可删（ADR-010 退出条件）；
4. 迁移 `AGENTS.md`（§1 的模块表换成 crate 表）、`README.md` 的构建段；
5. 归档旧项目（`git tag pre-crate-split`）；**不迁移旧笔记**，只有经新测试复现的结论才按 AGENTS.md §6.3 重写为新笔记。

---

## 4. 可见性迁移清单（按热点）

| 位置 | 数量 | 处理 |
|---|---|---|
| `render/*` 的 `pub(crate)` | 67 | 切 `render`/`render-gpu` 时逐条判断：跨 crate 需要 → `pub`（放进 `render::damage`/`render::loupe` 等语义模块）；否则保留 `pub(crate)` |
| `platform/*` 的 `pub(crate)` | 28 | 大部分随 `wayland`/`capture`/`inject` 各自的 crate 边界自然消失 |
| `layers/*` 的 `pub(crate)` | 14 | `layers` 是叶子（只被 `app` 用），多数保留 |
| `ui/*` 的 `pub(super)` | 47 处 `super::super` + 其余 | 五拆时按 §2.12 的下沉清单处理；未下沉的保持 |
| `app/*` 的 `super::super` | 13 | `app` 不拆，不动 |
| `pub(in path)` | 30 | 一律改 `pub(crate)`；若跨 crate 需要则改 `pub` + `#[doc(hidden)]` |

**判断顺序**（每个符号）：① 有没有跨 crate 的调用点？② 有 → `pub` 并放到语义化模块；
③ 没有 → 保留原可见性。**不允许**「先全改 `pub` 让编译过，以后再收」。

---

## 5. 验收记录表（Phase 8 填写实测值）

| 指标 | 旧项目 | 新项目 | 判据 |
|---|---|---|---|
| `cargo build -p crabsnap` 冷构建 | 待测 | 待测 | 新 ≤ 旧 |
| `cargo check --workspace` 增量 | 待测 | 待测 | 记录即可 |
| `target/debug` 体积 | 待测 | 待测 | 新 ≤ 旧 × 1.15 |
| 二进制 `.debug_*` 段数 | 0 | 0 | = 0 |
| `target/debug/crabsnap` 体积 | 待测 | 待测 | 记录（预期变小：无 egui/eframe） |
| `cargo tree -d` 重复项 | 待测 | 待测 | 不增加 |
| 测试数 | 622 | **重新审定后的数量** | 按 AGENTS.md §7 审：只保留原子、可证伪的。数量下降是预期，判据是"每条测试都能说出它防的是哪个不变量" |
| 示例数 | 34 | 34 | 不减少 |

---

## 6. 风险与回滚

| 风险 | 何时会炸 | 预案 |
|---|---|---|
| `cfg(crabsnap_egl/gbm)` 静默失效 | Phase 4/6 | Phase 4 结束强制 `grep -rn 'cfg(crabsnap_' crates/` 为 0；`--no-default-features` 构建必须绿 |
| 切分点 `pub(super)` 失效导致编译错误风暴 | Phase 2–6 | 内部布局不变；每个 Phase 只切 1–3 个 crate；错误集中在 `pub(super)` 就按 §4 处理 |
| 测试被「搬着搬着丢了」 | 全程 | 每个 Phase 记录 `cargo test -- --list \| wc -l`，数字只能增不能减 |
| `hotpath` 宏解析失败 | Phase 2 起 | 含 `#[hotpath::measure]` 的 crate 必须保留非 optional 依赖（`architecture.md` §7.2） |
| facade 掩盖真实依赖 | Phase 7 | `xtask arch` 只检查具体 crate 的依赖，不看 facade；facade 只允许 `pub use` |
| 设置窗口 exec 在 dev 下失败 | Phase 7 | 错误信息给构建命令；`cargo xtask run-settings` 兜底 |
| 一次性 diff 太大 review 不动 | 全程 | Phase 1 单独提交；Phase 2–7 每步只搬文件+改路径，`git diff --stat` 应满足「删除行数 ≈ 新增行数」 |
| 回滚 | 任一 Phase | 每个 Phase 一个提交 + tag（`phase-N-done`）；回滚即 `git reset --hard phase-(N-1)-done`，不涉及旧项目 |

---

## 7. 机械改写脚本的骨架

Phase 2–7 的「搬文件 + 改路径」建议用一次性脚本完成（不进仓库，只作为迁移工具）：

```python
# tools/migrate.py（临时，不进 crates/）
STEPS = [
    # (旧前缀, 新 crate 标识, 新内部前缀)
    ("core/error", "crabsnap_core", "error"),
    ("core/hdr", "crabsnap_color", "hdr"),
    ("model", "crabsnap_image", ""),
    ("platform/linux/wayland", "crabsnap_wayland", ""),
    ("render/gpu", "crabsnap_render_gpu", ""),
    # ...
]

def move(old_rel, new_crate):
    dst = f"crates/{new_crate}/src/" + strip_prefix(old_rel)
    shutil.move(f"src/{old_rel}", dst)

def rewrite(path, own_crate):
    # 1) crate::<old> → <new_crate>::<inner>；同 crate 时 → crate::<inner>
    # 2) 文档内链 [`crate::<old>`] 同样处理
    # 3) super::/self:: 不动（内部布局不变）
    ...
```

**每次运行后必须做的三件事**（缺一不可，对应 AGENTS.md 的「实物到了 ≠ 接线到了」）：

1. `git status --short` 核对文件数：搬走的文件数 = 新目录新增的文件数；
2. `grep -rn 'crate::<旧模块>' src/ crates/` 为 0（漏改的路径会留在旧 crate 里）；
3. `cargo check --workspace --all-targets` + `cargo test --workspace`。

---

## 8. 完成定义（DoD）

- [ ] `cargo xtask arch` 绿；`arch.toml` 里没有 `*` 通配（除了 `core` 的 forbid 项）
- [ ] `cargo check --workspace --all-targets` 与 `--no-default-features` 都绿
- [ ] `cargo test --workspace` 全绿；`./scripts/run-tests.sh` 绿；**新增测试逐条过 AGENTS.md §7.6 检查表**
- [ ] 34 个示例可编译，至少 5 个真机跑过并截图存档
- [ ] 两个二进制（`crabsnap`、`crabsnap-settings`）可运行；`crabsnap settings` 能拉起独立进程
- [ ] 根 `Cargo.toml` 的 profile 与旧项目逐字一致；`readelf` 无 `.debug_*`
- [ ] §5 表格全部填写实测值，且都满足判据
- [ ] 每个 crate 的 `lib.rs` 有「是什么 / 不知道什么」两行
- [ ] `AGENTS.md`、`README.md`、`docs/tech-selection.md` 已更新到 crate 语境
- [ ] 旧项目打 tag `pre-crate-split`；新 `docs/notes/` 条数在预算内（AGENTS.md §6.3），索引检查绿
