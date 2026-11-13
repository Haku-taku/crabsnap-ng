# ADR: 插入符与字面选择都以字簇为单位，呈现形式按 UTS #51 判定

状态：**已采纳并实现**（2026-11-04）。代码在 `ui/crates/ui-core/src/grapheme.rs`、
`ui/crates/ui-core/src/text_input.rs`、`ui/crates/ui-text/src/emoji.rs`、
`ui/crates/ui-platform/src/text_pass.rs` 的 `shape_run_parts` / `shape_covering_split`。

## 旧做法与问题

三处都按 `char`（码点）走：

1. **插入符运算**：方向键、退格、删除、双击选词都按码点步进。`3️⃣` 是三个码点，
   插入符能停在基字符与标记之间；`👨‍👩‍👧` 能停在 ZWJ 上。
2. **字面选择**：`shape_covering_split` 按码点把一段切给不同字面。键帽的 `3` 被文本字面
   认领，`FE0F`/`20E3` 落到别处，键帽于是只剩一个裸数字。
3. **整段一个 emoji 门**：只要段里出现 `FE0F`/`20E3` 就整段优先试 emoji 字面。emoji 字面的
   数字只是**键帽部件**（空字形、非零 glyph id），覆盖率检查因此通过 —— `3️⃣4` 整段交给
   emoji 字面，`4` 画成空白，用户看到「`3️⃣` 之后的数字消失」。

## 参考怎么做

- **UAX #29 字簇**是用户感知的「一个字符」，也是成熟引擎的插入符单位（Flutter
  `TextEditingValue`、GPUI、egui 的 `Galley` 光标都是簇级）。
- **UTS #51 呈现形式**逐簇判定。Parley 的 `parley_emoji`（改编自 c-emoji，Blink 的
  `EmojiSegmentation` 同源）就是一个状态机：喂入每个码点的 `Emoji`/`Emoji_Presentation`/
  `Emoji_Modifier`/`Emoji_Modifier_Base`/`Regional_Indicator` 位，输出 `Text` 或 `Emoji`。
- **字面按簇选择**：Chromium/Parley 都先按「簇的呈现形式 + 字面覆盖」切 run，再整形。

## 改用什么

- `ui_core::grapheme`：`clusters` / `snap` / `ceil` / `next` / `prev`，底层
  `unicode-segmentation`。`text_input` 的 `clamp`/`moved`/`backspace`/`delete`/`word_at`/
  `step_word` 全部改为簇级；`index_at` 把命中的字节退回簇边界。
- `ui_text::emoji`：`presentation`（`parley_emoji::EmojiDFA` + `unicode-properties` 的属性位）、
  `asks_emoji`、`uniform_presentation`。空段返回 `None`（无簇可一致）。
- `shape_run_parts`：段的字簇呈现一致时才允许单个字面接管（`shape_covering`），混合则直接走
  `shape_covering_split`，逐簇用对应的候选表挑字面。
- `face_candidates`：记住的字面只回答**同类呈现**的段 —— emoji 字面不得接管旁边的普通字簇。
- 断行：`split_by_cluster_with` 与省略号截断按簇切；一个比行宽还宽的簇独占一行，不切开。
- IME `DeleteSurrounding`：协议给的是**字节**数（`text-input-unstable-v3.xml`），旧实现当成
  字符数用 `moved` 换算。改为字节区间，起点向簇首退、终点向簇尾扩。

## 哪条检查抓回归

- `ui_core::grapheme::a_cluster_is_one_step_for_the_caret`（十个码点四个簇）
- `ui_core::text_input::an_arrow_step_crosses_a_keycap_as_one_character`、`a_delete_removes_whole_clusters_only`
- `ui_text::emoji::only_clusters_with_an_emoji_presentation_ask_for_emoji`（`❤` 是文本、`❤️` 是 emoji）
- `ui_platform::text_pass::a_keycap_sequence_shapes_as_one_colour_glyph`、
  `a_digit_after_a_keycap_keeps_its_own_glyph`（后者先红后绿：修前 `3️⃣4` 只有一个 Color 字形）
- 手工：`GALLERY_PAGE=text-field GALLERY_FOCUS=500,165 GALLERY_TYPE="3️⃣3️⃣4"`
  配 `GALLERY_KEY=home,right`（插入符停在两个键帽之间）与 `home,delete`（整簇删掉，不留悬空标记）。
