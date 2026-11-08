//! Groups (protocol 7): people who chose to work together on a relay.
//!
//! - Nobody is in a group by default. Every member can list the relay's
//!   groups with their names and descriptions; members, invites and the
//!   policy are shown to the group's members only.
//! - Anyone creates a group and is its first admin. Only members invite:
//!   an invite is a code, for one person (`instance_id`) or for whoever has
//!   it, limited in uses and time. Admins remove members, make other
//!   admins and change the policy; a member can leave. A group always
//!   keeps at least one admin. No shared group keys: membership is recorded
//!   per person.
//! - Every group has a policy file ([`GroupPolicy`]): Markdown with YAML
//!   frontmatter, the same format as a Nandi's own policies, saying what
//!   members may have done for them and how (forced approvals, allowed
//!   capabilities, limits). Its author, an admin, signs it with their
//!   device key; each member's Nandi checks the signature against the
//!   author's pinned key and enforces the file on work from the group. The
//!   relay checks only that the author is an admin and the version is the
//!   next one, and never parses the text.
//! - On a relay whose [`Visibility`] is [`Visibility::Groups`] (a public
//!   relay), members see and reach only people they share a group with:
//!   the directory, people search, capability queries and presence,
//!   routing, keys and requests are limited to them. On a company relay
//!   ([`Visibility::Company`], the default) everyone still sees everyone.
//!
//! Frames: `GroupRequest { requestId, op }` → `GroupReply { requestId,
//! outcome }`, and `GroupChanged` pushed live to a group's members (and to
//! someone invited) when it changes; a client lists again on reconnect.
//!
//! Offline: group calls answer "not connected"; a client may show the
//! groups and policies it last saw.

use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

/// Longest group name, in characters
pub const GROUP_NAME_MAX: usize = 80;
/// Longest group description, in characters
pub const GROUP_DESCRIPTION_MAX: usize = 500;
/// Largest policy file, in bytes
pub const GROUP_POLICY_MAX_BYTES: usize = 16 * 1024;

/// `GroupOutcome::Refused` codes
pub const GROUP_NOT_FOUND: &str = "group_not_found";
pub const GROUP_NOT_MEMBER: &str = "group_not_member";
pub const GROUP_NOT_ADMIN: &str = "group_not_admin";
pub const GROUP_INVALID: &str = "group_invalid";
pub const GROUP_INVITE_INVALID: &str = "group_invite_invalid";
pub const GROUP_POLICY_STALE: &str = "group_policy_stale";
pub const GROUP_LAST_ADMIN: &str = "group_last_admin";
pub const GROUP_LIMIT: &str = "group_limit";
/// A request to someone the asker shares no group with, on a
/// [`Visibility::Groups`] relay
pub const NO_SHARED_GROUP: &str = "no_shared_group";

/// Who sees whom on a relay
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Everyone enrolled sees everyone (a company's own relay)
    #[default]
    Company,
    /// Members see only people they share a group with (a public relay)
    Groups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Admin,
    Member,
}

/// A group as anyone on the relay sees it
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub member_count: u32,
    pub created_at: Timestamp,
    pub policy_version: u32,
    /// The asker's role; `None`: not a member
    #[serde(default)]
    pub role: Option<GroupRole>,
}

/// One member, as the group's members see them
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub instance_id: String,
    /// From their gateway card; empty when they have none
    #[serde(default)]
    pub display_name: String,
    pub role: GroupRole,
    pub joined_at: Timestamp,
}

/// A group's policy file, signed by the admin who wrote it
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupPolicy {
    /// 1 for the first; each change is the next
    pub version: u32,
    /// The file: Markdown with YAML frontmatter
    pub text: String,
    /// The admin's instance ID
    pub author: String,
    /// Ed25519 by the author's signing key over [`GroupPolicy::signed_bytes`]
    pub signature: String,
    pub updated_at: Timestamp,
}

impl GroupPolicy {
    /// What the author signs: the group, the version and the text, so a
    /// signature can't be moved to another group or replayed as another
    /// version
    pub fn signed_bytes(group_id: &str, version: u32, text: &str) -> Vec<u8> {
        format!("nandi-group-policy-v1\n{group_id}\n{version}\n{text}").into_bytes()
    }
}

/// An invite: a code for one person or for whoever has it
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupInvite {
    pub code: String,
    pub group_id: String,
    /// The group's name, for the person invited
    pub group_name: String,
    /// Only this instance may use it; `None`: whoever has the code
    #[serde(default)]
    pub instance_id: Option<String>,
    pub invited_by: String,
    pub created_at: Timestamp,
    /// `None`: until revoked
    #[serde(default)]
    pub expires_at: Option<Timestamp>,
    /// Joins left
    pub uses_left: u32,
}

/// A group as its members see it
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GroupDetail {
    pub summary: GroupSummary,
    pub members: Vec<GroupMember>,
    pub policy: GroupPolicy,
    /// Open invites
    pub invites: Vec<GroupInvite>,
}

/// What a client asks of the relay about groups
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "op", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum GroupOp {
    /// Every group on the relay, and the invites for the asker
    List,
    /// One group in full (members only)
    Get { group_id: String },
    /// A new group, the asker its first admin; `policy.version` is 1
    Create {
        name: String,
        description: String,
        policy: GroupPolicy,
    },
    /// Rename or redescribe (admins)
    Update {
        group_id: String,
        name: String,
        description: String,
    },
    /// The next policy version (admins)
    SetPolicy {
        group_id: String,
        policy: GroupPolicy,
    },
    /// An invite code (members); for one instance or anyone with it
    Invite {
        group_id: String,
        #[serde(default)]
        instance_id: Option<String>,
        max_uses: u32,
        #[serde(default)]
        expires_at: Option<Timestamp>,
    },
    /// Ends an invite (its author or an admin)
    RevokeInvite { code: String },
    /// Joins with an invite code
    Join { code: String },
    /// Leaves (the last admin must make another admin first)
    Leave { group_id: String },
    /// Takes someone out (admins)
    Remove {
        group_id: String,
        instance_id: String,
    },
    /// Makes someone an admin or a member again (admins)
    SetRole {
        group_id: String,
        instance_id: String,
        role: GroupRole,
    },
}

/// The relay's answer to a [`GroupOp`]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum GroupOutcome {
    List {
        groups: Vec<GroupSummary>,
        invites: Vec<GroupInvite>,
    },
    Group {
        group: GroupDetail,
    },
    Invite {
        invite: GroupInvite,
    },
    Done,
    Refused {
        code: String,
        message: String,
    },
}

/// What changed in a group, pushed to its members
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum GroupChange {
    Updated,
    Joined {
        instance_id: String,
    },
    Left {
        instance_id: String,
    },
    Removed {
        instance_id: String,
    },
    RoleChanged {
        instance_id: String,
        role: GroupRole,
    },
    PolicyChanged {
        version: u32,
    },
    /// Sent to the person invited too
    Invited {
        invite: GroupInvite,
    },
    InviteRevoked {
        code: String,
    },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn ops_are_tagged_snake_case_with_camel_case_fields() {
        let op = GroupOp::Invite {
            group_id: "g1".into(),
            instance_id: Some("bob-1".into()),
            max_uses: 1,
            expires_at: None,
        };
        assert_eq!(
            serde_json::to_value(&op).unwrap(),
            json!({"op": "invite", "groupId": "g1", "instanceId": "bob-1", "maxUses": 1, "expiresAt": null})
        );
        assert_eq!(
            serde_json::to_value(GroupOp::List).unwrap(),
            json!({"op": "list"})
        );
        let refused = GroupOutcome::Refused {
            code: GROUP_NOT_ADMIN.into(),
            message: "Only admins change the policy".into(),
        };
        assert_eq!(
            serde_json::to_value(&refused).unwrap()["kind"],
            json!("refused")
        );
    }

    #[test]
    fn the_signed_bytes_bind_group_and_version() {
        let a = GroupPolicy::signed_bytes("g1", 1, "text");
        assert_ne!(a, GroupPolicy::signed_bytes("g2", 1, "text"));
        assert_ne!(a, GroupPolicy::signed_bytes("g1", 2, "text"));
        assert!(a.starts_with(b"nandi-group-policy-v1\n"));
    }

    #[test]
    fn visibility_defaults_to_company() {
        assert_eq!(Visibility::default(), Visibility::Company);
        assert_eq!(
            serde_json::to_value(Visibility::Groups).unwrap(),
            json!("groups")
        );
    }
}
