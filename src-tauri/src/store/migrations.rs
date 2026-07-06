use rusqlite::Connection;

/// Ordered, append-only list of migrations. Never edit a shipped migration —
/// add a new one. Version N is `MIGRATIONS[N-1]`.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_history.sql"),
];

#[cfg(test)]
pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn apply(conn: &Connection) -> Result<(), rusqlite::Error> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    for (idx, sql) in MIGRATIONS.iter().enumerate() {
        let version = idx as i64 + 1;
        if version <= current {
            continue;
        }
        tracing::info!(version, "applying migration");
        // Each migration runs atomically: either the whole script applies and
        // user_version advances, or neither happens.
        conn.execute_batch(&format!(
            "BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"
        ))?;
    }
    Ok(())
}
