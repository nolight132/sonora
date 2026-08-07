// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill,
    point, px, relative, size, svg,
};

use ui::ActiveTheme as _;

use crate::{
    Backspace, BackspaceWord, Copy, Cut, Delete, DeleteWord, End, Home, INPUT_CONTEXT, Left, Paste,
    Right, SelectAll, SelectEnd, SelectHome, SelectLeft, SelectRight, SelectWordLeft,
    SelectWordRight, Space, WordLeft, WordRight,
};

const CARET: Pixels = px(2.);
const CARET_LINES: f32 = 1.25;

type Motion = fn(&str, usize) -> usize;

fn clamp_offset(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn clamp_range(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = clamp_offset(text, range.start);
    start..clamp_offset(text, range.end).max(start)
}

fn previous_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    text[offset..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| offset + index)
        .unwrap_or(text.len())
}

fn previous_word(text: &str, offset: usize) -> usize {
    let head = text[..clamp_offset(text, offset)].trim_end();
    head.char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(0)
}

fn next_word(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    let tail = &text[offset..];
    let skipped = tail.len() - tail.trim_start().len();
    let word = &tail[skipped..];
    let end = word
        .char_indices()
        .find(|(_, character)| character.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(word.len());
    offset + skipped + end
}

fn offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for character in text.chars() {
        if utf16 >= offset {
            break;
        }
        utf16 += character.len_utf16();
        utf8 += character.len_utf8();
    }
    utf8
}

fn offset_to_utf16(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    let mut utf8 = 0;
    for character in text.chars() {
        if utf8 >= offset {
            break;
        }
        utf8 += character.len_utf8();
        utf16 += character.len_utf16();
    }
    utf16
}

pub struct Input {
    focus_handle: FocusHandle,
    hint: SharedString,
    icon: Option<SharedString>,
    compact: bool,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    selecting: bool,
}

impl Input {
    pub fn new(hint: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            hint: hint.into(),
            icon: None,
            compact: false,
            content: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            selecting: false,
        }
    }

    pub fn icon(mut self, path: impl Into<SharedString>) -> Self {
        self.icon = Some(path.into());
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_hint(&mut self, hint: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.hint = hint.into();
        cx.notify();
    }

    fn placeholder(&self) -> SharedString {
        match self.hint.is_empty() {
            true => SharedString::default(),
            false => i18n::lookup(&self.hint, None),
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    fn cursor(&self) -> usize {
        match self.selection_reversed {
            true => self.selected_range.start,
            false => self.selected_range.end,
        }
    }

    fn step(&mut self, motion: Motion, backward: bool, cx: &mut Context<Self>) {
        let offset = match (self.selected_range.is_empty(), backward) {
            (true, _) => motion(&self.content, self.cursor()),
            (false, true) => self.selected_range.start,
            (false, false) => self.selected_range.end,
        };
        self.move_to(offset, cx);
    }

    fn extend(&mut self, motion: Motion, cx: &mut Context<Self>) {
        self.select_to(motion(&self.content, self.cursor()), cx);
    }

    fn erase(&mut self, motion: Motion, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.extend(motion, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.step(previous_boundary, true, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.step(next_boundary, false, cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.step(previous_word, true, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.step(next_word, false, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.extend(previous_boundary, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.extend(next_boundary, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.extend(previous_word, cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.extend(next_word, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn space(&mut self, _: &Space, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, " ", window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.erase(previous_boundary, window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.erase(next_boundary, window, cx);
    }

    fn backspace_word(&mut self, _: &BackspaceWord, window: &mut Window, cx: &mut Context<Self>) {
        self.erase(previous_word, window, cx);
    }

    fn delete_word(&mut self, _: &DeleteWord, window: &mut Window, cx: &mut Context<Self>) {
        self.erase(next_word, window, cx);
    }

    fn write_selection(&self, cx: &mut Context<Self>) -> bool {
        let range = clamp_range(&self.content, &self.selected_range);
        if range.is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(self.content[range].to_owned()));
        true
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        self.write_selection(cx);
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.write_selection(cx) {
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pasted) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let single_line = pasted.replace('\n', " ");
        self.replace_text_in_range(None, &single_line, window, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = true;
        let offset = self.offset_for(event.position);
        match event.modifiers.shift {
            true => self.select_to(offset, cx),
            false => self.move_to(offset, cx),
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.offset_for(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn offset_for(&self, position: Point<Pixels>) -> usize {
        let Some((bounds, line)) = self.last_bounds.as_ref().zip(self.last_layout.as_ref()) else {
            return 0;
        };
        if position.x < bounds.left() {
            return 0;
        }
        let offset = line
            .index_for_x(position.x - bounds.left())
            .unwrap_or(self.content.len());
        clamp_offset(&self.content, offset)
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = clamp_offset(&self.content, offset);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = clamp_offset(&self.content, offset);
        match self.selection_reversed {
            true => self.selected_range.start = offset,
            false => self.selected_range.end = offset,
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let range = clamp_range(&self.content, range);
        offset_to_utf16(&self.content, range.start)..offset_to_utf16(&self.content, range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let start = offset_from_utf16(&self.content, range.start);
        start..offset_from_utf16(&self.content, range.end).max(start)
    }

    fn edited_range(&self, range_utf16: Option<&Range<usize>>) -> Range<usize> {
        let range = range_utf16
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        clamp_range(&self.content, &range)
    }

    fn splice(&mut self, range: &Range<usize>, text: &str) {
        self.content =
            (self.content[..range.start].to_owned() + text + &self.content[range.end..]).into();
    }
}

impl Focusable for Input {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for Input {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = clamp_range(&self.content, &self.range_from_utf16(&range_utf16));
        actual.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edited_range(range_utf16.as_ref());
        self.splice(&range, text);

        let caret = range.start + text.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edited_range(range_utf16.as_ref());
        self.splice(&range, text);

        let caret = range.start + text.len();
        self.marked_range = (!text.is_empty()).then(|| range.start..caret);
        let selected = selected
            .map(|utf16| {
                range.start + offset_from_utf16(text, utf16.start)
                    ..range.start + offset_from_utf16(text, utf16.end)
            })
            .unwrap_or(caret..caret);
        self.selected_range = clamp_range(&self.content, &selected);
        self.selection_reversed = false;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = clamp_range(&self.content, &self.range_from_utf16(&range_utf16));
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(position.x - bounds.left())?;
        Some(offset_to_utf16(
            &self.content,
            clamp_offset(&self.content, index),
        ))
    }
}

struct Text {
    input: Entity<Input>,
}

struct Painted {
    line: Option<ShapedLine>,
    caret: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for Text {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Text {
    type RequestLayoutState = ();
    type PrepaintState = Painted;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let theme = *cx.theme();
        let input = self.input.read(cx);
        let empty = input.content.is_empty();
        let style = window.text_style();

        let (text, color) = match empty {
            true => (input.placeholder(), theme.muted_foreground),
            false => (input.content.clone(), style.color),
        };

        let selected = match empty {
            true => 0..0,
            false => clamp_range(&text, &input.selected_range),
        };
        let cursor = match empty {
            true => 0,
            false => clamp_offset(&text, input.cursor()),
        };
        let marked = match empty {
            true => None,
            false => input
                .marked_range
                .as_ref()
                .map(|range| clamp_range(&text, range)),
        };

        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match marked {
            Some(marked) => [
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: text.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            None => vec![run],
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(text, font_size, &runs, None);

        let (selection, caret) = match selected.is_empty() {
            true => (
                None,
                Some(fill(
                    Bounds::new(
                        point(
                            bounds.left() + line.x_for_index(cursor),
                            bounds.top() + (bounds.size.height - font_size * CARET_LINES) / 2.,
                        ),
                        size(CARET, font_size * CARET_LINES),
                    ),
                    theme.foreground,
                )),
            ),
            false => (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected.end),
                            bounds.bottom(),
                        ),
                    ),
                    theme.selection.opacity(0.4),
                )),
                None,
            ),
        };

        Painted {
            line: Some(line),
            caret,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        painted: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        if let Some(selection) = painted.selection.take() {
            window.paint_quad(selection);
        }

        let Some(line) = painted.line.take() else {
            return;
        };
        line.paint(
            bounds.origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();

        if focus.is_focused(window)
            && let Some(caret) = painted.caret.take()
        {
            window.paint_quad(caret);
        }

        self.input.update(cx, |input, _| {
            input.last_layout = (!input.content.is_empty()).then_some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for Input {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let height = match self.compact {
            true => theme.metrics.control_small,
            false => theme.metrics.field,
        };

        div()
            .flex()
            .flex_1()
            .items_center()
            .gap_2()
            .min_w_0()
            .h(height)
            .px_3()
            .rounded(theme.radius)
            .bg(theme.secondary)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .key_context(INPUT_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::backspace_word))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::space))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .when_some(self.icon.clone(), |this, path| {
                this.child(
                    svg()
                        .path(path)
                        .size_4()
                        .flex_none()
                        .text_color(theme.muted_foreground),
                )
            })
            .child(Text {
                input: cx.entity().clone(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTI: &str = "héllo wörld";

    #[test]
    fn clamps_past_the_end() {
        assert_eq!(clamp_offset("", 25), 0);
        assert_eq!(clamp_offset("abc", 25), 3);
        assert_eq!(clamp_offset("abc", 2), 2);
    }

    #[test]
    fn clamps_to_char_boundaries() {
        assert_eq!(clamp_offset(MULTI, 2), 1);
        assert_eq!(clamp_offset(MULTI, 3), 3);
        assert_eq!(clamp_offset("😀", 1), 0);
        assert_eq!(clamp_offset("😀", 3), 0);
        assert_eq!(clamp_offset("😀", 4), 4);
    }

    #[test]
    fn clamps_ranges_in_order() {
        assert_eq!(clamp_range("abc", &(9..12)), 3..3);
        assert_eq!(clamp_range("abc", &(2..1)), 2..2);
        assert_eq!(clamp_range(MULTI, &(2..7)), 1..7);
        assert_eq!(clamp_range("", &(4..25)), 0..0);
    }

    #[test]
    fn walks_char_boundaries() {
        assert_eq!(previous_boundary("", 25), 0);
        assert_eq!(next_boundary("", 25), 0);
        assert_eq!(previous_boundary(MULTI, 3), 1);
        assert_eq!(next_boundary(MULTI, 1), 3);
        assert_eq!(next_boundary("abc", 99), 3);
        assert_eq!(previous_boundary("abc", 99), 2);
    }

    #[test]
    fn walks_words_backwards() {
        assert_eq!(previous_word("", 25), 0);
        assert_eq!(previous_word("hello world", 11), 6);
        assert_eq!(previous_word("hello world", 6), 0);
        assert_eq!(previous_word("hello world   ", 14), 6);
        assert_eq!(previous_word("  ", 2), 0);
        assert_eq!(previous_word(MULTI, 13), 7);
    }

    #[test]
    fn walks_words_forwards() {
        assert_eq!(next_word("", 25), 0);
        assert_eq!(next_word("hello world", 0), 5);
        assert_eq!(next_word("hello world", 5), 11);
        assert_eq!(next_word("hello   world", 5), 13);
        assert_eq!(next_word("hello world", 99), 11);
        assert_eq!(next_word(MULTI, 0), 6);
    }

    #[test]
    fn converts_utf16_offsets() {
        assert_eq!(offset_from_utf16("😀a", 2), 4);
        assert_eq!(offset_from_utf16("😀a", 99), 5);
        assert_eq!(offset_to_utf16("😀a", 4), 2);
        assert_eq!(offset_to_utf16("😀a", 99), 3);
        assert_eq!(offset_from_utf16("", 25), 0);
        assert_eq!(offset_to_utf16("", 25), 0);
    }
}
