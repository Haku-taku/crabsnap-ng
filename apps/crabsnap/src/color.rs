//! 输出的颜色描述：合成器写进缓冲区的是什么。
//!
//! 描述跟着像素走，不跟着屏幕走：抓到的帧带着它的主色、传递函数与亮度范围，之后无论是显示
//! 还是存盘，都由绘制端按目标域换算。这里只负责把 `wp_color_manager_v1` 的说法读成一个
//! [`ContentColour`]，不做任何转换——转换一旦在入口发生，像素就永久损失了原本的幅面。
//!
//! 描述读不到、或者编码在这台机器上无法解释时返回 `None`：帧保持未描述，调用方按 sRGB 处理，
//! 宁可少做，也不假装知道。

use std::time::{Duration, Instant};

use ui_core::colour::{Chromaticities, ContentColour, PixelStorage, TransferFunction};
use ui_platform_wayland::WaylandLink;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::color_management::v1::client::wp_color_management_output_v1::WpColorManagementOutputV1;
use wayland_protocols::wp::color_management::v1::client::wp_color_manager_v1::{
    Primaries as NamedPrimaries, TransferFunction as NamedTransfer, WpColorManagerV1,
};
use wayland_protocols::wp::color_management::v1::client::wp_image_description_info_v1::{
    Event as InfoEvent, WpImageDescriptionInfoV1,
};
use wayland_protocols::wp::color_management::v1::client::wp_image_description_v1::WpImageDescriptionV1;

/// One output's description, everything a [`ContentColour`] needs but the packing.
pub(crate) struct Description {
    primaries: Chromaticities,
    transfer: TransferFunction,
    /// Peak luminance the output can carry, in nits; what tone mapping needs. The white
    /// level is not carried: PQ is absolute in nits, and every other transfer is relative
    /// to the destination's own white.
    max_nits: f32,
}

impl Description {
    /// What pixels in `storage` mean, given this description.
    pub(crate) fn content(&self, storage: PixelStorage) -> ContentColour {
        ContentColour {
            storage,
            primaries: self.primaries,
            transfer: self.transfer,
            max_nits: self.max_nits,
        }
    }

    /// Read one output's description, or `None` when the compositor says nothing usable.
    pub(crate) fn read(link: &WaylandLink, output: &WlOutput) -> Option<Self> {
        let mut queue = link.new_queue::<ColorState>();
        let qh = queue.handle();
        let manager = link
            .globals()
            .bind::<WpColorManagerV1, ColorState, ()>(&qh, 1..=3, ())
            .ok()?;
        let target = manager.get_output(output, &qh, ());
        let description = target.get_image_description(&qh, ());
        let info = description.get_information(&qh, ());
        let _ = link.connection().flush();
        let mut state = ColorState::default();
        // The description is a handful of events; a compositor that does not finish in a
        // second is not going to answer this probe at all.
        let deadline = Instant::now() + Duration::from_secs(1);
        while !state.done && Instant::now() < deadline {
            queue.roundtrip(&mut state).ok()?;
        }
        let connection = link.connection();
        // The info object is destroyed by its own `done`; the rest are ours to release.
        drop(info);
        description.destroy();
        target.destroy();
        manager.destroy();
        let _ = connection.flush();
        if !state.done || state.icc_only {
            return None;
        }
        Self::from_parts(&state)
    }

    /// `None` when the description is outside what this build can map.
    ///
    /// The primaries come as named values or as coordinates; coordinates are used whenever the
    /// compositor sends them, because a panel's measured primaries are rarely the standard set
    /// a name would claim.
    fn from_parts(state: &ColorState) -> Option<Self> {
        let primaries = match state.primaries_xy {
            Some(xy) => xy,
            None => match state.primaries? {
                NamedPrimaries::Srgb => Chromaticities::SRGB,
                NamedPrimaries::Bt2020 => Chromaticities::BT2020,
                NamedPrimaries::DisplayP3 => Chromaticities::DISPLAY_P3,
                _ => return None,
            },
        };
        let transfer = match (state.transfer, state.tf_power) {
            (Some(NamedTransfer::Srgb), _) | (Some(NamedTransfer::ExtSrgb), _) => {
                TransferFunction::Srgb
            }
            (Some(NamedTransfer::Gamma22), _) => TransferFunction::Gamma(2.2),
            (Some(NamedTransfer::Gamma28), _) => TransferFunction::Gamma(2.8),
            (Some(NamedTransfer::Bt1886), _) => TransferFunction::Gamma(2.4),
            (Some(NamedTransfer::ExtLinear), _) => TransferFunction::Linear,
            (Some(NamedTransfer::St2084Pq), _) => TransferFunction::Pq,
            (Some(_), _) => return None,
            (None, Some(exponent)) => TransferFunction::Gamma(exponent),
            (None, None) => return None,
        };
        Some(Self {
            primaries,
            transfer,
            max_nits: state.max_nits.unwrap_or(0.0),
        })
    }
}

/// What the description's events say, collected until `done`.
#[derive(Default)]
struct ColorState {
    primaries: Option<NamedPrimaries>,
    /// Primaries in xy order: red, green, blue, white.
    primaries_xy: Option<Chromaticities>,
    transfer: Option<NamedTransfer>,
    /// An exponent for a transfer function given by power rather than by name.
    tf_power: Option<f32>,
    max_nits: Option<f32>,
    icc_only: bool,
    done: bool,
}

impl Dispatch<WpColorManagerV1, ()> for ColorState {
    fn event(
        _: &mut Self,
        _: &WpColorManagerV1,
        _: <WpColorManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpColorManagementOutputV1, ()> for ColorState {
    fn event(
        _: &mut Self,
        _: &WpColorManagementOutputV1,
        _: <WpColorManagementOutputV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpImageDescriptionV1, ()> for ColorState {
    fn event(
        _: &mut Self,
        _: &WpImageDescriptionV1,
        _: <WpImageDescriptionV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpImageDescriptionInfoV1, ()> for ColorState {
    fn event(
        state: &mut Self,
        _: &WpImageDescriptionInfoV1,
        event: InfoEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            InfoEvent::PrimariesNamed {
                primaries: WEnum::Value(named),
            } => state.primaries = Some(named),
            InfoEvent::Primaries {
                r_x,
                r_y,
                g_x,
                g_y,
                b_x,
                b_y,
                w_x,
                w_y,
            } => {
                let xy = |x: i32, y: i32| (x as f32 / 1e6, y as f32 / 1e6);
                state.primaries_xy = Some(Chromaticities {
                    red: xy(r_x, r_y),
                    green: xy(g_x, g_y),
                    blue: xy(b_x, b_y),
                    white: xy(w_x, w_y),
                });
            }
            InfoEvent::TfPower { eexp } => state.tf_power = Some(eexp as f32 / 10_000.0),
            InfoEvent::TfNamed {
                tf: WEnum::Value(named),
            } => state.transfer = Some(named),
            InfoEvent::Luminances { max_lum, .. } => state.max_nits = Some(max_lum as f32),
            InfoEvent::IccFile { .. } => state.icc_only = true,
            InfoEvent::Done => state.done = true,
            _ => {}
        }
    }
}
