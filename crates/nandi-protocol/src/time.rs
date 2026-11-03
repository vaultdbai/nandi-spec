//! Timestamps on the wire: milliseconds since the Unix epoch as `f64`
//! (TypeScript bindings can't carry `u64`).

/// Milliseconds since the Unix epoch
pub type Timestamp = f64;

/// Now, in milliseconds since the epoch
pub fn now_ms() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_after_2020() {
        assert!(now_ms() > 1_577_836_800_000.0);
    }
}
