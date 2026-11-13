use std::cell::RefCell;
use std::collections::HashMap;

use swash::FontRef;

use crate::graphics::{Area, BezierPath, Color, Outline, Renderer, Transform, font};

use super::Command;

thread_local! {
    // each glyph's outline, read from the font once; swash gives every font its own key
    static OUTLINES: RefCell<HashMap<(u64, u16), Option<BezierPath>>> =
        RefCell::new(HashMap::new());
}

impl Renderer {
    /*
     * letters are drawn as small cached pictures, which the gpu handles far
     * faster than outlines; only rotated or stretched text needs the outlines
     */
    pub fn text(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        size: f32,
        color: Color,
        area: Area,
    ) {
        match self.transform.even_scale() {
            Some(scale) => self.text_pictures(content, font, size * scale, color, area),
            None => self.text_outlines(content, font, size, color, area),
        }
    }

    /*
     * the line starts on a whole pixel and every letter keeps its rounded
     * distance from that start, so a moving line never shimmers apart
     */
    fn text_pictures(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        pixel_size: f32,
        color: Color,
        area: Area,
    ) {
        let ascent = font.metrics(font::coords(font)).scale(pixel_size).ascent;

        let start = self.transform.map(area.x, area.y);

        let line_x = start.0.round();
        let baseline = (start.1 + ascent).round();

        for glyph in font::shape(content, font, pixel_size) {
            let letter_x = (line_x + glyph.x).round();
            let letter_y = baseline - glyph.y;

            self.commands.push(Command::Glyph {
                font: glyph.font,
                id: glyph.id,
                size: pixel_size,
                color,
                x: letter_x,
                y: letter_y,
            });
        }
    }

    fn text_outlines(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        size: f32,
        color: Color,
        area: Area,
    ) {
        let ascent = font.metrics(font::coords(font)).scale(size).ascent;

        let baseline = area.y + ascent;

        for glyph in font::shape(content, font, size) {
            let letter_x = area.x + glyph.x;
            let letter_baseline = baseline - glyph.y;

            // a space has no outline but still moves the pen
            let Some(path) = outline(glyph.font, glyph.id) else {
                continue;
            };

            let units_per_em = glyph.font.metrics(font::coords(glyph.font)).units_per_em;
            let face_units = size / f32::from(units_per_em);

            // fonts point y upward, the screen points it downward
            let letter_transform =
                Transform::from_row(face_units, 0.0, 0.0, -face_units, letter_x, letter_baseline);
            let transform = letter_transform.post_concat(self.transform);

            self.commands.push(Command::Fill {
                path,
                transform,
                color,
            });
        }
    }
}

fn outline(font: FontRef<'static>, id: u16) -> Option<BezierPath> {
    let key = (font.key.value(), id);

    OUTLINES.with_borrow_mut(|outlines| {
        let path = outlines
            .entry(key)
            .or_insert_with(|| read_outline(font, id));

        path.clone()
    })
}

fn read_outline(font: FontRef<'static>, id: u16) -> Option<BezierPath> {
    font::with_context(|context| {
        // no size keeps the outline in the font's own units
        let mut scaler = context
            .builder(font)
            .normalized_coords(font::coords(font))
            .build();

        let shape = scaler.scale_outline(id)?;

        Outline::new().read(shape.path()).finish()
    })
}
