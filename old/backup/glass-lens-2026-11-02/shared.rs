//! The glass material both interactive controls share.
//! 两个交互式玻璃控件共用的玻璃材质。

use ui_core::BackdropBlur;
use ui_core::draw::{BackdropMaterial, BlendShape, Glass, BLEND_SHAPES};

/// The jelly both interactive controls wobble with: a snappy 350 ms spring on the value.
/// 两个交互控件共用的果冻：跟踪同一个值的 snappy 350ms 弹簧。
pub const JELLY_STIFFNESS: f32 = 322.29;
pub const JELLY_DAMPING: f32 = 30.51;

/// The physical speed, in dp/s, at which the jelly reaches full distortion. The reference
/// scales a normalised-position velocity by 2 over its 201 dp slider travel.
/// 果冻达到最大形变的物理速度（dp/s）。参考把归一化位置的 2 倍速度当作满值，其滑块行程 201dp。
pub const JELLY_FULL_SPEED: f32 = 402.0;
/// The reference's 0.6 would squash a 1.6:1 knob into a circle; stop well short of it.
/// 参考的 0.6 会把 1.6:1 的滑块压成圆形；远在它之前就收住。
pub const JELLY_MAX: f32 = 0.3;

/// The scale pair for a value moving at `velocity` dp/s: squashed across, stretched along.
/// 值以 `velocity`（dp/s）移动时的缩放对：横向压扁、纵向拉伸。
///
/// Velocity is physical so both controls wobble alike: a switch's 18.8 dp travel would
/// otherwise saturate the same spring a slider needs 201 dp to reach.
/// 速度是物理量，两个控件的抖动因此一致：否则开关 18.8dp 的行程会把滑块需要 201dp 才到的
/// 同一个弹簧直接打满。
pub fn jelly_of(velocity: f32) -> (f32, f32) {
    let stretch = (velocity.abs() / JELLY_FULL_SPEED).clamp(0.0, 1.0) * JELLY_MAX;
    (1.0 - stretch * 0.5, 1.0 + stretch * 0.3)
}

/// The interactive glass both controls carry: no blur (`blur: 0`), refraction only.
/// 两个控件都带的交互玻璃：不模糊（`blur: 0`），只做折射。
///
/// `None` below the threshold, where the white material still covers the shape and the
/// pass cannot show. `interaction` is the reference's `interactionIntensity`; `scale`
/// turns the recipe's dp lengths into the device pixels the backdrop pass works in.
/// 低于阈值时为 `None`：此时白色物质仍覆盖形状，那一趟看不见。`interaction` 即参考的
/// `interactionIntensity`；`scale` 把配方的 dp 长度换成背景趟使用的设备像素。
pub fn glass_backdrop(
    recipe: &crate::material::GlassRecipe,
    interaction: f32,
    scale: f32,
) -> Option<BackdropBlur> {
    material(recipe, interaction, scale, &[], 0.0)
}

/// The same glass with `shapes` fused into it, in the node's own logical space.
/// 同一块玻璃，融合了 `shapes`，坐标在节点自身的逻辑空间。
///
/// `k` is the smooth-union radius in dp: two shapes further apart than `k` never neck.
/// `k` 是平滑并集半径（dp）：相距超过 `k` 的两个形状不会连成液桥。
pub fn glass_blend(
    recipe: &crate::material::GlassRecipe,
    interaction: f32,
    scale: f32,
    shapes: &[BlendShape],
    k: f32,
) -> Option<BackdropBlur> {
    material(recipe, interaction, scale, shapes, k)
}

fn material(
    recipe: &crate::material::GlassRecipe,
    interaction: f32,
    scale: f32,
    shapes: &[BlendShape],
    k: f32,
) -> Option<BackdropBlur> {
    let interaction = interaction.clamp(0.0, 1.0);
    if interaction <= 0.02 {
        return None;
    }
    let mut blend = [BlendShape::NONE; BLEND_SHAPES];
    for (slot, shape) in blend.iter_mut().zip(shapes) {
        *slot = *shape;
    }
    Some(BackdropBlur {
        radius: 1,
        // The lens frosted nothing (`blur: 0`); the bar frosts 3 dp.
        // 透镜不磨砂（`blur: 0`）；标签栏磨砂 3dp。
        sigma: recipe.blur * scale,
        material: BackdropMaterial::Glass(Glass {
            quality: ui_core::draw::GlassQuality::Premium,
            tint: recipe.tint,
            rim_thickness: recipe.rim_thickness * scale,
            rim_smoothing: recipe.rim_smoothing,
            ambient_rim: recipe.ambient_rim,
            ambient_strength: recipe.ambient_strength,
            // Over a captured backdrop the body is translucent glass, not the multiplier.
            // 有截图背景时本体是半透明玻璃，而不是那个乘数。
            base_alpha: 0.70,
            edge_alpha: 0.95,
            light_direction: recipe.light_direction,
            light_intensity: recipe.light_intensity,
            saturation: recipe.saturation,
            glow_intensity: recipe.glow_intensity,
            interaction,
            chromatic: recipe.chromatic,
            edge_absorption: recipe.edge_absorption,
            edge_zone: recipe.edge_zone * scale,
            refractive_index: recipe.refractive_index,
            thickness: recipe.thickness,
            lens_model: recipe.lens_model,
            rim_shade: recipe.rim_shade,
            rim_shade_ends: recipe.rim_shade_ends,
            magnification: recipe.magnification,
            fresnel_strength: recipe.fresnel_strength,
            whiten: recipe.whiten,
            blend,
            blend_count: shapes.len().min(BLEND_SHAPES) as u8,
            blend_k: k,
        }),
    })
}
