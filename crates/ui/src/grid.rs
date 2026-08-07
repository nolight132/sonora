// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;

use gpui::prelude::*;
use gpui::{
    AbsoluteLength, AnyElement, App, Context, Corners, Div, Entity, EventEmitter, FocusHandle,
    Focusable, Interactivity, MouseButton, MouseDownEvent, Pixels, Point, ScrollHandle,
    SharedString, StyleRefinement, TextAlign, Window, actions, anchored, div, point, px, svg,
};

use crate::menu::Menu;
use crate::metrics::{Metrics, snapped};
use crate::theme::ActiveTheme as _;

actions!(grid, [SelectNext, SelectPrevious, Deselect]);

pub const GRID_CONTEXT: &str = "Grid";

const PADDING: Pixels = px(8.);
const TRAIL: Pixels = px(4.);
const MIN_CELL: Pixels = px(24.);
const MIN_FLEXIBLE: Pixels = px(120.);
const SLACK: Pixels = px(2.);
const OVERSCAN: usize = 2;

pub const ROW_GROUP: &str = "grid-row";

#[derive(Clone, Copy)]
pub enum Width {
    Fixed(Pixels),
    Fill(f32),
    Thumb,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Sort {
    Ascending,
    Descending,
}

pub struct ColumnSpec<F: 'static> {
    pub field: F,
    pub key: &'static str,
    pub header: &'static str,
    pub align: TextAlign,
    pub width: Width,
    pub flush: bool,
    pub sortable: bool,
    pub hide_below: Pixels,
}

impl<F: 'static> ColumnSpec<F> {
    pub fn label(&self) -> SharedString {
        match self.header.is_empty() {
            true => SharedString::default(),
            false => i18n::lookup(self.header, None),
        }
    }

    fn share(&self) -> f32 {
        match self.width {
            Width::Fill(share) => share,
            _ => 0.,
        }
    }

    fn resolve(&self, flexible: Pixels, shares: f32, metrics: Metrics) -> Pixels {
        match self.width {
            Width::Fixed(width) => width,
            Width::Thumb => metrics.thumb + metrics.pad * 2.,
            Width::Fill(share) if shares > 0. => flexible * (share / shares),
            Width::Fill(_) => Pixels::ZERO,
        }
    }
}

pub struct Cell<F> {
    pub field: F,
    pub width: Pixels,
    pub align: TextAlign,
    pub display: usize,
    pub row: usize,
}

impl<F> Cell<F> {
    pub fn frame(&self) -> Div {
        frame(self.width, self.align)
    }

    pub fn middle(&self) -> Div {
        div().w(self.width).h_full().flex().items_center()
    }
}

pub trait GridSource: 'static {
    type Field: Copy + PartialEq + 'static;

    fn columns(&self) -> &'static [ColumnSpec<Self::Field>];
    fn rows(&self, cx: &App) -> usize;
    fn cell(&self, cell: Cell<Self::Field>, cx: &mut App) -> AnyElement;

    fn context_menu(&self, _row: usize, _cx: &App) -> Option<Menu> {
        None
    }

    fn context_menu_will_open(&self, _row: usize, _cx: &App) {}

    fn compare(&self, _field: Self::Field, a: usize, b: usize, _cx: &App) -> Ordering {
        a.cmp(&b)
    }

    fn matches(&self, _row: usize, _query: &str, _cx: &App) -> bool {
        true
    }

    fn playing(&self, _row: usize, _cx: &App) -> bool {
        false
    }

    fn is_loading(&self, _cx: &App) -> bool {
        false
    }
}

fn frame(width: Pixels, align: TextAlign) -> Div {
    let frame = div().w(width).flex_none().min_w_0();
    match align {
        TextAlign::Left => frame.truncate(),
        TextAlign::Center => frame.flex().justify_center(),
        TextAlign::Right => frame.flex().justify_end(),
    }
}

struct Resolved<F: 'static> {
    spec: &'static ColumnSpec<F>,
    width: Pixels,
}

pub struct GridDelegate<S: GridSource> {
    source: S,
    columns: Vec<Resolved<S::Field>>,
    width: Pixels,
    hidden: Vec<String>,
    selected: Option<usize>,
    sort: Option<(S::Field, Sort)>,
    filter: String,
    order: Vec<usize>,
}

impl<S: GridSource> GridDelegate<S> {
    pub fn new(source: S, width: Pixels, cx: &App) -> Self {
        let columns = build(source.columns(), width, cx.theme().metrics, &[]);
        let mut delegate = Self {
            source,
            columns,
            width,
            hidden: Vec::new(),
            selected: None,
            sort: None,
            filter: String::new(),
            order: Vec::new(),
        };
        delegate.reorder(cx);
        delegate
    }

    pub fn source(&self) -> &S {
        &self.source
    }

    pub fn with_sort(mut self, field: S::Field, direction: Sort, cx: &App) -> Self {
        self.sort = Some((field, direction));
        self.reorder(cx);
        self
    }

    pub fn set_width(&mut self, width: Pixels, cx: &App) {
        self.width = width;
        self.relayout(cx);
    }

    fn rebuild(&mut self, cx: &App) {
        self.relayout(cx);
        self.reorder(cx);
    }

    pub fn row(&self, display: usize) -> usize {
        self.order.get(display).copied().unwrap_or(display)
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    fn display_of(&self, row: usize) -> Option<usize> {
        self.order.iter().position(|&candidate| candidate == row)
    }

    pub fn row_count(&self) -> usize {
        self.order.len()
    }

    fn relayout(&mut self, cx: &App) {
        self.columns = build(
            self.source.columns(),
            self.width,
            cx.theme().metrics,
            &self.hidden,
        );
    }

    pub fn hidden(&self) -> &[String] {
        &self.hidden
    }

    pub fn set_hidden(&mut self, hidden: Vec<String>, cx: &App) {
        self.hidden = hidden;
        self.relayout(cx);
    }

    pub fn toggles(&self) -> Vec<Toggle> {
        self.source
            .columns()
            .iter()
            .map(|spec| Toggle {
                key: spec.key,
                label: spec.label(),
                visible: !self.hidden.iter().any(|hidden| hidden == spec.key),
            })
            .collect()
    }

    pub fn set_filter(&mut self, query: &str, cx: &App) {
        self.filter = query.trim().to_lowercase();
        self.reorder(cx);
    }

    fn reorder(&mut self, cx: &App) {
        let mut order: Vec<usize> = (0..self.source.rows(cx))
            .filter(|row| self.filter.is_empty() || self.source.matches(*row, &self.filter, cx))
            .collect();

        if let Some((field, direction)) = self.sort {
            match direction {
                Sort::Ascending => order.sort_by(|&a, &b| self.source.compare(field, a, b, cx)),
                Sort::Descending => order.sort_by(|&a, &b| self.source.compare(field, b, a, cx)),
            }
        }

        self.order = order;
    }

    fn inner_width(&self, col_ix: usize) -> Pixels {
        let trailing = col_ix + 1 == self.columns.len();
        let gutter = if trailing { TRAIL } else { Pixels::ZERO };
        (self.columns[col_ix].width - PADDING * 2. - gutter).max(MIN_CELL)
    }

    fn direction(&self, field: S::Field) -> Option<Sort> {
        self.sort
            .filter(|(sorted, _)| *sorted == field)
            .map(|(_, direction)| direction)
    }
}

fn build<F: Copy + PartialEq + 'static>(
    specs: &'static [ColumnSpec<F>],
    room: Pixels,
    metrics: Metrics,
    hidden: &[String],
) -> Vec<Resolved<F>> {
    let available = (room - SLACK).max(MIN_FLEXIBLE);
    let mut visible: Vec<_> = specs
        .iter()
        .filter(|spec| available >= spec.hide_below)
        .filter(|spec| !hidden.iter().any(|key| key == spec.key))
        .collect();
    if visible.is_empty() {
        visible.extend(specs.iter().take(1));
    }

    let fixed = visible
        .iter()
        .map(|spec| spec.resolve(Pixels::ZERO, 0., metrics))
        .fold(Pixels::ZERO, |total, width| total + width);
    let shares: f32 = visible.iter().map(|spec| spec.share()).sum();
    let flexible = (available - fixed).max(MIN_FLEXIBLE);

    let mut columns: Vec<Resolved<F>> = visible
        .iter()
        .map(|spec| Resolved {
            spec,
            width: spec.resolve(flexible, shares, metrics),
        })
        .collect();

    let total = columns
        .iter()
        .map(|column| column.width)
        .fold(Pixels::ZERO, |total, width| total + width);
    let leftover = available - total;
    let stretchy = visible
        .iter()
        .rposition(|spec| spec.share() > 0.)
        .unwrap_or(visible.len().saturating_sub(1));
    if leftover > Pixels::ZERO {
        if let Some(column) = columns.get_mut(stretchy) {
            column.width += leftover;
        }
    }

    columns
}

pub enum GridEvent {
    DoubleClicked(usize),
}

pub struct Toggle {
    pub key: &'static str,
    pub label: SharedString,
    pub visible: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Viewport {
    pub top: Pixels,
    pub height: Pixels,
}

impl Viewport {
    fn rows(&self, row: Pixels) -> usize {
        (self.height / row).ceil().max(0.) as usize + OVERSCAN
    }

    fn first(&self, head: Pixels, row: Pixels) -> usize {
        ((self.top - head) / row).floor().max(0.) as usize
    }
}

pub struct GridState<S: GridSource> {
    delegate: GridDelegate<S>,
    viewport: Viewport,
    corners: Corners<Pixels>,
    focus: FocusHandle,
    scroll: Option<ScrollHandle>,
    context_menu: Option<(usize, Point<Pixels>)>,
}

impl<S: GridSource> EventEmitter<GridEvent> for GridState<S> {}

impl<S: GridSource> Focusable for GridState<S> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl<S: GridSource> GridState<S> {
    pub fn new(delegate: GridDelegate<S>, cx: &mut Context<Self>) -> Self {
        Self {
            delegate,
            viewport: Viewport::default(),
            corners: Corners::default(),
            focus: cx.focus_handle(),
            scroll: None,
            context_menu: None,
        }
    }

    pub fn follow(mut self, scroll: ScrollHandle) -> Self {
        self.scroll = Some(scroll);
        self
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.step(1, window, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, window, cx);
    }

    fn deselect(&mut self, _: &Deselect, _: &mut Window, cx: &mut Context<Self>) {
        if self.delegate.selected.is_none() {
            return;
        }
        self.delegate.selected = None;
        cx.notify();
    }

    fn step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.delegate.row_count();
        if count == 0 {
            return;
        }

        let display = match self
            .delegate
            .selected
            .and_then(|row| self.delegate.display_of(row))
        {
            Some(current) => current.saturating_add_signed(delta).min(count - 1),
            None if delta < 0 => count - 1,
            None => 0,
        };

        self.delegate.selected = Some(self.delegate.row(display));
        self.reveal(display, window, cx);
        cx.notify();
    }

    fn reveal(&self, display: usize, window: &Window, cx: &App) {
        let Some(scroll) = &self.scroll else {
            return;
        };

        let metrics = cx.theme().metrics;
        let row = snapped(metrics.row, window);
        let head = snapped(metrics.header, window);
        let top = head + row * display as f32;
        let above = self.viewport.top + head;
        let below = self.viewport.top + self.viewport.height;

        let delta = if top < above {
            top - above
        } else if top + row > below {
            top + row - below
        } else {
            return;
        };

        let offset = scroll.offset();
        scroll.set_offset(point(offset.x, offset.y - delta));
    }

    fn height(&self, head: Pixels, row: Pixels) -> Pixels {
        head + row * self.delegate.row_count() as f32
    }

    pub fn delegate(&self) -> &GridDelegate<S> {
        &self.delegate
    }

    pub fn delegate_mut(&mut self) -> &mut GridDelegate<S> {
        &mut self.delegate
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.delegate.rebuild(cx);
        cx.notify();
    }

    fn toggle_sort(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        let Some(column) = self.delegate.columns.get(col_ix) else {
            return;
        };
        if !column.spec.sortable {
            return;
        }

        let field = column.spec.field;
        self.delegate.sort = match self.delegate.direction(field) {
            None => Some((field, Sort::Ascending)),
            Some(Sort::Ascending) => Some((field, Sort::Descending)),
            Some(Sort::Descending) => None,
        };
        self.delegate.rebuild(cx);
        cx.notify();
    }

    fn header(&self, head: Pixels, top: Corners<Pixels>, cx: &mut Context<Self>) -> Div {
        let theme = *cx.theme();
        let heads: Vec<_> = self
            .delegate
            .columns
            .iter()
            .enumerate()
            .map(|(ix, column)| {
                (
                    ix,
                    column.width,
                    self.delegate.inner_width(ix),
                    column.spec.align,
                    column.spec.label(),
                    column.spec.sortable,
                    self.delegate.direction(column.spec.field),
                )
            })
            .collect();

        div()
            .flex()
            .flex_none()
            .h(head)
            .bg(theme.table_head)
            .rounded_tl(top.top_left)
            .rounded_tr(top.top_right)
            .border_b_1()
            .border_color(theme.table_row_border)
            .text_color(theme.table_head_foreground)
            .children(heads.into_iter().map(
                |(ix, width, inner, align, header, sortable, direction)| {
                    div()
                        .id(("head", ix))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap_1()
                        .w(width)
                        .h_full()
                        .px(PADDING)
                        .when(sortable, |this| {
                            this.cursor_pointer()
                                .hover(move |style| style.text_color(theme.foreground))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                        this.toggle_sort(ix, cx)
                                    }),
                                )
                        })
                        .child(frame(inner - sort_room(sortable), align).child(header))
                        .when(sortable, |this| {
                            this.child(
                                svg()
                                    .path(sort_icon(direction))
                                    .size(px(12.))
                                    .flex_none()
                                    .text_color(match direction {
                                        Some(_) => theme.foreground,
                                        None => theme.table_head_foreground,
                                    }),
                            )
                        })
                },
            ))
    }

    fn rows(&self, head: Pixels, row_height: Pixels, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = *cx.theme();
        let count = self.delegate.order.len();
        let first = self.viewport.first(head, row_height);
        let last = (first + self.viewport.rows(row_height)).min(count);
        let bottom = self.corners.bottom_left.max(self.corners.bottom_right);

        (first..last)
            .map(|display| {
                let row = self.delegate.row(display);
                let tail = display + 1 == count;
                let selected = self.delegate.selected == Some(row);
                let playing = self.delegate.source.playing(row, cx);
                let cells: Vec<AnyElement> = (0..self.delegate.columns.len())
                    .map(|ix| {
                        let column = &self.delegate.columns[ix];
                        let cell = Cell {
                            field: column.spec.field,
                            width: self.delegate.inner_width(ix),
                            align: column.spec.align,
                            display,
                            row,
                        };
                        let width = column.width;
                        div()
                            .flex_none()
                            .w(width)
                            .h_full()
                            .px(PADDING)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .line_height(row_height)
                            .child(self.delegate.source.cell(cell, cx))
                            .into_any_element()
                    })
                    .collect();

                div()
                    .id(("row", display))
                    .group(ROW_GROUP)
                    .absolute()
                    .top(head + row_height * display as f32)
                    .left_0()
                    .w_full()
                    .flex()
                    .items_center()
                    .h(row_height)
                    .when(tail, |this| {
                        this.rounded_bl(self.corners.bottom_left)
                            .rounded_br(self.corners.bottom_right)
                    })
                    .when(!tail || bottom == Pixels::ZERO, |this| {
                        this.border_b_1().border_color(theme.table_row_border)
                    })
                    .when(playing, |this| this.bg(theme.muted))
                    .when(selected, |this| this.bg(theme.table_active))
                    .when(!selected, |this| {
                        this.hover(move |style| style.bg(theme.table_hover))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            window.focus(&this.focus.clone(), cx);
                            this.delegate.selected = Some(row);
                            if event.click_count >= 2 {
                                cx.emit(GridEvent::DoubleClicked(display));
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            if this.delegate.source.context_menu(row, cx).is_some() {
                                this.delegate.source.context_menu_will_open(row, cx);
                                window.prevent_default();
                                this.context_menu = Some((row, event.position));
                                cx.notify();
                            }
                        }),
                    )
                    .children(cells)
                    .into_any_element()
            })
            .collect()
    }
}

fn unpinned(corners: Corners<Pixels>, pinned: Pixels) -> Corners<Pixels> {
    Corners {
        top_left: (corners.top_left - pinned).max(Pixels::ZERO),
        top_right: (corners.top_right - pinned).max(Pixels::ZERO),
        ..Corners::default()
    }
}

fn radii(style: &StyleRefinement, rem: Pixels) -> Corners<Pixels> {
    let resolve = |length: Option<AbsoluteLength>| {
        length
            .map(|length| length.to_pixels(rem))
            .unwrap_or_default()
            .max(Pixels::ZERO)
    };

    Corners {
        top_left: resolve(style.corner_radii.top_left),
        top_right: resolve(style.corner_radii.top_right),
        bottom_right: resolve(style.corner_radii.bottom_right),
        bottom_left: resolve(style.corner_radii.bottom_left),
    }
}

fn sort_room(sortable: bool) -> Pixels {
    if sortable { px(16.) } else { Pixels::ZERO }
}

fn sort_icon(direction: Option<Sort>) -> &'static str {
    match direction {
        Some(Sort::Ascending) => "icons/chevron-up.svg",
        Some(Sort::Descending) => "icons/chevron-down.svg",
        None => "icons/chevrons-up-down.svg",
    }
}

impl<S: GridSource> Render for GridState<S> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let metrics = cx.theme().metrics;
        let backdrop = cx.theme().background;
        let row = snapped(metrics.row, window);
        let head = snapped(metrics.header, window);
        let height = self.height(head, row);
        let pinned = self.viewport.top.clamp(Pixels::ZERO, height - head);
        let top = unpinned(self.corners, pinned);
        let context_menu = self.context_menu.and_then(|(row, position)| {
            self.delegate.source.context_menu(row, cx).map(|menu| {
                anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        menu.on_action(cx.listener(|this, _, _, cx| {
                            this.context_menu = None;
                            cx.notify();
                        }))
                        .on_dismiss(cx.listener(|this, _, _, cx| {
                            this.context_menu = None;
                            cx.notify();
                        })),
                    )
            })
        });

        div()
            .key_context(GRID_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::deselect))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if this.delegate.selected.is_none() {
                    return;
                }
                this.delegate.selected = None;
                cx.notify();
            }))
            .relative()
            .w_full()
            .h(height)
            .children(self.rows(head, row, cx))
            .child(
                div()
                    .occlude()
                    .absolute()
                    .top(pinned)
                    .left_0()
                    .w_full()
                    .bg(backdrop)
                    .rounded_tl(top.top_left)
                    .rounded_tr(top.top_right)
                    .child(self.header(head, top, cx)),
            )
            .when_some(context_menu, |this, menu| this.child(menu))
    }
}

#[derive(IntoElement)]
pub struct Grid<S: GridSource> {
    base: Div,
    state: Entity<GridState<S>>,
}

impl<S: GridSource> Styled for Grid<S> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl<S: GridSource> InteractiveElement for Grid<S> {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl<S: GridSource> RenderOnce for Grid<S> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let corners = radii(self.base.style(), window.rem_size());
        self.state.update(cx, |state, _| state.corners = corners);

        self.base.child(self.state)
    }
}

pub fn grid<S: GridSource>(state: &Entity<GridState<S>>) -> Grid<S> {
    Grid {
        base: div(),
        state: state.clone(),
    }
}
