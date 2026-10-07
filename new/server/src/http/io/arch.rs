use hyper::upgrade::OnUpgrade;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Io;

#[derive(Clone, Copy, Debug, Default)]
pub struct LocalExec;

pub struct Upgrading {
    pub(super) pending : OnUpgrade,
}
