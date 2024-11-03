pub(crate) trait LendingIterator {
    type Item<'this>
    where
        Self: 'this;
    fn next(&mut self) -> Option<Self::Item<'_>>;
}

pub(crate) struct WindowsMut<'a, T, const SIZE: usize> {
    slice: &'a mut [T],
    start: usize,
}

impl<'a, T, const SIZE: usize> LendingIterator for WindowsMut<'a, T, SIZE> {
    type Item<'this> = &'this mut [T; SIZE] where 'a: 'this;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        let result = self
            .slice
            .get_mut(self.start..)?
            .get_mut(..SIZE)?
            .try_into()
            .unwrap();
        self.start += 1;
        Some(result)
    }
}

pub(crate) fn windows_mut<T, const SIZE: usize>(slice: &mut [T]) -> WindowsMut<'_, T, SIZE> {
    assert_ne!(SIZE, 0);
    WindowsMut { slice, start: 0 }
}
