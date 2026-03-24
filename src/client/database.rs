use rusqlite::Connection;
use serde_json::{json, Value};

use crate::error::{Result, SxmcError};

pub fn inspect_sqlite(source: &str, table: Option<&str>, search: Option<&str>) -> Result<Value> {
    let conn = Connection::open(source).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to open SQLite database '{}': {}",
            source, e
        ))
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT name, type, COALESCE(sql, '') \
             FROM sqlite_schema \
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|e| SxmcError::Other(format!("Failed to inspect SQLite schema: {}", e)))?;

    let search_lower = search.map(|value| value.to_ascii_lowercase());
    let explicit_table = table.map(|value| value.to_string());
    let rows = stmt
        .query_map([], |row| {
            let name: String = row.get(0)?;
            let object_type: String = row.get(1)?;
            let sql: String = row.get(2)?;
            Ok((name, object_type, sql))
        })
        .map_err(|e| SxmcError::Other(format!("Failed to query SQLite schema: {}", e)))?;

    let mut entries = Vec::new();
    for row in rows {
        let (name, object_type, sql) =
            row.map_err(|e| SxmcError::Other(format!("Failed to read SQLite schema row: {}", e)))?;

        if let Some(expected) = explicit_table.as_deref() {
            if name != expected {
                continue;
            }
        }

        if let Some(pattern) = search_lower.as_deref() {
            let haystack = format!("{} {} {}", name, object_type, sql).to_ascii_lowercase();
            if !haystack.contains(pattern) {
                continue;
            }
        }

        let columns = inspect_sqlite_columns(&conn, &name, &object_type)?;
        entries.push(json!({
            "name": name,
            "object_type": object_type,
            "sql": if sql.is_empty() { Value::Null } else { Value::String(sql) },
            "column_count": columns.len(),
            "columns": columns,
        }));
    }

    if let Some(expected) = explicit_table {
        if entries.is_empty() {
            return Err(SxmcError::Other(format!(
                "Table or view '{}' was not found in SQLite database '{}'.",
                expected, source
            )));
        }
    }

    Ok(json!({
        "discovery_schema": "sxmc_discover_db_v1",
        "source_type": "database",
        "database_type": "sqlite",
        "source": source,
        "selected_table": table,
        "search": search,
        "count": entries.len(),
        "entries": entries,
    }))
}

fn inspect_sqlite_columns(
    conn: &Connection,
    table_name: &str,
    object_type: &str,
) -> Result<Vec<Value>> {
    if object_type != "table" {
        return Ok(Vec::new());
    }

    let pragma = format!(
        "PRAGMA table_info({})",
        sqlite_identifier_literal(table_name)
    );
    let mut stmt = conn.prepare(&pragma).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to inspect columns for '{}': {}",
            table_name, e
        ))
    })?;

    let rows = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            let data_type: String = row.get(2)?;
            let not_null: i64 = row.get(3)?;
            let default_value: Option<String> = row.get(4)?;
            let primary_key_position: i64 = row.get(5)?;
            Ok(json!({
                "name": name,
                "data_type": data_type,
                "not_null": not_null != 0,
                "default": default_value,
                "primary_key": primary_key_position != 0,
                "primary_key_position": primary_key_position,
            }))
        })
        .map_err(|e| {
            SxmcError::Other(format!(
                "Failed to read columns for '{}': {}",
                table_name, e
            ))
        })?;

    let mut columns = Vec::new();
    for row in rows {
        columns.push(row.map_err(|e| {
            SxmcError::Other(format!(
                "Failed to decode SQLite column for '{}': {}",
                table_name, e
            ))
        })?);
    }
    Ok(columns)
}

fn sqlite_identifier_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
