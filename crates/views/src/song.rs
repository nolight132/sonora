// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, FontWeight, Render, SharedString, Window, div, px};
use i18n::t;
use router::{Destination, Link as _};
use spotify::{Credit, Track};
use state::{Playback, SongDetail};
use ui::{
    ActiveTheme as _, Artwork, Avatar, Button, Fact, InfoCard, Initials, Scrollbar, Text, clock,
    eyebrow, heading,
};

use crate::cells;
use crate::hero::{HeroMetaStrip, HeroPlayButton, PageHero, release_date_label};

pub(crate) struct SongView {
    detail: Entity<SongDetail>,
    playback: Entity<Playback>,
    scrollbar: Entity<Scrollbar>,
}

impl SongView {
    fn language_label(code: &str) -> SharedString {
        match code {
            "ar" => t!("language-ar"),
            "de" => t!("language-de"),
            "en" => t!("language-en"),
            "es" => t!("language-es"),
            "fr" => t!("language-fr"),
            "hi" => t!("language-hi"),
            "it" => t!("language-it"),
            "ja" => t!("language-ja"),
            "ko" => t!("language-ko"),
            "pt" => t!("language-pt"),
            "ru" => t!("language-ru"),
            "tr" => t!("language-tr"),
            "uk" => t!("language-uk"),
            "zh" => t!("language-zh"),
            "zxx" => t!("language-zxx"),
            _ => SharedString::from(code.to_owned()),
        }
    }

    pub(crate) fn new(
        detail: Entity<SongDetail>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&detail, |this, _, cx| {
            this.scrollbar
                .read(cx)
                .scroll()
                .set_offset(gpui::Point::default());
            cx.notify();
        })
        .detach();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        Self {
            detail,
            playback,
            scrollbar: cx.new(|_| Scrollbar::new(gpui::ScrollHandle::new())),
        }
    }

    fn hero(&self, track: &Track, cx: &Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let cover = self
            .detail
            .read(cx)
            .album()
            .and_then(|album| album.album.cover_large.clone())
            .or_else(|| track.cover.clone());
        let album = self.detail.read(cx).album().map(|detail| &detail.album);
        let release = album
            .map(|album| release_date_label(&album.release_date))
            .filter(|release| !release.is_empty());

        let mut meta = HeroMetaStrip::new().item(cells::artist_links(
            "song-artists",
            track.artist_refs.clone(),
            track.artists.clone(),
            theme.muted_foreground,
        ));
        if let Some(release) = release {
            meta = meta.text(release);
        }
        meta = meta.text(clock(track.duration));

        let actions = div()
            .flex()
            .items_center()
            .gap_3()
            .child(HeroPlayButton::new(
                "play-song",
                t!("song-play"),
                vec![track.clone()],
                self.playback.clone(),
            ))
            .when_some(track.album_id.clone(), |this, album_id| {
                this.child(
                    Button::new("open-album")
                        .label(t!("song-view-album"))
                        .outline()
                        .on_click(move |_, _, cx| {
                            router::navigate(Destination::Album(album_id.clone().into()), cx);
                        }),
                )
            });

        PageHero::new("song-hero", track.name.clone())
            .cover(cover)
            .eyebrow(t!("song-eyebrow"))
            .meta(meta)
            .actions(actions)
            .explicit(track.explicit)
            .into_any_element()
    }

    fn overview(&self, track: &Track, cx: &Context<Self>) -> AnyElement {
        let album = self.detail.read(cx).album();
        let album_name = album
            .map(|detail| detail.album.name.clone())
            .unwrap_or_else(|| track.album.clone());
        let release = album
            .map(|detail| detail.album.release_date.clone())
            .unwrap_or_default();
        let release = match release.is_empty() {
            true => t!("common-unknown"),
            false => release_date_label(&release),
        };
        let label = album
            .map(|detail| detail.album.label.clone())
            .filter(|value| !value.is_empty())
            .map(SharedString::from)
            .unwrap_or_else(|| t!("common-not-provided"));
        let number = match (track.disc_number, track.track_number) {
            (disc, number) if disc > 1 => t!("song-disc-track", disc = disc, track = number),
            (_, number) if number > 0 => t!("song-track", track = number),
            _ => t!("common-not-provided"),
        };
        let streams = self
            .detail
            .read(cx)
            .playcount()
            .map(cells::count)
            .unwrap_or_else(|| t!("common-not-available"));
        let facts = [
            (t!("song-album"), SharedString::from(album_name)),
            (t!("song-released"), release),
            (t!("song-streams"), streams),
            (t!("song-position"), number),
            (t!("song-label"), label),
            (
                t!("song-popularity"),
                t!("song-popularity-value", value = track.popularity),
            ),
        ];
        InfoCard::new(t!("song-about"))
            .stretch()
            .child(
                div().flex().flex_col().children(
                    facts.into_iter().enumerate().map(|(index, (name, value))| {
                        Fact::new(name, value).striped(index % 2 == 1)
                    }),
                ),
            )
            .into_any_element()
    }

    fn credits(&self, track: &Track, cx: &Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let rows: Vec<_> = if track.credits.is_empty() {
            track
                .artist_refs
                .iter()
                .map(|artist| Credit {
                    name: artist.name.clone(),
                    role: t!("song-performed-by").to_string(),
                    id: artist.id.clone(),
                })
                .collect()
        } else {
            track.credits.clone()
        };
        let portraits = self.detail.read(cx).portraits().clone();
        InfoCard::new(t!("song-credits"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .children(rows.into_iter().enumerate().map(|(index, credit)| {
                        let portrait = credit.id.as_ref().and_then(|id| portraits.get(id)).cloned();
                        let avatar = match portrait {
                            Some(portrait) => Avatar::new(Some(portrait))
                                .size(theme.metrics.thumb)
                                .into_any_element(),
                            None => Initials::new(credit.name.clone(), theme.metrics.thumb)
                                .into_any_element(),
                        };
                        let row = div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px(theme.metrics.pad)
                            .py(theme.metrics.pad / 2.)
                            .rounded(theme.radius)
                            .child(avatar)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(div().font_weight(FontWeight::MEDIUM).child(credit.name))
                                    .child(
                                        div()
                                            .text_size(theme.text(Text::Small))
                                            .text_color(theme.muted_foreground)
                                            .child(credit.role),
                                    ),
                            );

                        match credit.id {
                            Some(id) => row
                                .id(("song-credit", index))
                                .cursor_pointer()
                                .hover(|style| style.bg(theme.secondary_hover))
                                .rounded(theme.radius)
                                .link(Destination::Artist(id.into()))
                                .into_any_element(),
                            None => row.into_any_element(),
                        }
                    })),
            )
            .into_any_element()
    }

    fn discovery(&self, track: &Track, cx: &Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let tags = track.tags.clone();
        let languages = match track.languages.is_empty() {
            true => t!("common-not-provided"),
            false => SharedString::from(
                track
                    .languages
                    .iter()
                    .map(|language| Self::language_label(language))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        };
        InfoCard::new(t!("song-details"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().when_else(
                        tags.is_empty(),
                        |this| this.child(Fact::new(t!("song-genres"), t!("common-not-available"))),
                        |this| {
                            this.child(
                                div()
                                    .flex()
                                    .items_start()
                                    .justify_between()
                                    .gap_4()
                                    .px(theme.metrics.pad)
                                    .py(theme.metrics.pad / 2.)
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_color(theme.muted_foreground)
                                            .child(t!("song-genres")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_1()
                                            .min_w_0()
                                            .flex_wrap()
                                            .justify_end()
                                            .gap_2()
                                            .children(tags.into_iter().map(|tag| {
                                                div()
                                                    .px_3()
                                                    .py_1()
                                                    .rounded_full()
                                                    .bg(theme.secondary)
                                                    .border_1()
                                                    .border_color(theme.border)
                                                    .text_size(theme.text(Text::Small))
                                                    .child(tag)
                                            })),
                                    ),
                            )
                        },
                    ))
                    .child(Fact::new(t!("song-language"), languages).striped(true))
                    .child(Fact::new(
                        t!("song-content"),
                        match track.explicit {
                            true => t!("song-explicit"),
                            false => t!("song-clean"),
                        },
                    )),
            )
            .into_any_element()
    }

    fn artist_profile(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *cx.theme();
        let detail = self.detail.read(cx);
        let artist = detail.artist()?;
        let artist_id = detail.track()?.artist_refs.first()?.id.clone()?;
        let bio = artist
            .biography
            .clone()
            .unwrap_or_else(|| t!("song-artist-fallback").to_string());
        let bio = if bio.chars().count() > 360 {
            format!("{}…", bio.chars().take(360).collect::<String>())
        } else {
            bio
        };
        Some(
            div()
                .id("song-artist-profile")
                .flex()
                .items_center()
                .gap_5()
                .p_5()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .cursor_pointer()
                .hover(|style| style.bg(theme.secondary))
                .link(Destination::Artist(artist_id.into()))
                .child(
                    Artwork::new(artist.cover_large.clone())
                        .size(px(88.))
                        .circle(),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap_2()
                        .child(eyebrow(t!("song-about-artist"), cx))
                        .child(heading(artist.name.clone(), cx))
                        .child(div().text_color(theme.muted_foreground).child(bio)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(theme.text(Text::Small))
                        .font_weight(FontWeight::MEDIUM),
                )
                .into_any_element(),
        )
    }
}

impl Render for SongView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let scroll = self.scrollbar.read(cx).scroll().clone();
        let (track, error, loading) = {
            let detail = self.detail.read(cx);
            (
                detail.track().cloned(),
                detail.error().map(str::to_owned),
                detail.is_loading(),
            )
        };

        div()
            .size_full()
            .child(
                div()
                    .id("song-page")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .px(theme.metrics.inset)
                    .py(theme.metrics.inset)
                    .when(loading && track.is_none(), |this| {
                        this.child(
                            div()
                                .py_8()
                                .text_color(theme.muted_foreground)
                                .child(t!("song-loading")),
                        )
                    })
                    .when_some(error, |this, error| {
                        this.child(div().pb_4().text_color(theme.danger).child(error))
                    })
                    .when_some(track, |this, track| {
                        this.child(self.hero(&track, cx))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_stretch()
                                    .gap_5()
                                    .child(
                                        div()
                                            .min_w(px(300.))
                                            .flex_1()
                                            .child(self.overview(&track, cx)),
                                    )
                                    .child(
                                        div()
                                            .min_w(px(300.))
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_5()
                                            .child(self.credits(&track, cx))
                                            .child(self.discovery(&track, cx)),
                                    ),
                            )
                            .when_some(self.artist_profile(cx), |this, profile| {
                                this.child(div().pt_5().child(profile))
                            })
                            .when_some(
                                self.detail
                                    .read(cx)
                                    .album()
                                    .and_then(|album| album.album.copyrights.first())
                                    .cloned(),
                                |this, copyright| {
                                    let copyright = match copyright.starts_with(['©', '℗']) {
                                        true => SharedString::from(copyright),
                                        false => t!("song-copyright", notice = copyright),
                                    };
                                    this.child(
                                        div()
                                            .pt_5()
                                            .text_size(theme.text(Text::Tiny))
                                            .text_color(theme.muted_foreground)
                                            .child(copyright),
                                    )
                                },
                            )
                    }),
            )
            .child(self.scrollbar.clone())
    }
}
