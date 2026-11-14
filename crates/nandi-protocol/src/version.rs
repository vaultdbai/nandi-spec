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
//! | A relay stops speaking an old shape it used to read  | Raise [`MIN_SUPPORTED_PROTOCOL`]    |
//!
//! After a bump, keep [`MIN_SUPPORTED_PROTOCOL`] where it is only when
//! the relay still reads and answers every older version in the range
//! the way that version expects. A client never receives a message type
//! it doesn't know: an older client would fail to parse it.

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

/// The oldest client version a relay of this crate's version accepts.
///
/// Equal to [`PROTOCOL_VERSION`]: version 7 added `GroupChanged`, which
/// a relay pushes to group members unasked, so a version 6 client would
/// fail to parse it.
pub const MIN_SUPPORTED_PROTOCOL: u32 = 7;

// A range that accepts nothing would refuse every client
const _: () = assert!(MIN_SUPPORTED_PROTOCOL <= PROTOCOL_VERSION);

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
    fn our_range_accepts_our_own_version() {
        assert!(ProtocolRange::OURS.accepts(PROTOCOL_VERSION));
        assert!(!ProtocolRange::OURS.accepts(PROTOCOL_VERSION + 1));
        assert!(!ProtocolRange::OURS.accepts(MIN_SUPPORTED_PROTOCOL - 1));
    }
}
