//! Draws a `ui_core::DrawList` with OpenGL ES.
//! 用 OpenGL ES 绘制 `ui_core::DrawList`。
//!
//! Rectangles use an instanced signed-distance shader; glyphs sample a coverage
//! atlas. Both emit premultiplied alpha so one blend equation covers both.
//! 矩形用实例化的有符号距离着色器；字形采样覆盖率图集。两者都输出预乘 alpha，
//! 因此一个混合方程即可覆盖两者。

use glow::HasContext;
use std::collections::{HashMap, HashSet};
use ui_core::draw::{DrawList, GlyphPrim, ImageEntry, ImagePrim, PrimKind, RectPrim, ShadowPrim};
use ui_core::effect::Effect;
use ui_core::geom::Color;

use crate::material::BackdropMaterial;

/// Which program shades a blurred backdrop.
/// 用哪个程序给模糊后的背景着色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shade {
    Plain,
    Acrylic,
    Glass,
}

/// Which material a backdrop entry is shaded with: its own if it names one, else the
/// renderer-wide default.
/// 背景条目用哪种材质着色：自己指定的优先，否则用渲染器级的默认值。
pub(crate) fn resolve(
    entry: BackdropMaterial,
    default: Option<BackdropMaterial>,
) -> Option<BackdropMaterial> {
    match entry {
        BackdropMaterial::Plain => default,
        named => Some(named),
    }
}

impl Shade {
    /// The plain blit is the fallback for "no material" and for an explicit `Plain`.
    /// 纯写回既是「没有材质」的退路，也是显式 `Plain` 的结果。
    pub(crate) fn of(material: Option<BackdropMaterial>) -> Self {
        match material {
            None | Some(BackdropMaterial::Plain) => Shade::Plain,
            Some(BackdropMaterial::Acrylic(_)) => Shade::Acrylic,
            Some(BackdropMaterial::Glass(_)) => Shade::Glass,
        }
    }
}
use crate::shader;
use ui_gpu_egl::GpuSurface;
use crate::GpuError;

/// Counters for one frame.
/// 单帧的计数器。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    pub rects: u32,
    pub glyphs: u32,
    pub draw_calls: u32,
    /// Frosts executed this frame.
    /// 本帧执行的磨砂次数。
    pub frosts: u32,
    /// Backdrop passes that also shaded a material (acrylic or glass) after the blur.
    /// 模糊之后又跑了材质（acrylic 或玻璃）的背景趟数。
    pub material_passes: u32,
    /// Image textures uploaded this frame; a cached `ImageEntry::id` does not count.
    /// 本帧上传的图像纹理数；命中缓存的 `ImageEntry::id` 不计。
    pub image_uploads: u32,
    /// Soft shadows drawn this frame.
    /// 本帧绘制的柔和阴影数。
    pub shadows_drawn: u32,
    /// Image primitives drawn this frame.
    /// 本帧绘制的图像图元数。
    pub images_drawn: u32,
}

/// A coverage atlas uploaded to the GPU.
/// 已上传到 GPU 的覆盖率图集。
pub struct AtlasUpload {
    pub width: u32,
    pub height: u32,
    /// Single-channel coverage.
    /// 单通道覆盖率。
    pub data: Vec<u8>,
}

pub(crate) struct Program {
    pub(crate) program: glow::Program,
    pub(crate) vao: glow::VertexArray,
    unit_vbo: glow::Buffer,
    pub(crate) instance_vbo: glow::Buffer,
    pub(crate) uniforms: Uniforms,
}

pub(crate) struct Uniforms {
    pub(crate) viewport: Option<glow::UniformLocation>,
    atlas_size: Option<glow::UniformLocation>,
    atlas: Option<glow::UniformLocation>,
    color_atlas_size: Option<glow::UniformLocation>,
    color_atlas: Option<glow::UniformLocation>,
    image: Option<glow::UniformLocation>,
    opacity: Option<glow::UniformLocation>,
    /// Sampler name differs per program: glyphs use `u_atlas`, layers `u_layer`.
    /// 采样器名随程序不同：字形用 `u_atlas`，层用 `u_layer`。
    layer_tex: Option<glow::UniformLocation>,
    clip_rect: Option<glow::UniformLocation>,
    clip_radius: Option<glow::UniformLocation>,
    clip_shape: Option<glow::UniformLocation>,
    clip_on: Option<glow::UniformLocation>,
    /// Effect pass inputs; `None` for programs that do not run one.
    /// 效果趟的输入；不跑效果的程序为 `None`。
    pub(crate) src: Option<glow::UniformLocation>,
    pub(crate) texel: Option<glow::UniformLocation>,
    pub(crate) dir: Option<glow::UniformLocation>,
    pub(crate) taps: Option<glow::UniformLocation>,
    /// Sliding-window radius, used by the box program instead of `u_weights`.
    /// 滑动窗口半径，由箱式程序使用，代替 `u_weights`。
    pub(crate) radius: Option<glow::UniformLocation>,
    pub(crate) weights: Option<glow::UniformLocation>,
    pub(crate) color: Option<glow::UniformLocation>,
    /// Output transform inputs; `None` for programs that do not encode.
    /// 输出变换的输入；不编码的程序为 `None`。
    pub(crate) scale: Option<glow::UniformLocation>,
    pub(crate) offset: Option<glow::UniformLocation>,
    pub(crate) gamut: Option<glow::UniformLocation>,
    layer_rect: Option<glow::UniformLocation>,
    layer_radius: Option<glow::UniformLocation>,
    layer_exp: Option<glow::UniformLocation>,
    layer_skip_empty: Option<glow::UniformLocation>,
    /// Material uniforms; `None` for programs that shade no backdrop.
    /// 材质 uniform；不着色背景的程序为 `None`。
    tint: Option<glow::UniformLocation>,
    luminosity: Option<glow::UniformLocation>,
    noise: Option<glow::UniformLocation>,
    glass: Option<glow::UniformLocation>,
    rim_params: Option<glow::UniformLocation>,
    alpha: Option<glow::UniformLocation>,
    light: Option<glow::UniformLocation>,
    effects: Option<glow::UniformLocation>,
    edge_zone: Option<glow::UniformLocation>,
    /// Premium material: refractive index, dispersion, thickness, refract scale.
    /// Premium 材质：折射率、色散、厚度、折射归一。
    optical: Option<glow::UniformLocation>,
    /// Premium material: ambient strength, ambient rim, Fresnel, whiten.
    /// Premium 材质：环境强度、贴边环光、菲涅尔、漂白。
    ambient: Option<glow::UniformLocation>,
    /// Premium material: device scale, meniscus absorption.
    /// Premium 材质：设备缩放、半月板吸收。
    edge: Option<glow::UniformLocation>,
    /// Blend group: live shape count and smooth-union radius.
    /// 融合组：有效形状数与平滑并集半径。
    blend: Option<glow::UniformLocation>,
    /// Lens: how much the backdrop under the face is shrunk.
    /// 透镜：面下背景缩小多少。
    lens: Option<glow::UniformLocation>,
    /// Blend group: each shape's centre and half size, device pixels.
    /// 融合组：每个形状的中心与半尺寸，设备像素。
    shapes: Option<glow::UniformLocation>,
    /// Blend group: each shape's four corner radii, device pixels.
    /// 融合组：每个形状的四个圆角半径，设备像素。
    shape_radius: Option<glow::UniformLocation>,
}

/// GPU painter.
/// GPU 绘制器。
/// One offscreen render target for a layer, reused across frames by depth.
/// 离屏层渲染目标，按深度跨帧复用。
pub(crate) struct LayerTarget {
    pub(crate) fbo: glow::Framebuffer,
    pub(crate) tex: glow::Texture,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Storage precision, so a format change reallocates instead of silently reusing.
    /// 存储精度；格式变化时会重新分配而不是静默复用。
    pub(crate) format: crate::effect_pass::TargetFormat,
}

/// One cached image texture; `id` and size decide whether the upload is reused.
/// 一张缓存的图像纹理；`id` 与尺寸决定是否复用已有上传。
struct ImageTexture {
    tex: glow::Texture,
    width: u32,
    height: u32,
}

/// The texture one `ImagePrim` samples, resolved for the current frame.
/// 一个 `ImagePrim` 采样的纹理，已在本帧解析好。
#[derive(Clone, Copy)]
struct FrameImage {
    tex: glow::Texture,
    width: u32,
    height: u32,
}

pub struct GpuRenderer {
    rects: Program,
    /// Stroked circular arcs, shaped analytically in the fragment.
    /// 描边圆弧，在片元中解析成形。
    sectors: Program,
    glyphs: Program,
    /// Rounded-rect texture sampling, one draw per run of primitives sharing a texture.
    /// 圆角矩形纹理采样，每段共享同一纹理的图元一次绘制。
    images: Program,
    /// Analytic soft shadows; no blur pass, so a shadow costs one draw.
    /// 解析柔和阴影；无模糊趟，因此一个阴影只花一次绘制。
    shadows: Program,
    layers: Program,
    /// Full-screen pass shared by every effect in the chain.
    /// 效果链中每个效果共用的全屏趟。
    effects: Program,
    /// Output transform into PQ, used only when HDR is declared.
    /// 输出变换为 PQ，仅在声明 HDR 时使用。
    output: Program,
    /// (scale, offset) for the PQ output transform; `None` keeps the buffer as sRGB.
    /// PQ 输出变换的（缩放、偏移）；`None` 表示缓冲保持 sRGB。
    hdr: Option<[f32; 2]>,
    /// Internal storage precision for the frame and every layer target.
    /// 帧与所有层目标的内部存储精度。
    target_format: crate::effect_pass::TargetFormat,
    /// sRGB -> output gamut, row-major; identity until a description is read.
    /// sRGB -> 输出色域，行主序；读到描述前是单位矩阵。
    gamut: [[f32; 3]; 3],
    output_scratch: Option<LayerTarget>,
    /// Device-space bounds per nesting depth: the only region worth filtering or compositing.
    /// 每层嵌套深度的设备空间边界：唯一值得滤波与合成的区域。
    layer_bounds: Vec<ui_core::geom::Rect>,
    /// Device-space corner radii per depth, mirroring `layer_bounds`.
    /// 每层深度的设备空间圆角半径，与 `layer_bounds` 对应。
    layer_radius: Vec<ui_core::geom::Corners>,
    /// Persistent frame buffer: the target rotates, so the accumulated frame lives here.
    /// 持久帧缓冲：目标会轮转，因此累积的画面放在这里。
    frame: Option<LayerTarget>,
    /// Root of the active layer chain; `end_layer` composites depth 0 here.
    /// 当前层链的根；`end_layer` 把深度 0 合成到这里。
    root_fbo: Option<glow::Framebuffer>,
    /// Domain every intermediate is allocated in; follows the output description.
    /// 所有中间产物分配时所用的域；跟随输出描述。
    working: ui_core::colour::Encoding,
    /// Ping-pong scratch per nesting depth. These hold no meaningful content between
    /// frames: each frame recomputes every stage from the live scene.
    /// 每个嵌套深度的乒乓暂存。帧间不含任何有意义内容：每帧都从当帧场景重算每个阶段。
    effect_scratch: Vec<crate::effect_pass::Scratch>,
    /// Sliding-window box pass, used for blurs whose radius exceeds the kernel array.
    /// 滑动窗口箱式趟，用于半径超出核数组长度的模糊。
    boxpass: Program,
    /// Acrylic composite over the blurred backdrop.
    /// 模糊背景之上的 acrylic 合成。
    acrylic: Program,
    /// Glass composite over the blurred backdrop.
    /// 模糊背景之上的玻璃合成。
    glass: Program,
    /// The refracting lens, used when a surface asks for `GlassQuality::Premium`.
    /// 折射透镜，表面要求 `GlassQuality::Premium` 时使用。
    glass_premium: Program,
    /// Style-level backdrop material; `None` keeps the plain blur blit.
    /// 风格级背景材质；`None` 保留纯模糊写回。
    material: Option<BackdropMaterial>,
    /// Scratch for backdrop blurs; kept apart from the layer scratch so a frost and a
    /// layer effect in the same frame cannot overwrite each other's intermediates.
    /// 背景模糊的暂存；与层暂存分开，使同帧的磨砂与层效果不会互相覆盖中间结果。
    backdrop_scratch: crate::effect_pass::Scratch,
    /// Offscreen targets indexed by nesting depth.
    /// 按嵌套深度索引的离屏目标。
    layer_targets: Vec<LayerTarget>,
    atlas_tex: Option<glow::Texture>,
    atlas_size: (u32, u32),
    color_atlas_tex: Option<glow::Texture>,
    color_atlas_size: (u32, u32),
    /// Uploaded image textures keyed by `ImageEntry::id`; ids absent from a frame are freed.
    /// 按 `ImageEntry::id` 缓存的图像纹理；某帧不再出现的 id 会被释放。
    image_textures: HashMap<u64, ImageTexture>,
    /// Textures resolved for the frame being drawn, parallel to `DrawList::images`.
    /// 正在绘制的那一帧解析出的纹理，与 `DrawList::images` 平行。
    frame_images: Vec<FrameImage>,
    scale: f32,
    stats: RenderStats,
}

impl GpuRenderer {
    /// Compile both programs and set up vertex arrays.
    /// 编译两个程序并配置顶点数组。
    pub fn new(gl: &glow::Context, is_gles: bool) -> Result<Self, GpuError> {
                let layers = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &shader::layer_fragment(),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_opacity",
                "u_layer",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_skip_empty",
            ],
        )?;
        let effects = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::EFFECT_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &["u_viewport", "u_src", "u_texel", "u_dir", "u_taps", "u_weights", "u_color"],
        )?;
        let boxpass = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::BOX_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &["u_viewport", "u_src", "u_texel", "u_dir", "u_radius", "u_color"],
        )?;
        let output = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::OUTPUT_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &["u_viewport", "u_src", "u_scale", "u_offset", "u_gamut"],
        )?;
        let rects = Program::new(
            gl,
            is_gles,
            shader::RECT_VERTEX,
            &shader::rect_fragment(),
            shader::RECT_STRIDE,
            &[
                ("a_rect", 4),
                ("a_radius", 4),
                ("a_fill", 4),
                ("a_border", 4),
                ("a_border2", 4),
                ("a_border_g", 1),
                ("a_shape", 1),
                ("a_fill2", 4),
                ("a_grad", 2),
                ("a_stops", 4),
                ("a_extent", 2),
                ("a_border_w", 1),
                ("a_notch", 2),
                ("a_flip", 2),
            ],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let sectors = Program::new(
            gl,
            is_gles,
            shader::SECTOR_VERTEX,
            &shader::sector_fragment(),
            shader::SECTOR_STRIDE,
            &[
                ("a_centre", 2),
                ("a_geom", 4),
                ("a_fill", 4),
                ("a_caps", 1),
            ],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let glyphs = Program::new(
            gl,
            is_gles,
            shader::GLYPH_VERTEX,
            &shader::glyph_fragment(),
            shader::GLYPH_STRIDE,
            &[("a_rect", 4), ("a_atlas", 4), ("a_color", 4), ("a_kind", 1)],
            &[
                "u_viewport",
                "u_atlas_size",
                "u_atlas",
                "u_color_atlas_size",
                "u_color_atlas",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let shadows = Program::new(
            gl,
            is_gles,
            shader::SHADOW_VERTEX,
            &shader::shadow_fragment(),
            shader::SHADOW_STRIDE,
            &[
                ("a_rect", 4),
                ("a_radius", 4),
                ("a_colour", 4),
                ("a_soft", 2),
                ("a_shape", 1),
            ],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let images = Program::new(
            gl,
            is_gles,
            shader::IMAGE_VERTEX,
            &shader::image_fragment(),
            shader::IMAGE_STRIDE,
            &[
                ("a_rect", 4),
                ("a_radius", 4),
                ("a_uv", 4),
                ("a_shape", 1),
            ],
            &[
                "u_viewport",
                "u_image",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let acrylic = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &shader::acrylic_fragment(),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_tint",
                "u_luminosity",
                "u_noise",
            ],
        )?;
        let glass = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &shader::glass_fragment(),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_glass",
                "u_rim_params",
                "u_alpha",
                "u_light",
                "u_effects",
                "u_edge_zone",
            ],
        )?;
        let glass_premium = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &shader::premium_glass_fragment(),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_glass",
                "u_optical",
                "u_light",
                "u_ambient",
                "u_edge",
                "u_blend",
                "u_shape[0]",
                "u_shape_radius[0]",
            ],
        )?;
        Ok(Self {
            rects,
            sectors,
            glyphs,
            images,
            shadows,
            layers,
            effects,
            boxpass,
            acrylic,
            glass,
            glass_premium,
            material: None,
            output,
            layer_bounds: Vec::new(),
            layer_radius: Vec::new(),
            hdr: None,
            target_format: crate::effect_pass::TargetFormat::Rgba8,
            gamut: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            output_scratch: None,
            frame: None,
            root_fbo: None,
            working: ui_core::colour::Encoding::SDR_PREMULTIPLIED,
            effect_scratch: Vec::new(),
            backdrop_scratch: Default::default(),
            layer_targets: Vec::new(),
            atlas_tex: None,
            atlas_size: (1, 1),
            color_atlas_tex: None,
            color_atlas_size: (1, 1),
            image_textures: HashMap::new(),
            frame_images: Vec::new(),
            scale: 1.0,
            stats: RenderStats::default(),
        })
    }

    /// Every frame now repaints the whole surface, so a presenter must treat the damage
    /// as full. This reports it explicitly instead of letting callers guess.
    /// 现在每帧都重画整个表面，因此呈现方必须按全量脏区处理。此方法显式报告这一点，
    /// 而不是让调用方去猜。
    pub fn repaints_everything(&self) -> bool {
        true
    }

    /// Device pixels per logical point.
    /// 每逻辑点对应的设备像素。
    /// Change the working domain; every cached intermediate becomes invalid.
    /// 切换工作域；所有缓存的中间产物随之失效。
    /// Turn on the PQ output transform; `None` writes sRGB straight through.
    /// 打开 PQ 输出变换；`None` 表示直接写 sRGB。
    pub fn set_hdr(&mut self, reference_nits: Option<(f32, f32)>) {
        self.hdr = reference_nits.map(|(reference, min)| {
            [
                ui_core::colour::reference_scale(reference),
                ui_core::colour::min_offset(min),
            ]
        });
    }

    /// Choose the internal storage precision; targets reallocate on the next frame.
    /// 选择内部存储精度；下一帧会重新分配目标。
    pub fn set_target_format(&mut self, format: crate::effect_pass::TargetFormat) {
        self.target_format = format;
    }

    /// Set the sRGB -> output gamut matrix, row-major.
    /// 设置 sRGB -> 输出色域的矩阵，行主序。
    pub fn set_gamut(&mut self, gamut: [[f32; 3]; 3]) {
        self.gamut = gamut;
    }

    pub fn set_working(&mut self, working: ui_core::colour::Encoding) {
        self.working = working;
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Install the style's backdrop material; `None` keeps the plain blur blit.
    /// 装入风格的背景材质；`None` 保留纯模糊写回。
    pub fn set_backdrop_material(&mut self, material: Option<BackdropMaterial>) {
        self.material = material;
    }

    /// Upload (or replace) the coverage atlas.
    /// 上传（或替换）覆盖率图集。
    pub fn set_atlas(&mut self, gl: &glow::Context, atlas: AtlasUpload) -> Result<(), GpuError> {
        upload_atlas(
            gl,
            &mut self.atlas_tex,
            atlas.width,
            atlas.height,
            glow::R8,
            glow::RED,
            &atlas.data,
        )?;
        self.atlas_size = (atlas.width, atlas.height);
        Ok(())
    }

    /// Upload (or replace) the colour atlas: premultiplied RGBA8.
    /// 上传（或替换）颜色图集：预乘 RGBA8。
    pub fn set_color_atlas(
        &mut self,
        gl: &glow::Context,
        atlas: AtlasUpload,
    ) -> Result<(), GpuError> {
        upload_atlas(
            gl,
            &mut self.color_atlas_tex,
            atlas.width,
            atlas.height,
            glow::RGBA8,
            glow::RGBA,
            &atlas.data,
        )?;
        self.color_atlas_size = (atlas.width, atlas.height);
        Ok(())
    }

    /// Clear `target` to `color` (straight alpha, 0..=1).
    /// 把 `target` 清屏为 `color`（直通 alpha，0..=1）。
    pub fn clear(&mut self, gl: &glow::Context, target: &dyn GpuSurface, color: [f32; 4]) {
        target.bind(gl);
                unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(color[0], color[1], color[2], color[3]);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    /// Render `list` into `target`.
    /// 把 `list` 渲染进 `target`。
    ///
    /// Every frame clears the persistent frame buffer and repaints the whole scene. That is
    /// what Flutter, Skia-backed toolkits and Haze all do, and it is the reason they do not
    /// have the class of bug we kept hitting: no stage can read a pixel that nobody wrote
    /// this frame. Cross-frame reuse of filtered pixels was tried and removed — see
    /// `docs/effect-cache-plan.md` §11.
    /// 每帧清空持久帧缓冲并重画整个场景。Flutter、Skia 系工具包与 Haze 都是如此，这也正是
    /// 它们不会遇到我们反复踩到的那类 bug 的原因：任何阶段都读不到本帧没人写过的像素。
    /// 跨帧复用滤波像素曾实现过并已撤除，见 `docs/effect-cache-plan.md` §11。
    pub fn render(
        &mut self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
        list: &DrawList,
    ) -> Result<RenderStats, GpuError> {
        self.stats = RenderStats::default();
        let size = (target.size().0, target.size().1);
        let (frame_fbo, frame_tex) = crate::effect_pass::ensure_target(
            &mut self.frame,
            gl,
            size.0,
            size.1,
            self.target_format,
        )?;
        // The frame buffer persists across frames, so it must be cleared in full: anything
        // left from the previous frame would be composited against and shown.
        // 帧缓冲跨帧保留，因此必须整帧清空：上一帧残留的任何内容都会被混合并显示出来。
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(frame_fbo));
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.root_fbo = Some(frame_fbo);
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(frame_fbo));
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
        }
        let viewport = (target.size().0 as f32, target.size().1 as f32);
        unsafe {
            gl.enable(glow::BLEND);
            // Premultiplied source over destination.
            // 预乘源覆盖目标。
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::SCISSOR_TEST);
        }

        let mut rect_data: Vec<f32> = Vec::new();
        let mut glyph_data: Vec<f32> = Vec::new();
        // Active layer chain; depth `d` renders into `layer_targets[d]`.
        // 当前层链；深度 `d` 渲染进 `layer_targets[d]`。
        let mut stack: Vec<u32> = Vec::new();

        // A layer starts empty, so a frosted surface inside one samples its parent: the
        // layer is composited back over that same parent, so it is what the surface covers.
        // 层初始为空，因此层内的磨砂面采样它的父目标：层随后又合成回这个父目标，
        // 它覆盖的正是那里的像素。
        let backdrops = list.backdrop_order();
        let mut done = 0usize;
        self.sync_images(gl, list)?;

        for batch in list.plan_batches() {
            self.sync_layers(gl, list, &mut stack, &batch.layers, viewport)?;
            while done < backdrops.len() && backdrops[done].0 <= batch.epoch {
                let (_, entry) = backdrops[done];
                done += 1;
                // Run here, before the frost node's own fill: everything the kernel reads
                // was painted by earlier batches, and running later would smear its text.
                // 在此执行，早于磨砂节点自己的填充：核读取的内容都由更早的批次画好，
                // 放到后面会把它的文字一起抹掉。
                if std::env::var("CRABSNAP_NO_BACKDROP").is_err() {
                    let (src, dst) = self.backdrop_pair(frame_tex, frame_fbo, &stack);
                    self.apply_backdrop(gl, size, dst, src, entry)?;
                }
            }
            match batch.kind {
                PrimKind::Shadow => {
                    // One analytic draw for the whole run: no pass, no temporary texture.
                    // 整段一次解析绘制：没有趟，也没有临时纹理。
                    let mut data: Vec<f32> = Vec::new();
                    for i in batch.range.start..batch.range.end {
                        if let Some(p) = list.prims.shadows.get(i as usize) {
                            push_shadow(&mut data, p, self.scale);
                        }
                    }
                    if !data.is_empty() {
                        set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport);
                        self.draw_shadows(gl, viewport, &data, batch.clip)?;
                    }
                }
                PrimKind::Image => {
                    // Consecutive primitives on one texture share a draw; a batch may mix them.
                    // 同一纹理上连续的图元共用一次绘制；一个批次里可能混有多张纹理。
                    let mut start = batch.range.start;
                    while start < batch.range.end {
                        let Some(first) = list.prims.images.get(start as usize) else {
                            return Err(GpuError::from(format!(
                                "image batch names prim {start} of {}",
                                list.prims.images.len()
                            )));
                        };
                        let mut end = start + 1;
                        while end < batch.range.end
                            && list
                                .prims
                                .images
                                .get(end as usize)
                                .is_some_and(|p| p.texture == first.texture)
                        {
                            end += 1;
                        }
                        let Some(img) = self.frame_images.get(first.texture as usize).copied()
                        else {
                            return Err(GpuError::from(format!(
                                "image prim {start} samples texture {} of {}",
                                first.texture,
                                self.frame_images.len()
                            )));
                        };
                        let mut data: Vec<f32> = Vec::new();
                        for i in start..end {
                            if let Some(p) = list.prims.images.get(i as usize) {
                                push_image(&mut data, p, self.scale, (img.width, img.height));
                            }
                        }
                        if !data.is_empty() {
                            set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport);
                            self.draw_images(gl, viewport, &data, img.tex, batch.clip)?;
                        }
                        start = end;
                    }
                }
                PrimKind::Rect => {
                    for i in batch.range.start..batch.range.end {
                        if let Some(p) = list.prims.rects.get(i as usize) {
                            push_rect(&mut rect_data, p, self.scale);
                        }
                    }
                    if !rect_data.is_empty() {
                        set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport);
                        self.draw_rects(gl, viewport, &rect_data, batch.clip)?;
                        rect_data.clear();
                    }
                }
                PrimKind::Sector => {
                    let mut data: Vec<f32> = Vec::new();
                    for i in batch.range.start..batch.range.end {
                        if let Some(p) = list.prims.sectors.get(i as usize) {
                            push_sector(&mut data, p, self.scale);
                        }
                    }
                    if !data.is_empty() {
                        set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport);
                        self.draw_sectors(gl, viewport, &data, batch.clip)?;
                    }
                }
                PrimKind::Glyph => {
                    // A glyph wholly outside the batch's clip would only be scissored away;
                    // dropping it here leaves a long line in a narrow field as layout only.
                    // 完全落在批次裁剪框外的字形只会被剪掉；在此丢弃，
                    // 窄字段里的长行便只付排版代价。
                    let scissor = batch.clip.map(|c| device_scissor(c.rect, self.scale));
                    for i in batch.range.start..batch.range.end {
                        if let Some(p) = list.prims.glyphs.get(i as usize) {
                            if scissor.is_some_and(|s| !glyph_hits_scissor(p, self.scale, s)) {
                                continue;
                            }
                            push_glyph(&mut glyph_data, p, self.scale);
                        }
                    }
                    if !glyph_data.is_empty() {
                        set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport);
                        self.draw_glyphs(gl, viewport, &glyph_data, batch.clip)?;
                        glyph_data.clear();
                    }
                }
                // Raw text without a pre-rasterized atlas has no GPU path yet.
                // 没有预光栅图集的原始文本暂时没有 GPU 路径。
                PrimKind::Text => {}
            }
        }

        // A backdrop with no batch after it would otherwise never run: the loop only
        // advances the queue when it sees a later epoch.
        // 后面没有批次的 backdrop 否则永远不会执行：循环只在遇到更晚的世代时才推进队列。
        while done < backdrops.len() {
            let (_, entry) = backdrops[done];
            done += 1;
            if std::env::var("CRABSNAP_NO_BACKDROP").is_err() {
                let (src, dst) = self.backdrop_pair(frame_tex, frame_fbo, &stack);
                self.apply_backdrop(gl, size, dst, src, entry)?;
            }
        }
        while let Some(index) = stack.pop() {
            let depth = stack.len();
            let effects = list
                .layers
                .get(index as usize)
                .map_or(&[][..], |l| &l.effects[..]);
            self.end_layer(gl, depth, effects, viewport)?;
        }
        self.blit_frame(gl, target, size, frame_fbo, frame_tex)?;
        Ok(self.stats)
    }

    /// Bind the target that layer `depth` renders into, creating it on demand.
    /// 绑定第 `depth` 层渲染用的目标，按需创建。
    fn begin_layer(
        &mut self,
        gl: &glow::Context,
        depth: usize,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
                while self.layer_targets.len() <= depth {
            let tex = unsafe { gl.create_texture()? };
            let fbo = unsafe { gl.create_framebuffer()? };
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::NEAREST as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::NEAREST as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_S,
                    glow::CLAMP_TO_EDGE as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_T,
                    glow::CLAMP_TO_EDGE as i32,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(tex),
                    0,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            }
            self.layer_targets.push(LayerTarget {
                fbo,
                tex,
                width,
                height,
                format: self.target_format,
            });
        }

        let slot = &mut self.layer_targets[depth];
        if slot.width != width || slot.height != height || slot.format != self.target_format {
            slot.format = self.target_format;
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(slot.tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            slot.width = width;
            slot.height = height;
        }

        let fbo = slot.fbo;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, width as i32, height as i32);
            // A layer starts fully transparent; leftovers would leak through.
            // 层从全透明开始；残留会渗进来。
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::SCISSOR_TEST);
        }
        Ok(())
    }

    /// Composite layer `depth` into its parent with `opacity`.
    /// 把第 `depth` 层按 `opacity` 合成到父级。
    /// Hand the persistent frame to the rotating target, applying the output transform.
    /// 把持久帧交给轮转目标，并施加输出变换。
    ///
    /// Both the static path and the redraw path go through here: skipping it on a static
    /// frame leaves sRGB values in a surface declared as PQ, which reads as washed out.
    /// 静止帧与重绘帧都必须走这里：静止帧跳过它会让声明为 PQ 的 surface 里留着 sRGB 值，
    /// 表现为发白发亮。
    fn blit_frame(
        &mut self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
        size: (u32, u32),
        frame_fbo: glow::Framebuffer,
        frame_tex: glow::Texture,
    ) -> Result<(), GpuError> {
        let mut from = frame_fbo;
        if let Some([scale, offset]) = self.hdr {
            let scratch =
                crate::effect_pass::ensure_target(
                &mut self.output_scratch,
                gl,
                size.0,
                size.1,
                self.target_format,
            )?;
            let p = &self.output;
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(scratch.0));
                gl.viewport(0, 0, size.0 as i32, size.1 as i32);
                gl.disable(glow::BLEND);
                gl.disable(glow::SCISSOR_TEST);
                gl.use_program(Some(p.program));
                if let Some(u) = &p.uniforms.viewport {
                    gl.uniform_2_f32(Some(u), size.0 as f32, size.1 as f32);
                }
                if let Some(u) = &p.uniforms.src {
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(frame_tex));
                    gl.uniform_1_i32(Some(u), 0);
                }
                if let Some(u) = &p.uniforms.scale {
                    gl.uniform_1_f32(Some(u), scale);
                }
                if let Some(u) = &p.uniforms.offset {
                    gl.uniform_1_f32(Some(u), offset);
                }
                if let Some(u) = &p.uniforms.gamut {
                    // GLSL reads mat3 column-major, so transpose on the way out.
                    // GLSL 按列主序读 mat3，因此传出时转置。
                    let g = &self.gamut;
                    let cols = [
                        g[0][0], g[1][0], g[2][0], g[0][1], g[1][1], g[2][1], g[0][2], g[1][2],
                        g[2][2],
                    ];
                    gl.uniform_matrix_3_f32_slice(Some(u), false, &cols);
                }
                gl.bind_vertex_array(Some(p.vao));
                let quad: [f32; 4] = [0.0, 0.0, size.0 as f32, size.1 as f32];
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&quad), glow::DYNAMIC_DRAW);
                gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, 1);
                gl.enable(glow::BLEND);
            }
            from = scratch.0;
        }
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            target.bind(gl);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(from));
            gl.blit_framebuffer(
                0,
                0,
                size.0 as i32,
                size.1 as i32,
                0,
                0,
                size.0 as i32,
                size.1 as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
        Ok(())
    }

    fn end_layer(
        &mut self,
        gl: &glow::Context,
        depth: usize,
        effects: &[Effect],
        viewport: (f32, f32),
    ) -> Result<(), GpuError> {
                let Some(slot) = self.layer_targets.get(depth) else {
            return Ok(());
        };
        let (layer_w, layer_h, layer_tex) = (slot.width, slot.height, slot.tex);

        // Effects and compositing both stop at the layer's bounds; the rest is transparent.
        // 效果与合成都到层边界为止，其余是透明的。
        let full = ui_core::geom::Rect::new(0.0, 0.0, viewport.0, viewport.1);
        let bounds = self.layer_bounds.get(depth).copied().unwrap_or(full);
        let scissor = match bounds.intersection(full) {
            Some(r) => device_scissor(r, 1.0),
            None => [0, 0, 0, 0],
        };

        // The layer was painted in full this frame, so its effects are recomputed in full:
        // nothing here may depend on what a previous frame left in these textures.
        // 层在本帧被整层画好，因此其效果也整层重算：这里任何东西都不得依赖上一帧留在这
        // 些纹理里的内容。
        if self.effect_scratch.len() <= depth {
            self.effect_scratch.resize_with(depth + 1, Default::default);
        }
        let tex = crate::effect_pass::run(
            &mut self.effect_scratch[depth],
            gl,
            &self.effects,
            &self.boxpass,
            layer_tex,
            layer_w,
            layer_h,
            bounds,
            self.target_format,
            effects,
        )?;

        // Bind the parent: the root target at depth 0, otherwise the outer layer.
        // 绑定父级：深度 0 时是根目标，否则是外层。
        if depth == 0 {
            // Depth 0 composites into the persistent frame buffer, not the target.
            // 深度 0 合成到持久帧缓冲，而不是目标。
            if let Some(root) = self.root_fbo {
                unsafe {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(root));
                    gl.viewport(0, 0, viewport.0 as i32, viewport.1 as i32);
                }
            }
        } else if let Some(parent) = self.layer_targets.get(depth - 1) {
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(parent.fbo));
                gl.viewport(0, 0, parent.width as i32, parent.height as i32);
            }
        }

        // The frame was cleared in full before painting, so the layer may composite over its
        // whole bounds: there is no earlier composite of this layer to accumulate against.
        // 绘制前已整帧清空，因此层可以覆盖自己的整个边界：不存在本层的上一次合成可累积。
        unsafe {
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
            let p = &self.layers;
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            if let Some(u) = p.uniforms.opacity {
                gl.uniform_1_f32(Some(&u), 1.0);
            }
            // Rounded corners are a property of the layer, not of this pass.
            // 圆角是层的属性，不是这一趟的。
            if let Some(u) = &p.uniforms.layer_rect {
                gl.uniform_4_f32(
                    Some(u),
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.size.w,
                    bounds.size.h,
                );
            }
            let corners = self
                .layer_radius
                .get(depth)
                .copied()
                .unwrap_or(ui_core::geom::Corners::all(0.0));
            if let Some(u) = &p.uniforms.layer_radius {
                gl.uniform_4_f32(Some(u), corners.tl, corners.tr, corners.br, corners.bl);
            }
            if let Some(u) = &p.uniforms.layer_exp {
                gl.uniform_1_f32(Some(u), corners.shape.exponent());
            }
            if let Some(u) = &p.uniforms.layer_skip_empty {
                gl.uniform_1_f32(Some(u), 0.0);
            }
            if let Some(u) = p.uniforms.layer_tex {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.uniform_1_i32(Some(&u), 0);
            }
            let data = [0.0f32, 0.0, viewport.0, viewport.1];
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                1,
            );
            gl.enable(glow::SCISSOR_TEST);
        }
        self.stats.draw_calls += 1;
        Ok(())
    }

    /// Blur what is already painted inside `entry`, in place on the frame target.
    /// 在帧目标上就地模糊 `entry` 矩形内已画好的内容。
    ///
    /// The filter reads the live frame and runs over all of it: everything the kernel needs
    /// was painted by earlier batches this frame, so no tap can reach a pixel from an earlier
    /// frame, and edge taps meet real neighbours instead of a cropped copy's transparent
    /// border. The result is written back over the node's own rect with corner coverage,
    /// which is what keeps the frost's rounded shape.
    /// 滤波器读当帧画面并跑整帧：核需要的一切都由本帧更早的批次画好，因此没有 tap 能触到
    /// 上一帧的像素，边缘 tap 遇到的也是真实邻居而非裁剪副本的透明边界。结果按节点自身
    /// 矩形加圆角覆盖率写回，磨砂因此保留圆角。
    /// The texture a backdrop reads and the target it lands in, for the layer in force.
    /// 背景模糊读取的纹理与落点，按当前生效的层决定。
    fn backdrop_pair(
        &self,
        frame_tex: glow::Texture,
        frame_fbo: glow::Framebuffer,
        stack: &[u32],
    ) -> (glow::Texture, glow::Framebuffer) {
        match stack.len() {
            0 => (frame_tex, frame_fbo),
            n => {
                let src = if n >= 2 {
                    self.layer_targets[n - 2].tex
                } else {
                    frame_tex
                };
                (src, self.layer_targets[n - 1].fbo)
            }
        }
    }

    fn apply_backdrop(
        &mut self,
        gl: &glow::Context,
        size: (u32, u32),
        dst_fbo: glow::Framebuffer,
        src_tex: glow::Texture,
        entry: ui_core::draw::BackdropEntry,
    ) -> Result<(), GpuError> {
        let s = self.scale;
        let full = ui_core::geom::Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32);
        let device = ui_core::geom::Rect::new(
            entry.rect.origin.x * s,
            entry.rect.origin.y * s,
            entry.rect.size.w * s,
            entry.rect.size.h * s,
        );
        let Some(clip) = device.intersection(full) else {
            return Ok(());
        };
        let scissor = device_scissor(clip, 1.0);
        if scissor[2] <= 0 || scissor[3] <= 0 {
            return Ok(());
        }

        let effect = ui_core::effect::Effect::box_blur(entry.sigma);
        let filtered = crate::effect_pass::run(
            &mut self.backdrop_scratch,
            gl,
            &self.effects,
            &self.boxpass,
            src_tex,
            size.0,
            size.1,
            full,
            self.target_format,
            &[effect],
        )?;
        if filtered == src_tex {
            return Ok(());
        }
        self.stats.frosts += 1;
        self.composite_backdrop(gl, size, dst_fbo, device, scissor, entry, filtered)
    }

    /// Write the filtered backdrop back over the node's rect, shaded as the style asks.
    /// 把滤波后的背景写回节点矩形，并按风格要求着色。
    fn composite_backdrop(
        &mut self,
        gl: &glow::Context,
        size: (u32, u32),
        dst_fbo: glow::Framebuffer,
        device: ui_core::geom::Rect,
        scissor: [i32; 4],
        entry: ui_core::draw::BackdropEntry,
        filtered: glow::Texture,
    ) -> Result<(), GpuError> {
        let s = self.scale;
        // A node may name its own material; the renderer-wide one is the default for nodes
        // that only asked for a blur.
        // 节点可以自带材质；渲染器级的那份只作为「只要求模糊」的节点的默认值。
        let material = resolve(entry.material, self.material);
        let shade = Shade::of(material);
        // Without a material this is the plain blit: one texture read with corner coverage.
        // 未装材质时就是纯写回：一次纹理采样加圆角覆盖。
        // The premium lens is a different material, not a knob on the flat one.
        // Premium 透镜是另一套材质，而不是平面那套的旋钮。
        let premium = matches!(
            material,
            Some(BackdropMaterial::Glass(g)) if g.quality == ui_core::draw::GlassQuality::Premium
        );
        let p = match shade {
            Shade::Plain => &self.layers,
            Shade::Acrylic => &self.acrylic,
            Shade::Glass if premium => &self.glass_premium,
            Shade::Glass => &self.glass,
        };
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dst_fbo));
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
            // Premultiplied blend against the frame: corner coverage is below 1 there, and
            // overwriting would replace opaque pixels with partially transparent ones.
            // 对帧做预乘混合：圆角处覆盖率小于 1，覆盖会把不透明像素换成半透明。
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), size.0 as f32, size.1 as f32);
            }
            if let Some(u) = p.uniforms.opacity {
                gl.uniform_1_f32(Some(&u), 1.0);
            }
            if let Some(u) = &p.uniforms.layer_rect {
                gl.uniform_4_f32(
                    Some(u),
                    device.origin.x,
                    device.origin.y,
                    device.size.w,
                    device.size.h,
                );
            }
            if let Some(u) = &p.uniforms.layer_radius {
                gl.uniform_4_f32(
                    Some(u),
                    entry.corners.tl * s,
                    entry.corners.tr * s,
                    entry.corners.br * s,
                    entry.corners.bl * s,
                );
            }
            if let Some(u) = &p.uniforms.layer_exp {
                gl.uniform_1_f32(Some(u), entry.corners.shape.exponent());
            }
            if let Some(u) = &p.uniforms.layer_skip_empty {
                gl.uniform_1_f32(Some(u), 0.0);
            }
            if let Some(u) = p.uniforms.layer_tex {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(filtered));
                gl.uniform_1_i32(Some(&u), 0);
            }
            if let Some(u) = p.uniforms.src {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(filtered));
                gl.uniform_1_i32(Some(&u), 0);
            }
            match material {
                None | Some(BackdropMaterial::Plain) => {}
                Some(BackdropMaterial::Acrylic(a)) => {
                    if let Some(u) = p.uniforms.tint {
                        gl.uniform_4_f32(Some(&u), a.tint.r, a.tint.g, a.tint.b, a.tint.a);
                    }
                    if let Some(u) = p.uniforms.luminosity {
                        gl.uniform_4_f32(
                            Some(&u),
                            a.luminosity.r,
                            a.luminosity.g,
                            a.luminosity.b,
                            a.luminosity.a,
                        );
                    }
                    if let Some(u) = p.uniforms.noise {
                        gl.uniform_1_f32(Some(&u), a.noise);
                    }
                }
                Some(BackdropMaterial::Glass(g)) if premium => {
                    if let Some(u) = p.uniforms.glass {
                        gl.uniform_4_f32(Some(&u), g.tint.r, g.tint.g, g.tint.b, g.tint.a);
                    }
                    // The premium lens measures in 3× display pixels: the reference passes
                    // `thickness * dpr / 3`, so a dp keeps its logical rim width on every screen.
                    // Premium 透镜以 3 倍屏像素计量：参考传 `thickness * dpr / 3`，dp 在每种屏幕上宽度不变。
                    let lens = s / 3.0;
                    if let Some(u) = p.uniforms.optical {
                        gl.uniform_4_f32(
                            Some(&u),
                            g.refractive_index,
                            g.chromatic,
                            g.thickness * lens,
                            1.0,
                        );
                    }
                    if let Some(u) = p.uniforms.light {
                        gl.uniform_4_f32(
                            Some(&u),
                            g.light_direction[0],
                            g.light_direction[1],
                            g.light_intensity,
                            g.saturation,
                        );
                    }
                    if let Some(u) = p.uniforms.ambient {
                        gl.uniform_4_f32(
                            Some(&u),
                            g.ambient_strength,
                            g.ambient_rim * lens,
                            g.fresnel_strength,
                            g.whiten,
                        );
                    }
                    if let Some(u) = p.uniforms.edge {
                        gl.uniform_4_f32(
                            Some(&u),
                            lens,
                            g.edge_absorption,
                            g.rim_shade,
                            g.rim_shade_ends,
                        );
                    }
                    if let Some(u) = p.uniforms.blend {
                        let paraxial = g.lens_model == ui_core::draw::LensModel::Paraxial;
                        gl.uniform_4_f32(
                            Some(&u),
                            g.blend_count as f32,
                            g.blend_k * s,
                            f32::from(u8::from(paraxial)),
                            // The hairline is measured in device pixels, as the reference's
                            // `dprScale` is; `u_edge.x` stays the lens' 3× normaliser.
                            // 发丝描边按设备像素计量，与参考的 `dprScale` 一致；
                            // `u_edge.x` 仍是透镜的 3 倍归一化系数。
                            s,
                        );
                    }
                    if let Some(u) = p.uniforms.lens {
                        gl.uniform_4_f32(Some(&u), g.magnification, 0.0, 0.0, 0.0);
                    }
                    // Fused shapes arrive in the node's logical space; the shader works in
                    // device pixels, so each is offset by the node's origin and scaled once.
                    // 融合形状以节点的逻辑坐标传入；着色器按设备像素工作，因此按节点原点偏移并统一缩放。
                    if g.blend_count > 0 {
                        let mut boxes = [0.0f32; 16];
                        let mut radii = [0.0f32; 16];
                        for (i, shape) in g.blend.iter().take(g.blend_count as usize).enumerate() {
                            let hw = shape.rect.size.w.abs() * 0.5 * s;
                            let hh = shape.rect.size.h.abs() * 0.5 * s;
                            boxes[i * 4] = device.origin.x + shape.rect.origin.x * s + hw;
                            boxes[i * 4 + 1] = device.origin.y + shape.rect.origin.y * s + hh;
                            boxes[i * 4 + 2] = hw;
                            boxes[i * 4 + 3] = hh;
                            let cap = hw.min(hh);
                            radii[i * 4] = (shape.corners.tl * s).min(cap);
                            radii[i * 4 + 1] = (shape.corners.tr * s).min(cap);
                            radii[i * 4 + 2] = (shape.corners.br * s).min(cap);
                            radii[i * 4 + 3] = (shape.corners.bl * s).min(cap);
                        }
                        if let Some(u) = p.uniforms.shapes {
                            gl.uniform_4_f32_slice(Some(&u), &boxes);
                        }
                        if let Some(u) = p.uniforms.shape_radius {
                            gl.uniform_4_f32_slice(Some(&u), &radii);
                        }
                    }
                }
                Some(BackdropMaterial::Glass(g)) => {
                    if let Some(u) = p.uniforms.glass {
                        gl.uniform_4_f32(Some(&u), g.tint.r, g.tint.g, g.tint.b, g.tint.a);
                    }
                    if let Some(u) = p.uniforms.rim_params {
                        gl.uniform_4_f32(
                            Some(&u),
                            g.rim_thickness,
                            g.rim_smoothing,
                            g.ambient_rim,
                            g.ambient_strength,
                        );
                    }
                    if let Some(u) = p.uniforms.alpha {
                        gl.uniform_4_f32(
                            Some(&u),
                            g.base_alpha,
                            g.edge_alpha,
                            g.glow_intensity,
                            g.interaction,
                        );
                    }
                    if let Some(u) = p.uniforms.light {
                        gl.uniform_4_f32(
                            Some(&u),
                            g.light_direction[0],
                            g.light_direction[1],
                            g.light_intensity,
                            g.saturation,
                        );
                    }
                    if let Some(u) = p.uniforms.effects {
                        gl.uniform_2_f32(Some(&u), g.chromatic, g.edge_absorption);
                    }
                    if let Some(u) = p.uniforms.edge_zone {
                        gl.uniform_1_f32(Some(&u), g.edge_zone);
                    }
                }
            }
            // `a_rect.y` is the top in device space, and the quad runs down from there, so
            // the instance must cover the whole frame; coverage() then cuts the node's shape.
            // `a_rect.y` 是设备空间顶部，四边形由此向下延伸，因此实例要覆盖整帧；
            // 再由 coverage() 切出节点形状。
            let data = [0.0f32, 0.0, size.0 as f32, size.1 as f32];
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                crate::shader::UNIT_QUAD.len() as i32 / 2,
                1,
            );
        }
        self.stats.draw_calls += 1;
        if material.is_some() {
            self.stats.material_passes += 1;
        }
        Ok(())
    }

    /// Pop and push layers so the active chain equals `want`.
    /// 弹入弹出层，使活动层链等于 `want`。
    fn sync_layers(
        &mut self,
        gl: &glow::Context,
        list: &DrawList,
        stack: &mut Vec<u32>,
        want: &[u32],
        viewport: (f32, f32),
    ) -> Result<(), GpuError> {
        let mut common = 0;
        while common < stack.len() && common < want.len() && stack[common] == want[common] {
            common += 1;
        }
        while stack.len() > common {
            let index = stack.pop().expect("non-empty");
            let depth = stack.len();
            let effects = list
                .layers
                .get(index as usize)
                .map_or(&[][..], |l| &l.effects[..]);
            self.end_layer(gl, depth, effects, viewport)?;
        }
        for &_index in &want[common..] {
            let depth = stack.len();
            let (w, h) = (viewport.0 as u32, viewport.1 as u32);
            self.begin_layer(gl, depth, w, h)?;
            // Record the layer's device-space bounds: effects and compositing stop there.
            // 记下层的设备空间边界：效果与合成都到此为止。
            let s = self.scale;
            let b = list
                .layers
                .get(_index as usize)
                .map_or(ui_core::geom::Rect::new(0.0, 0.0, 0.0, 0.0), |l| l.bounds);
            let device = ui_core::geom::Rect::new(b.origin.x * s, b.origin.y * s, b.size.w * s, b.size.h * s);
            if self.layer_bounds.len() <= depth {
                self.layer_bounds.resize(depth + 1, ui_core::geom::Rect::new(0.0, 0.0, 0.0, 0.0));
            }
            self.layer_bounds[depth] = device;
            let corners = list
                .layers
                .get(_index as usize)
                .map_or(ui_core::geom::Corners::all(0.0), |l| l.radius);
            if self.layer_radius.len() <= depth {
                self.layer_radius
                    .resize(depth + 1, ui_core::geom::Corners::all(0.0));
            }
            self.layer_radius[depth] = ui_core::geom::Corners {
                tl: corners.tl * s,
                tr: corners.tr * s,
                br: corners.br * s,
                bl: corners.bl * s,
                ..corners
            }
            // A layer is clipped to its own bounds, so the same half-side limit applies.
            // 层会被裁到自身边界，因此同样适用短边一半的上限。
            .clamp_to(device.size);
            stack.push(_index);
        }
        Ok(())
    }

    fn draw_rects(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
                let p = &self.rects;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                as_bytes(data),
                glow::DYNAMIC_DRAW,
            );
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::RECT_STRIDE as usize) as i32,
            );
        }
        self.stats.rects += (data.len() / shader::RECT_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_sectors(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.sectors;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::SECTOR_STRIDE as usize) as i32,
            );
        }
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_glyphs(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let Some(atlas_tex) = self.atlas_tex else {
            return Ok(());
        };
                let p = &self.glyphs;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            if let Some(u) = p.uniforms.atlas_size {
                gl.uniform_2_f32(Some(&u), self.atlas_size.0 as f32, self.atlas_size.1 as f32);
            }
            if let Some(u) = p.uniforms.color_atlas_size {
                gl.uniform_2_f32(
                    Some(&u),
                    self.color_atlas_size.0 as f32,
                    self.color_atlas_size.1 as f32,
                );
            }
            if let Some(u) = p.uniforms.atlas {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(atlas_tex));
                gl.uniform_1_i32(Some(&u), 0);
            }
            if let Some(u) = p.uniforms.color_atlas {
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(
                    glow::TEXTURE_2D,
                    Some(self.color_atlas_tex.unwrap_or(atlas_tex)),
                );
                gl.uniform_1_i32(Some(&u), 1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::GLYPH_STRIDE as usize) as i32,
            );
        }
        self.stats.glyphs += (data.len() / shader::GLYPH_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    /// Upload the frame's textures, reusing one GL texture per `ImageEntry::id`.
    /// 上传本帧的纹理，每个 `ImageEntry::id` 复用一个 GL 纹理。
    fn sync_images(&mut self, gl: &glow::Context, list: &DrawList) -> Result<(), GpuError> {
        self.frame_images.clear();
        for entry in &list.images {
            let cached = self.image_textures.get(&entry.id);
            let sized = cached.filter(|c| c.width == entry.width && c.height == entry.height);
            let img = match sized {
                Some(c) => FrameImage {
                    tex: c.tex,
                    width: c.width,
                    height: c.height,
                },
                None => {
                    let tex = match cached {
                        Some(c) => c.tex,
                        None => unsafe { gl.create_texture()? },
                    };
                    upload_image(gl, tex, entry)?;
                    self.image_textures.insert(
                        entry.id,
                        ImageTexture {
                            tex,
                            width: entry.width,
                            height: entry.height,
                        },
                    );
                    self.stats.image_uploads += 1;
                    FrameImage {
                        tex,
                        width: entry.width,
                        height: entry.height,
                    }
                }
            };
            self.frame_images.push(img);
        }
        // Free ids this frame does not name: an app that mints an id per frame must not leak.
        // 释放本帧未出现的 id：逐帧新建 id 的应用不得泄漏。
        let live: HashSet<u64> = list.images.iter().map(|e| e.id).collect();
        self.image_textures.retain(|id, t| {
            let keep = live.contains(id);
            if !keep {
                unsafe { gl.delete_texture(t.tex) };
            }
            keep
        });
        Ok(())
    }

    fn draw_images(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        texture: glow::Texture,
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.images;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            if let Some(u) = p.uniforms.image {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.uniform_1_i32(Some(&u), 0);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::IMAGE_STRIDE as usize) as i32,
            );
        }
        self.stats.images_drawn += (data.len() / shader::IMAGE_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_shadows(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.shadows;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::SHADOW_STRIDE as usize) as i32,
            );
        }
        self.stats.shadows_drawn += (data.len() / shader::SHADOW_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    /// Wrap mode of the cached texture for `id`; `None` when it is not resident.
    /// 该 `id` 缓存纹理的环绕模式；不在缓存中时为 `None`。
    pub fn image_wrap(&self, gl: &glow::Context, id: u64) -> Option<(i32, i32)> {
        let t = self.image_textures.get(&id)?;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(t.tex));
            let s = gl.get_tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S);
            let v = gl.get_tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T);
            gl.bind_texture(glow::TEXTURE_2D, None);
            Some((s, v))
        }
    }

    /// Read layer `depth` back, for diagnosing compositing.
    /// 回读第 `depth` 层，用于诊断合成。
    pub fn read_layer(&self, gl: &glow::Context, depth: usize) -> Option<(u32, u32, Vec<u8>)> {
        let slot = self.layer_targets.get(depth)?;
                let (w, h) = (slot.width, slot.height);
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(slot.fbo));
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Some((w, h, buf))
    }

    /// Block until the GPU has finished writing to the target.
    /// 阻塞到 GPU 完成对目标的写入。
    ///
    /// A dma-buf handed to another process needs the writes completed first;
    /// without this the compositor can sample the buffer before the GPU is done.
    /// 交给另一个进程的 dma-buf 必须先完成写入；否则 compositor 可能在 GPU 完成前采样。
    pub fn finish(&self, gl: &glow::Context) {
        unsafe { gl.finish() };
    }

    /// Convert straight-alpha RGBA8 into the premultiplied BGRA that `ui_paint::Pixels` holds.
    /// 把直通 alpha 的 RGBA8 转成 `ui_paint::Pixels` 使用的预乘 BGRA。
    ///
    /// `read_pixels` and `Pixels` disagree on *both* channel order and alpha convention;
    /// this function exists so callers stop re-deriving it wrongly.
    /// `read_pixels` 与 `Pixels` 在通道序和 alpha 约定上**都**不同；
    /// 这个函数的存在就是为了让调用方不必再各自推错一遍。
    pub fn to_premultiplied_bgra(straight_rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(straight_rgba.len());
        for p in straight_rgba.chunks_exact(4) {
            let a = p[3] as u32;
            let pm = |c: u8| ((c as u32 * a + 127) / 255) as u8;
            out.extend_from_slice(&[pm(p[2]), pm(p[1]), pm(p[0]), p[3]]);
        }
        out
    }

    /// Read the target back as top-down straight-alpha RGBA8.
    /// 把目标读回为自顶向下、直通 alpha 的 RGBA8。
    pub fn read_pixels(&self, gl: &glow::Context, target: &dyn GpuSurface) -> Result<Vec<u8>, GpuError> {
        target.bind(gl);
                let w = target.size().0 as usize;
        let h = target.size().1 as usize;
        let mut buf = vec![0u8; w * h * 4];
        unsafe {
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
        }
        // Rendering already writes top-down rows, so only premultiplication has to
        // be undone here.
        // 渲染已按自顶向下行序写入，这里只需还原非预乘。
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            let src = y * w * 4;
            let dst = y * w * 4;
            for x in 0..w {
                let s = src + x * 4;
                let d = dst + x * 4;
                let a = buf[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                if a == 255 {
                    out[d..d + 4].copy_from_slice(&buf[s..s + 4]);
                } else {
                    // Un-premultiply so callers see straight alpha.
                    // 还原非预乘，使调用方看到直通 alpha。
                    for c in 0..3 {
                        out[d + c] = ((buf[s + c] as u32 * 255 + a / 2) / a).min(255) as u8;
                    }
                    out[d + 3] = a as u8;
                }
            }
        }
        Ok(out)
    }

    /// Free GL objects. Must be called while `ctx` is current.
    /// 释放 GL 对象。必须在 `ctx` 为当前上下文时调用。
    pub fn release(&self, gl: &glow::Context) {
                unsafe {
            for p in [
                &self.rects,
                &self.sectors,
                &self.glyphs,
                &self.images,
                &self.shadows,
            ] {
                gl.delete_program(p.program);
                gl.delete_vertex_array(p.vao);
                gl.delete_buffer(p.unit_vbo);
                gl.delete_buffer(p.instance_vbo);
            }
            for t in self.image_textures.values() {
                gl.delete_texture(t.tex);
            }
            if let Some(t) = self.atlas_tex {
                gl.delete_texture(t);
            }
            if let Some(t) = self.color_atlas_tex {
                gl.delete_texture(t);
            }
        }
    }
}

impl Program {
    fn new(
        gl: &glow::Context,
        is_gles: bool,
        vertex: &str,
        fragment: &str,
        stride: i32,
        instance_attribs: &[(&str, i32)],
        required_uniforms: &[&str],
    ) -> Result<Self, GpuError> {
        let program = compile_program(gl, vertex, fragment, is_gles)?;
        unsafe {
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));

            let unit_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(unit_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                as_bytes(&shader::UNIT_QUAD),
                glow::STATIC_DRAW,
            );
            let unit_loc = gl.get_attrib_location(program, "a_unit");
            if let Some(loc) = unit_loc {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(loc, 2, glow::FLOAT, false, 8, 0);
            }

            let instance_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                as_bytes(&vec![0.0f32; stride as usize * 64]),
                glow::DYNAMIC_DRAW,
            );
            let mut offset = 0i32;
            for (name, size) in instance_attribs {
                if let Some(loc) = gl.get_attrib_location(program, name) {
                    gl.enable_vertex_attrib_array(loc);
                    gl.vertex_attrib_pointer_f32(
                        loc,
                        *size,
                        glow::FLOAT,
                        false,
                        stride * 4,
                        offset * 4,
                    );
                    gl.vertex_attrib_divisor(loc, 1);
                }
                offset += size;
            }

            // A mistyped uniform name yields `None` and then silently does nothing,
            // so a required uniform missing here is a startup error, not a warning.
            // 写错 uniform 名会得到 `None` 并静默失效，因此必需项缺失是启动错误而非警告。
            for name in required_uniforms {
                if gl.get_uniform_location(program, name).is_none() {
                    return Err(GpuError::from(format!(
                        "shader is missing required uniform `{name}`"
                    )));
                }
            }

            let uniforms = Uniforms {
                viewport: gl.get_uniform_location(program, "u_viewport"),
                atlas_size: gl.get_uniform_location(program, "u_atlas_size"),
                atlas: gl.get_uniform_location(program, "u_atlas"),
                color_atlas_size: gl.get_uniform_location(program, "u_color_atlas_size"),
                color_atlas: gl.get_uniform_location(program, "u_color_atlas"),
                image: gl.get_uniform_location(program, "u_image"),
                opacity: gl.get_uniform_location(program, "u_opacity"),
                layer_tex: gl.get_uniform_location(program, "u_layer"),
                clip_rect: gl.get_uniform_location(program, "u_clip_rect"),
                clip_radius: gl.get_uniform_location(program, "u_clip_radius"),
                clip_shape: gl.get_uniform_location(program, "u_clip_shape"),
                clip_on: gl.get_uniform_location(program, "u_clip_on"),
                src: gl.get_uniform_location(program, "u_src"),
                texel: gl.get_uniform_location(program, "u_texel"),
                dir: gl.get_uniform_location(program, "u_dir"),
                taps: gl.get_uniform_location(program, "u_taps"),
                radius: gl.get_uniform_location(program, "u_radius"),
                weights: gl.get_uniform_location(program, "u_weights"),
                color: gl.get_uniform_location(program, "u_color"),
                scale: gl.get_uniform_location(program, "u_scale"),
                offset: gl.get_uniform_location(program, "u_offset"),
                gamut: gl.get_uniform_location(program, "u_gamut"),
                layer_rect: gl.get_uniform_location(program, "u_rect"),
                layer_radius: gl.get_uniform_location(program, "u_radius"),
                layer_exp: gl.get_uniform_location(program, "u_exp"),
                layer_skip_empty: gl.get_uniform_location(program, "u_skip_empty"),
                tint: gl.get_uniform_location(program, "u_tint"),
                luminosity: gl.get_uniform_location(program, "u_luminosity"),
                noise: gl.get_uniform_location(program, "u_noise"),
                glass: gl.get_uniform_location(program, "u_glass"),
                rim_params: gl.get_uniform_location(program, "u_rim_params"),
                alpha: gl.get_uniform_location(program, "u_alpha"),
                light: gl.get_uniform_location(program, "u_light"),
                effects: gl.get_uniform_location(program, "u_effects"),
                edge_zone: gl.get_uniform_location(program, "u_edge_zone"),
                optical: gl.get_uniform_location(program, "u_optical"),
                ambient: gl.get_uniform_location(program, "u_ambient"),
                edge: gl.get_uniform_location(program, "u_edge"),
                blend: gl.get_uniform_location(program, "u_blend"),
                lens: gl.get_uniform_location(program, "u_lens"),
                shapes: gl.get_uniform_location(program, "u_shape[0]"),
                shape_radius: gl.get_uniform_location(program, "u_shape_radius[0]"),
            };
            gl.bind_vertex_array(None);
            Ok(Self {
                program,
                vao,
                unit_vbo,
                instance_vbo,
                uniforms,
            })
        }
    }
}

/// Pack one shadow: rect, radius, colour, blur + dy, shape.
/// 打包一个阴影：矩形、圆角、颜色、衰减宽度与偏移、指数。
fn push_shadow(out: &mut Vec<f32>, p: &ShadowPrim, scale: f32) {
    let r = p.rect;
    let (rgb, a) = split_color(p.colour);
    let rad = [
        p.corners.tl * scale,
        p.corners.tr * scale,
        p.corners.br * scale,
        p.corners.bl * scale,
    ];
    // Corners may not exceed half the shorter side, or the SDF folds.
    // 圆角不得超过短边一半，否则 SDF 会折。
    let half = (r.size.w * scale).min(r.size.h * scale) * 0.5;
    let rad: Vec<f32> = rad.iter().map(|v| v.min(half).max(0.0)).collect();
    out.extend_from_slice(&[
        r.origin.x * scale,
        r.origin.y * scale,
        r.size.w * scale,
        r.size.h * scale,
        rad[0],
        rad[1],
        rad[2],
        rad[3],
        rgb[0],
        rgb[1],
        rgb[2],
        a,
        (p.blur * scale).max(0.5),
        p.dy * scale,
        p.corners.shape.exponent(),
        0.0,
    ]);
}

fn push_rect(out: &mut Vec<f32>, p: &RectPrim, scale: f32) {
    let r = p.rect;
    // Same clamp as the CPU path: a radius over half the short side folds the SDF.
    // 与 CPU 路径同样的夹取：圆角超过短边一半会让 SDF 折叠。
    let radius = p.radius.clamp_to(r.size);
    let (fill, alpha) = split_color(p.fill);
    // Gradient: the far stop plus (axis, enabled). Backends without the shader support
    // fall back to `fill`, which is why `fill` is always the near stop.
    // 渐变：远端色加上（轴，是否启用）。没有该着色器支持的后端回落到 `fill`，
    // 因此 `fill` 始终是近端色。
    let (far, axis, enabled) = match p.gradient {
        Some(g) => {
            let (c, a) = split_color(g.to);
            let axis = match g.axis {
                ui_core::draw::GradientAxis::Vertical => 0.0,
                ui_core::draw::GradientAxis::Horizontal => 1.0,
            };
            ([c[0], c[1], c[2], a], axis, 1.0)
        }
        None => ([0.0; 4], 0.0, 0.0),
    };
    let (border, width) = match p.border {
        Some((c, w)) => {
            let (br, ba) = split_color(c);
            ([br[0], br[1], br[2], ba], w * scale)
        }
        None => ([0.0; 4], 0.0),
    };
    // A stroke gradient reuses the fill's axis: every WinUI elevation border is vertical.
    // 描边渐变复用填充的轴：WinUI 的立面描边全是竖向。
    let (border_far, border_grad) = match p.border_gradient {
        Some(g) => {
            let (c, a) = split_color(g.to);
            ([c[0], c[1], c[2], a], 1.0)
        }
        None => ([0.0; 4], 0.0),
    };
    let fill_stops = p.gradient.map(|g| g.stops).unwrap_or((0.0, 1.0));
    let border_stops = p.border_gradient.map(|g| g.stops).unwrap_or((0.0, 1.0));
    let fill_extent = p.gradient.and_then(|g| g.extent).map(|e| e * scale).unwrap_or(0.0);
    let border_extent = p
        .border_gradient
        .and_then(|g| g.extent)
        .map(|e| e * scale)
        .unwrap_or(0.0);
    // An empty range (`x >= y`) is how the shader sees "no notch", so a missing one sends 0,0.
    let (notch_lo, notch_hi) = match p.notch {
        Some((from, to)) => (from * scale, to * scale),
        None => (0.0, 0.0),
    };
    // Light-theme elevation borders are mirrored; each brush states its own flip.
    let (fill_flip, border_flip) = (
        p.gradient.map(|g| f32::from(g.flip)).unwrap_or(0.0),
        p.border_gradient.map(|g| f32::from(g.flip)).unwrap_or(0.0),
    );
    out.extend_from_slice(&[
        r.origin.x * scale,
        r.origin.y * scale,
        r.size.w * scale,
        r.size.h * scale,
        radius.tl * scale,
        radius.tr * scale,
        radius.br * scale,
        radius.bl * scale,
        fill[0],
        fill[1],
        fill[2],
        alpha,
        border[0],
        border[1],
        border[2],
        border[3],
        border_far[0],
        border_far[1],
        border_far[2],
        border_far[3],
        border_grad,
        radius.shape.exponent(),
        far[0],
        far[1],
        far[2],
        far[3],
        axis,
        enabled,
        fill_stops.0,
        fill_stops.1,
        border_stops.0,
        border_stops.1,
        fill_extent,
        border_extent,
        width,
        notch_lo,
        notch_hi,
        fill_flip,
        border_flip,
    ]);
}

fn push_sector(out: &mut Vec<f32>, p: &ui_core::draw::SectorPrim, scale: f32) {
    let s = &p.sector;
    let (rgb, alpha) = split_color(p.colour);
    // The sweep is normalised here so the shader needs no negative-angle handling.
    // 扫动量在此归一，片元里便不必处理负角。
    let sweep = s.sweep();
    out.extend_from_slice(&[
        s.centre.x * scale,
        s.centre.y * scale,
        s.radius * scale,
        (s.radius - s.thickness).max(0.0) * scale,
        s.trim.0,
        sweep,
        rgb[0],
        rgb[1],
        rgb[2],
        alpha,
        f32::from(s.round_caps),
    ]);
}

fn upload_atlas(
    gl: &glow::Context,
    slot: &mut Option<glow::Texture>,
    width: u32,
    height: u32,
    internal: u32,
    format: u32,
    data: &[u8],
) -> Result<(), GpuError> {
    unsafe {
        let tex = match *slot {
            Some(t) => t,
            None => {
                let t = gl.create_texture()?;
                *slot = Some(t);
                t
            }
        };
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            internal as i32,
            width as i32,
            height as i32,
            0,
            format,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(data)),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    }
    Ok(())
}

fn push_glyph(out: &mut Vec<f32>, p: &GlyphPrim, scale: f32) {
    let (rgb, alpha) = split_color(p.color);
    out.extend_from_slice(&[
        p.rect.origin.x * scale,
        p.rect.origin.y * scale,
        p.rect.size.w * scale,
        p.rect.size.h * scale,
        p.atlas.origin.x,
        p.atlas.origin.y,
        p.atlas.size.w,
        p.atlas.size.h,
        rgb[0],
        rgb[1],
        rgb[2],
        alpha * p.alpha,
        match p.kind {
            ui_core::draw::GlyphKind::Coverage => 0.0,
            ui_core::draw::GlyphKind::Color => 1.0,
        },
    ]);
}

/// Whether a glyph's device rect touches a device scissor `[x, y, w, h]`.
/// 字形的设备矩形是否碰到设备裁剪框 `[x, y, w, h]`。
fn glyph_hits_scissor(p: &GlyphPrim, scale: f32, scissor: [i32; 4]) -> bool {
    let x0 = (p.rect.origin.x * scale).floor();
    let y0 = (p.rect.origin.y * scale).floor();
    let x1 = ((p.rect.origin.x + p.rect.size.w) * scale).ceil();
    let y1 = ((p.rect.origin.y + p.rect.size.h) * scale).ceil();
    x1 > scissor[0] as f32
        && y1 > scissor[1] as f32
        && x0 < (scissor[0] + scissor[2]) as f32
        && y0 < (scissor[1] + scissor[3]) as f32
}

fn push_image(out: &mut Vec<f32>, p: &ImagePrim, scale: f32, tex: (u32, u32)) {
    let r = p.rect;
    let corners = p.corners.clamp_to(r.size);
    // Normalised source rect; texels are never scaled, only the destination is.
    // 归一化的源矩形；纹素从不缩放，只有目标矩形会。
    let w = tex.0.max(1) as f32;
    let h = tex.1.max(1) as f32;
    let u0 = p.src.origin.x / w;
    let v0 = p.src.origin.y / h;
    let u1 = (p.src.origin.x + p.src.size.w) / w;
    let v1 = (p.src.origin.y + p.src.size.h) / h;
    out.extend_from_slice(&[
        r.origin.x * scale,
        r.origin.y * scale,
        r.size.w * scale,
        r.size.h * scale,
        corners.tl * scale,
        corners.tr * scale,
        corners.br * scale,
        corners.bl * scale,
        u0,
        v0,
        u1,
        v1,
        p.corners.shape.exponent(),
    ]);
}

/// Upload one RGBA8 image with `CLAMP_TO_EDGE`, so a small source sub-rect never tiles.
/// 上传一张 `CLAMP_TO_EDGE` 的 RGBA8 图像，使小的源子矩形绝不平铺。
fn upload_image(gl: &glow::Context, tex: glow::Texture, entry: &ImageEntry) -> Result<(), GpuError> {
    let needed = (entry.width as usize) * (entry.height as usize) * 4;
    if entry.width == 0 || entry.height == 0 || entry.data.len() < needed {
        return Err(GpuError::from(format!(
            "image {} is {}x{} but carries {} bytes (want {needed})",
            entry.id,
            entry.width,
            entry.height,
            entry.data.len()
        )));
    }
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            entry.width as i32,
            entry.height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(entry.data.as_slice())),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    }
    Ok(())
}

fn split_color(c: Color) -> ([f32; 3], f32) {
    ([c.r, c.g, c.b], c.a)
}

/// Apply a clip: scissor bounds the rect cheaply, the shader handles the rounded shape.
/// 应用裁剪：scissor 廉价地限制矩形，圆角形状由着色器处理。
fn set_clip_uniforms(
    gl: &glow::Context,
    program: &Program,
    clip: Option<ui_core::draw::ClipEntry>,
    scale: f32,
) {
    unsafe {
        // Must run with `program` current: setting a uniform with no program bound is
        // GL_INVALID_OPERATION, and the error then surfaces at an unrelated call.
        // 必须在 `program` 为当前程序时调用：无当前程序时设 uniform 会报
        // GL_INVALID_OPERATION，而错误会在毫不相关的调用处才显现。
        let rounded = clip.is_some_and(|c| !c.corners.is_zero());
        if let Some(u) = program.uniforms.clip_on {
            gl.uniform_1_f32(Some(&u), if rounded { 1.0 } else { 0.0 });
        }
        let Some(c) = clip.filter(|_| rounded) else {
            return;
        };
        // The corners follow the uncut box (`shape`); the scissor still bounds `rect`.
        // 圆角跟随未裁切的盒子（`shape`）；scissor 仍按 `rect` 限制。
        let clamped = c.corners.clamp_to(c.shape.size);
        if let Some(u) = program.uniforms.clip_rect {
            gl.uniform_4_f32(
                Some(&u),
                c.shape.origin.x * scale,
                c.shape.origin.y * scale,
                c.shape.size.w * scale,
                c.shape.size.h * scale,
            );
        }
        if let Some(u) = program.uniforms.clip_radius {
            gl.uniform_4_f32(
                Some(&u),
                clamped.tl * scale,
                clamped.tr * scale,
                clamped.br * scale,
                clamped.bl * scale,
            );
        }
        if let Some(u) = program.uniforms.clip_shape {
            gl.uniform_1_f32(Some(&u), clamped.shape.exponent());
        }
    }
}

fn set_scissor(
    gl: &glow::Context,
    clip: Option<ui_core::geom::Rect>,
    scale: f32,
    viewport: (f32, f32),
) {
    let clip_device = clip.map(|r| device_scissor(r, scale));
    match clip_device {
        None => unsafe { gl.disable(glow::SCISSOR_TEST) },
        Some(r) => {
            // No Y flip: this renderer maps device y=0 to GL's bottom, which is the same
            // memory row the compositor reads as the image top (see the vertex shader).
            // 不做 Y 翻转：本渲染器把设备 y=0 映射到 GL 底部，而那正是 compositor
            // 当作图像顶部的同一条内存行（见顶点着色器）。
            let _ = viewport;
            unsafe {
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(r[0], r[1], r[2], r[3]);
            }
        }
    }
}

/// A logical rect as a device scissor: round the *far edge* out, not the size, or a fractional
/// origin drops the last row the rect covers (CPU `clip_to` and Skia's `roundOut` do the same).
/// 逻辑矩形转设备裁剪框：对**远边**取上整而不是对尺寸取上整，否则分数原点会丢掉矩形覆盖的最后一行
/// （CPU 的 `clip_to` 与 Skia 的 `roundOut` 同理）。
pub(crate) fn device_scissor(r: ui_core::geom::Rect, scale: f32) -> [i32; 4] {
    let x0 = (r.origin.x * scale).floor().max(0.0);
    let y0 = (r.origin.y * scale).floor().max(0.0);
    let x1 = ((r.origin.x + r.size.w) * scale).ceil().max(0.0);
    let y1 = ((r.origin.y + r.size.h) * scale).ceil().max(0.0);
    [x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32]
}

pub(crate) fn as_bytes(data: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
}

fn compile_program(
    gl: &glow::Context,
    vertex: &str,
    fragment: &str,
    is_gles: bool,
) -> Result<glow::Program, GpuError> {
    let header = if is_gles { "#version 300 es\n" } else { "#version 330\n" };
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let shader = gl.create_shader(kind)?;
            // `AA` is spliced into every program from one definition, so the vertex
            // expansion and the fragment ramp cannot drift apart.
            // `AA` 由同一份定义拼进每个程序，顶点外扩与片元渐变因此不会走偏。
            gl.shader_source(shader, &format!("{header}{}", shader::splice(src, shader::AA_GLSL)));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let log = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                return Err(GpuError::Gl(format!("shader compile: {log}")));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            return Err(GpuError::Gl(format!("program link: {log}")));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        Ok(program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::geom::{Color, Corners, Rect};

    /// Slot offsets in a rect instance, in the order `push_rect` writes them.
    /// 矩形实例里的槽位偏移，顺序与 `push_rect` 的写入顺序一致。
    const RECT_BORDER: usize = 12;
    const RECT_BORDER_W: usize = 34;
    const RECT_NOTCH: usize = 35;

    fn rect_prim(fill: Color) -> RectPrim {
        RectPrim {
            rect: Rect::new(10.0, 20.0, 30.0, 40.0),
            radius: Corners::all(4.0),
            fill,
            gradient: None,
            border: None,
            border_gradient: None,
            notch: None,
        }
    }

    fn acrylic() -> ui_core::draw::Acrylic {
        ui_core::draw::Acrylic {
            tint: Color::from_rgba8(252, 252, 252, 0),
            luminosity: Color::from_rgba8(252, 252, 252, 217),
            noise: 0.02,
        }
    }

    fn glass() -> ui_core::draw::Glass {
        ui_core::draw::Glass::STANDARD
    }

    #[test]
    fn a_node_that_names_its_material_is_shaded_by_that_material() {
        // Per-node material is the point of `BackdropEntry::material`: a Fluent panel and a
        // glass pill in one frame must not have to share one choice.
        // 按节点选材质正是 `BackdropEntry::material` 的意义：同一帧里的 Fluent 面板与玻璃胶囊
        // 不必共用一种。
        let default = Some(BackdropMaterial::Acrylic(acrylic()));
        assert_eq!(
            Shade::of(resolve(BackdropMaterial::Glass(glass()), default)),
            Shade::Glass,
            "the node's own material wins"
        );
        assert_eq!(
            Shade::of(resolve(BackdropMaterial::Plain, default)),
            Shade::Acrylic,
            "a plain entry takes the default"
        );
    }

    #[test]
    fn a_plain_entry_leaves_the_blur_untouched_when_no_default_is_set() {
        // `Plain` means "I only asked for a blur": the renderer-wide material applies if one
        // was set, otherwise the blur is written back as-is.
        // `Plain` 意为「我只要求模糊」：设了渲染器级材质就套用，否则模糊结果原样写回。
        assert_eq!(resolve(BackdropMaterial::Plain, None), None);
        assert_eq!(Shade::of(None), Shade::Plain);
        assert_eq!(Shade::of(Some(BackdropMaterial::Plain)), Shade::Plain);
    }

    #[test]
    fn rect_instance_matches_the_shader_stride() {
        let mut out = Vec::new();
        push_rect(&mut out, &rect_prim(Color::from_rgba8(255, 0, 0, 255)), 1.0);
        assert_eq!(
            out.len(),
            shader::RECT_STRIDE as usize,
            "shader declares {0} floats per instance",
            shader::RECT_STRIDE
        );
    }

    #[test]
    fn rect_instance_fields_land_at_the_shader_offsets() {
        let mut out = Vec::new();
        push_rect(
            &mut out,
            &rect_prim(Color::from_rgba8(51, 102, 153, 204)),
            1.0,
        );
        // Layout: rect(0..4), radius(4..8), fill(8..12), border(12..16).
        // 布局：矩形(0..4)、圆角(4..8)、填充(8..12)、边框(12..16)。
        assert_eq!(&out[0..4], &[10.0, 20.0, 30.0, 40.0]);
        assert_eq!(&out[4..8], &[4.0, 4.0, 4.0, 4.0]);
        let c = Color::from_rgba8(51, 102, 153, 204);
        assert_eq!(&out[8..12], &[c.r, c.g, c.b, c.a]);
    }

    fn sector_prim(trim: (f32, f32)) -> ui_core::draw::SectorPrim {
        ui_core::draw::SectorPrim {
            bounds: ui_core::geom::Rect::new(0.0, 0.0, 32.0, 32.0),
            colour: Color::from_rgba8(0, 120, 212, 255),
            sector: ui_core::draw::Sector {
                centre: ui_core::geom::Point::new(16.0, 16.0),
                radius: 14.0,
                thickness: 2.0,
                trim,
                round_caps: true,
            },
        }
    }

    #[test]
    fn sector_instance_matches_the_shader_stride_and_offsets() {
        let mut out = Vec::new();
        push_sector(&mut out, &sector_prim((0.0, 0.25)), 2.0);
        assert_eq!(
            out.len(),
            shader::SECTOR_STRIDE as usize,
            "shader declares {0} floats per instance",
            shader::SECTOR_STRIDE
        );
        // Layout: centre(0..2), geometry(2..6), fill(6..10), caps(10).
        // 布局：圆心(0..2)、几何(2..6)、填充(6..10)、帽(10)。
        assert_eq!(&out[0..2], &[32.0, 32.0], "centre and radius scale");
        assert_eq!(out[2], 28.0, "outer radius scales");
        assert_eq!(out[3], 24.0, "the stroke grows inwards");
        assert_eq!(&out[4..6], &[0.0, 0.25], "turns do not scale");
        assert_eq!(out[10], 1.0);
    }

    #[test]
    fn a_full_turn_reaches_the_shader_as_one_whole_sweep() {
        let mut out = Vec::new();
        push_sector(&mut out, &sector_prim((0.0, 1.0)), 1.0);
        assert_eq!(out[5], 1.0, "a full turn must not fold to zero");
    }

    #[test]
    fn a_wrapping_sweep_is_normalised_before_the_shader_sees_it() {
        let mut out = Vec::new();
        push_sector(&mut out, &sector_prim((0.75, 0.25)), 1.0);
        assert_eq!(out[5], 0.5, "a sweep across zero turns stays positive");
    }

    #[test]
    fn rect_instance_scales_geometry_but_not_colour() {
        let mut a = Vec::new();
        push_rect(&mut a, &rect_prim(Color::from_rgba8(255, 0, 0, 255)), 1.0);
        let mut b = Vec::new();
        push_rect(&mut b, &rect_prim(Color::from_rgba8(255, 0, 0, 255)), 2.0);
        // Geometry doubles.
        // 几何尺寸翻倍。
        assert_eq!(&b[0..4], &[20.0, 40.0, 60.0, 80.0]);
        // Colour is scale-invariant.
        // 颜色与缩放无关。
        assert_eq!(&a[8..12], &b[8..12]);
    }

    #[test]
    fn border_colour_and_width_land_in_separate_slots() {
        let mut out = Vec::new();
        let mut prim = rect_prim(Color::from_rgba8(0, 0, 0, 255));
        let border = Color::from_rgba8(255, 128, 0, 200);
        prim.border = Some((border, 3.0));
        push_rect(&mut out, &prim, 2.0);
        // A width folded into the alpha slot makes the shader read `rgb * width` as the colour.
        // 把宽度折进 alpha 槽会让着色器把 `rgb * width` 当作颜色。
        assert_eq!(&out[RECT_BORDER..RECT_BORDER + 4], &[border.r, border.g, border.b, border.a]);
        assert_eq!(out[RECT_BORDER_W], 3.0 * 2.0, "border width must be scaled");
    }

    #[test]
    fn the_notch_travels_scaled_and_absent_sends_an_empty_range() {
        let mut out = Vec::new();
        let mut prim = rect_prim(Color::WHITE);
        prim.notch = Some((4.0, 12.0));
        push_rect(&mut out, &prim, 2.0);
        assert_eq!(&out[RECT_NOTCH..RECT_NOTCH + 2], &[8.0, 24.0], "the gap is device-space geometry");
        // The shader reads `x >= y` as no notch, so a missing one must not be (0, w).
        // 着色器把 `x >= y` 读作无缺口，因此缺省值不能是 (0, w)。
        out.clear();
        prim.notch = None;
        push_rect(&mut out, &prim, 2.0);
        assert_eq!(&out[RECT_NOTCH..RECT_NOTCH + 2], &[0.0, 0.0]);
    }

    #[test]
    fn glyph_instance_matches_the_shader_stride() {
        let mut out = Vec::new();
        push_glyph(
            &mut out,
            &GlyphPrim {
                rect: Rect::new(1.0, 2.0, 3.0, 4.0),
                atlas: Rect::new(5.0, 6.0, 7.0, 8.0),
                alpha: 0.5,
                color: Color::from_rgba8(10, 20, 30, 255),
                kind: ui_core::draw::GlyphKind::Coverage,
            },
            1.0,
        );
        assert_eq!(out.len(), shader::GLYPH_STRIDE as usize);
        assert_eq!(&out[0..4], &[1.0, 2.0, 3.0, 4.0], "dest rect");
        assert_eq!(&out[4..8], &[5.0, 6.0, 7.0, 8.0], "atlas rect");
        assert_eq!(out[11], 0.5, "alpha multiplier goes in a_color.w");
        assert_eq!(out[12], 0.0, "coverage kind selects the R8 atlas");
    }

    #[test]
    fn glyph_culling_keeps_every_glyph_that_touches_the_clip() {
        // Culling an overflowing line must never eat a visible glyph: the predicate may
        // reject only what is wholly outside, including a glyph straddling the edge.
        // 裁剪溢出的一行绝不能吃掉可见字形：判据只能拒绝完全在外的字形，跨边缘的也要留下。
        let glyph = |x: f32| GlyphPrim {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            atlas: Rect::new(0.0, 0.0, 10.0, 10.0),
            alpha: 1.0,
            color: Color::BLACK,
            kind: ui_core::draw::GlyphKind::Coverage,
        };
        let scissor = device_scissor(Rect::new(10.0, 0.0, 20.0, 20.0), 2.0);
        assert_eq!(scissor, [20, 0, 40, 40]);
        assert!(!glyph_hits_scissor(&glyph(0.0), 2.0, scissor), "entirely left");
        assert!(!glyph_hits_scissor(&glyph(40.0), 2.0, scissor), "entirely right");
        assert!(glyph_hits_scissor(&glyph(5.0), 2.0, scissor), "straddling the left edge");
        assert!(glyph_hits_scissor(&glyph(20.0), 2.0, scissor), "inside");
    }

    #[test]
    fn glyph_atlas_coordinates_are_not_scaled() {
        let mut a = Vec::new();
        let prim = GlyphPrim {
            rect: Rect::new(1.0, 2.0, 3.0, 4.0),
            atlas: Rect::new(5.0, 6.0, 7.0, 8.0),
            alpha: 1.0,
            color: Color::from_rgba8(0, 0, 0, 255),
            kind: ui_core::draw::GlyphKind::Coverage,
        };
        push_glyph(&mut a, &prim, 1.0);
        let mut b = Vec::new();
        push_glyph(&mut b, &prim, 3.0);
        // Atlas texels are never scaled; only the destination is.
        // 图集纹素从不缩放；只有目标矩形会。
        assert_eq!(&a[4..8], &b[4..8]);
        assert_eq!(&b[0..4], &[3.0, 6.0, 9.0, 12.0]);
    }

    fn image_prim(src: Rect, corners: Corners) -> ImagePrim {
        ImagePrim {
            rect: Rect::new(10.0, 20.0, 30.0, 40.0),
            corners,
            texture: 0,
            src,
        }
    }

    #[test]
    fn image_instance_matches_the_shader_stride() {
        let mut out = Vec::new();
        push_image(
            &mut out,
            &image_prim(Rect::new(0.0, 0.0, 8.0, 8.0), Corners::ZERO),
            1.0,
            (64, 64),
        );
        assert_eq!(out.len(), shader::IMAGE_STRIDE as usize);
    }

    #[test]
    fn image_uv_covers_the_source_rect_rather_than_the_whole_texture() {
        // Sampling the whole texture instead of `src` is how a small source sub-rect ends up
        // tiled or showing the wrong quadrant; the uv rect must come from `src` alone.
        // 采样整张纹理而不是 `src`，正是小源子矩形被平铺或显示错象限的原因；
        // uv 矩形必须只由 `src` 决定。
        let mut out = Vec::new();
        push_image(
            &mut out,
            &image_prim(Rect::new(16.0, 8.0, 16.0, 16.0), Corners::ZERO),
            1.0,
            (64, 32),
        );
        assert_eq!(&out[8..12], &[0.25, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn image_instance_scales_geometry_but_not_uv() {
        let prim = image_prim(Rect::new(0.0, 0.0, 32.0, 32.0), Corners::all(4.0));
        let mut a = Vec::new();
        push_image(&mut a, &prim, 1.0, (64, 64));
        let mut b = Vec::new();
        push_image(&mut b, &prim, 2.0, (64, 64));
        assert_eq!(&b[0..4], &[20.0, 40.0, 60.0, 80.0]);
        assert_eq!(&b[4..8], &[8.0, 8.0, 8.0, 8.0]);
        assert_eq!(&a[8..12], &b[8..12]);
    }

    #[test]
    fn image_corners_are_clamped_to_half_the_shorter_side() {
        // A radius beyond half the side has no shape to describe; the shader's SDF would
        // invert the corner instead of rounding it.
        // 超过短边一半的半径没有可描述的形状；着色器的 SDF 会把圆角翻转而不是切圆。
        let mut out = Vec::new();
        push_image(
            &mut out,
            &image_prim(Rect::new(0.0, 0.0, 8.0, 8.0), Corners::all(999.0)),
            1.0,
            (8, 8),
        );
        assert_eq!(&out[4..8], &[15.0, 15.0, 15.0, 15.0]);
    }

    #[test]
    fn rect_corners_are_clamped_to_half_the_shorter_side() {
        // A short track segment with a capsule radius would otherwise render as a crescent
        // on the GPU while the CPU path draws a capsule; both must agree.
        // 短轨道段带胶囊半径时，GPU 会画成月牙而 CPU 画成胶囊；两边必须一致。
        let mut out = Vec::new();
        let mut prim = rect_prim(Color::from_rgba8(255, 0, 0, 255));
        prim.rect = Rect::new(0.0, 0.0, 8.0, 40.0);
        prim.radius = Corners { tl: 20.0, tr: 2.0, br: 2.0, bl: 20.0, ..Corners::ZERO };
        push_rect(&mut out, &prim, 1.0);
        assert_eq!(&out[4..8], &[4.0, 2.0, 2.0, 4.0]);
    }

    #[test]
    fn a_scissor_keeps_the_row_its_far_edge_covers() {
        // 207.8..256.8 covers device row 256; `ceil(size)` from a floored origin stops at 255 and
        // silently drops the row, which is how a 1dp divider under a fractional box disappears.
        // 207.8..256.8 覆盖设备行 256；从取整原点起 `ceil(size)` 会止于 255 并静默丢掉该行，
        // 分数盒下方的 1dp 分隔线就是这样消失的。
        assert_eq!(device_scissor(Rect::new(0.0, 207.8, 10.0, 49.0), 1.0), [0, 207, 10, 50]);
        // The far edge rounds out in device space, not in logical space.
        // 远边在设备空间取上整，而不是逻辑空间。
        assert_eq!(device_scissor(Rect::new(0.0, 10.1, 4.0, 2.0), 1.5), [0, 15, 6, 4]);
        // An integral rect is untouched.
        // 整数矩形不受影响。
        assert_eq!(device_scissor(Rect::new(3.0, 4.0, 5.0, 6.0), 1.0), [3, 4, 5, 6]);
    }
}
