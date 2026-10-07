use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, Deserialize};
use crate::core::{error::{AppError, AppFail, AppResult}, time::now_ms};

#[derive(Clone,Serialize,Deserialize)]
pub struct Cancellation {
    pub action_id: String,
    pub request_id: String,
    pub route: String,
    pub state: String,
    pub reason: String,
    pub created_ms: u64,
    pub expires_ms: u64,
}
impl Cancellation {
    pub fn new ( request_id: String, route: String, reason: String, ttl_ms: u64 ) -> Self {
        let now=now_ms();
        Self { action_id:uuid::Uuid::new_v4().to_string(),request_id,route,state:"requested".into(),
            reason,created_ms:now,expires_ms:now+ttl_ms }
    }
}
pub(super) fn create ( connection: &Connection, value: Cancellation ) -> AppResult<Cancellation> {
    let mut statement=connection.prepare("SELECT payload FROM cancellations WHERE request_id=?1 AND route=?2 AND expires_ms>?3 ORDER BY created_ms DESC LIMIT 1")
        .or_fail("Cannot prepare cancellation")?;
    let previous: Option<String>=statement.query_row(params![value.request_id,value.route,now_ms() as i64],|row|row.get(0)).optional().or_fail("Cannot read cancellation")?;
    if let Some(previous)=previous { return serde_json::from_str(&previous).or_fail("Invalid cancellation"); }
    connection.execute("DELETE FROM cancellations WHERE expires_ms<=?1",[now_ms() as i64]).or_fail("Cannot prune cancellation history")?;
    let count: i64=connection.query_row("SELECT COUNT(*) FROM cancellations",[],|row|row.get(0)).or_fail("Cannot count cancellations")?;
    if count>=10000 { return Err(AppError::invalid("Cancellation budget exhausted")); }
    let payload=serde_json::to_string(&value).or_fail("Cannot serialize cancellation")?;
    connection.execute("INSERT INTO cancellations VALUES (?1,?2,?3,?4,?5,?6)",params![
        value.action_id,value.request_id,value.route,value.created_ms as i64,value.expires_ms as i64,payload]).or_fail("Cannot persist cancellation")?;
    Ok(value)
}
pub(super) fn acknowledge ( connection: &Connection, id: &str, state: &str ) -> AppResult<Cancellation> {
    if !matches!(state,"accepted"|"cancelled"|"too_late"|"unsupported") { return Err(AppError::invalid("Invalid cancellation outcome")); }
    let payload: String=connection.query_row("SELECT payload FROM cancellations WHERE action_id=?1",[id],|row|row.get(0)).or_fail("Unknown cancellation")?;
    let mut value: Cancellation=serde_json::from_str(&payload).or_fail("Invalid cancellation")?;
    if value.state==state { return Ok(value); }
    if value.expires_ms<=now_ms() || !matches!(value.state.as_str(),"requested"|"accepted") {
        return Err(AppError::invalid("Cancellation outcome is expired or final"));
    }
    value.state=state.into();
    connection.execute("UPDATE cancellations SET payload=?2 WHERE action_id=?1",
        params![id,serde_json::to_string(&value).or_fail("Cannot encode cancellation outcome")?]).or_fail("Cannot persist cancellation outcome")?;
    Ok(value)
}
pub(super) fn list ( connection: &Connection ) -> AppResult<Vec<Cancellation>> {
    let mut query=connection.prepare("SELECT payload FROM cancellations WHERE expires_ms>?1 ORDER BY CASE WHEN json_extract(payload,'$.state') IN ('requested','accepted') THEN 0 ELSE 1 END, created_ms ASC LIMIT 200").or_fail("Cannot list cancellations")?;
    let rows=query.query_map([now_ms() as i64],|row|row.get::<_,String>(0)).or_fail("Cannot query cancellations")?;
    rows.map(|row|serde_json::from_str(&row.or_fail("Cannot read cancellation")?).or_fail("Invalid cancellation")).collect()
}
