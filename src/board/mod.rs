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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Figure {
    Pawn,
    Bishop,
    Knight,
    Queen,
    King,
    Rook,
}

#[derive(Clone)]
pub enum StartLine {
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
    pub start_line: StartLine,
}

impl Piece {
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
        let mut figures = HashMap::new();
        figures.insert(3, Figure::Rook);
        figures.insert(4, Figure::Knight);
        figures.insert(5, Figure::Bishop);
        figures.insert(6, Figure::Queen);
        figures.insert(7, Figure::King);
        figures.insert(8, Figure::Bishop);
        figures.insert(9, Figure::Knight);
        figures.insert(10, Figure::Rook);

        let mut cells = HashMap::new();

        for position in Position::into_enum_iter() {
            let mut cell_content = CellContent::Empty;

            if position.row() == Row::R2 {
                cell_content = CellContent::Piece(Piece {
                    figure: Figure::Pawn,
                    color: Color::Red,
                    have_not_move_yet: true,
                    start_line: StartLine::Row(Row::R2),
                });
            } else if position.column() == Column::b {
                cell_content = CellContent::Piece(Piece {
                    figure: Figure::Pawn,
                    color: Color::Blue,
                    have_not_move_yet: true,
                    start_line: StartLine::Column(Column::b),
                });
            } else if position.row() == Row::R13 {
                cell_content = CellContent::Piece(Piece {
                    figure: Figure::Pawn,
                    color: Color::Yellow,
                    have_not_move_yet: true,
                    start_line: StartLine::Row(Row::R13),
                });
            } else if position.column() == Column::m {
                cell_content = CellContent::Piece(Piece {
                    figure: Figure::Pawn,
                    color: Color::Green,
                    have_not_move_yet: true,
                    start_line: StartLine::Column(Column::m),
                });
            } else if position.row() == Row::R1 {
                let col_idx = position.column().get_index();
                if let Some(figure) = figures.get(&col_idx) {
                    cell_content = CellContent::Piece(Piece {
                        figure: (*figure).clone(),
                        color: Color::Red,
                        have_not_move_yet: true,
                        start_line: StartLine::Row(Row::R1),
                    });
                }
            } else if position.column() == Column::a {
                let row_idx = position.row().get_index();
                if let Some(figure) = figures.get(&row_idx) {
                    cell_content = CellContent::Piece(Piece {
                        figure: (*figure).clone(),
                        color: Color::Blue,
                        have_not_move_yet: true,
                        start_line: StartLine::Column(Column::a),
                    });
                }
            } else if position.row() == Row::R14 {
                let col_idx = position.column().get_index();
                if let Some(figure) = figures.get(&col_idx) {
                    cell_content = CellContent::Piece(Piece {
                        figure: (*figure).clone(),
                        color: Color::Yellow,
                        have_not_move_yet: true,
                        start_line: StartLine::Row(Row::R14),
                    });
                }
            } else if position.column() == Column::n {
                let row_idx = position.row().get_index();
                if let Some(figure) = figures.get(&row_idx) {
                    cell_content = CellContent::Piece(Piece {
                        figure: (*figure).clone(),
                        color: Color::Green,
                        have_not_move_yet: true,
                        start_line: StartLine::Column(Column::n),
                    });
                }
            }
            let cell = Cell {
                position: position.clone(),
                content: cell_content,
            };
            cells.insert(position, cell);
        }
        Board { cells }
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
                                    StartLine::Column(attacker_starting_col) => {
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
                                    StartLine::Row(attacker_starting_row) => {
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
