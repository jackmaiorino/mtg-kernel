use mtg_kernel::fast_sampler::{FastCategoricalScratch, FAST_CATEGORICAL_MAX_ACTIONS};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

struct CountingAllocator;

thread_local! {
    static TRACK_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record_tracked_allocation() {
    let _ = TRACK_ALLOCATIONS.try_with(|tracking| {
        if tracking.get() {
            let _ = ALLOCATIONS.try_with(|allocations| {
                allocations.set(allocations.get().saturating_add(1));
            });
        }
    });
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_tracked_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_tracked_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_tracked_allocation();
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn complete_admitted_sample_path_is_allocation_free() {
    let logits = core::array::from_fn::<_, FAST_CATEGORICAL_MAX_ACTIONS, _>(|index| {
        -(((index * 37) % 4_097) as f32 / 256.0)
    });
    let mut scratch = FastCategoricalScratch::default();
    black_box(
        scratch
            .sample(black_box(&logits), black_box(0_u64))
            .unwrap(),
    );
    ALLOCATIONS.with(|allocations| allocations.set(0));
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(true));
    for seed in 0..1_024 {
        black_box(scratch.sample(black_box(&logits), black_box(seed)).unwrap());
    }
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    assert_eq!(allocations, 0);
}
