// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use gpui::SharedString;

pub fn clock(value: Duration) -> SharedString {
    let total = value.as_secs();
    let hours = total / 3600;
    let minutes = total % 3600 / 60;
    let seconds = total % 60;
    match hours {
        0 => SharedString::from(format!("{minutes}:{seconds:02}")),
        _ => SharedString::from(format!("{hours}:{minutes:02}:{seconds:02}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hours_minutes_and_seconds() {
        assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn omits_zero_hours() {
        assert_eq!(clock(Duration::from_secs(63)), "1:03");
    }

    #[test]
    fn starts_hours_at_one_hour() {
        assert_eq!(clock(Duration::from_secs(3600)), "1:00:00");
    }
}
