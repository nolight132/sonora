// SPDX-License-Identifier: GPL-3.0-or-later

use crate::skeleton::Skeleton;
use crate::theme::ActiveTheme as _;
use gpui::prelude::*;
use gpui::{
    App, Asset, Div, Hsla, ImageCacheError, ImageSource, ImgResourceLoader, Interactivity,
    ObjectFit, Pixels, RenderImage, Resource, SharedString, SharedUri, StyleRefinement, Styled,
    Window, div, img, px, svg,
};
use std::sync::Arc;

const FALLBACK_ICON: &str = "icons/music.svg";
const ROUNDED: Pixels = px(4.);

#[derive(Clone)]
enum SquareImageLoader {}

impl Asset for SquareImageLoader {
    type Source = Resource;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let (image, _) = cx.fetch_asset::<ImgResourceLoader>(&source);
        async move { image.await.map(crop_square) }
    }
}

fn crop_square(image: Arc<RenderImage>) -> Arc<RenderImage> {
    if image.frame_count() == 0
        || (0..image.frame_count()).all(|frame| {
            let size = image.size(frame);
            size.width == size.height
        })
    {
        return image;
    }

    let frames = (0..image.frame_count())
        .map(|frame| {
            let size = image.size(frame);
            let width = u32::from(size.width);
            let height = u32::from(size.height);
            let side = width.min(height);
            let left = (width - side) / 2;
            let top = (height - side) / 2;
            let source = image
                .as_bytes(frame)
                .expect("render image frame should have pixel data");
            let row_bytes = side as usize * 4;
            let mut pixels = Vec::with_capacity(row_bytes * side as usize);

            for row in 0..side {
                let start = (((top + row) * width + left) * 4) as usize;
                pixels.extend_from_slice(&source[start..start + row_bytes]);
            }

            let buffer = image::RgbaImage::from_raw(side, side, pixels)
                .expect("square crop should contain one complete image");
            image::Frame::from_parts(buffer, 0, 0, image.delay(frame))
        })
        .collect::<Vec<_>>();

    Arc::new(RenderImage::new(frames))
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
            interactivity,
        } = self;
        let muted = cx.theme().muted_foreground;
        let rounded = match (circle, radius) {
            (true, _) => size / 2.,
            (false, Some(radius)) => radius,
            (false, None) => cx.theme().radius.min(ROUNDED),
        };
        let placeholder = move || blank(size, rounded, muted).into_any_element();

        match url {
            Some(url) => {
                let resource = Resource::Uri(SharedUri::from(url.to_string()));
                let source = ImageSource::from(move |window: &mut Window, cx: &mut App| {
                    window.use_asset::<SquareImageLoader>(&resource, cx)
                });

                refined(
                    img(source)
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
            None => refined(blank(size, rounded, muted), interactivity).into_any_element(),
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

fn blank(size: Pixels, rounded: Pixels, muted: Hsla) -> Div {
    div()
        .size(size)
        .rounded(rounded)
        .bg(muted.opacity(0.12))
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(FALLBACK_ICON)
                .size(size * 0.46)
                .text_color(muted.opacity(0.5)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_images_are_center_cropped_to_square_pixels() {
        let buffer =
            image::RgbaImage::from_fn(4, 2, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let source = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));

        let cropped = crop_square(source);
        let size = cropped.size(0);
        let red_channels = cropped
            .as_bytes(0)
            .unwrap()
            .chunks_exact(4)
            .map(|pixel| pixel[0])
            .collect::<Vec<_>>();

        assert_eq!((u32::from(size.width), u32::from(size.height)), (2, 2));
        assert_eq!(red_channels, [1, 2, 1, 2]);
    }
}
