use std::borrow::Cow;

use mtk::{ui::View, windowing::Window};

pub const IOSEVKA_REGULAR_BYTES: &[u8] = include_bytes!("./assets/fonts/Iosevka-Regular.ttf");
pub const IOSEVKA_ITALIC_BYTES: &[u8] = include_bytes!("./assets/fonts/Iosevka-Italic.ttf");
pub const IOSEVKA_BOLD_BYTES: &[u8] = include_bytes!("./assets/fonts/Iosevka-Bold.ttf");
pub const IOSEVKA_BOLDITALIC_BYTES: &[u8] = include_bytes!("./assets/fonts/Iosevka-BoldItalic.ttf");

pub const INTER_VARIABLE_REGULAR_BYTES: &[u8] = include_bytes!("./assets/fonts/InterVariable.ttf");
pub const INTER_VARIABLE_ITALIC_BYTES: &[u8] =
    include_bytes!("./assets/fonts/InterVariable-Italic.ttf");

pub const NOTOSANS_JP_VARIABLE: &[u8] = include_bytes!("./assets/fonts/NotoSansJP-Variable.ttf");
pub const NOTOSANS_KR_VARIABLE: &[u8] = include_bytes!("./assets/fonts/NotoSansKR-Variable.ttf");
pub const NOTOSANS_SC_VARIABLE: &[u8] = include_bytes!("./assets/fonts/NotoSansSC-Variable.ttf");

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Font {
    Iosevka,
    InterVariable,
    NotoSansCJK,
}

impl Font {
    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Font::Iosevka => "Iosevka".into(),
            Font::InterVariable => format!("Inter Variable, {}", Font::NotoSansCJK.name()).into(),
            Font::NotoSansCJK => "Noto Sans JP, Noto Sans SC, Noto Sans KR".into(),
        }
    }
}

impl Font {
    pub fn load<S: 'static, V: 'static>(&self, window: Window<S, V>) -> Window<S, V>
    where
        V: View<S>,
        V::Message: 'static + Send,
    {
        match self {
            Font::Iosevka => window
                .with_font_bytes(IOSEVKA_REGULAR_BYTES)
                .with_font_bytes(IOSEVKA_BOLD_BYTES)
                .with_font_bytes(IOSEVKA_ITALIC_BYTES)
                .with_font_bytes(IOSEVKA_BOLDITALIC_BYTES),
            Font::InterVariable => window
                .with_font_bytes(INTER_VARIABLE_REGULAR_BYTES)
                .with_font_bytes(INTER_VARIABLE_ITALIC_BYTES),
            Font::NotoSansCJK => window
                .with_font_bytes(NOTOSANS_SC_VARIABLE)
                .with_font_bytes(NOTOSANS_JP_VARIABLE)
                .with_font_bytes(NOTOSANS_KR_VARIABLE),
        }
    }
}
