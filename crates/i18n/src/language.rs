// SPDX-License-Identifier: GPL-3.0-or-later

use unic_langid::{LanguageIdentifier, langid};

pub const AUTO: &str = "auto";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Language {
    English,
    Russian,
    Ukrainian,
    Polish,
}

impl Language {
    pub const ALL: [Self; 4] = [Self::English, Self::Russian, Self::Ukrainian, Self::Polish];

    pub fn id(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::Russian => "ru",
            Self::Ukrainian => "uk",
            Self::Polish => "pl",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::Ukrainian => "Українська",
            Self::Polish => "Polski",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|language| language.id() == id)
    }

    pub fn detect() -> Self {
        let Some(locale) = sys_locale::get_locale() else {
            return Self::English;
        };
        let primary = base(&locale);

        Self::ALL
            .into_iter()
            .find(|language| base(language.id()) == primary)
            .unwrap_or(Self::English)
    }

    pub(crate) fn tag(self) -> LanguageIdentifier {
        match self {
            Self::English => langid!("en-US"),
            Self::Russian => langid!("ru"),
            Self::Ukrainian => langid!("uk"),
            Self::Polish => langid!("pl"),
        }
    }

    pub(crate) fn source(self) -> &'static str {
        match self {
            Self::English => include_str!("../../../assets/i18n/en-US/main.ftl"),
            Self::Russian => include_str!("../../../assets/i18n/ru/main.ftl"),
            Self::Ukrainian => include_str!("../../../assets/i18n/uk/main.ftl"),
            Self::Polish => include_str!("../../../assets/i18n/pl/main.ftl"),
        }
    }
}

pub fn resolve(id: &str) -> Language {
    Language::from_id(id).unwrap_or_else(Language::detect)
}

fn base(tag: &str) -> String {
    tag.split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_lowercase()
}
