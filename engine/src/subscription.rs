//! The cadence world snapshots go out on.
//!
//! Snapshots exist so a Lua plugin's `on_tick` hook sees the overlay's entities
//! the same way it sees the in-terminal engine's. They are *not* a frame stream:
//! the simulation runs at 60 FPS and a plugin reacting to where a sprite is does
//! not need 60 messages a second, so Neovim asks for an interval and gets one
//! snapshot per interval. Nothing is sent at all until it asks.

use std::time::{Duration, Instant};

/// The fastest cadence a client may ask for: one snapshot per simulated frame.
const MIN_INTERVAL_MS: u64 = 16;
/// The slowest. Past this the client is better off polling `GetStatus`.
const MAX_INTERVAL_MS: u64 = 5_000;

/// Whether Neovim wants snapshots, and when the last one went out.
#[derive(Debug, Default)]
pub struct Subscription {
    interval: Option<Duration>,
    last_emitted: Option<Instant>,
}

impl Subscription {
    /// Subscribes at a clamped interval, or unsubscribes on `None` or `0`.
    ///
    /// The interval is bounded rather than trusted: an unclamped `1` would ask
    /// the engine to serialise every entity a thousand times a second.
    pub fn set_interval_ms(&mut self, interval_ms: Option<u64>) {
        match interval_ms {
            None | Some(0) => {
                self.interval = None;
                self.last_emitted = None;
            }
            Some(requested) => {
                self.interval = Some(Duration::from_millis(
                    requested.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS),
                ));
            }
        }
    }

    pub fn is_subscribed(&self) -> bool {
        self.interval.is_some()
    }

    /// Seconds since the previous snapshot, if one is due at `now`.
    ///
    /// The first poll after subscribing reports the interval itself rather than
    /// the time since some unrelated earlier moment, so a plugin's first `dt` is
    /// the cadence it asked for instead of however long the session had been up.
    pub fn poll(&mut self, now: Instant) -> Option<f32> {
        let interval = self.interval?;

        let elapsed = match self.last_emitted {
            None => interval,
            Some(last) => {
                let elapsed = now.duration_since(last);
                if elapsed < interval {
                    return None;
                }
                elapsed
            }
        };

        self.last_emitted = Some(now);
        Some(elapsed.as_secs_f32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_due_until_someone_subscribes() {
        let mut subscription = Subscription::default();
        assert!(!subscription.is_subscribed());
        assert_eq!(subscription.poll(Instant::now()), None);
    }

    #[test]
    fn the_first_snapshot_is_due_immediately_and_reports_the_interval() {
        let mut subscription = Subscription::default();
        subscription.set_interval_ms(Some(100));
        assert_eq!(subscription.poll(Instant::now()), Some(0.1));
    }

    #[test]
    fn a_poll_inside_the_interval_is_not_due() {
        let start = Instant::now();
        let mut subscription = Subscription::default();
        subscription.set_interval_ms(Some(100));
        assert!(subscription.poll(start).is_some());

        assert_eq!(subscription.poll(start + Duration::from_millis(99)), None);
        assert_eq!(
            subscription.poll(start + Duration::from_millis(100)),
            Some(0.1)
        );
    }

    #[test]
    fn dt_is_the_time_actually_elapsed_not_the_interval_asked_for() {
        let start = Instant::now();
        let mut subscription = Subscription::default();
        subscription.set_interval_ms(Some(100));
        subscription.poll(start);

        // A blocked frame means the next snapshot is late; a plugin integrating
        // against `dt` has to be told the truth about how late.
        let dt = subscription
            .poll(start + Duration::from_millis(250))
            .expect("a snapshot is due");
        assert!((dt - 0.25).abs() < 1e-6, "dt was {dt}");
    }

    #[test]
    fn an_interval_is_clamped_at_both_ends() {
        let start = Instant::now();
        let mut subscription = Subscription::default();

        subscription.set_interval_ms(Some(1));
        assert_eq!(
            subscription.poll(start),
            Some(Duration::from_millis(MIN_INTERVAL_MS).as_secs_f32())
        );

        let mut slow = Subscription::default();
        slow.set_interval_ms(Some(600_000));
        assert_eq!(
            slow.poll(start),
            Some(Duration::from_millis(MAX_INTERVAL_MS).as_secs_f32())
        );
    }

    #[test]
    fn zero_and_none_both_unsubscribe() {
        let mut subscription = Subscription::default();
        subscription.set_interval_ms(Some(100));
        subscription.set_interval_ms(Some(0));
        assert!(!subscription.is_subscribed());

        subscription.set_interval_ms(Some(100));
        subscription.set_interval_ms(None);
        assert!(!subscription.is_subscribed());
        assert_eq!(subscription.poll(Instant::now()), None);
    }
}
