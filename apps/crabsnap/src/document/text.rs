//! A text mark: the frame it paints, the block it draws, and the one geometry both the
//! picture and its field read.
//!
//! The frame is derived, never dragged loose: it hugs the shaped text plus the padding, or
//! takes the width the user pulled it out to. Both the mark on the canvas and the field over
//! it call [`TextMark::frame`], so the two cannot drift apart the way two hand-aligned boxes
//! do. Shaping is the host's and is asked for during the build, so the frame a picture is
//! painted with is the frame measured on that same frame.

use std::sync::Arc;

use ui_core::cx::Cx;
use ui_core::draw::{DrawList, RectPrim, TextPrim};
use ui_core::geom::{Color, Corners, Point, Rect, Size};
use ui_core::text::{TextBlock, TextRun, TextStyle};
use ui_core::{NodeId, TextAlign};

use super::Style;

/// A text mark's type size in image pixels, which is the picture's own unit.
pub const DEFAULT_SIZE: f32 = 20.0;
/// The range the format allows for a type size; the wheel walks it a pixel at a time.
pub const MIN_SIZE: f32 = 8.0;
pub const MAX_SIZE: f32 = 200.0;
/// Space between the text and the frame's edge, and its range.
pub const DEFAULT_PADDING: f32 = 4.0;
pub const MAX_PADDING: f32 = 64.0;
/// The frame's corner. A radius past the padding rounds into the glyphs' box, which is a look
/// the bar offers on purpose: padding is how a user gives the corner room back.
pub const DEFAULT_RADIUS: f32 = 4.0;
pub const MAX_RADIUS: f32 = 64.0;
/// The glyph outline's width and range, in image pixels.
pub const DEFAULT_OUTLINE: f32 = 2.0;
pub const MAX_OUTLINE: f32 = 16.0;
/// The frame's hairline, painted inside the frame so no pixel outside it moves.
pub const BORDER: f32 = 1.0;
/// The wash a text mark starts with: dark and translucent, so the picture reads under it
/// and the glyphs read on any picture.
pub const DEFAULT_BACKGROUND: Color = Color::from_rgba8(20, 20, 20, 199);
/// An empty buffer still needs a box to put a caret in, this many ems wide.
const EMPTY_WIDTH: f32 = 0.3;

/// What a text mark draws around its glyphs, beyond the ink its style carries.
#[derive(Clone, Debug, PartialEq)]
pub struct TextMark {
    /// The frame's top-left corner, in image pixels.
    pub at: Point,
    pub text: String,
    pub size: f32,
    /// The width the user pulled the frame out to; `None` hugs the shaped text.
    pub width: Option<f32>,
    pub padding: f32,
    pub radius: f32,
    pub align: TextAlign,
    pub background: Option<Color>,
    pub border: Option<Color>,
    /// Ink drawn around each glyph, at [`Self::outline_width`].
    pub outline: Option<Color>,
    pub outline_width: f32,
    /// The shaped text's size, as the last measurement found it. The frame is read from it,
    /// so whatever paints a text mark has to measure first: see [`Self::measure`].
    pub content: Size,
    /// The inputs `content` was measured from; a mark whose key still matches is not shaped
    /// a second time. It is cache bookkeeping, so it takes no part in a mark's identity.
    measured: Option<Measured>,
}

/// What a measurement depended on. The wrap is the width the lines were offered, not the
/// frame's: the frame itself is derived from the measurement.
#[derive(Clone, Debug, PartialEq)]
struct Measured {
    text: String,
    size: f32,
    wrap: f32,
    align: TextAlign,
}

impl TextMark {
    /// A mark about to be typed into at `at`.
    pub fn new(at: Point) -> Self {
        Self {
            at,
            text: String::new(),
            size: DEFAULT_SIZE,
            width: None,
            padding: DEFAULT_PADDING,
            radius: DEFAULT_RADIUS,
            align: TextAlign::Start,
            background: Some(DEFAULT_BACKGROUND),
            border: None,
            outline: None,
            outline_width: DEFAULT_OUTLINE,
            content: Size::new(DEFAULT_SIZE * 0.6, DEFAULT_SIZE * 1.3),
            measured: None,
        }
    }

    /// The box this mark paints, in image pixels.
    pub fn frame(&self) -> Rect {
        let hugged = self.content.w.max(self.size * EMPTY_WIDTH) + 2.0 * self.padding;
        let width = self.width.map_or(hugged, |width| width.max(hugged));
        Rect::new(
            self.at.x,
            self.at.y,
            width,
            self.content.h + 2.0 * self.padding,
        )
    }

    /// The width the text is measured at: the frame's interior when one was pulled out.
    pub fn measure_width(&self) -> f32 {
        match self.width {
            Some(width) => (width - 2.0 * self.padding).max(0.0),
            None => f32::INFINITY,
        }
    }

    /// The width the lines are offered when painting. A hugged frame has no width of its
    /// own: its lines were measured unbroken, so paint offers them the same freedom —
    /// breaking at the measured width would fold a line the measurement kept whole as soon
    /// as shaping rounds it a fraction wider than the size it reported.
    pub fn wrap(&self) -> f32 {
        match self.width {
            Some(_) => self.measure_width().max(self.content.w),
            None => f32::INFINITY,
        }
    }

    /// The corner the frame is drawn with: never past half its short side, where the two
    /// arcs would meet and the box would stop being a rounded rectangle.
    pub fn corner(&self) -> f32 {
        let frame = self.frame();
        self.radius
            .clamp(0.0, MAX_RADIUS)
            .min(frame.size.w.min(frame.size.h) * 0.5)
    }

    /// Whether the glyphs take an outline at all.
    pub fn outlined(&self) -> bool {
        self.outline.is_some() && self.outline_width > 0.0
    }

    /// The paragraph both the picture and the field draw: the same text, size and alignment.
    pub fn block(&self) -> TextBlock {
        self.block_of(self.text.clone())
    }

    fn block_of(&self, text: String) -> TextBlock {
        let mut style = TextStyle::new(self.size);
        if let Some(color) = self.outline
            && self.outlined()
        {
            style = style.outline(self.outline_width, color);
        }
        TextBlock {
            runs: vec![TextRun::new(text, style)],
            align: self.align,
            ..Default::default()
        }
    }

    /// This mark with `preedit` spliced at `caret`: the paragraph its commit will leave,
    /// and where the preedit starts in it.
    ///
    /// The copy has not been measured. Whoever draws it measures it first, so the frame on
    /// screen is the box the committed text will produce rather than the one measured
    /// before the composition existed.
    pub fn composed_with(&self, caret: usize, preedit: &ui_core::Preedit) -> (TextMark, usize) {
        let (text, at) = ui_core::text_input::composed(&self.text, &preedit.text, caret);
        let shown = TextMark {
            text,
            measured: None,
            ..self.clone()
        };
        (shown, at)
    }

    /// Shape the text again if its inputs moved, and answer whether the frame changed size.
    ///
    /// A headless build has no measurer; the estimate the mark already carries then stands,
    /// rather than a width invented from a guess.
    pub fn measure(&mut self, cx: &Cx<'_>) -> bool {
        let key = Measured {
            text: self.text.clone(),
            size: self.size,
            wrap: self.measure_width(),
            align: self.align,
        };
        if self.measured.as_ref() == Some(&key) {
            return false;
        }
        let Some(size) = cx.text_size(&self.block(), key.wrap) else {
            return false;
        };
        let changed = self.content != size;
        self.content = size;
        self.measured = Some(key);
        changed
    }

    /// This mark in a frame `width` wide, with its right edge at `right` when one is given
    /// and its left edge where it was otherwise. A side grip sets how wide the text may
    /// run and leaves the glyphs their size: the frame only ever widens what the text
    /// needs, so an alignment has slack to move within.
    pub fn widened(&self, width: f32, right: Option<f32>) -> TextMark {
        let width = width.max(self.content.w + 2.0 * self.padding);
        TextMark {
            at: Point::new(right.map_or(self.at.x, |right| right - width), self.at.y),
            width: Some(width),
            measured: None,
            ..self.clone()
        }
    }

    /// This mark wearing `dressing`: the words, the place and a pulled-out width stay the
    /// mark's own, and everything the bar sets travels. A template that carried its text into
    /// a live field would replace what is being typed with the empty buffer it started from.
    pub fn dressed_as(&self, dressing: &TextMark) -> TextMark {
        TextMark {
            size: dressing.size,
            padding: dressing.padding,
            radius: dressing.radius,
            align: dressing.align,
            background: dressing.background,
            border: dressing.border,
            outline: dressing.outline,
            outline_width: dressing.outline_width,
            measured: None,
            ..self.clone()
        }
    }

    /// The same mark with its frame's corner moved to `map`'s answer.
    pub(super) fn translated(&self, map: impl Fn(Point) -> Point) -> TextMark {
        TextMark {
            at: map(self.at),
            ..self.clone()
        }
    }

    /// The same mark mapped from one box onto another. The font follows the vertical scale,
    /// which is what a line of text follows; the width follows its own axis. Padding, corner
    /// and outline travel with the font so the frame keeps its proportions.
    pub(super) fn rebased(&self, map: impl Fn(Point) -> Point, sx: f32, sy: f32) -> TextMark {
        let sy = sy.abs().max(0.01);
        TextMark {
            at: map(self.at),
            size: (self.size * sy).clamp(MIN_SIZE, MAX_SIZE),
            width: self.width.map(|width| width * sx.abs()),
            padding: (self.padding * sy).clamp(0.0, MAX_PADDING),
            radius: (self.radius * sy).clamp(0.0, MAX_RADIUS),
            outline_width: (self.outline_width * sy).clamp(0.0, MAX_OUTLINE),
            content: Size::new(
                self.content.w * sx.abs().max(0.01),
                self.content.h * sy,
            ),
            measured: None,
            ..self.clone()
        }
    }

    /// Draw the frame and its text into `list`, in image pixels.
    ///
    /// `composing` is the range an input method's bytes cover in this mark's text — the mark
    /// already carries them — and the engine underlines it.
    pub fn paint(
        &self,
        list: &mut DrawList,
        owner: NodeId,
        style: &Style,
        composing: Option<std::ops::Range<usize>>,
    ) {
        let frame = self.frame();
        if self.background.is_some() || self.border.is_some() {
            list.push_rect(
                owner,
                RectPrim {
                    rect: frame,
                    radius: Corners::all(self.corner()),
                    fill: self.background.unwrap_or(Color::TRANSPARENT),
                    gradient: None,
                    border: self.border.map(|color| (color, BORDER)),
                    border_gradient: None,
                    notch: None,
                },
            );
        }
        list.push_text(
            owner,
            TextPrim {
                block: Arc::new(self.block()),
                origin: Point::new(
                    frame.origin.x + self.padding,
                    frame.origin.y + self.padding,
                ),
                max_width: self.wrap(),
                layout: None,
                height: 0.0,
                ink: style.ink,
                caret: None,
                selection: None,
                composing,
                shape_scale: 1.0,
                zoom: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame is one derived box: the text the picture paints, plus its padding, with an
    /// explicit width that may only widen it. The field, the handles and the picture all read
    /// this one function, so a frame that could hug narrower than its text would put every
    /// one of them a different place.
    #[test]
    fn the_frame_hugs_its_text_and_an_explicit_width_only_widens_it() {
        let mut mark = TextMark {
            content: Size::new(40.0, 20.0),
            ..TextMark::new(Point::ZERO)
        };
        let hug = 40.0 + 2.0 * DEFAULT_PADDING;
        assert_eq!(mark.frame().size.w, hug);
        assert_eq!(mark.frame().size.h, 20.0 + 2.0 * DEFAULT_PADDING);
        assert_eq!(
            mark.wrap(),
            f32::INFINITY,
            "a hugged frame offers its lines no break of its own"
        );

        mark.width = Some(10.0);
        assert_eq!(
            mark.frame().size.w,
            hug,
            "a width narrower than the text cannot cut it"
        );
        assert!(
            mark.wrap() >= mark.content.w,
            "and the lines are still offered the width the measurement found"
        );

        mark.width = Some(200.0);
        assert_eq!(mark.frame().size.w, 200.0);
        assert_eq!(mark.measure_width(), 200.0 - 2.0 * DEFAULT_PADDING);
        assert_eq!(mark.wrap(), mark.measure_width());

        // A run wider than the frame is broken at its own width rather than spilling past
        // the box the measurement sized.
        mark.content.w = 300.0;
        assert_eq!(mark.wrap(), 300.0);

        // Rounding is the bar's to offer: a radius past the padding is drawn, and only half
        // the short side bounds it, where the two arcs would meet.
        mark.padding = 2.0;
        mark.radius = 10.0;
        assert_eq!(mark.frame().size.h, 24.0);
        assert_eq!(mark.corner(), 10.0);
        mark.radius = MAX_RADIUS;
        assert_eq!(mark.corner(), 12.0);
    }
}
