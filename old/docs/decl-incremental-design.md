# 增量声明重建：让控件声明真正被缓存

> 现状：声明缓存（`DeclCache`）机制完整，但**没有任何元素给出缓存键**，因此每帧仍重建
> 整棵声明树。本文说明为什么现在没人给键、给出键需要补什么，以及分几步落地。

## 1. 现状（2026-11-04 实测，275Hz，progress 页）

```
build 370µs  (decl 127 · layout 58 · apply 53 · text 40 · damage/paint 16 · hit 12)
present 162µs
```

`decl cache: N entries, 0 hits, 0 misses` —— `element_decl` 只在元素返回
`cache_key()` 时才查缓存，而全仓只有 Fluent `Navigation` 返回，且它转发给一个
`Container`（`Container::cache_key` 已改为 `None`，见 design-errors）。等价于缓存没接上。

## 2. 机制已经就位（缺的只是键）

| 能力 | 位置 | 状态 |
|---|---|---|
| 记录事实 | `Cx::record` → `Recorder::push`（按帧分段） | 有 |
| 校验事实 | `Cx::validate` / `dep_holds`；动画调用**重放** | 有 |
| 子树联动 | 校验一条目 = 自身依赖成立 ∧ 每个有缓存条目的孩子成立 | 有 |
| 复用声明 | `element_decl` 命中返回上一帧的 `Rc<Decl>` | 有 |
| 跳过应用 | `Tree::apply_decl` 的 `Rc::ptr_eq` 快路径 | 有 |
| **给出键** | `Element::cache_key()` | **无实现** |

## 3. 为什么 `Container` 不能给键，而控件可以

`Container::decl` 是「app 建好的 `own` 外壳 + 子元素」，`own` 来自 app 的
`Container::new(..).gap(..).padding(..)` —— 这些是**应用输入**，不是引擎事实，记不进
`Dep`。条目因此永远无法证明 `own` 没变，`Container` 被移出缓存是正确的。

控件不同：`Widget::decl(&self, cx)` 的契约写明「控件可用的一切都经 `cx` 到达」。
引擎事实（悬停、按下、动画、滚动）全部可记录、可重放；剩下的唯一输入是控件**自身的
字段**（`Opts`、标签、数值）。把这份输入压成一个指纹，随条目存下，查找时比对即可。

**健全性判据（控件作者必须遵守）**：`decl()` 除 `cx` 之外读到的一切，都必须进指纹。

## 4. 设计

### 4.1 键 = 身份 + 指纹

```rust
/// Identity and content fingerprint, when the control can prove both without building.
pub struct CachedKey { pub id: Id, pub props: u64 }
```

- `Element::cache_key() -> Option<CachedKey>`；`Container` 仍返回 `None`。
- `Widget::cache_key() -> Option<CachedKey>`；`Styled` 直接转发控件的答案。
- `Entry` 增加 `props: u64`；`Cx::cached_decl(key)` 改为 `cached_decl(key: CachedKey)`，
  `entry.props != key.props` 直接算未命中——**指纹不是 `Dep`**，因为校验期引擎拿不到
  「当前」指纹，只有元素自己知道。

### 4.2 指纹怎么算

控件通常已经收一个 `Opts`（`Md3Slider::with_opts(opts)`）。规则：**`decl` 只读 `Opts`
与 `cx`**，则指纹 = `hash(opts)`。落地时对 `Opts` 派生 `Hash`，控件补一行
`fn cache_key(&self) -> Option<CachedKey> { Some(CachedKey::new(self.id, hash(&self.opts))) }`。

对不收敛在单一结构里的控件（字段散在 `self` 上），逐个手写 `hash` 组合，或先把它收进
一个 `props` 结构——后者更好，顺带统一了控件形状。

### 4.3 首次构建与后续查找

```
lookup(key, props) → 命中: 返回 Rc<Decl>
                   → 未命中: begin_record(); decl(cx); end_record(key, props, decl)
```

`end_record` 把 `props` 写进条目。注意：**不要**把指纹塞进 `Dep`——`dep_holds` 无法
回答它；写成条目字段，只由查找方比对。

## 5. 分阶段

| 阶段 | 内容 | 验收 |
|---|---|---|
| 1 | 引擎接线：`CachedKey`、`Entry.props`、`element_decl` 传参 | 全量测试绿；缓存统计仍是 `0 hits`（没人给键） |
| 2 | `Styled` 转发 + 先接 2~3 个控件（progress 页用到的） | `decl` µs 下降；`hits` 上升；`CRABSNAP_TARGET_CHECK=1` 全程 `differing=0`；selftest 219 ✓ |
| 3 | 其余控件按家族推进（md3 → fluent → glass） | 每批一次像素比对 + CPU/帧 |
| 4 | 容器是否也缓存：先量 `own` 的相等比较成本，再决定 | 数据说话，不预设 |
| 5 | 文档：`docs/notes/decl-cache.md` 改写成现行契约（它现在还写着「`Container` 返回自身 id」），并把健全性判据写进控件作者指南 | 引用一致 |

## 6. 风险与对策

- **漏字段 → 陈旧像素**（静默）。对策：指纹统一从 `Opts` 派生，减少手写面；每批控件
  接入后用 `CRABSNAP_TARGET_CHECK` + 截图比对；对「改一个 props 必须看到变化」的控件
  写一条测试。
- **命中率低而白付哈希成本**。对策：每阶段都看 `decl cache` 的 hits/misses 与 `decl` µs，
  不降就回退。
- **动画与缓存的关系**：`Dep::Anim*` 校验时重放，实例照常推进；但 `Age`/`Now` 类
  时间读会让条目每帧失效——时间驱动的控件本来就要每帧重建，不算损失。

## 7. 阶段 2 实测：控件级声明缓存是**中性**的（2026-11-04）

接上 `Md3Slider` 后（slider 页，sway 拖动 20s）：

```
decl cache: 9 entries, 6128 hits, 370 misses        # 94% 命中
A/B（CRABSNAP_NO_DECL_CACHE=1）：decl 116.6 / 115.7µs，root_decl 34.7 / 34.4µs
```

**命中率 94%，CPU 却没有变化。** 把 `decl` 拆开看原因：

```
decl 111µs = view 79µs（app 构造元素树）+ root_decl 32µs（引擎转成声明）
```

两件事同时成立：

1. 控件把昂贵的工作放在**构造函数**里（`with_opts` 解析 spec、算颜色、推进动画），
   `decl()` 只是把已算好的字段拼成 `Decl`——本来就不贵，缓存掉它省不了什么；
2. 命中仍要走 `validate`（每个依赖重放、递归校验子条目），成本与那些便宜的
   `decl()` 体相当。

所以这条路的上限是 `root_decl` 的 32µs，而不是 `decl` 的 111µs。**真正的成本在
`view()` 的 79µs：应用每帧重新构造整棵元素树**，引擎无从知晓哪棵子树没变。

### 要继续压，得换这一层

- **app 侧子树复用**：给 `Cx` 一个 `memo(key, deps, || subtree)`，app 用它包住整页里
  不变的区块；引擎记录 deps，命中时连元素树都不构建。这是唯一能碰到 `view()` 的
  改动，但要求 app 逐块标注（gallery 的页面代码要改）。
- **子树级 layout/paint 跳过**：声明 `Rc` 不变时，`apply`/`layout`/`paint`/`hit` 仍在
  逐节点走（32+38+12+12µs）。跳过它们同样是子树失效问题，可与上一条共用同一套标记。

保留现状的理由：控件级缓存本身正确、有测试、实测中性；一旦出现 `decl()` 真的昂贵的
控件（玻璃层、长文本），它是现成的。但**不要**指望它带来两位数收益。

## 8. 阶段 B 实测：容器缓存 + 子树跳过同样中性（2026-11-04）

思路：让容器也能自证——`Element::cache_matches(&cached)` 让容器把自建的外壳与缓存里的
那份**逐字段比对**（`Decl::own_eq`，含 `PartialEq` 漏掉的 `disabled`）；条目增加
`whole`（构建时每个子元素都是已校验的条目），`validate` 对 `whole` 条目要求全部子条目
成立，`cached_decl_matching` 要求 `whole`；`apply_decl` 遇到同一个 `Rc` 就地跳过整棵
子树（`mark_reused_subtree`）。

```
容器缓存开/关（slider 页 sway 10s）：683 vs 669 µs/帧   —— 噪声内
各阶段（改前 → 改后）：root_decl 34.0 → 33.3，apply 31.6 → 31.3，layout 35 → 35
decl cache：29 entries，6641 hits，16037 misses
```

**为什么没用**：

1. **交互期间缓存整体作废**。app 在 `view()` 里读的路由事实（悬停、指针位置）每帧都变，
   `Cx::settle_outside` 因此每帧 `invalidate_decls()`——命中率接近 0，正是最需要缓存的
   时候。这是既有的保守契约，放宽它就是拿陈旧子树冒险。
2. **动画节点长在树干上**。`Rc` 复用只能救"没变的兄弟分支"；拖动一个滑块会改变它到根
   的整条路径上每一层的 `Rc`，于是没有任何一层能被复用。
3. **树遍历不看 `Rc`**。即使命中，省下的也只是声明构造（`root_decl` ~34µs 的一部分），
   `apply`/`layout`/`hit`/`paint` 仍逐节点走。

**留下的东西**：`whole` 修掉了一个真正的漏洞——旧 `validate` 只检查"有登记的子条目"，
子条目消失后父条目仍算成立，会把陈旧子树冻住；现在 `whole` 条目必须全部子条目成立。
`apply_decl` 的同 `Rc` 跳过本身是免费的（复用真的发生时才走到）。容器缓存实测中性，
理由与阶段 2 相同：正确、有测试、留着备用。

**结论**：以 `Rc` 恒等为信号的子树复用，在本引擎当前的失效模型下拿不到收益。要走
"没变的子树不重建"，得换信号（例如 layout 侧的脏标记 + 约束/原点未变），并同时让
`layout`/`paint` 认这个标记——那是另一套设计，不是本缓存能延伸出来的。
