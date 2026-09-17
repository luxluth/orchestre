use std::sync::Arc;

use arc_swap::ArcSwap;
use mtk::{
    BoxShadow, Edges, JustifyContent, Lens, ObjectFit, ScrollAxis, ScrollbarStyle, Size, Style,
    SvgData, TransitionProperty, ViewStyleExt,
    animation::Curve,
    rgba,
    ui::{
        EventKind, View, ViewEventExt,
        widgets::{async_image, column, container, row, scroll_view, svg, text},
    },
};

use crate::{
    icons,
    orchestra::{Orchestra, track::Id},
    pages::Theme,
    svg_display,
};

#[derive(Clone, Debug)]
pub enum AlbumMsg {
    GotoLibrary,
}

#[derive(Lens, Clone, Debug, Default)]
pub struct AlbumState {
    pub album_id: Id,
}

pub fn render(
    state: &AlbumState,
    orchestra: Option<Arc<ArcSwap<Orchestra>>>,
    theme: Theme,
) -> impl View<AlbumState, Message = AlbumMsg> + use<> {
    let orch = orchestra.as_ref().unwrap();
    let guard = orch.load();

    let album = guard.get_album(&state.album_id).unwrap();
    let cover = guard.get_cover(&album.cover.unwrap_or_default()).unwrap();
    let (main_bg, main_fg) = cover
        .swatches
        .first()
        .map(|e| {
            (
                e.to_color(),
                e.to_color().get_tinted_contrast_text(4.5, 0.12),
            )
        })
        .unwrap_or((theme.bg(), theme.fg()));

    let scrollbar_style = ScrollbarStyle {
        thumb_color: main_fg,
        track_color: Some(main_bg.with_alpha(120)),
        ..Default::default()
    };

    column((
        row((
            column((
                text(&album.name).style(
                    Style::new()
                        .width(Size::Fill)
                        .apply(theme.heading())
                        .update_text_style(|t| {
                            t.color = main_fg;
                            t.wrap = true;
                        }),
                ),
                text("ALBUM").style(
                    Style::new()
                        .apply(theme.subtitle())
                        .update_text_style(|t| t.color = main_fg),
                ),
            ))
            .style(Style::new().width(Size::Fill).height(Size::Fit)),
            svg_display!(
                SvgData::from_str(icons::CHEVRON_DOWN).unwrap(),
                main_fg,
                38,
                2.
            )
            .style(
                Style::new()
                    .opacity(0.7)
                    .on_hover(|s| s.opacity(1.))
                    .on_active(|s| s.scale(0.9))
                    .transition(TransitionProperty::Opacity, 150., Curve::ease_in_out()),
            )
            .on_event(EventKind::Click, |_| Some(AlbumMsg::GotoLibrary)),
        ))
        .style(
            Style::new()
                .width(Size::Fill)
                .gap(10.)
                .padding_edges(Edges::lr(30.).top(20.))
                // .align_items(AlignItems::Center)
                .justify_content(JustifyContent::SpaceBetween),
        ),
        scroll_view(
            container((async_image(cover.get_path()).fit(ObjectFit::Cover).style(
                Style::new()
                    .width(Size::Fixed(300))
                    .aspect_ratio(1.0)
                    .border(1., main_fg.with_alpha(20))
                    .box_shadow(BoxShadow::new(rgba!(0, 0, 0, 45)).offset(0., 2.).blur(6.))
                    .add_box_shadow(
                        BoxShadow::new(rgba!(0, 0, 0, 35))
                            .offset(0., 14.)
                            .blur(28.)
                            .spread(-4.),
                    )
                    .box_shadow(BoxShadow::sm())
                    .corner_radius(8.),
            ),))
            .style(Style::new().padding_xy(30., 10.)),
        )
        .axis(ScrollAxis::Vertical)
        .scrollbar(scrollbar_style)
        .style(Style::new().height(Size::Fill).width(Size::Fill)),
    ))
    .style(
        Style::new()
            .bg_color(main_bg)
            .width(Size::Fill)
            .height(Size::Fill)
            .gap(28.),
    )
    .on_global_key_down(|_, k| {
        if k.is_escape() {
            Some(AlbumMsg::GotoLibrary)
        } else {
            None
        }
    })
}
