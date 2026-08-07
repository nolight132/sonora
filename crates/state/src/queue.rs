// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{HashMap, VecDeque};

use gpui::Context;
use spotify::Track;

fn scramble(upcoming: &mut VecDeque<Track>) {
    let mut tracks: Vec<Track> = upcoming.drain(..).collect();
    fastrand::shuffle(&mut tracks);
    *upcoming = tracks.into();
}

fn restore(upcoming: &mut VecDeque<Track>, source: &[Track]) {
    let mut slots: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (index, track) in source.iter().enumerate() {
        if let Some(id) = track.id.as_deref() {
            slots.entry(id).or_default().push_back(index);
        }
    }

    let mut ranked: Vec<(usize, Track)> = upcoming
        .drain(..)
        .map(|track| {
            let rank = track
                .id
                .as_deref()
                .and_then(|id| slots.get_mut(id))
                .and_then(|found| found.pop_front())
                .unwrap_or(usize::MAX);
            (rank, track)
        })
        .collect();

    ranked.sort_by_key(|(rank, _)| *rank);
    *upcoming = ranked.into_iter().map(|(_, track)| track).collect();
}

fn move_item<T>(items: &mut VecDeque<T>, from: usize, to: usize) -> bool {
    if from >= items.len() || to >= items.len() || from == to {
        return false;
    }
    let Some(item) = items.remove(from) else {
        return false;
    };
    items.insert(to, item);
    true
}

fn select_past<T>(
    past: &mut Vec<T>,
    current: &mut Option<T>,
    upcoming: &mut VecDeque<T>,
    index: usize,
) -> bool {
    if index >= past.len() {
        return false;
    }

    let mut replay = VecDeque::from(past.split_off(index));
    let selected = replay.pop_front().expect("past index was checked");
    if let Some(playing) = current.replace(selected) {
        replay.push_back(playing);
    }
    replay.append(upcoming);
    *upcoming = replay;
    true
}

fn select_upcoming<T>(
    past: &mut Vec<T>,
    current: &mut Option<T>,
    upcoming: &mut VecDeque<T>,
    index: usize,
) -> bool {
    let Some(selected) = upcoming.remove(index) else {
        return false;
    };

    past.extend(current.replace(selected));
    past.extend(upcoming.drain(..index));
    true
}

fn in_order<'a, T: PartialEq + 'a>(sequence: impl Iterator<Item = &'a T>, source: &[T]) -> bool {
    sequence.eq(source.iter())
}

fn gap_target(from: usize, gap: usize, len: usize) -> usize {
    let gap = gap.min(len);
    if gap > from { gap - 1 } else { gap }
}

pub struct Queue {
    past: Vec<Track>,
    current: Option<Track>,
    upcoming: VecDeque<Track>,
    source: Vec<Track>,
    shuffle: bool,
    revision: u64,
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

impl Queue {
    pub fn new() -> Self {
        Self {
            past: Vec::new(),
            current: None,
            upcoming: VecDeque::new(),
            source: Vec::new(),
            shuffle: false,
            revision: 0,
        }
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn set_shuffle(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.shuffle == on {
            return;
        }
        self.shuffle = on;
        match on {
            true => scramble(&mut self.upcoming),
            false => restore(&mut self.upcoming, &self.source),
        }
        self.changed(cx);
    }

    pub fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        self.set_shuffle(!self.shuffle, cx);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.revision = self.revision.wrapping_add(1);
        cx.notify();
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn past(&self) -> impl ExactSizeIterator<Item = &Track> {
        self.past.iter()
    }

    pub fn current(&self) -> Option<&Track> {
        self.current.as_ref()
    }

    pub fn upcoming(&self) -> impl ExactSizeIterator<Item = &Track> {
        self.upcoming.iter()
    }

    pub fn len(&self) -> usize {
        self.upcoming.len()
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.upcoming.is_empty()
    }

    pub fn has_next(&self) -> bool {
        !self.upcoming.is_empty()
    }

    pub fn has_previous(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn clear_upcoming(&mut self, cx: &mut Context<Self>) {
        if self.upcoming.is_empty() {
            return;
        }
        self.upcoming.clear();
        self.changed(cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.past.clear();
        self.current = None;
        self.upcoming.clear();
        self.source.clear();
        self.changed(cx);
    }

    pub fn reordered(&self) -> bool {
        let sequence = self
            .past
            .iter()
            .chain(self.current.as_ref())
            .chain(self.upcoming.iter());

        !self.source.is_empty() && !in_order(sequence, &self.source)
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) -> Option<Track> {
        if self.source.is_empty() {
            return None;
        }

        let index = self
            .current
            .as_ref()
            .and_then(|current| self.source.iter().position(|track| track == current))
            .unwrap_or_default();
        let source = self.source.clone();
        self.start(source, index, cx)
    }

    pub fn start(
        &mut self,
        tracks: Vec<Track>,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<Track> {
        if index >= tracks.len() {
            return None;
        }

        self.source = tracks.clone();
        let mut past = tracks;
        self.upcoming = past.split_off(index + 1).into();
        if self.shuffle {
            scramble(&mut self.upcoming);
        }
        self.current = past.pop();
        self.past = past;
        self.changed(cx);
        self.current.clone()
    }

    pub fn append(&mut self, track: Track, cx: &mut Context<Self>) {
        self.upcoming.push_back(track);
        self.changed(cx);
    }

    pub fn move_upcoming(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if move_item(&mut self.upcoming, from, to) {
            self.changed(cx);
        }
    }

    pub fn move_upcoming_to_gap(&mut self, from: usize, gap: usize, cx: &mut Context<Self>) {
        let to = gap_target(from, gap, self.upcoming.len());
        self.move_upcoming(from, to, cx);
    }

    pub fn remove_upcoming(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.upcoming.remove(index).is_some() {
            self.changed(cx);
        }
    }

    pub fn next(&mut self, cx: &mut Context<Self>) -> Option<Track> {
        let next = self.upcoming.pop_front()?;
        if let Some(played) = self.current.replace(next) {
            self.past.push(played);
        }
        self.changed(cx);
        self.current.clone()
    }

    pub fn rewind(&mut self, cx: &mut Context<Self>) -> Option<Track> {
        let mut tracks = std::mem::take(&mut self.past);
        tracks.extend(self.current.take());
        tracks.extend(self.upcoming.drain(..));
        self.start(tracks, 0, cx)
    }

    pub fn previous(&mut self, cx: &mut Context<Self>) -> Option<Track> {
        let previous = self.past.pop()?;
        if let Some(playing) = self.current.replace(previous) {
            self.upcoming.push_front(playing);
        }
        self.changed(cx);
        self.current.clone()
    }

    pub fn play_past(&mut self, index: usize, cx: &mut Context<Self>) -> Option<Track> {
        if !select_past(&mut self.past, &mut self.current, &mut self.upcoming, index) {
            return None;
        }
        self.changed(cx);
        self.current.clone()
    }

    pub fn play_upcoming(&mut self, index: usize, cx: &mut Context<Self>) -> Option<Track> {
        if !select_upcoming(&mut self.past, &mut self.current, &mut self.upcoming, index) {
            return None;
        }
        self.changed(cx);
        self.current.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use spotify::Track;

    use super::{gap_target, in_order, move_item, restore, scramble, select_past, select_upcoming};

    fn track(id: &str) -> Track {
        Track {
            id: Some(id.to_owned()),
            name: id.to_owned(),
            playable: true,
            artists: String::new(),
            artist_refs: Vec::new(),
            album: String::new(),
            album_id: None,
            cover: None,
            duration: Duration::from_secs(180),
            added_at: None,
            playcount: None,
            popularity: 0,
            explicit: false,
            track_number: 0,
            disc_number: 0,
            tags: Vec::new(),
            languages: Vec::new(),
            credits: Vec::new(),
        }
    }

    fn ids(tracks: &VecDeque<Track>) -> Vec<String> {
        tracks
            .iter()
            .map(|track| track.name.clone())
            .collect::<Vec<_>>()
    }

    fn listing(count: usize) -> Vec<Track> {
        (0..count).map(|index| track(&index.to_string())).collect()
    }

    #[test]
    fn scrambling_keeps_every_track() {
        let source = listing(20);
        let mut upcoming: VecDeque<Track> = source.iter().cloned().collect();

        scramble(&mut upcoming);

        let mut seen = ids(&upcoming);
        let mut expected = ids(&source.iter().cloned().collect());
        seen.sort();
        expected.sort();
        assert_eq!(seen, expected);
    }

    #[test]
    fn restoring_returns_the_source_order() {
        let source = listing(20);
        let mut upcoming: VecDeque<Track> = source.iter().cloned().collect();

        scramble(&mut upcoming);
        restore(&mut upcoming, &source);

        assert_eq!(ids(&upcoming), ids(&source.iter().cloned().collect()));
    }

    #[test]
    fn restoring_puts_unknown_tracks_last() {
        let source = vec![track("a"), track("b"), track("c")];
        let mut upcoming = VecDeque::from(vec![
            track("queued-one"),
            track("c"),
            track("queued-two"),
            track("a"),
        ]);

        restore(&mut upcoming, &source);

        assert_eq!(ids(&upcoming), ["a", "c", "queued-one", "queued-two"]);
    }

    #[test]
    fn restoring_keeps_both_copies_of_a_repeated_track() {
        let source = vec![track("a"), track("b"), track("a")];
        let mut upcoming = VecDeque::from(vec![track("a"), track("a"), track("b")]);

        restore(&mut upcoming, &source);

        assert_eq!(ids(&upcoming), ["a", "b", "a"]);
    }

    #[test]
    fn spots_a_reordered_queue() {
        let source = [1, 2, 3, 4];

        assert!(in_order([1, 2, 3, 4].iter(), &source));
        assert!(!in_order([1, 3, 2, 4].iter(), &source));
        assert!(!in_order([1, 2, 3].iter(), &source));
        assert!(!in_order([1, 2, 3, 4, 5].iter(), &source));
    }

    #[test]
    fn moves_items_in_both_directions() {
        let mut items = VecDeque::from([1, 2, 3, 4]);

        assert!(move_item(&mut items, 0, 2));
        assert_eq!(items, [2, 3, 1, 4]);
        assert!(move_item(&mut items, 3, 1));
        assert_eq!(items, [2, 4, 3, 1]);
    }

    #[test]
    fn ignores_invalid_moves() {
        let mut items = VecDeque::from([1, 2, 3]);

        assert!(!move_item(&mut items, 1, 1));
        assert!(!move_item(&mut items, 3, 0));
        assert!(!move_item(&mut items, 0, 3));
        assert_eq!(items, [1, 2, 3]);
    }

    #[test]
    fn selecting_past_rebuilds_the_queue_from_that_track() {
        let mut past = vec![1, 2, 3];
        let mut current = Some(4);
        let mut upcoming = VecDeque::from([5, 6]);

        assert!(select_past(&mut past, &mut current, &mut upcoming, 1));
        assert_eq!(past, [1]);
        assert_eq!(current, Some(2));
        assert_eq!(upcoming, [3, 4, 5, 6]);
    }

    #[test]
    fn selecting_invalid_past_track_keeps_the_queue() {
        let mut past = vec![1, 2];
        let mut current = Some(3);
        let mut upcoming = VecDeque::from([4, 5]);

        assert!(!select_past(&mut past, &mut current, &mut upcoming, 2));
        assert_eq!(past, [1, 2]);
        assert_eq!(current, Some(3));
        assert_eq!(upcoming, [4, 5]);
    }

    #[test]
    fn selecting_upcoming_moves_skipped_tracks_to_the_past() {
        let mut past = vec![1];
        let mut current = Some(2);
        let mut upcoming = VecDeque::from([3, 4, 5]);

        assert!(select_upcoming(&mut past, &mut current, &mut upcoming, 1));
        assert_eq!(past, [1, 2, 3]);
        assert_eq!(current, Some(4));
        assert_eq!(upcoming, [5]);
    }

    #[test]
    fn selecting_invalid_upcoming_track_keeps_the_queue() {
        let mut past = vec![1];
        let mut current = Some(2);
        let mut upcoming = VecDeque::from([3, 4]);

        assert!(!select_upcoming(&mut past, &mut current, &mut upcoming, 2));
        assert_eq!(past, [1]);
        assert_eq!(current, Some(2));
        assert_eq!(upcoming, [3, 4]);
    }

    #[test]
    fn converts_gaps_to_insertion_indices() {
        assert_eq!(gap_target(0, 3, 4), 2);
        assert_eq!(gap_target(0, 4, 4), 3);
        assert_eq!(gap_target(3, 1, 4), 1);
        assert_eq!(gap_target(2, 0, 4), 0);
        assert_eq!(gap_target(1, 1, 4), 1);
        assert_eq!(gap_target(1, 2, 4), 1);
        assert_eq!(gap_target(0, 10, 4), 3);
    }

    #[test]
    fn gap_moves_match_visual_positions() {
        let mut items = VecDeque::from([1, 2, 3, 4]);

        let to = gap_target(0, 3, items.len());
        assert!(move_item(&mut items, 0, to));
        assert_eq!(items, [2, 3, 1, 4]);

        let to = gap_target(3, 0, items.len());
        assert!(move_item(&mut items, 3, to));
        assert_eq!(items, [4, 2, 3, 1]);

        let to = gap_target(1, 2, items.len());
        assert!(!move_item(&mut items, 1, to));
        assert_eq!(items, [4, 2, 3, 1]);
    }
}
