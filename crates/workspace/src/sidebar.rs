use std::cell::Cell;
use ui::ActiveTheme as _;

use gpui::prelude::*;
use gpui::{AnyElement, Context, DragMoveEvent, Empty, Entity, Pixels, Render};
use gpui::{Window, div, px, svg};
use router::{Destination, LibraryTab, Link as _, Navigation};
use state::{AppSettings, Spotty};

const NAV: [(&str, &str, Option<Destination>); 4] = [
    ("Home", "icons/house.svg", Some(Destination::Home)),
    ("Search", "icons/search.svg", Some(Destination::Search)),
    (
        "Your Library",
        "icons/library-big.svg",
        Some(Destination::Library(LibraryTab::Songs)),
    ),
    (
        "Settings",
        "icons/settings.svg",
        Some(Destination::Settings),
    ),
];

const TABS: [(&str, LibraryTab); 3] = [
    ("Songs", LibraryTab::Songs),
    ("Albums", LibraryTab::Albums),
    ("Playlists", LibraryTab::Playlists),
];

const MIN_WIDTH: Pixels = px(130.);
const MAX_WIDTH: Pixels = px(400.);
const NARROW: Pixels = px(520.);

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
}

impl Sidebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = Spotty::global(cx).settings.clone();
        let width = px(settings.read(cx).sidebar_width()).clamp(MIN_WIDTH, MAX_WIDTH);
        let open = settings.read(cx).sidebar_open();
        let trail = router::trail(cx);

        cx.observe(&trail, |_, _, cx| cx.notify()).detach();

        Self {
            settings,
            trail,
            width,
            open,
            forced: None,
            cramped: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.forced.unwrap_or(self.open && !self.cramped)
    }

    pub fn occupied_width(&self) -> Pixels {
        if self.is_open() {
            self.width
        } else {
            Pixels::ZERO
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
        let cramped = auto_hide && space_left < NARROW;
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
        let radius = theme.radius;
        let current = self.trail.read(cx).current();
        self.adapt(window, cx);

        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, (label, icon, destination)) in NAV.into_iter().enumerate() {
            if matches!(destination, Some(Destination::Library(_))) {
                let inside = matches!(current, Destination::Library(_));
                let text = if inside { foreground } else { muted };

                rows.push(
                    div()
                        .id(index)
                        .flex()
                        .items_center()
                        .gap_2p5()
                        .h(nav)
                        .px_3()
                        .rounded(radius)
                        .cursor_pointer()
                        .hover(move |style| style.bg(sidebar_accent))
                        .child(svg().path(icon).size_4().flex_none().text_color(text))
                        .child(div().text_color(text).child(label))
                        .link(Destination::Library(LibraryTab::Songs))
                        .into_any_element(),
                );

                let middle = nav / 2.;

                rows.push(
                    div()
                        .flex()
                        .flex_col()
                        .ml_4()
                        .children(TABS.into_iter().enumerate().map(|(step, (name, tab))| {
                            let chosen = current == Destination::Library(tab);
                            let tint = if chosen { foreground } else { muted };
                            let tail = step + 1 == TABS.len();

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
                                        .top_0()
                                        .w(px(1.))
                                        .h(if tail { middle } else { nav })
                                        .bg(sidebar_border),
                                )
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
                                    div()
                                        .id(name)
                                        .flex()
                                        .flex_1()
                                        .items_center()
                                        .h(nav)
                                        .px_3()
                                        .rounded(radius)
                                        .cursor_pointer()
                                        .when(chosen, |this| this.bg(sidebar_accent))
                                        .hover(move |style| style.bg(sidebar_accent))
                                        .child(div().text_color(tint).child(name))
                                        .link(Destination::Library(tab)),
                                )
                        }))
                        .into_any_element(),
                );
                continue;
            }

            let active = destination
                .as_ref()
                .is_some_and(|it| it.same_section(&current));
            let text = if active { foreground } else { muted };

            rows.push(
                div()
                    .id(index)
                    .flex()
                    .items_center()
                    .gap_2p5()
                    .h(nav)
                    .px_3()
                    .rounded(radius)
                    .cursor_pointer()
                    .when(active, |this| this.bg(sidebar_accent))
                    .hover(move |style| style.bg(sidebar_accent))
                    .child(svg().path(icon).size_4().flex_none().text_color(text))
                    .child(div().text_color(text).child(label))
                    .when_some(destination, |this, destination| this.link(destination))
                    .into_any_element(),
            );
        }

        div()
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
            )
    }
}
