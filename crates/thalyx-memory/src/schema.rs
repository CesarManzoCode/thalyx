//! The memory schema.
//!
//! Kept in one place, like the graph's, so the shape of what is stored can be
//! read at a glance rather than reassembled from the code that writes it.

use rusqlite::Connection;

pub fn apply(connection: &Connection) -> rusqlite::Result<()> {
    connection.pragma_update(None, "foreign_keys", "ON")?;

    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS records (
            id           INTEGER PRIMARY KEY,
            task         TEXT NOT NULL,
            -- 'fact' or 'note'. The two layers live in one table and are
            -- separated on the way out, so a query can never accidentally
            -- return one believing it is the other.
            layer        TEXT NOT NULL,
            text         TEXT NOT NULL,
            recorded_at  INTEGER NOT NULL,
            -- The embedding, little-endian f32. Which embedder produced it is
            -- in `meta`, and reading with a different one is an error.
            vector       BLOB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS records_by_task ON records (task);

        -- What a fact was recorded against. Facts only: a note is an
        -- inference and there is nothing to check it against.
        --
        -- ON DELETE CASCADE so that deleting notes can never leave witness
        -- rows behind pointing at nothing.
        CREATE TABLE IF NOT EXISTS witnessed (
            record    INTEGER NOT NULL REFERENCES records (id) ON DELETE CASCADE,
            path      TEXT NOT NULL,
            size      INTEGER NOT NULL,
            mtime_ns  INTEGER NOT NULL,
            existed   INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS witnessed_by_record ON witnessed (record);

        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;

    Ok(())
}
