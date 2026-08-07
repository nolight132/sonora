// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

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
    Button, Initials, Look, MAX_FONT, MIN_FONT, Menu, MenuItem, Rounding, Skeleton, Text, Theme,
    ThemeKind,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
const SOURCE_URL: &str = "https://github.com/nolight132/sonora";

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
    themes_open: bool,
    corners_open: bool,
    languages_open: bool,
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
            themes_open: false,
            corners_open: false,
            languages_open: false,
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
                    this.themes_open = false;
                    this.corners_open = false;
                    this.languages_open = false;
                    cx.notify();
                }))
        }))
    }

    fn panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let rows: Vec<AnyElement> = match self.tab {
            Tab::Appearance => vec![
                self.theme_row(cx).into_any_element(),
                self.adaptive_row(cx).into_any_element(),
                self.corners_row(cx).into_any_element(),
                self.language_row(cx).into_any_element(),
                self.font_row(cx).into_any_element(),
                self.auto_hide_row(cx).into_any_element(),
            ]
            .into_iter()
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

        let picker = div()
            .relative()
            .child(
                Button::new("language-picker")
                    .label(format!("{current}  ▾"))
                    .small()
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.languages_open = !this.languages_open;
                        cx.notify();
                    })),
            )
            .when(self.languages_open, |this| {
                this.child(
                    Menu::new("language-dropdown")
                        .top(px(30.))
                        .right_0()
                        .w(px(170.))
                        .on_dismiss(cx.listener(|this, _, _, cx| {
                            this.languages_open = false;
                            cx.notify();
                        }))
                        .items(entries.map(|(id, label)| {
                            MenuItem::new(id, label)
                                .selected(chosen == id)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings
                                        .update(cx, |settings, cx| settings.set_language(id, cx));
                                    this.languages_open = false;
                                    cx.notify();
                                }))
                        })),
                )
            });

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

        let picker = div()
            .relative()
            .child(
                Button::new("corners-picker")
                    .label(format!("{}  ▾", look.rounding.label()))
                    .small()
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.corners_open = !this.corners_open;
                        cx.notify();
                    })),
            )
            .when(self.corners_open, |this| {
                this.child(
                    Menu::new("corners-dropdown")
                        .top(px(30.))
                        .right_0()
                        .w(px(170.))
                        .on_dismiss(cx.listener(|this, _, _, cx| {
                            this.corners_open = false;
                            cx.notify();
                        }))
                        .items(Rounding::ALL.into_iter().map(|rounding| {
                            let overrides = overrides.clone();
                            MenuItem::new(rounding.id(), rounding.label())
                                .selected(look.rounding == rounding)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.update(cx, |settings, cx| {
                                        settings.set_rounding(rounding.id(), cx);
                                    });
                                    this.corners_open = false;
                                    Theme::set(Look { rounding, ..look }, &overrides, cx);
                                    cx.notify();
                                }))
                        })),
                )
            });

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
        let overrides = self.settings.read(cx).theme_overrides().clone();

        let picker = div()
            .relative()
            .child(
                Button::new("theme-picker")
                    .label(format!("{}  ▾", current.label()))
                    .small()
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.themes_open = !this.themes_open;
                        cx.notify();
                    })),
            )
            .when(self.themes_open, |this| {
                this.child(
                    Menu::new("theme-dropdown")
                        .top(px(30.))
                        .right_0()
                        .w(px(170.))
                        .on_dismiss(cx.listener(|this, _, _, cx| {
                            this.themes_open = false;
                            cx.notify();
                        }))
                        .items(ThemeKind::ALL.into_iter().map(|kind| {
                            let overrides = overrides.clone();
                            MenuItem::new(kind.id(), kind.label())
                                .selected(current == kind)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.update(cx, |settings, cx| {
                                        settings.set_theme(kind.id(), cx);
                                    });
                                    this.themes_open = false;
                                    Theme::fade(Look { kind, ..look }, &overrides, cx);
                                    cx.notify();
                                }))
                        })),
                )
            });

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

    fn adaptive_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let muted = theme.muted_foreground;
        let small = theme.text(Text::Small);
        let on = self.settings.read(cx).adaptive_theme();

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
                    this.settings
                        .update(cx, |settings, cx| settings.set_adaptive_theme(!on, cx));
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
                    .when(self.tab == Tab::About, |this| this.child(self.notice(cx))),
            )
    }
}
