//! Versioned complete-agent descriptors and BO3 trajectory validation.
//!
//! These interfaces do not run a match or update a model. A valid descriptor is
//! not a compatibility certificate for an unimplemented opening/search adapter,
//! and a structurally valid trajectory is not independent proof of visibility.

mod package;
mod trajectory;

pub use package::*;
pub use trajectory::*;

pub(crate) fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

pub(crate) fn hex_digest(value: &str, digits: usize) -> bool {
    value.len() == digits
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests;
