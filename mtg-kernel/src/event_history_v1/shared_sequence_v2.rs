//! Persistent ordered storage with bounded copy-on-write leaves and spines.
//!
//! Hot clone/prefix/append operations never materialize a contiguous history.
//! `as_vec` and `as_mut_vec` are explicit cold compatibility operations. After
//! arbitrary mutable Vec access, reads use that Vec until `repack`, `push` or
//! a shortening `truncate` consumes it back into the tree. Cloning or taking a
//! prefix of that cold editable representation is necessarily linear.
//!
//! Each node counts its elements of each kind, and each leaf keeps the kind
//! of each of its elements beside it, so a kind filter skips subtrees that
//! hold none of its kinds and, inside a leaf, reads the kinds instead of the
//! elements.
use std::iter::FusedIterator;
use std::ops::{Bound, RangeBounds};
use std::sync::{Arc, OnceLock};

const LEAF: usize = 64;
const FANOUT: usize = 32;

enum Body<T: Clone, const C: usize> {
    /// The elements, and the kind of each when the sequence counts kinds
    /// (none when `C == 0`). A kind is written with its element, by the
    /// sequence's kind function, so a kind filter reads these instead of
    /// the elements.
    Leaf(Vec<T>, Vec<u16>),
    Branch(Vec<Arc<Node<T, C>>>),
}

struct Node<T: Clone, const C: usize> {
    len: usize,
    counts: [u32; C],
    body: Body<T, C>,
}

impl<T: Clone, const C: usize> Clone for Node<T, C> {
    fn clone(&self) -> Self {
        Self {
            len: self.len,
            counts: self.counts,
            body: match &self.body {
                Body::Leaf(values, kinds) => Body::Leaf(values.clone(), kinds.clone()),
                Body::Branch(children) => Body::Branch(children.clone()),
            },
        }
    }
}

fn kind_index<T, const C: usize>(value: &T, kind: fn(&T) -> usize) -> Option<usize> {
    if C == 0 {
        None
    } else {
        let index = kind(value);
        assert!(index < C, "sequence kind is outside its count domain");
        Some(index)
    }
}

fn add_count<const C: usize>(counts: &mut [u32; C], index: Option<usize>) {
    if let Some(index) = index {
        counts[index] = counts[index]
            .checked_add(1)
            .expect("sequence kind count overflow");
    }
}

/// Records an element's kind in its leaf, beside the element.
fn add_kind<const C: usize>(kinds: &mut Vec<u16>, index: Option<usize>) {
    const { assert!(C <= 1 << 16, "count domain wider than a leaf's kinds") };
    if let Some(index) = index {
        kinds.push(index as u16);
    }
}

impl<T: Clone, const C: usize> Node<T, C> {
    fn leaf(values: Vec<T>, kind: fn(&T) -> usize) -> Self {
        debug_assert!(!values.is_empty() && values.len() <= LEAF);
        let mut counts = [0; C];
        let mut kinds = Vec::with_capacity(if C == 0 { 0 } else { values.len() });
        for value in &values {
            let index = kind_index::<T, C>(value, kind);
            add_count(&mut counts, index);
            add_kind::<C>(&mut kinds, index);
        }
        Self {
            len: values.len(),
            counts,
            body: Body::Leaf(values, kinds),
        }
    }

    /// A new leaf holding just `value`, of kind `index`.
    fn single(value: T, index: Option<usize>) -> Self {
        let mut counts = [0; C];
        add_count(&mut counts, index);
        let mut kinds = Vec::new();
        add_kind::<C>(&mut kinds, index);
        Self {
            len: 1,
            counts,
            body: Body::Leaf(vec![value], kinds),
        }
    }

    fn branch(children: Vec<Arc<Self>>) -> Self {
        debug_assert!(!children.is_empty() && children.len() <= FANOUT);
        let mut len = 0usize;
        let mut counts = [0u32; C];
        for child in &children {
            len = len
                .checked_add(child.len)
                .expect("sequence length overflow");
            for (total, count) in counts.iter_mut().zip(child.counts) {
                *total = total
                    .checked_add(count)
                    .expect("sequence kind count overflow");
            }
        }
        Self {
            len,
            counts,
            body: Body::Branch(children),
        }
    }

    /// A carry is a sibling of the same height containing just the new item.
    /// Full leaves are shared unchanged, rather than cloned before splitting.
    fn append(node: &mut Arc<Self>, value: T, index: Option<usize>) -> Option<Arc<Self>> {
        if matches!(&node.body, Body::Leaf(values, _) if values.len() == LEAF) {
            return Some(Arc::new(Self::single(value, index)));
        }
        let node = Arc::make_mut(node);
        match &mut node.body {
            Body::Leaf(values, kinds) => {
                values.push(value);
                add_kind::<C>(kinds, index);
            }
            Body::Branch(children) => {
                if let Some(carry) =
                    Self::append(children.last_mut().expect("nonempty branch"), value, index)
                {
                    if children.len() == FANOUT {
                        return Some(Arc::new(Self::branch(vec![carry])));
                    }
                    children.push(carry);
                }
            }
        }
        node.len = node.len.checked_add(1).expect("sequence length overflow");
        add_count(&mut node.counts, index);
        None
    }

    fn prefix(node: &Arc<Self>, len: usize, kind: fn(&T) -> usize) -> Arc<Self> {
        debug_assert!(len > 0 && len <= node.len);
        if len == node.len {
            return node.clone();
        }
        match &node.body {
            Body::Leaf(values, _) => Arc::new(Self::leaf(values[..len].to_vec(), kind)),
            Body::Branch(children) => {
                let mut remaining = len;
                let mut selected = Vec::new();
                for child in children {
                    if remaining == 0 {
                        break;
                    }
                    if remaining >= child.len {
                        selected.push(child.clone());
                        remaining -= child.len;
                    } else {
                        // Keep the child's height. Only the outer root may be
                        // collapsed, so future sibling carries stay balanced.
                        selected.push(Self::prefix(child, remaining, kind));
                        remaining = 0;
                    }
                }
                Arc::new(Self::branch(selected))
            }
        }
    }
}

/// `kind` must be a deterministic classification in `0..C`. For `C == 0`
/// it is never called. Counts and structure are derived, not serialized data.
pub(crate) struct SharedSequence<T: Clone, const C: usize> {
    root: Option<Arc<Node<T, C>>>,
    kind: fn(&T) -> usize,
    flat: OnceLock<Vec<T>>,
    editable: bool,
}

impl<T: Clone, const C: usize> Clone for SharedSequence<T, C> {
    fn clone(&self) -> Self {
        if self.editable {
            return Self::from_vec(self.flat.get().expect("editable Vec").clone(), self.kind);
        }
        Self {
            root: self.root.clone(),
            kind: self.kind,
            flat: OnceLock::new(),
            editable: false,
        }
    }
}

impl<T: Clone, const C: usize> SharedSequence<T, C> {
    pub(crate) fn new(kind: fn(&T) -> usize) -> Self {
        Self {
            root: None,
            kind,
            flat: OnceLock::new(),
            editable: false,
        }
    }

    /// Consumes elements into leaves without cloning them.
    pub(crate) fn from_vec(values: Vec<T>, kind: fn(&T) -> usize) -> Self {
        let mut values = values.into_iter();
        let mut level = Vec::new();
        loop {
            let chunk: Vec<_> = values.by_ref().take(LEAF).collect();
            if chunk.is_empty() {
                break;
            }
            level.push(Arc::new(Node::leaf(chunk, kind)));
        }
        while level.len() > 1 {
            let mut children = level.into_iter();
            level = Vec::new();
            loop {
                let chunk: Vec<_> = children.by_ref().take(FANOUT).collect();
                if chunk.is_empty() {
                    break;
                }
                level.push(Arc::new(Node::branch(chunk)));
            }
        }
        Self {
            root: level.pop(),
            kind,
            flat: OnceLock::new(),
            editable: false,
        }
    }

    pub(crate) fn len(&self) -> usize {
        if self.editable {
            self.flat.get().expect("editable Vec").len()
        } else {
            self.root.as_ref().map_or(0, |node| node.len)
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn counts(&self) -> [u32; C] {
        if self.editable {
            let mut counts = [0; C];
            for value in self.flat.get().expect("editable Vec") {
                add_count(&mut counts, kind_index::<T, C>(value, self.kind));
            }
            counts
        } else {
            self.root.as_ref().map_or([0; C], |node| node.counts)
        }
    }

    /// The leaf holding the logical `index`, the position of the leaf's first
    /// element, and the nodes after the leaf under its parent: each begins
    /// where the one before it ends.
    fn leaf_and_rest_at(&self, mut index: usize) -> Option<LeafAt<'_, T, C>> {
        if self.editable {
            let values = self.flat.get().expect("editable Vec");
            return (index < values.len()).then_some((values.as_slice(), 0, &[][..]));
        }
        let mut node = self.root.as_deref()?;
        if index >= node.len {
            return None;
        }
        let mut start = 0;
        let mut rest: &[Arc<Node<T, C>>] = &[];
        loop {
            match &node.body {
                Body::Leaf(values, _) => return Some((values.as_slice(), start, rest)),
                Body::Branch(children) => {
                    for (at, child) in children.iter().enumerate() {
                        if index < child.len {
                            node = child;
                            rest = &children[at + 1..];
                            break;
                        }
                        index -= child.len;
                        start += child.len;
                    }
                }
            }
        }
    }

    fn leaf_at(&self, index: usize) -> Option<(&[T], usize)> {
        self.leaf_and_rest_at(index)
            .map(|(leaf, start, _)| (leaf, start))
    }

    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        let (leaf, start) = self.leaf_at(index)?;
        leaf.get(index - start)
    }

    /// Exact logical prefix equality. Shared immutable subtrees are skipped in
    /// one step; only unshared leaf segments compare their values. `Eq` is
    /// required because identity cannot prove non-reflexive `PartialEq` values
    /// (for example NaN) equal. No contiguous storage is materialized.
    pub(crate) fn common_prefix_len(&self, other: &Self) -> usize
    where
        T: Eq,
    {
        self.common_prefix_len_inner(other, || {})
    }

    // The callback lets tests measure traversal work as well as value compares:
    // counting only compares would miss an accidental walk over shared leaves.
    fn common_prefix_len_inner(&self, other: &Self, mut visit: impl FnMut()) -> usize
    where
        T: Eq,
    {
        if self.editable || other.editable {
            return self
                .iter()
                .zip(other.iter())
                .take_while(|(a, b)| a == b)
                .count();
        }
        let limit = self.len().min(other.len());
        if limit == 0 {
            return 0;
        }
        fn height<T: Clone, const C: usize>(mut node: &Node<T, C>) -> usize {
            let mut height = 0;
            while let Body::Branch(children) = &node.body {
                height += 1;
                node = &children[0];
            }
            height
        }
        fn expand<T: Clone, const C: usize>(
            frontier: &mut Vec<(&Arc<Node<T, C>>, usize, usize, usize)>,
        ) {
            let (node, height, start, end) = frontier.pop().expect("nonempty frontier");
            let Body::Branch(children) = &node.body else {
                unreachable!("only branch frontiers expand");
            };
            let mut child_end = node.len;
            for child in children.iter().rev() {
                let child_start = child_end - child.len;
                let from = start.max(child_start);
                let to = end.min(child_end);
                if from < to {
                    frontier.push((child, height - 1, from - child_start, to - child_start));
                }
                child_end = child_start;
            }
        }
        let a = self.root.as_ref().expect("nonempty tree");
        let b = other.root.as_ref().expect("nonempty tree");
        let mut left = vec![(a, height(a), 0, limit)];
        let mut right = vec![(b, height(b), 0, limit)];
        let mut matched = 0;
        while matched < limit {
            visit();
            let (a, a_height, a_start, a_end) = *left.last().expect("left prefix remaining");
            let (b, b_height, b_start, b_end) = *right.last().expect("right prefix remaining");
            let count = (a_end - a_start).min(b_end - b_start);
            if !(Arc::ptr_eq(a, b) && a_start == b_start) {
                // Expanding both sides blindly would discard a shared subtree
                // when one root has grown or a prefix root has collapsed.
                if a_height != 0 || b_height != 0 {
                    if a_height >= b_height && a_height != 0 {
                        expand(&mut left);
                    }
                    if b_height >= a_height && b_height != 0 {
                        expand(&mut right);
                    }
                    continue;
                }
                let (Body::Leaf(a_values, _), Body::Leaf(b_values, _)) = (&a.body, &b.body) else {
                    unreachable!("height zero is a leaf");
                };
                for offset in 0..count {
                    if a_values[a_start + offset] != b_values[b_start + offset] {
                        return matched + offset;
                    }
                }
            }
            matched += count;
            left.pop();
            right.pop();
            if a_start + count < a_end {
                left.push((a, a_height, a_start + count, a_end));
            }
            if b_start + count < b_end {
                right.push((b, b_height, b_start + count, b_end));
            }
        }
        matched
    }

    pub(crate) fn prefix(&self, len: usize) -> Self {
        let len = len.min(self.len());
        if self.editable {
            return Self::from_vec(
                self.flat.get().expect("editable Vec")[..len].to_vec(),
                self.kind,
            );
        }
        let mut result = Self::new(self.kind);
        if len != 0 {
            let mut root = Node::prefix(self.root.as_ref().expect("nonempty tree"), len, self.kind);
            while let Body::Branch(children) = &root.body {
                if children.len() != 1 {
                    break;
                }
                root = children[0].clone();
            }
            result.root = Some(root);
        }
        result
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        if len >= self.len() {
            return;
        }
        self.repack();
        *self = self.prefix(len);
    }

    pub(crate) fn push(&mut self, value: T) {
        let index = kind_index::<T, C>(&value, self.kind);
        self.repack();
        // Only explicit cold materialization creates this cache.
        self.flat.take();
        if let Some(root) = &mut self.root {
            if let Some(carry) = Node::append(root, value, index) {
                *root = Arc::new(Node::branch(vec![root.clone(), carry]));
            }
        } else {
            self.root = Some(Arc::new(Node::single(value, index)));
        }
    }

    pub(crate) fn iter(&self) -> Iter<'_, T, C> {
        self.range(..)
    }

    /// Range bounds and overflow reject like slice indexing.
    pub(crate) fn range(&self, range: impl RangeBounds<usize>) -> Iter<'_, T, C> {
        let start = match range.start_bound() {
            Bound::Included(&start) => start,
            Bound::Excluded(&start) => start.checked_add(1).expect("sequence range start overflow"),
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&end) => end.checked_add(1).expect("sequence range end overflow"),
            Bound::Excluded(&end) => end,
            Bound::Unbounded => self.len(),
        };
        assert!(
            start <= end && end <= self.len(),
            "sequence range out of bounds"
        );
        Iter {
            sequence: self,
            front: Default::default(),
            siblings: Default::default(),
            middle: start..end,
            back: Default::default(),
        }
    }

    pub(crate) fn partition_point(&self, mut predicate: impl FnMut(&T) -> bool) -> usize {
        let (mut left, mut right) = (0, self.len());
        while left < right {
            let middle = left + (right - left) / 2;
            if predicate(self.get(middle).expect("in-bounds midpoint")) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left
    }

    /// Explicit cold, linear materialization for compatibility callers. Never
    /// used by tree reads, clones, prefix, append, ranges or partition_point.
    pub(crate) fn as_vec(&self) -> &Vec<T> {
        self.flat.get_or_init(|| self.iter().cloned().collect())
    }

    /// Invalidates the tree before arbitrary edits. No count or cached tree
    /// data is consulted while the returned Vec is the authoritative storage.
    pub(crate) fn as_mut_vec(&mut self) -> &mut Vec<T> {
        self.as_vec();
        self.editable = true;
        self.root = None;
        self.flat.get_mut().expect("materialized Vec")
    }

    /// Moves the cold edited elements into fresh leaves, without cloning T.
    pub(crate) fn repack(&mut self) {
        if self.editable {
            let values = self.flat.take().expect("editable Vec");
            *self = Self::from_vec(values, self.kind);
        }
    }
}

/// Range cursor that walks each leaf as a slice. `front` and `back` are slice
/// iterators over the unvisited elements of the leaf at each end, and `middle`
/// is the logical range between them that neither has loaded yet. A step
/// inside a leaf is a slice iterator's step. Moving forward to the next leaf
/// takes the next of `siblings`, the nodes after the front leaf under its
/// parent, so a forward walk descends the spine once per parent, not once per
/// leaf. Moving backward descends it once per leaf.
///
/// The out-of-line steps take and return the cursor's fields by value and
/// never borrow the cursor, so a loop over it can keep it in registers: the
/// adapters that step element by element (`any`, `find`, `position`,
/// `filter_map`, a `for` loop) then cost about what they cost over a slice.
/// `nth` skips whole leaves and `fold`/`rfold` walk whole leaf slices.
pub(crate) struct Iter<'a, T: Clone, const C: usize> {
    sequence: &'a SharedSequence<T, C>,
    /// Unvisited elements of the leaf at the front.
    front: std::slice::Iter<'a, T>,
    /// The nodes after the front leaf under its parent. While `middle` is not
    /// empty, the first of them begins at `middle.start`.
    siblings: std::slice::Iter<'a, Arc<Node<T, C>>>,
    /// Logical positions after `front`'s and before `back`'s.
    middle: std::ops::Range<usize>,
    /// Unvisited elements of the leaf at the back.
    back: std::slice::Iter<'a, T>,
}

impl<T: Clone, const C: usize> Clone for Iter<'_, T, C> {
    fn clone(&self) -> Self {
        Self {
            sequence: self.sequence,
            front: self.front.clone(),
            siblings: self.siblings.clone(),
            middle: self.middle.clone(),
            back: self.back.clone(),
        }
    }
}

/// What the front of a range loads next: the elements, the nodes after their
/// leaf under its parent, and where the range then starts.
struct FrontSegment<'a, T: Clone, const C: usize> {
    elements: std::slice::Iter<'a, T>,
    siblings: std::slice::Iter<'a, Arc<Node<T, C>>>,
    middle_start: usize,
}

/// The first leaf segment of the non-empty `middle`. `siblings` are the nodes
/// after the leaf that ends at `middle.start`: the first is the next leaf.
/// With none left, or a branch next, the leaf is found from the root.
#[inline(never)]
fn front_segment<'a, T: Clone, const C: usize>(
    sequence: &'a SharedSequence<T, C>,
    mut siblings: std::slice::Iter<'a, Arc<Node<T, C>>>,
    middle: std::ops::Range<usize>,
) -> FrontSegment<'a, T, C> {
    let next_leaf = siblings.next().and_then(|node| match &node.body {
        Body::Leaf(values, _) => Some(values.as_slice()),
        Body::Branch(_) => None,
    });
    let (leaf, start) = match next_leaf {
        Some(leaf) => (leaf, middle.start),
        None => {
            let (leaf, start, rest) = sequence
                .leaf_and_rest_at(middle.start)
                .expect("cursor in bounds");
            siblings = rest.iter();
            (leaf, start)
        }
    };
    let end = (middle.end - start).min(leaf.len());
    FrontSegment {
        elements: leaf[middle.start - start..end].iter(),
        siblings,
        middle_start: start + end,
    }
}

/// The last leaf segment of the non-empty `middle`, and where `middle` then
/// ends.
#[inline(never)]
fn back_segment<T: Clone, const C: usize>(
    sequence: &SharedSequence<T, C>,
    middle: std::ops::Range<usize>,
) -> (std::slice::Iter<'_, T>, usize) {
    let (leaf, start) = sequence.leaf_at(middle.end - 1).expect("cursor in bounds");
    let from = middle.start.max(start) - start;
    (leaf[from..middle.end - start].iter(), start + from)
}

impl<T: Clone, const C: usize> Iter<'_, T, C> {
    /// Loads the first leaf segment of the non-empty `middle` into `front`.
    #[inline(always)]
    fn load_front(&mut self) {
        let segment = front_segment(self.sequence, self.siblings.clone(), self.middle.clone());
        self.front = segment.elements;
        self.siblings = segment.siblings;
        self.middle.start = segment.middle_start;
    }

    /// Loads the last leaf segment of the non-empty `middle` into `back`.
    #[inline(always)]
    fn load_back(&mut self) {
        let (elements, end) = back_segment(self.sequence, self.middle.clone());
        self.back = elements;
        self.middle.end = end;
    }
}

impl<'a, T: Clone, const C: usize> Iterator for Iter<'a, T, C> {
    type Item = &'a T;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.front.next() {
            return Some(value);
        }
        if self.middle.is_empty() {
            return self.back.next();
        }
        self.load_front();
        self.front.next()
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n < self.front.len() {
            return self.front.nth(n);
        }
        let n = n - self.front.len();
        self.front = Default::default();
        if n >= self.middle.len() {
            let n = n - self.middle.len();
            self.middle.start = self.middle.end;
            return self.back.nth(n);
        }
        if n != 0 {
            // The next leaf is no longer the one `siblings` begins with.
            self.middle.start += n;
            self.siblings = Default::default();
        }
        self.load_front();
        self.front.next()
    }
    /// One slice fold per leaf (`count`, `sum`, `for_each`, `last`, ...).
    #[inline]
    fn fold<B, F>(mut self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let mut accumulator = std::mem::take(&mut self.front).fold(init, &mut f);
        while !self.middle.is_empty() {
            self.load_front();
            accumulator = std::mem::take(&mut self.front).fold(accumulator, &mut f);
        }
        self.back.fold(accumulator, f)
    }
}

impl<T: Clone, const C: usize> DoubleEndedIterator for Iter<'_, T, C> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.back.next_back() {
            return Some(value);
        }
        if self.middle.is_empty() {
            return self.front.next_back();
        }
        self.load_back();
        self.back.next_back()
    }
    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n < self.back.len() {
            return self.back.nth_back(n);
        }
        let n = n - self.back.len();
        self.back = Default::default();
        if n >= self.middle.len() {
            let n = n - self.middle.len();
            self.middle.end = self.middle.start;
            return self.front.nth_back(n);
        }
        self.middle.end -= n;
        self.load_back();
        self.back.next_back()
    }
    #[inline]
    fn rfold<B, F>(mut self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        let mut accumulator = std::mem::take(&mut self.back).rfold(init, &mut f);
        while !self.middle.is_empty() {
            self.load_back();
            accumulator = std::mem::take(&mut self.back).rfold(accumulator, &mut f);
        }
        self.front.rfold(accumulator, f)
    }
}
impl<T: Clone, const C: usize> ExactSizeIterator for Iter<'_, T, C> {
    #[inline]
    fn len(&self) -> usize {
        self.front.len() + self.middle.len() + self.back.len()
    }
}
impl<T: Clone, const C: usize> FusedIterator for Iter<'_, T, C> {}

impl<T: Clone, const C: usize> SharedSequence<T, C> {
    /// The elements whose kind is one of `kinds`, with their positions, in
    /// order: what filtering `iter().enumerate()` by kind yields. A subtree
    /// whose counts hold none of `kinds` is skipped whole, so a scan for rare
    /// kinds reads only the leaves that hold them, and within a leaf it reads
    /// the leaf's kinds, not its elements, so it touches only the elements it
    /// yields. Editable storage is filtered element by element. Like `iter`,
    /// it never allocates: the flat encoders that validate history promise no
    /// allocation.
    pub(crate) fn of_kinds(&self, kinds: impl IntoIterator<Item = usize>) -> KindIter<'_, T, C> {
        self.kind_filter(kinds, true)
    }

    /// `of_kinds` reading every leaf, to show that skipping by counts loses
    /// no element.
    #[cfg(test)]
    pub(crate) fn of_kinds_reading_every_leaf(
        &self,
        kinds: impl IntoIterator<Item = usize>,
    ) -> KindIter<'_, T, C> {
        self.kind_filter(kinds, false)
    }

    /// Every element with its position, whatever its kind: what a kind
    /// filter gives with every guard forced open (tests).
    #[cfg(test)]
    pub(crate) fn every_element_with_position(&self) -> KindIter<'_, T, C> {
        let mut iter = self.kind_filter([], false);
        iter.every = true;
        iter
    }

    fn kind_filter(
        &self,
        kinds: impl IntoIterator<Item = usize>,
        skip: bool,
    ) -> KindIter<'_, T, C> {
        const { assert!(C <= KindSet::CAPACITY, "count domain wider than a kind set") };
        assert!(C != 0, "kind filter outside the sequence's count domain");
        let mut wanted = KindSet::default();
        for kind in kinds {
            assert!(kind < C, "kind filter outside the sequence's count domain");
            wanted.insert(kind);
        }
        KindIter {
            kinds: wanted,
            sequence: self,
            skip,
            every: false,
            flat: self.editable,
            leaf: None,
            visited: 0,
            position: 0,
            siblings: &[],
            resume: 0,
        }
    }

    /// A kind filter that yields nothing and reads nothing: for a caller
    /// whose own counts already show that none of its kinds is here.
    pub(crate) fn of_no_kinds(&self) -> KindIter<'_, T, C> {
        KindIter {
            kinds: KindSet::default(),
            sequence: self,
            skip: true,
            every: false,
            flat: false,
            leaf: None,
            visited: 0,
            position: 0,
            siblings: &[],
            resume: usize::MAX,
        }
    }
}

/// The kinds a kind filter reads, one bit each.
#[derive(Clone, Copy, Default)]
struct KindSet([u64; KIND_WORDS]);

/// Words in a kind set: room for 512 kinds.
const KIND_WORDS: usize = 8;

impl KindSet {
    const CAPACITY: usize = KIND_WORDS * 64;

    fn insert(&mut self, kind: usize) {
        self.0[kind / 64] |= 1 << (kind % 64);
    }

    fn contains(&self, kind: usize) -> bool {
        self.0
            .get(kind / 64)
            .is_some_and(|bits| bits & (1 << (kind % 64)) != 0)
    }

    /// Whether `counts` holds any of the kinds.
    fn any_counted<const C: usize>(&self, counts: &[u32; C]) -> bool {
        self.0.iter().enumerate().any(|(word, &bits)| {
            let mut bits = bits;
            while bits != 0 {
                if counts[word * 64 + bits.trailing_zeros() as usize] != 0 {
                    return true;
                }
                bits &= bits - 1;
            }
            false
        })
    }
}

/// A leaf, the position of its first element, and the nodes after it under
/// its parent: each begins where the one before it ends.
type LeafAt<'a, T, const C: usize> = (&'a [T], usize, &'a [Arc<Node<T, C>>]);

/// A leaf's node, the position of its first element, and the nodes after it
/// under its parent.
type WantedLeaf<'a, T, const C: usize> = (&'a Node<T, C>, usize, &'a [Arc<Node<T, C>>]);

/// The first leaf under `children` that counts a wanted kind and begins at or
/// after `from`. `start` is where the first child begins.
///
/// The recursion is as deep as the tree is tall, which is at most 12 levels.
/// Every child but a branch's last is full, because `from_vec` fills them,
/// `append` adds a sibling only once the last child is full, and `prefix`
/// cuts only the last. So a root `h` levels above its leaves holds more than
/// `LEAF * FANOUT^(h - 1)` elements, and no length a `usize` can count needs
/// more than 12 levels.
fn first_wanted_leaf<'a, T: Clone, const C: usize>(
    children: &'a [Arc<Node<T, C>>],
    mut start: usize,
    from: usize,
    kinds: &KindSet,
    skip: bool,
) -> Option<WantedLeaf<'a, T, C>> {
    for (at, child) in children.iter().enumerate() {
        let end = start + child.len;
        if end > from && (!skip || kinds.any_counted(&child.counts)) {
            match &child.body {
                Body::Leaf(..) => {
                    if start >= from {
                        return Some((child.as_ref(), start, &children[at + 1..]));
                    }
                }
                Body::Branch(grandchildren) => {
                    let found = first_wanted_leaf(grandchildren, start, from, kinds, skip);
                    if found.is_some() {
                        return found;
                    }
                }
            }
        }
        start = end;
    }
    None
}

/// Cursor over the leaves that count a wanted kind, in order. It steps
/// through the leaves under one parent, then finds the next wanted leaf from
/// the root, so it passes over a subtree that counts none of its kinds
/// without reading it. Inside a leaf it reads the leaf's kinds and touches
/// only the elements it yields. It holds its kinds inline and nothing per
/// level of the tree: a scan allocates nothing, and the cursor is cheap to
/// build and to move even when it yields nothing.
pub(crate) struct KindIter<'a, T: Clone, const C: usize> {
    kinds: KindSet,
    sequence: &'a SharedSequence<T, C>,
    /// Whether subtrees counting none of `kinds` are skipped.
    skip: bool,
    /// Whether every element is yielded, whatever its kind (tests).
    every: bool,
    /// Whether the current leaf is the editable Vec, which keeps no kinds.
    flat: bool,
    /// The current tree leaf, how many of its elements were visited, and the
    /// position of the first unvisited one.
    leaf: Option<&'a Node<T, C>>,
    visited: usize,
    position: usize,
    /// The nodes after the current leaf under its parent, and the position
    /// where the first of them begins. Once they are spent, the search for
    /// the next leaf resumes from that position.
    siblings: &'a [Arc<Node<T, C>>],
    resume: usize,
}

impl<'a, T: Clone, const C: usize> KindIter<'a, T, C> {
    fn wanted(&self, node: &Node<T, C>) -> bool {
        !self.skip || self.kinds.any_counted(&node.counts)
    }

    /// The unvisited elements of the current leaf and, in a tree leaf, their
    /// kinds.
    fn unvisited(&self) -> (&'a [T], &'a [u16]) {
        if self.flat {
            let values = self.sequence.flat.get().expect("editable Vec");
            return (&values[self.visited..], &[]);
        }
        match self.leaf.map(|node| &node.body) {
            Some(Body::Leaf(values, tags)) => (&values[self.visited..], &tags[self.visited..]),
            _ => (&[], &[]),
        }
    }

    /// Moves to the first wanted leaf that begins at or after `resume`,
    /// found from the root. False, for good, when there is none.
    fn seek(&mut self) -> bool {
        let tree = match self.sequence.editable {
            // Cold editable storage is one leaf, which the filter began with.
            true => None,
            false => self.sequence.root.as_deref(),
        };
        let found = match tree {
            Some(root) if self.resume < root.len && self.wanted(root) => match &root.body {
                // A leaf root is read from its start or not at all.
                Body::Leaf(..) => (self.resume == 0).then_some((root, 0, &[][..])),
                Body::Branch(children) => {
                    first_wanted_leaf(children, 0, self.resume, &self.kinds, self.skip)
                }
            },
            _ => None,
        };
        let Some((leaf, start, siblings)) = found else {
            self.resume = usize::MAX;
            return false;
        };
        self.flat = false;
        self.leaf = Some(leaf);
        self.visited = 0;
        self.position = start;
        self.siblings = siblings;
        self.resume = start + leaf.len;
        true
    }
}

impl<'a, T: Clone, const C: usize> Iterator for KindIter<'a, T, C> {
    type Item = (usize, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (values, tags) = self.unvisited();
            let found = if self.every {
                (!values.is_empty()).then_some(0)
            } else if self.flat {
                let kind = self.sequence.kind;
                values
                    .iter()
                    .position(|value| self.kinds.contains(kind(value)))
            } else {
                tags.iter()
                    .position(|&tag| self.kinds.contains(usize::from(tag)))
            };
            if let Some(at) = found {
                let position = self.position + at;
                self.visited += at + 1;
                self.position = position + 1;
                return Some((position, &values[at]));
            }
            self.visited += values.len();
            self.position += values.len();
            match self.siblings.split_first() {
                Some((node, rest)) => match &node.body {
                    // The next leaf under the same parent begins at `resume`.
                    Body::Leaf(values, _) => {
                        self.siblings = rest;
                        let start = self.resume;
                        self.resume += values.len();
                        if self.wanted(node) {
                            self.leaf = Some(node.as_ref());
                            self.visited = 0;
                            self.position = start;
                        }
                    }
                    // Not a leaf: its leaves are found from the root.
                    Body::Branch(_) => self.siblings = &[],
                },
                None => {
                    if !self.seek() {
                        return None;
                    }
                }
            }
        }
    }
}
impl<T: Clone, const C: usize> FusedIterator for KindIter<'_, T, C> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn kind(value: &usize) -> usize {
        value % 3
    }
    fn oracle(tree: &SharedSequence<usize, 3>, values: &[usize]) {
        assert_eq!(tree.len(), values.len());
        assert_eq!(tree.is_empty(), values.is_empty());
        assert!(tree.iter().eq(values.iter()));
        assert!(tree.iter().rev().eq(values.iter().rev()));
        assert_eq!(tree.get(values.len()), None);
        let mut counts = [0; 3];
        for value in values {
            counts[kind(value)] += 1;
        }
        assert_eq!(tree.counts(), counts);
        for index in [
            0,
            1,
            63,
            64,
            65,
            2047,
            2048,
            2049,
            values.len().saturating_sub(1),
        ] {
            assert_eq!(tree.get(index), values.get(index));
        }
        kinds_oracle(tree, values, kind);
    }
    /// Every tree leaf keeps exactly its elements' kinds, in order, as the
    /// sequence's kind function gives them, and none when `C == 0`.
    fn assert_leaf_kinds<T: Clone, const C: usize>(tree: &SharedSequence<T, C>) {
        fn walk<T: Clone, const C: usize>(node: &Node<T, C>, kind: fn(&T) -> usize) {
            match &node.body {
                Body::Leaf(values, kinds) => {
                    let expected: Vec<u16> = values
                        .iter()
                        .filter_map(|value| kind_index::<T, C>(value, kind))
                        .map(|index| u16::try_from(index).unwrap())
                        .collect();
                    assert_eq!(kinds, &expected, "a leaf keeps its elements' kinds");
                }
                Body::Branch(children) => children.iter().for_each(|child| walk(child, kind)),
            }
        }
        if let Some(root) = &tree.root {
            walk(root, tree.kind);
        }
    }
    fn kinds_oracle<const C: usize>(
        tree: &SharedSequence<usize, C>,
        values: &[usize],
        kind: fn(&usize) -> usize,
    ) {
        assert_leaf_kinds(tree);
        for kinds in [
            vec![],
            vec![0],
            vec![C - 1],
            vec![C - 1, 0],
            (0..C).collect(),
        ] {
            let expected = values
                .iter()
                .enumerate()
                .filter(|(_, value)| kinds.contains(&kind(value)));
            assert!(
                tree.of_kinds(kinds.clone()).eq(expected.clone()),
                "kinds {kinds:?}"
            );
            assert!(
                tree.of_kinds_reading_every_leaf(kinds.clone()).eq(expected),
                "kinds {kinds:?} reading every leaf"
            );
        }
        assert!(tree
            .every_element_with_position()
            .eq(values.iter().enumerate()));
        assert!(tree.of_no_kinds().next().is_none());
        // A spent filter stays spent.
        let mut spent = tree.of_kinds(0..C);
        assert_eq!(spent.by_ref().count(), values.len());
        assert!(spent.next().is_none() && spent.next().is_none());
    }

    /// A kind filter is built and moved once for every guarded check, most
    /// of which find nothing. Up to 128 bytes the compiler copies a value
    /// inline on any x86-64; past that each move is a call to `memcpy`.
    #[test]
    fn a_kind_filter_is_small_enough_to_move_inline() {
        assert!(std::mem::size_of::<KindIter<'_, usize, 3>>() <= 128);
    }

    #[test]
    fn vec_oracle_at_leaf_spine_and_root_boundaries() {
        for len in [0, 1, 63, 64, 65, 2047, 2048, 2049, 65535, 65536, 65537] {
            let values: Vec<_> = (0..len).collect();
            let built = SharedSequence::<_, 3>::from_vec(values.clone(), kind);
            let mut pushed = SharedSequence::<_, 3>::new(kind);
            for value in &values {
                pushed.push(*value);
            }
            oracle(&built, &values);
            oracle(&pushed, &values);
            for cut in [
                0,
                1,
                63,
                64,
                65,
                len / 2,
                len.saturating_sub(1),
                len,
                len + 17,
            ] {
                let mut prefix = built.prefix(cut);
                let mut expected = values[..cut.min(len)].to_vec();
                oracle(&prefix, &expected);
                for value in len..len + 70 {
                    prefix.push(value);
                    expected.push(value);
                }
                oracle(&prefix, &expected);
                let mut truncated = pushed.clone();
                truncated.truncate(cut);
                oracle(&truncated, &values[..cut.min(len)]);
            }
            oracle(&built, &values);
        }
    }

    #[test]
    fn mixed_double_ended_ranges_and_large_skips_match_slices() {
        let values: Vec<_> = (0..4099).collect();
        let tree = SharedSequence::<_, 3>::from_vec(values.clone(), kind);
        for (start, end) in [(0, 0), (0, 4099), (63, 2049), (64, 65), (2047, 4098)] {
            let mut a = tree.range(start..end);
            let mut b = values[start..end].iter();
            for n in [0, 2, 63, 0, 64, 100, 1, usize::MAX] {
                assert_eq!(a.len(), b.len());
                assert_eq!(a.next(), b.next());
                assert_eq!(a.next_back(), b.next_back());
                assert_eq!(a.nth(n), b.nth(n));
                assert_eq!(a.nth_back(n), b.nth_back(n));
                assert_eq!(a.size_hint(), b.size_hint());
            }
            assert!(a.eq(b));
        }
        assert!(tree.range(63..=65).eq(values[63..=65].iter()));
        assert!(tree
            .range((Bound::Excluded(63), Bound::Unbounded))
            .eq(values[64..].iter()));
        for target in [0, 1, 63, 64, 65, 2048, 4098, 4099, 5000] {
            assert_eq!(
                tree.partition_point(|x| *x < target),
                values.partition_point(|x| *x < target)
            );
        }
    }

    #[derive(Debug)]
    struct Counted {
        value: usize,
        clones: Arc<AtomicUsize>,
    }
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.clones.fetch_add(1, Ordering::Relaxed);
            Self {
                value: self.value,
                clones: self.clones.clone(),
            }
        }
    }
    fn counted_kind(value: &Counted) -> usize {
        value.value % 3
    }

    #[test]
    fn fork_prefix_and_append_clone_at_most_one_bounded_leaf() {
        let clones = Arc::new(AtomicUsize::new(0));
        let values = (0..65539)
            .map(|value| Counted {
                value,
                clones: clones.clone(),
            })
            .collect();
        let tree = SharedSequence::<_, 3>::from_vec(values, counted_kind);
        assert_eq!(clones.load(Ordering::Relaxed), 0);
        for cut in [
            1,
            63,
            64,
            65,
            2047,
            2048,
            2049,
            65536,
            65538,
            65539,
            usize::MAX,
        ] {
            clones.store(0, Ordering::Relaxed);
            let mut prefix = tree.prefix(cut);
            assert!(clones.load(Ordering::Relaxed) < LEAF);
            let mut fork = prefix.clone();
            let before = clones.load(Ordering::Relaxed);
            fork.push(Counted {
                value: 99,
                clones: clones.clone(),
            });
            assert!(clones.load(Ordering::Relaxed) - before < LEAF);
            assert_eq!(prefix.len(), cut.min(tree.len()));
            assert_eq!(fork.len(), prefix.len() + 1);
            let before = clones.load(Ordering::Relaxed);
            prefix.push(Counted {
                value: 100,
                clones: clones.clone(),
            });
            assert!(clones.load(Ordering::Relaxed) - before < LEAF);
            // Appending to a fork copies the shared last leaf with its kinds
            // and leaves the original's alone.
            assert_leaf_kinds(&fork);
            assert_leaf_kinds(&prefix);
            assert_eq!(fork.counts(), {
                let mut counts = [0; 3];
                fork.iter()
                    .for_each(|value| counts[counted_kind(value)] += 1);
                counts
            });
        }
        assert_leaf_kinds(&tree);
        assert_eq!(tree.len(), 65539);
    }

    #[test]
    fn cold_arbitrary_edits_repack_and_zero_count_domain() {
        let mut tree = SharedSequence::<_, 3>::from_vec((0..2050).collect(), kind);
        let original = tree.clone();
        let mut expected: Vec<_> = (0..2050).collect();
        assert_eq!(tree.as_vec(), &expected);
        let edit = |values: &mut Vec<usize>| {
            values.remove(64);
            values.insert(3, 10001);
            values[2048] = 11;
            values.retain(|value| value % 7 != 0);
            values.extend([17, 18, 19]);
        };
        edit(tree.as_mut_vec());
        edit(&mut expected);
        oracle(&tree, &expected);
        oracle(&tree.clone(), &expected);
        oracle(&tree.prefix(65), &expected[..65]);
        tree.repack();
        assert!(!tree.editable && tree.flat.get().is_none());
        oracle(&tree, &expected);
        tree.as_mut_vec().push(33);
        expected.push(33);
        tree.push(34);
        expected.push(34);
        oracle(&tree, &expected);
        tree.as_mut_vec().reverse();
        expected.reverse();
        tree.truncate(66);
        expected.truncate(66);
        oracle(&tree, &expected);
        oracle(&original, &(0..2050).collect::<Vec<_>>());
        let mut no_counts =
            SharedSequence::<usize, 0>::from_vec((0..3000).collect(), |_| panic!("kind called"));
        no_counts.push(3000);
        no_counts.truncate(65);
        no_counts.as_mut_vec().push(66);
        assert_eq!(no_counts.counts(), [0u32; 0]);
        no_counts.repack();
        assert_eq!(no_counts.len(), 66);
        assert_leaf_kinds(&no_counts);
    }

    #[test]
    fn repack_moves_cold_elements_without_cloning() {
        let clones = Arc::new(AtomicUsize::new(0));
        let mut tree = SharedSequence::<_, 3>::from_vec(
            (0..3000)
                .map(|value| Counted {
                    value,
                    clones: clones.clone(),
                })
                .collect(),
            counted_kind,
        );
        tree.as_mut_vec().swap(0, 2999);
        clones.store(0, Ordering::Relaxed);
        tree.repack();
        assert_eq!(clones.load(Ordering::Relaxed), 0);
        assert_eq!(tree.get(0).unwrap().value, 2999);
    }

    /// Kind 2 is rare, so most leaves and spines count none of it and the
    /// kind filter skips them; every shape must still match a plain filter.
    #[test]
    fn rare_kind_filters_match_a_filtered_scan_in_every_shape() {
        fn sparse(value: &usize) -> usize {
            if value % 997 == 5 {
                2
            } else {
                value % 2
            }
        }
        for len in [0, 1, 5, 6, 64, 65, 2048, 2049, 70_000] {
            let values: Vec<_> = (0..len).collect();
            let built = SharedSequence::<_, 3>::from_vec(values.clone(), sparse);
            let mut pushed = SharedSequence::<_, 3>::new(sparse);
            for value in &values {
                pushed.push(*value);
            }
            kinds_oracle(&built, &values, sparse);
            kinds_oracle(&pushed, &values, sparse);
            for cut in [0, 6, 64, 1000, len / 2, len] {
                let mut prefix = built.prefix(cut);
                let mut expected = values[..cut.min(len)].to_vec();
                kinds_oracle(&prefix, &expected, sparse);
                let shared = prefix.clone();
                for value in [5, 7, 1002, 1999] {
                    prefix.push(value);
                    expected.push(value);
                }
                kinds_oracle(&prefix, &expected, sparse);
                kinds_oracle(&shared, &values[..cut.min(len)], sparse);
            }
            let mut edited = built.clone();
            edited.as_mut_vec().insert(0, 5);
            let mut expected = values.clone();
            expected.insert(0, 5);
            kinds_oracle(&edited, &expected, sparse);
            edited.repack();
            kinds_oracle(&edited, &expected, sparse);
        }
    }

    /// The bound on the kind filter's descent: leaves share one depth, every
    /// child but a branch's last is full, and a root above its leaves has a
    /// full first child and another, however the tree was built or cut.
    #[test]
    fn trees_are_no_taller_than_their_length_needs() {
        fn height<const C: usize>(node: &Node<usize, C>) -> u32 {
            let Body::Branch(children) = &node.body else {
                return 0;
            };
            let below = height(&children[0]);
            let full = LEAF * FANOUT.pow(below);
            for (at, child) in children.iter().enumerate() {
                assert_eq!(height(child), below, "leaves at one depth");
                if at + 1 < children.len() {
                    assert_eq!(child.len, full, "child {at} of {} is full", children.len());
                }
            }
            below + 1
        }
        fn check(tree: &SharedSequence<usize, 3>) {
            let Some(root) = &tree.root else {
                return;
            };
            let levels = height(root);
            if let Body::Branch(children) = &root.body {
                assert!(children.len() >= 2, "a branch root has two children");
                assert!(tree.len() > LEAF * FANOUT.pow(levels - 1));
            }
            assert!(levels <= 12, "one frame of the descent per level");
        }
        for len in [0, 1, 64, 65, 2048, 2049, 70_000] {
            let values: Vec<_> = (0..len).collect();
            let built = SharedSequence::<_, 3>::from_vec(values.clone(), kind);
            check(&built);
            let mut pushed = SharedSequence::<_, 3>::new(kind);
            for value in &values {
                pushed.push(*value);
            }
            check(&pushed);
            for cut in [
                1,
                63,
                64,
                65,
                2047,
                2048,
                2049,
                len / 2,
                len.saturating_sub(1),
            ] {
                for source in [&built, &pushed] {
                    let mut prefix = source.prefix(cut);
                    check(&prefix);
                    for value in 0..3000 {
                        prefix.push(value);
                    }
                    check(&prefix);
                    let mut truncated = source.clone();
                    truncated.truncate(cut);
                    check(&truncated);
                }
            }
            let mut edited = built.clone();
            edited.as_mut_vec().push(7);
            edited.repack();
            check(&edited);
        }
    }

    /// Every way of stepping a range cursor against a slice iterator: single
    /// steps from both ends, skips inside a leaf, across leaves and past the
    /// end, clones, folds and searches of what is left, in the interleavings
    /// a fixed pseudo-random sequence reaches, over ranges that start and end
    /// inside, at and across leaf boundaries.
    fn cursor_oracle(tree: &SharedSequence<usize, 3>, values: &[usize], seed: &mut u64) {
        let mut draw = |bound: usize| {
            *seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (*seed >> 33) as usize % bound
        };
        let len = values.len();
        for _ in 0..24 {
            let start = draw(len + 1);
            let end = start + draw((len - start).min(300) + 1);
            let mut a = tree.range(start..end);
            let mut b = values[start..end].iter();
            loop {
                assert_eq!(a.len(), b.len());
                assert_eq!(a.size_hint(), b.size_hint());
                match draw(24) {
                    0..=5 => assert_eq!(a.next(), b.next()),
                    6..=11 => assert_eq!(a.next_back(), b.next_back()),
                    12..=14 => {
                        let n = draw(70);
                        assert_eq!(a.nth(n), b.nth(n));
                    }
                    15..=17 => {
                        let n = draw(70);
                        assert_eq!(a.nth_back(n), b.nth_back(n));
                    }
                    18 => {
                        let n = draw(len + 2);
                        assert_eq!(a.nth(n), b.nth(n));
                    }
                    19 => {
                        let n = draw(len + 2);
                        assert_eq!(a.nth_back(n), b.nth_back(n));
                    }
                    20 | 21 => {
                        // What is left, by step and by fold, in both orders.
                        assert!(a.clone().eq(b.clone()));
                        assert!(a.clone().rev().eq(b.clone().rev()));
                        let forwards = a.clone().fold(Vec::new(), |mut seen, value| {
                            seen.push(*value);
                            seen
                        });
                        assert!(forwards.iter().eq(b.clone()));
                        let backwards = a.clone().rfold(Vec::new(), |mut seen, value| {
                            seen.push(*value);
                            seen
                        });
                        assert!(backwards.iter().eq(b.clone().rev()));
                    }
                    _ => {
                        // The adapters that stop early step element by element.
                        let target = start + draw(end - start + 1);
                        assert_eq!(
                            a.clone().position(|value| *value == target),
                            b.clone().position(|value| *value == target)
                        );
                        assert_eq!(
                            a.clone().rfind(|value| **value <= target),
                            b.clone().rfind(|value| **value <= target)
                        );
                        assert_eq!(
                            a.clone().any(|value| *value == target),
                            b.clone().any(|value| *value == target)
                        );
                    }
                }
                if b.len() == 0 {
                    assert_eq!(a.len(), 0);
                    assert_eq!(a.next(), None);
                    assert_eq!(a.next_back(), None);
                    assert_eq!(a.nth(3), None);
                    assert_eq!(a.nth_back(3), None);
                    break;
                }
            }
        }
    }

    #[test]
    fn range_cursor_steps_match_a_slice_iterator_in_every_form() {
        let mut seed = 0x5eed;
        for len in [
            0, 1, 2, 63, 64, 65, 127, 128, 129, 200, 2047, 2048, 2049, 4099,
        ] {
            let values: Vec<_> = (0..len).collect();
            let built = SharedSequence::<_, 3>::from_vec(values.clone(), kind);
            let mut pushed = SharedSequence::<_, 3>::new(kind);
            for value in &values {
                pushed.push(*value);
            }
            cursor_oracle(&built, &values, &mut seed);
            cursor_oracle(&pushed, &values, &mut seed);
            // A cut leaves a short last leaf, which later pushes fill.
            let cut = len / 2 + 1;
            let mut prefix = built.prefix(cut);
            let mut expected = values[..cut.min(len)].to_vec();
            for value in len..len + 70 {
                prefix.push(value);
                expected.push(value);
            }
            cursor_oracle(&prefix, &expected, &mut seed);
            // Editable storage is one slice, and repacked it is a tree again.
            let mut edited = built.clone();
            edited.as_mut_vec().push(len);
            let mut expected = values.clone();
            expected.push(len);
            cursor_oracle(&edited, &expected, &mut seed);
            edited.repack();
            cursor_oracle(&edited, &expected, &mut seed);
        }
    }

    #[test]
    fn invalid_range_panics_like_a_slice() {
        let tree = SharedSequence::<_, 3>::from_vec(vec![0, 1, 2], kind);
        for bounds in [
            (Bound::Included(3), Bound::Excluded(2)),
            (Bound::Unbounded, Bound::Included(3)),
            (Bound::Excluded(usize::MAX), Bound::Unbounded),
        ] {
            assert!(std::panic::catch_unwind(|| {
                tree.range(bounds);
            })
            .is_err());
        }
    }

    #[test]
    fn common_prefix_matches_vec_at_boundaries_and_divergences() {
        fn expected(a: &[usize], b: &[usize]) -> usize {
            a.iter().zip(b).take_while(|(a, b)| a == b).count()
        }
        for len in [0, 1, 63, 64, 65, 2047, 2048, 2049, 65536, 65537] {
            let values: Vec<_> = (0..len).collect();
            let tree = SharedSequence::<_, 3>::from_vec(values.clone(), kind);
            let mut separate = SharedSequence::<_, 3>::new(kind);
            for value in &values {
                separate.push(*value);
            }
            assert_eq!(tree.common_prefix_len(&tree.clone()), len);
            assert_eq!(tree.common_prefix_len(&separate), len);
            for cut in [0, 1, 63, 64, 65, 2047, 2048, 2049, len / 2, len, len + 1] {
                let cut = cut.min(len);
                let prefix = tree.prefix(cut);
                assert_eq!(tree.common_prefix_len(&prefix), cut);
                assert_eq!(prefix.common_prefix_len(&tree), cut);
                // Appending at a cut replaces only the bounded last leaf, and
                // can grow a root after prefix() has collapsed its ancestors.
                let mut fork = prefix.clone();
                let mut altered = values[..cut].to_vec();
                for value in [usize::MAX, 1, 2, 3, 4] {
                    fork.push(value);
                    altered.push(value);
                }
                let common = expected(&values, &altered);
                assert_eq!(tree.common_prefix_len(&fork), common);
                assert_eq!(fork.common_prefix_len(&tree), common);
                let unshared = SharedSequence::<_, 3>::from_vec(altered, kind);
                assert_eq!(tree.common_prefix_len(&unshared), common);
            }
        }
    }

    #[test]
    fn common_prefix_reads_cold_edits_exactly_before_and_after_repack() {
        let tree = SharedSequence::<_, 3>::from_vec((0..4099).collect(), kind);
        let mut edited = tree.clone();
        // A clean materialization must not become the authority for a tree
        // comparison, nor prevent the identity shortcut.
        assert_eq!(edited.as_vec().len(), tree.len());
        assert_eq!(tree.common_prefix_len(&edited), tree.len());
        edited.as_mut_vec()[2048] = 7777;
        for _ in 0..2 {
            assert_eq!(tree.common_prefix_len(&edited), 2048);
            assert_eq!(edited.common_prefix_len(&tree), 2048);
            assert_eq!(edited.common_prefix_len(&edited), edited.len());
            edited.repack();
        }
        edited.as_mut_vec().truncate(64);
        assert_eq!(tree.common_prefix_len(&edited), 64);
        edited.as_mut_vec().clear();
        assert_eq!(edited.common_prefix_len(&tree), 0);
        assert_eq!(tree.common_prefix_len(&edited), 0);
    }

    #[derive(Clone)]
    struct Compared {
        value: usize,
        comparisons: Arc<AtomicUsize>,
    }
    impl PartialEq for Compared {
        fn eq(&self, other: &Self) -> bool {
            self.comparisons.fetch_add(1, Ordering::Relaxed);
            self.value == other.value
        }
    }
    impl Eq for Compared {}

    #[test]
    fn shared_prefix_skips_whole_subtrees_not_just_equal_leaves() {
        let comparisons = Arc::new(AtomicUsize::new(0));
        let len = 262147;
        let tree = SharedSequence::<_, 0>::from_vec(
            (0..len)
                .map(|value| Compared {
                    value,
                    comparisons: comparisons.clone(),
                })
                .collect(),
            |_| panic!("zero-count sequence must not classify values"),
        );
        let mut visits = 0;
        assert_eq!(
            tree.common_prefix_len_inner(&tree.clone(), || visits += 1),
            len
        );
        assert_eq!(visits, 1);
        assert_eq!(comparisons.load(Ordering::Relaxed), 0);
        for cut in [
            1,
            63,
            64,
            2047,
            2048,
            2049,
            65535,
            65536,
            65537,
            len - 1,
            len,
        ] {
            let prefix = tree.prefix(cut);
            let mut appended = prefix.clone();
            appended.push(Compared {
                value: usize::MAX,
                comparisons: comparisons.clone(),
            });
            for (left, right, expected) in [
                (&tree, &prefix, cut),
                (&prefix, &tree, cut),
                (&tree, &appended, cut),
                (&appended, &tree, cut),
                (&prefix, &appended, cut),
                (&appended, &prefix, cut),
            ] {
                comparisons.store(0, Ordering::Relaxed);
                let mut visits = 0;
                assert_eq!(
                    left.common_prefix_len_inner(right, || visits += 1),
                    expected
                );
                assert!(comparisons.load(Ordering::Relaxed) <= LEAF, "cut={cut}");
                // This is far below the >4096 leaf visits a leaf-by-leaf
                // identity traversal would need for the long shared prefix.
                assert!(visits <= 4 * FANOUT, "cut={cut}, visited={visits}");
                assert!(left.flat.get().is_none() && right.flat.get().is_none());
            }
        }
    }
}
