// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

mod text;

pub use text::Input;

use gpui::{KeyBinding, actions};
use ui::{Deselect, GRID_CONTEXT, SelectNext, SelectPrevious};

actions!(
    sonora,
    [
        Quit,
        SignOut,
        RefreshLibrary,
        TogglePlayback,
        OpenFilter,
        OpenSearch,
        OpenSettings
    ]
);

actions!(
    input,
    [
        Backspace,
        BackspaceWord,
        Delete,
        DeleteWord,
        Left,
        Right,
        WordLeft,
        WordRight,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectAll,
        Home,
        End,
        SelectHome,
        SelectEnd,
        Paste,
        Dismiss,
        Cut,
        Copy,
        Space
    ]
);

pub const WORKSPACE_CONTEXT: &str = "Workspace";
pub const INPUT_CONTEXT: &str = "Input";

pub fn bindings() -> Vec<KeyBinding> {
    let editing = Some(INPUT_CONTEXT);
    let away_from_text = format!("{WORKSPACE_CONTEXT} && !{INPUT_CONTEXT}");
    let table = Some(GRID_CONTEXT);

    vec![
        KeyBinding::new("down", SelectNext, table),
        KeyBinding::new("up", SelectPrevious, table),
        KeyBinding::new("escape", Deselect, table),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("cmd-r", RefreshLibrary, None),
        KeyBinding::new("ctrl-r", RefreshLibrary, None),
        KeyBinding::new("cmd-f", OpenFilter, None),
        KeyBinding::new("ctrl-f", OpenFilter, None),
        KeyBinding::new("shift-cmd-f", OpenSearch, None),
        KeyBinding::new("shift-ctrl-f", OpenSearch, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("space", TogglePlayback, Some(&away_from_text)),
        KeyBinding::new("escape", Dismiss, Some(WORKSPACE_CONTEXT)),
        KeyBinding::new("backspace", Backspace, editing),
        KeyBinding::new("ctrl-backspace", BackspaceWord, editing),
        KeyBinding::new("delete", Delete, editing),
        KeyBinding::new("ctrl-delete", DeleteWord, editing),
        KeyBinding::new("left", Left, editing),
        KeyBinding::new("right", Right, editing),
        KeyBinding::new("ctrl-left", WordLeft, editing),
        KeyBinding::new("ctrl-right", WordRight, editing),
        KeyBinding::new("shift-left", SelectLeft, editing),
        KeyBinding::new("shift-right", SelectRight, editing),
        KeyBinding::new("shift-ctrl-left", SelectWordLeft, editing),
        KeyBinding::new("shift-ctrl-right", SelectWordRight, editing),
        KeyBinding::new("home", Home, editing),
        KeyBinding::new("end", End, editing),
        KeyBinding::new("cmd-left", Home, editing),
        KeyBinding::new("cmd-right", End, editing),
        KeyBinding::new("shift-home", SelectHome, editing),
        KeyBinding::new("shift-end", SelectEnd, editing),
        KeyBinding::new("shift-cmd-left", SelectHome, editing),
        KeyBinding::new("shift-cmd-right", SelectEnd, editing),
        KeyBinding::new("cmd-a", SelectAll, editing),
        KeyBinding::new("ctrl-a", SelectAll, editing),
        KeyBinding::new("cmd-v", Paste, editing),
        KeyBinding::new("ctrl-v", Paste, editing),
        KeyBinding::new("cmd-c", Copy, editing),
        KeyBinding::new("ctrl-c", Copy, editing),
        KeyBinding::new("cmd-x", Cut, editing),
        KeyBinding::new("ctrl-x", Cut, editing),
        KeyBinding::new("space", Space, editing),
        KeyBinding::new("escape", Dismiss, editing),
    ]
}
