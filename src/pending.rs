//! Pending Claude Code work, per session, and what the cube should do when
//! it changes. Pure logic, no clock of its own, so it is easy to test.
//!
//! Rules from the plan:
//! - Notifications are merged: pending items are counted, never queued as
//!   separate animations.
//! - Needs you wins over Done. A new Done while something needs you only
//!   changes the count.
//! - A new Done while another Done is pending replays the Done wave at most
//!   once a minute.
//! - Done items clear themselves after the Done timeout.
//! - Going back to a session (a new prompt) or ending it clears its item.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How often a Done may replay while another Done is already pending.
pub const DONE_REPLAY_GAP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Claude is waiting for you in this session.
    NeedsYou(String),
    /// Claude finished in this session.
    Done(String),
    /// You are back in this session, or it ended.
    Clear(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Done,
    NeedsYou,
}

/// What the cube should do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    Nothing,
    /// Start (or keep) the Needs you loop.
    NeedsYou,
    /// Play the Done animation (after the Needs you loop slides away, if
    /// it is showing).
    Done,
    /// Nothing needs you any more: slide the Needs you loop away.
    StopNeedsYou,
}

#[derive(Debug, Default)]
pub struct Pending {
    items: HashMap<String, (State, Instant)>,
    last_done_played: Option<Instant>,
}

impl Pending {
    pub fn needs_you(&self) -> usize {
        self.items
            .values()
            .filter(|(s, _)| *s == State::NeedsYou)
            .count()
    }

    pub fn done(&self) -> usize {
        self.items
            .values()
            .filter(|(s, _)| *s == State::Done)
            .count()
    }

    pub fn clear_all(&mut self) -> Reaction {
        let had_needs = self.needs_you() > 0;
        self.items.clear();
        if had_needs {
            Reaction::StopNeedsYou
        } else {
            Reaction::Nothing
        }
    }

    pub fn apply(&mut self, event: Event, now: Instant) -> Reaction {
        let had_needs = self.needs_you() > 0;
        let had_done = self.done() > 0;
        match event {
            Event::NeedsYou(session) => {
                self.items.insert(session, (State::NeedsYou, now));
                if had_needs {
                    Reaction::Nothing
                } else {
                    Reaction::NeedsYou
                }
            }
            Event::Done(session) => {
                self.items.insert(session, (State::Done, now));
                if self.needs_you() > 0 {
                    return Reaction::Nothing;
                }
                let recently = self
                    .last_done_played
                    .is_some_and(|t| now.duration_since(t) < DONE_REPLAY_GAP);
                if had_done && !had_needs && recently {
                    return Reaction::Nothing;
                }
                self.last_done_played = Some(now);
                Reaction::Done
            }
            Event::Clear(session) => {
                self.items.remove(&session);
                if had_needs && self.needs_you() == 0 {
                    Reaction::StopNeedsYou
                } else {
                    Reaction::Nothing
                }
            }
        }
    }

    /// Drop Done items older than `timeout` (None: never). Returns whether
    /// anything changed.
    pub fn expire(&mut self, now: Instant, timeout: Option<Duration>) -> bool {
        let Some(timeout) = timeout else {
            return false;
        };
        let before = self.items.len();
        self.items
            .retain(|_, (s, since)| *s == State::NeedsYou || now.duration_since(*since) < timeout);
        self.items.len() != before
    }

    /// When the oldest Done item expires, for scheduling a wake-up.
    pub fn next_expiry(&self, timeout: Option<Duration>) -> Option<Instant> {
        let timeout = timeout?;
        self.items
            .values()
            .filter(|(s, _)| *s == State::Done)
            .map(|(_, since)| *since + timeout)
            .min()
    }

    /// One line for the tray menu.
    pub fn summary(&self) -> String {
        match (self.needs_you(), self.done()) {
            (0, 0) => "Nothing pending".into(),
            (n, 0) => format!("{n} waiting for you"),
            (0, d) => format!("{d} done"),
            (n, d) => format!("{n} waiting for you, {d} done"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str) -> String {
        id.to_string()
    }

    #[test]
    fn needs_you_plays_once_and_stops_when_answered() {
        let t = Instant::now();
        let mut p = Pending::default();
        assert_eq!(p.apply(Event::NeedsYou(s("a")), t), Reaction::NeedsYou);
        assert_eq!(p.apply(Event::NeedsYou(s("b")), t), Reaction::Nothing);
        assert_eq!(p.needs_you(), 2);
        assert_eq!(p.apply(Event::Clear(s("a")), t), Reaction::Nothing);
        assert_eq!(p.apply(Event::Clear(s("b")), t), Reaction::StopNeedsYou);
        assert_eq!(p.summary(), "Nothing pending");
    }

    #[test]
    fn done_while_needs_you_only_counts() {
        let t = Instant::now();
        let mut p = Pending::default();
        p.apply(Event::NeedsYou(s("a")), t);
        assert_eq!(p.apply(Event::Done(s("b")), t), Reaction::Nothing);
        assert_eq!((p.needs_you(), p.done()), (1, 1));
    }

    #[test]
    fn approving_then_finishing_turns_needs_you_into_done() {
        let t = Instant::now();
        let mut p = Pending::default();
        p.apply(Event::NeedsYou(s("a")), t);
        // permission approved in the terminal, then Claude finishes
        assert_eq!(p.apply(Event::Done(s("a")), t), Reaction::Done);
        assert_eq!((p.needs_you(), p.done()), (0, 1));
    }

    #[test]
    fn done_replays_at_most_once_a_minute() {
        let t = Instant::now();
        let mut p = Pending::default();
        assert_eq!(p.apply(Event::Done(s("a")), t), Reaction::Done);
        let soon = t + Duration::from_secs(20);
        assert_eq!(p.apply(Event::Done(s("b")), soon), Reaction::Nothing);
        let later = t + Duration::from_secs(61);
        assert_eq!(p.apply(Event::Done(s("c")), later), Reaction::Done);
        assert_eq!(p.done(), 3);
    }

    #[test]
    fn done_items_time_out_but_needs_you_stays() {
        let t = Instant::now();
        let mut p = Pending::default();
        p.apply(Event::Done(s("a")), t);
        p.apply(Event::NeedsYou(s("b")), t);
        let ten = Some(Duration::from_secs(600));
        assert_eq!(p.next_expiry(ten), Some(t + Duration::from_secs(600)));
        assert!(!p.expire(t + Duration::from_secs(599), ten));
        assert!(p.expire(t + Duration::from_secs(600), ten));
        assert_eq!((p.needs_you(), p.done()), (1, 0));
        assert!(!p.expire(t + Duration::from_secs(9999), None));
    }

    #[test]
    fn summary_reads_well() {
        let t = Instant::now();
        let mut p = Pending::default();
        assert_eq!(p.summary(), "Nothing pending");
        p.apply(Event::NeedsYou(s("a")), t);
        p.apply(Event::Done(s("b")), t);
        assert_eq!(p.summary(), "1 waiting for you, 1 done");
        assert_eq!(p.clear_all(), Reaction::StopNeedsYou);
    }
}
