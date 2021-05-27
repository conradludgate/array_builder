use array_builder::ArrayBuilder;

struct ArrayIterator<I: Iterator, const N: usize> {
    builder: ArrayBuilder<I::Item, N>,
    iter: I,
}

impl<I: Iterator, const N: usize> Iterator for ArrayIterator<I, N> {
    type Item = [I::Item; N];

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY:
        // ArrayBuilder methods are safe as long as the lengths are respected
        // Push can only be called when length < N
        // Build can only be called when length == N
        unsafe {
            for _ in self.builder.len()..N {
                // If the underlying iterator returns None
                // then we won't have enough data to return a full array
                // so we can bail early and return None
                self.builder.push_unchecked(self.iter.next()?);
            }
            // At this point, we must have N elements in the builder
            // So extract the array and reset the builder for the next call
            Some(self.builder.take().build_unchecked())
        }
    }
}

impl<I: Iterator, const N: usize> ArrayIterator<I, N> {
    pub fn new(i: impl IntoIterator<IntoIter=I>) -> Self {
        Self {
            builder: ArrayBuilder::new(),
            iter: i.into_iter(),
        }
    }

    pub fn remaining(&self) -> &[I::Item] {
        &self.builder
    }
}

#[test]
fn array_iterator() {
    let mut i = ArrayIterator::new(0..10);
    assert_eq!(Some([0, 1, 2, 3]), i.next());
    assert_eq!(Some([4, 5, 6, 7]), i.next());
    assert_eq!(None, i.next());
    assert_eq!(&[8, 9], i.remaining());
}
