use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

pub struct Db {
    pub(super) path       : PathBuf,
    pub(super) connection : Mutex<Connection>,
}
