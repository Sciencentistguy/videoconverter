use crate::{ARGS, tv::TVOptions};

use std::error::Error;

use rusqlite::{Connection, OptionalExtension, params};
use tap::Tap;
use tracing::*;

#[derive(Debug)]
pub struct Db {
    connection: Connection,
}

impl Db {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let path = if let Some(db_path) = &ARGS.db_path {
            db_path.clone()
        } else {
            dirs::cache_dir()
                .unwrap()
                .join("videoconverter/videoconverter.sqlite")
        };
        if path.is_dir() {
            return Err(format!("Database path {path:?} is a directory, expected a file").into());
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
    pub fn find(&self, title: &str) -> Option<TVOptions> {
        let title = title
            .to_lowercase()
            .replace(|c: char| !c.is_ascii_alphanumeric() && c != ' ', " ");

        // Split into words and join with " OR " to allow partial matches
        let query = title.split_whitespace().collect::<Vec<_>>().join(" OR ");

        trace!(query = %query, "Searching DB for title match.");

        let (title, season, episode) = self
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
                    let title = row.get(0)?;
                    let season = row.get(1)?;
                    let episode = row.get(2)?;
                    Ok((title, season, episode))
                },
            )
            .optional()
            .unwrap()
            .tap(|res| {
                if let Some((title, season, episode)) = res {
                    trace!(%title, %season, %episode, "Found matching entry in DB.");
                } else {
                    trace!("No matching TV show found in DB.");
                }
            })?;

        Some(TVOptions {
            title,
            season,
            episode,
        })
    }

    pub fn write(&self, state: &TVOptions) {
        trace!(title = %state.title, season = %state.season, episode = %state.episode, "Writing TV show state to DB.");
        self.connection
            .execute(
                "INSERT INTO entries (title, season, episode)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(title) DO UPDATE SET
                        season  = excluded.season,
                        episode = excluded.episode;
                    ",
                params![state.title, state.season, state.episode],
            )
            .unwrap();
    }
}
