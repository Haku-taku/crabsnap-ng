//! GLSL sources and the instance layout each program expects.
//! GLSL 源码，以及每个程序期望的实例数据布局。

/// Floats per rounded-rect instance: rect, radius, fill, border, shape, far stop, gradient,
/// border width, notch. Colour and width are separate slots: one slot cannot carry both.
/// 每个圆角矩形实例的浮点数：矩形、圆角、填充、边框、形状、远端色、渐变、边框宽、缺口。
/// 颜色与宽度各占一槽：同一槽不能兼任两者。
pub const RECT_STRIDE: i32 = 39;

/// Largest tap count the effect shader accepts; keep in sync with `u_weights[33]`.
/// 效果着色器接受的最大 tap 数；与 `u_weights[33]` 保持一致。
pub const MAX_TAPS: u32 = 33;

/// The SDF feather, in device pixels: coverage ramps over this width, and every quad reaches
/// this far past its shape so that no covered pixel centre falls outside it.
/// SDF 羽化宽度（设备像素）：覆盖率在此宽度内渐变，每个四边形也向外伸出同样距离，
/// 使任何有覆盖的像素中心都落在其内。
pub const AA_GLSL: &str = "const float AA = 0.5;";

/// Clip uniforms and the coverage test shared by every fragment program.
/// 所有片元程序共用的裁剪 uniform 与覆盖度判定。
///
/// Composed into the sources rather than duplicated, so the two programs cannot drift.
/// 组合进源串而非复制，避免两个程序各自漂移。
pub const CLIP_GLSL: &str = r#"
uniform vec4 u_clip_rect;     // x, y, w, h in device pixels
uniform vec4 u_clip_radius;   // tl, tr, br, bl in device pixels
uniform float u_clip_shape;   // superellipse exponent
uniform float u_clip_on;

// Superellipse norm of a non-negative pair; n = 2 is the circle, n = 1 a bevel.
// 非负数对的超椭圆范数；n = 2 为圆，n = 1 为直线斜切。
float clip_lp_norm(vec2 q, float n) {
    if (n <= 1.0) {
        return q.x + q.y;
    }
    if (n >= 64.0) {
        return max(q.x, q.y);
    }
    return pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
}

// Coverage of the rounded clip at a device position; 1.0 when no clip is set.
// 设备位置处的圆角裁剪覆盖度；未设置裁剪时为 1.0。
float clip_coverage(vec2 device) {
    if (u_clip_on < 0.5) {
        return 1.0;
    }
    vec2 half_size = u_clip_rect.zw * 0.5;
    vec2 local = device - (u_clip_rect.xy + half_size);
    vec4 r = u_clip_radius;
    // Same (tl, tr, br, bl) packing as `sd_round_box`, so the same quadrant pick.
    // 与 `sd_round_box` 相同的 (tl, tr, br, bl) 打包，因此象限取值也相同。
    r.xy = (local.x > 0.0) ? vec2(r.y, r.z) : vec2(r.x, r.w);
    r.x = (local.y > 0.0) ? r.y : r.x;
    vec2 q = abs(local) - half_size + r.x;
    float d = min(max(q.x, q.y), 0.0) + clip_lp_norm(max(q, vec2(0.0)), u_clip_shape) - r.x;
    return clamp(AA - d, 0.0, 1.0);
}
"#;

/// Rounded-shape coverage plus the uniforms it reads, shared by every backdrop program.
/// 圆角形状覆盖度及其读取的 uniform，所有背景程序共用。
pub const COVERAGE_GLSL: &str = r#"
uniform vec4 u_rect;    // x, y, w, h in device pixels
uniform vec4 u_radius;  // tl, tr, br, bl in device pixels
uniform float u_exp;    // superellipse exponent

// Same level set as the CPU rasteriser, so both backends cut the same corner.
// 与 CPU 光栅化器相同的等值线，使两个后端切出相同的圆角。
float snorm(float x, float y, float n) {
    if (n <= 1.0) return x + y;
    if (n >= 64.0) return max(x, y);
    return pow(pow(x, n) + pow(y, n), 1.0 / n);
}

float coverage(vec2 p) {
    float l = u_rect.x, t = u_rect.y;
    float r = l + u_rect.z, b = t + u_rect.w;
    if (p.x < l || p.x > r || p.y < t || p.y > b) return 0.0;
    if (u_radius.x > 0.0 && p.x < l + u_radius.x && p.y < t + u_radius.x)
        return clamp(u_radius.x + AA - snorm(abs(p.x - l - u_radius.x), abs(p.y - t - u_radius.x), u_exp), 0.0, 1.0);
    if (u_radius.y > 0.0 && p.x > r - u_radius.y && p.y < t + u_radius.y)
        return clamp(u_radius.y + AA - snorm(abs(p.x - r + u_radius.y), abs(p.y - t - u_radius.y), u_exp), 0.0, 1.0);
    if (u_radius.z > 0.0 && p.x > r - u_radius.z && p.y > b - u_radius.z)
        return clamp(u_radius.z + AA - snorm(abs(p.x - r + u_radius.z), abs(p.y - b + u_radius.z), u_exp), 0.0, 1.0);
    if (u_radius.w > 0.0 && p.x < l + u_radius.w && p.y > b - u_radius.w)
        return clamp(u_radius.w + AA - snorm(abs(p.x - l - u_radius.w), abs(p.y - b + u_radius.w), u_exp), 0.0, 1.0);
    return 1.0;
}
"#;

/// Rounded-box signed distance, shared by the rect and the glass programs.
/// 圆角矩形有符号距离，矩形与玻璃程序共用。
pub const SDF_GLSL: &str = r#"
// Superellipse norm of a non-negative pair; n = 2 is the circle, n = 1 a bevel.
// 非负数对的超椭圆范数；n = 2 为圆，n = 1 为直线斜切。
float lp_norm(vec2 q, float n) {
    if (n <= 1.0) {
        return q.x + q.y;
    }
    if (n >= 64.0) {
        return max(q.x, q.y);
    }
    return pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
}

// Signed distance to a rounded box; negative inside. `r` is (tl, tr, br, bl), y down, so the
// quadrant pick must take (tl,bl) on the left and (tr,br) on the right — iq's packing is not this one.
// 到圆角矩形的有符号距离；内部为负。`r` 是 (tl, tr, br, bl)，y 向下，
// 因此象限取值左侧取 (tl,bl)、右侧取 (tr,br)——iq 那份打包顺序不是这个。
float sd_round_box(vec2 p, vec2 b, vec4 r, float n) {
    r.xy = (p.x > 0.0) ? vec2(r.y, r.z) : vec2(r.x, r.w);
    r.x  = (p.y > 0.0) ? r.y : r.x;
    vec2 q = abs(p) - b + r.x;
    return min(max(q.x, q.y), 0.0) + lp_norm(max(q, vec2(0.0)), n) - r.x;
}
"#;

/// Floats per glyph instance: dest rect, atlas rect, colour, kind.
/// 每个字形实例的浮点数：目标矩形、图集矩形、颜色、种类。
pub const GLYPH_STRIDE: i32 = 13;

/// Floats per image instance: dest rect, radius, uv rect, shape exponent.
/// 每个图像实例的浮点数：目标矩形、圆角、uv 矩形、形状指数。
pub const IMAGE_STRIDE: i32 = 13;

/// Unit quad shared by both programs; scaled per instance.
/// 两个程序共用的单位四边形，按实例缩放。
pub const UNIT_QUAD: [f32; 12] = [
    0.0, 0.0, 1.0, 0.0, 1.0, 1.0, //
    0.0, 0.0, 1.0, 1.0, 0.0, 1.0,
];

/// Floats per shadow instance: rect, radius, colour, blur, dy, shape.
/// 每个阴影实例的浮点数：矩形、圆角、颜色、衰减宽度、垂直偏移、指数。
pub const SHADOW_STRIDE: i32 = 16;

pub const SHADOW_VERTEX: &str = r#"
precision highp float;
in vec2 a_unit;
in vec4 a_rect;      // the *shadowed* shape, device pixels, y down
in vec4 a_radius;
in vec4 a_colour;    // rgb + peak alpha
in vec2 a_soft;      // x: falloff width, y: dy
in float a_shape;
uniform vec2 u_viewport;
out vec2 v_local;
out vec2 v_half;
out vec4 v_radius;
out vec4 v_colour;
out vec2 v_soft;
out float v_shape;
out vec2 v_device;
void main() {
    // Grow by the falloff on every side and by the offset, so the whole gradient fits.
    // 四边各外扩一个衰减宽度，再加上偏移，整个渐变才落在四边形内。
    vec2 pad = vec2(a_soft.x) + vec2(max(a_soft.y, 0.0), abs(a_soft.y));
    vec2 origin = a_rect.xy - pad;
    vec2 size = a_rect.zw + 2.0 * pad;
    // Reaches `AA` past the falloff so the outermost covered pixel is still rasterised.
    // 在衰减宽度之外再伸 `AA`，最外那圈有覆盖的像素因此仍会被光栅化。
    vec2 device = origin - AA + a_unit * (size + 2.0 * AA);
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    // Evaluate the SDF in the shadowed shape's own space, from the real device position.
    // 在被投影形状自身的空间里求 SDF，取自真实设备位置。
    v_local = device - (a_rect.xy + a_rect.zw * 0.5);
    v_half = a_rect.zw * 0.5;
    v_radius = a_radius;
    v_colour = a_colour;
    v_soft = a_soft;
    v_shape = a_shape;
    v_device = device;
}
"#;

/// Flat ±100 px shift for diagnosis: a displaced wash proves the quad is drawn at all.
/// 诊断用的 +100px 平移：能把色块挪出来就证明四边形确实被画了。
pub const SHADOW_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_local;
in vec2 v_half;
in vec4 v_radius;
in vec4 v_colour;
in vec2 v_soft;
in float v_shape;
in vec2 v_device;
out vec4 frag;

void main() {
    float d = sd_round_box(v_local, v_half, v_radius, v_shape);
    // The SDF is negative inside: the shadow is opaque there and gone `blur` outside.
    // SDF 在形状内为负：阴影在此不透明，向外 `blur` 处消失。
    float t = clamp(d / max(v_soft.x, 1e-4), 0.0, 1.0);
    t = 1.0 - t;
    float a = v_colour.a * t * t * (3.0 - 2.0 * t);
    a *= clip_coverage(v_device);
    if (a <= 0.0) {
        discard;
    }
    frag = vec4(v_colour.rgb * a, a);
}
"#;

pub const RECT_VERTEX: &str = r#"
precision highp float;
in vec2 a_unit;
in vec4 a_rect;      // x, y, w, h in device pixels, y down
in vec4 a_radius;    // tl, tr, br, bl
in vec4 a_fill;
in vec4 a_border;    // straight rgb + alpha
in vec4 a_border2;   // the stroke gradient's far stop
in float a_border_g; // 1 = the stroke carries a gradient
in float a_border_w; // border width in device pixels
in float a_shape;    // superellipse exponent 2^k
in vec4 a_fill2;     // gradient's far stop
in vec2 a_grad;      // x: 0 = vertical, 1 = horizontal; y: 1 = enabled
in vec4 a_stops;     // fill near/far, border near/far, as gradient-space u
in vec2 a_extent;    // fill and border gradient lengths in device px; 0 = the box's own
in vec2 a_notch;     // top-border gap, x range from the box's left edge; empty when x >= y
in vec2 a_flip;      // 1 = that gradient is measured from the box's far edge
uniform vec2 u_viewport;
out vec2 v_local;
out vec2 v_half;
out vec4 v_radius;
out vec4 v_fill;
out vec4 v_border;
out vec4 v_border2;
out float v_border_g;
out float v_border_w;
out float v_shape;
out vec2 v_device;
out vec4 v_fill2;
out vec2 v_grad;
out vec4 v_stops;
out vec2 v_extent;
out vec2 v_notch;
out vec2 v_flip;
void main() {
    // Reaches `AA` past the box, so a pixel centred just outside it still gets a fragment.
    // 向外多伸 `AA`，中心恰在盒外的像素因此仍会被光栅化。
    vec2 device = a_rect.xy - AA + a_unit * (a_rect.zw + 2.0 * AA);
    // No y inversion: GL row 0 and a dma-buf row 0 are the same memory row, and
    // the compositor reads that row as the top of the image.
    // 不做 y 反转：GL 第 0 行与 dma-buf 第 0 行是同一条内存行，
    // 而 compositor 把该行当作图像顶部。
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    // From the real device position, not `a_unit`: a snapped quad spans more than the box,
    // and scaling `a_unit` would stretch the SDF space non-uniformly (ovals, fat strokes).
    // 取自真实设备位置而非 `a_unit`：取整后的四边形比盒子大，按 `a_unit` 缩放会把
    // SDF 空间非等比拉伸（圆变椭圆、描边变粗）。
    v_local = device - (a_rect.xy + a_rect.zw * 0.5);
    v_half = a_rect.zw * 0.5;
    v_radius = a_radius;
    v_fill = a_fill;
    v_border = a_border;
    v_border2 = a_border2;
    v_border_g = a_border_g;
    v_border_w = a_border_w;
    v_shape = a_shape;
    v_device = device;
    v_fill2 = a_fill2;
    v_grad = a_grad;
    v_stops = a_stops;
    v_extent = a_extent;
    v_notch = a_notch;
    v_flip = a_flip;
}
"#;

pub const RECT_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_local;
in vec2 v_half;
in vec4 v_radius;
in vec4 v_fill;
in vec4 v_border;
in vec4 v_border2;
in float v_border_g;
in float v_border_w;
in float v_shape;
in vec2 v_device;
in vec4 v_fill2;
in vec2 v_grad;
in vec4 v_stops;
in vec2 v_extent;
in vec2 v_notch;
in vec2 v_flip;
out vec4 frag;

// Coverage-weighted premultiplied colour so one blend equation covers rects
// and glyphs: rgb must be scaled by alpha, not just by coverage.
// 按覆盖度加权的预乘颜色，使一个混合方程同时覆盖矩形与字形：
// rgb 必须乘 alpha，而不只是乘覆盖度。
vec4 premul(vec4 straight, float coverage) {
    float a = straight.a * coverage;
    return vec4(straight.rgb * a, a);
}

// Map a gradient-space coordinate through a pair of stops.
// 把渐变空间坐标经一对停点重映射。
float remap(float u, float lo, float hi) {
    return clamp((u - lo) / max(hi - lo, 1e-5), 0.0, 1.0);
}

void main() {
    float d = sd_round_box(v_local, v_half, v_radius, v_shape);
    // One device pixel of feather; the SDF is already in pixel units.
    // 一个设备像素的羽化；SDF 本身就以像素为单位。
    float outer = clamp(AA - d, 0.0, 1.0);
    float width = v_border_w;
    // A gradient is sampled in the box's own space: v_local is centred, v_half is the
    // half extent, so t runs 0..1 across the box on the chosen axis.
    // 渐变在盒子自身空间里采样：v_local 居中、v_half 是半尺寸，故 t 沿选定轴跨过 0..1。
    // `along` is the distance from the box's near edge; `span` is either the box's own
    // extent or the brush's shorter run (WinUI's elevation borders cover only a few units).
    // `along` 是自盒子近缘起的距离；`span` 或是盒子自身长度，或是画刷更短的跨度
    // （WinUI 的立面描边只覆盖几个单位）。
    bool vertical = v_grad.x < 0.5;
    float along = vertical ? (v_local.y + v_half.y) : (v_local.x + v_half.x);
    float box = vertical ? 2.0 * v_half.y : 2.0 * v_half.x;
    // A mirrored brush measures the same run from the far edge; the box mirrors with it.
    // 镜像画刷从远缘度量同一段渐变；盒子随之镜像。
    float along_f = v_flip.x > 0.5 ? box - along : along;
    float along_b = v_flip.y > 0.5 ? box - along : along;
    vec4 fill = v_fill;
    if (v_grad.y > 0.5) {
        float span = v_extent.x > 0.0 ? v_extent.x : box;
        fill = mix(v_fill, v_fill2, remap(along_f / max(span, 1e-5), v_stops.x, v_stops.y));
    }
    // Ring and inner area are disjoint parts of one shape: weight them by area and scale
    // once. Two stacked blends count the feather twice and bleed fill into the ring.
    // 环与内部是同一形状的互斥面积：按面积加权后只缩放一次。
    // 叠加两次混合会把羽化算重，让填充渗进环里。
    vec4 fill_pm = premul(fill, 1.0);
    vec4 src = fill_pm;
    if (width > 0.0) {
        float inner = clamp(AA - (d + width), 0.0, 1.0);
        float ring = max(outer - inner, 0.0);
        // A notch cuts the stroke where it crosses the top edge; the fill survives, so a
        // transparent box shows the backdrop through the gap. The half-pixel is the stroke's
        // antialiased skirt, which reaches past the band and would otherwise survive as a line.
        // 缺口在描边穿过顶边处切掉它；填充仍在，因此透明盒子会透出背景。那半个像素是描边的
        // 抗锯齿边缘，它会越过带边界，否则会残留成一条细线。
        float local_x = v_local.x + v_half.x;
        if (v_notch.y > v_notch.x && local_x >= v_notch.x && local_x <= v_notch.y
            && v_local.y <= -v_half.y + width + 0.5) {
            ring = 0.0;
        }
        vec4 stroke = v_border;
        if (v_border_g > 0.5) {
            float span = v_extent.y > 0.0 ? v_extent.y : box;
            stroke = mix(v_border, v_border2, remap(along_b / max(span, 1e-5), v_stops.z, v_stops.w));
        }
        vec4 border_pm = premul(stroke, 1.0);
        vec4 over = border_pm + fill_pm * (1.0 - border_pm.a);
        src = mix(fill_pm, over, clamp(ring / max(outer, 1e-5), 0.0, 1.0));
    }
    vec4 colour = src * outer;
    // The clip attenuates premultiplied colour uniformly.
    // 裁剪按同一比例衰减预乘颜色。
    colour *= clip_coverage(v_device);
    if (colour.a <= 0.0) {
        discard;
    }
    frag = colour;
}
"#;

pub const GLYPH_VERTEX: &str = r#"
precision highp float;
in vec2 a_unit;
in vec4 a_rect;     // dest x, y, w, h in device pixels
in vec4 a_atlas;    // atlas x, y, w, h in texels
in vec4 a_color;    // straight rgb, alpha = coverage multiplier
in float a_kind;    // 0 = coverage atlas, 1 = colour atlas
uniform vec2 u_viewport;
uniform vec2 u_atlas_size;
uniform vec2 u_color_atlas_size;
out vec2 v_uv;
out vec4 v_color;
out float v_kind;
out vec2 v_device;
void main() {
    vec2 device = a_rect.xy + a_unit * a_rect.zw;
    // No y inversion: GL row 0 and a dma-buf row 0 are the same memory row, and
    // the compositor reads that row as the top of the image.
    // 不做 y 反转：GL 第 0 行与 dma-buf 第 0 行是同一条内存行，
    // 而 compositor 把该行当作图像顶部。
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    vec2 texel = a_atlas.xy + a_unit * a_atlas.zw;
    vec2 atlas_size = mix(u_atlas_size, u_color_atlas_size, step(0.5, a_kind));
    v_uv = texel / atlas_size;
    v_color = a_color;
    v_kind = a_kind;
    v_device = device;
}
"#;

pub const GLYPH_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
in vec4 v_color;
in float v_kind;
in vec2 v_device;
uniform sampler2D u_atlas;
uniform sampler2D u_color_atlas;
out vec4 frag;
void main() {
    float m = v_color.a * clip_coverage(v_device);
    float cover = texture(u_atlas, v_uv).r;
    // The colour atlas is already premultiplied; scale it by alpha and clip only.
    // 颜色图集已是预乘值，只需按 alpha 与裁剪缩放。
    vec4 colour = texture(u_color_atlas, v_uv) * m;
    vec4 tinted = vec4(v_color.rgb * (cover * m), cover * m);
    vec4 result = mix(tinted, colour, step(0.5, v_kind));
    if (result.a <= 0.0) {
        discard;
    }
    frag = result;
}
"#;

/// Samples one RGBA8 texture through a rounded rect, stride is `IMAGE_STRIDE`.
/// 在一个圆角矩形内采样一张 RGBA8 纹理，步长为 `IMAGE_STRIDE`。
pub const IMAGE_VERTEX: &str = r#"
precision highp float;
in vec2 a_unit;
in vec4 a_rect;      // dest x, y, w, h in device pixels
in vec4 a_radius;    // tl, tr, br, bl
in vec4 a_uv;        // u0, v0, u1, v1 normalised
in float a_shape;    // superellipse exponent 2^k
uniform vec2 u_viewport;
out vec2 v_local;
out vec2 v_half;
out vec4 v_radius;
out float v_shape;
out vec2 v_uv;
out vec2 v_device;
void main() {
    vec2 device = a_rect.xy + a_unit * a_rect.zw;
    // No y inversion: GL row 0 is the same memory row the compositor reads as the image top.
    // 不做 y 反转：GL 第 0 行正是 compositor 当作图像顶部的那条内存行。
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_local = a_unit * a_rect.zw - a_rect.zw * 0.5;
    v_half = a_rect.zw * 0.5;
    v_radius = a_radius;
    v_shape = a_shape;
    // The source sub-rect is mapped onto the whole destination, never tiled.
    // 源子矩形映射到整个目标区域，绝不平铺。
    v_uv = mix(a_uv.xy, a_uv.zw, a_unit);
    v_device = device;
}
"#;

pub const IMAGE_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_local;
in vec2 v_half;
in vec4 v_radius;
in float v_shape;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_image;
out vec4 frag;
void main() {
    float d = sd_round_box(v_local, v_half, v_radius, v_shape);
    // One device pixel of feather; the SDF is already in pixel units.
    // 一个设备像素的羽化；SDF 本身就以像素为单位。
    float cov = clamp(AA - d, 0.0, 1.0);
    if (cov <= 0.0) {
        discard;
    }
    // The texture holds straight alpha; premultiply so the frame's blend equation applies.
    // 纹理是直通 alpha；先预乘，才能套用帧的混合方程。
    vec4 s = texture(u_image, v_uv);
    float a = s.a * cov * clip_coverage(v_device);
    if (a <= 0.0) {
        discard;
    }
    frag = vec4(s.rgb * a, a);
}
"#;

/// Instanced quad for compositing a layer texture, stride is `RECT_STRIDE`.
/// 用于合成层纹理的实例化四边形，步长为 `RECT_STRIDE`。
pub const LAYER_VERTEX: &str = r#"
precision highp float;
in vec2 a_unit;
in vec4 a_rect;     // dest x, y, w, h in device pixels
uniform vec2 u_viewport;
out vec2 v_uv;
out vec2 v_device;
void main() {
    vec2 device = a_rect.xy + a_unit * a_rect.zw;
    v_device = device;
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    // The layer was rendered with the same projection, so v=0 is device row 0.
    // 层用同样的投影渲染，因此 v=0 对应设备第 0 行。
    v_uv = a_unit;
}
"#;

pub const LAYER_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_layer;
uniform float u_opacity;
uniform float u_skip_empty; // 1 when copying a backdrop: skip pixels nothing painted yet
out vec4 frag;

void main() {
    float cov = coverage(v_device);
    if (cov <= 0.0) {
        frag = vec4(0.0);
        return;
    }
    vec4 s = texture(u_layer, v_uv);
    // Cleared-but-not-yet-repainted pixels are transparent; storing them as backdrop
    // would let the blur average alpha 0 in and punch a see-through band.
    // 已清空但尚未重绘的像素是透明的；把它们当成背景存下，模糊会把 alpha 0 平均进来，
    // 打出一条可透视的带。
    if (u_skip_empty > 0.5 && s.a <= 0.0) {
        discard;
    }
    // The layer holds premultiplied colour, so scaling all channels stays valid.
    // 层内是预乘颜色，因此整体缩放所有通道仍然合法。
    frag = s * u_opacity * cov;
}
"#;

/// Composite a blurred backdrop as WinUI acrylic: luminosity, tint, then grain.
/// 把模糊后的背景按 WinUI acrylic 合成：亮度、着色、噪点。
pub const ACRYLIC_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_src;
uniform vec4 u_tint;        // straight rgb, alpha = TintOpacity x modifier
uniform vec4 u_luminosity;  // straight rgb, alpha = TintLuminosityOpacity
uniform float u_noise;      // grain layer opacity
out vec4 frag;

const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// Skia's luminosity/colour pair: keep hue and chroma, take the requested luminosity.
// Skia 的 luminosity/color 混合对：保留色相与饱和度，取指定亮度。
vec3 set_lum(vec3 c, float l) {
    vec3 d = c + vec3(l - dot(c, LUMA));
    float lo = min(min(d.r, d.g), d.b);
    float hi = max(max(d.r, d.g), d.b);
    float dl = dot(d, LUMA);
    if (lo < 0.0) {
        d = vec3(dl) + (d - vec3(dl)) * (dl / max(dl - lo, 1e-5));
    }
    if (hi > 1.0) {
        d = vec3(dl) + (d - vec3(dl)) * ((1.0 - dl) / max(hi - dl, 1e-5));
    }
    return clamp(d, 0.0, 1.0);
}

// Deterministic grain per device pixel; the reference tiles a 2% noise image instead.
// 逐设备像素的确定性噪点；参考实现改为平铺一张 2% 的噪点图。
float grain(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    float cov = coverage(v_device);
    if (cov <= 0.0) {
        discard;
    }
    vec4 s = texture(u_src, v_uv);
    if (s.a <= 0.0) {
        discard;
    }
    // The frame holds premultiplied encoded colour; the blend pair works on straight.
    // 帧里是预乘的编码值；这组混合运算需要直通值。
    vec3 c = s.rgb / s.a;
    c = mix(c, set_lum(c, dot(u_luminosity.rgb, LUMA)), u_luminosity.a);
    vec3 tinted = u_tint.a >= 0.999 ? u_tint.rgb : set_lum(u_tint.rgb, dot(c, LUMA));
    c = mix(c, tinted, u_tint.a);
    c = mix(c, vec3(grain(v_device)), u_noise);
    frag = vec4(c * s.a, s.a) * cov;
}
"#;

/// Composite a blurred backdrop as glass, ported from the reference's indicator shader.
/// 把模糊后的背景按玻璃合成，移植自参考实现指示器着色器。
///
/// Source: `liquid_glass_widgets/shaders/interactive_indicator.frag:113-439`. The reference
/// samples a captured image with its own origin; we sample the backdrop layer by device
/// position, so every offset here is a device-pixel delta on `v_device`.
/// 来源：`liquid_glass_widgets/shaders/interactive_indicator.frag:113-439`。参考用带原点的
/// 截图取样；我们按设备位置取背景层，因此这里的每个位移都是 `v_device` 上的设备像素差。
pub const GLASS_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_src;
uniform vec2 u_viewport;
uniform vec4 u_glass;       // glassColor, straight rgba
uniform vec4 u_rim_params;  // rimThickness(px), rimSmoothing, ambientRim, ambientStrength
uniform vec4 u_alpha;       // baseAlpha, edgeAlpha, glowIntensity, interaction
uniform vec4 u_light;       // direction.xy, intensity, saturation
uniform vec2 u_effects;     // chromaticAberration, edgeAbsorption
uniform float u_edge_zone;  // edgeZone in device pixels
out vec4 frag;

// Calibrated so the top rim reads brighter without hardening small pills.
// 标定值：顶部边缘更亮，同时不让小胶囊显得生硬。
const float BEVEL = 0.18;

// Straight colour of a premultiplied sample.
// 预乘样本的直通颜色。
vec3 straight(vec4 s) {
    return s.a > 0.0 ? s.rgb / s.a : vec3(0.0);
}

void main() {
    float cov = coverage(v_device);
    if (cov <= 0.0) {
        discard;
    }
    vec2 size = u_rect.zw;
    vec2 half_size = size * 0.5;
    vec2 p = v_device - (u_rect.xy + half_size);
    float dist = sd_round_box(p, half_size, u_radius, u_exp);
    float dist_edge = abs(dist);
    // Our corners are per-corner; a capsule is uniform, so the largest radius drives the
    // skeleton. A non-uniform shape keeps the correct SDF and only its normal is approximate.
    // 我们的圆角是逐角的；胶囊四角相同，取最大半径构造骨架。非均匀形状的 SDF 仍正确，
    // 只有法线是近似。
    float corner = max(max(u_radius.x, u_radius.y), max(u_radius.z, u_radius.w));
    vec2 inner = max(half_size - corner, vec2(0.0));
    vec2 closest = clamp(p, -inner, inner);
    vec2 to_edge = p - closest;
    float edge_len = length(to_edge);
    vec2 normal = edge_len > 0.001 ? to_edge / edge_len : vec2(0.0);

    // Edge bend: quadratic falloff reaching `edge_zone` inward, scaled by interaction.
    // 边缘弯折：二次衰减伸入 `edge_zone`，随交互强度放大。
    float influence = smoothstep(u_edge_zone, 0.0, dist_edge);
    influence *= influence;
    float bend = 0.9 * (0.5 + u_alpha.w * 0.7);
    vec2 offset = normal * influence * bend * size.y * 0.35;
    vec2 split = normal * influence * u_effects.x * 0.12;

    vec4 flat_sample = texture(u_src, v_uv);
    if (flat_sample.a <= 0.0) {
        discard;
    }
    vec2 uv = v_device / u_viewport;
    vec2 bend_uv = offset / u_viewport;
    vec2 split_uv = split / u_viewport;
    vec2 tapped = v_uv + bend_uv;
    // A tap on unpainted pixels keeps the unrefracted colour.
    // 落在未绘制像素上的取样保留未折射的颜色。
    vec4 mid = texture(u_src, clamp(tapped, 0.0, 1.0));
    vec4 picked = mid.a > 0.0 ? mid : flat_sample;
    vec3 bg;
    vec4 red = texture(u_src, clamp(tapped + split_uv, 0.0, 1.0));
    vec4 blue = texture(u_src, clamp(tapped - split_uv, 0.0, 1.0));
    bg.r = straight(red.a > 0.0 ? red : flat_sample).r;
    bg.g = straight(picked).g;
    bg.b = straight(blue.a > 0.0 ? blue : flat_sample).b;

    // Key light on the lit edge, kick light on the opposite one (x^8 and x^12 by squares).
    // 受光边缘的主高光与对侧的反光高光（用平方连乘代替 8 次、12 次幂）。
    float facing = dot(normal, u_light.xy);
    float lc = max(facing, 0.0);
    float lc2 = lc * lc;
    float lc4 = lc2 * lc2;
    float key = lc4 * lc4 * u_light.z * 0.5;
    float kc = max(-facing, 0.0);
    float kc2 = kc * kc;
    float kc4 = kc2 * kc2;
    float kc8 = kc4 * kc4;
    float kick = kc8 * kc4 * u_light.z * 0.5;
    float rim_brightness = u_rim_params.z + key + kick;

    // Fresnel glow: edges read slightly brighter than the body.
    // 菲涅尔辉光：边缘比本体略亮。
    vec2 radial = p / max(half_size, vec2(0.001));
    float fresnel = dot(radial, radial) * 0.25;

    // Hairline rim, brightened at the top by the synthetic bevel gradient.
    // 发丝边，顶部由合成倒角梯度提亮。
    float border = 1.0 - smoothstep(0.0, u_rim_params.y, dist_edge - u_rim_params.x);
    float bevel = -normal.y * BEVEL;
    float bright = max(0.0, rim_brightness + bevel * border);
    vec3 rim_colour = vec3(bright) * (u_rim_params.z * 10.0);

    vec3 colour = bg * u_light.w;
    colour += rim_colour * border * 1.5;
    colour += vec3(fresnel * u_alpha.z);
    colour += vec3(u_rim_params.w);
    colour = mix(colour, colour + u_glass.rgb * 0.2, u_glass.a);

    // Meniscus absorption: a hemisphere profile, weaker on the lit side where the
    // specular energy masks it.
    // 半月板吸收：半球剖面，受光侧因高光补偿而更弱。
    float r_norm = clamp(dist_edge / max(u_edge_zone, 0.001), 0.0, 1.0);
    float lens = sqrt(max(0.0, 1.0 - r_norm * r_norm));
    float lit = mix(1.4, 0.6, facing * 0.5 + 0.5);
    colour *= max(0.0, 1.0 - lens * (u_effects.y * lit));
    colour = min(colour, vec3(1.2));

    // Over a captured backdrop the body is translucent glass, not the multiplier.
    // 在有截图背景时，本体是半透明玻璃，而不是那个乘数。
    float glass_alpha = mix(0.70, 0.95, influence);
    float ring = border * 0.9 * clamp(u_rim_params.z * 10.0, 0.0, 1.0);
    glass_alpha = max(glass_alpha, ring);
    float alpha = glass_alpha * flat_sample.a * cov;
    frag = vec4(colour * alpha, alpha);
}
"#;

/// The premium material: a real normal field refracts the backdrop, then Fresnel, specular
/// rim light and meniscus darkening sit on top (`liquid_glass_render.frag:308-940`).
/// Premium 材质：用真实法线场折射背景，再叠菲涅尔、镜面边缘光与半月板变暗。
///
/// The reference runs this in two passes because Impeller cannot read back its own SDF; we
/// evaluate the same normal analytically, so the geometry texture is not needed here.
/// 参考把它拆成两趟是因为 Impeller 读不回自己的 SDF；我们在同一片元里解析地求同一个法线，
/// 因此这里不需要几何纹理。
pub const PREMIUM_GLASS_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
in vec2 v_device;
uniform sampler2D u_src;
uniform vec2 u_viewport;
uniform vec4 u_glass;     // glassColor, straight rgba
uniform vec4 u_optical;   // refractiveIndex, chromaticAberration, thickness(px), refractScale
uniform vec4 u_light;     // direction.xy, intensity, saturation
uniform vec4 u_ambient;   // ambientStrength, ambientRim(px), fresnelStrength, whiten
uniform vec4 u_edge;      // dprScale, edgeAbsorption, rimShade, rimShadeEnds
uniform vec4 u_blend;     // live shape count, smooth-union radius k, lens model, device scale
uniform vec4 u_lens;      // magnification, 0, 0, 0
uniform vec4 u_shape[4];  // centre.xy, half.xy of each fused shape, device pixels
uniform vec4 u_shape_radius[4]; // tl, tr, br, bl of each fused shape, device pixels
out vec4 frag;

const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// Straight colour of a premultiplied sample.
// 预乘样本的直通颜色。
vec3 straight(vec4 s) {
    return s.a > 0.0 ? s.rgb / s.a : vec3(0.0);
}

// A highlight tinted by what it sits over: neutral on grey, coloured on colour.
// 被其底下的内容着色的高光：灰底取中性白，彩底则带同色。
vec3 highlight_colour(vec3 bg, float target) {
    float lum = dot(bg, LUMA);
    float peak = max(max(bg.r, bg.g), bg.b);
    float l = lum * 2.5;
    float s = peak * 2.5;
    vec3 tinted = bg / max(lum, 0.001) * target;
    return mix(vec3(target), tinted, (l / (1.0 + l)) * (s / (1.0 + s)));
}

// Rec. 709 luminance-preserving saturation.
// Rec.709 保亮度饱和度。
vec3 saturate_colour(vec3 c, float s) {
    return clamp(mix(vec3(dot(c, LUMA)), c, s), 0.0, 1.0);
}

// Branchless smooth union: at k = 0 this is exactly min(d1, d2).
// 无分支平滑并集：k = 0 时恰好退化为 min(d1, d2)。
float smooth_union(float d1, float d2, float k) {
    float e = max(k - abs(d1 - d2), 0.0);
    return min(d1, d2) - e * e * 0.25 / max(k, 1e-5);
}

// Distance to the fused group. The node's rect only bounds the capture; the union is
// built from the shapes alone, so a group may reach past the sliding node's own box.
// 到融合组的距离。节点矩形只用来圈定捕获范围；并集只由各形状构成，因此融合可以超出节点盒子。
//
// Forward and backward folds averaged, unrolled: that cancels the order bias one fold
// shows from three shapes up (`sdf.glsl:236-300`), and no dynamic index survives.
// 正反两向折叠取平均并展开：三个形状起，单向折叠的顺序偏差因此消失，也不再需要动态下标。
float group_sdf(vec2 device) {
    float s0 = sd_round_box(device - u_shape[0].xy, u_shape[0].zw, u_shape_radius[0], u_exp);
    if (u_blend.x < 1.5) {
        return s0;
    }
    float k = u_blend.y;
    float s1 = sd_round_box(device - u_shape[1].xy, u_shape[1].zw, u_shape_radius[1], u_exp);
    if (u_blend.x < 2.5) {
        return smooth_union(s0, s1, k);
    }
    float s2 = sd_round_box(device - u_shape[2].xy, u_shape[2].zw, u_shape_radius[2], u_exp);
    float fwd = smooth_union(smooth_union(s0, s1, k), s2, k);
    float bwd = smooth_union(smooth_union(s2, s1, k), s0, k);
    if (u_blend.x < 3.5) {
        return mix(fwd, bwd, 0.5);
    }
    float s3 = sd_round_box(device - u_shape[3].xy, u_shape[3].zw, u_shape_radius[3], u_exp);
    fwd = smooth_union(fwd, s3, k);
    bwd = smooth_union(smooth_union(smooth_union(s3, s2, k), s1, k), s0, k);
    return mix(fwd, bwd, 0.5);
}

void main() {
    vec2 size = u_rect.zw;
    vec2 half_size = size * 0.5;
    vec2 p = v_device - (u_rect.xy + half_size);
    bool grouped = u_blend.x > 0.5;
    float dist = grouped ? group_sdf(v_device)
                         : sd_round_box(p, half_size, u_radius, u_exp);
    // A fused surface is not cut to the node's rect: its coverage is the union's own.
    // 融合后的表面不按节点矩形切边：覆盖度由并集自身给出。
    float cov = grouped ? clamp(AA - dist, 0.0, 1.0) : coverage(v_device);
    if (cov <= 0.0) {
        discard;
    }

    // Outward normal of the level set, zero on the flat interior.
    // 等值线的外法线，平面内部为零。
    vec2 n2;
    if (grouped) {
        // The union's normal belongs to neither shape: take the field's own gradient.
        // 并集的法线不属于任何单个形状：取场自身的梯度。
        float e = 0.5;
        vec2 g = vec2(
            group_sdf(v_device + vec2(e, 0.0)) - group_sdf(v_device - vec2(e, 0.0)),
            group_sdf(v_device + vec2(0.0, e)) - group_sdf(v_device - vec2(0.0, e)));
        n2 = dot(g, g) > 1e-8 ? normalize(g) : vec2(0.0);
    } else {
        float corner = max(max(u_radius.x, u_radius.y), max(u_radius.z, u_radius.w));
        vec2 inner = max(half_size - corner, vec2(0.0));
        vec2 to_edge = p - clamp(p, -inner, inner);
        float elen = length(to_edge);
        n2 = elen > 0.001 ? to_edge / elen : vec2(0.0);
    }

    // Quarter-circle bevel: grazing at the rim, flat `thickness` inside. `sin_i` is the
    // encoded height the reference stores in its geometry texture.
    // 四分之一圆倒角：边缘掠射，深入 `thickness` 后为平面。`sin_i` 就是参考几何纹理里的高度。
    float thickness = max(u_optical.z, 0.001);
    float rim_dist = max(-dist, 0.0);
    float u = clamp(rim_dist / thickness, 0.0, 1.0);
    // |grad h| = (1 - u) / sqrt(u(2 - u)), so sin = |grad| / sqrt(1 + |grad|^2) = 1 - u.
    // |grad h| = (1 - u) / sqrt(u(2 - u))，故 sin = |grad| / sqrt(1 + |grad|²) = 1 - u。
    float sin_i = 1.0 - u;
    float cos_i = sqrt(max(0.0, u * (2.0 - u)));
    vec3 normal = normalize(vec3(n2 * sin_i, max(cos_i, 0.001)));

    // Refraction of the backdrop, in the reference's two lens models.
    // 背景折射，走参考的两套透镜模型。
    vec2 disp;
    if (u_blend.z > 0.5) {
        // LiquidGlassKit's thumb: the backdrop is captured over `1 / m` of the shape's own
        // size, so the face shows an m× copy of what is behind it. Only the rim bends, by
        // the thin-prism deviation `-tan(t - i)` with `i = asin((1 - d / T)²)`.
        // LiquidGlassKit 的滑块：背景按形状自身尺寸的 `1 / m` 范围捕获，面上因此是背后画面的
        // m 倍。只有边缘弯折，弯折量取薄棱镜偏角 `-tan(t - i)`，其中 `i = asin((1 - d / T)²)`。
        float mag = clamp(u_lens.x, 0.05, 4.0);
        float depth_ratio = clamp(1.0 - rim_dist / thickness, 0.0, 1.0);
        float incident = asin(clamp(depth_ratio * depth_ratio, 0.0, 1.0));
        float transmitted = asin(clamp(sin(incident) / max(u_optical.x, 1.001), 0.0, 1.0));
        float bend = rim_dist >= thickness ? 0.0 : -tan(transmitted - incident);
        disp = p * (1.0 / mag - 1.0)
             - n2 * bend * 0.05 * u_blend.w * size.y * u_optical.w;
    } else {
        vec3 bent = refract(vec3(0.0, 0.0, -1.0), normal, 1.0 / max(u_optical.x, 0.001));
        float travel = (cos_i * thickness + thickness * 8.0) / max(0.001, abs(bent.z));
        disp = bent.xy * travel * u_optical.w;
    }
    // A lens cannot show what lies beyond its far side: hold the reach inside the shape.
    // 透镜看不到自身远侧之外：把采样距离压在形状内。
    float max_reach = 0.5 * min(size.x, size.y);
    float reach = length(disp);
    if (reach > max_reach) {
        disp *= max_reach / max(reach, 0.001);
    }

    // iOS 27 hairline: a ~2 px line on the very edge that shows the backdrop unrefracted,
    // untinted, and a fixed step darker. It samples just outside the shape.
    // iOS 27 发丝描边：贴在最外缘约 2px 的一圈，显示未折射、未着色的背景，并压暗一个定值。
    // 它在形状外侧取样。
    float dpr = max(0.1, u_blend.w);
    float hairline = 0.0;
    if (u_edge.z > 0.001) {
        hairline = 1.0 - smoothstep(0.75 * dpr, 2.0 * dpr, rim_dist);
        // `rim_dist` saturates on the flat face, which on thin glass sits inside the line.
        // `rim_dist` 在平面处饱和，薄玻璃上那一点就落在描边宽度之内。
        hairline *= 1.0 - step(0.999, cos_i);
        disp = mix(disp, n2 * (2.5 * dpr), hairline);
    }

    vec2 duv = disp / u_viewport;
    vec4 mid = texture(u_src, clamp(v_uv + duv, 0.0, 1.0));
    vec3 bg;
    if (u_optical.y < 0.01) {
        bg = straight(mid);
    } else {
        float s = u_optical.y * 0.5;
        vec4 red = texture(u_src, clamp(v_uv + duv * (1.0 + s), 0.0, 1.0));
        vec4 blue = texture(u_src, clamp(v_uv + duv * (1.0 - s), 0.0, 1.0));
        bg = vec3(straight(red).r, straight(mid).g, straight(blue).b);
    }

    // Luminosity-preserving tint: the hue moves toward the glass colour, the brightness
    // stays the backdrop's, so the body can never read as a flat wash.
    // 保亮度的着色：色相向玻璃色偏移、亮度仍是背景的，本体因此不会读成一块平涂。
    float bg_luma = dot(bg, LUMA);
    vec3 tinted = clamp(u_glass.rgb + (bg_luma - dot(u_glass.rgb, LUMA)), 0.0, 1.0);
    float chroma = max(max(u_glass.r, u_glass.g), u_glass.b)
                 - min(min(u_glass.r, u_glass.g), u_glass.b);
    vec3 direct = mix(bg, u_glass.rgb, u_glass.a);
    vec3 colour = mix(direct, mix(bg, tinted, u_glass.a), clamp(chroma * 8.0, 0.0, 1.0));

    // Content-adaptive strength: a dark backdrop reads as richer glass.
    // 内容自适应：背景越暗，玻璃越浓。
    float adaptive = mix(1.2, 0.8, bg_luma);
    colour = saturate_colour(colour, u_light.w * adaptive);
    colour = mix(colour, u_glass.rgb, u_glass.a * 0.12 * (adaptive - 1.0));

    // Whitening veil, gated by luminance so darks underneath stay dark.
    // 漂白罩，按亮度门控，使底下的暗部保持暗。
    float veil = smoothstep(0.40, 0.80, dot(colour, LUMA));
    colour = mix(colour, vec3(1.0), clamp(u_ambient.w, 0.0, 1.0) * veil);
    // The hairline shows the backdrop as it is: no tint, no lift, no specular.
    // 发丝描边显示的是原样背景：不着色、不抬亮、不打高光。
    colour = mix(colour, bg, hairline);

    // Meniscus: a hemisphere profile absorbs most where the glass is thickest, and less
    // on the lit side where the specular energy masks it.
    // 半月板：半球剖面在最厚处吸收最多，受光侧因高光补偿而更弱。
    float r_rim = clamp(1.0 - cos_i, 0.0, 1.0);
    float rim_thickness = 1.0 - sqrt(max(0.0, 1.0 - r_rim * r_rim));
    float lit = mix(1.4, 0.6, dot(n2, u_light.xy) * 0.5 + 0.5);
    colour *= max(0.0, 1.0 - sqrt(rim_thickness) * u_edge.y * lit);

    // Edge lighting: the thin specular lobe the reference calibrated on a 3× display.
    // 边缘受光：参考在 3 倍屏上标定的那条细高光。
    float thickness_scale = clamp(40.0 * max(0.1, u_edge.x) / max(thickness, 1.0), 1.0, 4.0);
    float edge_threshold = mix(0.8, 0.5, 1.0 / thickness_scale);
    float edge_factor = 1.0 - smoothstep(0.0, edge_threshold, cos_i);
    if (edge_factor > 0.01 && (u_light.z > 0.0 || u_ambient.x > 0.0)) {
        float along = dot(n2, u_light.xy);
        float main_l = max(along, 0.0);
        float opp_l = max(-along, 0.0);
        float total = main_l + opp_l * 0.8;
        float raw = (total * sqrt(total) * u_light.z * 3.0 + u_ambient.x * 0.5)
                  * edge_factor * thickness_scale * 0.8 * (1.0 - hairline);
        colour = mix(colour, highlight_colour(bg, 1.0), raw / (1.0 + raw));
    }

    // Fresnel ring: a hairline band hugging the rim, brighter than the body.
    // 菲涅尔环：贴边的一条亮带，比本体更亮。
    float ring_window = 0.75 * max(0.1, u_edge.x);
    float rim_at = max(u_ambient.y, 0.0);
    float ring = (1.0 - smoothstep(rim_at - ring_window, rim_at + ring_window, rim_dist))
               * step(0.001, rim_at);
    float fresnel = ((1.0 - cos_i) * edge_factor * 0.12 * u_ambient.z + ring * 0.45)
                  * (1.0 - hairline);
    colour = clamp(colour + vec3(fresnel), 0.0, 1.0);

    // iOS 27 hairline darkening: a fixed step below the backdrop, strongest across the
    // light axis and weakest at its ends, subtracted rather than blended.
    // iOS 27 描边压暗：背景下方的一个定值台阶，横跨光轴处最强、轴端最弱，用减法而非混合。
    if (hairline > 0.0) {
        float along = dot(n2, u_light.xy);
        float across = pow(max(1.0 - along * along, 0.0), 1.75);
        float shade = u_edge.z * 0.314 * mix(u_edge.w, 1.0, across);
        colour = max(colour - vec3(shade * hairline), 0.0);
    }

    frag = vec4(colour * cov, cov);
}
"#;

/// Full-screen pass: one separable convolution tap row plus a colour matrix.
/// 全屏一趟：一行可分离卷积加上一个颜色矩阵。
///
/// `u_dir` picks the axis, so the same program runs the horizontal and the vertical pass;
/// `u_color` is the category's second operation, applied after the convolution.
/// `u_dir` 选择方向，因此同一个程序跑横向与纵向两趟；`u_color` 是类别的第二个操作，
/// 在卷积之后应用。
pub const EFFECT_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_texel;      // 1 / texture size
uniform vec2 u_dir;        // (1,0) horizontal, (0,1) vertical, (0,0) single tap
uniform int u_taps;        // weights in use, odd
uniform float u_weights[33];
uniform mat4 u_color;
out vec4 frag;
void main() {
    int radius = (u_taps - 1) / 2;
    vec4 acc = vec4(0.0);
    for (int i = 0; i < 33; i++) {
        if (i < u_taps) {
            vec2 off = u_dir * u_texel * float(i - radius);
            // CLAMP_TO_EDGE matches the CPU path's border clamp.
            // CLAMP_TO_EDGE 与 CPU 路径的边界钳制一致。
            acc += texture(u_src, v_uv + off) * u_weights[i];
        }
    }
    frag = u_color * acc;
}
"#;

/// Full-screen pass: one sliding-window box average along `u_dir`.
/// 全屏一趟：沿 `u_dir` 的一趟滑动窗口箱式平均。
///
/// GLSL cannot carry a running sum across pixels, so the window is summed per fragment.
/// The cost is therefore O(radius) in the shader rather than O(1); three such passes still
/// beat an explicit 85-tap kernel and, more importantly, are not limited by `u_weights`.
/// GLSL 无法跨像素携带运行和，因此窗口在每个片元内求和。着色器里的代价是 O(radius)
/// 而非 O(1)；但三趟仍优于显式 85-tap 核，更重要的是不受 `u_weights` 长度限制。
pub const BOX_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_texel;
uniform vec2 u_dir;
uniform int u_radius;
uniform mat4 u_color;
out vec4 frag;
void main() {
    vec4 acc = vec4(0.0);
    float span = float(2 * u_radius + 1);
    // A uniform bound is legal here and keeps the cost at one tap per tap.
    // 这里用 uniform 作上界是合法的，能让代价与 tap 数成正比。
    for (int i = -u_radius; i <= u_radius; i++) {
        vec2 off = u_dir * u_texel * float(i);
        // CLAMP_TO_EDGE keeps the edge pixels weighted like the CPU path does.
        // CLAMP_TO_EDGE 让边缘像素的权重与 CPU 路径一致。
        acc += texture(u_src, v_uv + off);
    }
    frag = u_color * (acc / span);
}
"#;

/// Fragment source with the shared clip block spliced in after `precision`.
/// 把共享裁剪块插到 `precision` 之后的片元源串。
///
/// `precision` must come first: GLSL ES fragment shaders have no default float precision.
/// `precision` 必须最先出现：GLSL ES 片元着色器没有默认 float 精度。
pub(crate) fn splice(body: &str, block: &str) -> String {
    body.replacen(
        "precision highp float;",
        &format!("precision highp float;\n{block}"),
        1,
    )
}

fn with_clip(body: &str) -> String {
    splice(body, CLIP_GLSL)
}

fn with_coverage(body: &str) -> String {
    splice(body, COVERAGE_GLSL)
}

fn with_sdf(body: &str) -> String {
    splice(body, SDF_GLSL)
}

pub fn rect_fragment() -> String {
    with_sdf(&with_clip(RECT_FRAGMENT))
}

/// Floats per arc instance: centre, geometry, fill, caps.
/// 每段弧实例的浮点数：圆心、几何、填充、帽。
pub const SECTOR_STRIDE: i32 = 11;

pub const SECTOR_VERTEX: &str = r#"
precision highp float;
in vec2 a_unit;
in vec2 a_centre;   // device pixels
in vec4 a_geom;     // outer radius, inner radius, start turns, sweep turns
in vec4 a_fill;     // straight rgb + alpha
in float a_caps;    // 1 = round caps at both ends
uniform vec2 u_viewport;
out vec2 v_device;
out vec2 v_centre;
out vec4 v_geom;
out vec4 v_fill;
out float v_caps;
void main() {
    // Reaches `AA` past the outer radius: the feather also covers pixels centred outside it.
    // 向外缘之外多伸 `AA`：中心恰在外缘之外的像素同样有羽化覆盖。
    vec2 device = a_centre + (a_unit * 2.0 - 1.0) * (a_geom.x + AA);
    vec2 ndc = vec2(
        device.x / u_viewport.x * 2.0 - 1.0,
        device.y / u_viewport.y * 2.0 - 1.0
    );
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_device = device;
    v_centre = a_centre;
    v_geom = a_geom;
    v_fill = a_fill;
    v_caps = a_caps;
}
"#;

pub const SECTOR_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_device;
in vec2 v_centre;
in vec4 v_geom;
in vec4 v_fill;
in float v_caps;
out vec4 frag;

const float TAU = 6.28318530718;

// Feather of one device pixel against a signed distance inside the stroke.
// 以有符号距离对一个设备像素的羽化。
float feather(float d) {
    return clamp(AA - d, 0.0, 1.0);
}

void main() {
    float r_out = v_geom.x;
    float r_in = v_geom.y;
    float start = v_geom.z;
    float sweep = v_geom.w;
    vec2 p = v_device - v_centre;
    float d = length(p);
    // Annulus first: the same level set the CPU rasteriser uses.
    // 先算环带：与 CPU 光栅化器相同的等值线。
    float ring = max(r_in - d, d - r_out);
    float cov;
    if (sweep >= 1.0) {
        cov = feather(ring);
    } else {
        // Angular distance measured along the arc, in device pixels at this radius.
        // 沿弧度量得的角度距离，按该半径换算成设备像素。
        float turns = fract(atan(p.y, p.x) / TAU);
        float rel = fract(turns - start);
        float arc = TAU * d;
        float angle = (rel <= sweep)
            ? -min(rel, sweep - rel) * arc
            : min(rel - sweep, 1.0 - rel) * arc;
        cov = feather(max(ring, angle));
    }
    // Round caps are circles on the stroke's midline at both end points.
    // 圆头帽是两端端点处、描边中线上的圆。
    if (v_caps > 0.5) {
        float half_w = (r_out - r_in) * 0.5;
        float mid = r_out - half_w;
        vec2 a = vec2(cos(start * TAU), sin(start * TAU));
        vec2 b = vec2(cos((start + sweep) * TAU), sin((start + sweep) * TAU));
        float dc = min(length(p - mid * a), length(p - mid * b));
        cov = max(cov, feather(dc - half_w));
    }
    float a = v_fill.a * cov * clip_coverage(v_device);
    if (a <= 0.0) {
        discard;
    }
    frag = vec4(v_fill.rgb * a, a);
}
"#;

pub fn sector_fragment() -> String {
    with_clip(SECTOR_FRAGMENT)
}

pub fn glyph_fragment() -> String {
    with_clip(GLYPH_FRAGMENT)
}

pub fn shadow_fragment() -> String {
    with_sdf(&with_clip(SHADOW_FRAGMENT))
}

pub fn image_fragment() -> String {
    with_sdf(&with_clip(IMAGE_FRAGMENT))
}

pub fn layer_fragment() -> String {
    with_coverage(LAYER_FRAGMENT)
}

pub fn acrylic_fragment() -> String {
    with_coverage(ACRYLIC_FRAGMENT)
}

pub fn glass_fragment() -> String {
    with_coverage(&with_sdf(GLASS_FRAGMENT))
}

pub fn premium_glass_fragment() -> String {
    with_coverage(&with_sdf(PREMIUM_GLASS_FRAGMENT))
}

/// Output transform: sRGB-encoded premultiplied in, PQ-encoded premultiplied out.
/// 输出变换：输入是 sRGB 编码的预乘值，输出是 PQ 编码的预乘值。
pub const OUTPUT_FRAGMENT: &str = r#"
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform float u_scale;
uniform float u_offset;
uniform mat3 u_gamut;
out vec4 frag;

float srgb_decode(float c) {
    return c <= 0.040449237 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float pq_encode(float l) {
    const float m1 = 2610.0 / 16384.0;
    const float m2 = 2523.0 / 32.0;
    const float c1 = 3424.0 / 4096.0;
    const float c2 = 2413.0 / 128.0;
    const float c3 = 2392.0 / 128.0;
    float p = pow(l, m1);
    return pow((c1 + c2 * p) / (1.0 + c3 * p), m2);
}

void main() {
    vec4 c = texture(u_src, v_uv);
    if (c.a <= 0.0) {
        frag = vec4(0.0);
        return;
    }
    // Premultiplied encoded values cannot be decoded directly; un-premultiply first.
    // 预乘的编码值不能直接解码，先解预乘。
    vec3 straight = c.rgb / c.a;
    vec3 lin = vec3(srgb_decode(straight.r), srgb_decode(straight.g), srgb_decode(straight.b));
    // sRGB -> BT.2020 in linear light; the surface is declared as BT.2020.
    // 线性光下 sRGB -> BT.2020；该 surface 声明为 BT.2020。
    lin = u_gamut * lin;
    vec3 code = vec3(pq_encode(clamp(u_scale * lin.r + u_offset, 0.0, 1.0)),
                     pq_encode(clamp(u_scale * lin.g + u_offset, 0.0, 1.0)),
                     pq_encode(clamp(u_scale * lin.b + u_offset, 0.0, 1.0)));
    frag = vec4(code * c.a, c.a);
}
"#;
