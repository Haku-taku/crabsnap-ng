# i18n（Fluent）方案

> 状态：**调研 + 设计，未实施**。日期 2026-10-31。
> 范围：`ui/` workspace 的 UI 引擎、三套控件与 gallery；不涉及旧项目 `crabsnap` 的
> `core/i18n` 实现。旧项目材料按 `AGENTS.md` §1 只作待验证输入。
>
> 采信规则按 `AGENTS.md` §1.2：每条承重事实给出本仓库 `path:line`、外部源码/官方文档，
> 或一个可运行的最小探针。做不到的标 `[未证实]` 并写风险。

---

## §0 结论

1. **新增一个可移植 crate `ui-i18n`，它是全 workspace 唯一的 i18n 实现。**
   依赖只允许 `fluent-bundle 0.16.0`、`unic-langid 0.9.6`、可选的
   `fluent-langneg 0.13.1`；构建期用 `fluent-syntax 0.12.0` 解析 `.ftl`。
   `ui-widgets` 与 `ui-widgets-md3` 依赖它；`ui-widgets-fluent`/`ui-widgets-glass`
   目前没有自有文案，可以不直接依赖；`ui-gallery` 直接依赖它；`ui-style` **不**依赖它。
   新 crate 必须加进
   `ui/crates/ui-platform/tests/platform_boundary.rs:16` 的 `PORTABLE`，否则
   `every_abstraction_crate_is_classified` 会直接红。

2. **控件通过 `Cx` 拿字符串，不通过 `Decl`、也不通过全局 `t()`。**
   `Catalog` 以 `Arc<Catalog>` 放进 `Env` 的一层；`ui-widgets::Cx::new`
   （`ui/crates/ui-widgets/src/cx.rs:29`）用 `env.find::<Arc<Catalog>>()` 解析一次并缓存。
   `cx.t(Msg)` / `cx.t_args(Msg, &Args)` 是控件的唯一入口。`Decl` 只携带已解析的
   `Arc<str>`/文本，不携带消息 key。

3. **静态消息在加载时预格式化，运行时零 bundle 查找、零分配。**
   `build.rs` 从 `locales/en/**/*.ftl` 生成 `Msg` 常量与 `ALL` 表；每个 locale 把无参数消息
   格式化成 `Vec<Arc<str>>`，`cx.t(Msg)` 是一次数组索引 + `Arc` 克隆。带参数消息走
   `format_pattern`，按需格式化。`Text` 的 body 从 `String` 改为 `Arc<str>` 是端到端零分配的
   配套改动；不改的话 i18n 层仍不做查找，但 `Text::build` 还会 clone 一次
   （`ui/crates/ui-widgets/src/text.rs:221`）。

4. **locale 回退是显式链，每 locale 一个 bundle。**
   `Catalog` 按 `zh-CN → zh → en` 依次查。`FluentBundle.locales` 只用于 intl 格式化器的
   语言协商，**不会**让 `get_message` 跨 bundle 回退；探针已证实：只装 zh 资源的 bundle
   查不到只存在于 en 的 key。切 locale = 新建不可变 `Arc<Catalog>` + 新 `Env` 层，旧缓存随
   旧 Catalog 丢弃；不在原 Catalog 上原地清缓存。

5. **Fluent 0.16 的能力边界必须写进验收。**
   它**没有 `intl` feature**（features 只有 `default` 与 `all-benchmarks`）；
   `NUMBER` 实际只调 `f64::to_string`，不做分组、小数点或货币本地化；
   `DATETIME` 未实现（`add_builtins` 里是 `TODO`，issue #181 仍 open）。
   复数、`select`、变量插值可用；本地化数字/日期另加 ICU4X 2.4（phase 3）。
   **不要写 `-plural` 变体**：Fluent 的复数是 select expression 的 CLDR 类别分支；
   **消息 id 不能带点**，只能用 `[a-zA-Z][a-zA-Z0-9_-]*`，点留给 attribute。

---

## §1 运行时形状与数据流

### 1.1 现状：控件是纯函数，每帧重建 `Decl`

| 事实 | 位置 |
|---|---|
| 控件是 `(self, cx) -> Decl` 的纯函数，不跨帧持有状态 | `ui/crates/ui-widgets/src/widget.rs:20` |
| `Cx` 持有引擎上下文与当前 `Env` | `ui/crates/ui-widgets/src/cx.rs:19` |
| `Env` 是不透明、可链式嵌套的作用域；参数是 `Arc<dyn Any + Send + Sync>` | `ui/crates/ui-style/src/env.rs:9`、`:14` |
| `Decl` 携带 `env`，相等性按 `Arc::ptr_eq` 比较 | `ui/crates/ui-core/src/tree.rs:525`、`:536-541` |
| 文本控件把 `body: String` 克隆进 `TextRun` | `ui/crates/ui-widgets/src/text.rs:27`、`:221` |
| `TextRun::new` 收 `impl Into<String>` | `ui/crates/ui-core/src/text.rs:397` |
| `Styled` 每个元素新建一个 `Cx`，并克隆当前 `Env` | `ui/crates/ui-widgets/src/styled.rs:29` |

这决定了三件事：

- 字符串不能在控件里“缓存到 `self`”——控件本身每帧都可能重新构造；
- 每帧重建 `Decl` 意味着任何“每帧查 bundle”的设计都会被放大到每个文本节点；
- `Decl` 的 `env` 身份是现成的缓存失效信号：换 `Env` 就会让旧声明不相等。

### 1.2 为什么不是另外三种形状

| 方案 | 问题 |
|---|---|
| `Decl` 里放 `Rc<str>`，由调用方解析 | 解析点分散到每个调用方；控件内部的默认文案（日期/时间选择器）没有调用方可以传；同一 key 在不同页面重复解析；`Decl` 相等性会退化成字符串比较 |
| 编译期常量（`&'static str`） | 只能覆盖默认 locale；复数/`select` 需要运行时参数；切 locale 必须重编译 |
| app 侧自己维护一张表，控件只收 `&str` | 这正是今天的现状：`ButtonParams::new(id, "Filled")` 之类的字面量散落在 gallery 与控件默认值里；没有统一入口，也没有防漏翻检查 |

### 1.3 选定的形状

```
app 启动 / 切 locale
  └─ Catalog::builtin(requested, fallback)  ->  Arc<Catalog>
       └─ Env::with(style_env, catalog.clone())   // 一层不透明参数
            └─ Styled { control, env }
                 └─ Cx::new(core, env)
                      ├─ i18n = env.find::<Arc<Catalog>>().cloned()
                      └─ control.decl(cx)
                           └─ cx.t(Msg::MD3_DATE_PICKER_HEADER) -> Arc<str>
                                └─ Text::new(id, arc, size, ink)
```

`ui-i18n` 不依赖 `ui-core`/`ui-style`/`ui-widgets`，所以不会成环。`Env` 仍然只认识
`Arc<dyn Any>`；`ui-style` 不知道 Fluent 的存在。

### 1.4 最小代码草图

构建期生成的 key 表（`ui-i18n/build.rs` 写进 `OUT_DIR`）：

```rust
// ui-i18n/src/msg.rs（生成）
/// A message key, an index into the generated table.
/// 消息 key，指向生成表的索引。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Msg(u16);

/// One generated entry: Fluent id, optional attribute, and whether it needs args.
/// 一条生成项：Fluent id、可选 attribute、是否需要参数。
pub struct MsgSpec {
    pub id: &'static str,
    pub attr: Option<&'static str>,
    pub dynamic: bool,
}

pub const ALL: &[MsgSpec] = &[
    MsgSpec { id: "md3-date-picker-header", attr: None, dynamic: false },
    // ...
];

pub const MD3_DATE_PICKER_HEADER: Msg = Msg(0);
```

运行时 catalog：

```rust
// ui-i18n/src/catalog.rs
pub struct Catalog {
    /// Locale bundles in fallback order; first is the requested locale.
    /// 按回退顺序排列的 locale bundle；第一个是请求的 locale。
    chain: Vec<LocaleBundle>,
}

struct LocaleBundle {
    bundle: fluent_bundle::concurrent::FluentBundle<Arc<FluentResource>>,
    /// Preformatted no-arg messages, indexed by `Msg`.
    /// 预格式化的无参消息，按 `Msg` 索引。
    static_text: Vec<Option<Arc<str>>>,
}

impl Catalog {
    /// A static message: one array index and one refcount bump.
    /// 静态消息：一次数组索引加一次引用计数。
    pub fn text(&self, msg: Msg) -> Arc<str> { /* ... */ }

    /// A parameterized message: format on demand, fall through locales on error.
    /// 带参消息：按需格式化，出错时沿回退链继续。
    pub fn text_with(&self, msg: Msg, args: &Args<'_>) -> Arc<str> { /* ... */ }

    /// The app's own key path; built-in keys use `Msg`.
    /// 应用自有 key 的入口；内置 key 走 `Msg`。
    pub fn text_by_id(&self, id: &str) -> Arc<str> { /* ... */ }

    /// Same, with arguments; `text_by_id` is the no-arg convenience.
    /// 同上，带参数；`text_by_id` 是无参便捷入口。
    pub fn text_by_id_with(&self, id: &str, args: &Args<'_>) -> Arc<str> { /* ... */ }
}
```

`Cx` 侧：

```rust
// ui-widgets/src/cx.rs
pub struct Cx<'a, 'b> {
    core: &'b mut ui_core::cx::Cx<'a>,
    env: Arc<Env>,
    /// Resolved once per element build; the Env chain is not walked per lookup.
    /// 每个元素构建时解析一次；每次查字符串不再走 Env 链。
    i18n: Option<Arc<ui_i18n::Catalog>>,
}

impl<'a, 'b> Cx<'a, 'b> {
    pub fn t(&self, msg: ui_i18n::Msg) -> Arc<str> {
        self.catalog().text(msg)
    }

    pub fn t_args(&self, msg: ui_i18n::Msg, args: &ui_i18n::Args<'_>) -> Arc<str> {
        self.catalog().text_with(msg, args)
    }

    /// The app's catalog, or the read-only English one for tests and bare `Env::root()`.
    /// 应用 catalog；测试与裸 `Env::root()` 用只读英文 catalog。
    fn catalog(&self) -> &ui_i18n::Catalog {
        self.i18n.as_deref().unwrap_or_else(ui_i18n::Catalog::english)
    }
}
```

`Catalog::english()` 是 `OnceLock<Catalog>` 里的只读英文 catalog：没有 `Catalog` 的
`Env`（`Env::root()`、旧测试、`Styled` 的单元测试）会回落到它，并在第一次回退时
`log::warn!` 一次。它不替代应用 catalog，只是让“忘记装 catalog”不变成 panic；阶段 0 的
验收会断言 gallery 的根 `Env` 确实装了非英文 catalog（切到 zh-CN 能出中文）。

控件里只出现 key，不出现英文：

```rust
Text::new(
    self.id.with("title"),
    cx.t(ui_i18n::Msg::MD3_TIME_PICKER_TITLE),
    p.title_type.size,
    p.title,
)
```

### 1.5 每帧成本

- **静态消息**：`Catalog::text` 是 `Vec` 索引 + `Arc::clone`；没有 `get_message`、没有
  `format_pattern`、没有 `String` 分配。基准见 §2.4。
- **带参消息**：一次 `get_message` + 一次 `format_pattern` + 一次 `String`/`Arc` 分配。
  本机 release 实测约 46–204 ns/次（§2.4），相对布局/绘制的微秒级成本可以忽略。
- **缓存命中时**：`DeclCache` 命中就不会调用 `decl()`，也就不会调用 `cx.t`；locale 切换
  必须换 `Env`，`Decl` 的 `env` 身份比较会让旧声明失效（`tree.rs:536-541`）。
- **文本节点本身的分配**：`Text::body` 是 `String`，`build` 里 `self.body.clone()`
  （`ui-widgets/src/text.rs:221`）仍会分配一次。要让“每个静态文本节点每帧零分配”成立，
  需要把 `Text::body` 与 `TextRun.text` 改成 `Arc<str>`（`Arc<str>: From<&str>` 与
  `From<String>` 都存在，调用点可以渐进迁移）。这是 i18n 的配套改动，不是 i18n 查找成本。

---

## §2 Fluent 用法与成本

### 2.1 crate 分工与版本（2026-10-31 核对 crates.io / docs.rs）

| crate | 最新版本 | 职责 | 本项目用不用 |
|---|---|---|---|
| `fluent-syntax` | **0.12.0**（2025-05-22，rust-version 1.64） | 解析 FTL 成 AST；`json`/`serde` 是可选 feature | **构建期**用；运行期由 `fluent-bundle` 间接依赖 |
| `fluent-bundle` | **0.16.0**（2025-05-22，rust-version 1.67） | `FluentResource`、`FluentBundle`、`FluentArgs`、`format_pattern`、`NUMBER` | **用** |
| `fluent-templates` | **0.15.1**（2026-08-05，rust-version 1.70） | `static_loader!` / `ArcLoader`、`Loader::lookup`、目录扫描与回退 | **不用**（见 §2.2） |
| `unic-langid` | **0.9.6**（2025-05-09） | `LanguageIdentifier`、`langid!` | **用** |
| `fluent-langneg` | **0.13.1**（2025-10-24，依赖 `unic-langid 0.9`） | `negotiate_languages` | 可选；只在“用户偏好列表 × 可用 locale”时用 |
| `intl_pluralrules` | **7.0.2**（2022-10-19） | CLDR 复数规则；`fluent-bundle` 的传递依赖 | 间接 |
| `intl-memoizer` | **0.5.3**（2025-05-22） | 记忆化 intl 格式化器；并发版用 `Mutex` | 间接 |
| `fluent-pseudo` | **0.3.3**（2025-05-22） | 伪本地化 `transform` | 可选，测试用 |

来源：crates.io API（`/api/v1/crates/<name>`，2026-10-31）；`fluent-bundle` 源码
`Cargo.toml`；`fluent-langneg` 0.13.1 的 `Cargo.toml` 第 65 行 `unic-langid 0.9`。

**关键版本陷阱**：`fluent-langneg` **0.14.2** 已改用 `icu_locid`（docs.rs 依赖页），
而 `fluent-bundle 0.16` 仍用 `unic-langid 0.9`。两者混用会出现
`unic_langid::LanguageIdentifier` 与 `icu_locid::LanguageIdentifier` 类型不匹配。
**本项目固定 `fluent-langneg = "0.13.1"`**，不要升 0.14。

### 2.2 为什么不用 `fluent-templates`

`fluent-templates` 能省掉加载器与回退代码，但它的 `Loader::lookup` 签名返回 `String`
（`src/loader.rs:33`、`:38`），每次调用都分配；`static_loader!` 还要在构建期扫描目录，
需要 `ignore` 或 `walkdir` feature（`macros/src/lib.rs` 的 `read_from_dir`）。它没有
“生成 key 常量”和“预格式化静态消息”的入口，也把缺失 key 变成
`"Unknown localization key: ..."` 字符串。我们只需要它的加载/回退思想，不需要它的
模板引擎与字符串分配，所以直接用 `fluent-bundle` + 自己的 `build.rs`。

`i18n-embed 0.16.0` 是另一个高层选择：它封装了 `FluentLanguageLoader`、`LanguageLoader`
trait、`desktop-requester`（`sys-locale 0.3`）。但它的 `get` 同样返回 `String`，且 locale
探测属于平台/宿主职责；`ui-i18n` 要保持可移植，不在引擎里读环境变量或 OS 设置。
`rust-i18n 4.2.4` 用 YAML + `t!` 宏，不是 Fluent，`AGENTS.md §6.5` 已指定 Fluent，排除。

### 2.3 真实 `.ftl` 片段与调用代码

文件：`ui-i18n/locales/en/md3.ftl`

```ftl
# Date picker. The widget draws what it is given; these are the app's words.
# 日期选择器。控件只画交给它的字，这些是应用的字。
md3-date-picker-header = Select date
md3-date-picker-month-january = January
md3-date-picker-month-february = February
# ... 其余月份与星期缩写
md3-date-picker-weekday-mon = M
md3-date-picker-weekday-tue = T

# Time picker.
md3-time-picker-title = Enter time
md3-time-picker-hour = Hour
md3-time-picker-minute = Minute
md3-time-picker-am = AM
md3-time-picker-pm = PM

# Plural: the variant key is a CLDR category, not a "-plural" suffix.
# 复数：变体 key 是 CLDR 类别，不是 "-plural" 后缀。
md3-list-selected-count =
    { $count ->
        [one] { $count } item selected
       *[other] { $count } items selected
    }

# select for gender/quantity; `*` marks the required default variant.
# 用 select 处理性别/数量；`*` 标出必需的默认变体。
gallery-greeting =
    { $gender ->
        [male] He liked it.
        [female] She liked it.
       *[other] They liked it.
    }

# Attributes belong to the message, not to a separate key.
# attribute 属于消息，不是单独的 key。
md3-dialog-delete-title = Delete { $name }?
    .confirm = Delete
    .cancel = Cancel

# NUMBER is a builtin, but see §2.5: it is not locale-aware in 0.16.
md3-list-file-count =
    { $count ->
        [one] One file
       *[other] { NUMBER($count) } files
    }
```

调用：

```rust
use ui_i18n::{Args, Msg};

let mut args = Args::new();
args.set("count", 3);
let text = cx.t_args(Msg::MD3_LIST_SELECTED_COUNT, &args);

// Static: no args, no lookup, no allocation.
// 静态：无参数、无查找、无分配。
let title = cx.t(Msg::MD3_TIME_PICKER_TITLE);
```

`Args` 是 `FluentArgs` 的薄包装，只暴露 `set`/`get`，避免控件直接依赖 `fluent-bundle`。

### 2.4 查找与格式化成本（本机 release 实测）

探针：`fluent-bundle 0.16.0`，`concurrent::FluentBundle`，`--release`，5,000,000 次循环。

| 操作 | 实测 |
|---|---|
| `format_pattern` 静态消息（单 `TextElement`） | **5.0 ns/op**，返回 `Cow::Borrowed` |
| `get_message`（`FxHashMap` 查找） | **6.2 ns/op** |
| `format_pattern` 带一个变量 | **46.1 ns/op**，分配一个 `String` |
| `format_pattern` 复数 select | **204.2 ns/op**，分配一个 `String` |

源码依据：

- `get_message` 是 `entries: FxHashMap<String, Entry>` 的一次查找
  （`fluent-bundle-0.16.0/src/bundle.rs:139`、`:402`；`src/entry.rs:40`）。
- `format_pattern` 对“只有一个 `TextElement`”的模式直接返回借用值
  （`src/resolver/pattern.rs:94-96`）；含 placeable 时构造 `String`
  （同文件 `:99` 起）。
- `FluentBundle` 只记忆化 intl 格式化器（这里是复数规则），**不缓存格式化结果**；
  已格式化字符串的缓存必须由我们做。

### 2.5 数字、日期、ICU4X

这是 Fluent 0.16 最容易被误判的部分：

| 说法 | 实际情况 | 证据 |
|---|---|---|
| `fluent-bundle` 有 `intl` feature，开启后走 ICU4X | **0.16.0 没有 `intl` feature**；features 只有 `default`、`all-benchmarks` | crates.io 0.16.0 features；`Cargo.toml` |
| `NUMBER` 会按 locale 分组/用本地小数点 | **不会**。`FluentNumber::as_string` 只调 `self.value.to_string()`，再补 `minimum_fraction_digits`；`use_grouping` 字段被解析但从未读取 | `fluent-bundle-0.16.0/src/types/number.rs:148-156`；`use_grouping` 在 `:72`、`:87`、`:114` |
| `DATETIME` 可用 | **未实现**。`add_builtins` 只有 `NUMBER`，下一行是 `// TODO: DATETIME()` | `src/bundle.rs:599-604`；issue [#181](https://github.com/projectfluent/fluent-rs/issues/181) 仍 open |
| 复数规则来自 ICU4X | 来自 `intl_pluralrules 7.0.2`；`fluent-bundle` 通过 `intl-memoizer` 记忆化 | `src/types/plural.rs`；`intl-memoizer-0.5.3/src/concurrent.rs:8-18` |

所以：

- **phase 1** 只用 Fluent 的复数/`select`/变量插值；`NUMBER` 只用于整数计数，接受
  “1000 显示成 1000 而不是 1,000”的现状。
- **phase 3** 再引入 `icu_decimal 2.4.0` / `icu_datetime 2.4.0`（`icu 2.4.0`，2026-10-22）。
  三种接法：
  1. 在 Rust 侧用 ICU4X 格式化，把结果作为 `FluentValue::String` 传入；缺点是不能再用
     这个字符串做复数选择，需要同时传 `$count`（数字）与 `$count_str`（字符串）。
  2. 给 `FluentBundle` 注册自定义函数（如 `LOCAL_NUMBER`、`LOCAL_DATETIME`），在 FTL 里
     显式调用；不能覆盖内置 `NUMBER`（`add_function` 对同名项返回 `Overriding`）。
  3. 实现 `FluentType` 自定义值，让 `{ $n }` 走 `as_string_threadsafe`；但
     `FluentValue::Custom` 不参与复数类别匹配，复数消息仍要传 `FluentNumber`。
- **`DATETIME` 不要写进 FTL 直到 phase 3**，否则 0.16 会输出 `{DATETIME()}` 或
  `{DATETIME($date)}` 原文。

### 2.6 `Send + Sync` 与并发

- 默认 `FluentBundle<R>` 的 memoizer 是 `IntlLangMemoizer`，内部 `RefCell`
  （`intl-memoizer-0.5.3/src/lib.rs`），**不是 `Sync`**。
- `fluent_bundle::concurrent::FluentBundle` 用 `Mutex<TypeMap>`
  （`intl-memoizer-0.5.3/src/concurrent.rs:8-18`），`new_concurrent`
  在 `fluent-bundle-0.16.0/src/concurrent.rs:32`。
- 本仓库外的最小工程用 `static_assertions` 编译验证：

```rust
static_assertions::assert_impl_all!(
    fluent_bundle::concurrent::FluentBundle<FluentResource>: Send, Sync
);
static_assertions::assert_not_impl_any!(
    fluent_bundle::FluentBundle<FluentResource>: Sync
);
```

`Env` 要求参数 `Send + Sync`（`ui-style/src/env.rs:20`），所以 `Catalog` 必须用并发版
bundle；`Catalog` 里的动态缓存用 `std::sync::RwLock`，不要用 `RefCell`。

### 2.7 隔离标记

`FluentBundle` 默认 `use_isolating = true`，会给 placeable 包 U+2068/U+2069
（`src/resolver/pattern.rs:35-60`）。实测：

```
plural 2 => "\u{2068}2\u{2069} files"
```

本方案在 `Catalog` 构造时调 `set_use_isolating(false)`：

- 引擎的文本管线目前没有完整 bidi 支持（`docs/research/layout-flexbox.md:215` 明确把 RTL
  留到需要 i18n 时）；
- 这两个控制字符会进入 `Text` body、选择偏移和快照测试，收益不足以抵消复杂度；
- 等 bidi/RTL 真正落地时再打开，并把“混合方向变量”加入验收。

风险写在 §8。

---

## §3 locale 与回退

### 3.1 类型与解析

`Locale` 直接用 `unic_langid::LanguageIdentifier`（0.9.6）。`langid!("zh-CN")` 需要
`unic-langid` 的 `macros` feature；运行时从字符串解析用 `"zh-CN".parse()`。

### 3.2 显式回退链

```rust
/// Build `zh-Hans-CN -> zh-Hans -> zh -> en` without asking a negotiator.
/// 直接构造 `zh-Hans-CN -> zh-Hans -> zh -> en`，不依赖协商器。
fn chain(requested: &LanguageIdentifier, fallback: &LanguageIdentifier) -> Vec<LanguageIdentifier> {
    let mut out = Vec::new();
    // Exact tag.
    // 精确标签。
    out.push(requested.clone());
    // Language + script, then language alone.
    // 语言 + 文字，然后只留语言。
    if requested.script.is_some() {
        let mut lid = requested.clone();
        lid.region = None;
        lid.clear_variants();
        out.push(lid);
    }
    if requested.region.is_some() {
        let mut lid = requested.clone();
        lid.script = None;
        lid.clear_variants();
        out.push(lid);
    }
    let mut language_only = requested.clone();
    language_only.script = None;
    language_only.region = None;
    language_only.clear_variants();
    out.push(language_only);
    // Fallback exact, then fallback language.
    // 回退 locale 的精确标签，再退到它的语言。
    out.push(fallback.clone());
    let mut fallback_language = fallback.clone();
    fallback_language.script = None;
    fallback_language.region = None;
    fallback_language.clear_variants();
    out.push(fallback_language);
    out.dedup();
    out
}
```

例子：

| 请求 | 链 |
|---|---|
| `zh-CN` | `zh-CN → zh → en` |
| `zh-Hans-CN` | `zh-Hans-CN → zh-Hans → zh-CN → zh → en` |
| `en-US` | `en-US → en` |
| `de-DE` | `de-DE → de → en`（`de` 没有资源时直接 `en`） |

### 3.3 为什么每 locale 一个 bundle

`FluentBundle.locales` 的文档原话是：它保存 locale 回退链是为了“与 i18n 格式化器做语言
协商”，例如日期时间格式化器在该 locale 不可用时找一个合理的回退
（`fluent-bundle-0.16.0/src/bundle.rs` 的 `Locale Fallback Chain` 文档段）。
`get_message` 只查这个 bundle 自己 `add_resource` 进来的条目（`src/entry.rs:40`）。

探针验证：

```
zh bundle only-en => false
```

只装 zh 资源的 bundle，即使 `locales = [zh-CN, zh, en]`，也查不到 en 的 key。
把 en 资源也 `add_resource` 进同一个 bundle 可以让它“查到”，但：

- `add_resource` 遇到重复 id 会返回 `Err(Overriding)`，并**保留先加入的那份**
  （`src/bundle.rs:192-240`；实测 `dup k => "first"`）；
- 同一个 bundle 的复数规则按第一个 locale 决定，消息引用也会跨 locale 解析；
- 这不是“回退”，而是“把多语言混成一张表”。

所以：**每个 locale 一个 bundle，`Catalog` 在 bundle 之间按 §3.2 的链依次查**。
`add_resource` 的 `Err(Overriding)` 在加载期应当直接失败，不靠“先加入者赢”。

### 3.4 `fluent-langneg` 的边界

`fluent-langneg 0.13.1` 的 `negotiate_languages(requested, available, default, strategy)`
适合“用户偏好列表 × 已加载 locale”的匹配。实测 `Filtering` 对
`zh-CN` 给 `["zh-CN", "zh", "zh-Hans", "en"]`，对 `zh-Hans-CN` 给
`["zh", "zh-CN", "zh-Hans", "en"]`——顺序不完全是我们要的显式链。因此：

- **简单 `zh-CN → zh → en` 用 §3.2 的手写链**，确定、可测；
- 只有需要解析 `Accept-Language` 或用户排序偏好时才用 `fluent-langneg`；
- 固定 0.13.1，理由见 §2.1。

手写链**不展开 likely subtags**（例如不会自动把 `zh-CN` 补成 `zh-Hans`）。如果译者只提供
`zh-Hans` 而不提供 `zh-CN`，需要在 `Catalog` 的可用 locale 列表里补一条，或把
`fluent-langneg` 的 `Filtering` 结果作为补充链。phase 1 的 `zh-CN → zh → en` 不需要它。

### 3.5 运行时切 locale

`Catalog` 是**不可变**的：构造后只读，静态表与 bundle 都不改。切 locale：

```rust
let next = Catalog::builtin(langid!("ja"), langid!("en"))?;
let env = Env::with(Some(parent_without_catalog), next);
```

- 旧 `Arc<Catalog>` 在所有引用消失后释放，静态表与缓存一起释放；
- 新 `Env` 的 `Arc` 身份与旧的不同，`Decl::PartialEq` 的 `env` 比较
  （`ui-core/src/tree.rs:536-541`）会让旧声明失效；
- 不要在同一个 `Catalog` 上改 `current_locale` 再清缓存：那会让“已格式化的静态表”
  与“当前 locale”可能短暂不一致，而且需要额外的同步。

如果 app 每帧都新建 `Env`（gallery 的 `style_env` 现在就是这样），`DeclCache` 本来就
不会因为 env 身份命中；这不会因 i18n 变差。i18n 只要求“切 locale 时一定换 Env”。

### 3.6 缺 key 的行为

`Catalog::text` / `text_with` 按链依次尝试：

1. 当前 locale 有静态值 → 返回；
2. 当前 locale 有该消息但格式化报错（缺参数、选择器错误）→ `log::warn!` 一次，继续；
3. 下一个 locale；
4. 全部失败 → `log::error!` 一次，返回 `⟨id⟩`（带尖括号，肉眼可见），不 panic。

“warn 一次”用 `Mutex<HashSet<(Locale, Msg)>>` 记录，避免每帧刷日志。缺 key 不是主要防线，
主要防线在 §6 的测试。

### 3.7 locale 从哪来

`ui-i18n` 不读 `LANG`/`LC_ALL`，也不调 `sys-locale`：这些是宿主/平台职责。app 决定：

1. 显式 CLI/config；
2. 否则平台 locale（app 层可用 `sys-locale 0.3.2`，它属于平台绑定代码）；
3. 默认 `en`。

这样 `ui-i18n` 保持 `PORTABLE`，也符合 `docs/architecture.md` 里“文案由宿主传入”的
分层意图（虽然那段描述的是 `settings-ui`）。

---

## §4 键命名与文件组织

### 4.1 FTL 标识符不允许点

Fluent 的 EBNF 明确写：

```
Identifier ::= [a-zA-Z] [a-zA-Z0-9_-]*
Attribute  ::= line_end blank? "." Identifier ...
```

来源：[projectfluent/fluent `spec/fluent.ebnf`](https://github.com/projectfluent/fluent/blob/master/spec/fluent.ebnf)。
本仓库探针也验证过：`md3.dialog.delete-title = ...` 被 `fluent-syntax 0.12` 解析成
`Junk`，报 `ExpectedToken('=')`；改成 `md3-dialog-delete-title` 后正常。

因此键名规则是：

- **消息 id**：`<domain>-<component>-<part>`，小写 kebab-case，例如
  `md3-date-picker-header`、`md3-time-picker-hour`、`widgets-shortcut-ctrl`、
  `gallery-nav-controls`、`gallery-buttons-filled`。
- **attribute**：Fluent 自己的点号，例如
  `md3-dialog-delete-title.confirm`、`.cancel`。
- 不使用 `-plural` 后缀；复数写在消息内部的 select 变体。
- 不使用大写、空格、缩写；key 是稳定 API，改名必须改代码（生成常量会抓住）。

### 4.2 文件组织

```
ui-i18n/
  locales/
    en/
      common.ftl
      widgets.ftl
      md3.ftl
      fluent.ftl
      glass.ftl
      gallery.ftl
    zh-CN/
      common.ftl
      widgets.ftl
      md3.ftl
      fluent.ftl
      glass.ftl
      gallery.ftl
  build.rs
  src/
    lib.rs
    msg.rs        # generated
    catalog.rs
    args.rs
```

- **按控件 crate 分 domain，不按屏幕分。** 屏幕是组合，控件 crate 才是字符串的语义所有者；
  按屏幕分会在布局变化时搬 key，也会让同一个按钮出现两份文案。
- `domain` = crate 名去掉 `ui-` 前缀：`widgets`、`md3`、`fluent`、`glass`、`gallery`。
- 一个 domain 一个文件，文件内按控件用 `##` 分组注释；不要一个控件一个文件（文件太多），
  也不要一个巨型文件（冲突多）。
- 同一个 key 在多个 locale 文件中必须存在；文件按 locale 分目录，构建期按 domain 聚合。

### 4.3 `ui-widgets-md3` 自己的字符串归谁

**文本归 `ui-i18n`，使用点归 `ui-widgets-md3`。**

- `ui-widgets-md3` 的源码里不再出现英文 UI 文案；它引用
  `ui_i18n::Msg::MD3_TIME_PICKER_HOUR` 这类生成常量。
- `ui-i18n/locales/en/md3.ftl` 是 `md3-*` key 的唯一来源；`zh-CN/md3.ftl` 是翻译。
- 这样做的代价：加一条控件文案要改 `ui-i18n`（一个跨 crate 的改动）。收益：一个
  `build.rs` 生成全局 key 常量，漏 key 是编译错误；翻译文件集中，便于交给译者。
- 如果将来某个控件 crate 要独立发布，把它的 domain 文件与生成片段一起搬走即可；运行时
  API 不变。
- app 自己的文案不进入 `Msg` 常量：用 `CatalogBuilder::add_fragment` + 字符串 key；
  内置 `Msg` 只覆盖 workspace 自带的字符串。

### 4.4 什么不翻译

| 类别 | 例子 | 处理 |
|---|---|---|
| 设计令牌名、角色名 | `onSurfaceVariant`、`primaryContainer` | 不翻译；它们是标识符 |
| 图标名 | `chevron_left`、`arrow_drop_down` | 不翻译；`Icon` 的 `&'static str` 是查找键 |
| 字体族 | `Google Sans Text`、`Material Symbols Outlined` | 不翻译；`TextStyle.family` 是编译期常量 |
| 产品/体系名 | `MD3`、`Fluent 2`、`HIG` | 一般保留；如要本地化，放 `common-*` 由译者决定 |
| 快捷键物理键名 | `Ctrl`、`Esc`、`Left` | **翻译**；不同键盘布局/语言有不同写法 |
| 诊断/调试标签 | gallery 的 `provenance`、`spec`/`derived` | 开发诊断，不翻译；若要本地化，放 `gallery-*` |
| 用户输入内容 | 文本框里的文字、文件名 | 不翻译；是数据 |

`ui-design/src/system.rs:20` 的 `SystemId::name()` 与 `ui-design/src/source.rs:25` 的
`Source::label()` 被 gallery 的 system 页直接打印；它们是诊断/产品名，按上表处理。

---

## §5 迁移路径

### 5.1 硬编码规模（可复现的统计）

原始字符串字面量（包含测试、spec 表、图标码点表）：

```bash
cd ui
for d in ui-widgets ui-widgets-md3 ui-widgets-fluent ui-widgets-glass ui-gallery ui-core ui-style; do
  printf '%s: ' "$d"
  grep -rhoP '"(?:\\.|[^"\\])*"' crates/$d/src crates/$d/examples 2>/dev/null | wc -l
done
```

| crate | 原始字面量 |
|---|---:|
| `ui-widgets` | 4302 |
| `ui-widgets-md3` | 2587 |
| `ui-widgets-fluent` | 58 |
| `ui-widgets-glass` | 63 |
| `ui-gallery` | 1586 |
| `ui-core` | 696 |
| `ui-style` | 88 |

这些数字**不是**“待翻译文案数”：`ui-widgets` 的 4302 条大部分是
`icon_codepoints.rs` 的图标名；`ui-widgets-md3` 的 2587 条大部分是 `spec/*_tokens.rs`
的令牌名、角色名与测试断言。

按“UI sink + 大写开头/含空格/非 ASCII、排除 Id/tag/format/测试”启发式统计
（脚本见附录 A.4）：

| 范围 | 估计的 UI 文案 |
|---|---:|
| `ui-gallery/examples/gallery/pages` + `shell.rs` + `app.rs`（不含 `main.rs`、`selftest/`） | **265** |
| `ui-widgets-md3` 的日期选择器默认值 | 20（12 月份 + 7 星期 + 1 标题） |
| `ui-widgets-md3` 的时间选择器默认值 | 5（标题、Hour、Minute、AM、PM） |
| `ui-core` 的快捷键标签 | 17（4 修饰键 + 12 键名 + 1 分隔符） |
| `ui-widgets` / `ui-widgets-fluent` / `ui-widgets-glass` 的用户可见默认值 | 0 |
| **合计（不含 gallery 的 265）** | **42** |

gallery 的 265 条分布（前 12）：

| 文件 | 条数 |
|---|---:|
| `pages/buttons.rs` | 33 |
| `pages/text_field.rs` | 31 |
| `pages/dialogs.rs` | 26 |
| `pages/app_bar.rs` | 20 |
| `pages/tabs.rs` | 18 |
| `pages/lists.rs` | 17 |
| `pages/chips.rs` | 17 |
| `pages/inline.rs` | 12 |
| `pages/fab.rs` | 12 |
| `pages/grid.rs` | 9 |
| `pages/carousel.rs` | 8 |
| `pages/cards.rs` | 8 |

结论：**控件库自身的默认文案只有 42 条，gallery 才是大头**。迁移顺序应当是
“先控件默认值（小、影响真实控件）→ 再 gallery（大、验证面广）”。

### 5.2 阶段 0：`ui-i18n` 骨架

改动：

- 新建 `ui/crates/ui-i18n`，加入 workspace（workspace 的 `members = ["crates/*"]`
  会自动包含，但 `PORTABLE` 要手动加）。
- `Cargo.toml`：`fluent-bundle = "0.16.0"`、`unic-langid = { version = "0.9.6",
  features = ["macros"] }`、`log = "0.4"`；`[build-dependencies] fluent-syntax = "0.12.0"`。
- `build.rs`：解析 `locales/en/**/*.ftl`，生成 `Msg` 常量、`ALL`、`MsgSpec`
  （`id`、`attr`、`dynamic`）；同时扫描 `locales/*/` 生成“locale → `include_str!` 资源”
  的静态表。检测重复 key、非法字符、Rust 常量名冲突。
  生成的 `include_str!` 不能写相对路径——生成文件位于 `OUT_DIR`，相对路径会解析到那里。
  用 `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/en/md3.ftl"))`，
  或让 `build.rs` 把 `.ftl` 复制进 `OUT_DIR` 再用相对路径；并加
  `println!("cargo:rerun-if-changed=locales");`。
- `Catalog::builtin(requested, fallback)`：按 §3.2 建链，加载 `locales/<locale>/*.ftl`，
  每 locale 一个 `concurrent::FluentBundle`，`add_builtins()`，
  `set_use_isolating(false)`，预格式化静态消息。
- `ui-widgets::Cx` 加 `i18n` 字段与 `t`/`t_args`；`Cx::for_test` 装英文内置 catalog。
- `ui-platform/tests/platform_boundary.rs` 的 `PORTABLE` 加 `"ui-i18n"`。

验收：

```bash
cd ui
cargo build --workspace --tests --examples          # 零 warning
cargo test -p ui-platform --test platform_boundary  # 三条全绿
cargo test -p ui-i18n                               # en / zh-CN 各一条格式化测试
```

`ui-i18n` 的最小测试：

```rust
#[test]
fn a_static_message_formats_in_both_locales() {
    let en = Catalog::builtin(langid!("en"), langid!("en")).unwrap();
    let zh = Catalog::builtin(langid!("zh-CN"), langid!("en")).unwrap();
    assert_eq!(&*en.text(Msg::MD3_DATE_PICKER_HEADER), "Select date");
    assert_eq!(&*zh.text(Msg::MD3_DATE_PICKER_HEADER), "选择日期");
}
```

### 5.3 阶段 1：控件默认文案（42 条）

| 位置 | 现状 | 改法 |
|---|---|---|
| `ui-widgets-md3/src/date_picker.rs:188-206` | `CalendarLabels { months: [&'static str; 12], weekdays: [&'static str; 7], header: &'static str }`，`Default` 写死英文 | `CalendarLabels<'a>` 改生命周期，删 `Default`（或改成 `#[cfg(test)]` 的 `english_for_test()`）；`DatePickerParams` 要求调用方传 labels |
| `ui-widgets-md3/src/time_picker.rs:131-151` | `TimePickerLabels<'a>` 已有生命周期，但 `Default` 写死英文 | 删 `Default`；调用方用 `cx.t(Msg::MD3_TIME_PICKER_*)` 构造 |
| `ui-core/src/shortcut.rs:35-73` | `KeyChord::label()` 返回英文修饰键与键名 | 把格式化移到 `ui-widgets`（它已依赖 `ui-core` + `ui-i18n`）：`shortcut_label(chord, cx)`；`ui-core` 保留 `KeyChord` 纯数据，不引入 Fluent |

验收：

```bash
cd ui
grep -rn '"January"\|"Select date"\|"Enter time"\|"Ctrl"\|"Esc"' \
  crates/ui-widgets-md3/src crates/ui-core/src crates/ui-widgets/src \
  | grep -v '/tests/'   # 必须为空
cargo test --workspace --features ui-text-fontique/system
```

gallery 的 date/time picker 页在 en / zh-CN 下各截一张图；时间选择器的 AM/PM 在 zh-CN
下应显示为“上午/下午”（由 FTL 决定，不由控件决定）。

### 5.4 阶段 2：gallery（265 条）

改动顺序：

1. `shell.rs`：`Page::title()`（`:168`）返回 `Msg`；`SECTIONS`（`:100`）的 section 标题改
   `Msg`；`STYLES`（`:241`）的显示名改 `Msg`（`tag` 仍是 `&'static str`）。
2. `pages/*.rs`：`BtnSample.label`、`TAB_TEXT`、`TAB_ICON`、`FAB_LABELS`、`title:`、
   `caption`、`hint`、`label(...)` 等 UI sink 全部换成 `Msg` 或 `cx.t(...)`。
3. 保持 ID/tag 不变：`Page::tag()`、`body_id()`、`Id::of(...)` 仍是字符串；它们不是文案。
4. gallery 的 `main.rs` 启动日志/CLI 错误属于用户可见输出，可以走同一个 catalog；如果
   暂不迁移，必须在 §8 记为已知缺口。

验收：

```bash
cd ui
# 把附录 A.4 的脚本落到 scripts/check-i18n-literals.py；先看它红，再改到绿
python3 scripts/check-i18n-literals.py
# 伪本地化：每个可见标签都应有括号与变形字符
GALLERY_PSEUDO=1 GALLERY_PAGE=buttons GALLERY_SECONDS=300 \
  ./target/release/examples/gallery &
scripts/nested-session.sh shot /tmp/gallery-pseudo.png
scripts/nested-session.sh stop
```

同时跑 `GALLERY_SELFTEST=1`：selftest 用 ID 定位，不应因文案变化而失败。

### 5.5 阶段 3：本地化数字/日期（可选）

- 加 `icu = "2.4.0"`（或 `icu_decimal`/`icu_datetime`）到 `ui-i18n` 的 `icu` feature。
- `NUMBER` 的替换方案按 §2.5 的 1/2/3 选一种；先做 `de-DE` 的 `1.234,5` 与 `en-US` 的
  `1,234.5` 对照测试。
- 日期选择器的月份/星期仍来自 FTL，不要用 ICU 的月份名覆盖译者选择的词。
- 验收：`cargo test -p ui-i18n --features icu`；默认 feature 构建不受影响。

### 5.6 阶段 4：完整 app（本 workspace 之外）

`docs/architecture.md` 的 43-crate 方案里 `crabsnap-core` 负责 `i18n::{t,init,init_with}`。
落地时：

- `ui-i18n` 作为 UI 引擎的实现；`crabsnap-core::i18n` 可以重导出它，或把它搬进 core。
- **不要保留全局 `t()` 单例**：它把 locale 藏在全局状态里，测试无法并行，也和 `Env`
  作用域冲突。`Catalog` + `Env` 是替代形状。
- CLI 的 `--help`、daemon 消息、错误输出用同一个 catalog；locale 由 app 在启动时决定。

---

## §6 防漏翻的检查

### 6.1 编译期：key 必须存在

`build.rs` 从 `locales/en/**/*.ftl` 生成 `Msg` 常量。代码里写
`Msg::MD3_TIME_PICKER_HOUR`；如果 `.ftl` 里没有这个 key，常量不存在，编译失败。
这比“运行时回退并打日志”强得多：

- 改名 key → 所有引用点编译失败；
- 删 key → 同上；
- 拼错 key → 同上；
- 新增 key 但没写代码 → 不会报错（无害）；`build.rs` 可以额外生成“未使用 key”列表，
  由可选警告或人工审查处理，不作为验收门槛。

`build.rs` 还要拒绝：

- 同一 locale 内重复 id（`add_resource` 会返回 `Err(Overriding)`）；
- 不符合 `[a-zA-Z][a-zA-Z0-9_-]*` 的 id；
- 两个 id 归一化成同一个 Rust 常量名（例如 `a-b` 与 `a_b`）；
- `en` 中缺失、但被其他 locale 引用的 key（在加载期由 §6.2 抓）。

### 6.2 测试期：locale 完整性

放在 `ui-i18n/tests/locale_parity.rs`（`tests/` 允许读文件；`src` 内的单元测试不做 I/O）：

```rust
#[test]
fn every_locale_has_exactly_the_english_keys() {
    let en = collect_ids("locales/en");
    for locale in ["zh-CN", "ja", "de"] {
        let got = collect_ids(&format!("locales/{locale}"));
        assert_eq!(got, en, "{locale} keys differ from en");
    }
}
```

`collect_ids` 用 `fluent-syntax` 的 AST 收集 `Message.id` 与每个
`Message.attributes[].id`，组合成 `(message, Option<attr>)`。缺失、拼错、多出来的 key
都会红。若某个 locale 暂时允许缺 key，必须在一个显式的 `ALLOW_MISSING` 表里写
`(locale, key, reason)`，测试只对这些放行；不允许“静默回退”。

### 6.3 测试期：每个 key 都能格式化

放在 `ui-i18n/tests/format_all_keys.rs`。对每个 `Msg`：

1. 从 `en` 的 AST 里收集该消息（及 attribute）中所有 `VariableReference` 的变量名；
2. 用 `FluentArgs` 给每个变量填一个 dummy 值（`0.0` 或 `"x"`）；
3. `format_pattern`，断言 `errors.is_empty()`。

这能抓住：

- FTL 里 `{$naem}` 这类拼写错误（格式化时报 missing argument）；
- select 表达式没有默认变体（Fluent 解析器会报错，测试再兜一层）；
- 消息引用指向不存在的 message/term；
- `NUMBER`/自定义函数不存在（会输出 `{NUMBER()}` 并报错）。

注意：`0.0` 对字符串 select 会落到 `*[other]`，这正好是默认分支；对复数会落到
`*[other]` 或 `[one]`（英文 1 会命中 `[one]`，0 命中 `[other]`），两种都应零错误。

### 6.4 测试期：禁止新的硬编码 UI 字面量

在 `ui-i18n/tests/no_hardcoded_ui_strings.rs`（`tests/` 允许 I/O，理由同
`platform_boundary.rs`）扫描：

- `ui-gallery/examples/gallery/pages/**/*.rs`、`shell.rs`、`app.rs`；
- `ui-widgets*/src/**/*.rs`，排除 `#[cfg(test)]` 模块、`spec/`、`icon_codepoints.rs`。

规则：在 UI sink 行（`label(`、`page_title(`、`captioned_row(`、`column(`、
`ButtonParams::new`、`ChipParams::new`、`ListItemParams::new`、`BtnSample::new`、
`title:`、`hint:`、`caption`、`supporting(`、`overline(`）上，如果出现字符串字面量，
且该行不含 `Msg::`/`cx.t(`/`Id::of`/`Id::str`/`.with(`/`Page::`/`format!`，就报红。
允许表：角色名、字体名、图标名、`textSelection`、`md3`/`fluent`/`glass` 列 tag 等。

这条检查是启发式的，会有误报；但它的价值是“新增硬编码时会有人看见”。先让它对现状红，
迁移到绿；之后任何新红都要求加 `Msg` 或加允许表条目（后者要在 PR 里说明理由）。

### 6.5 伪本地化

`fluent-pseudo 0.3.3` 的 `transform` 可以把所有字母变形并加长，用来肉眼发现“没走
catalog 的字符串”：

```rust
bundle.set_transform(Some(|s| fluent_pseudo::transform(s, false, true)));
```

注意：transform 是 `fn(&str) -> Cow<str>`（不是闭包捕获），所以 `Catalog` 要把它作为
构造参数；**伪本地化模式必须用单独的 `Catalog`**，不能复用已经预格式化的静态表，否则
伪文本会被缓存进正常 catalog。

gallery 加 `GALLERY_PSEUDO=1`：每个可见标签都应是 `[Ħeeŀŀoo ...]` 形态；漏掉的说明
它没走 `Msg`。

### 6.6 运行时：回退与日志

§3.6 的回退是最后一道防线，不是验收依据。测试可以断言：

```rust
#[test]
fn a_missing_key_is_visible_and_logged_once() {
    let c = Catalog::builtin(langid!("zh-CN"), langid!("en")).unwrap();
    assert_eq!(&*c.text_by_id("__missing-for-test"), "⟨__missing-for-test⟩");
}
```

（`text_by_id` 也是 app 自有 key 的入口；内置 key 仍走 `Msg`，不会因为这条测试路径而
失去编译期检查。）

### 6.7 CI 命令

```bash
cd ui
cargo build --workspace --tests --examples
cargo test --workspace --features ui-text-fontique/system
cargo test -p ui-platform --test platform_boundary
cargo test -p ui-i18n
```

按 `AGENTS.md` §7.6：每条新检查都要先看它红（临时删一个 key / 加一个字面量），再恢复看绿。

---

## §7 与现有文档的冲突/衔接

### 7.1 `docs/architecture.md`：`crabsnap-core` 的 i18n

`docs/architecture.md:260-267` 写 `crabsnap-core` 负责
`i18n::{t,init,init_with}`、`fluent-bundle 0.16.0`、`unic-langid 0.9.6`，
`.ftl` 用 `include_str!`。那是**43-crate 迁移方案**的目标，不是当前 `ui/` workspace 的
现状；当前 workspace 没有 `crabsnap-core`。

衔接方式：

- 当前：新增 `ui-i18n`，只服务 `ui/`。
- 迁移后：`crabsnap-core::i18n` 可以重导出 `ui-i18n`，或把 `ui-i18n` 搬进 core。
- **不要照搬 `t/init/init_with` 的全局单例形状**：它把 locale 放在全局可变状态里，
  测试不能并行，也和 `Env` 作用域冲突。`Catalog` + `Env` 是替代。
- `docs/architecture.md:528` 说 `settings-ui` “不依赖 i18n（文案由宿主传入）”；这与本方案
  一致：控件只收已解析文本，不自己选 locale。

### 7.2 `docs/shortcut-system-design.md` §7

原文：`KeyChord::label()` 与修饰键名是硬编码英文，违反 `AGENTS.md §6.5`；等 Fluent 层
建立后改为消息引用（`docs/shortcut-system-design.md:82`）。

本方案解决它，但**不是**在 `ui-core` 里加 Fluent：

- `ui-core` 保持纯数据（`KeyChord`、`Key`、`Modifiers`）；
- 标签格式化移到 `ui-widgets` 的 `shortcut_label(chord, cx)`，用
  `Msg::WIDGETS_SHORTCUT_CTRL` 等 key；
- `KeyChord::label()` 删除或标记 `#[deprecated]`，指向新函数。
- 理由：`ui-core` 是 L0，加 `fluent-bundle` 会让引擎依赖一个只用于展示的库，也违反
  “引擎不认识控件/文案”的分层。

### 7.3 `docs/style-object-design.md` / `docs/style-system-design.md`

- `style-object-design.md` 的核心纪律是“令牌一律强类型，永远不写字符串”。本方案把同一
  纪律延伸到消息：用 `Msg` 常量，不在控件里写裸 key 字符串。
- `style-system-design.md:517` 的 `ToggleSpec { label: &'a str }`、`Request` 的 label
  都是“调用方传入已解析文本”。这与本方案一致：解析发生在构造 spec 之前（app/控件层），
  `ui-style` 不认识 i18n。
- `Env` 是本方案的传输通道，但 `ui-style` 不需要依赖 `ui-i18n`：`Env` 只存
  `Arc<dyn Any>`（`env.rs:14`），`ui-widgets` 负责 downcast。

### 7.4 `docs/icon-text-design.md`

- `TextStyle.family` 是 `&'static str`，文档明确说“族是编译期常量，绝不是用户数据”
  （`docs/icon-text-design.md` §3.2）。i18n 字符串属于 `TextRun.text`，不属于 `Family`；
  两者不冲突。
- `Text` 的 body 是 `String`（`ui-widgets/src/text.rs:27`），`TextRun::new` 收
  `impl Into<String>`（`ui-core/src/text.rs:397`）。这是 §1.5 的配套改动点：改成
  `Arc<str>` 后，静态消息从 catalog 到文本节点只增加引用计数。
- `Icon` 的名字是查找键，不是文案；不要翻译，也不要让 `Icon` 走 `Msg`。

### 7.5 `docs/ime-text-design.md`

输入框里的用户输入是数据，不翻译；placeholder、label、错误提示是消息。二者在
`TextFieldParams` 里已经分开（`hint`/`label` 是调用方传入，`value` 是用户数据）。
本方案只替换前者，不动后者。

### 7.6 分层边界与 crate-map

- `ui-i18n` 只依赖纯 Rust crate，没有 `std::os::*`、`libc`、`std::process`，可以进
  `PORTABLE`。
- 它不依赖 `ui-core`/`ui-style`/`ui-widgets`，所以不会成环；`ui-widgets` 依赖它，
  依赖方向仍然向上。
- `docs/crate-map.md` 的 43-crate 表里没有 `ui-i18n`；当前 `ui/` workspace 的 crate 列表
  以 `ui/Cargo.toml` 与 `platform_boundary.rs` 为准。迁移文档落地时再补一行。
- 新 crate 不加进 `PORTABLE`，`every_abstraction_crate_is_classified` 会红
  （`platform_boundary.rs:188-197`）。这是阶段 0 的验收项。

### 7.7 RTL

`docs/research/layout-flexbox.md:215` 把 RTL 留到需要 i18n 时。本方案 phase 1 关闭
Fluent 的隔离标记（§2.7），与“RTL 尚未实现”一致；等 RTL 落地时再打开，并补混合方向
测试。这是有意偏差，不是遗漏。

---

## §8 未证实项与风险

1. **`[未证实]` ICU4X 2.4 的具体 API 与输出**。本文只核对了 crates.io 版本
   （`icu 2.4.0`、`icu_decimal 2.4.0`、`icu_datetime 2.4.0`，2026-10-22），没有编译或跑
   `de-DE` 的 `1.234,5`。风险：ICU4X 2.x 的 API 与数据 provider 仍在演进，编译时间与
   二进制体积会上升；phase 3 必须先做最小探针。

2. **`[未证实]` Fluent 0.16 与 `fluent-langneg 0.13.1` 在本 workspace 的完整集成**。
   版本与类型已核对（都用 `unic-langid 0.9`），但没有把两者放进同一个 `Cargo.lock`
   跑过。风险：其他依赖若引入 `unic-langid` 0.8 或 `icu_locid`，会出现重复类型；
   阶段 0 用 `cargo tree -i unic-langid` 对账。

3. **`[未证实]` FSI/PDI 关闭后的 bidi 行为**。`set_use_isolating(false)` 是为了让文本
   管线不被控制字符污染；但混合方向变量（英文标签里插阿拉伯人名）可能重排。风险：
   等 RTL 支持落地时必须重新评估，不能把“关闭隔离”当成永久决定。

4. **`[未证实]` 伪本地化与静态预格式化的交互**。`set_transform` 在 `format_pattern` 时
   生效；如果伪本地化 catalog 复用了正常 catalog 的静态表，伪文本会被缓存。方案要求
   伪本地化用独立 `Catalog`，但没有实现验证。风险：`GALLERY_PSEUDO=1` 的实现必须自己
   构造 catalog。

5. **`[未证实]` gallery 265 条启发式统计的精确性**。原始字面量 1586 条是精确的；
   265 条是按“UI sink + 大写/空格/非 ASCII、排除 Id/tag/format/测试”的脚本估计，
   会把 `MD3`/`Fluent` 之类的非文案算进去，也会漏掉小写文案。风险：迁移验收不能只依赖
   这个数字，最终以 §6.4 的扫描为准。

6. **`[未证实]` `Text` 改 `Arc<str>` 的波及面**。`Text.body` 是 `pub` 字段，`TextRun.text`
   被 `TextBlock::text`、`text_pass` 的哈希、`ui-text` 的断行读取；`Arc<str>` 解引用为
   `&str`，理论上兼容，但没有全量编译验证。风险：少量调用点需要显式 `.into()`；
   阶段 1 先做机械替换，不改语义。

7. **性能数字是微基准，不是保证**。5.0/6.2/46.1/204.2 ns 是本机 release 的
   `Instant` 循环结果，受 CPU、缓存、字符串长度影响。风险：不要把它写进性能预算；
   需要时用 `--release` 的 gallery 帧剖析（`AGENTS.md` 命令表）复核。

8. **Fluent 0.16 的维护节奏**。`fluent-bundle 0.16.0` 发布于 2025-05-22，之后没有新版本；
   `DATETIME` issue 仍 open。风险：如果项目需要 ICU4X 原生集成，可能要自己维护
   formatter，或转向 `i18n-embed`/`fluent-templates` 的上层封装。本文的选择是
   “低层可控 + 自己缓存”，代价是要自己补数字/日期。

9. **`ui-gallery` 是示例，不是产品**。给 gallery 做完整翻译（265 条）成本不低；
   但 `AGENTS.md §6.5` 没有给示例开例外。风险：如果决定“gallery 只用英文”，必须在
   `AGENTS.md` 或 `docs/control-completion-progress.md` 明确写例外，否则 §6.4 的检查
   会一直红。

10. **`[未证实]` 平台 locale 探测**。`sys-locale 0.3.2` 在 Wayland/Flatpak 下的行为没有
    在本仓库验证；本方案把它放在 app 层，`ui-i18n` 不依赖它。风险：宿主可能拿到空值，
    需要默认 `en` 与显式 CLI 覆盖。

---

## 附录 A：探针与命令

### A.1 版本与 API 核对

```bash
# crates.io 版本（2026-10-31）
for c in fluent-bundle fluent-syntax fluent-templates unic-langid fluent-langneg \
         intl-memoizer intl_pluralrules fluent-pseudo; do
  curl -s -A 'research-bot/1.0' "https://crates.io/api/v1/crates/$c" \
    | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["crate"]["name"], d["crate"]["max_version"], d["crate"]["updated_at"])'
done
```

关键结果：

| crate | 版本 | 备注 |
|---|---|---|
| `fluent-bundle` | 0.16.0 | features 只有 `default`/`all-benchmarks`；无 `intl` |
| `fluent-syntax` | 0.12.0 | `json`/`serde` 可选 |
| `fluent-templates` | 0.15.1 | 依赖 `fluent-bundle 0.16`、`fluent-syntax 0.12`、`fluent-langneg 0.13` |
| `unic-langid` | 0.9.6 | `macros`/`likelysubtags`/`serde` |
| `fluent-langneg` | 0.13.1 | 依赖 `unic-langid 0.9`；0.14.2 改用 `icu_locid` |
| `fluent-pseudo` | 0.3.3 | 可选测试 |
| `icu` / `icu_decimal` / `icu_datetime` | 2.4.0 | phase 3 |

### A.2 `Send + Sync` 断言

```rust
static_assertions::assert_impl_all!(
    fluent_bundle::concurrent::FluentBundle<FluentResource>: Send, Sync
);
static_assertions::assert_not_impl_any!(
    fluent_bundle::FluentBundle<FluentResource>: Sync
);
```

### A.3 复现 §2.4 的基准

```rust
let ftl = "static = Hello\nwitharg = Hello, { $name }!\n";
let res = FluentResource::try_new(ftl.to_string()).unwrap();
let mut b: fluent_bundle::concurrent::FluentBundle<FluentResource> =
    fluent_bundle::concurrent::FluentBundle::new_concurrent(vec![langid!("en")]);
b.add_resource(res).unwrap();
b.add_builtins().unwrap();

let n = 5_000_000;
let msg = b.get_message("static").unwrap();
let pat = msg.value().unwrap();
let t = Instant::now();
for _ in 0..n { let _ = b.format_pattern(pat, None, &mut vec![]); }
println!("static: {:.1} ns/op", t.elapsed().as_nanos() as f64 / n as f64);
```

### A.4 复现 §5.1 的统计

```bash
cd ui
# 原始字面量
for d in ui-widgets ui-widgets-md3 ui-widgets-fluent ui-widgets-glass ui-gallery ui-core ui-style; do
  printf '%s: ' "$d"
  grep -rhoP '"(?:\\.|[^"\\])*"' crates/$d/src crates/$d/examples 2>/dev/null | wc -l
done
```

UI 文案的启发式统计脚本（保存为 `scripts/check-i18n-literals.py` 之前的原型）：

```python
import re, pathlib
pat = re.compile(r'"((?:\\.|[^"\\])*)"')
skip = ['Id::of','Id::str','.with(','format!','assert','expect(','panic!',
        '#[','use ','Page::','tag()','env::var','std::env','//',
        'icon_codepoints','font =','role_from','ROLE_NAMES']
def uiish(s):
    if not s or '{' in s or '}' in s: return False
    if re.fullmatch(r'[a-z0-9_./:-]+', s): return False
    if re.fullmatch(r'[A-Z_]+', s): return False
    if s in ('md3','fluent','glass','body','note','row','cap','col',
             'label','textSelection','supporting','overline','leading','trailing'):
        return False
    if 'Google Sans' in s or 'Material Symbols' in s: return False
    return any(c.isalpha() for c in s) and (
        s[0].isupper() or ' ' in s or any(ord(c) > 127 for c in s))

root = pathlib.Path('crates/ui-gallery/examples/gallery')
total = 0
for p in root.rglob('*.rs'):
    if '/selftest/' in str(p) or p.name == 'main.rs':
        continue
    for line in p.read_text().splitlines():
        code = line.split('//')[0]
        if any(x in code for x in skip):
            continue
        total += sum(1 for m in pat.finditer(code) if uiish(m.group(1)))
print(total)  # 265
```

### A.5 官方文档与源码

- Fluent 语法：
  - [Selectors](https://projectfluent.org/fluent/guide/selectors.html)（复数类别、默认变体）
  - [Terms](https://projectfluent.org/fluent/guide/terms.html)（`-term`，不能被运行时直接取）
  - [Functions](https://projectfluent.org/fluent/guide/functions.html)（`NUMBER`/`DATETIME`）
  - [EBNF](https://github.com/projectfluent/fluent/blob/master/spec/fluent.ebnf)（Identifier 不允许点）
- `fluent-bundle 0.16.0` 源码：
  - `src/bundle.rs:402` `get_message`
  - `src/bundle.rs:485` `format_pattern`
  - `src/bundle.rs:599-604` `add_builtins` / `TODO: DATETIME()`
  - `src/concurrent.rs:32` `new_concurrent`
  - `src/types/number.rs:148-156` `FluentNumber::as_string`
  - `src/resolver/pattern.rs:94-96` 单 `TextElement` 返回借用值
- `fluent-bundle` issue [#181](https://github.com/projectfluent/fluent-rs/issues/181)（DATETIME 未实现）
- `fluent-langneg 0.13.1` `Cargo.toml:65`（`unic-langid 0.9`）
- `fluent-templates 0.15.1` `src/loader.rs:33`（`lookup -> String`）
- `i18n-embed 0.16.0` docs.rs（`FluentLanguageLoader::get -> String`，`desktop-requester` 用 `sys-locale`）
- 参考框架：
  - [GTK 4 `Gtk.Builder`](https://docs.gtk.org/gtk4/class.Builder.html)：`translatable="yes"`、
    `translation-domain`、`gettext`/`dgettext`，`xgettext` 提取
  - [Qt 6 `QObject::tr()`](https://doc.qt.io/qt-6/qobject.html#tr)：`tr(sourceText,
    disambiguation, n)`；[`QCoreApplication::translate()`](https://doc.qt.io/qt-6/qcoreapplication.html#translate)
    的 `n` 用于 `%n` 复数；`QTranslator` 加载 `.qm`
  - [Flutter internationalization](https://docs.flutter.dev/ui/accessibility-and-internationalization/internationalization)：
    `intl` + ARB + `flutter gen-l10n` + `AppLocalizations`，支持 placeholders/plurals/selects
  - egui 无内置 i18n；社区 `egui-i18n` 0.2.0（[docs.rs](https://docs.rs/egui-i18n/latest/egui_i18n/)）
  - GPUI 无第一方 i18n；Zed 的 [i18n RFC](https://github.com/zed-industries/zed/discussions/43592)
    提议 `fluent-bundle`；社区 `gpui-es-fluent`（[docs.rs](https://docs.rs/gpui-es-fluent/)）
