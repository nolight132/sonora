// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::process::Command;

use gpui::{
    AnyElement, Context, Entity, FontWeight, Pixels, Render, SharedString, Window, div, px,
};
use gpui::{ScrollHandle, prelude::*};
use i18n::{Language, t};
use state::{AppSettings, Playback, Session, SessionState, Sonora};
use ui::{ActiveTheme as _, Scrollbar, Scroller};
use ui::{
    Avatar, Button, InfoCard, Initials, Look, MAX_FONT, MAX_TRANSPARENCY, MIN_FONT, Menu, MenuItem,
    Popover, Popovers, Rounding, Scrubber, ScrubberState, Skeleton, Text, Theme, ThemeKind,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
const SOURCE_URL: &str = "https://github.com/nolight132/sonora";

const THEMES: &str = "themes";
const CORNERS: &str = "corners";
const LANGUAGES: &str = "languages";

#[derive(Clone, Copy)]
struct Member {
    login: &'static str,
    avatar: &'static str,
    profile: &'static str,
    role: Role,
}

#[derive(Clone, Copy)]
enum Role {
    LeadMaintainer,
    Maintainer,
    Contributor,
}

impl Role {
    fn label(self) -> SharedString {
        match self {
            Self::LeadMaintainer => t!("settings-role-lead-maintainer"),
            Self::Maintainer => t!("settings-role-maintainer"),
            Self::Contributor => t!("settings-role-contributor"),
        }
    }
}

macro_rules! member {
    ($login:literal, $role:expr) => {
        Member {
            login: $login,
            avatar: concat!("https://github.com/", $login, ".png"),
            profile: concat!("https://github.com/", $login),
            role: $role,
        }
    };
}

const MEMBERS: [Member; 5] = [
    member!("nolight132", Role::LeadMaintainer),
    member!("zxsleebu", Role::Maintainer),
    member!("fx-got", Role::Maintainer),
    member!("Makakashan", Role::Contributor),
    member!("imizgun", Role::Contributor),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Appearance,
    Playback,
    Account,
    About,
}

impl Tab {
    const ALL: [Self; 4] = [Self::Appearance, Self::Playback, Self::Account, Self::About];

    fn id(self) -> &'static str {
        match self {
            Self::Appearance => "tab-appearance",
            Self::Playback => "tab-playback",
            Self::Account => "tab-account",
            Self::About => "tab-about",
        }
    }

    fn label(self) -> SharedString {
        match self {
            Self::Appearance => t!("settings-tab-appearance"),
            Self::Playback => t!("settings-tab-playback"),
            Self::Account => t!("settings-tab-account"),
            Self::About => t!("settings-tab-about"),
        }
    }
}

pub struct SettingsView {
    session: Entity<Session>,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    tab: Tab,
    scrollbar: Entity<Scrollbar>,
    transparency: ScrubberState,
    popovers: Popovers,
}

impl SettingsView {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&session, |_, _, cx| cx.notify()).detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        Self {
            session,
            playback,
            settings,
            tab: Tab::Appearance,
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new())),
            transparency: ScrubberState::new("transparency"),
            popovers: Popovers::default(),
        }
    }

    fn tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().gap_1().children(Tab::ALL.map(|tab| {
            Button::new(tab.id())
                .label(tab.label())
                .small()
                .selected(self.tab == tab)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = tab;
                    this.popovers.close();
                    cx.notify();
                }))
        }))
    }

    fn panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let rows: Vec<AnyElement> = match self.tab {
            Tab::Appearance => vec![
                self.theme_row(cx).into_any_element(),
                self.transparent_row(cx).into_any_element(),
            ]
            .into_iter()
            .chain(
                self.settings
                    .read(cx)
                    .transparent()
                    .then(|| self.transparency_row(cx).into_any_element()),
            )
            .chain([
                self.adaptive_row(cx).into_any_element(),
                self.corners_row(cx).into_any_element(),
                self.language_row(cx).into_any_element(),
                self.font_row(cx).into_any_element(),
                self.auto_hide_row(cx).into_any_element(),
            ])
            .chain(decorated().then(|| self.decorations_row(cx).into_any_element()))
            .chain(decorated().then(|| self.side_row(cx).into_any_element()))
            .collect(),
            Tab::Playback => vec![self.playback_row(cx).into_any_element()],
            Tab::Account => vec![self.account_row(cx).into_any_element()],
            Tab::About => vec![
                self.version_row(cx).into_any_element(),
                self.license_row(cx).into_any_element(),
                self.source_row(cx).into_any_element(),
            ],
        };

        let mut panel = div().flex().flex_col();
        for (index, row) in rows.into_iter().enumerate() {
            if index > 0 {
                panel = panel.child(div().h(px(1.)).w_full().bg(border));
            }
            panel = panel.child(row);
        }
        panel
    }

    fn look(&self, cx: &Context<Self>) -> Look {
        Look {
            tint: cx.theme().tint,
            ..self.settings.read(cx).look()
        }
    }

    fn language_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let chosen = self.settings.read(cx).language().to_owned();
        let current = match Language::from_id(&chosen) {
            Some(language) => SharedString::from(language.label()),
            None => t!("settings-language-system"),
        };

        let entries = std::iter::once((i18n::AUTO, t!("settings-language-system"))).chain(
            Language::ALL
                .into_iter()
                .map(|language| (language.id(), SharedString::from(language.label()))),
        );

        let picker = Popover::new(LANGUAGES, self.popovers.clone())
            .button(
                Button::new("language-picker")
                    .label(format!("{current}  ▾"))
                    .small()
                    .outline(),
            )
            .menu(
                Menu::new("language-dropdown")
                    .top(px(30.))
                    .right_0()
                    .w(px(170.))
                    .items(entries.map(|(id, label)| {
                        MenuItem::new(id, label)
                            .selected(chosen == id)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings
                                    .update(cx, |settings, cx| settings.set_language(id, cx));
                                this.popovers.close();
                                cx.notify();
                            }))
                    })),
            );

        self.row(
            t!("settings-language"),
            t!("settings-language-detail"),
            muted,
            small,
            picker.into_any_element(),
        )
    }

    fn corners_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides().clone();

        let picker = Popover::new(CORNERS, self.popovers.clone())
            .button(
                Button::new("corners-picker")
                    .label(format!("{}  ▾", look.rounding.label()))
                    .small()
                    .outline(),
            )
            .menu(
                Menu::new("corners-dropdown")
                    .top(px(30.))
                    .right_0()
                    .w(px(170.))
                    .items(Rounding::ALL.into_iter().map(|rounding| {
                        let overrides = overrides.clone();
                        MenuItem::new(rounding.id(), rounding.label())
                            .selected(look.rounding == rounding)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings.update(cx, |settings, cx| {
                                    settings.set_rounding(rounding.id(), cx);
                                });
                                this.popovers.close();
                                Theme::set(Look { rounding, ..look }, &overrides, cx);
                                cx.notify();
                            }))
                    })),
            );

        self.row(
            t!("settings-corners"),
            t!("settings-corners-detail"),
            muted,
            small,
            picker.into_any_element(),
        )
    }

    fn font_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides().clone();

        let step = move |id: &'static str, label: &'static str, delta: f32| {
            let overrides = overrides.clone();
            let wanted = (look.font + delta).clamp(MIN_FONT, MAX_FONT);

            Button::new(id)
                .label(label)
                .small()
                .outline()
                .disabled(wanted == look.font)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_font_size(wanted, cx));
                    Theme::set(
                        Look {
                            font: wanted,
                            ..look
                        },
                        &overrides,
                        cx,
                    );
                    cx.notify();
                }))
        };

        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(step("font-smaller", "−", -1.))
            .child(div().child(t!("settings-font-value", size = look.font.round() as i64)))
            .child(step("font-larger", "+", 1.));

        self.row(
            t!("settings-font"),
            t!("settings-font-detail"),
            muted,
            small,
            actions.into_any_element(),
        )
    }

    fn decorations_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).window_controls();

        self.row(
            t!("settings-window-controls"),
            t!("settings-window-controls-detail"),
            muted,
            small,
            Button::new("window-controls")
                .label(match on {
                    true => t!("common-on"),
                    false => t!("common-off"),
                })
                .small()
                .outline()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_window_controls(!on, cx));
                }))
                .into_any_element(),
        )
    }

    fn side_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let settings = self.settings.read(cx);
        let left = settings.controls_on_left();
        let shown = settings.window_controls();

        self.row(
            t!("settings-controls-side"),
            t!("settings-controls-side-detail"),
            muted,
            small,
            Button::new("controls-side")
                .label(match left {
                    true => t!("common-left"),
                    false => t!("common-right"),
                })
                .small()
                .outline()
                .disabled(!shown)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_controls_on_left(!left, cx));
                }))
                .into_any_element(),
        )
    }

    fn auto_hide_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).auto_hide_sidebar();

        self.row(
            t!("settings-auto-hide"),
            t!("settings-auto-hide-detail"),
            muted,
            small,
            Button::new("auto-hide-sidebar")
                .label(match on {
                    true => t!("common-on"),
                    false => t!("common-off"),
                })
                .small()
                .outline()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_auto_hide_sidebar(!on, cx));
                }))
                .into_any_element(),
        )
    }

    fn profile(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;

        div()
            .flex()
            .items_center()
            .gap_4()
            .child(match self.session.read(cx).state() {
                SessionState::SignedIn(profile) => {
                    Initials::new(profile.display_name.clone(), px(64.)).into_any_element()
                }
                _ => Skeleton::new().size(px(64.)).circle().into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(match self.session.read(cx).state() {
                        SessionState::SignedIn(profile) => div()
                            .child(profile.display_name.clone())
                            .text_size(theme.text(Text::Large))
                            .font_weight(FontWeight::SEMIBOLD)
                            .into_any_element(),
                        _ => Skeleton::new().w(px(140.)).h(px(14.)).into_any_element(),
                    })
                    .child(match self.session.read(cx).state() {
                        SessionState::SignedIn(profile) => div()
                            .child(profile.id.clone())
                            .text_color(muted)
                            .text_size(theme.text(Text::Small))
                            .into_any_element(),
                        _ => Skeleton::new().w(px(90.)).h(px(10.)).into_any_element(),
                    }),
            )
    }

    fn theme_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let look = self.look(cx);
        let current = look.kind;
        let adaptive = self.settings.read(cx).adaptive_theme();
        let overrides = self.settings.read(cx).theme_overrides().clone();

        let picker = Popover::new(THEMES, self.popovers.clone())
            .button(
                Button::new("theme-picker")
                    .label(format!("{}  ▾", current.label()))
                    .small()
                    .outline(),
            )
            .menu(
                Menu::new("theme-dropdown")
                    .top(px(30.))
                    .right_0()
                    .w(px(170.))
                    .items(ThemeKind::ALL.into_iter().map(|kind| {
                        let item = MenuItem::new(kind.id(), kind.label()).selected(current == kind);
                        match adaptive && !matches!(kind, ThemeKind::Dark | ThemeKind::Light) {
                            true => item.disabled(),
                            false => {
                                let overrides = overrides.clone();
                                item.on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.update(cx, |settings, cx| {
                                        settings.set_theme(kind.id(), cx);
                                    });
                                    this.popovers.close();
                                    Theme::fade(Look { kind, ..look }, &overrides, cx);
                                    cx.notify();
                                }))
                            }
                        }
                    })),
            );

        let settings = self.settings.clone();
        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("open-theme-config")
                    .label(t!("settings-theme-config"))
                    .small()
                    .outline()
                    .on_click(move |_, _, cx| {
                        let path = settings.update(cx, |settings, _| settings.ensure_file());
                        if let Err(error) = open_settings_file(&path) {
                            eprintln!("sonora: cannot open {}: {error}", path.display());
                        }
                    }),
            )
            .child(picker);

        self.row(
            t!("settings-theme"),
            t!("settings-theme-detail"),
            muted,
            small,
            actions.into_any_element(),
        )
    }

    fn transparent_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides().clone();

        self.row(
            t!("settings-transparent"),
            t!("settings-transparent-detail"),
            muted,
            small,
            Button::new("transparent-background")
                .label(match look.transparent {
                    true => t!("common-on"),
                    false => t!("common-off"),
                })
                .small()
                .outline()
                .on_click(cx.listener(move |this, _, _, cx| {
                    let transparent = !look.transparent;
                    this.settings
                        .update(cx, |settings, cx| settings.set_transparent(transparent, cx));
                    Theme::fade(
                        Look {
                            transparent,
                            ..look
                        },
                        &overrides,
                        cx,
                    );
                }))
                .into_any_element(),
        )
    }

    fn transparency_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides().clone();
        let value = look.transparency / MAX_TRANSPARENCY;
        let percent = (look.transparency * 100.).round() as i64;

        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div().w(theme.metrics.cover).child(
                    Scrubber::new(&self.transparency, value)
                        .colors(theme.progress_bar, theme.muted, theme.foreground)
                        .on_move(cx.listener(move |this, fraction: &f32, _, cx| {
                            let transparency = *fraction * MAX_TRANSPARENCY;
                            this.settings.update(cx, |settings, cx| {
                                settings.set_transparency(transparency, cx)
                            });
                            Theme::set(
                                Look {
                                    transparency,
                                    ..look
                                },
                                &overrides,
                                cx,
                            );
                        })),
                ),
            )
            .child(
                div()
                    .w(theme.metrics.control)
                    .text_right()
                    .child(t!("settings-transparency-value", percent = percent)),
            );

        self.row(
            t!("settings-transparency"),
            t!("settings-transparency-detail"),
            muted,
            small,
            control.into_any_element(),
        )
    }

    fn adaptive_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).adaptive_theme();
        let look = self.look(cx);
        let overrides = self.settings.read(cx).theme_overrides().clone();

        self.row(
            t!("settings-adaptive"),
            t!("settings-adaptive-detail"),
            muted,
            small,
            Button::new("adaptive-theme")
                .label(match on {
                    true => t!("common-on"),
                    false => t!("common-off"),
                })
                .small()
                .outline()
                .on_click(cx.listener(move |this, _, _, cx| {
                    let adaptive = !on;
                    let kind = match adaptive
                        && !matches!(look.kind, ThemeKind::Dark | ThemeKind::Light)
                    {
                        true => ThemeKind::Dark,
                        false => look.kind,
                    };
                    this.settings.update(cx, |settings, cx| {
                        settings.set_adaptive_theme(adaptive, cx);
                        if kind != look.kind {
                            settings.set_theme(kind.id(), cx);
                        }
                    });
                    if kind != look.kind {
                        Theme::fade(Look { kind, ..look }, &overrides, cx);
                    }
                }))
                .into_any_element(),
        )
    }

    fn playback_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.playback.read(cx).normalisation();

        self.row(
            t!("settings-normalisation"),
            t!("settings-normalisation-detail"),
            muted,
            small,
            Button::new("normalisation")
                .label(match on {
                    true => t!("common-on"),
                    false => t!("common-off"),
                })
                .small()
                .outline()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.playback
                        .update(cx, |playback, cx| playback.set_normalisation(!on, cx));
                }))
                .into_any_element(),
        )
    }

    fn account_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let session = self.session.clone();

        self.row(
            t!("settings-account"),
            t!("settings-account-detail"),
            muted,
            small,
            Button::new("sign-out")
                .label(t!("settings-sign-out"))
                .small()
                .outline()
                .icon("icons/log-out.svg")
                .on_click(move |_, _, cx| {
                    session.update(cx, |session, cx| session.sign_out(cx));
                })
                .into_any_element(),
        )
    }

    fn version_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        self.row(
            t!("settings-version"),
            t!("settings-version-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            div().child(VERSION).into_any_element(),
        )
    }

    fn license_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        self.row(
            t!("settings-license"),
            t!("settings-license-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            Button::new("license")
                .label(t!("settings-license-view"))
                .small()
                .outline()
                .icon("icons/link.svg")
                .on_click(|_, _, cx| cx.open_url(LICENSE_URL))
                .into_any_element(),
        )
    }

    fn source_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        self.row(
            t!("settings-source"),
            t!("settings-source-detail"),
            theme.muted_foreground,
            theme.text(Text::Small),
            Button::new("source")
                .label(t!("settings-source-view"))
                .small()
                .outline()
                .icon("icons/link.svg")
                .on_click(|_, _, cx| cx.open_url(SOURCE_URL))
                .into_any_element(),
        )
    }

    fn notice(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .text_color(theme.muted_foreground)
            .text_size(theme.text(Text::Small))
            .child(t!("settings-notice"))
    }

    fn team(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        InfoCard::new(t!("settings-team")).flex_none().child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .children(MEMBERS.into_iter().enumerate().map(|(index, member)| {
                    div()
                        .id(("team-member", index))
                        .flex()
                        .items_center()
                        .gap_3()
                        .px(theme.metrics.pad)
                        .py(theme.metrics.pad / 2.)
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.secondary_hover))
                        .on_click(move |_, _, cx| cx.open_url(member.profile))
                        .child(Avatar::new(Some(member.avatar)).size(theme.metrics.thumb))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(div().font_weight(FontWeight::MEDIUM).child(member.login))
                                .child(
                                    div()
                                        .text_size(theme.text(Text::Small))
                                        .text_color(theme.muted_foreground)
                                        .child(t!("settings-team-github")),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(theme.text(Text::Small))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.muted_foreground)
                                .child(member.role.label()),
                        )
                })),
        )
    }

    fn row(
        &self,
        title: SharedString,
        detail: SharedString,
        muted: gpui::Hsla,
        small: Pixels,
        action: gpui::AnyElement,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .py_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().child(title))
                    .child(div().text_color(muted).text_size(small).child(detail)),
            )
            .child(action)
    }
}

fn decorated() -> bool {
    cfg!(not(target_os = "macos"))
}

fn open_settings_file(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()?;

    #[cfg(target_os = "macos")]
    Command::new("open").arg(path).spawn()?;

    #[cfg(target_os = "linux")]
    Command::new("xdg-open").arg(path).spawn()?;

    Ok(())
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;

        Scroller::new("settings", &self.scrollbar)
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .w_full()
                    .max_w(px(640.))
                    .p_6()
                    .child(self.profile(cx))
                    .child(div().h(px(1.)).w_full().bg(border))
                    .child(self.tabs(cx))
                    .child(self.panel(cx))
                    .when(self.tab == Tab::About, |this| {
                        this.child(self.team(cx)).child(self.notice(cx))
                    }),
            )
    }
}
