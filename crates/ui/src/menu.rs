// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Anchor, AnyWindowHandle, App, Bounds, ClickEvent, Div, ElementId, Entity, Interactivity,
    MouseButton, MouseDownEvent, Pixels, Point, SharedString, Size, Stateful, StyleRefinement,
    Window, anchored, deferred, div, point, px, svg,
};

use crate::Artwork;
use crate::metrics::snapped;
use crate::scrollbar::Scrollbar;
use crate::shield::Shield;
use crate::theme::ActiveTheme as _;

const SUBMENU_CLOSE_DELAY: Duration = Duration::from_millis(160);
const SUBMENU_FALLBACK_WIDTH: Pixels = px(236.);
const SUBMENU_TOP: Pixels = px(-14.);
const SCROLLBAR_GUTTER: Pixels = px(8.);
const WINDOW_MARGIN: Pixels = px(8.);

type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type Dismiss = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
type Action = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Clone, Default)]
pub struct SubmenuState {
    open: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    parent_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    safe_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl SubmenuState {
    fn is_open(&self) -> bool {
        self.open.get()
    }

    fn hover(&self, hovered: bool, window: AnyWindowHandle, cx: &mut App) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);

        if hovered {
            if !self.open.replace(true) {
                cx.refresh_windows();
            }
            return;
        }

        let state = self.clone();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(SUBMENU_CLOSE_DELAY).await;
            let inside = cx.update(|cx| {
                cx.update_window(window, |_, window, _| {
                    state
                        .safe_bounds
                        .get()
                        .is_some_and(|bounds| bounds.contains(&window.mouse_position()))
                })
                .unwrap_or(false)
            });
            cx.update(|cx| {
                if state.generation.get() == generation && !inside && state.open.replace(false) {
                    cx.refresh_windows();
                }
            });
        })
        .detach();
    }

    fn observe_panel(&self, bounds: Bounds<Pixels>) {
        self.safe_bounds.set(Some(Bounds {
            origin: Point {
                x: bounds.origin.x - px(4.),
                y: bounds.origin.y - px(12.),
            },
            size: Size {
                width: bounds.size.width + px(16.),
                height: bounds.size.height + px(24.),
            },
        }));
    }

    fn observe_parent(&self, bounds: Bounds<Pixels>) {
        self.parent_bounds.set(Some(bounds));
    }

    fn should_flip(&self, viewport_width: Pixels) -> bool {
        let Some(parent) = self.parent_bounds.get() else {
            return false;
        };
        let submenu_width = self
            .safe_bounds
            .get()
            .map(|bounds| bounds.size.width)
            .unwrap_or(SUBMENU_FALLBACK_WIDTH);
        parent.right() + submenu_width + WINDOW_MARGIN > viewport_width
    }

    fn contains(&self, position: Point<Pixels>) -> bool {
        self.safe_bounds
            .get()
            .is_some_and(|bounds| bounds.contains(&position))
    }

    pub fn reset(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.open.set(false);
        self.parent_bounds.set(None);
        self.safe_bounds.set(None);
    }
}

struct Submenu {
    menu: Box<Menu>,
    state: SubmenuState,
}

pub struct MenuItem {
    id: ElementId,
    label: SharedString,
    selected: bool,
    disabled: bool,
    separator: bool,
    icon: Option<&'static str>,
    artwork: Option<Option<SharedString>>,
    press: Option<Press>,
    submenu: Option<Submenu>,
}

impl MenuItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            selected: false,
            disabled: false,
            separator: false,
            icon: None,
            artwork: None,
            press: None,
            submenu: None,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn separator(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: SharedString::default(),
            selected: false,
            disabled: true,
            separator: true,
            icon: None,
            artwork: None,
            press: None,
            submenu: None,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn icon(mut self, path: &'static str) -> Self {
        self.icon = Some(path);
        self
    }

    pub fn artwork(mut self, url: Option<impl Into<SharedString>>) -> Self {
        self.artwork = Some(url.map(Into::into));
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.press = Some(Box::new(handler));
        self
    }

    pub fn submenu(mut self, menu: Menu, state: SubmenuState) -> Self {
        let mut menu = menu;
        menu.hover_guard = Some(state.clone());
        self.submenu = Some(Submenu {
            menu: Box::new(menu),
            state,
        });
        self
    }
}

#[derive(IntoElement)]
pub struct Menu {
    base: Stateful<Div>,
    items: Vec<MenuItem>,
    dismiss: Option<Dismiss>,
    action: Option<Action>,
    priority: usize,
    deferred: bool,
    scrollbar: Option<Entity<Scrollbar>>,
    hover_guard: Option<SubmenuState>,
}

impl Menu {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            items: Vec::new(),
            dismiss: None,
            action: None,
            priority: 1,
            deferred: true,
            scrollbar: None,
            hover_guard: None,
        }
    }

    pub fn item(mut self, item: MenuItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = MenuItem>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn on_dismiss(
        mut self,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.dismiss = Some(Box::new(handler));
        self
    }

    pub fn on_action(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some(Rc::new(handler));
        self
    }

    pub fn priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }

    pub fn scrollbar(mut self, scrollbar: Entity<Scrollbar>) -> Self {
        self.scrollbar = Some(scrollbar);
        self
    }

    fn inline(mut self) -> Self {
        self.deferred = false;
        self
    }
}

impl Styled for Menu {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Menu {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Menu {}

impl RenderOnce for Menu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            items,
            dismiss,
            action,
            priority,
            deferred: should_defer,
            scrollbar,
            hover_guard,
        } = self;

        if let (Some(scrollbar), Some(guard)) = (scrollbar.as_ref(), hover_guard.clone()) {
            scrollbar.update(cx, |scrollbar, _| {
                scrollbar
                    .set_hover_guard(move |hovered, window, cx| guard.hover(hovered, window, cx));
            });
        }

        let theme = *cx.theme();
        let overrides = std::mem::take(base.style());
        let dismiss_guards: Vec<_> = items
            .iter()
            .filter_map(|item| item.submenu.as_ref().map(|submenu| submenu.state.clone()))
            .collect();
        let bounds_guards = dismiss_guards.clone();
        let viewport_width = window.viewport_size().width;

        let rows = items.into_iter().map(move |item| {
            let MenuItem {
                id,
                label,
                selected,
                disabled,
                separator,
                icon,
                artwork,
                press,
                submenu,
            } = item;

            if separator {
                return div()
                    .id(id)
                    .h(px(1.))
                    .flex_none()
                    .mx_2()
                    .my_1()
                    .bg(theme.border)
                    .into_any_element();
            }
            let action = action.clone();
            let press_action = action.clone();
            let submenu_state = submenu.as_ref().map(|submenu| submenu.state.clone());
            let item_hover_guard = hover_guard.clone();
            let has_artwork = artwork.is_some();

            div()
                .id(id)
                .relative()
                .flex()
                .w_full()
                .min_w_0()
                .items_center()
                .justify_between()
                .px_3()
                .py_1()
                .rounded(theme.radius)
                .when_else(
                    disabled,
                    |this| this.text_color(theme.muted_foreground).cursor_default(),
                    |this| this.cursor_pointer(),
                )
                .when(selected, |this| this.bg(theme.secondary_active))
                .when(!disabled, |this| {
                    this.hover(move |this| this.bg(theme.secondary_hover))
                })
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .when_some(artwork, |this, artwork| {
                            this.child(Artwork::new(artwork).size(px(20.)).flex_none())
                        })
                        .when_some(icon.filter(|_| !has_artwork), |this, icon| {
                            this.child(svg().path(icon).size(px(14.)).flex_none().text_color(
                                if disabled {
                                    theme.muted_foreground
                                } else {
                                    theme.popover_foreground
                                },
                            ))
                        })
                        .child(div().truncate().child(label)),
                )
                .when(selected, |this| this.child("✓"))
                .when(submenu.is_some(), |this| this.child("›"))
                .when_some(submenu_state, |this, state| {
                    this.on_hover(move |hovered, window, cx| {
                        state.hover(*hovered, window.window_handle(), cx)
                    })
                })
                .when_some(item_hover_guard, |this, state| {
                    this.on_hover(move |hovered, window, cx| {
                        state.hover(*hovered, window.window_handle(), cx)
                    })
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when_some(press, |this, press| {
                    this.on_click(move |event, window, cx| {
                        press(event, window, cx);
                        if let Some(action) = press_action.as_ref() {
                            action(event, window, cx);
                        }
                    })
                })
                .when_some(submenu, |this, mut submenu| {
                    if submenu.menu.action.is_none() {
                        submenu.menu.action = action.clone();
                    }
                    let open = submenu.state.is_open();
                    let panel_state = submenu.state.clone();
                    let safe_state = submenu.state.clone();
                    let bounds_state = submenu.state.clone();
                    let flip_left = submenu.state.should_flip(viewport_width);
                    this.child(
                        div()
                            .absolute()
                            .top(SUBMENU_TOP)
                            .when(flip_left, |this| this.right_full())
                            .when(!flip_left, |this| this.left_full())
                            .when(!open, |this| this.invisible())
                            .child(
                                anchored()
                                    .anchor(if flip_left {
                                        Anchor::TopRight
                                    } else {
                                        Anchor::TopLeft
                                    })
                                    .snap_to_window_with_margin(WINDOW_MARGIN)
                                    .child(
                                        div()
                                            .on_children_prepainted(move |bounds, _, _| {
                                                if let Some(bounds) =
                                                    bounds.into_iter().reduce(|a, b| a.union(&b))
                                                {
                                                    bounds_state.observe_panel(bounds);
                                                }
                                            })
                                            .id("submenu-safe-area")
                                            .occlude()
                                            .pt_3()
                                            .pb_3()
                                            .when(flip_left, |this| this.pl_3().pr_1())
                                            .when(!flip_left, |this| this.pl_1().pr_3())
                                            .on_hover(move |hovered, window, cx| {
                                                safe_state.hover(
                                                    *hovered,
                                                    window.window_handle(),
                                                    cx,
                                                )
                                            })
                                            .child(submenu.menu.inline().relative().on_hover(
                                                move |hovered, window, cx| {
                                                    panel_state.hover(
                                                        *hovered,
                                                        window.window_handle(),
                                                        cx,
                                                    )
                                                },
                                            )),
                                    ),
                            ),
                    )
                })
                .into_any_element()
        });

        let content = match scrollbar.as_ref() {
            Some(scrollbar) => div()
                .id("menu-scroll-content")
                .flex()
                .flex_1()
                .w_full()
                .min_w_0()
                .min_h_0()
                .flex_col()
                .pr(SCROLLBAR_GUTTER)
                .overflow_y_scroll()
                .track_scroll(scrollbar.read(cx).scroll())
                .children(rows)
                .into_any_element(),
            None => div()
                .flex()
                .flex_col()
                .on_children_prepainted(move |bounds, _, _| {
                    if let Some(bounds) = bounds.into_iter().reduce(|a, b| a.union(&b)) {
                        for guard in &bounds_guards {
                            guard.observe_parent(bounds);
                        }
                    }
                })
                .children(rows)
                .into_any_element(),
        };
        let body = match scrollbar {
            Some(scrollbar) => div()
                .relative()
                .flex()
                .flex_1()
                .w_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(content)
                .child(scrollbar)
                .into_any_element(),
            None => content,
        };
        let mut menu = base
            .absolute()
            .flex()
            .flex_col()
            .p_1()
            .rounded(theme.radius)
            .border_1()
            .gap_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .text_color(theme.popover_foreground)
            .occlude()
            .when_some(dismiss, |this, dismiss| {
                this.on_mouse_down_out(move |event, window, cx| {
                    if !dismiss_guards
                        .iter()
                        .any(|guard| guard.contains(event.position))
                    {
                        dismiss(event, window, cx);
                    }
                })
            })
            .when(should_defer, |this| {
                let viewport = window.viewport_size();
                let chrome = snapped(theme.metrics.title_bar, window);
                this.child(
                    anchored().position(point(Pixels::ZERO, chrome)).child(
                        Shield::new("menu-shield")
                            .w(viewport.width)
                            .h(viewport.height - chrome),
                    ),
                )
            })
            .child(body);

        menu.style().refine(&overrides);

        if should_defer {
            deferred(menu).with_priority(priority).into_any_element()
        } else {
            menu.into_any_element()
        }
    }
}
