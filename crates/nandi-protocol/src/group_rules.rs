//! The `requests:` rules in a group's policy ([`GroupRequestRules`]): what
//! the group's members may have done for each other, and when the
//! provider's person approves first. The relay reads them from the signed
//! policy text (nothing else in it) and enforces them at `SubmitTask`,
//! from metadata only: requester, provider, capability, group, the time.
//!
//! ```yaml
//! requests:
//!   approval: always          # always | first-time | per-member
//!   daily_limit: 20           # leave out for no limit
//!   capabilities: { allow: [], deny: [] }
//!   hours: "Mon-Fri 08:00-18:00"
//!   time_zone: Europe/London  # IANA name; UTC when left out
//!   always_ask: [send, payment]
//! ```
//!
//! | Rule | Outcome at the relay |
//! | --- | --- |
//! | `capabilities` | refused: [`crate::GROUP_CAPABILITY_NOT_ALLOWED`] |
//! | `daily_limit` | refused: [`crate::GROUP_DAILY_LIMIT`] |
//! | `approval: first-time` | marked ([`crate::SealedTask::first_time_groups`]) |
//! | `hours` | held: [`crate::HOLD_OUTSIDE_HOURS`] |
//! | `always_ask` | held: [`crate::HOLD_ALWAYS_ASK`] |
//! | `approval: always` | held: [`crate::HOLD_APPROVAL_ALWAYS`] |
//!
//! Capability patterns are case-insensitive globs (`review*`, `send-?`).
//! An `always_ask` term also names any capability it is a word of: `send`
//! names `send-email`.

use jiff::tz::{TimeZone, TimeZoneDatabase};
use serde::Deserialize;

use crate::group::GroupApproval;
use crate::time::Timestamp;

/// The rules in a group's policy that the relay enforces at `SubmitTask`,
/// read from the `requests:` block of the policy's YAML frontmatter
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRequestRules {
    pub approval: GroupApproval,
    /// Requests one requester may send one provider a UTC day; `None`: no
    /// limit
    pub daily_limit: Option<u32>,
    pub capabilities: CapabilityRules,
    /// When requests may run without the provider's approval; `None`: any
    /// time
    pub hours: Option<GroupHours>,
    /// Capabilities (globs, or words in them) that always wait for the
    /// provider's approval
    pub always_ask: Vec<String>,
}

/// The capabilities a group lets its members ask each other for
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRules {
    /// Only these (globs); empty: any
    #[serde(default)]
    pub allow: Vec<String>,
    /// Never these (globs); wins over `allow`
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Days of the week and a time of day in one time zone, e.g. `Mon-Fri
/// 08:00-18:00` in `Europe/London`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHours {
    /// As the policy wrote it
    text: String,
    /// Monday first
    days: [bool; 7],
    /// Minutes since midnight, `start < end`
    start: u16,
    end: u16,
    zone: TimeZone,
}

/// The parts of a policy's frontmatter [`GroupRequestRules::read`] looks at
#[derive(Deserialize)]
struct PolicyFrontmatter {
    #[serde(default)]
    requests: Option<RequestsBlock>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestsBlock {
    #[serde(default)]
    approval: Option<GroupApproval>,
    #[serde(default)]
    daily_limit: Option<u32>,
    #[serde(default)]
    capabilities: Option<CapabilityRules>,
    #[serde(default)]
    hours: Option<String>,
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    always_ask: Vec<String>,
}

impl GroupRequestRules {
    /// Reads the rules from a policy file (Markdown with YAML
    /// frontmatter). No frontmatter or no `requests:` block: the defaults
    /// (`approval: always`, any capability, any time, no limit). A key in
    /// `requests:` it doesn't know, a `daily_limit` of 0, a bad pattern,
    /// hours or time zone is an error.
    pub fn read(policy_text: &str) -> Result<Self, String> {
        let Some(yaml) = frontmatter(policy_text)? else {
            return Ok(Self::default());
        };
        if yaml.trim().is_empty() {
            return Ok(Self::default());
        }
        let front: PolicyFrontmatter = serde_yaml::from_str(yaml)
            .map_err(|e| format!("The policy's frontmatter doesn't read: {e}"))?;
        let Some(requests) = front.requests else {
            return Ok(Self::default());
        };
        Self::from_block(requests)
    }

    fn from_block(requests: RequestsBlock) -> Result<Self, String> {
        if requests.daily_limit == Some(0) {
            return Err("requests.daily_limit is 0; leave it out for no limit".to_string());
        }
        let capabilities = requests.capabilities.unwrap_or_default();
        check_patterns("requests.capabilities.allow", &capabilities.allow)?;
        check_patterns("requests.capabilities.deny", &capabilities.deny)?;
        let always_ask: Vec<String> = requests
            .always_ask
            .iter()
            .map(|term| term.trim().to_string())
            .filter(|term| !term.is_empty())
            .collect();
        check_patterns("requests.always_ask", &always_ask)?;
        let hours = requests
            .hours
            .map(|hours| GroupHours::parse(&hours, requests.time_zone.as_deref()))
            .transpose()
            .map_err(|e| format!("requests.hours {e}"))?;
        Ok(Self {
            approval: requests.approval.unwrap_or_default(),
            daily_limit: requests.daily_limit,
            capabilities,
            hours,
            always_ask,
        })
    }

    /// Whether the group lets its members ask for `capability`
    pub fn allows(&self, capability: &str) -> bool {
        let CapabilityRules { allow, deny } = &self.capabilities;
        let matches = |pattern: &String| glob_matches(pattern, capability);
        !deny.iter().any(matches) && (allow.is_empty() || allow.iter().any(matches))
    }

    /// The `always_ask` term that names `capability`, if one does
    pub fn always_asks_for(&self, capability: &str) -> Option<&str> {
        self.always_ask
            .iter()
            .find(|term| names(term, capability))
            .map(String::as_str)
    }
}

impl GroupHours {
    /// `Mon-Fri 08:00-18:00`, `Mon,Wed 09:00-12:00`, `08:00-18:00` (every
    /// day) or `Sat-Sun` (all day), in `time_zone` (an IANA name such as
    /// `Europe/London`; `None`: UTC). Overnight hours aren't supported.
    pub fn parse(text: &str, time_zone: Option<&str>) -> Result<Self, String> {
        let text = text.trim();
        let mut days = [true; 7];
        let (mut start, mut end) = (0u16, 24 * 60);
        let mut seen_days = false;
        let mut seen_time = false;
        for part in text.split_whitespace() {
            if part.contains(':') && !seen_time {
                let (from, to) = part
                    .split_once('-')
                    .ok_or_else(|| format!("'{part}' isn't a time range like 08:00-18:00"))?;
                start = minutes(from)?;
                end = minutes(to)?;
                if start >= end {
                    return Err(format!(
                        "'{part}' ends before it starts (overnight hours aren't supported)"
                    ));
                }
                seen_time = true;
            } else if !seen_days && !seen_time {
                days = [false; 7];
                for item in part.split(',') {
                    let (from, to) = item.split_once('-').unwrap_or((item, item));
                    let (from, to) = (day(from)?, day(to)?);
                    if from > to {
                        return Err(format!("'{item}' runs backwards; write it Monday first"));
                    }
                    days[from..=to].iter_mut().for_each(|d| *d = true);
                }
                seen_days = true;
            } else {
                return Err(format!(
                    "'{text}' isn't like 'Mon-Fri 08:00-18:00': unexpected '{part}'"
                ));
            }
        }
        if !seen_days && !seen_time {
            return Err("is empty; write it like 'Mon-Fri 08:00-18:00'".to_string());
        }
        let zone = match time_zone.map(str::trim) {
            None => TimeZone::UTC,
            Some(name) => TimeZoneDatabase::bundled().get(name).map_err(|_| {
                format!("has a time zone '{name}' that isn't an IANA name like Europe/London")
            })?,
        };
        Ok(Self {
            text: text.to_string(),
            days,
            start,
            end,
            zone,
        })
    }

    /// Whether `at` (milliseconds since the epoch) falls inside, in the
    /// hours' time zone
    pub fn contains(&self, at: Timestamp) -> bool {
        // Whole milliseconds: a Timestamp is an f64 on the wire
        let Ok(at) = jiff::Timestamp::from_millisecond(at as i64) else {
            return false;
        };
        let local = at.to_zoned(self.zone.clone());
        let weekday = usize::from(local.weekday().to_monday_zero_offset().unsigned_abs());
        let minute =
            u16::from(local.hour().unsigned_abs()) * 60 + u16::from(local.minute().unsigned_abs());
        self.days[weekday] && (self.start..self.end).contains(&minute)
    }

    /// The hours as the policy wrote them, and their time zone
    pub fn describe(&self) -> String {
        let zone = self.zone.iana_name().unwrap_or("UTC");
        format!("{} {zone}", self.text)
    }
}

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

fn day(text: &str) -> Result<usize, String> {
    let lower = text.trim().to_ascii_lowercase();
    DAYS.iter()
        .position(|d| lower.starts_with(d))
        .ok_or_else(|| format!("'{text}' isn't a day (Mon, Tue, Wed, Thu, Fri, Sat, Sun)"))
}

fn minutes(text: &str) -> Result<u16, String> {
    let bad = || format!("'{text}' isn't a time like 08:00");
    let (h, m) = text.trim().split_once(':').ok_or_else(bad)?;
    let (h, m): (u16, u16) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
    if m >= 60 || h > 24 || (h == 24 && m > 0) {
        return Err(bad());
    }
    Ok(h * 60 + m)
}

fn check_patterns(field: &str, patterns: &[String]) -> Result<(), String> {
    for pattern in patterns {
        glob::Pattern::new(pattern.trim())
            .map_err(|e| format!("{field} has a bad pattern '{pattern}': {e}"))?;
    }
    Ok(())
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let options = glob::MatchOptions {
        case_sensitive: false,
        ..glob::MatchOptions::default()
    };
    glob::Pattern::new(pattern.trim()).is_ok_and(|p| p.matches_with(value, options))
}

/// `term` names `capability`: the same, a glob over it, or one of its
/// words (`send` in `send-email`)
fn names(term: &str, capability: &str) -> bool {
    let term = term.trim();
    glob_matches(term, capability)
        || capability
            .split(['-', '_', ':', '.', '/'])
            .any(|word| word.eq_ignore_ascii_case(term))
}

/// The YAML between a leading `---` line and the next `---` line; `None`
/// when the text doesn't start with one
fn frontmatter(text: &str) -> Result<Option<&str>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Ok(None);
    };
    if first.trim_end() != "---" {
        return Ok(None);
    }
    let start = first.len();
    let mut offset = start;
    for line in lines {
        if line.trim_end() == "---" {
            return Ok(Some(&text[start..offset]));
        }
        offset += line.len();
    }
    Err("The policy's frontmatter has no closing '---'".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use GroupApproval::{Always, FirstTime, PerMember};

    fn rules(approval: GroupApproval, daily_limit: Option<u32>) -> GroupRequestRules {
        GroupRequestRules {
            approval,
            daily_limit,
            ..GroupRequestRules::default()
        }
    }

    fn read(requests: &str) -> GroupRequestRules {
        GroupRequestRules::read(&format!("---\nrequests:\n{requests}---\nWhy.\n")).unwrap()
    }

    /// 2026-11-11 (a Wednesday) at `hh:mm` UTC, in milliseconds
    fn wednesday_utc(hh: i64, mm: i64) -> Timestamp {
        let midnight = 1_794_355_200_000_i64;
        (midnight + (hh * 60 + mm) * 60_000) as Timestamp
    }

    #[test]
    fn request_rules_read_from_the_frontmatter() {
        let cases: &[(&str, &str, Result<GroupRequestRules, &str>)] = &[
            ("no frontmatter", "Just words.\n", Ok(rules(Always, None))),
            ("empty frontmatter", "---\n---\nWhy.\n", Ok(rules(Always, None))),
            (
                "no requests block",
                "---\ntools: { deny: [shell] }\n---\n",
                Ok(rules(Always, None)),
            ),
            ("an empty block", "---\nrequests:\n---\n", Ok(rules(Always, None))),
            (
                "first-time with a limit",
                "---\nrequests:\n  approval: first-time\n  daily_limit: 20\n---\nWhy.\n",
                Ok(rules(FirstTime, Some(20))),
            ),
            (
                "per-member, CRLF and a BOM",
                "\u{feff}---\r\nrequests:\r\n  approval: per-member\r\n---\r\n",
                Ok(rules(PerMember, None)),
            ),
            (
                "a limit of 0",
                "---\nrequests:\n  daily_limit: 0\n---\n",
                Err("daily_limit is 0"),
            ),
            (
                "an unknown approval",
                "---\nrequests:\n  approval: sometimes\n---\n",
                Err("doesn't read"),
            ),
            (
                "an unknown key",
                "---\nrequests:\n  approvals: always\n---\n",
                Err("unknown field"),
            ),
            (
                "a bad capability pattern",
                "---\nrequests:\n  capabilities: { deny: [\"[\"] }\n---\n",
                Err("requests.capabilities.deny has a bad pattern"),
            ),
            (
                "an unknown capability key",
                "---\nrequests:\n  capabilities: { only: [review] }\n---\n",
                Err("unknown field"),
            ),
            (
                "bad hours",
                "---\nrequests:\n  hours: \"Mon-Fri 18:00-08:00\"\n---\n",
                Err("requests.hours '18:00-08:00' ends before it starts"),
            ),
            (
                "an unknown time zone",
                "---\nrequests:\n  hours: \"Mon-Fri 08:00-18:00\"\n  time_zone: Mars/Olympus\n---\n",
                Err("requests.hours has a time zone 'Mars/Olympus'"),
            ),
            (
                "no closing line",
                "---\nrequests:\n  approval: always\n",
                Err("no closing"),
            ),
        ];
        for (name, text, expected) in cases {
            match (GroupRequestRules::read(text), expected) {
                (Ok(got), Ok(want)) => assert_eq!(&got, want, "{name}"),
                (Err(got), Err(want)) => assert!(got.contains(want), "{name}: {got}"),
                (got, want) => panic!("{name}: got {got:?}, want {want:?}"),
            }
        }
    }

    #[test]
    fn every_rule_reads_together() {
        let got = read(
            "  approval: per-member\n  daily_limit: 5\n  capabilities: { allow: [review*], deny: [review-secret] }\n  hours: \"Mon-Fri 08:00-18:00\"\n  time_zone: Europe/London\n  always_ask: [send, \" \", payment]\n",
        );
        assert_eq!(got.approval, PerMember);
        assert_eq!(got.daily_limit, Some(5));
        assert_eq!(got.capabilities.allow, ["review*"]);
        assert_eq!(got.capabilities.deny, ["review-secret"]);
        assert_eq!(got.always_ask, ["send", "payment"], "blank terms dropped");
        assert_eq!(
            got.hours.unwrap().describe(),
            "Mon-Fri 08:00-18:00 Europe/London"
        );
    }

    #[test]
    fn capabilities_allow_and_deny() {
        let open = read("  approval: per-member\n");
        let only_review = read("  capabilities: { allow: [review*] }\n");
        let no_secrets = read("  capabilities: { allow: [review*], deny: [\"*secret*\"] }\n");
        // (rules, capability, allowed)
        let cases = [
            (&open, "anything", true),
            (&only_review, "review", true),
            (&only_review, "Review-Code", true),
            (&only_review, "summarize", false),
            (&no_secrets, "review-code", true),
            (&no_secrets, "review-secrets", false),
        ];
        for (rules, capability, allowed) in cases {
            assert_eq!(rules.allows(capability), allowed, "{capability}");
        }
    }

    #[test]
    fn always_ask_names_a_capability_or_a_word_of_it() {
        let rules = read("  always_ask: [send, \"pay*\"]\n");
        let cases = [
            ("send", Some("send")),
            ("send-email", Some("send")),
            ("email.SEND", Some("send")),
            ("sender", None),
            ("payment", Some("pay*")),
            ("review", None),
        ];
        for (capability, term) in cases {
            assert_eq!(rules.always_asks_for(capability), term, "{capability}");
        }
    }

    #[test]
    fn hours_read_in_their_time_zone() {
        // (hours, time zone, at, inside)
        let cases = [
            ("Mon-Fri 08:00-18:00", None, wednesday_utc(8, 0), true),
            ("Mon-Fri 08:00-18:00", None, wednesday_utc(7, 59), false),
            ("Mon-Fri 08:00-18:00", None, wednesday_utc(18, 0), false),
            ("Sat-Sun", None, wednesday_utc(12, 0), false),
            ("Mon,Wed 09:00-12:00", None, wednesday_utc(9, 30), true),
            ("08:00-18:00", None, wednesday_utc(12, 0), true),
            ("00:00-24:00", None, wednesday_utc(23, 59), true),
            // New York is UTC-5 in November: 13:00 UTC is 08:00 there
            (
                "Mon-Fri 08:00-18:00",
                Some("America/New_York"),
                wednesday_utc(13, 0),
                true,
            ),
            (
                "Mon-Fri 08:00-18:00",
                Some("America/New_York"),
                wednesday_utc(12, 59),
                false,
            ),
            // Tokyo is UTC+9: 20:00 UTC Wednesday is 05:00 Thursday there
            (
                "Wed 00:00-24:00",
                Some("Asia/Tokyo"),
                wednesday_utc(20, 0),
                false,
            ),
            ("Thu", Some("Asia/Tokyo"), wednesday_utc(20, 0), true),
        ];
        for (text, zone, at, inside) in cases {
            let hours = GroupHours::parse(text, zone).unwrap();
            assert_eq!(hours.contains(at), inside, "{text} in {zone:?} at {at}");
        }
    }

    #[test]
    fn bad_hours_say_why() {
        let cases = [
            ("", "is empty"),
            ("Mon-Fri 08:00", "isn't a time range"),
            ("Fri-Mon", "runs backwards"),
            ("Someday", "isn't a day"),
            ("Mon 08:00-25:00", "isn't a time like"),
            ("Mon 08:00-18:00 extra", "unexpected 'extra'"),
        ];
        for (text, why) in cases {
            let error = GroupHours::parse(text, None).unwrap_err();
            assert!(error.contains(why), "{text}: {error}");
        }
    }
}
