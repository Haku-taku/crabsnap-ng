//! The interactive glass material, with every value traceable to the reference source.
//! 交互玻璃材质，每个数值都可追溯到参考源码。
//!
//! The reference defaults these controls to `GlassQuality.standard`; its premium path is the
//! same material Apple ships, and the values below are the premium ones
//! (`glass_switch.dart:694-720`, `glass_slider.dart:741-770`). We run premium.
//! 参考给这些控件的默认值是 `GlassQuality.standard`；它的 premium 路径才是 Apple 那套材质，
//! 下面取的就是 premium 的数值（`glass_switch.dart:694-720`、`glass_slider.dart:741-770`）。

use ui_core::geom::Color;
use ui_design::Appearance;
use ui_style::Style;

/// One glass surface's resolved values, before the device scale is applied.
/// 一块玻璃平面解析后的数值，尚未乘设备缩放。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassRecipe {
    /// `glassColor`: body wash; `a` mixes in `rgb × 0.2`.
    /// `glassColor`：本体洗色；`a` 用它混入 `rgb × 0.2`。
    pub tint: Color,
    /// `rimThickness` after normalization, in dp.
    /// 归一化后的 `rimThickness`，单位 dp。
    pub rim_thickness: f32,
    /// `rimSmoothing`: how many pixels the hairline takes to fade.
    /// `rimSmoothing`：发丝边淡出所需的像素数倍率。
    pub rim_smoothing: f32,
    /// `ambientRim` after normalization: the ring hugging the edge, `0..=1`.
    /// 归一化后的 `ambientRim`：贴边环光强度，`0..=1`。
    pub ambient_rim: f32,
    /// `ambientStrength` after normalization: flat lift over the whole body.
    /// 归一化后的 `ambientStrength`：整块本体抬亮的常数。
    pub ambient_strength: f32,
    /// `lightDirection`: the unit vector the key highlight faces.
    /// `lightDirection`：主高光朝向的单位向量。
    pub light_direction: [f32; 2],
    /// `lightIntensity` after normalization; `0` on the standard path.
    /// 归一化后的 `lightIntensity`；standard 路径下为 `0`。
    pub light_intensity: f32,
    /// `saturation`: multiplier on the refracted backdrop.
    /// `saturation`：折射背景的饱和度乘数。
    pub saturation: f32,
    /// `glowIntensity` after normalization.
    /// 归一化后的 `glowIntensity`。
    pub glow_intensity: f32,
    pub chromatic: f32,
    pub edge_absorption: f32,
    /// `edgeZone` in dp: how far the edge bend reaches inward (standard path only).
    /// `edgeZone`（dp）：边缘弯折向内的影响距离（仅 standard 路径）。
    pub edge_zone: f32,
    /// `refractiveIndex`: how hard the lens bends the backdrop.
    /// `refractiveIndex`：透镜弯折背景的强度。
    pub refractive_index: f32,
    /// `thickness` in dp: the bevel's quarter-circle radius.
    /// `thickness`（dp）：倒角四分之一圆的半径。
    pub thickness: f32,
    /// `lensModel`: the exact Snell bevel, or iOS 27's gentler paraxial band.
    /// `lensModel`：精确斯涅尔倒角，还是 iOS 27 那道更平缓的近轴边缘带。
    pub lens_model: ui_core::draw::LensModel,
    /// `rimShade`: the iOS 27 hairline outline, `0` = off.
    /// `rimShade`：iOS 27 发丝描边，`0` 表示不画。
    pub rim_shade: f32,
    /// `rimShadeEnds`: how much of the shade survives at the light axis' ends.
    /// `rimShadeEnds`：光轴两端还保留多少压暗。
    pub rim_shade_ends: f32,
    /// How much the backdrop under the face is shrunk (LiquidGlassKit's `magnification`).
    /// 面下背景缩小多少（LiquidGlassKit 的 `magnification`）。
    pub magnification: f32,
    /// `fresnelStrength`: grazing-angle lift at the rim.
    /// `fresnelStrength`：掠射角处的边缘提亮。
    pub fresnel_strength: f32,
    /// `whitenStrength`: veil toward white, 0..=1.
    /// `whitenStrength`：向白色漂洗的强度，0..=1。
    pub whiten: f32,
    /// `blur` in dp: the backdrop's frost radius, 0 for the bare lens.
    /// `blur`（dp）：背景的磨砂半径，裸透镜为 0。
    pub blur: f32,
}

/// Which interactive control the material is for: they differ on the premium path.
/// 材质属于哪个交互控件：两者在 premium 路径上的数值不同。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Switch,
    Slider,
    /// The bar's own capsule: a wide surface that frosts rather than lenses.
    /// 标签栏自身的胶囊：一块宽面，做磨砂而非透镜。
    TabBar,
    /// The capsule's moving lens: clear, strongly lit, no frost.
    /// 胶囊上移动的透镜：通透、强受光、不磨砂。
    TabLens,
}

/// The interactive glass for the pack's appearance (`glass_switch.dart:694-720`).
/// 指定外观下的交互玻璃（`glass_switch.dart:694-720`）。
pub fn glass_recipe(pack: &Style, control: Control) -> GlassRecipe {
    let dark = pack.appearance() == Some(Appearance::Dark);
    // Both controls share the glass colour and the premium rim/ambient values. Their lens is
    // LiquidGlassKit's thumb preset: `glassThickness: 10`, `refractiveIndex: 1.11`, and a
    // 0.75 magnification on the switch's expanded thumb (`LiquidGlassView.swift:56-78`,
    // `LiquidGlassSwitch.swift:70`).
    // 两个控件共用玻璃色以及 premium 的边缘、环光数值。透镜取 LiquidGlassKit 的滑块预设：
    // `glassThickness: 10`、`refractiveIndex: 1.11`，开关展开态再叠 0.75 的放大系数
    // （`LiquidGlassView.swift:56-78`、`LiquidGlassSwitch.swift:70`）。
    let (tint, refractive_index, thickness, light_intensity) = match (control, dark) {
        (Control::Switch, false) => (Color::rgba(0.88, 0.88, 0.90, 0.12), 1.11, 10.0, 2.0),
        (Control::Switch, true) => (Color::rgba(1.0, 1.0, 1.0, 0.08), 1.11, 10.0, 2.0),
        (Control::Slider, false) => (Color::rgba(0.88, 0.88, 0.90, 0.12), 1.11, 10.0, 1.8),
        (Control::Slider, true) => (Color::rgba(1.0, 1.0, 1.0, 0.08), 1.11, 10.0, 1.8),
        // `kBottomBarGlassDefaults` is identical in both appearances (`tab_bar_bottom_internal.dart:47-57`).
        // `kBottomBarGlassDefaults` 在两种外观下完全相同（`tab_bar_bottom_internal.dart:47-57`）。
        (Control::TabBar, _) => (Color::rgba(1.0, 1.0, 1.0, 0.24), 1.59, 30.0, 0.6),
        (Control::TabLens, _) => (Color::rgba(1.0, 1.0, 1.0, 0.0), 1.10, 20.0, 2.0),
    };
    let bar = matches!(control, Control::TabBar);
    let lens = matches!(control, Control::TabLens);
    // The interactive controls wear the iOS 27 material the device shows: the paraxial rim
    // band and the hairline outline. The bar keeps the older, already-approved look.
    // 交互控件用设备上那套 iOS 27 材质：近轴边缘带与发丝描边。标签栏保持已验收的旧外观。
    let ios27 = !bar && !lens;
    GlassRecipe {
        tint,
        // Premium rim thickness, as the reference passes it (`:708`, `:754`).
        // premium 的边缘厚度，取参考传入值。
        rim_thickness: if matches!(control, Control::TabLens) { 0.8 } else { 0.5 },
        rim_smoothing: 1.5,
        // Premium ambient rim (`:712`, `:760`).
        // premium 的环境环光。
        ambient_rim: if bar { 0.0 } else { 0.1 },
        // `ambientStrength` defaults to 0 (`liquid_glass_settings.dart:118`).
        // `ambientStrength` 默认 0。
        ambient_strength: if bar { 1.0 } else { 0.0 },
        // `GlassDefaults.lightAngle` = 0.75π, passed as `[cos, -sin]`.
        // `GlassDefaults.lightAngle` = 0.75π，以 `[cos, -sin]` 传入。
        light_direction: [-0.70710678, -0.70710678],
        light_intensity,
        saturation: if bar { 0.7 } else { 1.5 },
        glow_intensity: 0.75,
        // `chromaticAberration` default: `liquid_glass_settings.dart:109` (0.01).
        // `chromaticAberration` 默认值：`liquid_glass_settings.dart:109`（0.01）。
        // The bar disperses hard (0.3); the lens sits on it with none.
        // 栏的色散很强（0.3）；它上面的透镜不色散。
        chromatic: if bar {
            0.3
        } else if lens {
            0.0
        } else {
            0.01
        },
        edge_absorption: 0.0,
        edge_zone: 14.0,
        refractive_index,
        thickness,
        lens_model: if ios27 {
            ui_core::draw::LensModel::Paraxial
        } else {
            ui_core::draw::LensModel::Spherical
        },
        // `LiquidGlassSettings.ios27Light/Dark` both carry `rimShade: 1`; the light preset
        // keeps 20 % of it at the light axis' ends, the dark one none.
        // `ios27Light/Dark` 都是 `rimShade: 1`；浅色预设保留轴端 20%，深色不留。
        rim_shade: if ios27 { 1.0 } else { 0.0 },
        rim_shade_ends: if dark { 0.0 } else { 0.2 },
        // The switch's expanded thumb captures a wider backdrop and draws it back down;
        // the slider's does not (`LiquidGlassSwitch.swift:70` vs `LiquidGlassSlider.swift:111`).
        // 开关展开态的滑块捕获更大范围的背景再缩回；滑块的没有
        // （`LiquidGlassSwitch.swift:70` 对比 `LiquidGlassSlider.swift:111`）。
        magnification: if matches!(control, Control::Switch) { 0.75 } else { 1.0 },
        fresnel_strength: 1.0,
        whiten: 0.0,
        // The bar frosts 3 dp; the lens samples the bar unfrosted.
        // 标签栏磨砂 3dp；透镜采样的是未磨砂的那一层。
        blur: if bar { 3.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appearance drives the glass colour and the hairline: the lens itself is shared, but
    /// the dark pack's line dissolves into the rim light at the ends of the light axis.
    /// 外观驱动玻璃色与描边：透镜本身共用，但深色包的线在光轴两端融进边缘光里。
    #[test]
    fn the_dark_pack_gets_its_own_tint_and_hairline_ends() {
        let light = glass_recipe(&crate::pack_of(Appearance::Light), Control::Slider);
        let dark = glass_recipe(&crate::pack_of(Appearance::Dark), Control::Slider);
        assert_ne!(light.tint, dark.tint);
        assert!((light.rim_shade_ends - 0.2).abs() < 1e-5);
        assert!(dark.rim_shade_ends.abs() < 1e-5);
    }

    /// The interactive controls wear LiquidGlassKit's thumb lens; the bar keeps the older
    /// one, and must not pick the hairline up by accident. Only the switch's expanded thumb
    /// shrinks the backdrop under its face.
    /// 交互控件用 LiquidGlassKit 的滑块透镜；标签栏保持旧透镜，且不得意外带上描边。
    /// 只有开关展开态的滑块会缩小面下的背景。
    #[test]
    fn the_interactive_glass_carries_the_liquid_glass_kit_lens() {
        for control in [Control::Switch, Control::Slider] {
            let r = glass_recipe(&crate::pack(), control);
            assert_eq!(r.lens_model, ui_core::draw::LensModel::Paraxial);
            assert!((r.thickness - 10.0).abs() < 1e-5, "{control:?} thickness");
            assert!((r.refractive_index - 1.11).abs() < 1e-5, "{control:?} index");
            assert!((r.rim_shade - 1.0).abs() < 1e-5, "{control:?} hairline");
        }
        let switch = glass_recipe(&crate::pack(), Control::Switch);
        let slider = glass_recipe(&crate::pack(), Control::Slider);
        assert!((switch.magnification - 0.75).abs() < 1e-5, "the switch shrinks its face");
        assert!((slider.magnification - 1.0).abs() < 1e-5, "the slider does not");
        let bar = glass_recipe(&crate::pack(), Control::TabBar);
        assert_eq!(bar.lens_model, ui_core::draw::LensModel::Spherical);
        assert!(bar.rim_shade.abs() < 1e-5);
    }
}
