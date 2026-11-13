# clippy 基线（2026-11-04）

> 队列 B3 的产物：先让 clippy 跑得完，再把存量告警分级。**这里是账本，不是待办清单**——
> 新代码不得增加告警；改到某个文件时顺手清掉它自己的。

## 1. 命令与现状

```
cargo clippy --workspace --all-targets
```

| 指标 | 基线前 | B3 基线 | **机械清理后（2026-11-04）** |
|---|---|---|---|
| 编译错误（deny 级 lint） | **1**（clippy 因此跑不完全仓） | 0 | 0 |
| 告警（按编译单元计） | 476 | 465 | 118 |
| 告警（按源码站点去重） | ~320 | 303 | **75** |

按 crate 的站点数（去重，清理后）：`ui-gallery` 18、`ui-widgets-md3` 16、`ui-gpu-egl` 10、
`ui-paint-gpu` 6、`ui-platform` 5、`ui-core` 5、`ui-platform-wayland` 3、`ui-paint` 3、
`ui-text-fontique` 2、`ui-design` 2，其余各 1（`ui-widgets-glass` 已归零）。

按 lint 的站点数（清理后）：`too_many_arguments` 16、`needless_range_loop` 14、
`type_complexity` 11、`missing_transmute_annotations` 10、`field_reassign_with_default` 8、
`large_enum_variant` 7、`manual_checked_ops` 3、`chunks_exact_to_as_chunks` 2，其余各 1。

**没有加 `-D warnings`**：这些站点不修完就守住它只会逼人加 `allow`。规矩改为
「新增代码零告警 + 触碰即清」。

## 2. 本轮已修（真有害或零风险）

| lint | 站点 | 为什么修 |
|---|---|---|
| `reversed_empty_ranges`（deny） | `ui-core/src/text_input.rs` | 测试要用反转区间验证 `delete_range` 的归一化；原写法让 clippy 直接拒绝编译，整个基线跑不完。改为显式 `#[allow]` 并写明理由 |
| `no_effect` | `ui-gallery/examples/gallery/main.rs` | `Event::PointerMoved { pos };` 被当语句丢弃——手势脚本本想把指针先移到按下点，事件从未送达。改为像扫动脚本那样先 `dispatch` 移动再按下 |
| `neg_cmp_op_on_partial_ord` | `ui-core/src/grid.rs` | **不采纳建议**：`!(space > 0.0)` 正是让 NaN 算「没有空间」的写法，clippy 建议的 `space <= 0.0` 会放过 NaN、把 NaN 传给整条 `fr` 轨道。改成 `space.is_nan() \|\| space <= 0.0`，行为不变且意图显式 |
| `if_same_then_else` | `ui-design/src/fluent.rs`、`ui-widgets-md3/tests/token_parity.rs`、`ui-platform/examples/window_hello.rs` | 三处两个分支写同一结果：前两处是真同义（合并条件），示例里那个 `if i == 0` 是残留，两臂都是 `Damage::Full` |
| `redundant_locals` | `ui-core/src/layout_engine.rs`、`ui-platform/examples/window_hello.rs` | `let params = params;` / `let node = node;` 都是空绑定，删除 |
| `needless_update` | `ui-widgets-md3/tests/scaffold.rs` | `FluentOptions` 只有一个字段，`..Default::default()` 是空更新 |
| `wrong_self_convention` | `ui-core/src/colour.rs` | `Chromaticities` 是 `Copy`，`to_xyz(&self)` 改为取值 |
| `len_without_is_empty` / `new_without_default` | `ui-style/src/roles.rs`、`ui-platform-wayland/src/input.rs` | 补 `is_empty` 与 `Default`（委托 `new`） |
| `unnecessary_sort_by`、`question_mark`、`manual_clamp`、`let_and_return`×2、`filter_next` | `ui-core/src/hit.rs`、`ui-style/src/roles.rs`、`ui-text-fontique/src/lib.rs`、`ui-widgets-md3/tests/mod.rs` | 等价改写，无行为变化 |

验证：`cargo build --workspace --tests --examples` 零 warning、全量 41 个 test binary 通过、
gallery selftest 210 ✓ / 0 ✗。

### 2.1 整仓机械清理（2026-11-04，B4）

`cargo clippy --workspace --all-targets --fix --allow-dirty --allow-no-vcs`，跑两遍
（第二遍收掉 `--no-deps` 才可见的那几条）。**站点 304 → 75，改动 123 个文件**。
清掉的是纯机械类：`collapsible_if`（106，此前担心注释被挤进条件表达式，实测 clippy 会保留
注释位置）、`derivable_impls`（20）、`unnecessary_lazy_evaluations` 之外的可机器改写项、
`manual_range_contains`、`redundant_static_lifetimes` 等。

本仓库**没有版本控制**（`/home/ollie/git/crabsnap-ng` 不是 git 仓库），所以先
`rsync -a --exclude target` 快照到 `/tmp/ui-snapshot` 再动，事后用 `diff -rq` 核对改动面。
验证：零 warning、全量测试 1095 项通过、gallery selftest 219 ✓ / 0 ✗。

## 3. 接受并记录（不修，附理由）

| lint | 站点 | 判定 |
|---|---|---|
| `collapsible_if` | ~~106~~ 0 | 机械清理清掉（注释位置实测被保留） |
| `derivable_impls` | ~~20~~ 0 | 机械清理清掉（`#[derive(Default)] + #[default]`） |
| `chunks_exact_to_as_chunks` | 2 | 新 lint 推 `as_chunks`；现有 `chunks_exact` 语义相同、可读性更好，保持一致 |
| `unnecessary_lazy_evaluations` | ~~19~~ 0 | 机械清理清掉 |
| `too_many_arguments` | 16 | 集中在 `layout_engine.rs` 的内部递归函数（8–13 参）；拆成上下文结构体会改变调用形态，属于重构而非清理 |
| `needless_range_loop` | 14 | 多为「同一循环里索引 2–3 个数组」（`for k in 0..N { a[k] = b[k] }`）或示例页枚举，`enumerate()` 反而要同时解构多个迭代器；改写收益低于阅读成本 |
| `type_complexity` | 11 | 多是 `Rc<RefCell<Vec<…>>>` 这类引擎自有结构，抽别名只是把复杂度藏起来 |
| `missing_transmute_annotations` | 10 | FFI 符号表：目标类型已由 `let f: FnPtr = transmute(p)` 的绑定给出，调用处再加 turbofish 是噪声 |
| `field_reassign_with_default` | 8 | 结构体默认值 + 逐个改字段；改成结构体字面量会把 `Default` 里已有的字段重复一遍 |
| `large_enum_variant` | 7 | 逐一看过：`NodeData`、`BackdropMaterial`、`Material`、`AppBarLeading`、示例页枚举。box 会换来每次构建每个节点一次堆分配，**比省下的几个字更贵**；`BackdropEntry` 一类每帧只有个位数 |
| `explicit_auto_deref`、`bool_assert_comparison`、`manual_is_multiple_of`、`unnecessary_cast`、`clone_on_copy`、`manual_checked_ops`、`collapsible_match`、`unnecessary_map_or`、`unnecessary_mut_passed`、`empty_line_after_doc_comments`、`redundant_static_lifetimes` | 各 ≤3 | 纯等价改写；`clone_on_copy` 在几处刻意的复制比 `*x` 更清楚，`bool_assert_comparison` 全在测试 |

## 4. 规矩

1. 新增/改动的代码 `cargo clippy --workspace --all-targets` 里自己的站点必须为 0。
2. 文件被触碰时，顺手清掉**该文件**的告警（顺序：先跑测试，再清，再跑）。
3. 要加 `allow` 必须写一行为什么（先例：`text_input.rs` 的反转区间）。
4. clippy 的建议可能与规格冲突——`grid.rs` 的 NaN 是例子；遇到这种，改写成**显式且等价**的形式，并把理由记进这里或 `docs/lessons/design-errors.md`。
