use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, SharedString, div, svg};
use i18n::t;
use music::{RemoteDevice, RemoteKind};
use state::{Playback, Remotes};
use ui::{ActiveTheme as _, MenuItem, Picker, Popovers, Text};

const DEVICES: &str = "devices";

pub(crate) fn devices(
    remotes: &Entity<Remotes>,
    playback: &Entity<Playback>,
    group: &Popovers,
    cx: &App,
) -> AnyElement {
    let theme = *cx.theme();
    let held = remotes.read(cx);
    let engaged = held.engaged().cloned();
    let listed: Vec<RemoteDevice> = held.devices().to_vec();

    let hosting = held.hosting();
    let here = MenuItem::new("device-here", t!("device-this"))
        .icon("icons/monitor.svg")
        .selected(engaged.is_none() && !hosting)
        .on_click({
            let remotes = remotes.clone();
            move |_, _, cx| {
                remotes.update(cx, |remotes, cx| {
                    remotes.stand_down(cx);
                    remotes.release(cx);
                })
            }
        });

    let vacant = listed
        .is_empty()
        .then(|| MenuItem::new("device-none", t!("device-none")).disabled());

    let items = listed.into_iter().map(|device| {
        let remotes = remotes.clone();
        let playback = playback.clone();
        let id = device.id.clone();
        let picked = engaged.as_ref().is_some_and(|held| held.id == device.id);

        MenuItem::new(
            SharedString::from(device.id.clone()),
            SharedString::from(device.name),
        )
        .icon(glyph(device.kind))
        .selected(picked)
        .on_click(move |_, _, cx| {
            let resume = playback.read(cx).handoff(cx);
            remotes.update(cx, |remotes, cx| remotes.engage(&id, resume, cx));
        })
    });

    Picker::icon(DEVICES, group, "icons/cast.svg")
        .tooltip("device-title")
        .width(Picker::REGULAR)
        .tint(match engaged.is_some() || hosting {
            true => theme.primary,
            false => theme.muted_foreground,
        })
        .item(here)
        .items(vacant)
        .items(items)
        .into_any_element()
}

/// The line under the track name while another device is playing, so it is obvious the
/// transport is not driving this computer.
pub(crate) fn playing_on(remotes: &Entity<Remotes>, cx: &App) -> Option<impl IntoElement> {
    let theme = *cx.theme();
    let held = remotes.read(cx);
    let label = match held.engaged() {
        Some(device) => t!("device-playing-on", name = device.name.clone()),
        None => match held.hosting() {
            true => t!("device-hosting"),
            false => return None,
        },
    };
    let size = theme.text(Text::Tiny);

    Some(
        div()
            .flex()
            .items_center()
            .gap_1()
            .text_size(size)
            .text_color(theme.primary)
            .child(
                svg()
                    .path(icons::path("icons/cast.svg"))
                    .size(size)
                    .text_color(theme.primary),
            )
            .child(label),
    )
}

pub(crate) fn available(remotes: &Entity<Remotes>, cx: &App) -> bool {
    let held = remotes.read(cx);
    held.reachable() && (!held.devices().is_empty() || held.engaged().is_some() || held.hosting())
}

fn glyph(kind: RemoteKind) -> &'static str {
    match kind {
        RemoteKind::Computer => "icons/monitor.svg",
        RemoteKind::Phone => "icons/smartphone.svg",
        RemoteKind::Speaker => "icons/speaker.svg",
        RemoteKind::Screen => "icons/tv-minimal.svg",
        RemoteKind::Car => "icons/car.svg",
    }
}
