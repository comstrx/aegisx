use rusqlite::Connection;
use crate::core::error::{AppFail,AppResult};

pub(super) fn prepare ( connection: &Connection ) -> AppResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS event_journeys (request_id TEXT PRIMARY KEY,last_id INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS event_journeys_last ON event_journeys(last_id);").or_fail("Cannot initialize journey index")?;
    let missing:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM events LIMIT 1) AND NOT EXISTS(SELECT 1 FROM event_journeys LIMIT 1)",[],|row|row.get(0))
        .or_fail("Cannot inspect journey migration")?;
    if missing {
        connection.execute_batch("INSERT INTO event_journeys SELECT request_id,MAX(id) FROM events GROUP BY request_id;")
            .or_fail("Cannot migrate legacy lifecycle index")?;
    }
    Ok(())
}
pub(super) fn prune ( connection: &mut Connection, retention: usize ) -> AppResult<()> {
    let transaction=connection.transaction().or_fail("Cannot begin retention")?;
    let cutoff:i64=transaction.query_row("SELECT COALESCE(MAX(id),0)-?1 FROM events",[retention as i64],|row|row.get(0))
        .or_fail("Cannot find retention boundary")?;
    // An indexed, bounded list replaces repeated GROUP BY scans of the full event table.
    transaction.execute("DELETE FROM events WHERE request_id IN
        (SELECT request_id FROM event_journeys WHERE last_id<=?1 ORDER BY last_id LIMIT 1024)",[cutoff])
        .or_fail("Cannot retain whole journeys")?;
    transaction.execute("DELETE FROM event_journeys WHERE request_id IN
        (SELECT request_id FROM event_journeys WHERE last_id<=?1 ORDER BY last_id LIMIT 1024)",[cutoff])
        .or_fail("Cannot retain journey index")?;
    transaction.commit().or_fail("Cannot commit retention")
}
