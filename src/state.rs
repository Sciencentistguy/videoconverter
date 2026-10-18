use crate::{ARGS, Result, tv::TVOptions};

use color_eyre::eyre::eyre;
use rusqlite::{Connection, OptionalExtension, params};
use tabular::{Table, row};
use tap::Tap;
use tracing::*;

#[derive(Debug)]
pub struct Db {
    connection: Connection,
}

impl Db {
    pub fn new() -> Result<Self> {
        let path = if let Some(db_path) = &ARGS.db_path {
            db_path.clone()
        } else {
            dirs::cache_dir()
                .ok_or_else(|| eyre!("Could not determine a cache directory; pass --statefile"))?
                .join("videoconverter/videoconverter.sqlite")
        };
        if path.is_dir() {
            return Err(eyre!(
                "Database path {path:?} is a directory, expected a file"
            ));
        }
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;

        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
                      title   TEXT    PRIMARY KEY,
                      season  INTEGER,
                      episode INTEGER
                  ) STRICT;
                  CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
                      title,
                      content='entries',
                      content_rowid='rowid',
                      tokenize='porter'  -- removes punctuation & normalises words
                  );
                  
                  -- Triggers to keep FTS index in sync with the content table
                  CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
                    INSERT INTO entries_fts(rowid, title) VALUES (new.rowid, new.title);
                  END;
                  CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
                    INSERT INTO entries_fts(entries_fts, rowid, title) VALUES('delete', old.rowid, old.title);
                  END;
                  CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE ON entries BEGIN
                    INSERT INTO entries_fts(entries_fts, rowid, title) VALUES('delete', old.rowid, old.title);
                    INSERT INTO entries_fts(rowid, title) VALUES (new.rowid, new.title);
                  END;"
        )?;

        Ok(Self { connection })
    }

    pub fn find(&self, title: &str) -> Result<Option<TVOptions>> {
        let title = title
            .to_lowercase()
            .replace(|c: char| !c.is_ascii_alphanumeric() && c != ' ', " ");

        // Split into words and join with " OR " to allow partial matches
        let query = title.split_whitespace().collect::<Vec<_>>().join(" OR ");

        // An empty MATCH expression is an FTS5 syntax error
        if query.is_empty() {
            trace!("Title contains no searchable words.");
            return Ok(None);
        }

        trace!(query = %query, "Searching DB for title match.");

        let found = self
            .connection
            .query_row(
                "SELECT entries.title, entries.season, entries.episode
                     FROM entries_fts
                     JOIN entries ON entries_fts.rowid = entries.rowid
                     WHERE entries_fts.title MATCH ?1
                     ORDER BY rank
                     LIMIT 1;
                    ",
                params![query],
                |row| {
                    Ok(TVOptions {
                        title: row.get(0)?,
                        season: row.get(1)?,
                        episode: row.get(2)?,
                    })
                },
            )
            .optional()?
            .tap(|res| match res {
                Some(TVOptions {
                    title,
                    season,
                    episode,
                }) => trace!(%title, %season, %episode, "Found matching entry in DB."),
                None => trace!("No matching TV show found in DB."),
            });

        Ok(found)
    }

    pub fn write(&self, state: &TVOptions) -> Result<()> {
        trace!(title = %state.title, season = %state.season, episode = %state.episode, "Writing TV show state to DB.");
        self.connection.execute(
            "INSERT INTO entries (title, season, episode)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(title) DO UPDATE SET
                        season  = excluded.season,
                        episode = excluded.episode;
                    ",
            params![state.title, state.season, state.episode],
        )?;
        Ok(())
    }

    pub fn update_episode(&self, title: &str, episode: u32) -> Result<()> {
        trace!(title = %title, episode = %episode, "Updating episode number in DB.");
        self.connection.execute(
            "UPDATE entries
                     SET episode = ?1
                     WHERE title = ?2;
                    ",
            params![episode, title],
        )?;
        Ok(())
    }

    pub fn dump(&self) -> Result<()> {
        let mut stmt = self.connection.prepare(
            "SELECT title, season, episode
                 FROM entries",
        )?;
        let mut rows = stmt.query([])?;
        let mut table = Table::new("{:<} {:<} {:<}");
        table.add_row(row!("Title", "Season", "Episode",));
        table.add_heading("---");
        while let Some(row) = rows.next()? {
            table.add_row(row!(
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?
            ));
        }
        println!("{}", table);
        Ok(())
    }

    pub fn remove_entry(&self, title: &str) -> Result<()> {
        let count = self
            .connection
            .execute("DELETE FROM entries WHERE title = ?1", params![title])?;
        match count {
            0 => println!("Entry for '{}' not found", title),
            1 => println!("Successfully deleted entry for '{}'", title),
            count => println!("Successfully deleted {} entries for '{}'", count, title),
        }

        Ok(())
    }
}
