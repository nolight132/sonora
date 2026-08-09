// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, Hsla, Interactivity, MouseButton, SharedString, Stateful,
    StyleRefinement, Window, div, px, svg,
};

use crate::metrics::Text;
use crate::theme::ActiveTheme as _;
use crate::tooltip::{Perch, Tooltip};

enum Variant {
    Secondary,
    Ghost,
    Outline,
    Primary,
    Danger,
}

#[derive(IntoElement)]
pub struct Button {
    base: Stateful<Div>,
    label: Option<SharedString>,
    icon: Option<SharedString>,
    variant: Variant,
    small: bool,
    disabled: bool,
    selected: bool,
    backgroundless: bool,
    hoverless: bool,
    hovered: Option<StyleRefinement>,
    pressed: Option<StyleRefinement>,
    tint: Option<Hsla>,
    tooltip: Option<(SharedString, Perch)>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Button {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            label: None,
            icon: None,
            variant: Variant::Secondary,
            small: false,
            disabled: false,
            selected: false,
            backgroundless: false,
            hoverless: false,
            hovered: None,
            pressed: None,
            tint: None,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, path: impl Into<SharedString>) -> Self {
        self.icon = Some(path.into());
        self
    }

    pub fn ghost(mut self) -> Self {
        self.variant = Variant::Ghost;
        self
    }

    pub fn outline(mut self) -> Self {
        self.variant = Variant::Outline;
        self
    }

    pub fn primary(mut self) -> Self {
        self.variant = Variant::Primary;
        self
    }

    pub fn danger(mut self) -> Self {
        self.variant = Variant::Danger;
        self
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn tint(mut self, tint: Hsla) -> Self {
        self.tint = Some(tint);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn backgroundless(mut self) -> Self {
        self.backgroundless = true;
        self
    }

    pub fn hoverless(mut self) -> Self {
        self.hoverless = true;
        self
    }

    pub fn tooltip(mut self, key: impl Into<SharedString>) -> Self {
        self.tooltip = Some((key.into(), Perch::Pointer));
        self
    }

    pub fn tooltip_above(mut self, key: impl Into<SharedString>) -> Self {
        self.tooltip = Some((key.into(), Perch::Above));
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Button {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }

    fn hover(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.hovered = Some(f(self.hovered.take().unwrap_or_default()));
        self
    }
}

impl StatefulInteractiveElement for Button {
    fn active(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.pressed = Some(f(self.pressed.take().unwrap_or_default()));
        self
    }
}

struct Palette {
    background: Option<Hsla>,
    hover: Option<Hsla>,
    active: Option<Hsla>,
    foreground: Hsla,
    border: Option<Hsla>,
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            label,
            icon,
            variant,
            small,
            disabled,
            selected,
            backgroundless,
            hoverless,
            hovered,
            pressed,
            tint,
            tooltip,
            on_click,
        } = self;

        let theme = cx.theme();
        let subtle = |border| Palette {
            background: None,
            hover: Some(theme.secondary_hover),
            active: Some(theme.secondary_active),
            foreground: theme.foreground,
            border,
        };
        let solid = |background, hover, foreground| Palette {
            background: Some(background),
            hover: Some(hover),
            active: Some(hover),
            foreground,
            border: None,
        };
        let mut palette = match variant {
            Variant::Secondary => Palette {
                background: Some(theme.secondary),
                hover: Some(theme.secondary_hover),
                active: Some(theme.secondary_active),
                foreground: theme.foreground,
                border: Some(theme.border),
            },
            Variant::Ghost => subtle(None),
            Variant::Outline => subtle(Some(theme.border)),
            Variant::Primary => solid(theme.primary, theme.primary_hover, theme.primary_foreground),
            Variant::Danger => solid(theme.danger, theme.danger_hover, theme.danger_foreground),
        };
        if backgroundless {
            palette.background = None;
            palette.hover = None;
            palette.active = None;
        }

        let selected_background = theme.secondary_active;
        let radius = theme.radius;
        let interactive = !disabled;
        let (height, padding, gap) = match small {
            true => (theme.metrics.control_small, px(8.), px(4.)),
            false => (theme.metrics.control, px(12.), px(6.)),
        };
        let (hover, active) = match interactive {
            true => (palette.hover, palette.active),
            false => (None, None),
        };
        let hovered = match hoverless {
            true => None,
            false => state_style(hover, hovered),
        };
        let pressed = state_style(active, pressed);
        let overrides = std::mem::take(base.style());

        let mut button = base
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(gap)
            .h(height)
            .px(padding)
            .rounded(radius)
            .text_color(tint.unwrap_or(palette.foreground))
            .when(small, |this| this.text_size(theme.text(Text::Label)))
            .when(disabled, |this| this.opacity(0.4))
            .when_some(palette.background, |this, background| this.bg(background))
            .when(selected && !backgroundless, |this| {
                this.bg(selected_background)
            })
            .when_some(palette.border, |this, border| {
                this.border_1().border_color(border)
            })
            .when(interactive, |this| this.cursor_pointer())
            .when_some(tooltip, |this, (key, perch)| {
                this.tooltip(Tooltip::build(key, perch))
            })
            .when_some(hovered, |this, style| this.hover(move |_| style))
            .when_some(pressed, |this, style| this.active(move |_| style))
            .when_some(icon, |this, path| {
                this.child(
                    svg()
                        .path(path)
                        .size(px(16.))
                        .flex_none()
                        .text_color(tint.unwrap_or(palette.foreground)),
                )
            })
            .when_some(label, |this, label| {
                this.child(div().min_w_0().truncate().child(label))
            })
            .when(interactive, |this| {
                this.when_some(on_click, |this, handler| {
                    this.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |event, window, cx| handler(event, window, cx))
                })
            });

        button.style().refine(&overrides);
        button
    }
}

fn state_style(
    background: Option<Hsla>,
    overrides: Option<StyleRefinement>,
) -> Option<StyleRefinement> {
    if background.is_none() && overrides.is_none() {
        return None;
    }

    let mut style = StyleRefinement::default();
    if let Some(background) = background {
        style = style.bg(background);
    }
    if let Some(overrides) = overrides {
        style.refine(&overrides);
    }
    Some(style)
}
