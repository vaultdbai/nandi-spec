use std::fmt;

/// Why a key, a signature or an envelope was refused. Every failure to
/// open is one of these; none of them says more than which check failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    /// Not the expected encoding or length (a key, a signature, an
    /// envelope's bytes)
    Malformed(String),
    /// Not a key on its curve
    BadKey(String),
    /// The envelope uses a suite this crate doesn't speak
    UnknownSuite(String),
    /// Sealed for another key than ours
    WrongRecipient,
    /// The seal didn't open: the ciphertext, the associated data or the
    /// keys don't match what was sealed
    NotOpened,
    /// The content isn't signed by the expected author
    BadSignature,
    /// A reseal that changes what the author signed (task, purpose or
    /// capability)
    Rebound,
    /// The system's randomness failed
    Randomness,
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(what) => write!(f, "Malformed: {what}"),
            Self::BadKey(what) => write!(f, "Not a valid key: {what}"),
            Self::UnknownSuite(suite) => write!(f, "Unknown seal suite {suite}"),
            Self::WrongRecipient => f.write_str("Sealed for another key"),
            Self::NotOpened => f.write_str("The envelope doesn't open with this key"),
            Self::BadSignature => f.write_str("The content isn't signed by its author"),
            Self::Rebound => {
                f.write_str("A reseal can't change the task, purpose or capability signed")
            }
            Self::Randomness => f.write_str("The system's randomness failed"),
        }
    }
}

impl std::error::Error for SealError {}
