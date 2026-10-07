use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, Params, Row};

use crate::core::error::{AppError, AppFail, AppResult};
use super::arch::Db;

impl Db {

    pub fn open ( path: &Path ) -> AppResult<Self> {

        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {

            std::fs::create_dir_all(parent).or_fail_with(|| format!("cannot create {}", parent.display()))?;

        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags).or_fail_with(|| format!("cannot open database {}", path.display()))?;

        connection.busy_timeout(Duration::from_millis(2_000)).or_fail("cannot set busy timeout")?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;").or_fail("cannot configure database")?;

        Ok(Self { path: path.to_path_buf(), connection: Mutex::new(connection) })

    }

    pub fn path ( &self ) -> &Path {

        &self.path

    }

    pub fn batch ( &self, sql: &str ) -> AppResult<()> {

        self.lock().execute_batch(sql).map_err(Self::wrap)

    }

    pub fn execute ( &self, sql: &str, params: impl Params ) -> AppResult<usize> {

        self.lock().execute(sql, params).map_err(Self::wrap)

    }

    pub fn rows <T> ( &self, sql: &str, params: impl Params, map: impl FnMut(&Row<'_>) -> rusqlite::Result<T> ) -> AppResult<Vec<T>> {

        let connection = self.lock();
        let mut statement = connection.prepare_cached(sql).map_err(Self::wrap)?;
        let rows = statement.query_map(params, map).map_err(Self::wrap)?;

        rows.collect::<rusqlite::Result<Vec<T>>>().map_err(Self::wrap)

    }

    pub fn first <T> ( &self, sql: &str, params: impl Params, map: impl FnMut(&Row<'_>) -> rusqlite::Result<T> ) -> AppResult<Option<T>> {

        Ok(self.rows(sql, params, map)?.into_iter().next())

    }

    pub fn checkpoint ( &self ) -> AppResult<()> {

        self.batch("PRAGMA wal_checkpoint(TRUNCATE);")

    }

    fn lock ( &self ) -> std::sync::MutexGuard<'_, Connection> {

        self.connection.lock().unwrap_or_else(|poisoned| poisoned.into_inner())

    }

    fn wrap ( error: rusqlite::Error ) -> AppError {

        AppError::Fail { message: "database".to_string(), source: Box::new(error) }

    }

}
