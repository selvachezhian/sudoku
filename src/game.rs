//! Game state: the player's board, notes, undo/redo, hints, timer and scoring.

use crate::sudoku::{self, Difficulty, Grid, Puzzle};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    InProgress,
    Completed,
    Revealed,
}

impl Status {
    pub fn key(self) -> &'static str {
        match self {
            Status::InProgress => "in_progress",
            Status::Completed => "completed",
            Status::Revealed => "revealed",
        }
    }
    pub fn from_key(s: &str) -> Self {
        match s {
            "completed" => Status::Completed,
            "revealed" => Status::Revealed,
            _ => Status::InProgress,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Status::InProgress => "In progress",
            Status::Completed => "Solved",
            Status::Revealed => "Revealed",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CellChange {
    idx: usize,
    old_val: u8,
    old_notes: u16,
    new_val: u8,
    new_notes: u16,
}

/// A single entry of the persistent move log.
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub action: &'static str,
    pub cell: Option<usize>,
    pub value: u8,
    pub correct: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceOutcome {
    Ignored,
    Cleared,
    Correct,
    Wrong,
    Solved,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub id: i64,
    pub difficulty: Difficulty,
    pub givens: Grid,
    pub solution: Grid,
    pub cells: Grid,
    pub notes: [u16; 81],
    pub hinted: [bool; 81],
    pub status: Status,
    pub mistakes: u32,
    pub hints: u32,
    pub moves: u32,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    elapsed_base: f64,
    running_since: Option<Instant>,
    undo: Vec<Vec<CellChange>>,
    redo: Vec<Vec<CellChange>>,
    pub pending_log: Vec<LogEntry>,
}

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Game {
    pub fn new(difficulty: Difficulty, puzzle: Puzzle) -> Self {
        Game {
            id: 0,
            difficulty,
            givens: puzzle.givens,
            solution: puzzle.solution,
            cells: puzzle.givens,
            notes: [0; 81],
            hinted: [false; 81],
            status: Status::InProgress,
            mistakes: 0,
            hints: 0,
            moves: 0,
            started_at: now_ts(),
            finished_at: None,
            elapsed_base: 0.0,
            running_since: None,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_log: vec![LogEntry {
                action: "start",
                cell: None,
                value: 0,
                correct: None,
            }],
        }
    }

    /// Rebuilds a game from persisted fields.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: i64,
        difficulty: Difficulty,
        givens: Grid,
        solution: Grid,
        cells: Grid,
        notes: [u16; 81],
        hinted: [bool; 81],
        status: Status,
        mistakes: u32,
        hints: u32,
        moves: u32,
        elapsed: f64,
        started_at: i64,
        finished_at: Option<i64>,
    ) -> Self {
        Game {
            id,
            difficulty,
            givens,
            solution,
            cells,
            notes,
            hinted,
            status,
            mistakes,
            hints,
            moves,
            started_at,
            finished_at,
            elapsed_base: elapsed,
            running_since: None,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_log: Vec::new(),
        }
    }

    // ---------------------------------------------------------------- timer

    pub fn elapsed(&self) -> f64 {
        self.elapsed_base
            + self
                .running_since
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0)
    }

    pub fn is_running(&self) -> bool {
        self.running_since.is_some()
    }

    pub fn resume_timer(&mut self) {
        if self.status == Status::InProgress && self.running_since.is_none() {
            self.running_since = Some(Instant::now());
        }
    }

    pub fn pause_timer(&mut self) {
        if let Some(t) = self.running_since.take() {
            self.elapsed_base += t.elapsed().as_secs_f64();
        }
    }

    // -------------------------------------------------------------- queries

    pub fn is_playing(&self) -> bool {
        self.status == Status::InProgress
    }

    pub fn is_given(&self, i: usize) -> bool {
        self.givens[i] != 0
    }

    pub fn is_locked(&self, i: usize) -> bool {
        self.givens[i] != 0 || self.hinted[i] || self.status != Status::InProgress
    }

    /// The value to display: after revealing we show the full solution.
    pub fn display_value(&self, i: usize) -> u8 {
        if self.status == Status::Revealed {
            self.solution[i]
        } else {
            self.cells[i]
        }
    }

    pub fn is_wrong(&self, i: usize) -> bool {
        self.cells[i] != 0 && self.cells[i] != self.solution[i]
    }

    /// True when a cell was not solved by the player before revealing.
    pub fn was_revealed(&self, i: usize) -> bool {
        self.status == Status::Revealed && self.givens[i] == 0 && self.cells[i] != self.solution[i]
    }

    /// How many times digit `v` currently appears on the board.
    pub fn count_of(&self, v: u8) -> usize {
        (0..81).filter(|&i| self.display_value(i) == v).count()
    }

    pub fn filled_correct(&self) -> usize {
        (0..81).filter(|&i| self.cells[i] == self.solution[i]).count()
    }

    pub fn givens_count(&self) -> usize {
        self.givens.iter().filter(|&&v| v != 0).count()
    }

    pub fn can_undo(&self) -> bool {
        self.is_playing() && !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        self.is_playing() && !self.redo.is_empty()
    }

    // -------------------------------------------------------------- actions

    fn apply(&mut self, changes: Vec<CellChange>) {
        for c in &changes {
            self.cells[c.idx] = c.new_val;
            self.notes[c.idx] = c.new_notes;
        }
        if !changes.is_empty() {
            self.undo.push(changes);
            self.redo.clear();
            self.moves += 1;
        }
    }

    fn log(&mut self, action: &'static str, cell: Option<usize>, value: u8, correct: Option<bool>) {
        self.pending_log.push(LogEntry {
            action,
            cell,
            value,
            correct,
        });
    }

    /// Places digit `v` in cell `i`. Pressing the same digit again clears it.
    pub fn place(&mut self, i: usize, v: u8) -> PlaceOutcome {
        if self.is_locked(i) || !(1..=9).contains(&v) {
            return PlaceOutcome::Ignored;
        }
        if self.cells[i] == v {
            self.erase(i);
            return PlaceOutcome::Cleared;
        }
        let mut changes = vec![CellChange {
            idx: i,
            old_val: self.cells[i],
            old_notes: self.notes[i],
            new_val: v,
            new_notes: 0,
        }];
        let bit = 1u16 << (v - 1);
        for j in sudoku::peers(i) {
            if self.cells[j] == 0 && self.notes[j] & bit != 0 {
                changes.push(CellChange {
                    idx: j,
                    old_val: 0,
                    old_notes: self.notes[j],
                    new_val: 0,
                    new_notes: self.notes[j] & !bit,
                });
            }
        }
        self.apply(changes);
        let correct = self.solution[i] == v;
        self.log("place", Some(i), v, Some(correct));
        if !correct {
            self.mistakes += 1;
            return PlaceOutcome::Wrong;
        }
        if self.check_solved() {
            PlaceOutcome::Solved
        } else {
            PlaceOutcome::Correct
        }
    }

    pub fn toggle_note(&mut self, i: usize, v: u8) {
        if self.is_locked(i) || self.cells[i] != 0 || !(1..=9).contains(&v) {
            return;
        }
        let old = self.notes[i];
        self.apply(vec![CellChange {
            idx: i,
            old_val: 0,
            old_notes: old,
            new_val: 0,
            new_notes: old ^ (1 << (v - 1)),
        }]);
        self.log("note", Some(i), v, None);
    }

    pub fn erase(&mut self, i: usize) {
        if self.is_locked(i) || (self.cells[i] == 0 && self.notes[i] == 0) {
            return;
        }
        let old = self.cells[i];
        self.apply(vec![CellChange {
            idx: i,
            old_val: old,
            old_notes: self.notes[i],
            new_val: 0,
            new_notes: 0,
        }]);
        self.log("erase", Some(i), old, None);
    }

    /// Undo the last move. Returns the primary affected cell.
    pub fn undo(&mut self) -> Option<usize> {
        if !self.is_playing() {
            return None;
        }
        let changes = self.undo.pop()?;
        let mut first = None;
        for c in changes.iter().rev() {
            if self.givens[c.idx] != 0 || self.hinted[c.idx] {
                continue;
            }
            self.cells[c.idx] = c.old_val;
            self.notes[c.idx] = c.old_notes;
            first = Some(c.idx);
        }
        self.log("undo", first, 0, None);
        self.redo.push(changes);
        first
    }

    pub fn redo(&mut self) -> Option<usize> {
        if !self.is_playing() {
            return None;
        }
        let changes = self.redo.pop()?;
        let mut first = None;
        for c in &changes {
            if self.givens[c.idx] != 0 || self.hinted[c.idx] {
                continue;
            }
            self.cells[c.idx] = c.new_val;
            self.notes[c.idx] = c.new_notes;
            if first.is_none() {
                first = Some(c.idx);
            }
        }
        self.log("redo", first, 0, None);
        self.undo.push(changes);
        self.check_solved();
        first
    }

    /// Reveals one cell. Prefers the selected cell, otherwise the cell with
    /// the fewest candidates (the most "logical" next step).
    pub fn hint(&mut self, selected: Option<usize>) -> Option<usize> {
        if !self.is_playing() {
            return None;
        }
        let needs = |g: &Game, i: usize| !g.is_locked(i) && g.cells[i] != g.solution[i];
        let target = match selected {
            Some(i) if needs(self, i) => Some(i),
            _ => {
                // Prefer fixing mistakes first, then the easiest empty cell.
                if let Some(i) = (0..81).find(|&i| needs(self, i) && self.is_wrong(i)) {
                    Some(i)
                } else {
                    (0..81).filter(|&i| needs(self, i)).min_by_key(|&i| {
                        let mut used = 0u16;
                        for j in sudoku::peers(i) {
                            let v = self.cells[j];
                            if v != 0 && v == self.solution[j] {
                                used |= 1 << (v - 1);
                            }
                        }
                        9 - used.count_ones()
                    })
                }
            }
        }?;
        let v = self.solution[target];
        self.cells[target] = v;
        self.notes[target] = 0;
        self.hinted[target] = true;
        let bit = 1u16 << (v - 1);
        for j in sudoku::peers(target) {
            self.notes[j] &= !bit;
        }
        self.hints += 1;
        self.moves += 1;
        self.log("hint", Some(target), v, Some(true));
        self.check_solved();
        Some(target)
    }

    pub fn reveal(&mut self) {
        if !self.is_playing() {
            return;
        }
        self.pause_timer();
        self.status = Status::Revealed;
        self.finished_at = Some(now_ts());
        self.log("reveal", None, 0, None);
    }

    pub fn restart(&mut self) {
        self.cells = self.givens;
        self.notes = [0; 81];
        self.hinted = [false; 81];
        self.status = Status::InProgress;
        self.mistakes = 0;
        self.hints = 0;
        self.moves = 0;
        self.elapsed_base = 0.0;
        self.running_since = None;
        self.finished_at = None;
        self.undo.clear();
        self.redo.clear();
        self.log("restart", None, 0, None);
    }

    fn check_solved(&mut self) -> bool {
        if self.status == Status::InProgress && self.cells == self.solution {
            self.pause_timer();
            self.status = Status::Completed;
            self.finished_at = Some(now_ts());
            self.log("solved", None, 0, None);
            true
        } else {
            self.status == Status::Completed
        }
    }

    /// Units (rows/cols/boxes) containing `i` that are fully and correctly filled.
    pub fn completed_units_at(&self, i: usize) -> Vec<[usize; 9]> {
        sudoku::units()
            .into_iter()
            .filter(|u| u.contains(&i) && u.iter().all(|&j| self.cells[j] == self.solution[j]))
            .collect()
    }

    // --------------------------------------------------------- serialization

    pub fn notes_to_string(&self) -> String {
        self.notes.iter().map(|n| format!("{n:03x}")).collect()
    }

    pub fn notes_from_string(s: &str) -> [u16; 81] {
        let mut out = [0u16; 81];
        for (i, slot) in out.iter_mut().enumerate() {
            if let Some(chunk) = s.get(i * 3..i * 3 + 3) {
                *slot = u16::from_str_radix(chunk, 16).unwrap_or(0) & 0x1ff;
            }
        }
        out
    }

    pub fn hinted_to_string(&self) -> String {
        self.hinted.iter().map(|&h| if h { '1' } else { '0' }).collect()
    }

    pub fn hinted_from_string(s: &str) -> [bool; 81] {
        let mut out = [false; 81];
        for (i, b) in s.bytes().take(81).enumerate() {
            out[i] = b == b'1';
        }
        out
    }
}

pub fn format_duration(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_by_hints() {
        let mut g = Game::new(Difficulty::Easy, sudoku::generate(Difficulty::Easy));
        while g.is_playing() {
            g.hint(None).unwrap();
        }
        assert_eq!(g.status, Status::Completed);
    }

    #[test]
    fn mistakes_and_undo() {
        let mut g = Game::new(Difficulty::Easy, sudoku::generate(Difficulty::Easy));
        let i = (0..81).find(|&i| g.givens[i] == 0).unwrap();
        let wrong = (1..=9).find(|&v| v != g.solution[i]).unwrap();
        assert_eq!(g.place(i, wrong), PlaceOutcome::Wrong);
        assert_eq!(g.mistakes, 1);
        assert!(g.is_wrong(i));
        g.undo();
        assert_eq!(g.cells[i], 0);
        g.redo();
        assert_eq!(g.cells[i], wrong);
    }

    #[test]
    fn notes_roundtrip() {
        let mut g = Game::new(Difficulty::Easy, sudoku::generate(Difficulty::Easy));
        g.notes[5] = 0b1_0100_0001;
        assert_eq!(Game::notes_from_string(&g.notes_to_string()), g.notes);
    }
}
