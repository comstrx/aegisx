use std::path::Path;
use rusqlite::{Connection, params};
use crate::core::{error::{AppFail, AppResult},time::now_ms};
use super::Model;

pub(super) struct Journal { gate: crate::core::WriteGate, connection: Connection, artifact: String }
impl Journal {
    pub fn open ( path: &Path, artifact: &str, synchronous: &str, gate: crate::core::WriteGate ) -> AppResult<Self> {
        let connection=Connection::open(path).or_fail("Cannot open analysis journal")?;
        connection.busy_timeout(std::time::Duration::from_millis(500)).or_fail("Cannot set analysis journal timeout")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS analysis_jobs (
            id INTEGER PRIMARY KEY, request_id TEXT NOT NULL, artifact TEXT NOT NULL,
            created_ms INTEGER NOT NULL, features TEXT NOT NULL, state TEXT NOT NULL,
            score REAL, finished_ms INTEGER);
            CREATE INDEX IF NOT EXISTS analysis_pending ON analysis_jobs(state,id);").or_fail("Cannot initialize analysis journal")?;
        connection.pragma_update(None,"synchronous",if synchronous=="full" {"FULL"} else {"NORMAL"}).or_fail("Cannot set journal durability")?;
        Ok(Self {gate,connection,artifact:artifact.into()})
    }
    pub fn prepare ( &mut self, jobs: &[(String,super::Vector)] ) -> AppResult<Vec<i64>> {
        if jobs.is_empty() {return Ok(Vec::new());}
        let encoded=jobs.iter().map(|(_,features)|serde_json::to_string(features.as_slice()))
            .collect::<Result<Vec<_>,_>>().or_fail("Cannot encode analysis features")?;
        let _guard=self.gate.lock();
        let transaction=self.connection.transaction().or_fail("Cannot begin analysis journal batch")?;
        let mut ids=Vec::with_capacity(jobs.len());
        {
            let mut insert=transaction.prepare_cached("INSERT INTO analysis_jobs(request_id,artifact,created_ms,features,state) VALUES (?1,?2,?3,?4,'pending')")
                .or_fail("Cannot prepare analysis journal")?;
            for ((request,_),features) in jobs.iter().zip(encoded) {
                insert.execute(params![request,self.artifact,now_ms() as i64,features]).or_fail("Cannot journal analysis job")?;
                ids.push(transaction.last_insert_rowid());
            }
        }
        transaction.commit().or_fail("Cannot commit analysis jobs")?;
        Ok(ids)
    }
    pub fn complete ( &mut self, results: &[(i64,&str,Option<f32>)] ) -> AppResult<()> {
        if results.is_empty() {return Ok(());}
        let _guard=self.gate.lock();
        let transaction=self.connection.transaction().or_fail("Cannot begin analysis completion")?;
        {
            let mut update=transaction.prepare_cached("UPDATE analysis_jobs SET state=?2,score=?3,finished_ms=?4 WHERE id=?1")
                .or_fail("Cannot prepare analysis completion")?;
            for (id,state,score) in results {
                update.execute(params![id,state,score,now_ms() as i64]).or_fail("Cannot complete analysis job")?;
            }
        }
        let last=results.last().unwrap().0;
        let first=results.first().unwrap().0;
        if last/128!=first.saturating_sub(1)/128 {
            transaction.execute("DELETE FROM analysis_jobs WHERE id < ?1-10000 AND state!='pending'",[last])
                .or_fail("Cannot retain analysis journal")?;
        }
        transaction.commit().or_fail("Cannot commit analysis completion")
    }
    pub fn recover ( &mut self, model: &mut Model ) -> AppResult<u64> {
        let mut count=0;
        loop {
            let rows={
                let mut query=self.connection.prepare("SELECT id,artifact,features FROM analysis_jobs WHERE state='pending' ORDER BY id LIMIT 64")
                    .or_fail("Cannot inspect pending analysis")?;
                query.query_map([],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))
                    .or_fail("Cannot read pending analysis")?.collect::<Result<Vec<_>,_>>().or_fail("Invalid pending analysis")?
            };
            if rows.is_empty() {break;}
            let results=rows.into_iter().map(|(id,artifact,encoded)|{
                let values=if encoded.len()<=16384 {serde_json::from_str::<Vec<f32>>(&encoded).ok()} else {None};
                let score=values.and_then(|values|values.try_into().ok()).filter(|_|artifact==self.artifact && model.info.input_schema.is_none())
                    .and_then(|features|model.score(features).ok());
                (id,if score.is_some() {"recovered_observe"} else {"recovery_unavailable"},score)
            }).collect::<Vec<_>>();
            self.complete(&results)?;
            count+=results.len() as u64;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_commit_is_atomic_and_interrupted_jobs_recover_without_callbacks () {
        let path=std::env::temp_dir().join(format!("aegisx-journal-{}.db",uuid::Uuid::new_v4()));
        let mut model=Model::load().unwrap();
        let mut journal=Journal::open(&path,&model.info.artifact_sha256,"full",Default::default()).unwrap();
        journal.connection.execute_batch("CREATE TRIGGER reject_fixture BEFORE INSERT ON analysis_jobs
            WHEN NEW.request_id='reject' BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        assert!(journal.prepare(&[("ok".into(),[0.0; crate::module::inference::FEATURE_COUNT]),("reject".into(),[0.0; crate::module::inference::FEATURE_COUNT])]).is_err());
        let count:i64=journal.connection.query_row("SELECT COUNT(*) FROM analysis_jobs",[],|row|row.get(0)).unwrap();
        assert_eq!(count,0);
        journal.connection.execute_batch("DROP TRIGGER reject_fixture;").unwrap();
        assert_eq!(journal.prepare(&[("first".into(),[0.0; crate::module::inference::FEATURE_COUNT]),("second".into(),[1.0; crate::module::inference::FEATURE_COUNT])]).unwrap().len(),2);
        drop(journal);
        let mut journal=Journal::open(&path,&model.info.artifact_sha256,"full",Default::default()).unwrap();
        assert_eq!(journal.recover(&mut model).unwrap(),2);
        let count:i64=journal.connection.query_row("SELECT COUNT(*) FROM analysis_jobs WHERE state='recovery_unavailable' AND score IS NULL",[],|row|row.get(0)).unwrap();
        assert_eq!(count,2);
        drop(journal);
        for suffix in ["","-wal","-shm"] {let _=std::fs::remove_file(format!("{}{suffix}",path.display()));}
    }
}
