use std::path::Path;
use rusqlite::{Connection, OpenFlags};

use crate::core::error::{AppError, AppFail, AppResult};
use super::{Event, Store};

impl Store {

    pub fn inspect ( path: &Path, request: Option<&str>, limit: usize ) -> AppResult<Vec<Event>> {

        if !(1..=10000).contains(&limit) { return Err(AppError::invalid("Inspect limit must be 1..10000")); }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .or_fail("Cannot read lifecycle database")?;
        let mut statement = connection.prepare(
            "SELECT payload FROM (
                 SELECT id, payload FROM events WHERE (?1 IS NULL OR request_id = ?1)
                 ORDER BY id DESC LIMIT ?2
             ) ORDER BY id"
        ).or_fail("Cannot prepare lifecycle query")?;
        let rows = statement.query_map(rusqlite::params![request, limit as i64], |row| row.get::<_, String>(0))
            .or_fail("Cannot query lifecycle events")?;
        let mut events = Vec::new();
        for row in rows {
            events.push(serde_json::from_str(&row.or_fail("Cannot read lifecycle row")?).or_fail("Invalid stored event")?);
        }

        Ok(events)

    }

}
