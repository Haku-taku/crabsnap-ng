# 声明缓存的机制与 app 侧失效契约

## 症状

每个交互帧都在重建整棵声明树：`build` 13.5 ms，其中 `decl` 6.3 ms、`layout` 5.1 ms、
`apply` 0.33 ms——而相邻两帧的声明几乎逐字节相同。

## 根因

`Element::decl` 每帧全量重算（样式解析、文本块构造），`Tree::apply` 每帧深比较并克隆
`NodeData`（文本节点连 `Vec<TextRun>` 一起）。没有任何一层知道「这棵树没变」。

## 修法

- `Decl.children: Vec<Rc<Decl>>`。节点记下来源 `Rc<Decl>`，`apply` 用 `Rc::ptr_eq`
  跳过 `NodeData` 的深比较与克隆（仍逐节点走一遍，以标记 `reused`、折入 env/disabled）。
- `Element::cache_key() -> Option<CachedKey>`：身份 + 自身输入指纹。`element_decl` 先查
  `DeclCache`，命中直接返回上一帧的 `Rc<Decl>`，`decl()` 根本不跑。`Container` 给不出
  指纹（外壳由 app 建），因此不缓存；控件接入进度见 `docs/decl-incremental-design.md`。
- 构建期所有经 `Cx` / `NodeCx` 的读取都记成 `Dep`。动画调用记的是**调用本身**，
  校验时重放一次：值没变就命中，值变了就重建，实例也因此照常推进。
- 校验一条目 = 它自己的依赖全部成立 **且** `decl.children` 里每个有缓存条目的孩子都成立。

## 契约（app 必须遵守）

1. **引擎看不见的状态变了，就要说**：`App::changed` 被调用时引擎丢弃全部条目；
   自己改状态（主题、定时器）则在 `view()` 里调 `cx.invalidate_decls()`。
2. **在元素构建之外读引擎事实**（`page_body` 里 `cx.at(id).animate(..)` 再塞进控件）
   没有条目能认领这次读取，该帧缓存整体作废。这条是自动的（`Recorder::unattributed`）。
3. 重放假设：控件对 `animate` 的调用是「已记录依赖 + 控件自身参数」的函数。
   目标变了而依赖没变，动画就会冻在旧值上——那是第 1 条被违反了。

## 如何验证

- `cargo test -p ui-platform --lib -- decl_cache_tests`：复用、悬停失效、动画继续、
  上报失效四条，都做过「临时破坏 → 变红 → 恢复」。
- `cargo build --release --example gallery --features profile` 后看 `decl` / `apply` /
  `layout` 三行。`timed!("page_body")` 是 app 侧控件构造的成本，缓存管不到它。
