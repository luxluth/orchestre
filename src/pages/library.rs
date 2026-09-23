use std::sync::Arc;

use arc_swap::ArcSwap;
use mtk::{
    AlignItems, Edges, FlexDirection, JustifyContent, Lens, Motion, ObjectFit, Overflow,
    ScrollOffset, ScrollbarStyle, Size, Style, SvgData, TextSpan, TextStyle, TransitionProperty,
    VerticalAlignment,
    animation::Curve,
    clr, presence, switch,
    text_property::{Alignment, FontWeight},
    ui::{
        EventKind, View, ViewEventExt, ViewStyleExt,
        widgets::{
            SpanGeometry, async_image, column, container, rich_text, row, svg, text,
            virtual_list_count,
        },
    },
};

use super::ArtistLink;

use crate::{
    fonts::Font::{InterVariable, Iosevka},
    icons::{
        A_LARGE_SMALL, CALENDAR, DISC_ALBUM, LIST_SORT_ASCENDING, LIST_SORT_DESCENDING, MIC_VOCAL,
        PLAY,
    },
    orchestra::{
        Orchestra,
        track::{Id, Song},
    },
    pages::{Theme, TimeFormat},
    svg_display,
};

#[derive(Lens, Clone, Debug, Default)]
pub struct LibraryState {
    pub hovered_song: Option<Id>,
    pub active_filter: Filter,
    pub list_run_offset: f32,
    pub sorted_songs: Arc<Vec<Song>>,
}

#[derive(Clone, Debug)]
pub enum LibraryMsg {
    HoverSong(Id),
    SetFilterTag(FilterTag),
    SetFilterOrder(Order),
    ClickArtist(Id, SpanGeometry),
    SetSortMetric(SortMetric),
    ClickAlbum(Id),
    SetListRunOffset(f32),
    Play(Id),
}

pub fn song_pill(
    song: &Song,
    hsid: Option<Id>,
    orchestra: &Option<Arc<ArcSwap<Orchestra>>>,
    theme: Theme,
    index: usize,
) -> impl View<LibraryState, Message = LibraryMsg> + use<> {
    let id = song.id;
    let mut is_hovered: bool = false;
    if let Some(hsid) = hsid {
        is_hovered = hsid == id;
    }

    let (artist_names, artistlinks_spans) = get_artist_links(song, orchestra, theme);

    let leading = container((switch! {
        if is_hovered => svg(SvgData::from_str(PLAY).unwrap())
            .color(theme.fg())
            .fill(theme.fg())
            .stroke_width(0.)
            .fit(ObjectFit::Contain)
            .style(Style::new().width(Size::Fixed(18)).height(Size::Fixed(18)).on_active(|s| s.scale(0.96)))
            .on_event(EventKind::Click, move |_| Some(LibraryMsg::Play(id))),
        else => text(&format!("{}", index + 1)).style(
            Style::new().set_text_style(
                TextStyle::new()
                    .font_size(14.)
                    .color(theme.fg().with_alpha(180))
                    .font_weight(FontWeight::BOLD)
                    .font_family(Iosevka.name()),
            ),
        ),
    },))
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
                    .color(theme.fg())
                    .font_family(InterVariable.name()),
            ),
        ),
        rich_text(&artist_names)
            .spans(artistlinks_spans)
            .text_style(
                TextStyle::new()
                    .font_size(14.)
                    .color(theme.fg().with_alpha(180))
                    .italic()
                    .font_family(InterVariable.name()),
            )
            .on_span_click(|token, geom| match token {
                ArtistLink::Separator => None,
                ArtistLink::Link(id) => Some(LibraryMsg::ClickArtist(id, geom)),
            }),
        text(&song.duration.format_into_2_digit_seconds_multiple_part()).style(
            Style::new()
                .set_text_style(
                    TextStyle::new()
                        .font_size(14.)
                        .color(theme.fg().with_alpha(180))
                        .alignment(Alignment::End)
                        .italic()
                        .font_family(Iosevka.name()),
                )
                .flex_grow(1.),
        ),
    ))
    .style(
        Style::new()
            .border(1.0, theme.teal_gray())
            .corner_radius(4.0)
            .overflow(Overflow::Hidden)
            .width(Size::Percent(1.0))
            .align_items(AlignItems::Center)
            .gap(14.0)
            .padding(7.0)
            .on_hover(|s| s.border(1.0, theme.teal_gray_accent())),
    )
    .on_event(EventKind::HoverIn, move |_| Some(LibraryMsg::HoverSong(id))),))
    .style(
        Style::new()
            .width(Size::Percent(1.0))
            .padding_edges(Edges::all(0.).right(10.)),
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FilterTag {
    #[default]
    Songs,
    Albums,
    Artists,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Desc,
    Asc,
}

impl Order {
    pub fn name(&self) -> String {
        match self {
            Order::Asc => "Ascending Order".to_string(),
            Order::Desc => "Descending Order".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortMetric {
    #[default]
    ByDate,
    ByTitle,
}

impl SortMetric {
    pub const ALL: [SortMetric; 2] = [SortMetric::ByDate, SortMetric::ByTitle];

    pub fn name(&self) -> String {
        match self {
            SortMetric::ByDate => "Date".to_string(),
            SortMetric::ByTitle => "Title".to_string(),
        }
    }

    pub fn svg(&self) -> SvgData {
        match self {
            SortMetric::ByDate => SvgData::from_str(CALENDAR).unwrap(),
            SortMetric::ByTitle => SvgData::from_str(A_LARGE_SMALL).unwrap(),
        }
    }

    pub fn cycle(&self) -> SortMetric {
        let current_idx = Self::ALL.iter().position(|m| m == self).unwrap_or(0);
        Self::ALL[(current_idx + 1) % Self::ALL.len()]
    }
}

impl Order {
    pub fn flip(&self) -> Order {
        match self {
            Order::Asc => Order::Desc,
            _ => Order::Asc,
        }
    }

    pub fn svg(&self) -> SvgData {
        match self {
            Order::Desc => SvgData::from_str(LIST_SORT_DESCENDING).unwrap(),
            Order::Asc => SvgData::from_str(LIST_SORT_ASCENDING).unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Filter {
    pub tag: FilterTag,
    pub order: Order,
    pub metric: SortMetric,
}

impl FilterTag {
    pub fn name(&self) -> String {
        match self {
            FilterTag::Songs => "Songs".to_string(),
            FilterTag::Albums => "Albums".to_string(),
            FilterTag::Artists => "Artists".to_string(),
        }
    }
}

fn get_artist_links(
    song: &Song,
    orchestra: &Option<Arc<ArcSwap<Orchestra>>>,
    theme: Theme,
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
            .color(theme.fg().with_alpha(180))
            .hover_underline()
            .id(ArtistLink::Link(artist.id));
        artistlinks_spans.push(span);
        artist_names.push_str(&artist.name);
        if i < artists.len() - 1 {
            let span = TextSpan::new(artist_names.len()..(artist_names.len() + 2))
                .color(theme.fg().with_alpha(180))
                .id(ArtistLink::Separator);
            artistlinks_spans.push(span);
            artist_names.push_str(", ");
        }
    }

    (artist_names, artistlinks_spans)
}

pub fn page_filter(
    state: &LibraryState,
    theme: Theme,
) -> impl View<LibraryState, Message = LibraryMsg> + use<> {
    let filters: Vec<_> = vec![FilterTag::Songs, FilterTag::Albums, FilterTag::Artists]
        .iter()
        .map(|f| {
            let fi = *f;
            text(&f.name().to_uppercase())
                .style(
                    Style::new()
                        .padding_xy(18., 4.5)
                        .corner_radius(40.)
                        .bg_color(if state.active_filter.tag == *f {
                            clr!(ll_blue)
                        } else {
                            clr!(ll_blue).with_alpha(38)
                        })
                        .set_text_style(
                            TextStyle::new()
                                .font_size(16.)
                                .font_weight(FontWeight::BOLD)
                                .font_family(InterVariable.name())
                                .color(if state.active_filter.tag == *f {
                                    clr!(white)
                                } else {
                                    theme.fg()
                                }),
                        )
                        .on_active(|s| s.scale(0.96))
                        .transition_all(100., Curve::ease_in_out()),
                )
                .on_event(EventKind::Click, move |_: &LibraryState| {
                    Some(LibraryMsg::SetFilterTag(fi))
                })
        })
        .collect();

    row((
        container(filters).style(Style::new().gap(13.).flex_direction(FlexDirection::Row)),
        row((
            row((
                container((svg_display!(
                    state.active_filter.metric.svg(),
                    theme.fg(),
                    18,
                    3.
                ),)),
                text(state.active_filter.metric.name()).style(
                    Style::new().set_text_style(
                        TextStyle::new()
                            .font_size(16.)
                            .font_family(InterVariable.name())
                            .font_weight(FontWeight::BOLD)
                            .color(theme.fg()),
                    ),
                ),
            ))
            .style(
                Style::new()
                    .padding_xy(18., 4.5)
                    .gap(4.)
                    .corner_radius(40.)
                    .bg_color(clr!(ll_blue).with_alpha(38))
                    .on_active(|s| s.scale(0.96))
                    .transition_all(100., Curve::ease_in_out()),
            )
            .on_event(EventKind::Click, |e: &LibraryState| {
                Some(LibraryMsg::SetSortMetric(e.active_filter.metric.cycle()))
            }),
            container((
                svg_display!(state.active_filter.order.svg(), theme.fg(), 18, 4.)
                    .tooltip(&state.active_filter.order.name())
                    // .delay_ms(250)
                    .override_style(
                        Style::new()
                            .bg_color(theme.bg())
                            .border(1., theme.border())
                            .corner_radius(4.)
                            .padding_xy(8., 4.)
                            .set_text_style(
                                TextStyle::new()
                                    .color(theme.fg())
                                    .font_family(InterVariable.name())
                                    .font_size(12.)
                                    .vertical_alignment(VerticalAlignment::Center),
                            ),
                    ),
            ))
            .style(
                Style::new()
                    .padding(4.)
                    .corner_radius(50.)
                    .bg_color(clr!(ll_blue).with_alpha(38))
                    .on_active(|s| s.scale(0.96))
                    .transition_all(100., Curve::ease_in_out()),
            )
            .on_event(EventKind::Click, |e: &LibraryState| {
                Some(LibraryMsg::SetFilterOrder(e.active_filter.order.flip()))
            }),
        ))
        .style(Style::new().gap(6.)),
    ))
    .style(
        Style::new()
            .justify_content(JustifyContent::SpaceBetween)
            .width(Size::Fill),
    )
}

fn hovered_song_card(
    state: &LibraryState,
    orchestra: &Option<Arc<ArcSwap<Orchestra>>>,
    theme: Theme,
) -> impl View<LibraryState, Message = LibraryMsg> + use<> {
    let orch = orchestra.as_ref().unwrap();
    let guard = orch.load();

    column((
        state.hovered_song.as_ref().and_then(|id| {
            let song = guard.get_song(id).unwrap();
            let album_id = song.album?;
            let album = guard.get_album(&album_id)?;
            let cover_id = album.cover.as_ref()?;
            let cover = guard.get_cover(cover_id)?;

            let main_color = cover
                .swatches
                .first()
                .map(|e| e.to_color())
                .unwrap_or(theme.fg());

            Some(
                async_image(cover.get_path())
                    .fit(ObjectFit::Cover)
                    .style(
                        Style::new()
                            .width(Size::Fill)
                            .aspect_ratio(1.0)
                            .bg_color(main_color)
                            .corner_radius(8.)
                            .border(0., main_color)
                            .on_hover(|s| s.border(5., main_color))
                            .on_active(|s| s.scale(0.96))
                            .transition(TransitionProperty::BorderColor, 200., Curve::ease_in_out())
                            .transition(TransitionProperty::Border, 200., Curve::ease_in_out())
                            .transition(TransitionProperty::Scale, 100., Curve::ease_in_out()),
                    )
                    .on_event(EventKind::Click, move |_| {
                        Some(LibraryMsg::ClickAlbum(album_id))
                    }),
            )
        }),
        state.hovered_song.is_none().then_some(
            Some(text("Hover above an interactable element on the left")).style(
                Style::new().width(Size::Fill).set_text_style(
                    TextStyle::new()
                        .italic()
                        .font_family(InterVariable.name())
                        .color(theme.fg().with_alpha(133))
                        .wrap(true),
                ),
            ),
        ),
        state.hovered_song.as_ref().and_then(|id| {
            let song = guard.get_song(id).unwrap();
            let album_id = song.album?;
            let album = guard.get_album(&album_id)?;

            let (artist_names, artistlinks_spans) = get_artist_links(song, orchestra, theme);

            // let track_fmt = format!("Track N°{}", song.track);

            Some(
                column((
                    row((
                        svg_display!(SvgData::from_str(DISC_ALBUM).unwrap(), theme.fg(), 18, 3.),
                        text(&album.name).style(
                            Style::new().flex_grow(1.).width(Size::Fill).set_text_style(
                                TextStyle::new()
                                    .font_size(14.)
                                    .font_family(InterVariable.name())
                                    .font_weight(FontWeight::BOLD)
                                    .wrap(true)
                                    .color(theme.fg()),
                            ),
                        ),
                    ))
                    .style(
                        Style::new()
                            .gap(10.)
                            .align_items(AlignItems::Center)
                            .height(Size::Fit)
                            .width(Size::Fill),
                    ),
                    row((
                        svg_display!(SvgData::from_str(MIC_VOCAL).unwrap(), theme.fg(), 18, 3.),
                        rich_text(&artist_names)
                            .spans(artistlinks_spans)
                            .text_style(
                                TextStyle::new()
                                    .font_size(14.)
                                    .wrap(true)
                                    .color(theme.fg().with_alpha(180))
                                    .italic()
                                    .font_family(InterVariable.name()),
                            )
                            .on_span_click(|token, geom| match token {
                                ArtistLink::Separator => None,
                                ArtistLink::Link(id) => Some(LibraryMsg::ClickArtist(id, geom)),
                            })
                            .style(Style::new().flex_grow(1.).width(Size::Fill)),
                    ))
                    .style(
                        Style::new()
                            .gap(10.)
                            .align_items(AlignItems::Center)
                            .height(Size::Fit)
                            .width(Size::Fill),
                    ),
                    // row((text(&track_fmt)
                    //     .style(
                    //         Style::new().set_text_style(
                    //             TextStyle::new()
                    //                 .align(Alignment::Right)
                    //                 .font_size(14.)
                    //                 .wrap(true)
                    //                 .color(theme.fg().with_alpha(180))
                    //                 .italic()
                    //                 .font_family("Inter Variable"),
                    //         ),
                    //     )
                    //     .style(Style::new().flex_grow(1.).width(Size::Fill)),))
                    // .style(
                    //     Style::new()
                    //         .gap(10.)
                    //         .align_items(AlignItems::Center)
                    //         .height(Size::Fit)
                    //         .width(Size::Fill),
                    // ),
                ))
                .style(
                    Style::new()
                        .bg_color(clr!(ll_blue).with_alpha(38))
                        .corner_radius(8.)
                        .height(Size::Fit)
                        .width(Size::Fill)
                        .padding(10.),
                ),
            )
        }),
    ))
    .style(
        Style::new()
            .width(Size::Fill)
            .padding(10.)
            .gap(10.)
            .corner_radius(12.)
            .border(2., theme.fg().with_alpha(38)),
    )
}

pub fn songs_list(
    state: &LibraryState,
    orchestra: &Option<Arc<ArcSwap<Orchestra>>>,
    songs: Arc<Vec<Song>>,
    theme: Theme,
) -> impl View<LibraryState, Message = LibraryMsg> + use<> {
    const ITEM_HEIGHT: f32 = 45.0;

    let orch_clone = orchestra.clone();
    let hsid = state.hovered_song;

    let scrollbar_style = ScrollbarStyle {
        thumb_color: clr!(ll_blue),
        track_color: Some(theme.fg().with_alpha(38)),
        ..Default::default()
    };

    virtual_list_count(state.sorted_songs.len(), ITEM_HEIGHT, move |i| {
        container((song_pill(&songs[i], hsid, &orch_clone, theme, i),)).style(
            Style::new()
                .width(Size::Percent(1.0))
                .height(Size::Fixed(ITEM_HEIGHT as u32))
                .min_height(ITEM_HEIGHT)
                .max_height(ITEM_HEIGHT)
                .flex_shrink(0.0)
                .padding_edges(Edges::tb(4.5)),
        )
    })
    .buffer(5)
    .scrollbar(scrollbar_style)
    .style(
        Style::new()
            .width(Size::Percent(1.0))
            .height(Size::Fill)
            .flex_grow(1.),
    )
    .scroll_offset(ScrollOffset::Percent(state.list_run_offset))
    .on_scroll(|_, s| Some(LibraryMsg::SetListRunOffset(s.scroll_pct())))
}

pub fn render(
    state: &LibraryState,
    orchestra: Option<Arc<ArcSwap<Orchestra>>>,
    theme: Theme,
) -> impl View<LibraryState, Message = LibraryMsg> + use<> {
    let orch = orchestra.as_ref().unwrap();
    let guard = orch.load();

    let song_count = guard.collection.songs.len();
    let songs = state.sorted_songs.clone();

    column((
        column((
            text("Library").style(Style::new().apply(theme.heading(None))),
            text(if song_count > 1 {
                format!("{} songs", song_count)
            } else {
                format!("{} song", song_count)
            })
            .style(Style::new().apply(theme.subtitle(None))),
        )),
        row((
            column((
                page_filter(state, theme),
                switch! {
                    match state.active_filter.tag {
                        FilterTag::Songs => songs_list(state, &orchestra, songs, theme),
                        _ => text("")
                    }

                },
            ))
            .style(
                Style::new()
                    .height(Size::Percent(1.0))
                    .width(Size::Percent(0.6))
                    .padding_edges(Edges::all(0.).bottom(20.))
                    .gap(20.),
            ),
            presence(
                state.active_filter.tag == FilterTag::Songs,
                hovered_song_card(state, &orchestra, theme),
            )
            .enter(Motion::fade_in())
            .exit(Motion::fade_out())
            .style(Style::new().width(Size::Percent(0.4)).height(Size::Fill)),
        ))
        .style(
            Style::new()
                .width(Size::Percent(1.0))
                .height(Size::Fill)
                .gap(30.),
        ),
    ))
    .style(
        Style::new()
            .width(Size::Fill)
            .height(Size::Fill)
            .gap(28.)
            .padding_edges(Edges::lr(30.).top(20.).right(60.))
            .bg_color(theme.bg()),
    )
}
