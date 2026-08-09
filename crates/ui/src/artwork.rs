// SPDX-License-Identifier: GPL-3.0-or-later

use crate::skeleton::Skeleton;
use crate::theme::ActiveTheme as _;
use gpui::prelude::*;
use gpui::{
    App, Div, Entity, Global, Hsla, ImageCache, ImageCacheError, ImgResourceLoader, Interactivity,
    ObjectFit, Pixels, RenderImage, Resource, SharedString, SharedUri, StyleRefinement, Styled,
    Window, div, img, px, svg,
};
use std::{collections::HashMap, sync::Arc};

const FALLBACK_ICON: &str = "icons/music.svg";
const ROUNDED: Pixels = px(4.);
const CACHE_BYTES: usize = 128 * 1024 * 1024;
const CACHE_ITEMS: usize = 512;

struct Cached {
    value: Result<Arc<RenderImage>, ImageCacheError>,
    bytes: usize,
    used: u64,
}

struct ArtworkCache {
    items: HashMap<Resource, Cached>,
    bytes: usize,
    clock: u64,
}

struct Installed(Entity<ArtworkCache>);

impl Global for Installed {}

impl ArtworkCache {
    fn entity(cx: &mut App) -> Entity<Self> {
        if cx.try_global::<Installed>().is_none() {
            let cache = cx.new(|_| Self {
                items: HashMap::new(),
                bytes: 0,
                clock: 0,
            });
            cx.set_global(Installed(cache));
        }
        cx.global::<Installed>().0.clone()
    }

    fn insert(
        &mut self,
        resource: Resource,
        value: Result<Arc<RenderImage>, ImageCacheError>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let bytes = value.as_ref().map_or(0, |image| image_bytes(image));
        self.bytes = self.bytes.saturating_add(bytes);
        self.items.insert(
            resource,
            Cached {
                value,
                bytes,
                used: self.clock,
            },
        );

        while (self.bytes > CACHE_BYTES || self.items.len() > CACHE_ITEMS) && self.items.len() > 1 {
            let Some(resource) = self
                .items
                .iter()
                .min_by_key(|(_, cached)| cached.used)
                .map(|(resource, _)| resource.clone())
            else {
                break;
            };
            let Some(cached) = self.items.remove(&resource) else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(cached.bytes);
            cx.remove_asset::<ImgResourceLoader>(&resource);
            if let Ok(image) = cached.value {
                cx.drop_image(image, Some(window));
            }
        }
    }
}

impl ImageCache for ArtworkCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.clock = self.clock.saturating_add(1);
        if let Some(cached) = self.items.get_mut(resource) {
            cached.used = self.clock;
            return Some(cached.value.clone());
        }

        let value = window.use_asset::<ImgResourceLoader>(resource, cx)?;
        self.insert(resource.clone(), value.clone(), window, cx);
        Some(value)
    }
}

fn image_bytes(image: &RenderImage) -> usize {
    (0..image.frame_count())
        .filter_map(|frame| image.as_bytes(frame))
        .fold(0, |bytes, frame| bytes.saturating_add(frame.len()))
}

#[derive(IntoElement)]
pub struct Avatar {
    url: Option<SharedString>,
    size: Pixels,
}

impl Avatar {
    #[track_caller]
    pub fn new(url: Option<impl Into<SharedString>>) -> Self {
        Self {
            url: url.map(Into::into),
            size: px(28.),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Avatar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Artwork::new(self.url).size(self.size).circle().flex_none()
    }
}

#[derive(IntoElement)]
pub struct Artwork {
    url: Option<SharedString>,
    size: Pixels,
    circle: bool,
    radius: Option<Pixels>,
    fallback: SharedString,
    interactivity: Interactivity,
}

impl Artwork {
    #[track_caller]
    pub fn new(url: Option<impl Into<SharedString>>) -> Self {
        Self {
            url: url.map(Into::into),
            size: px(28.),
            circle: false,
            radius: None,
            fallback: FALLBACK_ICON.into(),
            interactivity: Interactivity::new(),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn circle(mut self) -> Self {
        self.circle = true;
        self
    }

    pub fn corner_radius(mut self, radius: Pixels) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn fallback(mut self, icon: impl Into<SharedString>) -> Self {
        self.fallback = icon.into();
        self
    }
}

impl Styled for Artwork {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.interactivity.base_style
    }
}

impl InteractiveElement for Artwork {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}

impl RenderOnce for Artwork {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            url,
            size,
            circle,
            radius,
            fallback,
            interactivity,
        } = self;
        let muted = cx.theme().muted_foreground;
        let rounded = match (circle, radius) {
            (true, _) => size / 2.,
            (false, Some(radius)) => radius,
            (false, None) => cx.theme().radius.min(ROUNDED),
        };
        let placeholder = {
            let fallback = fallback.clone();
            move || blank(size, rounded, muted, fallback.clone()).into_any_element()
        };

        match url {
            Some(url) => {
                let cache = ArtworkCache::entity(cx);
                refined(
                    img(SharedUri::from(url))
                        .image_cache(&cache)
                        .size(size)
                        .object_fit(ObjectFit::Cover)
                        .rounded(rounded)
                        .with_loading(move || {
                            Skeleton::new()
                                .size(size)
                                .rounded(rounded)
                                .into_any_element()
                        })
                        .with_fallback(placeholder),
                    interactivity,
                )
                .into_any_element()
            }
            None => {
                refined(blank(size, rounded, muted, fallback), interactivity).into_any_element()
            }
        }
    }
}

fn refined<T: Styled + InteractiveElement>(mut element: T, mut caller: Interactivity) -> T {
    let mut style = std::mem::take(element.style());
    style.refine(&caller.base_style);
    *caller.base_style = style;
    *element.interactivity() = caller;
    element
}

fn blank(size: Pixels, rounded: Pixels, muted: Hsla, fallback: SharedString) -> Div {
    div()
        .size(size)
        .rounded(rounded)
        .bg(muted.opacity(0.12))
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(fallback)
                .size(size * 0.46)
                .text_color(muted.opacity(0.5)),
        )
}
