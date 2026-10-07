pub struct Few <T, const N: usize> {
    pub(super) items : [Option<T>; N],
    pub(super) len   : usize,
}
