//! Hybrid Logical Clock (HLC).
//!
//! Two devices' wall clocks are never perfectly in sync, so plain
//! timestamps can't be trusted to order events across devices. An HLC
//! combines a physical clock with a logical counter so that:
//!
//! - Events on the *same* device are strictly ordered by wall-clock time.
//! - Events *received* from another device always advance the local
//!   clock past whatever the remote device had seen, so causally later
//!   events never get an earlier-looking timestamp.
//!
//! This is what Milestone 5 (clipboard sync) uses to decide "is this
//! incoming update newer than what I already have?" without trusting
//! either device's wall clock in isolation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single HLC timestamp: physical time plus a logical tiebreaker.
///
/// Ordering is derived (physical time first, then logical counter), which
/// is exactly the HLC comparison rule: whichever timestamp has the later
/// physical component wins; ties are broken by the logical counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HlcTimestamp {
    pub physical_time_ms: u64,
    pub logical_time: u64,
}

impl HlcTimestamp {
    pub const ZERO: HlcTimestamp = HlcTimestamp {
        physical_time_ms: 0,
        logical_time: 0,
    };
}

fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_millis() as u64
}

/// A device's Hybrid Logical Clock. One instance per device, shared across
/// every outgoing/incoming Envelope.
pub struct HybridLogicalClock {
    // Packed as (physical_time_ms << 20) | logical_time so a single
    // AtomicU64 gives us a lock-free compare-and-swap update. 20 bits of
    // logical counter (up to ~1M ticks within the same millisecond) is
    // far more than this protocol will ever need.
    packed: AtomicU64,
}

const LOGICAL_BITS: u32 = 20;
const LOGICAL_MASK: u64 = (1 << LOGICAL_BITS) - 1;

fn pack(physical_time_ms: u64, logical_time: u64) -> u64 {
    debug_assert!(logical_time <= LOGICAL_MASK, "logical counter overflow");
    (physical_time_ms << LOGICAL_BITS) | (logical_time & LOGICAL_MASK)
}

fn unpack(packed: u64) -> HlcTimestamp {
    HlcTimestamp {
        physical_time_ms: packed >> LOGICAL_BITS,
        logical_time: packed & LOGICAL_MASK,
    }
}

impl HybridLogicalClock {
    pub fn new() -> Self {
        Self {
            packed: AtomicU64::new(pack(wall_clock_ms(), 0)),
        }
    }

    /// Advance the clock for a locally-originated event (e.g. the user
    /// just copied something) and return its timestamp.
    pub fn now(&self) -> HlcTimestamp {
        loop {
            let current = self.packed.load(Ordering::SeqCst);
            let current_ts = unpack(current);
            let wall = wall_clock_ms();

            let next_ts = if wall > current_ts.physical_time_ms {
                HlcTimestamp {
                    physical_time_ms: wall,
                    logical_time: 0,
                }
            } else {
                // Wall clock hasn't advanced (or went backwards) since the
                // last tick — stay on the same physical time and bump the
                // logical counter instead, so ordering is still strict.
                HlcTimestamp {
                    physical_time_ms: current_ts.physical_time_ms,
                    logical_time: current_ts.logical_time + 1,
                }
            };

            let next_packed = pack(next_ts.physical_time_ms, next_ts.logical_time);
            if self
                .packed
                .compare_exchange(current, next_packed, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return next_ts;
            }
            // Lost the race to a concurrent `now()`/`receive()` — retry.
        }
    }

    /// Merge in a timestamp observed from a remote device (i.e. we just
    /// received an Envelope). Returns the new local timestamp, guaranteed
    /// to be strictly greater than both the previous local timestamp and
    /// the remote one, per the standard HLC receive rule:
    /// `local = max(local, remote, wall_clock) [+1 logical if tied]`.
    pub fn receive(&self, remote: HlcTimestamp) -> HlcTimestamp {
        loop {
            let current = self.packed.load(Ordering::SeqCst);
            let current_ts = unpack(current);
            let wall = wall_clock_ms();

            let max_physical = wall
                .max(current_ts.physical_time_ms)
                .max(remote.physical_time_ms);

            let next_ts = if max_physical == current_ts.physical_time_ms
                && max_physical == remote.physical_time_ms
            {
                HlcTimestamp {
                    physical_time_ms: max_physical,
                    logical_time: current_ts.logical_time.max(remote.logical_time) + 1,
                }
            } else if max_physical == current_ts.physical_time_ms {
                HlcTimestamp {
                    physical_time_ms: max_physical,
                    logical_time: current_ts.logical_time + 1,
                }
            } else if max_physical == remote.physical_time_ms {
                HlcTimestamp {
                    physical_time_ms: max_physical,
                    logical_time: remote.logical_time + 1,
                }
            } else {
                // Wall clock alone is ahead of both.
                HlcTimestamp {
                    physical_time_ms: max_physical,
                    logical_time: 0,
                }
            };

            let next_packed = pack(next_ts.physical_time_ms, next_ts.logical_time);
            if self
                .packed
                .compare_exchange(current, next_packed, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return next_ts;
            }
        }
    }
}

impl Default for HybridLogicalClock {
    fn default() -> Self {
        Self::new()
    }
}

/// The two rules every incoming Envelope must pass before a device acts on
/// it (Milestone 5 applies this to clipboard updates; later milestones
/// reuse it for anything else that syncs state):
///
/// 1. **Loop prevention** — never apply an update that originated from
///    yourself (it would have bounced back from the peer echoing it).
/// 2. **Causal ordering** — never apply an update that's causally older
///    than (or equal to) what you already have; only strictly newer
///    updates win.
///
/// Returns `true` if the update should be applied.
pub fn should_apply_update(
    local_device_id: &str,
    remote_source_device_id: &str,
    local_last_seen: HlcTimestamp,
    remote_timestamp: HlcTimestamp,
) -> bool {
    if remote_source_device_id == local_device_id {
        return false; // loop prevention
    }
    remote_timestamp > local_last_seen // causal ordering
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_strictly_monotonic() {
        let clock = HybridLogicalClock::new();
        let mut previous = clock.now();
        for _ in 0..1000 {
            let next = clock.now();
            assert!(next > previous, "HLC must be strictly increasing");
            previous = next;
        }
    }

    #[test]
    fn receive_advances_past_a_remote_timestamp_from_the_future() {
        let clock = HybridLogicalClock::new();
        let local_before = clock.now();

        // Simulate a remote device whose physical clock is far ahead.
        let remote = HlcTimestamp {
            physical_time_ms: local_before.physical_time_ms + 10_000,
            logical_time: 0,
        };

        let merged = clock.receive(remote);
        assert!(merged > remote, "must strictly exceed the remote timestamp");
        assert!(merged > local_before);

        // The clock should now stay ahead of that remote time for
        // subsequent local events too.
        let after = clock.now();
        assert!(after > merged);
    }

    #[test]
    fn receive_advances_past_a_remote_timestamp_from_the_past() {
        let clock = HybridLogicalClock::new();
        let local_before = clock.now();

        let remote = HlcTimestamp {
            physical_time_ms: 1, // far in the past
            logical_time: 999,
        };

        let merged = clock.receive(remote);
        assert!(merged > local_before);
        assert!(merged > remote);
    }

    #[test]
    fn loop_prevention_drops_self_originated_updates() {
        let last_seen = HlcTimestamp {
            physical_time_ms: 100,
            logical_time: 0,
        };
        let newer = HlcTimestamp {
            physical_time_ms: 200,
            logical_time: 0,
        };

        // Same device id as the source -> must be rejected even though
        // the timestamp is causally newer.
        assert!(!should_apply_update("device-a", "device-a", last_seen, newer));

        // Different device, newer timestamp -> accepted.
        assert!(should_apply_update("device-a", "device-b", last_seen, newer));
    }

    #[test]
    fn causal_ordering_drops_stale_or_equal_updates() {
        let last_seen = HlcTimestamp {
            physical_time_ms: 200,
            logical_time: 5,
        };
        let older = HlcTimestamp {
            physical_time_ms: 200,
            logical_time: 3,
        };
        let equal = last_seen;
        let newer = HlcTimestamp {
            physical_time_ms: 200,
            logical_time: 6,
        };

        assert!(!should_apply_update("device-a", "device-b", last_seen, older));
        assert!(!should_apply_update("device-a", "device-b", last_seen, equal));
        assert!(should_apply_update("device-a", "device-b", last_seen, newer));
    }
}
