//! SQLite persistence: games, the full move log, and app settings.

use crate::game::{Game, LogEntry, Status, now_ts};
use crate::sudoku::{Difficulty, grid_from_string, grid_to_string};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;

pub struct Db {
    conn: Connection,
}

#[derive(Clone, Debug)]
pub struct GameSummary {
    pub id: i64,
    pub difficulty: Difficulty,
    pub status: Status,
    pub mistakes: u32,
    pub hints: u32,
    pub elapsed: f64,
    pub started_at: i64,
    pub updated_at: i64,
    pub progress: f32,
}

#[derive(Clone, Debug)]
pub struct MoveRow {
    pub seq: i64,
    pub action: String,
    pub cell: Option<usize>,
    pub value: u8,
    pub correct: Option<bool>,
    pub at: i64,
}

fn db_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "RustSudoku", "Sudoku") {
        let dir = dirs.data_dir().to_path_buf();
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir.join("sudoku.db");
        }
    }
    PathBuf::from("sudoku.db")
}

impl Db {
    pub fn open() -> rusqlite::Result<Self> {
        let path = db_path();
        let conn = Connection::open(&path)?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS games (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                difficulty    TEXT    NOT NULL,
                puzzle        TEXT    NOT NULL,
                solution      TEXT    NOT NULL,
                state         TEXT    NOT NULL,
                notes         TEXT    NOT NULL,
                hinted        TEXT    NOT NULL,
                status        TEXT    NOT NULL,
                mistakes      INTEGER NOT NULL DEFAULT 0,
                hints         INTEGER NOT NULL DEFAULT 0,
                moves         INTEGER NOT NULL DEFAULT 0,
                elapsed_secs  REAL    NOT NULL DEFAULT 0,
                started_at    INTEGER NOT NULL,
                updated_at    INTEGER NOT NULL,
                finished_at   INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_games_updated ON games(updated_at DESC);

            CREATE TABLE IF NOT EXISTS moves (
                id       INTEGER PRIMARY KEY AUTOINCREMENT,
                game_id  INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                action   TEXT    NOT NULL,
                cell     INTEGER,
                value    INTEGER NOT NULL DEFAULT 0,
                correct  INTEGER,
                at       INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_moves_game ON moves(game_id, id);

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )
    }

    pub fn insert_game(&self, g: &Game) -> rusqlite::Result<i64> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO games (difficulty, puzzle, solution, state, notes, hinted, status,
                mistakes, hints, moves, elapsed_secs, started_at, updated_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                g.difficulty.key(),
                grid_to_string(&g.givens),
                grid_to_string(&g.solution),
                grid_to_string(&g.cells),
                g.notes_to_string(),
                g.hinted_to_string(),
                g.status.key(),
                g.mistakes,
                g.hints,
                g.moves,
                g.elapsed(),
                g.started_at,
                now,
                g.finished_at,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn save_game(&self, g: &Game) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE games SET state = ?2, notes = ?3, hinted = ?4, status = ?5, mistakes = ?6,
                hints = ?7, moves = ?8, elapsed_secs = ?9, updated_at = ?10, finished_at = ?11
             WHERE id = ?1",
            params![
                g.id,
                grid_to_string(&g.cells),
                g.notes_to_string(),
                g.hinted_to_string(),
                g.status.key(),
                g.mistakes,
                g.hints,
                g.moves,
                g.elapsed(),
                now_ts(),
                g.finished_at,
            ],
        )?;
        Ok(())
    }

    pub fn log_moves(&self, game_id: i64, entries: &[LogEntry]) -> rusqlite::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO moves (game_id, action, cell, value, correct, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let now = now_ts();
            for e in entries {
                stmt.execute(params![
                    game_id,
                    e.action,
                    e.cell.map(|c| c as i64),
                    e.value,
                    e.correct,
                    now
                ])?;
            }
        }
        tx.commit()
    }

    pub fn load_game(&self, id: i64) -> rusqlite::Result<Option<Game>> {
        self.conn
            .query_row(
                "SELECT id, difficulty, puzzle, solution, state, notes, hinted, status, mistakes,
                        hints, moves, elapsed_secs, started_at, finished_at
                 FROM games WHERE id = ?1",
                [id],
                |r| {
                    Ok(Game::restore(
                        r.get(0)?,
                        Difficulty::from_key(&r.get::<_, String>(1)?),
                        grid_from_string(&r.get::<_, String>(2)?),
                        grid_from_string(&r.get::<_, String>(3)?),
                        grid_from_string(&r.get::<_, String>(4)?),
                        Game::notes_from_string(&r.get::<_, String>(5)?),
                        Game::hinted_from_string(&r.get::<_, String>(6)?),
                        Status::from_key(&r.get::<_, String>(7)?),
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                        r.get(11)?,
                        r.get(12)?,
                        r.get(13)?,
                    ))
                },
            )
            .optional()
    }

    pub fn latest_in_progress(&self) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM games WHERE status = 'in_progress' ORDER BY updated_at DESC, id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn list_games(&self) -> rusqlite::Result<Vec<GameSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, difficulty, status, mistakes, hints, elapsed_secs, started_at,
                    updated_at, puzzle, state, solution
             FROM games ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let puzzle = grid_from_string(&r.get::<_, String>(8)?);
            let state = grid_from_string(&r.get::<_, String>(9)?);
            let solution = grid_from_string(&r.get::<_, String>(10)?);
            let to_fill = puzzle.iter().filter(|&&v| v == 0).count().max(1);
            let filled = (0..81)
                .filter(|&i| puzzle[i] == 0 && state[i] == solution[i])
                .count();
            Ok(GameSummary {
                id: r.get(0)?,
                difficulty: Difficulty::from_key(&r.get::<_, String>(1)?),
                status: Status::from_key(&r.get::<_, String>(2)?),
                mistakes: r.get(3)?,
                hints: r.get(4)?,
                elapsed: r.get(5)?,
                started_at: r.get(6)?,
                updated_at: r.get(7)?,
                progress: filled as f32 / to_fill as f32,
            })
        })?;
        rows.collect()
    }

    pub fn moves_for(&self, game_id: i64) -> rusqlite::Result<Vec<MoveRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, action, cell, value, correct, at FROM moves WHERE game_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([game_id], |r| {
            Ok(MoveRow {
                seq: r.get(0)?,
                action: r.get(1)?,
                cell: r.get::<_, Option<i64>>(2)?.map(|c| c as usize),
                value: r.get(3)?,
                correct: r.get(4)?,
                at: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete_game(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM games WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_history(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch("DELETE FROM moves; DELETE FROM games;")
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        let _ = self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sudoku;

    #[test]
    fn sqlite_game_and_move_history_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let mut g = Game::new(Difficulty::Medium, sudoku::generate(Difficulty::Medium));
        g.id = db.insert_game(&g).unwrap();
        let i = (0..81).find(|&i| g.givens[i] == 0).unwrap();
        g.place(i, g.solution[i]);
        g.reveal();
        let entries = std::mem::take(&mut g.pending_log);
        db.save_game(&g).unwrap();
        db.log_moves(g.id, &entries).unwrap();

        let loaded = db.load_game(g.id).unwrap().unwrap();
        assert_eq!(loaded.status, Status::Revealed);
        assert_eq!(loaded.cells[i], g.solution[i]);
        assert_eq!(db.moves_for(g.id).unwrap().len(), 3); // start, place, reveal

        db.set_setting("dark", "1");
        assert_eq!(db.get_setting("dark").as_deref(), Some("1"));
        db.delete_game(g.id).unwrap();
        assert!(db.list_games().unwrap().is_empty());
    }
}
