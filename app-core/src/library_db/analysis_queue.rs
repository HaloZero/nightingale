//! Persistent analyzer queue.
//!
//! Backs `analyzer::AnalysisQueue`. Each song hash gets one row carrying its
//! current status (`queued`, `analyzing` with a percentage, or `failed` with a
//! message, a `FailureKind`, and whether it's been acknowledged). The legacy
//! JSON store (`<data>/analysis_queue.json`) is imported once on first boot
//! and renamed to `.json.bak`.

use diesel::QueryResult;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel::upsert::excluded;

use crate::error::NightingaleError;

use super::connection::{with_conn, with_conn_mut};
use super::schema::analysis_queue;

pub(super) fn import_legacy_analysis_queue_json() -> Result<(), NightingaleError> {
    let path = crate::cache::analysis_queue_path();
    if !path.is_file() {
        return Ok(());
    }
    let data = match std::fs::read_to_string(&path) {
        Ok(data) => data,
        Err(_) => return Ok(()),
    };
    let value: serde_json::Value = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let Some(entries) = value.get("entries").and_then(|entries| entries.as_object()) else {
        return Ok(());
    };
    with_conn_mut(|conn| {
        conn.transaction::<_, NightingaleError, _>(|conn| {
            for (hash, value) in entries {
                let (status, pct, message) = parse_legacy_queue_status(value);
                // Legacy rows predate FailureKind/acknowledgement; kind is
                // left `None` (read back as `FailureKind::Other`) and
                // treated as unacked.
                upsert_queue(conn, hash, status, pct, message.as_deref(), None, false)?;
            }
            Ok(())
        })
    })?;
    let _ = std::fs::rename(&path, path.with_extension("json.bak"));
    Ok(())
}

fn parse_legacy_queue_status(
    value: &serde_json::Value,
) -> (&'static str, Option<i64>, Option<String>) {
    let Some(object) = value.as_object() else {
        return ("queued", None, None);
    };
    if object.contains_key("Queued") {
        return ("queued", None, None);
    }
    if let Some(percent) = object.get("Analyzing").and_then(|value| value.as_u64()) {
        return ("analyzing", Some(percent as i64), None);
    }
    if let Some(message) = object.get("Failed").and_then(|value| value.as_str()) {
        return ("failed", None, Some(message.to_string()));
    }
    ("queued", None, None)
}

fn upsert_queue(
    conn: &mut SqliteConnection,
    file_hash: &str,
    status: &str,
    analyzing_pct: Option<i64>,
    failed_message: Option<&str>,
    failed_kind: Option<&str>,
    failed_acknowledged: bool,
) -> QueryResult<()> {
    diesel::insert_into(analysis_queue::table)
        .values((
            analysis_queue::file_hash.eq(file_hash),
            analysis_queue::status.eq(status),
            analysis_queue::analyzing_pct.eq(analyzing_pct),
            analysis_queue::failed_message.eq(failed_message),
            analysis_queue::failed_kind.eq(failed_kind),
            analysis_queue::failed_acknowledged.eq(failed_acknowledged),
        ))
        .on_conflict(analysis_queue::file_hash)
        .do_update()
        .set((
            analysis_queue::status.eq(excluded(analysis_queue::status)),
            analysis_queue::analyzing_pct.eq(excluded(analysis_queue::analyzing_pct)),
            analysis_queue::failed_message.eq(excluded(analysis_queue::failed_message)),
            analysis_queue::failed_kind.eq(excluded(analysis_queue::failed_kind)),
            analysis_queue::failed_acknowledged.eq(excluded(analysis_queue::failed_acknowledged)),
        ))
        .execute(conn)?;
    Ok(())
}

pub(crate) fn analysis_queue_upsert_row(
    file_hash: &str,
    status: &str,
    analyzing_pct: Option<i64>,
    failed_message: Option<&str>,
    failed_kind: Option<&str>,
    failed_acknowledged: bool,
) -> Result<(), NightingaleError> {
    with_conn_mut(|conn| {
        upsert_queue(
            conn,
            file_hash,
            status,
            analyzing_pct,
            failed_message,
            failed_kind,
            failed_acknowledged,
        )?;
        Ok(())
    })
}

pub(crate) fn analysis_queue_delete(file_hash: &str) -> Result<(), NightingaleError> {
    with_conn_mut(|conn| {
        diesel::delete(analysis_queue::table.filter(analysis_queue::file_hash.eq(file_hash)))
            .execute(conn)?;
        Ok(())
    })
}

pub(crate) fn analysis_queue_clear() -> Result<(), NightingaleError> {
    with_conn_mut(|conn| {
        diesel::delete(analysis_queue::table).execute(conn)?;
        Ok(())
    })
}

/// Acknowledges exactly `file_hashes`, gated on `failed_kind = kind` so a
/// hash that's since failed differently isn't wrongly acknowledged.
pub(crate) fn analysis_queue_acknowledge_failures(
    kind: &str,
    file_hashes: &[String],
) -> Result<(), NightingaleError> {
    with_conn_mut(|conn| {
        diesel::update(
            analysis_queue::table
                .filter(analysis_queue::status.eq("failed"))
                .filter(analysis_queue::failed_kind.eq(kind))
                .filter(analysis_queue::file_hash.eq_any(file_hashes)),
        )
        .set(analysis_queue::failed_acknowledged.eq(true))
        .execute(conn)?;
        Ok(())
    })
}

type AnalysisQueueRow = (
    String,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    bool,
);

pub(crate) fn analysis_queue_load_rows() -> Result<Vec<AnalysisQueueRow>, NightingaleError> {
    with_conn(|conn| {
        Ok(analysis_queue::table
            .select((
                analysis_queue::file_hash,
                analysis_queue::status,
                analysis_queue::analyzing_pct,
                analysis_queue::failed_message,
                analysis_queue::failed_kind,
                analysis_queue::failed_acknowledged,
            ))
            .load(conn)?)
    })
}

pub(crate) fn analysis_queue_save_rows(rows: &[AnalysisQueueRow]) -> Result<(), NightingaleError> {
    with_conn_mut(|conn| {
        conn.transaction::<_, NightingaleError, _>(|conn| {
            diesel::delete(analysis_queue::table).execute(conn)?;
            for (hash, status, percent, message, kind, acknowledged) in rows {
                upsert_queue(
                    conn,
                    hash,
                    status,
                    *percent,
                    message.as_deref(),
                    kind.as_deref(),
                    *acknowledged,
                )?;
            }
            Ok(())
        })
    })
}
