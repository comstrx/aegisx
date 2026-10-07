#[derive(Clone, Debug)]
pub struct Rng {
    pub(super) state : [u64; 4],
}
