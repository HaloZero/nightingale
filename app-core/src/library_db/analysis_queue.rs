//! Persistent analyzer queue.
//!
//! Backs `analyzer::AnalysisQueue`. Each song hash gets one row carrying its
//! current status (`queued`, `analyzing` with a percentage, or `failed` with a
//! message, a `FailureKind`, and whether it's been acknowledged). The legacy
//! JSON store (`<data>/analysis_queue.json`) is imported once on first boot
//! and renamed to `.json.bak`.

use rusqlite::params;

use super::connection::{with_conn, with_conn_mut};

pub(super) fn import_legacy_analysis_queue_json() -> rusqlite::Result<()> {
    let path = crate::cache::analysis_queue_path();
    if !path.is_file() {
        return Ok(());
    }
    let data = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let v: serde_json::Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(entries) = v.get("entries").and_then(|e| e.as_object()) else {
        return Ok(());
    };
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        for (hash, val) in entries {
            let (st, pct, msg) = parse_legacy_queue_status(val);
            // Legacy rows predate FailureKind/acknowledgement; kind is left
            // NULL (read back as FailureKind::Other) and treated as unacked.
            tx.execute(
                "INSERT INTO analysis_queue (file_hash, status, analyzing_pct, failed_message, failed_kind, failed_acknowledged)
                 VALUES (?1, ?2, ?3, ?4, NULL, 0)
                 ON CONFLICT(file_hash) DO UPDATE SET
                   status = excluded.status,
                   analyzing_pct = excluded.analyzing_pct,
                   failed_message = excluded.failed_message,
                   failed_kind = excluded.failed_kind,
                   failed_acknowledged = excluded.failed_acknowledged",
                params![hash, st, pct, msg],
            )?;
        }
        tx.commit()?;
        Ok(())
    })?;
    let _ = std::fs::rename(&path, path.with_extension("json.bak"));
    Ok(())
}

fn parse_legacy_queue_status(v: &serde_json::Value) -> (&'static str, Option<i64>, Option<String>) {
    let Some(o) = v.as_object() else {
        return ("queued", None, None);
    };
    if o.contains_key("Queued") {
        return ("queued", None, None);
    }
    if let Some(n) = o.get("Analyzing").and_then(|x| x.as_u64()) {
        return ("analyzing", Some(n as i64), None);
    }
    if let Some(s) = o.get("Failed").and_then(|x| x.as_str()) {
        return ("failed", None, Some(s.to_string()));
    }
    ("queued", None, None)
}

fn upsert_queue_in_tx(
    tx: &rusqlite::Transaction<'_>,
    file_hash: &str,
    status: &str,
    analyzing_pct: Option<i64>,
    failed_message: Option<&str>,
    failed_kind: Option<&str>,
    failed_acknowledged: bool,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO analysis_queue (file_hash, status, analyzing_pct, failed_message, failed_kind, failed_acknowledged)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(file_hash) DO UPDATE SET
           status = excluded.status,
           analyzing_pct = excluded.analyzing_pct,
           failed_message = excluded.failed_message,
           failed_kind = excluded.failed_kind,
           failed_acknowledged = excluded.failed_acknowledged",
        params![file_hash, status, analyzing_pct, failed_message, failed_kind, failed_acknowledged],
    )?;
    Ok(())
}

pub fn analysis_queue_upsert_row(
    file_hash: &str,
    status: &str,
    analyzing_pct: Option<i64>,
    failed_message: Option<&str>,
    failed_kind: Option<&str>,
    failed_acknowledged: bool,
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        upsert_queue_in_tx(
            &tx,
            file_hash,
            status,
            analyzing_pct,
            failed_message,
            failed_kind,
            failed_acknowledged,
        )?;
        tx.commit()?;
        Ok(())
    })
}

pub fn analysis_queue_delete(file_hash: &str) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute(
            "DELETE FROM analysis_queue WHERE file_hash = ?",
            [file_hash],
        )?;
        Ok(())
    })
}

pub fn analysis_queue_clear() -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        c.execute("DELETE FROM analysis_queue", [])?;
        Ok(())
    })
}

/// Acknowledges exactly `file_hashes`, gated on `failed_kind = kind` so a
/// hash that's since failed differently isn't wrongly acknowledged.
pub fn analysis_queue_acknowledge_failures(kind: &str, file_hashes: &[String]) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        for file_hash in file_hashes {
            tx.execute(
                "UPDATE analysis_queue SET failed_acknowledged = 1
                 WHERE status = 'failed' AND failed_kind = ?1 AND file_hash = ?2",
                params![kind, file_hash],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
}

pub fn analysis_queue_load_rows()
-> rusqlite::Result<Vec<(String, String, Option<i64>, Option<String>, Option<String>, bool)>> {
    with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT file_hash, status, analyzing_pct, failed_message, failed_kind, failed_acknowledged FROM analysis_queue",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<bool>>(5)?.unwrap_or(false),
            ))
        })?;
        rows.collect()
    })
}

pub fn analysis_queue_save_rows(
    rows: &[(String, String, Option<i64>, Option<String>, Option<String>, bool)],
) -> rusqlite::Result<()> {
    with_conn_mut(|c| {
        let tx = c.transaction()?;
        tx.execute("DELETE FROM analysis_queue", [])?;
        for (hash, st, pct, msg, kind, acknowledged) in rows {
            upsert_queue_in_tx(
                &tx,
                hash,
                st.as_str(),
                *pct,
                msg.as_deref(),
                kind.as_deref(),
                *acknowledged,
            )?;
        }
        tx.commit()?;
        Ok(())
    })
}
