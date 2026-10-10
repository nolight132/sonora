use std::panic::Location;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ElementId, GlobalElementId, InspectorElementId,
    LayoutId, Pixels, Style, StyleRefinement, Window, relative, size,
};

type Build = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;

/// A block inside a scrolling region that builds its content only while the content is on
/// screen. Off screen it holds the height the content had when it was last built, so the region
/// keeps its extent. Content that changed height while away is corrected with one redraw on the
/// frame it scrolls back into view.
pub struct Lazy {
    id: ElementId,
    style: StyleRefinement,
    build: Option<Build>,
}

/// What a lazy block remembers between frames: how tall its content stood and whether it was
/// on screen, which decides whether the next frame lays the content out in place.
#[derive(Clone, Copy)]
struct Seen {
    height: Pixels,
    shown: bool,
}

impl Lazy {
    #[track_caller]
    pub fn new(
        id: impl Into<ElementId>,
        build: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            build: Some(Box::new(build)),
        }
    }
}

impl Styled for Lazy {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for Lazy {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Lazy {
    type RequestLayoutState = Option<AnyElement>;
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let seen = id.and_then(|id| seen(id, window));
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.flex_shrink = 0.;
        style.refine(&self.style);

        match seen {
            Some(Seen { height, shown }) if !shown && height > Pixels::ZERO => {
                style.size.height = height.into();
                (window.request_layout(style, [], cx), None)
            }
            _ => {
                let Some(build) = self.build.take() else {
                    return (window.request_layout(style, [], cx), None);
                };
                let mut content = build(window, cx);
                let child = content.request_layout(window, cx);
                (window.request_layout(style, [child], cx), Some(content))
            }
        }
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        laid: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let shown = window.content_mask().bounds.intersects(&bounds);

        if let Some(mut content) = laid.take() {
            remember(id, bounds.size.height, shown, window);
            if !shown {
                return None;
            }
            content.prepaint(window, cx);
            return Some(content);
        }
        if !shown {
            remember(id, bounds.size.height, false, window);
            return None;
        }
        let build = self.build.take()?;

        let mut content = build(window, cx);
        let measured = content.layout_as_root(
            size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::MinContent,
            ),
            window,
            cx,
        );
        content.prepaint_at(bounds.origin, window, cx);
        remember(id, measured.height, true, window);
        if measured.height != bounds.size.height {
            window.refresh();
        }
        Some(content)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _laid: &mut Self::RequestLayoutState,
        content: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(content) = content {
            content.paint(window, cx);
        }
    }
}

fn seen(id: &GlobalElementId, window: &mut Window) -> Option<Seen> {
    window.with_element_state::<Option<Seen>, _>(id, |seen, _| {
        let seen = seen.flatten();
        (seen, seen)
    })
}

fn remember(id: Option<&GlobalElementId>, height: Pixels, shown: bool, window: &mut Window) {
    let Some(id) = id else {
        return;
    };
    window.with_element_state::<Option<Seen>, _>(id, |_, _| ((), Some(Seen { height, shown })));
}
