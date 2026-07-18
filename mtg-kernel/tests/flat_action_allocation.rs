use mtg_kernel::rl_session::{
    FastActorResponseV1, FastActorSessionV1, FlatActionCoreV1, FlatActionDecisionSliceBuffersV1,
    FlatActionObjectV1, FlatActionRefV1,
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

#[test]
fn admitted_flat_action_slice_encode_allocates_nothing() {
    let mut session = FastActorSessionV1::reset_with_limits(82_001, 101, 256, 32_768);
    let mut actions = [FlatActionCoreV1::default(); 64];
    let mut refs = [FlatActionRefV1::default(); 256];
    let mut objects = [FlatActionObjectV1::default(); 128];
    let mut encoded_decisions = 0_usize;
    let mut observed_later_revision = false;

    for _ in 0..512 {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            break;
        };

        // Measure every reached shape independently. Reset/step/consume are
        // intentionally outside the tracked region; this test is solely the
        // admitted flat encoder's no-allocation contract.
        ALLOCATION_COUNT.store(0, Ordering::SeqCst);
        TRACK_ALLOCATIONS.store(true, Ordering::SeqCst);
        let encoded = std::hint::black_box(&session)
            .encode_current_flat_action_slice_v1(
                decision,
                &mut FlatActionDecisionSliceBuffersV1 {
                    actions: &mut actions,
                    refs: &mut refs,
                    objects: &mut objects,
                },
            )
            .unwrap();
        TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
        let allocation_count = ALLOCATION_COUNT.load(Ordering::SeqCst);

        std::hint::black_box((&actions, &refs, &objects));
        assert!(encoded.active_action_count > 0);
        assert_eq!(allocation_count, 0, "decision {encoded_decisions}");
        encoded_decisions += 1;
        observed_later_revision |= decision.environment_revision > 1;

        if matches!(
            session
                .consume_current_flat_action_slice_v1(encoded.binding, 0)
                .unwrap(),
            FastActorResponseV1::Terminal(_)
        ) {
            break;
        }
    }

    assert!(encoded_decisions > 1);
    assert!(observed_later_revision);
}
