# Sudoku

A fast, modern, cross-platform **Sudoku** desktop application written in **100% Rust** with embedded **SQLite** persistence.

---

## ✨ Features

- **Unique-Solution Puzzle Generator** — Generates random symmetric puzzles on the fly with guaranteed single solutions using a bitmask backtracking solver with the Minimum Remaining Values (MRV) heuristic.
- **4 Difficulty Levels** — **Easy** (38–42 givens), **Medium** (31–34 givens), **Hard** (26–28 givens), and **Expert** (22–24 givens).
- **Real-Time Mistake Highlighting** — Highlights wrong entries, shakes the cell on invalid placement, and marks conflicting peer cells in the same row, column, or 3×3 box (can be toggled on/off anytime).
- **Smart Hints & Full Solution Reveal** —
  - **Hint (`💡` / `H`)**: Fixes a mistaken cell or reveals the selected/most constrained empty cell.
  - **Show Solution (`👁`)**: Reveals the complete answer when you get stuck, distinguishing your solved cells from revealed ones and logging the game as *Revealed*.
- **Pencil Marks (Notes)** — Toggle candidate digits (`1–9`) inside any empty cell; placing a digit automatically clears matching notes in peer cells.
- **Full Undo / Redo** — Multi-cell undo/redo stack that restores both cell values and auto-cleared peer notes.
- **Complete SQLite History & Move Log** —
  - Every game is saved automatically as you play — close the app anytime and resume right where you left off.
  - Browse past games filtered by **Difficulty** and **Status** (*In progress*, *Solved*, *Revealed*).
  - **Resume** unfinished games, **Replay** any historical puzzle from scratch, or inspect the **Game Detail** view with a timestamped move-by-move log and board solution toggle.
- **Statistics Dashboard** — Tracks games played, solve count, win rate, current & best streaks, total time played, per-difficulty best/average times, and an interactive bar chart of recent solve times.
- **Polished UI/UX** — Light & Dark themes (auto-detects system theme), smooth animations (selection ring, segmented controls, unit-completion flashes, radial solve wave), focus-aware pause timer, and embedded **Inter** typography.

---

## 🖥️ Supported Platforms

Runs natively as a single standalone binary with zero external runtime dependencies on:

- **Windows** (Win32 / OpenGL)
- **macOS** (AppKit / OpenGL)
- **Linux** (Wayland & X11 / OpenGL)

SQLite is compiled directly into the binary via `rusqlite` (`bundled` feature), and fonts are embedded via `include_bytes!`.

---

## 🚀 Getting Started

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (`rustc` 1.85+ / Edition 2024)

### Run in Development

```bash
cargo run
```

### Build Optimized Release Binary

```bash
cargo build --release
```

The standalone executable will be generated at:
- **Windows:** `target\release\sudoku.exe`
- **macOS / Linux:** `target/release/sudoku`

### Run Unit Tests

```bash
cargo test
```

---

## ⌨️ Controls & Keyboard Shortcuts

| Key / Action | Description |
|---|---|
| `Click` or `←` `↑` `→` `↓` / `W` `A` `S` `D` | Select / navigate cells |
| `1` – `9` | Place digit in selected cell *(press same digit again to clear)* |
| `Shift` + `1` – `9` | Toggle pencil note for that digit |
| `N` | Toggle **Notes** mode on/off |
| `Backspace` / `Delete` / `0` | Erase selected cell |
| `H` | **Hint** — reveal selected cell or easiest next step |
| `Ctrl`/`⌘` + `Z` | Undo |
| `Ctrl`/`⌘` + `Y` or `Ctrl`/`⌘` + `Shift` + `Z` | Redo |
| `Space` / `P` | Pause / Resume game |
| `Esc` | Deselect cell / close modal |

---

## 🗄️ Where Data Is Stored

All puzzles, active games, move logs, and user preferences are stored locally in a single SQLite database (`sudoku.db` in WAL mode):

| OS | Path |
|---|---|
| **Windows** | `C:\Users\<Username>\AppData\Roaming\RustSudoku\Sudoku\data\sudoku.db` |
| **macOS** | `~/Library/Application Support/com.RustSudoku.Sudoku/sudoku.db` |
| **Linux** | `~/.local/share/sudoku/sudoku.db` *(or `$XDG_DATA_HOME/sudoku/sudoku.db`)* |

---

## 📁 Project Structure

```text
sudoku/
├── assets/
│   └── fonts/          # Embedded Inter font weights (400, 500, 700) + SIL OFL license
├── src/
│   ├── main.rs         # Native window setup & application entry point
│   ├── app.rs          # Play, History & Statistics screens, modals, keyboard handling
│   ├── board.rs        # 9×9 grid renderer, highlights, notes & completion animations
│   ├── game.rs         # Game state, undo/redo stack, smart hints, timer & serialization
│   ├── sudoku.rs       # Bitmask solver (MRV) & unique-solution puzzle generator
│   ├── db.rs           # SQLite schema, migrations, game & move log queries
│   ├── theme.rs        # Light/Dark color palettes, font loader & egui styling
│   └── widgets.rs      # Custom-painted buttons, segmented tabs, numpad & badges
└── Cargo.toml
```

