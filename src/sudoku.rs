//! Core Sudoku engine: a fast bitmask solver and a unique-solution puzzle generator.

use rand::Rng;
use rand::seq::SliceRandom;

pub type Grid = [u8; 81];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Expert,
}

impl Difficulty {
    pub const ALL: [Difficulty; 4] = [
        Difficulty::Easy,
        Difficulty::Medium,
        Difficulty::Hard,
        Difficulty::Expert,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
            Difficulty::Expert => "Expert",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Medium => "medium",
            Difficulty::Hard => "hard",
            Difficulty::Expert => "expert",
        }
    }

    pub fn from_key(s: &str) -> Self {
        match s {
            "medium" => Difficulty::Medium,
            "hard" => Difficulty::Hard,
            "expert" => Difficulty::Expert,
            _ => Difficulty::Easy,
        }
    }

    /// Target number of given (pre-filled) cells.
    fn target_givens(self, rng: &mut impl Rng) -> usize {
        match self {
            Difficulty::Easy => rng.gen_range(38..=42),
            Difficulty::Medium => rng.gen_range(31..=34),
            Difficulty::Hard => rng.gen_range(26..=28),
            Difficulty::Expert => rng.gen_range(22..=24),
        }
    }
}

#[inline]
pub fn row_of(i: usize) -> usize {
    i / 9
}
#[inline]
pub fn col_of(i: usize) -> usize {
    i % 9
}
#[inline]
pub fn box_of(i: usize) -> usize {
    (i / 27) * 3 + (i % 9) / 3
}

/// Indices of the 20 peers (same row, column or box) of a cell.
pub fn peers(i: usize) -> impl Iterator<Item = usize> {
    let (r, c, b) = (row_of(i), col_of(i), box_of(i));
    (0..81).filter(move |&j| j != i && (row_of(j) == r || col_of(j) == c || box_of(j) == b))
}

/// The 27 units (9 rows, 9 cols, 9 boxes) as cell index lists.
pub fn units() -> Vec<[usize; 9]> {
    let mut out = Vec::with_capacity(27);
    for r in 0..9 {
        out.push(std::array::from_fn(|k| r * 9 + k));
    }
    for c in 0..9 {
        out.push(std::array::from_fn(|k| k * 9 + c));
    }
    for b in 0..9 {
        let (br, bc) = ((b / 3) * 3, (b % 3) * 3);
        out.push(std::array::from_fn(|k| (br + k / 3) * 9 + bc + k % 3));
    }
    out
}

struct Solver {
    grid: Grid,
    rows: [u16; 9],
    cols: [u16; 9],
    boxes: [u16; 9],
}

const ALL: u16 = 0b1_1111_1111;

impl Solver {
    fn new(grid: &Grid) -> Option<Self> {
        let mut s = Solver {
            grid: *grid,
            rows: [0; 9],
            cols: [0; 9],
            boxes: [0; 9],
        };
        for i in 0..81 {
            let v = grid[i];
            if v != 0 {
                let bit = 1 << (v - 1);
                let (r, c, b) = (row_of(i), col_of(i), box_of(i));
                if s.rows[r] & bit != 0 || s.cols[c] & bit != 0 || s.boxes[b] & bit != 0 {
                    return None;
                }
                s.rows[r] |= bit;
                s.cols[c] |= bit;
                s.boxes[b] |= bit;
            }
        }
        Some(s)
    }

    #[inline]
    fn candidates(&self, i: usize) -> u16 {
        !(self.rows[row_of(i)] | self.cols[col_of(i)] | self.boxes[box_of(i)]) & ALL
    }

    #[inline]
    fn set(&mut self, i: usize, v: u8) {
        let bit = 1 << (v - 1);
        self.grid[i] = v;
        self.rows[row_of(i)] |= bit;
        self.cols[col_of(i)] |= bit;
        self.boxes[box_of(i)] |= bit;
    }

    #[inline]
    fn unset(&mut self, i: usize, v: u8) {
        let bit = !(1u16 << (v - 1));
        self.grid[i] = 0;
        self.rows[row_of(i)] &= bit;
        self.cols[col_of(i)] &= bit;
        self.boxes[box_of(i)] &= bit;
    }

    /// Picks the empty cell with the fewest candidates (MRV heuristic).
    fn best_cell(&self) -> Option<(usize, u16)> {
        let mut best: Option<(usize, u16)> = None;
        let mut best_n = 10;
        for i in 0..81 {
            if self.grid[i] == 0 {
                let c = self.candidates(i);
                let n = c.count_ones();
                if n < best_n {
                    best_n = n;
                    best = Some((i, c));
                    if n <= 1 {
                        break;
                    }
                }
            }
        }
        best
    }

    fn count(&mut self, limit: usize, found: &mut usize) {
        let Some((i, mut cands)) = self.best_cell() else {
            *found += 1;
            return;
        };
        while cands != 0 {
            let v = cands.trailing_zeros() as u8 + 1;
            cands &= cands - 1;
            self.set(i, v);
            self.count(limit, found);
            self.unset(i, v);
            if *found >= limit {
                return;
            }
        }
    }

    fn fill_random(&mut self, rng: &mut impl Rng) -> bool {
        let Some((i, cands)) = self.best_cell() else {
            return true;
        };
        let mut vals: Vec<u8> = (1..=9).filter(|v| cands & (1 << (v - 1)) != 0).collect();
        vals.shuffle(rng);
        for v in vals {
            self.set(i, v);
            if self.fill_random(rng) {
                return true;
            }
            self.unset(i, v);
        }
        false
    }

    fn solve(&mut self) -> bool {
        let Some((i, mut cands)) = self.best_cell() else {
            return true;
        };
        while cands != 0 {
            let v = cands.trailing_zeros() as u8 + 1;
            cands &= cands - 1;
            self.set(i, v);
            if self.solve() {
                return true;
            }
            self.unset(i, v);
        }
        false
    }
}

/// Counts solutions, stopping early once `limit` is reached.
pub fn count_solutions(grid: &Grid, limit: usize) -> usize {
    let Some(mut s) = Solver::new(grid) else {
        return 0;
    };
    let mut found = 0;
    s.count(limit, &mut found);
    found
}

#[allow(dead_code)]
pub fn solve(grid: &Grid) -> Option<Grid> {
    let mut s = Solver::new(grid)?;
    s.solve().then_some(s.grid)
}

#[derive(Clone, Debug)]
pub struct Puzzle {
    pub givens: Grid,
    pub solution: Grid,
}

/// Generates a random puzzle with exactly one solution.
pub fn generate(difficulty: Difficulty) -> Puzzle {
    let mut rng = rand::thread_rng();

    let mut s = Solver::new(&[0; 81]).expect("empty grid is valid");
    s.fill_random(&mut rng);
    let solution = s.grid;

    let target = difficulty.target_givens(&mut rng);
    let mut puzzle = solution;
    let mut givens = 81;

    // Remove cells in symmetric pairs (classic look) while uniqueness holds.
    let mut order: Vec<usize> = (0..=40).collect();
    order.shuffle(&mut rng);
    for &i in &order {
        if givens <= target {
            break;
        }
        let j = 80 - i;
        let (a, b) = (puzzle[i], puzzle[j]);
        if a == 0 {
            continue;
        }
        puzzle[i] = 0;
        puzzle[j] = 0;
        let removed = if i == j { 1 } else { 2 };
        if givens - removed < target.saturating_sub(1) || count_solutions(&puzzle, 2) != 1 {
            puzzle[i] = a;
            puzzle[j] = b;
        } else {
            givens -= removed;
        }
    }

    // Fine-tune with single removals if still above target.
    if givens > target {
        let mut cells: Vec<usize> = (0..81).collect();
        cells.shuffle(&mut rng);
        for i in cells {
            if givens <= target {
                break;
            }
            let a = puzzle[i];
            if a == 0 {
                continue;
            }
            puzzle[i] = 0;
            if count_solutions(&puzzle, 2) != 1 {
                puzzle[i] = a;
            } else {
                givens -= 1;
            }
        }
    }

    Puzzle {
        givens: puzzle,
        solution,
    }
}

pub fn grid_to_string(g: &Grid) -> String {
    g.iter().map(|&v| (b'0' + v) as char).collect()
}

pub fn grid_from_string(s: &str) -> Grid {
    let mut g = [0u8; 81];
    for (i, ch) in s.bytes().take(81).enumerate() {
        g[i] = ch.saturating_sub(b'0').min(9);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_puzzles_are_unique_and_consistent() {
        for d in Difficulty::ALL {
            let p = generate(d);
            assert_eq!(count_solutions(&p.givens, 2), 1);
            assert_eq!(solve(&p.givens).unwrap(), p.solution);
            for i in 0..81 {
                if p.givens[i] != 0 {
                    assert_eq!(p.givens[i], p.solution[i]);
                }
            }
        }
    }

    #[test]
    fn roundtrip_string() {
        let p = generate(Difficulty::Easy);
        assert_eq!(grid_from_string(&grid_to_string(&p.givens)), p.givens);
    }
}
