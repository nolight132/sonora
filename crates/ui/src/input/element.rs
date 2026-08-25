use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementInputHandler, Entity, GlobalElementId,
    InspectorElementId, LayoutId, MouseButton, PaintQuad, Pixels, ShapedLine, Style, TextRun,
    UnderlineStyle, Window, div, fill, point, px, relative, size, svg,
};

use i18n::t;

use crate::button::Button;
use crate::input::{CARET, CARET_LINES, INPUT_CONTEXT, Input, clamp_offset, clamp_range};
use crate::theme::ActiveTheme as _;

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
            glyph_render_mode: style.glyph_render_mode,
            text_embolden: style.text_embolden,
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
            .when(self.clearable && !self.content.is_empty(), |this| {
                this.child(
                    Button::new("input-clear")
                        .icon("icons/x.svg")
                        .tooltip("common-clear")
                        .aria_label(t!("common-clear"))
                        .small()
                        .ghost()
                        .px_1()
                        .on_click(cx.listener(|this, _, window, cx| this.clear(window, cx))),
                )
            })
    }
}
