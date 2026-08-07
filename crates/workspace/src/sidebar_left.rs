use std::cell::Cell;
use ui::{ActiveTheme as _, Button, Room, Shield};

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, DragMoveEvent, ElementId, Empty, Entity, Hsla, MouseButton,
    MouseDownEvent, Pixels, Render,
};
use gpui::{Window, div, px};
use router::{Destination, LibraryTab, Navigation, NavigationEvent, navigate};
use state::{AppSettings, Sonora};

const NAV: [(&str, &str, Option<Destination>); 4] = [
    ("nav-home", "icons/house.svg", Some(Destination::Home)),
    ("nav-search", "icons/search.svg", Some(Destination::Search)),
    (
        "nav-library",
        "icons/library-big.svg",
        Some(Destination::Library(LibraryTab::Songs)),
    ),
    (
        "nav-settings",
        "icons/settings.svg",
        Some(Destination::Settings),
    ),
];

const TABS: [(&str, LibraryTab); 3] = [
    ("nav-songs", LibraryTab::Songs),
    ("nav-albums", LibraryTab::Albums),
    ("nav-playlists", LibraryTab::Playlists),
];

const MIN_WIDTH: Pixels = px(130.);
const MAX_WIDTH: Pixels = px(400.);

struct SidebarResize {
    start_width: Pixels,
    start_x: Cell<Pixels>,
}

pub struct Sidebar {
    settings: Entity<AppSettings>,
    trail: Entity<Navigation>,
    width: Pixels,
    open: bool,
    cramped: bool,
    forced: Option<bool>,
    library_open: bool,
}

impl Sidebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        let width = px(settings.read(cx).sidebar_width()).clamp(MIN_WIDTH, MAX_WIDTH);
        let open = settings.read(cx).sidebar_open();
        let trail = router::trail(cx);

        cx.observe(&trail, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&trail, |this, _, _: &NavigationEvent, cx| this.dismiss(cx))
            .detach();

        Self {
            settings,
            trail,
            width,
            open,
            forced: None,
            cramped: false,
            library_open: true,
        }
    }

    pub fn is_open(&self) -> bool {
        self.forced.unwrap_or(self.open && !self.cramped)
    }

    pub fn overlays(&self) -> bool {
        self.cramped && self.is_open()
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.overlays() {
            return;
        }
        self.forced = Some(false);
        cx.notify();
    }

    pub fn occupied_width(&self) -> Pixels {
        match self.is_open() && !self.overlays() {
            true => self.width,
            false => Pixels::ZERO,
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        match self.cramped {
            true => self.forced = Some(!self.is_open()),
            false => {
                self.open = !self.open;
                self.persist(cx);
            }
        }
        cx.notify();
    }

    pub fn adapt(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.width = ui::snapped(self.width, window);

        let auto_hide = self.settings.read(cx).auto_hide_sidebar();
        let space_left = window.viewport_size().width - self.width;
        let cramped = auto_hide && !Room::of(space_left).fits(Room::Wide);
        if cramped != self.cramped {
            self.cramped = cramped;
            self.forced = None;
        }
    }

    fn persist(&self, cx: &mut Context<Self>) {
        let width = self.width / px(1.);
        let open = self.open;
        self.settings
            .update(cx, |settings, cx| settings.set_sidebar(width, open, cx));
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let sidebar_accent = theme.sidebar_accent;
        let foreground = theme.foreground;
        let muted = theme.muted_foreground;
        let sidebar_bg = theme.sidebar;
        let sidebar_border = theme.sidebar_border;
        let nav = theme.metrics.control;
        let current = self.trail.read(cx).current();
        self.adapt(window, cx);

        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, (key, icon, destination)) in NAV.into_iter().enumerate() {
            if matches!(destination, Some(Destination::Library(_))) {
                let inside = matches!(current, Destination::Library(_));
                let text = if inside { foreground } else { muted };
                let link_destination = if inside { None } else { destination };
                let target = link_destination.unwrap_or(current.clone());

                rows.push(
                    nav_row(index, key, text, sidebar_accent)
                        .icon(icon)
                        .on_click(cx.listener(move |this, _, _, cx| match inside {
                            true => {
                                this.library_open = !this.library_open;
                                cx.notify();
                            }
                            false => navigate(target.clone(), cx),
                        }))
                        .into_any_element(),
                );

                if self.library_open {
                    let middle = nav / 2.;

                    rows.push(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .ml_4()
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .bottom(middle)
                                    .w(px(1.))
                                    .bg(sidebar_border),
                            )
                            .children(TABS.into_iter().map(|(name, tab)| {
                                let chosen = current == Destination::Library(tab);
                                let tint = if chosen { foreground } else { muted };

                                div()
                                    .relative()
                                    .flex()
                                    .items_center()
                                    .h(nav)
                                    .pl_3()
                                    .child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .top(middle)
                                            .w(px(6.))
                                            .h(px(1.))
                                            .bg(sidebar_border),
                                    )
                                    .child(
                                        nav_row(name, name, tint, sidebar_accent)
                                            .flex_1()
                                            .when(chosen, |button| button.bg(sidebar_accent))
                                            .on_click(move |_, _, cx| {
                                                navigate(Destination::Library(tab), cx)
                                            }),
                                    )
                            }))
                            .into_any_element(),
                    );
                }
                continue;
            }

            let active = destination
                .as_ref()
                .is_some_and(|it| it.same_section(&current));
            let text = if active { foreground } else { muted };

            rows.push(
                nav_row(index, key, text, sidebar_accent)
                    .icon(icon)
                    .when(active, |button| button.bg(sidebar_accent))
                    .when_some(destination, |button, destination| {
                        button.on_click(move |_, _, cx| navigate(destination.clone(), cx))
                    })
                    .into_any_element(),
            );
        }

        let overlaid = self.overlays();
        let panel = div()
            .flex()
            .flex_col()
            .when(!self.is_open(), |this| this.hidden())
            .relative()
            .w(self.width)
            .flex_none()
            .h_full()
            .bg(sidebar_bg)
            .border_r_1()
            .border_color(sidebar_border)
            .when(overlaid, |this| {
                this.occlude().absolute().left_0().top_0().bottom_0()
            })
            .child(div().flex().flex_col().gap_1().p_3().children(rows))
            .child(
                div()
                    .id("sidebar-resize-handle")
                    .absolute()
                    .top_0()
                    .right(px(-4.))
                    .w(px(8.))
                    .h_full()
                    .cursor_col_resize()
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<SidebarResize>, window, cx| {
                            let resize = event.drag(cx);
                            let dragged = (resize.start_width + event.event.position.x
                                - resize.start_x.get())
                            .clamp(MIN_WIDTH, MAX_WIDTH);
                            this.width = ui::snapped(dragged, window);
                            this.persist(cx);
                            cx.notify();
                        },
                    ))
                    .on_drag(
                        SidebarResize {
                            start_width: self.width,
                            start_x: Cell::new(Pixels::ZERO),
                        },
                        |resize, _, window, cx| {
                            resize.start_x.set(window.mouse_position().x);
                            cx.new(|_| Empty)
                        },
                    ),
            );

        match overlaid {
            false => panel.into_any_element(),
            true => div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(
                    Shield::new("sidebar-shield")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss(cx)),
                        ),
                )
                .child(panel)
                .into_any_element(),
        }
    }
}

fn nav_row(id: impl Into<ElementId>, key: &'static str, tint: Hsla, accent: Hsla) -> Button {
    Button::new(id)
        .ghost()
        .label(i18n::lookup(key, None))
        .tint(tint)
        .gap_2p5()
        .justify_start()
        .hover(move |style| style.bg(accent))
        .active(move |style| style.bg(accent))
}
