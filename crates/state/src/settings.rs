// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{Context, Task};
use serde::{Deserialize, Serialize};
use ui::{Look, Rounding, ThemeKind, ThemeOverrides};

const SAVE_DELAY: Duration = Duration::from_millis(300);
const DEFAULT_VOLUME: f32 = 0.7;
const DEFAULT_SIDEBAR_WIDTH: f32 = 220.;
const DEFAULT_QUEUE_WIDTH: f32 = 380.;
const DEFAULT_FONT_SIZE: f32 = 14.;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Values {
    version: u32,
    volume: f32,
    normalisation: bool,
    sidebar_width: f32,
    sidebar_open: bool,
    queue_width: f32,
    language: String,
    hidden_columns: HashMap<String, Vec<String>>,
    appearance: Appearance,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Appearance {
    auto_hide_sidebar: bool,
    theme: String,
    adaptive_theme: bool,
    rounding: String,
    font_size: f32,
    window_controls: bool,
    controls_on_left: bool,
    theme_overrides: ThemeOverrides,
}

impl Default for Values {
    fn default() -> Self {
        Self {
            version: 1,
            volume: DEFAULT_VOLUME,
            normalisation: true,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            sidebar_open: true,
            queue_width: DEFAULT_QUEUE_WIDTH,
            language: i18n::AUTO.to_owned(),
            hidden_columns: HashMap::new(),
            appearance: Appearance::default(),
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            auto_hide_sidebar: true,
            theme: "dark".to_owned(),
            adaptive_theme: false,
            rounding: "subtle".to_owned(),
            font_size: DEFAULT_FONT_SIZE,
            window_controls: true,
            controls_on_left: false,
            theme_overrides: ThemeOverrides::default(),
        }
    }
}

pub struct AppSettings {
    values: Values,
    path: PathBuf,
    save: Option<Task<()>>,
}

impl AppSettings {
    pub fn load() -> Self {
        let path = settings_path();
        let values = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Values>(&bytes).unwrap_or_else(|error| {
                log::warn!("settings: cannot parse {}: {error}", path.display());
                Values::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Values::default(),
            Err(error) => {
                log::warn!("settings: cannot read {}: {error}", path.display());
                Values::default()
            }
        };

        Self {
            values,
            path,
            save: None,
        }
    }

    pub fn volume(&self) -> f32 {
        self.values.volume.clamp(0., 1.)
    }

    pub fn normalisation(&self) -> bool {
        self.values.normalisation
    }

    pub fn sidebar_width(&self) -> f32 {
        self.values.sidebar_width
    }

    pub fn sidebar_open(&self) -> bool {
        self.values.sidebar_open
    }

    pub fn queue_width(&self) -> f32 {
        self.values.queue_width
    }

    pub fn language(&self) -> &str {
        &self.values.language
    }

    pub fn auto_hide_sidebar(&self) -> bool {
        self.values.appearance.auto_hide_sidebar
    }

    pub fn theme(&self) -> &str {
        &self.values.appearance.theme
    }

    pub fn adaptive_theme(&self) -> bool {
        self.values.appearance.adaptive_theme
    }

    pub fn rounding(&self) -> &str {
        &self.values.appearance.rounding
    }

    pub fn look(&self) -> Look {
        Look {
            kind: ThemeKind::from_id(self.theme()),
            rounding: Rounding::from_id(self.rounding()),
            font: self.font_size(),
            tint: None,
        }
    }

    pub fn window_controls(&self) -> bool {
        self.values.appearance.window_controls
    }

    pub fn controls_on_left(&self) -> bool {
        self.values.appearance.controls_on_left
    }

    pub fn font_size(&self) -> f32 {
        self.values
            .appearance
            .font_size
            .clamp(ui::MIN_FONT, ui::MAX_FONT)
    }

    pub fn theme_overrides(&self) -> &ThemeOverrides {
        &self.values.appearance.theme_overrides
    }

    pub fn ensure_file(&self) -> PathBuf {
        if !self.path.exists() {
            self.save_now();
        }
        self.path.clone()
    }

    pub fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.values.volume = volume.clamp(0., 1.);
        self.schedule_save(cx);
    }

    pub fn set_normalisation(&mut self, normalisation: bool, cx: &mut Context<Self>) {
        self.values.normalisation = normalisation;
        self.schedule_save(cx);
    }

    pub fn hidden_columns(&self, section: &str) -> Vec<String> {
        self.values
            .hidden_columns
            .get(section)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_hidden_columns(
        &mut self,
        section: &str,
        hidden: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.values
            .hidden_columns
            .insert(section.to_owned(), hidden);
        self.schedule_save(cx);
    }

    pub fn set_sidebar(&mut self, width: f32, open: bool, cx: &mut Context<Self>) {
        self.values.sidebar_width = width;
        self.values.sidebar_open = open;
        self.schedule_save(cx);
    }

    pub fn set_queue_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.values.queue_width = width;
        self.schedule_save(cx);
    }

    pub fn set_language(&mut self, language: impl Into<String>, cx: &mut Context<Self>) {
        self.values.language = language.into();
        i18n::set(i18n::resolve(&self.values.language));
        cx.refresh_windows();
        self.schedule_save(cx);
    }

    pub fn set_auto_hide_sidebar(&mut self, auto_hide: bool, cx: &mut Context<Self>) {
        self.values.appearance.auto_hide_sidebar = auto_hide;
        self.schedule_save(cx);
    }

    pub fn set_theme(&mut self, theme: impl Into<String>, cx: &mut Context<Self>) {
        self.values.appearance.theme = theme.into();
        self.schedule_save(cx);
    }

    pub fn set_adaptive_theme(&mut self, adaptive: bool, cx: &mut Context<Self>) {
        self.values.appearance.adaptive_theme = adaptive;
        self.schedule_save(cx);
    }

    pub fn set_rounding(&mut self, rounding: impl Into<String>, cx: &mut Context<Self>) {
        self.values.appearance.rounding = rounding.into();
        self.schedule_save(cx);
    }

    pub fn set_window_controls(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.values.appearance.window_controls = shown;
        self.schedule_save(cx);
    }

    pub fn set_controls_on_left(&mut self, left: bool, cx: &mut Context<Self>) {
        self.values.appearance.controls_on_left = left;
        self.schedule_save(cx);
    }

    pub fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        self.values.appearance.font_size = size.clamp(ui::MIN_FONT, ui::MAX_FONT);
        self.schedule_save(cx);
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            this.update(cx, |this, _| this.save_now()).ok();
        }));
    }

    fn save_now(&self) {
        let Some(parent) = self.path.parent() else {
            return;
        };
        if let Err(error) = fs::create_dir_all(parent) {
            log::error!("settings: cannot create {}: {error}", parent.display());
            return;
        }

        let bytes = match serde_json::to_vec_pretty(&self.values) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::error!("settings: cannot serialize values: {error}");
                return;
            }
        };
        if let Err(error) = fs::write(&self.path, bytes) {
            log::error!("settings: cannot write {}: {error}", self.path.display());
        }
    }
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sonora")
        .join("settings.json")
}
