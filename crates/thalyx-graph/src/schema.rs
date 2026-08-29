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
            line        INTEGER NOT NULL,
            -- How this edge was learned: `import` from a declaration the file
            -- wrote, `symbol` from a name it used that only one file in the
            -- tree declares. Kept per row rather than in a second table
            -- because a caller asking "what depends on this" needs both and
            -- must be able to tell them apart — an import is what the file
            -- says about itself, a symbol edge is what the tree says about it.
            via         TEXT NOT NULL DEFAULT 'import'
        );

        CREATE INDEX IF NOT EXISTS edges_from ON edges (from_path);
        CREATE INDEX IF NOT EXISTS edges_to   ON edges (to_path);

        CREATE TABLE IF NOT EXISTS tags (
            path TEXT NOT NULL,
            tag  TEXT NOT NULL,
            PRIMARY KEY (path, tag)
        );

        CREATE INDEX IF NOT EXISTS tags_by_tag ON tags (tag);

        -- Where a name comes from. `Superficie-para-el-LLM.md`, punto C2: a
        -- result that says "function `login`, src/auth.rs, line 40" costs a
        -- fraction of the tokens of the textual matches for the same word, and
        -- has no false positives from comments.
        CREATE TABLE IF NOT EXISTS symbols (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL,
            -- `function`, `type`, `constant`: the coarseness five languages
            -- share. The language is on the node, so nothing is lost.
            kind  TEXT NOT NULL,
            path  TEXT NOT NULL,
            line  INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS symbols_by_name ON symbols (name);

        -- Where a name is used, which is the half `grep` gets wrong. Only names
        -- that are defined somewhere in this tree are recorded: an occurrence of
        -- `println` is not something anybody will ever ask for, and keeping it
        -- would trade a table that answers questions for one that is mostly
        -- vocabulary.
        CREATE TABLE IF NOT EXISTS mentions (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL,
            path  TEXT NOT NULL,
            line  INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS mentions_by_name ON mentions (name);

        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;

    // An index written before `via` existed has the old shape, and
    // `CREATE TABLE IF NOT EXISTS` does not touch it. Rather than delete the
    // index — which would make a version upgrade look like a lost cache — the
    // column is added and every row already there is an import, which is what
    // the old build could record.
    //
    // Asked for rather than attempted-and-ignored: a swallowed error here
    // would hide a real database fault behind the one it was written to
    // tolerate, which is rule 10 in the place it is cheapest to get wrong.
    if !has_column(connection, "edges", "via")? {
        connection
            .execute_batch("ALTER TABLE edges ADD COLUMN via TEXT NOT NULL DEFAULT 'import';")?;
    }

    Ok(())
}

/// Whether a table already has a column, asked of the database itself.
fn has_column(connection: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
