// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::{App, Context, Entity, Hsla, Task};
use state::{AppSettings, Playback, Sonora};
use ui::{ActiveTheme as _, Look, Theme};

pub struct Adaptive {
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    cover: Option<String>,
    task: Option<Task<()>>,
}

impl Adaptive {
    pub fn new(playback: Entity<Playback>, cx: &mut Context<Self>) -> Self {
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&playback, |this, _, cx| this.sync(cx)).detach();
        cx.observe(&settings, |this, _, cx| this.sync(cx)).detach();

        let mut adaptive = Self {
            playback,
            settings,
            cover: None,
            task: None,
        };
        adaptive.sync(cx);
        adaptive
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let cover = self
            .settings
            .read(cx)
            .adaptive_theme()
            .then(|| {
                self.playback
                    .read(cx)
                    .track()
                    .and_then(|track| track.cover.clone())
            })
            .flatten();

        if cover == self.cover {
            return;
        }
        self.cover = cover.clone();

        let Some(cover) = cover else {
            self.task = None;
            apply(None, cx);
            return;
        };

        let tint = ui::tint(cover, cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            let tint = tint.await;
            this.update(cx, |_, cx| apply(tint, cx)).ok();
        }));
    }
}

fn apply(tint: Option<Hsla>, cx: &mut App) {
    if cx.theme().tint == tint {
        return;
    }

    let settings = Sonora::global(cx).settings.clone();
    let (look, overrides) = {
        let settings = settings.read(cx);
        (
            Look {
                tint,
                ..settings.look()
            },
            settings.theme_overrides().clone(),
        )
    };
    Theme::fade(look, &overrides, cx);
}
