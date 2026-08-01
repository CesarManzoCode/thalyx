//! The index schema.
//!
//! Kept in one place and applied on open, so the shape of the index is
//! readable at a glance rather than scattered through the code that uses it.

use rusqlite::Connection;

pub fn apply(connection: &Connection) -> rusqlite::Result<()> {
    // WAL so a reader is never blocked by a rebuild in progress. The index is
    // a cache, so a reader seeing the previous contents mid-rebuild is exactly
    // the right behaviour — it is stale, and the freshness check says so.
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;

    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS nodes (
            path      TEXT PRIMARY KEY,
            language  TEXT,
            size      INTEGER NOT NULL,
            -- Recorded so freshness can be checked without reading contents.
            mtime_ns  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS edges (
            id          INTEGER PRIMARY KEY,
            from_path   TEXT NOT NULL,
            -- The reference as written, kept even when it resolves to nothing:
            -- "this file imports numpy" is information, and inventing a target
            -- would be worse than admitting it points outside the tree.
            raw_target  TEXT NOT NULL,
            to_path     TEXT,
            line        INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS edges_from ON edges (from_path);
        CREATE INDEX IF NOT EXISTS edges_to   ON edges (to_path);

        CREATE TABLE IF NOT EXISTS tags (
            path TEXT NOT NULL,
            tag  TEXT NOT NULL,
            PRIMARY KEY (path, tag)
        );

        CREATE INDEX IF NOT EXISTS tags_by_tag ON tags (tag);

        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;

    Ok(())
}
