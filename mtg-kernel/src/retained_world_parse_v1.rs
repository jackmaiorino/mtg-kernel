//! Exact reuse of typed parses of retained JSON worlds.
//!
//! Custody validators keep private input worlds as JSON bytes inside history
//! and parse them again on every validation, for every retained record. A
//! parse is a pure function of its bytes, and `GameState` has no hash maps or
//! custom clones, so a clone of an earlier successful parse of byte-identical
//! input is the same value. Only successes are remembered; a failure always
//! takes the full parse and returns its own error.

use crate::state::GameState;
use std::cell::RefCell;

struct Parsed {
    bytes: Vec<u8>,
    world: GameState,
}

/// A few retained worlds are live at once; the small cap bounds memory.
const CAPACITY: usize = 32;

pub(crate) fn parse_world(bytes: &[u8]) -> Result<GameState, serde_json::Error> {
    thread_local! {
        static PARSED: RefCell<Vec<Parsed>> = const { RefCell::new(Vec::new()) };
    }
    // try_with: during thread teardown the cache may be gone; then this is
    // simply the full parse. Equality compares every byte.
    let hit = PARSED
        .try_with(|parsed| {
            parsed
                .borrow()
                .iter()
                .find(|row| row.bytes.as_slice() == bytes)
                .map(|row| row.world.clone())
        })
        .ok()
        .flatten();
    if let Some(world) = hit {
        return Ok(world);
    }
    let world: GameState = serde_json::from_slice(bytes)?;
    let _ = PARSED.try_with(|parsed| {
        let mut parsed = parsed.borrow_mut();
        if parsed.len() >= CAPACITY {
            parsed.remove(0);
        }
        parsed.push(Parsed {
            bytes: bytes.to_vec(),
            world: world.clone(),
        });
    });
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;

    #[test]
    fn remembered_parse_equals_fresh_parse_and_failures_are_not_remembered() {
        let state = crate::extra_turn_v1::tests::ready(PlayerId::P0);
        let bytes = serde_json::to_vec(&state).unwrap();
        let fresh: GameState = serde_json::from_slice(&bytes).unwrap();
        let first = parse_world(&bytes).unwrap();
        let second = parse_world(&bytes).unwrap();
        assert_eq!(first, fresh);
        assert_eq!(second, fresh);
        assert_eq!(serde_json::to_vec(&second).unwrap(), bytes);
        let mut broken = bytes.clone();
        broken.pop();
        for _ in 0..2 {
            assert_eq!(
                parse_world(&broken).unwrap_err().to_string(),
                serde_json::from_slice::<GameState>(&broken)
                    .unwrap_err()
                    .to_string()
            );
        }
    }
}
