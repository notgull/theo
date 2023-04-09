// SPDX-License-Identifier: LGPL-3.0-or-later OR MPL-2.0
// This file is a part of `theo`.
//
// `theo` is free software: you can redistribute it and/or modify it under the terms of
// either:
//
// * GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
// * Mozilla Public License as published by the Mozilla Foundation, version 2.
//
// `theo` is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License or the Mozilla Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License and the Mozilla
// Public License along with `theo`. If not, see <https://www.gnu.org/licenses/>.

use piet_cosmic_text::{
    Text as CosmicText, TextLayout as CosmicTextLayout,
    TextLayoutBuilder as CosmicTextLayoutBuilder,
};
use piet_glow::{
    Text as GlowText, TextLayout as GlowTextLayout, TextLayoutBuilder as GlowTextLayoutBuilder,
};

/// The text backend for the system.
#[derive(Clone)]
pub struct Text(pub(crate) TextInner);

impl Text {
    pub(crate) fn as_inner(&self) -> &piet_cosmic_text::Text {
        match &self.0 {
            TextInner::Cosmic(inner) => inner,
            _ => panic!(),
        }
    }
}

#[derive(Clone)]
pub(crate) enum TextInner {
    Glow(GlowText),
    Cosmic(CosmicText),
}

/// The text layout builder for the system.
pub struct TextLayoutBuilder(pub(crate) TextLayoutBuilderInner);

pub(crate) enum TextLayoutBuilderInner {
    Glow(GlowTextLayoutBuilder),
    Cosmic(CosmicTextLayoutBuilder),
}

/// The text layout for the system.
#[derive(Clone)]
pub struct TextLayout(pub(crate) TextLayoutInner);

#[derive(Clone)]
pub(crate) enum TextLayoutInner {
    Glow(GlowTextLayout),
    Cosmic(CosmicTextLayout),
}

impl piet::Text for Text {
    type TextLayoutBuilder = TextLayoutBuilder;
    type TextLayout = TextLayout;

    fn font_family(&mut self, family_name: &str) -> Option<piet::FontFamily> {
        match &mut self.0 {
            TextInner::Glow(inner) => inner.font_family(family_name),
            TextInner::Cosmic(inner) => inner.font_family(family_name),
        }
    }

    fn load_font(&mut self, data: &[u8]) -> Result<piet::FontFamily, piet::Error> {
        match &mut self.0 {
            TextInner::Glow(inner) => inner.load_font(data),
            TextInner::Cosmic(inner) => inner.load_font(data),
        }
    }

    fn new_text_layout(&mut self, text: impl piet::TextStorage) -> Self::TextLayoutBuilder {
        match &mut self.0 {
            TextInner::Glow(inner) => {
                TextLayoutBuilder(TextLayoutBuilderInner::Glow(inner.new_text_layout(text)))
            }
            TextInner::Cosmic(inner) => {
                TextLayoutBuilder(TextLayoutBuilderInner::Cosmic(inner.new_text_layout(text)))
            }
        }
    }
}

impl piet::TextLayoutBuilder for TextLayoutBuilder {
    type Out = TextLayout;

    fn max_width(self, width: f64) -> Self {
        match self.0 {
            TextLayoutBuilderInner::Glow(inner) => {
                TextLayoutBuilder(TextLayoutBuilderInner::Glow(inner.max_width(width)))
            }
            TextLayoutBuilderInner::Cosmic(inner) => {
                TextLayoutBuilder(TextLayoutBuilderInner::Cosmic(inner.max_width(width)))
            }
        }
    }

    fn alignment(self, alignment: piet::TextAlignment) -> Self {
        match self.0 {
            TextLayoutBuilderInner::Glow(inner) => {
                TextLayoutBuilder(TextLayoutBuilderInner::Glow(inner.alignment(alignment)))
            }
            TextLayoutBuilderInner::Cosmic(inner) => {
                TextLayoutBuilder(TextLayoutBuilderInner::Cosmic(inner.alignment(alignment)))
            }
        }
    }

    fn default_attribute(self, attribute: impl Into<piet::TextAttribute>) -> Self {
        match self.0 {
            TextLayoutBuilderInner::Glow(inner) => TextLayoutBuilder(TextLayoutBuilderInner::Glow(
                inner.default_attribute(attribute),
            )),
            TextLayoutBuilderInner::Cosmic(inner) => TextLayoutBuilder(
                TextLayoutBuilderInner::Cosmic(inner.default_attribute(attribute)),
            ),
        }
    }

    fn range_attribute(
        self,
        range: impl std::ops::RangeBounds<usize>,
        attribute: impl Into<piet::TextAttribute>,
    ) -> Self {
        match self.0 {
            TextLayoutBuilderInner::Glow(inner) => TextLayoutBuilder(TextLayoutBuilderInner::Glow(
                inner.range_attribute(range, attribute),
            )),
            TextLayoutBuilderInner::Cosmic(inner) => TextLayoutBuilder(
                TextLayoutBuilderInner::Cosmic(inner.range_attribute(range, attribute)),
            ),
        }
    }

    fn build(self) -> Result<Self::Out, piet::Error> {
        match self.0 {
            TextLayoutBuilderInner::Glow(inner) => {
                Ok(TextLayout(TextLayoutInner::Glow(inner.build()?)))
            }
            TextLayoutBuilderInner::Cosmic(inner) => {
                Ok(TextLayout(TextLayoutInner::Cosmic(inner.build()?)))
            }
        }
    }
}

impl piet::TextLayout for TextLayout {
    fn size(&self) -> piet::kurbo::Size {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.size(),
            TextLayoutInner::Cosmic(inner) => inner.size(),
        }
    }

    fn trailing_whitespace_width(&self) -> f64 {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.trailing_whitespace_width(),
            TextLayoutInner::Cosmic(inner) => inner.trailing_whitespace_width(),
        }
    }

    fn image_bounds(&self) -> piet::kurbo::Rect {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.image_bounds(),
            TextLayoutInner::Cosmic(inner) => inner.image_bounds(),
        }
    }

    fn text(&self) -> &str {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.text(),
            TextLayoutInner::Cosmic(inner) => inner.text(),
        }
    }

    fn line_text(&self, line_number: usize) -> Option<&str> {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.line_text(line_number),
            TextLayoutInner::Cosmic(inner) => inner.line_text(line_number),
        }
    }

    fn line_metric(&self, line_number: usize) -> Option<piet::LineMetric> {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.line_metric(line_number),
            TextLayoutInner::Cosmic(inner) => inner.line_metric(line_number),
        }
    }

    fn line_count(&self) -> usize {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.line_count(),
            TextLayoutInner::Cosmic(inner) => inner.line_count(),
        }
    }

    fn hit_test_point(&self, point: piet::kurbo::Point) -> piet::HitTestPoint {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.hit_test_point(point),
            TextLayoutInner::Cosmic(inner) => inner.hit_test_point(point),
        }
    }

    fn hit_test_text_position(&self, idx: usize) -> piet::HitTestPosition {
        match &self.0 {
            TextLayoutInner::Glow(inner) => inner.hit_test_text_position(idx),
            TextLayoutInner::Cosmic(inner) => inner.hit_test_text_position(idx),
        }
    }
}
