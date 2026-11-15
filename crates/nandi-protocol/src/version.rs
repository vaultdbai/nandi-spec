//! Which protocol versions a relay and its clients speak, and which ones
//! they accept from each other.
//!
//! A relay accepts a client whose [`PROTOCOL_VERSION`] is in its range
//! [`MIN_SUPPORTED_PROTOCOL`]`..=`[`PROTOCOL_VERSION`] and refuses any
//! other with the `Error` code [`crate::VERSION_MISMATCH`]. `/health`
//! carries both ends ([`crate::RelayHealth::protocol_range`]), so a
//! client can tell before it joins whether the relay will take it, and
//! which side needs updating when it won't.
//!
//! # When to change the numbers
//!
//! | Change                                               | What to do                          |
//! | ---------------------------------------------------- | ----------------------------------- |
//! | A new field with `#[serde(default)]`                 | Nothing: no bump                    |
//! | A new message type, or a field renamed or retyped    | Bump [`PROTOCOL_VERSION`]           |
//!
//! # One version back, both ways
//!
//! Each release talks to the one before it: a relay at version N accepts
//! clients at N - 1, and a client at N works with a relay at N - 1. So
//! [`MIN_SUPPORTED_PROTOCOL`] is always `PROTOCOL_VERSION - 1` (checked at
//! compile time, see [`oldest_supported`]). Each side sends the other only
//! the messages its version knows ([`crate::MessageKind::since`]); a
//! client hides what an older relay can't do yet.
//!
//! A shape can be dropped only once every version in the range has
//! stopped using it: one release after the version that replaced it.
//!
//! The window starts at version 8. Version 7 accepts only itself:
//! a version 6 client can't read `GroupChanged`, which a relay sends
//! unasked, and no relay before 7 was released.

/// The wire version this crate speaks.
///
/// - 1: versions on registration and `/health`
/// - 2: the member directory (`QueryDirectory`) and people search
///   (`FindPeople`)
/// - 3: per-instance public keys at enrollment and registration, pinned
///   by the relay (`key_mismatch`) and carried in the directory
/// - 4: every person-to-person payload sealed (`envelope` replaces
///   `input`, `output`, `error`, notes, reasons, questions and answers),
///   `PublicKeysQuery`, the envelope cap (`envelope_too_large`)
/// - 5: routing at the relay (`RouteQuery` → `RoutePlan`, see
///   [`crate::routing`])
/// - 6: the team brain at the relay. Store-and-forward (`SubmitTask` with
///   an envelope per candidate, `Deliver`/`Ack`/`InboxSync`, see
///   [`crate::mail`]), failover, timeouts and abandons at the relay
///   ([`crate::TaskEvent`]), reassignment (`Reassign`, `ReassignResult`,
///   [`crate::Delivery::Reseal`]) and calendar handovers (`DutyWatch`,
///   `DutyReport`, see [`crate::duty`]). Gone: `DelegateTask`,
///   `TaskAssigned`, `TaskCompleted`, `TaskFailed`, `Undeliverable`, the
///   relay's `Peer` to a client, `ReassignTask`, `TaskTimedOut`,
///   `DutyQuery`/`DutyStatus`, `StatusResponse`'s result and
///   `SealedTask.continues`.
/// - 7: groups (`GroupRequest` → `GroupReply`, `GroupChanged`, see
///   [`crate::group`]), the relay's [`crate::Visibility`] on `Registered`
///   and `/health`
pub const PROTOCOL_VERSION: u32 = 7;

/// The oldest client version a relay of this crate's version accepts,
/// and the oldest relay version a client of it works with:
/// [`oldest_supported`]`(`[`PROTOCOL_VERSION`]`)`.
pub const MIN_SUPPORTED_PROTOCOL: u32 = 7;

/// The first version that must talk to the one before it
const FIRST_WINDOWED_VERSION: u32 = 8;

/// The oldest version a release at `current` talks to: the one before it,
/// from version 8 on, and itself before that
pub const fn oldest_supported(current: u32) -> u32 {
    if current >= FIRST_WINDOWED_VERSION {
        current - 1
    } else {
        current
    }
}

// After a bump, MIN_SUPPORTED_PROTOCOL follows: one version back, never
// more (no legacy shapes) and never less (no release left unable to talk
// to the one before it)
const _: () = assert!(MIN_SUPPORTED_PROTOCOL == oldest_supported(PROTOCOL_VERSION));

/// The versions one side accepts from the other: `min_supported..=current`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolRange {
    /// The oldest version accepted
    pub min_supported: u32,
    /// The version spoken, and the newest accepted
    pub current: u32,
}

impl ProtocolRange {
    /// The range of this crate: [`MIN_SUPPORTED_PROTOCOL`]`..=`[`PROTOCOL_VERSION`]
    pub const OURS: Self = Self {
        min_supported: MIN_SUPPORTED_PROTOCOL,
        current: PROTOCOL_VERSION,
    };

    /// Whether a peer speaking `version` is accepted
    pub fn accepts(self, version: u32) -> bool {
        (self.min_supported..=self.current).contains(&version)
    }

    /// Which side is behind when `version` isn't accepted, or `None`
    /// when it is
    pub fn mismatch(self, version: u32) -> Option<VersionGap> {
        if version < self.min_supported {
            Some(VersionGap::PeerTooOld)
        } else if version > self.current {
            Some(VersionGap::PeerTooNew)
        } else {
            None
        }
    }
}

/// Why a version is outside a [`ProtocolRange`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionGap {
    /// The peer is older than the oldest accepted: the peer needs updating
    PeerTooOld,
    /// The peer is newer than the range's side: that side needs updating
    PeerTooNew,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_accepts_its_ends_and_nothing_outside() {
        let range = ProtocolRange {
            min_supported: 5,
            current: 7,
        };
        let cases = [
            (0, false, Some(VersionGap::PeerTooOld)),
            (4, false, Some(VersionGap::PeerTooOld)),
            (5, true, None),
            (6, true, None),
            (7, true, None),
            (8, false, Some(VersionGap::PeerTooNew)),
            (u32::MAX, false, Some(VersionGap::PeerTooNew)),
        ];
        for (version, accepted, gap) in cases {
            assert_eq!(range.accepts(version), accepted, "version {version}");
            assert_eq!(range.mismatch(version), gap, "version {version}");
        }
    }

    #[test]
    fn a_release_talks_to_the_one_before_it_from_version_8() {
        let cases = [(1, 1), (6, 6), (7, 7), (8, 7), (9, 8), (20, 19)];
        for (current, oldest) in cases {
            assert_eq!(oldest_supported(current), oldest, "version {current}");
        }
        assert_eq!(MIN_SUPPORTED_PROTOCOL, oldest_supported(PROTOCOL_VERSION));
    }

    #[test]
    fn our_range_accepts_our_own_version() {
        assert!(ProtocolRange::OURS.accepts(PROTOCOL_VERSION));
        assert!(!ProtocolRange::OURS.accepts(PROTOCOL_VERSION + 1));
        assert!(!ProtocolRange::OURS.accepts(MIN_SUPPORTED_PROTOCOL - 1));
    }
}
