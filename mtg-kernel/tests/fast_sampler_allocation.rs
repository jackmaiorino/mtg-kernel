use mtg_kernel::fast_sampler::{FastCategoricalScratch, FAST_CATEGORICAL_MAX_ACTIONS};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
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
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for seed in 0..1_024 {
        black_box(scratch.sample(black_box(&logits), black_box(seed)).unwrap());
    }
    let after = ALLOCATIONS.load(Ordering::Relaxed);
    assert_eq!(after, before);
}
