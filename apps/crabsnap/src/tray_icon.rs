//! 托盘里的那张图。
//!
//! 通知区不接受引擎的绘制列表，只要像素，所以选框在这里光栅成一张 32×32 的图。形状与截图
//! 界面用的是同一套图元，图标才不像另一套视觉；通知区也不会替应用换色，颜色得自己两边都立住。

use ui_core::draw::{DrawList, RectPrim};
use ui_core::geom::{Color, Corners, Rect};
use ui_paint::{CpuRenderer, Pixels};
use ui_platform::tray::Icon;

/// 图标边长：桌面在通知区里自己按比例缩放，这块给的是 100% 时要画的那张。
const SIZE: u32 = 32;
/// 选框的线宽与圆角，按 32 点的画布取。
const FRAME: f32 = 3.0;
const RADIUS: f32 = 8.0;
/// 选框距离图边多远，按边框中心算。
const INSET: f32 = 5.5;
/// 选框的颜色：浅色和深色任务栏上都立得住的一支珊瑚红。
const INK: Color = Color::from_rgba8(0xE5, 0x48, 0x4D, 255);

pub(crate) fn tray_icon() -> Result<Icon, String> {
    let side = SIZE as f32 - INSET * 2.0;
    let mut list = DrawList::new(Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32));
    list.push_rect(
        DrawList::canvas_owner(),
        RectPrim {
            rect: Rect::new(INSET, INSET, side, side),
            radius: Corners::all(RADIUS),
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((INK, FRAME)),
            border_gradient: None,
            notch: None,
        },
    );
    let mut pixels = Pixels::new(SIZE, SIZE);
    let mut renderer = CpuRenderer::new();
    renderer.render(&list, &mut pixels, &ui_core::damage::Damage::Full);
    Icon::from_rgba(ui_paint::encode_rgba_straight(&pixels), SIZE, SIZE)
}
