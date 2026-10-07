#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Sys;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    pub rss_bytes     : Option<u64>,
    pub threads       : Option<u64>,
    pub open_fds      : Option<u64>,
    pub process_ticks : Option<u64>,
    pub machine_ticks : Option<u64>,
    pub cpus          : usize,
}
