use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{Connection, params};

use super::types::{Execution, ExecutionStatus};

pub fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".cache")
        .join("illef-workflow")
        .join("storage.sqlite")
}

pub fn logs_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".cache")
        .join("illef-workflow")
        .join("logs")
}

pub fn open_db() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open database: {}", path.display()))?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS executions (
            id          TEXT PRIMARY KEY,
            workflow    TEXT NOT NULL,
            status      TEXT NOT NULL,
            started_at  INTEGER NOT NULL,
            finished_at INTEGER,
            exit_code   INTEGER,
            log_path    TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_executions_workflow
            ON executions(workflow, started_at DESC);",
    )?;
    Ok(())
}

pub fn insert_execution(conn: &Connection, exec: &Execution) -> Result<()> {
    conn.execute(
        "INSERT INTO executions (id, workflow, status, started_at, finished_at, exit_code, log_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            exec.id,
            exec.workflow,
            exec.status.as_str(),
            exec.started_at.timestamp(),
            exec.finished_at.map(|t| t.timestamp()),
            exec.exit_code,
            exec.log_path,
        ],
    )?;
    Ok(())
}

pub fn update_execution_finished(
    conn: &Connection,
    id: &str,
    status: ExecutionStatus,
    finished_at: DateTime<Utc>,
    exit_code: i32,
) -> Result<()> {
    conn.execute(
        "UPDATE executions SET status = ?1, finished_at = ?2, exit_code = ?3 WHERE id = ?4",
        params![
            status.as_str(),
            finished_at.timestamp(),
            exit_code,
            id,
        ],
    )?;
    Ok(())
}

pub fn get_executions(conn: &Connection, workflow: &str, limit: usize) -> Result<Vec<Execution>> {
    let mut stmt = conn.prepare(
        "SELECT id, workflow, status, started_at, finished_at, exit_code, log_path
         FROM executions
         WHERE workflow = ?1
         ORDER BY started_at DESC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![workflow, limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i32>>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;

    let mut executions = Vec::new();
    for row in rows {
        let (id, workflow, status_str, started_ts, finished_ts, exit_code, log_path) = row?;
        executions.push(Execution {
            id,
            workflow,
            status: ExecutionStatus::from_str(&status_str)
                .unwrap_or(ExecutionStatus::Failed),
            started_at: Utc.timestamp_opt(started_ts, 0).unwrap(),
            finished_at: finished_ts.map(|ts| Utc.timestamp_opt(ts, 0).unwrap()),
            exit_code,
            log_path,
        });
    }

    Ok(executions)
}

pub fn get_last_execution(conn: &Connection, workflow: &str) -> Result<Option<Execution>> {
    let mut execs = get_executions(conn, workflow, 1)?;
    Ok(execs.pop())
}

pub fn get_execution_by_id(conn: &Connection, id: &str) -> Result<Option<Execution>> {
    let mut stmt = conn.prepare(
        "SELECT id, workflow, status, started_at, finished_at, exit_code, log_path
         FROM executions WHERE id = ?1",
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i32>>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;

    if let Some(row) = rows.next() {
        let (id, workflow, status_str, started_ts, finished_ts, exit_code, log_path) = row?;
        Ok(Some(Execution {
            id,
            workflow,
            status: ExecutionStatus::from_str(&status_str).unwrap_or(ExecutionStatus::Failed),
            started_at: Utc.timestamp_opt(started_ts, 0).unwrap(),
            finished_at: finished_ts.map(|ts| Utc.timestamp_opt(ts, 0).unwrap()),
            exit_code,
            log_path,
        }))
    } else {
        Ok(None)
    }
}
