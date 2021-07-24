pub mod position;

use crate::vault::Color;
use enum_iterator::IntoEnumIterator;
pub use position::{Column, Position, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;

/*struct Piece {
    color: Color,
    piece:
}*/

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum Figure {
    Pawn,
    Bishop,
    Knight,
    Queen,
    King,
    Rook,
}

#[derive(Clone)]
pub enum Line {
    Column(Column),
    Row(Row),
}

#[derive(Clone)]
pub struct Piece {
    figure: Figure,
    pub color: Color,
    // need for king rook castling
    have_not_move_yet: bool,
    // need for pawn direction determine
    pub start_line: Line,
}

impl Piece {
    pub fn new(figure: Figure, color: Color, start_line: Line) -> Piece {
        Piece {
            figure,
            color,
            start_line,
            have_not_move_yet: true,
        }
    }
    pub fn already_move(&self) -> bool {
        return !self.have_not_move_yet;
    }
}

pub enum CellContent {
    Empty,
    Piece(Piece),
}

pub struct Cell {
    pub position: Position,
    pub content: CellContent,
}

impl Cell {
    pub fn is_figure(&self, fig: &Figure) -> bool {
        return match &self.content {
            CellContent::Piece(p) => matches!(&p.figure, fig),
            CellContent::Empty => false,
        };
    }
    pub fn is_empty(&self) -> bool {
        matches!(&self.content, CellContent::Empty)
    }
    pub fn have_not_move_yet(&self) -> bool {
        return match &self.content {
            CellContent::Piece(p) => p.have_not_move_yet,
            CellContent::Empty => false,
        };
    }
    pub fn as_ref(&self) -> Option<&Piece> {
        match &self.content {
            CellContent::Piece(piece) => Some(piece),
            CellContent::Empty => None,
        }
    }
}

pub struct Board {
    pub cells: HashMap<Position, Cell>,
}

impl Board {
    pub fn new() -> Board {
        let figure_seq = [
            Figure::Rook,
            Figure::Knight,
            Figure::Bishop,
            Figure::Queen,
            Figure::King,
            Figure::Bishop,
            Figure::Knight,
            Figure::Rook,
        ];

        let mut figure_seq_reversed = figure_seq;
        figure_seq_reversed.reverse();

        let mut cells = HashMap::new();

        for position in Position::into_enum_iter() {
            let position_col_row = (position.column(), position.row());

            let cell_content = match position_col_row {
                (_, Row::R2) => {
                    CellContent::Piece(Piece::new(Figure::Pawn, Color::Red, Line::Row(Row::R2)))
                }
                (Column::b, _) => CellContent::Piece(Piece::new(
                    Figure::Pawn,
                    Color::Blue,
                    Line::Column(Column::b),
                )),
                (_, Row::R13) => {
                    CellContent::Piece(Piece::new(Figure::Pawn, Color::Yellow, Line::Row(Row::R13)))
                }
                (Column::m, _) => CellContent::Piece(Piece::new(
                    Figure::Pawn,
                    Color::Green,
                    Line::Column(Column::m),
                )),
                (col, Row::R1) => {
                    let figure = figure_seq.get((col.get_index() - 3) as usize).unwrap();
                    CellContent::Piece(Piece::new(*figure, Color::Red, Line::Row(Row::R1)))
                }
                (Column::a, row) => {
                    let figure = figure_seq.get((row.get_index() - 3) as usize).unwrap();
                    CellContent::Piece(Piece::new(*figure, Color::Blue, Line::Column(Column::a)))
                }
                (col, Row::R14) => {
                    let figure = figure_seq_reversed
                        .get((col.get_index() - 3) as usize)
                        .unwrap();
                    CellContent::Piece(Piece::new(*figure, Color::Yellow, Line::Row(Row::R14)))
                }
                (Column::n, row) => {
                    let figure = figure_seq_reversed
                        .get((row.get_index() - 3) as usize)
                        .unwrap();
                    CellContent::Piece(Piece::new(*figure, Color::Green, Line::Column(Column::n)))
                }
                (_) => CellContent::Empty,
            };

            let cell = Cell {
                position: position.clone(),
                content: cell_content,
            };
            cells.insert(position, cell);
        }
        return Board { cells };
    }

    pub fn cell(&self, pos: &Position) -> &Cell {
        self.cells.get(pos).unwrap()
    }

    pub fn cell_under_attack_from(&self, target_pos: &Position) -> Vec<&Cell> {
        let mut attackers = Vec::new();

        let target_cell = self.cell(target_pos);
        let row_idx = target_cell.position.row().get_index();
        let col_idx = target_cell.position.column().get_index();

        let knights_shifts = [
            (2, 1),
            (1, 2),
            (2, -1),
            (1, -2),
            (-2, 1),
            (-1, 2),
            (-2, -1),
            (-1, -2),
        ];
        for knight_shift in &knights_shifts {
            if let Ok(attacker_pos) =
                Position::try_from((col_idx + knight_shift.0, row_idx + knight_shift.1))
            {
                let attacker_cell = self.cell(&attacker_pos);
                if attacker_cell.is_figure(&Figure::Knight) {
                    attackers.push(attacker_cell);
                }
            }
        }

        let diagonals = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
        for shift in &diagonals {
            let mut distance = 0;
            while let Ok(attacker_pos) = Position::try_from((col_idx + shift.0, row_idx + shift.1))
            {
                let attacker_cell = self.cell(&attacker_pos);
                match &attacker_cell.content {
                    CellContent::Piece(attacker_piece) => match attacker_piece.figure {
                        Figure::Rook | Figure::Knight => break,
                        Figure::Queen | Figure::Bishop => {
                            attackers.push(attacker_cell);
                            break;
                        }
                        Figure::Pawn => {
                            if distance == 0 {
                                match &attacker_piece.start_line {
                                    Line::Column(attacker_starting_col) => {
                                        if attacker_starting_col.get_index() == 1 {
                                            if attacker_pos.column().get_index()
                                                < target_pos.column().get_index()
                                            {
                                                attackers.push(attacker_cell);
                                            }
                                        } else {
                                            if attacker_pos.column().get_index()
                                                > target_pos.column().get_index()
                                            {
                                                attackers.push(attacker_cell);
                                            }
                                        }
                                    }
                                    Line::Row(attacker_starting_row) => {
                                        if attacker_starting_row.get_index() == 1 {
                                            if attacker_pos.row().get_index()
                                                < target_pos.row().get_index()
                                            {
                                                attackers.push(attacker_cell);
                                            }
                                        } else {
                                            if attacker_pos.row().get_index()
                                                > target_pos.row().get_index()
                                            {
                                                attackers.push(attacker_cell);
                                            }
                                        }
                                    }
                                }
                            }
                            break;
                        }
                        Figure::King => {
                            if distance == 0 {
                                attackers.push(attacker_cell);
                            }
                            break;
                        }
                    },
                    CellContent::Empty => (),
                }
                distance += 1;
            }
        }

        let vertizontals = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        for shift in &vertizontals {
            let mut distance = 0;
            while let Ok(attacker_pos) = Position::try_from((col_idx + shift.0, row_idx + shift.1))
            {
                let attacker_cell = self.cell(&attacker_pos);
                match &attacker_cell.content {
                    CellContent::Piece(attacker_piece) => match attacker_piece.figure {
                        Figure::Pawn | Figure::Knight | Figure::Bishop => break,
                        Figure::Queen | Figure::Rook => {
                            attackers.push(attacker_cell);
                            break;
                        }
                        Figure::King => {
                            if distance == 0 {
                                attackers.push(attacker_cell);
                            }
                            break;
                        }
                    },
                    CellContent::Empty => (),
                }
                distance += 1;
            }
        }

        return attackers;
    }
}
