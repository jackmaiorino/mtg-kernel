use super::*;

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Messages drawn from a small pool so that whole states and actions repeat
/// across and within slots; lengths straddle block and checkpoint
/// boundaries and states share long prefixes.
fn slot(rng: &mut u64, base: &[u8]) -> DigestSlotV1 {
    let mut state_json = base[..(next(rng) as usize % base.len()).max(1)].to_vec();
    if next(rng).is_multiple_of(3) {
        let at = next(rng) as usize % state_json.len();
        state_json[at] ^= 1 + (next(rng) % 3) as u8;
    }
    let mut slot = DigestSlotV1 {
        state_json,
        ..DigestSlotV1::default()
    };
    for _ in 0..next(rng) % 7 {
        let len = [0usize, 1, 40, 107, 108, 109, 235, 300][next(rng) as usize % 8];
        let fill = b"abc"[next(rng) as usize % 3];
        let start = slot.action_json.len();
        slot.action_json.extend(std::iter::repeat_n(fill, len));
        slot.action_ranges.push((start, slot.action_json.len()));
    }
    slot
}

#[test]
fn digest_memo_matches_unmemoized_digests() {
    let mut rng = 0x5eed_u64;
    let base: Vec<u8> = (0..6000).map(|i| b"{\"x\":[1,2],}"[i % 12]).collect();
    // A pool of slots replayed in random order, so whole messages repeat.
    let pool: Vec<DigestSlotV1> = (0..12).map(|_| slot(&mut rng, &base)).collect();
    for budget in [1usize, 4_000, 20_000, 1 << 24] {
        let mut memo = DigestMemoV1::default();
        let (mut plain, mut memoized) = (DigestBatchV1::default(), DigestBatchV1::default());
        for round in 0..300 {
            let count = 1 + (next(&mut rng) % 3) as usize;
            let picked: Vec<DigestSlotV1> = (0..count)
                .map(|_| {
                    let source = &pool[next(&mut rng) as usize % pool.len()];
                    DigestSlotV1 {
                        state_json: source.state_json.clone(),
                        action_scratch: Vec::new(),
                        action_json: source.action_json.clone(),
                        action_ranges: source.action_ranges.clone(),
                    }
                })
                .collect();
            let expected = hash_prepared_slots_unmemoized_v2(&picked, &mut plain);
            let actual = hash_prepared_slots_memoized_v2(&picked, &mut memoized, &mut memo, budget);
            assert_eq!(expected, actual, "offsets, budget {budget} round {round}");
            assert_eq!(
                plain.blocks, memoized.blocks,
                "digests, budget {budget} round {round}"
            );
            assert!(memo.bytes <= budget, "memo exceeds its budget");
        }
    }
    // Each digest is the plain SHA-512 of namespace || counter || message.
    let probe = &pool[0];
    let mut memo = DigestMemoV1::default();
    let mut batch = DigestBatchV1::default();
    for _ in 0..2 {
        hash_prepared_slots_memoized_v2(
            std::slice::from_ref(probe),
            &mut batch,
            &mut memo,
            1 << 20,
        );
        for counter in 0..ACTION_HASH_BLOCK_COUNT_V1 {
            let job = Sha512JobV1::new(STATE_HASH_NAMESPACE_V2, counter as u32, &probe.state_json);
            assert_eq!(
                batch.blocks[counter],
                crate::sha512_multi_v1::sha512_job_scalar_v1(&job)
            );
        }
    }
}

#[test]
fn digest_memo_separates_namespaces() {
    let mut memo = DigestMemoV1::default();
    let message = b"same bytes";
    let hash_state = digest_memo_hash_v1(STATE_HASH_NAMESPACE_V2, message);
    memo.insert(
        1 << 20,
        hash_state,
        STATE_HASH_NAMESPACE_V2,
        message,
        [[7; 64]; ACTION_HASH_BLOCK_COUNT_V1],
    );
    let hash_action = digest_memo_hash_v1(ACTION_HASH_NAMESPACE_V1, message);
    assert!(memo
        .get(hash_action, ACTION_HASH_NAMESPACE_V1, message)
        .is_none());
    assert!(memo
        .get(hash_state, ACTION_HASH_NAMESPACE_V1, message)
        .is_none());
    assert!(memo
        .get(hash_state, STATE_HASH_NAMESPACE_V2, message)
        .is_some());
}
