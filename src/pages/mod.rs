use std::time::Duration;

use mtk::{Color, Style, clr, hsl, text_property::FontWeight};

use crate::fonts::Font::InterVariable;

pub mod album;
pub mod landing;
pub mod library;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

macro_rules! colored {
    ($name:ident, $dark:expr, $light:expr) => {
        fn $name(&self) -> Color {
            match self {
                Theme::Dark => $dark,
                Theme::Light => $light,
            }
        }
    };
}

impl Theme {
    colored!(bg, clr!(0x181818FF), clr!(white));
    colored!(fg, clr!(white), clr!(black));
    colored!(border, clr!(0x222222FF), clr!(0xe8e8e8FF));
    colored!(border_accent, clr!(0x2a2a2aFF), clr!(0xf0f0f0FF));
    colored!(
        error,
        hsl!(0, 58.6 / 100.0, 49.2 / 100.0),
        hsl!(360, 79.0 / 100.0, 49.2 / 100.0)
    );
    colored!(teal_gray, clr!(0x2f2f2fff), clr!(0xddddddff));
    colored!(teal_gray_accent, clr!(0x222222ff), clr!(0x9a9a9aff));

    pub fn heading(&self) -> impl Fn(Style) -> Style + '_ {
        return move |s| {
            s.update_text_style(|t| {
                t.font_size = 48.0;
                t.color = clr!(ll_blue);
                t.font_family = InterVariable.name();
                t.font_weight = FontWeight::BOLD;
            })
        };
    }

    pub fn subtitle(&self) -> impl Fn(Style) -> Style + '_ {
        return move |s| {
            s.update_text_style(|t| {
                t.font_size = 14.0;
                t.color = self.fg();
                t.font_family = InterVariable.name();
            })
            .opacity(0.7)
        };
    }
}

pub trait TimeFormat {
    fn format_into_2_digit_seconds_multiple_part(&self) -> String;
}

impl TimeFormat for Duration {
    fn format_into_2_digit_seconds_multiple_part(&self) -> String {
        let total_secs = self.as_secs();
        let hours = total_secs / 3600;
        let mins = (total_secs / 60) % 60;
        let secs = total_secs % 60;

        if hours > 0 {
            format!("{hours}:{mins:02}:{secs:02}")
        } else {
            format!("{mins}:{secs:02}")
        }
    }
}
