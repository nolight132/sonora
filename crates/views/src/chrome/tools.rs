// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Pixels, div, px};
use ui::{
    ActiveTheme as _, Button, FlagAxis, Menu, MenuItem, Mode, Popover, Popovers, RangeAxis,
    RangeScrubber, RangeState, Sort, SortAxis, Text, Toggle, eyebrow,
};

const MENU_WIDTH: Pixels = px(190.);
const FILTER_WIDTH: Pixels = px(260.);
const MENU_DROP: Pixels = px(30.);

const COLUMNS: &str = "columns";
const FILTERS: &str = "filters";
const SORTS: &str = "sorts";

#[derive(Default)]
pub(crate) struct Sliders(RefCell<Vec<(&'static str, RangeState)>>);

impl Sliders {
    fn state(&self, key: &'static str) -> RangeState {
        let mut cache = self.0.borrow_mut();
        if let Some((_, state)) = cache.iter().find(|(known, _)| *known == key) {
            return state.clone();
        }

        let state = RangeState::new(key);
        cache.push((key, state.clone()));
        state
    }
}

pub(crate) enum Sift {
    Range(&'static str, (f32, f32)),
    Flag(&'static str, bool),
    Reset,
}

pub(crate) fn columns(
    group: &Popovers,
    toggles: Vec<Toggle>,
    switch: impl Fn(&'static str, &mut App) + 'static,
) -> AnyElement {
    let switch = Rc::new(switch);

    Popover::new(COLUMNS, group.clone())
        .button(
            Button::new("columns-toggle")
                .icon("icons/columns-3.svg")
                .tooltip("tool-columns")
                .small()
                .ghost(),
        )
        .menu(
            Menu::new("columns-menu")
                .top(MENU_DROP)
                .right_0()
                .w(MENU_WIDTH)
                .items(toggles.into_iter().map(move |toggle| {
                    let key = toggle.key;
                    let switch = switch.clone();

                    MenuItem::new(key, toggle.label)
                        .selected(toggle.visible)
                        .on_click(move |_, _, cx| switch(key, cx))
                })),
        )
        .into_any_element()
}

pub(crate) fn sorts(
    group: &Popovers,
    axes: Vec<SortAxis>,
    rank: impl Fn(&'static str, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = *cx.theme();
    let sorted = axes.iter().any(|axis| axis.order.is_some());
    let rank = Rc::new(rank);

    Popover::new(SORTS, group.clone())
        .button(
            Button::new("sort-toggle")
                .icon("icons/arrow-up-down.svg")
                .tooltip("tool-sort")
                .small()
                .ghost()
                .tint(match sorted {
                    true => theme.primary,
                    false => theme.muted_foreground,
                }),
        )
        .menu(
            Menu::new("sort-menu")
                .top(MENU_DROP)
                .right_0()
                .w(MENU_WIDTH)
                .items(axes.into_iter().map(move |axis| {
                    let key = axis.key;
                    let rank = rank.clone();
                    let arrow = axis.order.map(|order| match order {
                        Sort::Ascending => "icons/chevron-up.svg",
                        Sort::Descending => "icons/chevron-down.svg",
                    });

                    MenuItem::new(key, axis.label)
                        .selected(axis.order.is_some())
                        .when_some(arrow, MenuItem::icon)
                        .on_click(move |_, _, cx| rank(key, cx))
                })),
        )
        .into_any_element()
}

pub(crate) fn filters(
    group: &Popovers,
    sliders: &Sliders,
    ranges: Vec<RangeAxis>,
    flags: Vec<FlagAxis>,
    sift: impl Fn(Sift, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = *cx.theme();
    let narrowed = ranges.iter().any(|axis| !axis.whole()) || flags.iter().any(|flag| flag.on);
    let sift = Rc::new(sift);

    let scrubbers: Vec<MenuItem> = ranges
        .iter()
        .map(|axis| {
            let key = axis.key;
            let unit = axis.unit;
            let copy = axis.clone();
            let sift = sift.clone();
            let state = sliders.state(key);

            MenuItem::new(key, axis.label.clone()).content(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .py_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(eyebrow(axis.label.clone(), cx))
                            .child(
                                div()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(format!(
                                        "{} - {}",
                                        unit.say(axis.value.0),
                                        unit.say(axis.value.1)
                                    )),
                            ),
                    )
                    .child(
                        RangeScrubber::new(&state, axis.share())
                            .stops(axis.stops())
                            .colors(theme.progress_bar, theme.muted, theme.foreground)
                            .on_change(move |share: &(f32, f32), _, cx| {
                                sift(Sift::Range(key, copy.at(*share)), cx);
                            }),
                    ),
            )
        })
        .collect();

    let switches: Vec<MenuItem> = flags
        .iter()
        .map(|flag| {
            let key = flag.key;
            let on = flag.on;
            let sift = sift.clone();

            MenuItem::new(key, flag.label.clone())
                .selected(on)
                .on_click(move |_, _, cx| sift(Sift::Flag(key, !on), cx))
        })
        .collect();

    let reset = sift.clone();

    Popover::new(FILTERS, group.clone())
        .button(
            Button::new("filters-toggle")
                .icon("icons/funnel.svg")
                .tooltip("tool-filters")
                .small()
                .ghost()
                .tint(match narrowed {
                    true => theme.primary,
                    false => theme.muted_foreground,
                }),
        )
        .menu(
            Menu::new("filters-menu")
                .top(MENU_DROP)
                .right_0()
                .w(FILTER_WIDTH)
                .items(scrubbers)
                .items(switches)
                .item(MenuItem::separator("filters-end"))
                .item(
                    MenuItem::new("filters-reset", i18n::t!("filter-reset"))
                        .on_click(move |_, _, cx| reset(Sift::Reset, cx)),
                ),
        )
        .into_any_element()
}

pub(crate) fn views(
    group: &Popovers,
    mode: Mode,
    shift: impl Fn(Mode, &mut App) + 'static,
) -> AnyElement {
    let next = match mode {
        Mode::List => Mode::Cards,
        Mode::Cards => Mode::List,
    };
    let group = group.clone();

    Button::new("view-toggle")
        .icon(next.icon())
        .tooltip(next.key())
        .small()
        .ghost()
        .on_click(move |_, _, cx| {
            group.close();
            shift(next, cx);
        })
        .into_any_element()
}
