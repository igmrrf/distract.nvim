//! What the world did, for the Lua plugin pipeline to observe.
//!
//! Plugins run in Lua on every backend. The in-terminal engines simulate there
//! and can dispatch a hook directly; the overlay simulates in this process, so
//! the events a hook cares about — a state transition, a boundary collision —
//! have to travel back over IPC or one plugin would only work on one backend.
//!
//! Recording is off until Neovim subscribes, because nothing should go on the
//! wire per frame for a session with no plugins. The queue is bounded: a client
//! that stops reading must not turn a 60 FPS event stream into unbounded memory.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

/// The most events held while Neovim has not drained them.
///
/// Two seconds of the busiest plausible world at 60 FPS. Past that the oldest
/// are dropped: a plugin reacting to a collision cares about the current one.
const MAX_PENDING_EVENTS: usize = 256;

pub const EDGE_LEFT: &str = "left";
pub const EDGE_RIGHT: &str = "right";
pub const EDGE_TOP: &str = "top";
pub const EDGE_BOTTOM: &str = "bottom";
/// A registered obstacle rather than a screen edge.
pub const EDGE_OBSTACLE: &str = "obstacle";

/// One thing worth telling a plugin about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum WorldEvent {
    #[serde(rename = "state_change")]
    StateChange { id: usize, from: String, to: String },
    #[serde(rename = "collision")]
    Collision { id: usize, edge: String },
}

impl WorldEvent {
    pub fn collision(id: usize, edge: &str) -> Self {
        WorldEvent::Collision {
            id,
            edge: edge.to_string(),
        }
    }
}

/// A bounded queue of world events, plus the states already reported.
#[derive(Debug, Default)]
pub struct Journal {
    is_enabled: bool,
    events: VecDeque<WorldEvent>,
    reported_states: HashMap<usize, String>,
    dropped: usize,
}

impl Journal {
    pub fn set_enabled(&mut self, is_enabled: bool) {
        self.is_enabled = is_enabled;
        if !is_enabled {
            self.events.clear();
            self.reported_states.clear();
            self.dropped = 0;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// How many events were dropped to stay inside the queue bound.
    ///
    /// Reported rather than silently discarded: a plugin missing a collision
    /// should be visible to whoever is debugging it.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    pub fn record(&mut self, event: WorldEvent) {
        if !self.is_enabled {
            return;
        }
        if self.events.len() >= MAX_PENDING_EVENTS {
            self.events.pop_front();
            self.dropped += 1;
        }
        self.events.push_back(event);
    }

    pub fn record_all(&mut self, events: impl IntoIterator<Item = WorldEvent>) {
        for event in events {
            self.record(event);
        }
    }

    /// Records a transition for every entity whose state differs from the one
    /// last reported, and forgets entities that no longer exist.
    ///
    /// Diffing against the last *reported* state rather than the state at the
    /// start of the frame is what catches a transition an editor event or a
    /// triggered action made between two frames.
    pub fn sync_states<'a, I>(&mut self, entities: I)
    where
        I: IntoIterator<Item = (usize, &'a str)>,
    {
        if !self.is_enabled {
            return;
        }

        let mut live = Vec::new();
        for (id, state) in entities {
            live.push(id);
            match self.reported_states.get(&id) {
                Some(previous) if previous == state => {}
                Some(previous) => {
                    let event = WorldEvent::StateChange {
                        id,
                        from: previous.clone(),
                        to: state.to_string(),
                    };
                    self.reported_states.insert(id, state.to_string());
                    self.record(event);
                }
                None => {
                    // A spawn is not a transition. Its state is remembered so
                    // the entity's first real change is the first thing
                    // reported.
                    self.reported_states.insert(id, state.to_string());
                }
            }
        }

        self.reported_states.retain(|id, _| live.contains(id));
    }

    pub fn drain(&mut self) -> Vec<WorldEvent> {
        self.dropped = 0;
        self.events.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> Journal {
        let mut journal = Journal::default();
        journal.set_enabled(true);
        journal
    }

    #[test]
    fn nothing_is_recorded_until_someone_subscribes() {
        let mut journal = Journal::default();
        journal.record(WorldEvent::collision(1, EDGE_LEFT));
        journal.sync_states([(1usize, "walk")]);
        assert!(journal.drain().is_empty());
    }

    #[test]
    fn a_spawn_is_not_a_transition_but_the_next_change_is() {
        let mut journal = enabled();
        journal.sync_states([(1usize, "idle")]);
        assert!(journal.drain().is_empty());

        journal.sync_states([(1usize, "walk")]);
        assert_eq!(
            journal.drain(),
            vec![WorldEvent::StateChange {
                id: 1,
                from: "idle".to_string(),
                to: "walk".to_string(),
            }]
        );
    }

    #[test]
    fn an_unchanged_state_reports_nothing_however_often_it_is_synced() {
        let mut journal = enabled();
        for _ in 0..10 {
            journal.sync_states([(1usize, "idle")]);
        }
        assert!(journal.drain().is_empty());
    }

    #[test]
    fn a_despawned_entity_is_forgotten_so_its_id_can_be_reused() {
        let mut journal = enabled();
        journal.sync_states([(1usize, "idle")]);
        journal.sync_states(std::iter::empty::<(usize, &str)>());
        // Id 1 spawned again, in a different state: a fresh entity, so no
        // transition from the previous tenant's state.
        journal.sync_states([(1usize, "walk")]);
        assert!(journal.drain().is_empty());
    }

    #[test]
    fn the_queue_is_bounded_and_says_how_much_it_dropped() {
        let mut journal = enabled();
        for _ in 0..(MAX_PENDING_EVENTS + 5) {
            journal.record(WorldEvent::collision(1, EDGE_RIGHT));
        }
        assert_eq!(journal.dropped(), 5);
        assert_eq!(journal.drain().len(), MAX_PENDING_EVENTS);
        assert_eq!(journal.dropped(), 0);
    }

    #[test]
    fn disabling_forgets_everything_so_a_resubscribe_starts_clean() {
        let mut journal = enabled();
        journal.sync_states([(1usize, "idle")]);
        journal.record(WorldEvent::collision(1, EDGE_TOP));
        journal.set_enabled(false);
        journal.set_enabled(true);
        assert!(journal.drain().is_empty());
        journal.sync_states([(1usize, "idle")]);
        assert!(journal.drain().is_empty());
    }

    #[test]
    fn a_collision_names_the_edge_it_happened_on() {
        let mut journal = enabled();
        journal.record_all([
            WorldEvent::collision(3, EDGE_BOTTOM),
            WorldEvent::collision(4, EDGE_LEFT),
        ]);
        assert_eq!(
            journal.drain(),
            vec![
                WorldEvent::Collision {
                    id: 3,
                    edge: "bottom".to_string()
                },
                WorldEvent::Collision {
                    id: 4,
                    edge: "left".to_string()
                },
            ]
        );
    }
}
