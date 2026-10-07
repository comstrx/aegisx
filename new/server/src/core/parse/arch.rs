#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Json;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Lua;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sandbox {
    pub source_bytes : usize,
    pub memory_bytes : usize,
    pub instructions : u32,
}
