use std::sync::Arc;

use arc_swap::ArcSwap;
use mtk::{
    AlignItems, BoxShadow, Color, Edges, JustifyContent, Lens, ObjectFit, Overflow, ScrollAxis,
    ScrollbarStyle, Size, Style, SvgData, TextSpan, TextStyle, TransitionProperty, ViewStyleExt,
    animation::Curve,
    rgba,
    text_property::{Alignment, FontWeight},
    ui::{
        EventKind, View, ViewEventExt,
        widgets::{
            Badge, SpanGeometry, async_image, badge, column, container, rich_text, row,
            scroll_view, svg, text,
        },
    },
};

use super::ArtistLink;

use crate::{
    fonts::Font::{InterVariable, Iosevka},
    icons,
    orchestra::{
        Orchestra,
        track::{Album, Id, Song},
    },
    pages::{Theme, TimeFormat},
    svg_display,
};

#[derive(Clone, Debug)]
pub enum AlbumMsg {
    GotoLibrary,
    ClickArtist(Id, SpanGeometry),
    HoverSong(Id),
    HoverSongQuit,
}

#[derive(Lens, Clone, Debug, Default)]
pub struct AlbumState {
    pub album_id: Id,
    pub hovered_song_id: Option<Id>,
}

fn get_artist_links(
    song: &Song,
    orchestra: &Option<Arc<ArcSwap<Orchestra>>>,
    fg: Color,
) -> (String, Vec<TextSpan<ArtistLink>>) {
    let orch = orchestra.as_ref().unwrap();
    let guard = orch.load();

    let artists: Vec<_> = song
        .artists
        .iter()
        .filter_map(|sid| guard.get_artist(&sid))
        .collect();

    let mut artist_names = String::new();

    let mut artistlinks_spans: Vec<TextSpan<ArtistLink>> = Vec::new();

    for (i, artist) in artists.iter().enumerate() {
        let span = TextSpan::new(artist_names.len()..(artist_names.len() + artist.name.len()))
            .color(fg.with_alpha(180))
            .hover_underline()
            .id(ArtistLink::Link(artist.id));
        artistlinks_spans.push(span);
        artist_names.push_str(&artist.name);
        if i < artists.len() - 1 {
            let span = TextSpan::new(artist_names.len()..(artist_names.len() + 2))
                .color(fg.with_alpha(180))
                .id(ArtistLink::Separator);
            artistlinks_spans.push(span);
            artist_names.push_str(", ");
        }
    }

    (artist_names, artistlinks_spans)
}

pub fn song_pill(
    song: &Song,
    hsid: Option<Id>,
    orchestra: &Option<Arc<ArcSwap<Orchestra>>>,
    (main_fg, main_bg): (Color, Color),
    index: usize,
) -> impl View<AlbumState, Message = AlbumMsg> + use<> {
    let id = song.id;
    let mut is_hovered: bool = false;
    if let Some(hsid) = hsid {
        is_hovered = hsid == id;
    }

    let (artist_names, artistlinks_spans) = get_artist_links(song, orchestra, main_fg);

    let leading = container((
        is_hovered.then_some(
            svg(SvgData::from_str(icons::PLAY).unwrap())
                .color(main_fg)
                .fill(main_fg)
                .stroke_width(0.)
                .fit(ObjectFit::Contain)
                .style(Style::new().width(Size::Fixed(18)).height(Size::Fixed(18))),
        ),
        (!is_hovered).then_some(
            text(&format!("{}", index + 1)).style(
                Style::new().set_text_style(
                    TextStyle::new()
                        .font_size(14.)
                        .color(main_fg.with_alpha(180))
                        .font_weight(FontWeight::BOLD)
                        .font_family(Iosevka.name()),
                ),
            ),
        ),
    ))
    .style(
        Style::new()
            .width(Size::Fixed(28))
            .height(Size::Fixed(18))
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center),
    );

    container((row((
        leading,
        text(&song.title).style(
            Style::new().set_text_style(
                TextStyle::new()
                    .font_size(14.)
                    .color(main_fg)
                    .font_family(InterVariable.name()),
            ),
        ),
        rich_text(&artist_names)
            .spans(artistlinks_spans)
            .text_style(
                TextStyle::new()
                    .font_size(14.)
                    .color(main_fg.with_alpha(180))
                    .italic()
                    .font_family(InterVariable.name()),
            )
            .on_span_click(|token, geom| match token {
                ArtistLink::Separator => None,
                ArtistLink::Link(id) => Some(AlbumMsg::ClickArtist(id, geom)),
            }),
        text(&song.duration.format_into_2_digit_seconds_multiple_part()).style(
            Style::new()
                .set_text_style(
                    TextStyle::new()
                        .font_size(14.)
                        .color(main_fg.with_alpha(180))
                        .alignment(Alignment::End)
                        .italic()
                        .font_family(Iosevka.name()),
                )
                .flex_grow(1.),
        ),
    ))
    .style(
        Style::new()
            .border(
                2.0,
                if main_bg.is_dark() {
                    main_bg.lighter(30.)
                } else {
                    main_bg.darker(30.)
                },
            )
            .corner_radius(4.0)
            .overflow(Overflow::Hidden)
            .width(Size::Percent(1.0))
            .align_items(AlignItems::Center)
            .gap(14.0)
            .padding(7.0)
            .on_hover(|s| {
                s.border(
                    2.0,
                    if main_bg.is_dark() {
                        main_bg.lighter(30.)
                    } else {
                        main_bg.darker(30.)
                    }
                    .with_alphaf(0.5),
                )
            }),
    ),))
    .on_event(EventKind::HoverIn, move |_| Some(AlbumMsg::HoverSong(id)))
    .on_event(EventKind::HoverOut, move |_| Some(AlbumMsg::HoverSongQuit))
    .style(
        Style::new()
            .width(Size::Percent(1.0))
            .padding_edges(Edges::all(0.).right(10.)),
    )
}

fn songs_section(
    state: &AlbumState,
    orchestra: Option<Arc<ArcSwap<Orchestra>>>,
    album: &Album,
    theme: Theme,
    (main_fg, main_bg): (Color, Color),
) -> impl View<AlbumState, Message = AlbumMsg> + use<> {
    let orch = orchestra.as_ref().unwrap();
    let guard = orch.load();

    let mut songs: Vec<_> = album
        .songs
        .iter()
        .filter_map(|sid| guard.get_song(sid))
        .collect();

    songs.sort_by_key(|s| s.track);

    let songs: Vec<_> = songs
        .iter()
        .enumerate()
        .filter_map(|(index, song)| {
            Some(song_pill(
                song,
                state.hovered_song_id,
                &orchestra,
                (main_fg, main_bg),
                index,
            ))
        })
        .collect();

    column((
        text("Songs").style(Style::new().apply(theme.h2(Some(main_fg)))),
        column(songs).style(
            Style::new()
                .width(Size::Fill)
                .gap(4.5)
                .height(Size::Fit)
                .padding_edges(Edges::all(0.).bottom(25.)),
        ),
    ))
    .style(
        Style::new()
            .gap(15.)
            .padding_xy(30., 10.)
            .width(Size::Fill)
            .height(Size::Fit),
    )
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

    let artist_id = album.artist.unwrap();
    let artist = guard.get_artist(&artist_id).unwrap();

    let release_date = album
        .date
        .map(|t| {
            if t.year != 0 {
                format!("{}", t.year)
            } else {
                "Unknown Year".to_string()
            }
        })
        .or(Some("Unknown Year".to_string()))
        .unwrap();

    let genres: Vec<Badge<AlbumMsg>> = album
        .genres
        .iter()
        .filter_map(|e| {
            if e.trim().is_empty() {
                None
            } else {
                Some(badge(e).custom(main_fg, main_bg))
            }
        })
        .collect();

    let scrollbar_style = ScrollbarStyle {
        thumb_color: main_fg,
        track_color: Some(main_fg.with_alphaf(0.3)),
        ..Default::default()
    };

    column((
        row((
            column((
                text(&album.name).style(
                    Style::new()
                        .width(Size::Fill)
                        .apply(theme.heading(Some(main_fg))),
                ),
                text("ALBUM").style(Style::new().apply(theme.subtitle(Some(main_fg)))),
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
            column((
                row((
                    async_image(cover.get_path()).fit(ObjectFit::Cover).style(
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
                    ),
                    column((
                        text(&artist.name).style(
                            Style::new()
                                .set_text_style(
                                    TextStyle::new()
                                        .font_size(14.)
                                        .color(main_fg)
                                        .italic()
                                        .font_family(InterVariable.name()),
                                )
                                .on_hover(|s| s.update_text_style(|s| s.underline = true)),
                        ),
                        text(&release_date).style(
                            Style::new().set_text_style(
                                TextStyle::new()
                                    .font_size(14.)
                                    .color(main_fg.with_alpha(150))
                                    .font_family(Iosevka.name()),
                            ),
                        ),
                        (!genres.is_empty())
                            .then_some(row(genres).style(Style::new().gap(5.).wrap())),
                    ))
                    .style(Style::new().gap(5.)),
                ))
                .style(Style::new().gap(15.).padding_xy(30., 10.)),
                songs_section(state, orchestra, album, theme, (main_fg, main_bg)),
            ))
            .style(
                Style::new()
                    .gap(40.)
                    .width(Size::Fill)
                    .height(Size::Fit)
                    .padding_edges(Edges::all(0.).bottom(50.)),
            ),
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
