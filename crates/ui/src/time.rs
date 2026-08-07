// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use gpui::SharedString;

pub fn clock(value: Duration) -> SharedString {
    let total = value.as_secs();
    SharedString::from(format!("{}:{:02}", total / 60, total % 60))
}
