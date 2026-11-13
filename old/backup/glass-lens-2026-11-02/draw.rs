//! Draw primitives, the order log, and batch planning.
//! 绘制图元、顺序日志与批次计划。
//!
//! Order and data are stored separately, so a backend can upload a whole typed
//! slice at once. A single `enum Cmd` would force a match per command.
//! 顺序与数据分开存储，后端可整段上传同类型数据。单一 `enum Cmd` 会逼出逐条 match。

use crate::geom::{Color, Corners, Insets, Rect};
use crate::id::{Id, NodeId};

/// Direction a linear gradient runs in.
/// 线性渐变的走向。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientAxis {
    /// Top to bottom.
    /// 自上而下。
    Vertical,
    /// Left to right.
    /// 自左而右。
    Horizontal,
}

/// A two-stop linear gradient in the primitive's own rect.
/// 图元自身矩形内的两段线性渐变。
///
/// XAML states the stops separately from the brush's extent, and WinUI's elevation borders
/// use both: a 3-unit run at offset 0.33 rather than a ramp across the whole control.
/// XAML 把停点与画刷的跨度分开写，WinUI 的立面描边两者都用：3 个单位、起点 0.33，
/// 而不是横跨整个控件的斜坡。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    pub from: Color,
    pub to: Color,
    pub axis: GradientAxis,
    /// Where the near and far stops sit in the gradient's own 0..1 space.
    /// 近端与远端停点在渐变自身 0..1 空间里的位置。
    pub stops: (f32, f32),
    /// `None` spans the whole box; `Some(len)` covers only the first `len` units on the axis.
    /// `None` 横跨整个盒子；`Some(len)` 只覆盖轴上头 `len` 个单位。
    pub extent: Option<f32>,
    /// Measure from the far edge inward: WinUI's light theme mirrors its elevation borders.
    /// 从远端向内度量：WinUI 的明色主题把立面描边镜像过来。
    pub flip: bool,
}

impl Gradient {
    /// A full-box gradient, near stop at the start and far stop at the end.
    /// 横跨整个盒子的渐变，近端在起点、远端在终点。
    pub const fn new(from: Color, to: Color, axis: GradientAxis) -> Self {
        Self { from, to, axis, stops: (0.0, 1.0), extent: None, flip: false }
    }

    /// Anchor the run at the box's far edge instead of its near edge.
    /// 把渐变锚在盒子的远端而不是近端。
    pub const fn flipped(mut self) -> Self {
        self.flip = true;
        self
    }

    /// Move the two stops inside the gradient's own space.
    /// 把两个停点移到渐变自身空间里的位置。
    pub const fn with_stops(mut self, near: f32, far: f32) -> Self {
        self.stops = (near, far);
        self
    }

    /// Cover only the first `len` units along the axis, holding the far colour after.
    /// 只覆盖轴上头 `len` 个单位，其后保持远端色。
    pub const fn over(mut self, len: f32) -> Self {
        self.extent = Some(len);
        self
    }

    /// Colour at `u`, the gradient's own 0..=1 coordinate.
    /// 渐变自身 0..=1 坐标 `u` 处的颜色。
    pub fn at(&self, u: f32) -> Color {
        let (near, far) = self.stops;
        let span = if far > near { far - near } else { 1.0 };
        self.from.lerp(self.to, ((u - near) / span).clamp(0.0, 1.0))
    }

    /// Colour at `pos` units along the axis, in a box `extent` units long.
    /// 轴上 `pos` 处的颜色，盒子在该轴上的长度为 `extent`。
    pub fn sample(&self, pos: f32, extent: f32) -> Color {
        // A flip mirrors the whole box before the run is measured into it.
        // 翻转先把整个盒子镜像，再在其中度量渐变。
        let pos = if self.flip { extent - pos } else { pos };
        let len = self.extent.unwrap_or(extent);
        let u = if len > 0.0 { pos / len } else { 0.0 };
        self.at(u)
    }
}

/// How far a press splat grows, and whether it drifts to the node's centre.
/// 按压墨迹扩到多大，以及是否漂移到节点中心。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkRadius {
    /// A fixed radius around the press point; the splat never drifts.
    /// 绕按下点的固定半径；墨迹不漂移。
    Fixed(f32),
    /// Covers the node: the splat drifts to the centre and grows to half the diagonal.
    /// 盖住整个节点：墨迹漂到中心，半径长到对角线的一半。
    Cover,
}

/// Where a ripple takes its rect and curve from: its effective background.
/// 涟漪从哪取矩形与曲线：它的有效背景。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkTarget {
    /// The node's own box and its declared (or inferred) curve.
    /// 节点自身的盒子与声明（或推断）的曲线。
    Node,
    /// An explicit rect and curve in the node's own space: the control's effective
    /// background, whether or not this frame paints one.
    /// 节点自身空间中的显式矩形与曲线：控件的有效背景，与本帧是否画了背景无关。
    Rect { rect: crate::geom::Rect, corners: crate::geom::Corners },
    /// The nth shape the node paints, in declaration order: that shape's rect and curve.
    /// 节点按声明顺序绘制的第 n 个形状：它的矩形与曲线。
    Shape(usize),
    /// A descendant node's box and curve, as Flutter's `getRectCallback`.
    /// 后代节点的盒子与曲线（Flutter 的 `getRectCallback`）。
    Descendant(Id),
}

/// Press ink the engine splats where a press lands and fades out after release.
/// 引擎在按下落点泼出、松手后淡出的按压墨迹。
///
/// The engine owns the splat's origin and lifetime: `Widget::decl` is a pure function and
/// cannot hold the press point across frames, so a control only states colour and size.
/// 落点与寿命由引擎持有：`Widget::decl` 是纯函数，留不住跨帧的按下点，控件只声明颜色与大小。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressInk {
    /// Ink colour; its alpha is the strength the state layer would have.
    /// 墨色；alpha 即状态层应有的强度。
    pub colour: Color,
    pub radius: InkRadius,
    /// The ink's effective background; the ripple sizes to it and is clipped to it.
    /// 墨迹的有效背景；涟漪以它为尺寸与裁剪基准。
    pub target: InkTarget,
    /// Opacity ramp while held; Flutter's `_kFadeInDuration`.
    /// 按住期间的透明度爬升；Flutter 的 `_kFadeInDuration`。
    pub fade_in: crate::anim::AnimSpec,
    /// Radius and drift; Flutter's `_kRadiusDuration`.
    /// 半径与漂移；Flutter 的 `_kRadiusDuration`。
    pub grow: crate::anim::AnimSpec,
    /// Fade after the press ends.
    /// 按下结束后的淡出。
    pub fade: crate::anim::AnimSpec,
}

impl PressInk {
    /// Flutter `InkRipple`: 75ms fade-in, 225ms radius on `Curves.ease`, then 375ms from the
    /// release with the fade in its last 150ms — a quick tap still shows the whole splash.
    /// Flutter `InkRipple`：75ms 淡入、225ms 半径（`Curves.ease`）；松手后 375ms，
    /// 淡出占最后 150ms——轻点一下也看得到完整墨迹。
    pub const fn new(colour: Color, radius: InkRadius) -> Self {
        Self::with_target(colour, radius, InkTarget::Node)
    }

    /// Ink sized to and clipped by `target`, its effective background.
    /// 涟漪以 `target`（有效背景）为尺寸与裁剪基准。
    pub const fn with_target(colour: Color, radius: InkRadius, target: InkTarget) -> Self {
        use crate::anim::{AnimSpec, Easing};
        Self {
            colour,
            radius,
            target,
            fade_in: AnimSpec::new(0.075, Easing::Linear),
            grow: AnimSpec::new(0.225, Easing::Cubic(0.25, 0.1, 0.25, 1.0)),
            fade: AnimSpec::new(0.15, Easing::Linear).delayed(0.225),
        }
    }
}

/// One filled (optionally rounded) box.
/// 一个填充（可圆角）盒。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectPrim {
    pub rect: Rect,
    pub radius: Corners,
    /// Solid colour; also the fallback where `gradient` is unsupported.
    /// 实色；同时也是 `gradient` 不被支持时的回落色。
    pub fill: Color,
    /// Optional gradient across `rect`; backends that cannot draw it use `fill`.
    /// 可选的、横跨 `rect` 的渐变；画不了的后端使用 `fill`。
    pub gradient: Option<Gradient>,
    pub border: Option<(Color, f32)>,
    /// A gradient across the stroke; `border` stays the near-stop fallback.
    /// 沿描边的渐变；`border` 仍是近端色回落。
    pub border_gradient: Option<Gradient>,
    /// Omit the border over this x-range of its top edge, measured from `rect`'s left.
    /// 在顶边这一段 x 区间（自 `rect` 左缘量起）不画描边。
    ///
    /// An outlined field's label cuts this gap; it is a property of the stroke, so it
    /// lives on the primitive rather than in a clip.
    /// 带描边字段的标签切出这个缺口；它是描边的属性，因此落在图元上而不是裁剪里。
    pub notch: Option<(f32, f32)>,
}

/// One stroked circular arc, with its bounding box for clipping and damage.
/// 一段圆形描边弧，附带用于裁剪与脏区的包围盒。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorPrim {
    pub bounds: Rect,
    /// Stroke colour.
    /// 描边颜色。
    pub colour: Color,
    pub sector: Sector,
}

/// A stroked circular arc, measured in turns clockwise from 3 o'clock.
/// 一段圆形描边弧，以“圈”计，从三点钟方向顺时针量起。
///
/// Turns rather than radians because the reference (Lottie's `TrimEnd`) expresses arcs
/// that way; a quarter turn is `0.25`, a full ring `0.0..=1.0`.
/// 用“圈”而非弧度，因为参考实现（Lottie 的 `TrimEnd`）就是这么表达弧的；
/// 四分之一圈是 `0.25`，整圈是 `0.0..=1.0`。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector {
    /// Centre, relative to `rect`'s origin.
    /// 圆心，相对 `rect` 原点。
    pub centre: crate::geom::Point,
    /// Radius of the stroke's outer edge.
    /// 描边外缘的半径。
    pub radius: f32,
    /// Stroke width, growing inwards.
    /// 描边宽度，向内生长。
    pub thickness: f32,
    /// Start and end in turns; the sweep runs clockwise from start to end.
    /// 起止位置（圈）；扫动自起点顺时针到终点。
    pub trim: (f32, f32),
    /// Round caps at both ends, as WinUI's `StrokeDashCap="Round"` draws them.
    /// 两端圆头帽，与 WinUI 的 `StrokeDashCap="Round"` 一致。
    pub round_caps: bool,
}

impl Sector {
    /// The sweep in turns; a span of one full turn or more is the whole ring.
    /// 扫动量（圈）；跨度达到一整圈或更多即为整环。
    ///
    /// `rem_euclid` alone would fold a full turn back to zero, which is how a 100% ring
    /// would vanish.
    /// 只用 `rem_euclid` 会把一整圈折回零，100% 的环便会消失。
    pub fn sweep(&self) -> f32 {
        let span = self.trim.1 - self.trim.0;
        if span >= 1.0 {
            1.0
        } else {
            span.rem_euclid(1.0)
        }
    }

    /// The bounding box of the stroke, in the same space as the primitive's `rect`.
    /// 描边包围盒，与图元的 `rect` 同一坐标系。
    ///
    /// Damage and the vertex quad both round this out to the pixel grid, so the feather's
    /// half pixel is covered without the box carrying a margin of its own.
    /// 脏区与顶点四边形都把它按像素网格向外取整，羽化的半个像素因此被覆盖，
    /// 盒子本身无需再留余量。
    pub fn bounds(&self) -> Rect {
        let r = self.radius;
        Rect::new(self.centre.x - r, self.centre.y - r, 2.0 * r, 2.0 * r)
    }
}

impl RectPrim {
    /// Colour at a point: the gradient if present, else the solid fill.
    /// 某点处的颜色：有渐变用渐变，否则用实色。
    pub fn colour_at(&self, px: f32, py: f32) -> Color {
        match &self.gradient {
            Some(g) => {
                let (pos, extent) = match g.axis {
                    GradientAxis::Vertical => (py - self.rect.origin.y, self.rect.size.h),
                    GradientAxis::Horizontal => (px - self.rect.origin.x, self.rect.size.w),
                };
                g.sample(pos, extent)
            }
            None => self.fill,
        }
    }

    /// The stroke's colour at a point: its gradient if present, else the solid.
    /// 描边在某点处的颜色：有渐变用渐变，否则用实色。
    pub fn border_colour_at(&self, px: f32, py: f32) -> Color {
        let Some((solid, _)) = self.border else {
            return Color::TRANSPARENT;
        };
        match &self.border_gradient {
            Some(g) => {
                let (pos, extent) = match g.axis {
                    GradientAxis::Vertical => (py - self.rect.origin.y, self.rect.size.h),
                    GradientAxis::Horizontal => (px - self.rect.origin.x, self.rect.size.w),
                };
                g.sample(pos, extent)
            }
            None => solid,
        }
    }

    /// Whether a point, relative to the rect's origin, falls in the stroke's top-edge gap.
    /// 某点（相对矩形原点）是否落在描边顶边的缺口里。
    ///
    /// The band is the stroke plus half a pixel: the antialiased skirt reaches past the
    /// nominal width, and leaving it behind draws a hairline under a floating label.
    /// 带为描边宽度再加半像素：抗锯齿边缘会越过名义宽度，留下它会在浮起标签下画出一条细线。
    pub fn in_notch(&self, x: f32, y: f32, width: f32, skirt: f32) -> bool {
        let Some((from, to)) = self.notch else {
            return false;
        };
        x >= from && x <= to && y <= width + 0.5 + skirt.max(0.0)
    }
}

/// Build a flat draw list from a laid-out tree: backgrounds then text, in tree order.
/// 从已布局的树生成扁平绘制列表：按树序先背景后文本。
///
/// Every primitive is placed at `visual_rect`, never `rect`: an offset lives only in the
/// visual rect, so painting the layout rect pins every offset child to its parent origin.
/// 每个图元都按 `visual_rect` 放置，绝不用 `rect`：偏移只存在于视觉矩形里，用布局矩形会把
/// 每个带偏移的子节点钉在父原点上。
pub fn list_from_tree(tree: &crate::tree::Tree, viewport: Rect) -> DrawList {
    let mut list = DrawList::new(viewport);
    for nid in tree.walk() {
        let Some(node) = tree.get(nid) else { continue };
        let visual = node.visual_rect();
        push_shapes(&mut list, nid, node, visual, |_, _| None);
        if let crate::tree::NodeData::Text {
            block,
            ink,
            caret,
            selection,
            composing,
            ..
        } = &node.data
        {
            list.push_text(
                nid,
                TextPrim {
                    block: block.clone(),
                    origin: visual.origin,
                    max_width: visual.size.w,
                    layout: node.inline.clone(),
                    height: visual.size.h,
                    ink: *ink,
                    caret: *caret,
                    selection: selection.clone(),
                    composing: composing.clone(),
                },
            );
        }
    }
    list
}

/// Emit one node's shapes in paint order; `image` resolves a texture slot for box images.
/// 按绘制顺序发出一个节点的形状；`image` 为盒子图像解析纹理槽位。
pub fn push_shapes(
    list: &mut DrawList,
    owner: NodeId,
    node: &crate::tree::Node,
    visual: Rect,
    mut image: impl FnMut(&crate::tree::ImageSpec, Rect) -> Option<ImagePrim>,
) {
    for shape in &node.shapes {
        match shape {
            Shape::Box {
                corners,
                fill,
                gradient,
                border,
                shadow,
                image: spec,
            } => {
                if let Some(sh) = shadow {
                    list.push_shadow(
                        owner,
                        ShadowPrim {
                            rect: visual,
                            corners: *corners,
                            colour: crate::geom::Color::rgba(0.0, 0.0, 0.0, sh.alpha),
                            blur: sh.blur,
                            dy: sh.dy,
                        },
                    );
                }
                list.push_rect(
                    owner,
                    RectPrim {
                        rect: visual,
                        radius: *corners,
                        fill: *fill,
                        gradient: *gradient,
                        border: *border,
                        border_gradient: None,
                        notch: None,
                    },
                );
                if let Some(spec) = spec {
                    if let Some(prim) = image(spec, visual) {
                        list.push_image_prim(owner, prim);
                    }
                }
            }
            Shape::Rect {
                rect,
                corners,
                fill,
                gradient,
                border,
            } => {
                list.push_rect(
                    owner,
                    RectPrim {
                        rect: rect.translate(visual.origin),
                        radius: *corners,
                        fill: *fill,
                        gradient: *gradient,
                        border: *border,
                        border_gradient: None,
                        notch: None,
                    },
                );
            }
            Shape::Shadow {
                rect,
                corners,
                colour,
                blur,
                dy,
            } => {
                list.push_shadow(
                    owner,
                    ShadowPrim {
                        rect: rect.translate(visual.origin),
                        corners: *corners,
                        colour: *colour,
                        blur: *blur,
                        dy: *dy,
                    },
                );
            }
            Shape::Outline {
                corners,
                colour,
                gradient,
                width,
                gap,
            } => {
                list.push_rect(
                    owner,
                    RectPrim {
                        rect: visual,
                        radius: *corners,
                        fill: Color::TRANSPARENT,
                        gradient: None,
                        border: Some((*colour, *width)),
                        border_gradient: *gradient,
                        notch: *gap,
                    },
                );
            }
            Shape::Sector {
                centre,
                radius,
                thickness,
                trim,
                round_caps,
                colour,
            } => {
                let sector = Sector {
                    centre: crate::geom::Point::new(
                        visual.origin.x + centre.x,
                        visual.origin.y + centre.y,
                    ),
                    radius: *radius,
                    thickness: *thickness,
                    trim: *trim,
                    round_caps: *round_caps,
                };
                list.push_sector(
                    owner,
                    SectorPrim {
                        bounds: sector.bounds(),
                        colour: *colour,
                        sector,
                    },
                );
            }
        }
    }
}

/// The focus adorner a node asks the engine to draw around itself.
/// 节点要求引擎在其周围绘制的焦点装饰。
///
/// It is a *post-layout* drawing: the rect is only known after layout, and the adorner
/// deliberately extends past the node's box (`margin` is negative, as WinUI writes it).
/// 它是**布局之后**的绘制：矩形只有在布局完成才确定，而装饰刻意伸出节点盒子
/// （`margin` 取负，与 WinUI 的写法一致）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusVisual {
    /// The outer stroke and its width. WinUI: `FocusVisualPrimaryBrush`, 2 px.
    /// 外圈颜色与宽度。WinUI：`FocusVisualPrimaryBrush`，2 像素。
    pub outer: Color,
    pub outer_width: f32,
    /// The inner stroke drawn just inside the outer one. WinUI: `FocusVisualSecondaryBrush`, 1 px.
    /// 紧贴外圈内侧画的内圈。WinUI：`FocusVisualSecondaryBrush`，1 像素。
    pub inner: Color,
    pub inner_width: f32,
    /// How far the adorner reaches outside the node on each side; negative, as XAML writes it.
    /// 装饰每边伸出节点的距离；取负，与 XAML 写法一致。
    pub margin: crate::geom::Insets,
    /// Extra radius over the node's own, so the two curves stay concentric.
    /// 在节点自身圆角上加的量，使两条曲线同心。
    pub radius: f32,
}

/// A paint-only shape in the node's own space: no identity, no layout slot, no hit target.
/// 节点自身坐标系中的纯绘制形状：无身份、不占布局槽位、不可命中。
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// The node's own box: its fill, gradient, border, shadow and image.
    /// 节点自身的盒子：填充、渐变、描边、阴影与图像。
    Box {
        corners: Corners,
        fill: Color,
        gradient: Option<Gradient>,
        border: Option<(Color, f32)>,
        shadow: Option<crate::tree::ShadowSpec>,
        image: Option<crate::tree::ImageSpec>,
    },
    /// An explicit rect: circles, pills and thumbnails inside the node's box.
    /// 显式矩形：节点盒子内的圆、胶囊与缩略图。
    Rect {
        rect: Rect,
        corners: Corners,
        fill: Color,
        gradient: Option<Gradient>,
        border: Option<(Color, f32)>,
    },
    /// A soft shadow under an explicit rect.
    /// 显式矩形下的柔和阴影。
    Shadow {
        rect: Rect,
        corners: Corners,
        colour: Color,
        blur: f32,
        dy: f32,
    },
    /// The node's own box drawn as a border only, its top edge interrupted over `gap`:
    /// an outlined field's label notch, measured from the node's left edge.
    /// 节点自身盒子只画描边，顶边在 `gap` 这一段中断：带描边字段的标签缺口，自节点左缘量起。
    Outline {
        corners: Corners,
        colour: Color,
        /// A gradient along the stroke; `colour` stays the near-stop fallback.
        /// 沿描边的渐变；`colour` 仍是近端色回落。
        gradient: Option<Gradient>,
        width: f32,
        gap: Option<(f32, f32)>,
    },
    /// A stroked circular arc: rings and progress sweeps.
    /// 一段圆形描边弧：圆环与进度扫动。
    Sector {
        /// Centre in the node's own space.
        /// 圆心，位于节点自身坐标系。
        centre: crate::geom::Point,
        radius: f32,
        thickness: f32,
        trim: (f32, f32),
        round_caps: bool,
        colour: Color,
    },
}

impl Shape {
    /// The area this shape can touch, given the box of the node that owns it.
    /// 该形状可能触及的区域，参数为其所属节点的盒子。
    pub fn bounds(&self, node: Rect) -> Rect {
        match self {
            Shape::Box { shadow, .. } => match shadow {
                // The shadow fades over roughly `2 * blur` and sits `dy` lower.
                // 阴影大约在 `2 × blur` 内淡出，并下沉 `dy`。
                Some(s) => node.inflate(s.blur).translate(crate::geom::Point::new(0.0, s.dy)),
                None => node,
            },
            // Sub-shapes are declared in the node's own space, so lift them into absolute.
            // 子形状声明在节点自身坐标系，因此需要抬到绝对坐标。
            Shape::Rect { rect, .. } => rect.translate(node.origin),
            Shape::Shadow { rect, blur, dy, .. } => rect
                .inflate(*blur)
                .translate(node.origin)
                .translate(crate::geom::Point::new(0.0, *dy)),
            Shape::Outline { .. } => node,
            // The arc is declared in the node's space too.
            // 弧同样声明在节点自身坐标系。
            Shape::Sector {
                centre,
                radius,
                ..
            } => Rect::new(
                node.origin.x + centre.x - radius,
                node.origin.y + centre.y - radius,
                2.0 * radius,
                2.0 * radius,
            ),
        }
    }
}

/// One run of text.
/// 一段文字。
#[derive(Clone, Debug, PartialEq)]
pub struct TextPrim {
    /// Styled runs plus paragraph properties; rich text adds runs here.
    /// 带样式的 run 与段落属性；富文本在这里增加 run。
    pub block: crate::text::TextBlock,
    pub origin: crate::geom::Point,
    /// Wrap width in logical units; `f32::INFINITY` means one line.
    /// 换行宽度（逻辑单位）；`f32::INFINITY` 表示不换行。
    pub max_width: f32,
    /// The lines layout wrapped this block into; `None` makes the painter wrap it itself.
    /// 布局把本块断成的行；`None` 时由绘制端自行断行。
    pub layout: Option<std::rc::Rc<crate::text::InlineLayout>>,
    /// The node's own box height; `0.0` when the caller does not know it.
    /// 节点自身的盒高；调用方不知道时为 `0.0`。
    ///
    /// A symbol is centred by its ink inside this box; text ignores it.
    /// 符号按墨在此盒中居中；文字忽略它。
    pub height: f32,
    /// Ink this text is drawn in; the component picks the role, the type ramp never does.
    /// 绘制这段文字的墨色；角色由控件选，字阶从不决定它。
    pub ink: Color,
    /// Draw the insertion caret at this byte offset.
    /// 在该字节偏移处绘制插入符。
    pub caret: Option<Caret>,
    /// Selected bytes and how this style draws them; `None` when nothing is selected.
    /// 被选中的字节以及本风格如何绘制它；没有选区时为 `None`。
    pub selection: Option<crate::selection::SelectionPrim>,
    /// Bytes the input method is composing; drawn with an underline.
    /// 输入法正在组合的字节；带下划线绘制。
    pub composing: Option<std::ops::Range<usize>>,
}

/// A text input's insertion caret: where it sits and what colour it takes.
/// 文本输入的插入符：位置与其颜色。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub offset: usize,
    pub color: Color,
}

/// Which atlas a glyph primitive samples.
/// 字形图元采样哪张图集。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphKind {
    /// One coverage byte per pixel, tinted by `GlyphPrim::color`.
    /// 每像素一字节覆盖率，由 `GlyphPrim::color` 着色。
    #[default]
    Coverage,
    /// Premultiplied RGBA per pixel, used as-is; `color` only scales alpha.
    /// 每像素预乘 RGBA，直接使用；`color` 只缩放 alpha。
    Color,
}

/// One pre-rasterized glyph blitted from an atlas.
/// 从图集贴上的一个已光栅化字形。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphPrim {
    /// Destination rect on screen.
    /// 屏幕上的目标矩形。
    pub rect: Rect,
    /// Atlas slot origin and size.
    /// 图集槽位的原点与尺寸。
    pub atlas: Rect,
    /// Linear multiplier applied to coverage or alpha (0..=1).
    /// 覆盖率或 alpha 上的线性乘子（0..=1）。
    pub alpha: f32,
    pub color: Color,
    /// Whether `atlas` indexes the coverage atlas or the colour atlas.
    /// `atlas` 指向覆盖率图集还是颜色图集。
    pub kind: GlyphKind,
}

/// One RGBA8 texture drawn into a rounded rect, sampling a source sub-rect.
/// 把一张 RGBA8 纹理画进圆角矩形，采样其源子矩形。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImagePrim {
    /// Destination in logical units.
    /// 目标矩形，逻辑单位。
    pub rect: Rect,
    pub corners: Corners,
    /// Index into `DrawList::images`.
    /// `DrawList::images` 的下标。
    pub texture: u32,
    /// Source rect in texture pixels, already resolved by the node's fit.
    /// 源矩形（纹理像素），已由节点的 fit 算好。
    pub src: Rect,
}

/// A soft shadow under a rounded rect. The falloff is analytic, so there is no blur pass.
/// 圆角矩形下的柔和阴影。衰减是解析的，因此不需要模糊趟。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowPrim {
    /// The shape being shadowed; the shadow extends `blur` beyond it.
    /// 被投影的形状；阴影向外延伸 `blur`。
    pub rect: Rect,
    pub corners: Corners,
    /// Colour and peak alpha.
    /// 颜色与峰值 alpha。
    pub colour: Color,
    /// Falloff width in logical units: the shadow fades over roughly `2 * blur`.
    /// 衰减宽度，逻辑单位：阴影大约在 `2 × blur` 内淡出。
    pub blur: f32,
    /// Vertical offset; WinUI's elevation shadows sit slightly below their surface.
    /// 垂直偏移；WinUI 的立面阴影略微落在表面下方。
    pub dy: f32,
}

/// One texture the frame refers to. `id` is stable across frames so a renderer can cache.
/// 本帧引用的一张纹理。`id` 跨帧稳定，渲染器据此缓存。
#[derive(Clone, Debug)]
pub struct ImageEntry {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// RGBA8, straight alpha, row-major.
    /// RGBA8、直通 alpha、行优先。
    pub data: std::sync::Arc<Vec<u8>>,
}

/// Primitive kinds, ordered so that `PrimKind as usize` indexes the bank array.
/// 图元种类；`as usize` 可直接索引各类型数组。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum PrimKind {
    Rect = 0,
    Text = 1,
    Glyph = 2,
    Image = 3,
    Shadow = 4,
    Sector = 5,
}

impl PrimKind {
    pub const COUNT: usize = 6;

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Why a datum is in the list, and where it lives.
/// 一条操作：什么种类、第几个。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimRef {
    pub kind: PrimKind,
    pub index: u32,
}

/// One entry in the paint-order log.
/// 绘制顺序日志中的一条。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// Draw the referenced primitive.
    /// 绘制引用的图元。
    Prim(PrimRef),
    /// Push a clip rect (index into `clips`).
    /// 压入裁剪矩形（`clips` 的下标）。
    PushClip(u32),
    /// Pop the innermost clip.
    /// 弹出最内层裁剪。
    PopClip,
    /// Push an offscreen layer (index into `layers`).
    /// 压入离屏层（`layers` 的下标）。
    PushLayer(u32),
    /// Pop the innermost layer.
    /// 弹出最内层层。
    PopLayer,
    /// Blur whatever is already painted inside a rect (index into `backdrops`).
    /// 模糊矩形内已画好的内容（`backdrops` 的下标）。
    Backdrop(u32),
}

/// WinUI acrylic: a luminosity wash, a colour-blended tint, then a grain layer.
/// WinUI acrylic：先亮度洗色、再色相着色、最后铺一层噪点。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Acrylic {
    /// Tint colour; `a` is `TintOpacity` times the tint-opacity modifier.
    /// 着色色；`a` 是 `TintOpacity` 乘上 tint 不透明度修正系数。
    pub tint: Color,
    /// Luminosity colour; `a` is `TintLuminosityOpacity`.
    /// 亮度色；`a` 是 `TintLuminosityOpacity`。
    pub luminosity: Color,
    /// Grain layer opacity; the reference tiles a 2% noise image here.
    /// 噪点层不透明度；参考实现在这里平铺一张 2% 的噪点图。
    pub noise: f32,
}

/// Glass: the reference's interactive-indicator surface over the backdrop.
/// 玻璃：参考实现交互指示器材质，叠加在背景之上。
///
/// Every field is one of the reference shader's uniforms (`interactive_indicator.frag:40-63`);
/// lengths are already in device pixels, so the renderer needs no scale factor.
/// 每个字段对应参考着色器的一个 uniform（`interactive_indicator.frag:40-63`）；
/// 长度已是设备像素，渲染器不必再换算。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    /// `glassColor`: body wash; `a` mixes in `rgb × 0.2`.
    /// `glassColor`：本体洗色；`a` 用它混入 `rgb × 0.2`。
    pub tint: Color,
    /// `rimThickness`: hairline width, in device pixels.
    /// `rimThickness`：发丝边宽，单位设备像素。
    pub rim_thickness: f32,
    /// `rimSmoothing`: how many pixels the hairline's edge takes to fade.
    /// `rimSmoothing`：发丝边淡出所需的像素数倍率。
    pub rim_smoothing: f32,
    /// `ambientRim`: strength of the ring that hugs the edge, `0..=1`.
    /// `ambientRim`：贴边环光的强度，`0..=1`。
    pub ambient_rim: f32,
    /// `ambientStrength`: flat lift added to the whole body.
    /// `ambientStrength`：整块本体抬亮的常数。
    pub ambient_strength: f32,
    /// `baseAlphaMultiplier`: body opacity away from the edge.
    /// `baseAlphaMultiplier`：远离边缘处的本体不透明度。
    pub base_alpha: f32,
    /// `edgeAlphaMultiplier`: opacity at the edge.
    /// `edgeAlphaMultiplier`：边缘处的不透明度。
    pub edge_alpha: f32,
    /// `lightDirection`: unit vector the rim's key highlight faces.
    /// `lightDirection`：边缘主高光朝向的单位向量。
    pub light_direction: [f32; 2],
    /// `lightIntensity`: strength of the key and kick highlights.
    /// `lightIntensity`：主高光与反光高光的强度。
    pub light_intensity: f32,
    /// `saturation`: multiplier applied to the refracted backdrop.
    /// `saturation`：折射背景的饱和度乘数。
    pub saturation: f32,
    /// `glowIntensity`: strength of the Fresnel body glow.
    /// `glowIntensity`：菲涅尔本体辉光的强度。
    pub glow_intensity: f32,
    /// `interactionIntensity`: `0` at rest, `1` fully pressed.
    /// `interactionIntensity`：静息为 `0`，完全按下为 `1`。
    pub interaction: f32,
    /// `chromaticAberration`: per-channel split of the refracted sample.
    /// `chromaticAberration`：折射采样的逐通道色散。
    pub chromatic: f32,
    /// `edgeAbsorption`: Beer-Lambert darkening at the meniscus, `0..=1`.
    /// `edgeAbsorption`：边缘半月板的比尔-朗伯吸收强度，`0..=1`。
    pub edge_absorption: f32,
    /// `edgeZone` in device pixels: how far the edge bend reaches inward.
    /// `edgeZone`（设备像素）：边缘弯折向内的影响距离。
    pub edge_zone: f32,
    /// Which reference material model to run: the flat indicator or the refracting lens.
    /// 走参考的哪套材质：平面指示器，还是真正折射的透镜。
    pub quality: GlassQuality,
    /// `refractiveIndex`: how hard the lens bends the backdrop, 1.0 = no bend.
    /// `refractiveIndex`：透镜弯折背景的强度，1.0 表示不弯。
    pub refractive_index: f32,
    /// `thickness` in device pixels: the bevel's quarter-circle radius.
    /// `thickness`（设备像素）：倒角四分之一圆的半径。
    pub thickness: f32,
    /// `fresnelStrength`: weight of the grazing-angle lift at the rim.
    /// `fresnelStrength`：掠射角处边缘提亮的权重。
    pub fresnel_strength: f32,
    /// `whitenStrength`: veil toward white, 0..=1.
    /// `whitenStrength`：向白色漂洗的强度，0..=1。
    pub whiten: f32,
    /// Which lens the bevel runs: exact Snell, or the paraxial approximation.
    /// 倒角走哪种透镜：精确斯涅尔，还是近轴近似。
    pub lens_model: LensModel,
    /// `rimShade`: the iOS 27 hairline outline's darkening, `0` = off.
    /// `rimShade`：iOS 27 发丝描边的压暗量，`0` 表示不画。
    pub rim_shade: f32,
    /// `rimShadeEnds`: how much of the shade survives at the light axis' ends.
    /// `rimShadeEnds`：光轴两端还保留多少压暗。
    pub rim_shade_ends: f32,
    /// How much the backdrop under the face is shrunk: the capture covers `1 / m` of the
    /// shape's size, so `0.75` shows a 0.75× copy. `1` leaves the face undistorted.
    /// 面下背景缩小多少：捕获范围是形状尺寸的 `1 / m`，故 `0.75` 显示 0.75 倍的画面。
    /// `1` 表示面不变形。
    pub magnification: f32,
    /// Extra shapes fused with the node's own rect, which is always shape zero.
    /// 与节点自身矩形融合的其他形状；节点自身矩形永远是第 0 个。
    pub blend: [BlendShape; BLEND_SHAPES],
    /// How many entries of `blend` are live; `0` shades the node's rect alone.
    /// `blend` 中有效的条目数；`0` 表示只按节点矩形着色。
    pub blend_count: u8,
    /// The smooth-union radius in logical px: how far a neck reaches between shapes.
    /// 平滑并集半径（逻辑像素）：两个形状之间的液桥延伸多远。
    pub blend_k: f32,
}

/// How many extra shapes one glass surface may fuse with.
/// 一块玻璃表面最多能融合多少个额外形状。
pub const BLEND_SHAPES: usize = 4;

/// The bevel's refraction model (`GlassLensModel`).
/// 倒角的折射模型（`GlassLensModel`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LensModel {
    /// Exact Snell refraction through the hemispherical bevel; folds hard at the rim.
    /// 通过半球倒角的精确斯涅尔折射；边缘折叠得很重。
    Spherical,
    /// Small-angle deviation, `(n − 1)` times the slope: the gentle rim band iOS 27 shows.
    /// 小角偏差，`(n − 1)` 乘斜率：iOS 27 那道平缓的边缘带。
    Paraxial,
}

/// One member of a blend group, in the node's own logical coordinates.
/// 融合组的一个成员，坐标在节点自身的逻辑空间。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendShape {
    pub rect: Rect,
    pub corners: Corners,
}

impl BlendShape {
    /// A shape that contributes nothing; the slot's default.
    /// 不参与任何贡献的形状；槽位的默认值。
    pub const NONE: Self = Self {
        rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        corners: Corners::ZERO,
    };
}

/// Which of the reference's two glass materials a surface asks for.
/// 表面要求的是参考的两套玻璃材质中的哪一套。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GlassQuality {
    /// The flat interactive-indicator shader: edge bend + rim, no real refraction.
    /// 平面交互指示器着色器：边缘弯折加描边，没有真正的折射。
    #[default]
    Standard,
    /// The premium two-stage material: a real normal field bending the backdrop.
    /// Premium 两段式材质：真实法线场弯折背景。
    Premium,
}

impl Glass {
    /// The reference's standard path with a captured backdrop behind it
    /// (`glass_effect.dart:541-581`, `interactive_indicator.frag:419-426`).
    /// 参考实现在有截图背景时的 standard 路径（`glass_effect.dart:541-581`、
    /// `interactive_indicator.frag:419-426`）。
    pub const STANDARD: Self = Self {
        tint: Color::rgba(0.88, 0.88, 0.90, 0.12),
        // 0.7 rimThickness × 0.35 standard normalization, in device pixels at scale 1.
        // 0.7 的 rimThickness × 0.35 的 standard 归一化，scale 1 下的设备像素值。
        rim_thickness: 0.245,
        rim_smoothing: 1.5,
        // 0.15 ambientRim × 0.7.
        ambient_rim: 0.105,
        ambient_strength: 0.0,
        // A backdrop makes the body translucent glass rather than the multiplier.
        // 有背景时本体是半透明玻璃，而不是那个乘数。
        base_alpha: 0.70,
        edge_alpha: 0.95,
        // `GlassDefaults.lightAngle` = 0.75π, passed as `[cos, -sin]`.
        // `GlassDefaults.lightAngle` = 0.75π，以 `[cos, -sin]` 传入。
        light_direction: [-0.70710678, -0.70710678],
        light_intensity: 0.0,
        saturation: 1.5,
        // 0.75 glowIntensity × 0.5 standard normalization.
        // 0.75 的 glowIntensity × 0.5 的 standard 归一化。
        glow_intensity: 0.375,
        interaction: 0.0,
        chromatic: 0.01,
        edge_absorption: 0.0,
        edge_zone: 14.0,
        quality: GlassQuality::Standard,
        refractive_index: 1.2,
        thickness: 20.0,
        lens_model: LensModel::Spherical,
        rim_shade: 0.0,
        rim_shade_ends: 0.2,
        magnification: 1.0,
        fresnel_strength: 1.0,
        whiten: 0.0,
        blend: [BlendShape::NONE; BLEND_SHAPES],
        blend_count: 0,
        blend_k: 0.0,
    };
}

/// How a blurred backdrop is shaded before it is written back over the frame.
/// 模糊后的背景在写回帧之前如何着色。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackdropMaterial {
    /// Plain blit of the blurred pixels; the node's own translucent fill supplies the tint.
    /// 直接写回模糊像素；着色由节点自己的半透明填充提供。
    Plain,
    Acrylic(Acrylic),
    Glass(Glass),
}

impl BackdropMaterial {
    /// The material that leaves the blur untouched.
    /// 不改动模糊结果的那种材质。
    pub const PLAIN: Self = Self::Plain;
}

/// A backdrop blur recorded by `Op::Backdrop`.
/// `Op::Backdrop` 记录的背景模糊。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropEntry {
    pub rect: Rect,
    pub corners: Corners,
    pub radius: u32,
    pub sigma: f32,
    /// How the blurred result is shaded; `PLAIN` keeps the tinted blur.
    /// 模糊结果如何着色；`PLAIN` 保留普通着色模糊。
    pub material: BackdropMaterial,
}

/// A clip region recorded by `Op::PushClip`.
/// `Op::PushClip` 记录的裁剪区域。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipEntry {
    pub rect: Rect,
    /// Corner shape of the clip; `Corners::ZERO` means a plain rectangle.
    /// 裁剪的圆角形状；`Corners::ZERO` 表示普通矩形。
    pub corners: Corners,
    /// The uncut box the corners belong to; equals `rect` unless an outer clip cut it.
    /// 圆角所属的未裁切盒子；除非被外层裁切，否则等于 `rect`。
    pub shape: Rect,
}

impl ClipEntry {
    pub fn rect(rect: Rect) -> Self {
        Self {
            rect,
            corners: Corners::ZERO,
            shape: rect,
        }
    }
}

/// A layer recorded by `Op::PushLayer`.
/// `Op::PushLayer` 记录的层。
#[derive(Clone, Debug, PartialEq)]
pub struct LayerEntry {
    pub bounds: Rect,
    /// Applied in order to the layer's pixels before compositing; empty means none.
    /// 合成前按顺序作用于本层像素；空表示无效果。
    pub effects: Vec<crate::effect::Effect>,
    pub radius: Corners,
}

impl LayerEntry {
    /// Uniform fade when the chain is exactly one opacity matrix.
    /// 当效果链恰好是一个透明度矩阵时给出均匀淡出值。
    pub fn opacity(&self) -> Option<f32> {
        match self.effects.as_slice() {
            [] => Some(1.0),
            [e] if e.kernel.is_identity() => e.color.as_opacity(),
            _ => None,
        }
    }
}

/// Typed primitive banks.
/// 按类型分组的图元数组。
#[derive(Default, Debug)]
pub struct Primitives {
    pub rects: Vec<RectPrim>,
    pub texts: Vec<TextPrim>,
    pub glyphs: Vec<GlyphPrim>,
    pub images: Vec<ImagePrim>,
    pub shadows: Vec<ShadowPrim>,
    pub sectors: Vec<SectorPrim>,
}

impl Primitives {
    pub fn len_of(&self, kind: PrimKind) -> usize {
        match kind {
            PrimKind::Rect => self.rects.len(),
            PrimKind::Text => self.texts.len(),
            PrimKind::Glyph => self.glyphs.len(),
            PrimKind::Image => self.images.len(),
            PrimKind::Shadow => self.shadows.len(),
            PrimKind::Sector => self.sectors.len(),
        }
    }
}

/// A contiguous run of same-kind primitives under one clip, one layer.
/// 同一裁剪、同一层下连续的一段同类型图元。
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub kind: PrimKind,
    /// Range into the typed bank.
    /// 在对应类型数组中的区间。
    pub range: core::ops::Range<u32>,
    /// Clip in force for this batch (`None` = unlimited).
    /// 本批次的裁剪（`None` 表示不裁剪）。
    pub clip: Option<ClipEntry>,
    /// Layer chain from outermost to innermost; empty means the root target.
    /// 从最外层到最内层的层链；空表示根目标。
    pub layers: Vec<u32>,
    /// Backdrop generation; a batch never spans two different values.
    /// 背景模糊世代；批次绝不跨越两个不同的值。
    pub epoch: u32,
}

/// The per-frame draw list.
/// 每帧的绘制列表。
#[derive(Default, Debug)]
pub struct DrawList {
    pub ops: Vec<Op>,
    pub prims: Primitives,
    pub clips: Vec<ClipEntry>,
    pub layers: Vec<LayerEntry>,
    pub backdrops: Vec<BackdropEntry>,
    /// Textures this frame samples, indexed by `ImagePrim::texture`.
    /// 本帧采样的纹理，由 `ImagePrim::texture` 索引。
    pub images: Vec<ImageEntry>,
    /// Owning node per primitive, for per-node damage diffing.
    /// 每个图元所属节点，供按节点做脏区差分。
    pub owners: Owners,
    /// Canvas size, for batching decisions.
    /// 画布尺寸，供批次决策。
    pub canvas: Rect,
}

/// Per-kind ownership tables, parallel to `Primitives`.
/// 与 `Primitives` 平行的归属表。
#[derive(Default, Debug)]
pub struct Owners {
    pub rects: Vec<NodeId>,
    pub texts: Vec<NodeId>,
    pub glyphs: Vec<NodeId>,
    pub images: Vec<NodeId>,
    pub shadows: Vec<NodeId>,
    pub sectors: Vec<NodeId>,
}

impl Owners {
    pub fn push(&mut self, kind: PrimKind, node: NodeId) {
        match kind {
            PrimKind::Rect => self.rects.push(node),
            PrimKind::Text => self.texts.push(node),
            PrimKind::Glyph => self.glyphs.push(node),
            PrimKind::Image => self.images.push(node),
            PrimKind::Shadow => self.shadows.push(node),
            PrimKind::Sector => self.sectors.push(node),
        }
    }

    pub fn of(&self, kind: PrimKind, index: usize) -> Option<NodeId> {
        match kind {
            PrimKind::Rect => self.rects.get(index).copied(),
            PrimKind::Text => self.texts.get(index).copied(),
            PrimKind::Glyph => self.glyphs.get(index).copied(),
            PrimKind::Image => self.images.get(index).copied(),
            PrimKind::Shadow => self.shadows.get(index).copied(),
            PrimKind::Sector => self.sectors.get(index).copied(),
        }
    }
}

impl DrawList {
    pub fn new(canvas: Rect) -> Self {
        Self {
            canvas,
            ..Default::default()
        }
    }

    pub fn clear(&mut self) {
        self.ops.clear();
        self.prims.rects.clear();
        self.prims.texts.clear();
        self.prims.glyphs.clear();
        self.prims.images.clear();
        self.prims.shadows.clear();
        self.prims.sectors.clear();
        self.clips.clear();
        self.layers.clear();
        self.backdrops.clear();
        self.images.clear();
        self.owners.rects.clear();
        self.owners.texts.clear();
        self.owners.glyphs.clear();
        self.owners.images.clear();
        self.owners.shadows.clear();
        self.owners.sectors.clear();
    }

    pub fn push_rect(&mut self, owner: NodeId, prim: RectPrim) -> u32 {
        let index = self.prims.rects.len() as u32;
        self.prims.rects.push(prim);
        self.owners.push(PrimKind::Rect, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Rect,
            index,
        }));
        index
    }

    /// Register a texture for this frame and return its index.
    /// 为本帧登记一张纹理并返回下标。
    pub fn push_image(&mut self, entry: ImageEntry) -> u32 {
        let index = self.images.len() as u32;
        self.images.push(entry);
        index
    }

    /// Add one stroked arc; its bounds ride along for damage and culling.
    /// 加入一段描边弧；包围盒一并带上，供脏区与裁剪使用。
    pub fn push_sector(&mut self, owner: NodeId, prim: SectorPrim) -> u32 {
        let index = self.prims.sectors.len() as u32;
        self.prims.sectors.push(prim);
        self.owners.push(PrimKind::Sector, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Sector,
            index,
        }));
        index
    }

    pub fn push_shadow(&mut self, owner: NodeId, prim: ShadowPrim) -> u32 {
        let index = self.prims.shadows.len() as u32;
        self.prims.shadows.push(prim);
        self.owners.push(PrimKind::Shadow, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Shadow,
            index,
        }));
        index
    }

    pub fn push_image_prim(&mut self, owner: NodeId, prim: ImagePrim) -> u32 {
        let index = self.prims.images.len() as u32;
        self.prims.images.push(prim);
        self.owners.push(PrimKind::Image, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Image,
            index,
        }));
        index
    }

    pub fn push_glyph(&mut self, owner: NodeId, prim: GlyphPrim) -> u32 {
        let index = self.prims.glyphs.len() as u32;
        self.prims.glyphs.push(prim);
        self.owners.push(PrimKind::Glyph, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Glyph,
            index,
        }));
        index
    }

    pub fn push_text(&mut self, owner: NodeId, prim: TextPrim) -> u32 {
        let index = self.prims.texts.len() as u32;
        self.prims.texts.push(prim);
        self.owners.push(PrimKind::Text, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Text,
            index,
        }));
        index
    }

    pub fn push_clip(&mut self, rect: Rect, corners: Corners) {
        self.push_clip_shaped(rect, corners, rect);
    }

    /// Push a clip whose corners belong to `shape`, an uncut box that an outer clip narrowed
    /// to `rect`: the cut edge stays square while the node's own corners stay round.
    /// 压入圆角属于 `shape` 的裁剪：外层把盒子裁到 `rect`，切口保持方角，节点自身的角仍是圆的。
    pub fn push_clip_shaped(&mut self, rect: Rect, corners: Corners, shape: Rect) {
        let index = self.clips.len() as u32;
        self.clips.push(ClipEntry { rect, corners, shape });
        self.ops.push(Op::PushClip(index));
    }

    pub fn pop_clip(&mut self) {
        self.ops.push(Op::PopClip);
    }

    pub fn push_layer(
        &mut self,
        bounds: Rect,
        effects: Vec<crate::effect::Effect>,
        radius: Corners,
    ) {
        let index = self.layers.len() as u32;
        self.layers.push(LayerEntry {
            bounds,
            effects,
            radius,
        });
        self.ops.push(Op::PushLayer(index));
    }

    pub fn pop_layer(&mut self) {
        self.ops.push(Op::PopLayer);
    }

    pub fn push_backdrop(&mut self, entry: BackdropEntry) {
        let index = self.backdrops.len() as u32;
        self.backdrops.push(entry);
        self.ops.push(Op::Backdrop(index));
    }

    /// Backdrops in paint order, paired with the epoch after which each must run.
    /// 按绘制顺序列出背景模糊，并给出每个应在哪个世代之后执行。
    pub fn backdrop_order(&self) -> Vec<(u32, BackdropEntry)> {
        let mut out = Vec::new();
        let mut epoch = 0u32;
        for op in &self.ops {
            if let Op::Backdrop(i) = *op {
                epoch += 1;
                if let Some(e) = self.backdrops.get(i as usize) {
                    out.push((epoch, *e));
                }
            }
        }
        out
    }

    /// Rect a primitive occupies, for damage and culling.
    /// 图元占据的矩形，供脏区与裁剪使用。
    pub fn prim_bounds(&self, pr: PrimRef) -> Rect {
        match pr.kind {
            PrimKind::Rect => self
                .prims
                .rects
                .get(pr.index as usize)
                .map(|p| p.rect)
                .unwrap_or(Rect::ZERO),
            PrimKind::Glyph => self
                .prims
                .glyphs
                .get(pr.index as usize)
                .map(|p| p.rect)
                .unwrap_or(Rect::ZERO),
            PrimKind::Image => self
                .prims
                .images
                .get(pr.index as usize)
                .map(|p| p.rect)
                .unwrap_or(Rect::ZERO),
            PrimKind::Shadow => self
                .prims
                .shadows
                .get(pr.index as usize)
                .map(|p| p.rect)
                .unwrap_or(Rect::ZERO),
            PrimKind::Sector => self
                .prims
                .sectors
                .get(pr.index as usize)
                .map(|p| p.bounds)
                .unwrap_or(Rect::ZERO),
            PrimKind::Text => {
                let t = self.prims.texts.get(pr.index as usize);
                match t {
                    Some(t) => {
                        // Approximation: width unknown here, so report the origin and a
                        // generous box; text damage is refined by the owning node rect.
                        // 近似：此处不知宽度，用来源点加一个宽松盒；文字脏区由所属节点矩形细化。
                        let (text, _) = t.block.flatten();
                        let size = t
                            .block
                            .runs
                            .first()
                            .map_or(14.0, |r| r.style.size);
                        Rect::new(
                            t.origin.x,
                            t.origin.y,
                            (text.chars().count() as f32 * size * 0.6).max(1.0),
                            size * 1.3,
                        )
                    }
                    None => Rect::ZERO,
                }
            }
        }
    }

    /// Group the order log into batches usable by a backend.
    /// 把顺序日志归并成后端可用的批次。
    ///
    /// A batch breaks when the kind, clip, or layer changes. Layers switch the
    /// render target, so primitives across them must never merge; this is why the
    /// plan is not a plain k-way merge of the typed banks.
    /// 种类、裁剪或层变化即断批。层会切换渲染目标，跨层图元绝不能合并；
    /// 这也是为什么批次计划不是对类型数组的简单 k 路归并。
    pub fn plan_batches(&self) -> Vec<Batch> {
        let mut out: Vec<Batch> = Vec::new();
        let mut clip_stack: Vec<ClipEntry> = Vec::new();
        let mut layer_stack: Vec<u32> = Vec::new();
        // A backdrop reads and rewrites the target, so nothing may batch across it.
        // 背景模糊会读并改写目标，因此不能有任何批次跨越它。
        let mut backdrop_epoch = 0u32;

        for op in &self.ops {
            match *op {
                Op::PushClip(i) => {
                    if let Some(c) = self.clips.get(i as usize) {
                        clip_stack.push(*c);
                    }
                }
                Op::PopClip => {
                    clip_stack.pop();
                }
                Op::Backdrop(_) => backdrop_epoch += 1,
                Op::PushLayer(i) => layer_stack.push(i),
                Op::PopLayer => {
                    layer_stack.pop();
                }
                Op::Prim(pr) => {
                    // Rects intersect; the shape comes from the clip that still bounds the
                    // intersection, since a narrower square clip must not inherit outer curves.
                    // 矩形求交；形状取自仍界定交集的那层裁剪，更窄的方角裁剪不得继承外层圆角。
                    let clip = clip_stack.split_first().map(|(first, rest)| {
                        let rect = rest
                            .iter()
                            .fold(first.rect, |acc, c| acc.intersection(c.rect).unwrap_or(Rect::ZERO));
                        let shaped = clip_stack.iter().rev().find(|c| same_rect(c.rect, rect));
                        let corners = shaped.map(|c| c.corners).unwrap_or(Corners::ZERO);
                        let shape = shaped.map(|c| c.shape).unwrap_or(rect);
                        ClipEntry { rect, corners, shape }
                    });
                    let layers = &layer_stack;
                    match out.last_mut() {
                        Some(last)
                            if last.kind == pr.kind
                                && last.clip == clip
                                && last.layers == *layers
                                && last.epoch == backdrop_epoch =>
                        {
                            last.range.end += 1;
                        }
                        _ => out.push(Batch {
                            kind: pr.kind,
                            range: pr.index..pr.index + 1,
                            clip,
                            layers: layers.clone(),
                            epoch: backdrop_epoch,
                        }),
                    }
                }
            }
        }
        out
    }
}

/// Area a node affects, including ink margin.
/// 节点影响的区域，含绘制外扩。
pub fn node_ink_rect(rect: Rect, ink: Insets) -> Rect {
    ink.expand(rect)
}

/// True when `rect` is the one that shaped `clip`: equal within rounding noise.
/// 当 `rect` 就是塑造了 `clip` 的那一个时返回 true：在取整噪声内相等。
fn same_rect(rect: Rect, clip: Rect) -> bool {
    const EPS: f32 = 0.01;
    (rect.origin.x - clip.origin.x).abs() < EPS
        && (rect.origin.y - clip.origin.y).abs() < EPS
        && (rect.size.w - clip.size.w).abs() < EPS
        && (rect.size.h - clip.size.h).abs() < EPS
}

/// Convenience for tests and examples.
/// 供测试与示例使用。
pub fn prim_ref(kind: PrimKind, index: u32) -> PrimRef {
    PrimRef { kind, index }
}

/// Owner of the primitive with this id, if any (debug helper).
/// 该 id 对应的图元归属（调试辅助）。
pub fn owner_of(list: &DrawList, id: Id) -> Option<NodeId> {
    let _ = (list, id);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::NodeId;

    fn nid(i: u32) -> NodeId {
        NodeId {
            index: i,
            generation: 0,
        }
    }

    #[test]
    fn a_laid_out_offset_reaches_the_draw_list() {
        // `rect` is the un-offset layout box; only `visual_rect` carries the offset. A
        // builder that reads `rect` pins every offset child to its parent's origin, which
        // is how a switch thumb ended up stuck at the start of its track.
        // `rect` 是未加偏移的布局盒，只有 `visual_rect` 带偏移。读 `rect` 的构建器会把每个
        // 带偏移的子节点钉在父原点上——滑块因此一直卡在轨道开头。
        use crate::layout::LayoutEngine as _;
        let mut tree = crate::tree::Tree::new();
        tree.apply(
            &crate::Decl::container(crate::Id::str("track"), crate::NodeKind::Box)
                .size(crate::layout::SizeClass::fixed(52.0, 32.0))
                .background(crate::geom::Color::WHITE, crate::geom::Corners::ZERO)
                .child(
                    crate::Decl::container(crate::Id::str("thumb"), crate::NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(16.0, 16.0))
                        .offset(8.0, 8.0)
                        .background(crate::geom::Color::WHITE, crate::geom::Corners::ZERO),
                ),
        );
        let root = tree.root().expect("root");
        crate::BuiltinLayout::new(crate::MonospaceMeasure::default()).solve(
            &mut tree,
            root,
            crate::Constraints::loose(crate::geom::Size::new(200.0, 200.0)),
        );
        let list = list_from_tree(&tree, Rect::new(0.0, 0.0, 200.0, 200.0));
        // The thumb is the only 16x16 primitive; the track is 52x32.
        // 滑块是唯一的 16x16 图元；轨道是 52x32。
        let thumb = list
            .prims
            .rects
            .iter()
            .find(|p| p.rect.size.w == 16.0)
            .expect("thumb primitive");
        assert_eq!(thumb.rect.origin.x, 8.0, "offset must be applied horizontally");
        assert_eq!(thumb.rect.origin.y, 8.0, "offset must be applied vertically");
    }

    fn rect_prim(w: f32) -> RectPrim {
        RectPrim {
            rect: Rect::new(0.0, 0.0, w, w),
            radius: Corners::ZERO,
            fill: Color::WHITE,
            gradient: None,
            border: None,
            border_gradient: None,
            notch: None,
        }
    }

    #[test]
    fn consecutive_same_kind_primitives_form_one_batch() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_rect(nid(1), rect_prim(10.0));
        list.push_rect(nid(2), rect_prim(10.0));

        let batches = list.plan_batches();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].range, 0..3);
    }

    #[test]
    fn clear_empties_every_side_table_not_just_the_ops() {
        // A table that outlives its ops accumulates stale entries; indices then point at
        // the wrong entry or the table grows without bound.
        // 与 op 不同步的表会累积陈旧条目；下标随即指错，或表无限增长。
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0), Corners::ZERO);
        list.push_layer(
            Rect::new(0.0, 0.0, 50.0, 50.0),
            Vec::new(),
            Corners::ZERO,
        );
        list.push_backdrop(BackdropEntry {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            corners: Corners::ZERO,
            radius: 6,
            sigma: 2.0,
            material: BackdropMaterial::PLAIN,
        });

        list.clear();

        assert!(list.ops.is_empty());
        assert!(list.clips.is_empty(), "clips must be cleared with the ops");
        assert!(list.layers.is_empty(), "layers must be cleared with the ops");
        assert!(
            list.backdrops.is_empty(),
            "backdrops must be cleared with the ops"
        );
    }

    #[test]
    fn a_backdrop_breaks_the_batch_so_nothing_paints_across_it() {
        // A backdrop rewrites the target in place; a batch spanning it would draw rects
        // that were meant to land after the blur, or blur pixels already drawn over.
        // 背景模糊会就地改写目标；跨越它的批次会把本该在模糊后画的矩形提前画上。
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_backdrop(BackdropEntry {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            corners: Corners::ZERO,
            radius: 6,
            sigma: 2.0,
            material: BackdropMaterial::PLAIN,
        });
        list.push_rect(nid(1), rect_prim(10.0));

        let batches = list.plan_batches();
        assert_eq!(batches.len(), 2, "the two rects must not share a batch");
        assert!(batches[0].epoch < batches[1].epoch);
    }

    #[test]
    fn backdrop_order_reports_each_blur_at_its_own_epoch() {
        // The renderer interleaves by epoch, so the numbering must be strictly increasing.
        // 渲染器按世代交织，因此编号必须严格递增。
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let entry = BackdropEntry {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            corners: Corners::ZERO,
            radius: 6,
            sigma: 2.0,
            material: BackdropMaterial::PLAIN,
        };
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_backdrop(entry);
        list.push_rect(nid(1), rect_prim(10.0));
        list.push_backdrop(entry);

        let order = list.backdrop_order();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0].0, 1);
        assert_eq!(order[1].0, 2);
    }

    #[test]
    fn clip_change_breaks_the_batch() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0), Corners::ZERO);
        list.push_rect(nid(1), rect_prim(10.0));
        list.pop_clip();
        list.push_rect(nid(2), rect_prim(10.0));

        let batches = list.plan_batches();
        assert_eq!(batches.len(), 3, "clip must split batches");
        assert_eq!(
            batches[1].clip.map(|c| c.rect),
            Some(Rect::new(0.0, 0.0, 50.0, 50.0))
        );
    }

    #[test]
    fn a_narrower_square_clip_drops_the_outer_rounded_shape() {
        // The curve belongs to the clip that still bounds the intersection; an outer rounded
        // panel must not round a child that clips itself square.
        // 曲线属于仍界定交集的那层裁剪；外层圆角面板不得给自身方角裁剪的子节点加圆角。
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let row = Rect::new(10.0, 10.0, 50.0, 20.0);
        list.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0), Corners::all(12.0));
        list.push_clip(row, Corners::ZERO);
        list.push_rect(nid(0), rect_prim(10.0));
        list.pop_clip();
        list.pop_clip();

        let clip = list.plan_batches()[0].clip.expect("a clip is in force");
        assert_eq!(clip.rect, row);
        assert!(clip.corners.is_zero(), "the square row inherited the panel's corners");
    }

    #[test]
    fn the_tight_rounded_clip_keeps_its_corners() {
        // The other direction of the same rule: when the rounded clip is the one bounding the
        // intersection, its curve is the one the batch must draw with.
        // 同一条规则的另一面：当界定交集的是圆角裁剪时，批次必须按它的曲线绘制。
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let panel = Rect::new(0.0, 0.0, 100.0, 100.0);
        list.push_clip(Rect::new(-20.0, -20.0, 300.0, 300.0), Corners::ZERO);
        list.push_clip(panel, Corners::all(12.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.pop_clip();
        list.pop_clip();

        let clip = list.plan_batches()[0].clip.expect("a clip is in force");
        assert_eq!(clip.rect, panel);
        assert_eq!(clip.corners, Corners::all(12.0));
    }

    #[test]
    fn layer_change_breaks_the_batch() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_layer(Rect::new(0.0, 0.0, 40.0, 40.0), vec![crate::effect::Effect::opacity(0.5)], Corners::ZERO);
        list.push_rect(nid(1), rect_prim(10.0));
        list.pop_layer();
        list.push_rect(nid(2), rect_prim(10.0));

        let batches = list.plan_batches();
        assert_eq!(batches.len(), 3, "layer must split batches");
        assert_eq!(batches[1].layers, vec![0]);
        assert!(batches[2].layers.is_empty());
    }

    #[test]
    fn kind_change_breaks_the_batch() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_text(
            nid(1),
            TextPrim {
                block: crate::text::TextBlock::plain(
                    "hi",
                    crate::text::TextStyle::new(12.0),
                ),
                origin: crate::geom::Point::new(0.0, 0.0),
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                ink: Color::WHITE,
                caret: None,
                selection: None,
                composing: None,
            },
        );

        let batches = list.plan_batches();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].kind, PrimKind::Rect);
        assert_eq!(batches[1].kind, PrimKind::Text);
    }

    #[test]
    fn owner_table_tracks_each_primitive() {
        let mut list = DrawList::new(Rect::ZERO);
        list.push_rect(nid(7), rect_prim(1.0));
        list.push_rect(nid(9), rect_prim(1.0));
        assert_eq!(list.owners.of(PrimKind::Rect, 0), Some(nid(7)));
        assert_eq!(list.owners.of(PrimKind::Rect, 1), Some(nid(9)));
    }
}
