use std::path::PathBuf;

use mtk::{
    AlignItems, AlignSelf, Edges, JustifyContent, Lens, ObjectFit, PositionStrategy, Size, Style,
    SvgData, TextStyle,
    animation::Curve,
    clr,
    text_property::FontWeight,
    ui::{
        EventKind, View, ViewEventExt, ViewStyleExt,
        widgets::{column, progress_bar, row, svg, text},
    },
};

use crate::{
    fonts::Font::{InterVariable, Iosevka},
    icons::{CHECK, FOLDER},
    pages::Theme,
};

const BANNER: &'static str = include_str!("../assets/banner.svg");

#[derive(Lens, Clone, Debug)]
pub struct LandingState {
    pub music_dir: String,
    pub is_indexing: bool,
    pub log: Option<String>,
    pub error: Option<String>,
}

impl Default for LandingState {
    fn default() -> Self {
        let default_dir = dirs::audio_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Music"))
            .to_string_lossy()
            .to_string();

        Self {
            music_dir: default_dir,
            is_indexing: false,
            log: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum LandingMsg {
    StartIndexing,
    PickFolder,
    FolderPicked(PathBuf),
}

pub fn music_dir_component(
    music_dir: &str,
    theme: Theme,
) -> impl View<LandingState, Message = LandingMsg> + use<> {
    row((
        svg(SvgData::from_str_with_color(FOLDER, theme.fg().with_alpha(150)).unwrap())
            .fit(ObjectFit::Contain)
            .style(Style::new().width(Size::Fixed(24)).height(Size::Fixed(24))),
        column((
            text("Click to choose the folder to index").style(
                Style::new().set_text_style(
                    TextStyle::new()
                        .font_family(InterVariable.name())
                        .color(theme.fg())
                        .font_weight(FontWeight::BOLD),
                ),
            ),
            text(music_dir).style(Style::new().set_text_style(TextStyle {
                color: theme.fg().with_alpha(125),
                font_family: Iosevka.name(),
                font_size: 13.0,
                vertical_alignment: mtk::style::VerticalAlignment::Center,
                ..Default::default()
            })),
        )),
        row((
            svg(SvgData::from_str_with_color(CHECK, theme.bg()).unwrap())
                .fit(ObjectFit::Contain)
                .style(Style::new().width(Size::Fixed(18)).height(Size::Fixed(18))),
        ))
        .on_event(EventKind::Click, |_| Some(LandingMsg::StartIndexing))
        .style(
            Style::new()
                .height(Size::Fill)
                .width(Size::Fixed(36))
                .update_constraints(|c| c.aspect_ratio = 1.0)
                .corner_radius(4.)
                .justify_content(JustifyContent::Center)
                .align_items(AlignItems::Center)
                .bg_color(theme.fg())
                .padding(Edges::all(10.))
                .on_active(|s| s.scale(0.97))
                .transition_all(140.0, Curve::ease_out()),
        ),
    ))
    .on_event(EventKind::Click, |_| Some(LandingMsg::PickFolder))
    .style(
        Style::new()
            .align_items(AlignItems::Center)
            .gap(10.0)
            .padding(Edges::symmetric(8.0, 10.0))
            .bg_color(theme.border())
            .border(1.0, theme.fg().with_alpha(80))
            .corner_radius(12.0)
            .on_hover(|s| {
                s.bg_color(theme.border_accent())
                    .border(1.0, theme.fg().with_alpha(90))
            })
            .on_active(|s| s.scale(0.97))
            .transition_all(140.0, Curve::ease_out()),
    )
}

pub fn render(
    state: &LandingState,
    theme: Theme,
) -> impl View<LandingState, Message = LandingMsg> + use<> {
    column((
        svg(SvgData::from_str(BANNER).unwrap())
            .color(clr!(white))
            .fit(ObjectFit::Contain)
            .style(
                Style::new()
                    .width(Size::Percent(0.70))
                    .height(Size::Fixed(120))
                    .align_self(AlignSelf::Center),
            ),
        (!state.is_indexing).then_some(music_dir_component(&state.music_dir, theme)),
        state.is_indexing.then_some(
            column((progress_bar(0.0)
                .indeterminate(true)
                .fill_color(clr!(white))
                .track_color(clr!(ll_blue)),))
            .style(
                Style::new()
                    .corner_radius(20.)
                    .width(Size::Fixed(200))
                    .height(Size::Fixed(10)),
            ),
        ),
        state.error.as_ref().and_then(|e| {
            Some(
                text(e).style(
                    Style::new().set_text_style(
                        TextStyle::new()
                            .family(InterVariable.name())
                            .color(theme.error())
                            .weight(FontWeight::BOLD),
                    ),
                ),
            )
        }),
        text(if state.log.is_some() {
            state.log.as_ref().unwrap()
        } else {
            ""
        })
        .style(
            Style::new()
                .set_text_style(
                    TextStyle::new()
                        .color(clr!(white))
                        .font_weight(FontWeight::BOLD)
                        .font_size(14.)
                        .font_family(Iosevka.name()),
                )
                .position(PositionStrategy::absolute().bottom(10.).left(10.)),
        ),
    ))
    .style(
        Style::new()
            .width(Size::Fill)
            .height(Size::Fill)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center)
            .gap(24.0)
            .bg_color(clr!(ll_blue)),
    )
}
