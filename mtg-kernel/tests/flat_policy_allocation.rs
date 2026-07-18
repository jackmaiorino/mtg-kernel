use mtg_kernel::flat_policy_v1::{
    FlatCompletedDungeonV1, FlatContextPathElementV1, FlatDecisionBuffersV1, FlatDecisionEncoderV1,
    FlatEffectSubtypeChangeV1, FlatObjectAbilityUseV1, FlatObjectCoreV1, FlatObjectGoadV1,
    FlatObjectSubtypeV1, FlatRelationV1,
};
use mtg_kernel::rl_session::{
    FastActorResponseV1, FastActorSessionV1, FlatActionCoreV1, FlatActionObjectV1, FlatActionRefV1,
    CANONICAL_RALLY_DECK_ID,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct CountingAllocator;

static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

struct OwnedBuffers {
    objects: Vec<FlatObjectCoreV1>,
    relations: Vec<FlatRelationV1>,
    object_subtypes: Vec<FlatObjectSubtypeV1>,
    ability_uses: Vec<FlatObjectAbilityUseV1>,
    goads: Vec<FlatObjectGoadV1>,
    completed_dungeons: Vec<FlatCompletedDungeonV1>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV1>,
    context_path_elements: Vec<FlatContextPathElementV1>,
    actions: Vec<FlatActionCoreV1>,
    action_refs: Vec<FlatActionRefV1>,
    action_objects: Vec<FlatActionObjectV1>,
}

impl OwnedBuffers {
    fn ample() -> Self {
        Self {
            objects: vec![FlatObjectCoreV1::default(); 1_024],
            relations: vec![FlatRelationV1::default(); 4_096],
            object_subtypes: vec![FlatObjectSubtypeV1::default(); 4_096],
            ability_uses: vec![FlatObjectAbilityUseV1::default(); 1_024],
            goads: vec![FlatObjectGoadV1::default(); 1_024],
            completed_dungeons: vec![FlatCompletedDungeonV1::default(); 256],
            effect_subtype_changes: vec![FlatEffectSubtypeChangeV1::default(); 1_024],
            context_path_elements: vec![FlatContextPathElementV1::default(); 1_024],
            actions: vec![FlatActionCoreV1::default(); 128],
            action_refs: vec![FlatActionRefV1::default(); 1_024],
            action_objects: vec![FlatActionObjectV1::default(); 1_024],
        }
    }

    fn view(&mut self) -> FlatDecisionBuffersV1<'_> {
        FlatDecisionBuffersV1 {
            objects: &mut self.objects,
            relations: &mut self.relations,
            object_subtypes: &mut self.object_subtypes,
            ability_uses: &mut self.ability_uses,
            goads: &mut self.goads,
            completed_dungeons: &mut self.completed_dungeons,
            effect_subtype_changes: &mut self.effect_subtype_changes,
            context_path_elements: &mut self.context_path_elements,
            actions: &mut self.actions,
            action_refs: &mut self.action_refs,
            action_objects: &mut self.action_objects,
        }
    }
}

fn assert_warmed_encode_allocates_nothing(mut session: FastActorSessionV1) {
    let mut encoder = FlatDecisionEncoderV1::default();
    let mut buffers = OwnedBuffers::ample();
    let mut decisions = 0_usize;
    for _ in 0..128 {
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            break;
        };
        let first = session
            .encode_current_flat_decision_v1(expected, &mut encoder, &mut buffers.view())
            .unwrap();

        ALLOCATION_COUNT.store(0, Ordering::SeqCst);
        TRACK_ALLOCATIONS.store(true, Ordering::SeqCst);
        let warmed = std::hint::black_box(&session)
            .encode_current_flat_decision_v1(
                expected,
                std::hint::black_box(&mut encoder),
                &mut buffers.view(),
            )
            .unwrap();
        TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
        assert_eq!(first, warmed);
        assert_eq!(
            ALLOCATION_COUNT.load(Ordering::SeqCst),
            0,
            "warmed decision {decisions}"
        );
        decisions += 1;
        if matches!(
            session
                .consume_current_flat_action_slice_v1(first.binding.action_binding, 0)
                .unwrap(),
            FastActorResponseV1::Terminal(_)
        ) {
            break;
        }
    }
    assert!(decisions > 1);
}

#[test]
fn burn_and_rally_warmed_complete_flat_encode_allocates_nothing() {
    assert_warmed_encode_allocates_nothing(FastActorSessionV1::reset_with_limits(
        90_010, 101, 256, 32_768,
    ));
    assert_warmed_encode_allocates_nothing(
        FastActorSessionV1::reset_with_decks_and_limits(
            90_011,
            102,
            256,
            32_768,
            [
                CANONICAL_RALLY_DECK_ID.to_string(),
                CANONICAL_RALLY_DECK_ID.to_string(),
            ],
        )
        .unwrap(),
    );
}
