pub mod position;

use crate::vault::Color;
use enum_iterator::IntoEnumIterator;
use once_cell::sync::Lazy;
pub use position::{Column, Position, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;

/*struct Piece {
    color: Color,
    piece:
}*/

pub struct CastlingPattern {
    pub space_between: Vec<Position>,
    pub king_path: Vec<Position>,
    pub rook_end_pos: Position,
    pub king_end_pos: Position,
}

pub static CASTLING_PATTERNS: Lazy<HashMap<(Position, Position), CastlingPattern>> =
    Lazy::new(|| {
        let mut m = HashMap::new();
        m.insert(
            (Position::d1, Position::h1),
            CastlingPattern {
                space_between: vec![Position::e1, Position::f1, Position::g1],
                king_path: vec![Position::g1, Position::f1],
                rook_end_pos: Position::g1,
                king_end_pos: Position::f1,
            },
        );
        m.insert(
            (Position::k1, Position::h1),
            CastlingPattern {
                space_between: vec![Position::i1, Position::j1],
                king_path: vec![Position::i1, Position::j1],
                rook_end_pos: Position::i1,
                king_end_pos: Position::j1,
            },
        );
        m.insert(
            (Position::a11, Position::a8),
            CastlingPattern {
                space_between: vec![Position::a10, Position::a9],
                king_path: vec![Position::a9, Position::a10],
                rook_end_pos: Position::a9,
                king_end_pos: Position::a10,
            },
        );
        m.insert(
            (Position::a4, Position::a8),
            CastlingPattern {
                space_between: vec![Position::a5, Position::a6, Position::a7],
                king_path: vec![Position::a7, Position::a6],
                rook_end_pos: Position::a7,
                king_end_pos: Position::a6,
            },
        );
        m.insert(
            (Position::k14, Position::g14),
            CastlingPattern {
                space_between: vec![Position::j14, Position::i14, Position::h14],
                king_path: vec![Position::h14, Position::i14],
                rook_end_pos: Position::h14,
                king_end_pos: Position::i14,
            },
        );
        m.insert(
            (Position::d14, Position::g14),
            CastlingPattern {
                space_between: vec![Position::e14, Position::f14],
                king_path: vec![Position::f14, Position::e14],
                rook_end_pos: Position::f14,
                king_end_pos: Position::e14,
            },
        );
        m.insert(
            (Position::n4, Position::n7),
            CastlingPattern {
                space_between: vec![Position::n5, Position::n6],
                king_path: vec![Position::n6, Position::n5],
                rook_end_pos: Position::n6,
                king_end_pos: Position::n5,
            },
        );
        m.insert(
            (Position::n11, Position::n7),
            CastlingPattern {
                space_between: vec![Position::n10, Position::n9, Position::n8],
                king_path: vec![Position::n8, Position::n9],
                rook_end_pos: Position::n8,
                king_end_pos: Position::n9,
            },
        );
        m
    });

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum Figure {
    Pawn,
    Bishop,
    Knight,
    Queen,
    King,
    Rook,
}

impl Figure {
    pub fn is(&self, figure: Figure) -> bool {
        matches!(self, figure)
    }
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
    pub home_line: Line,
}

impl Piece {
    pub fn new(figure: Figure, color: Color, home_line: Line) -> Piece {
        Piece {
            figure,
            color,
            home_line,
            have_not_move_yet: true,
        }
    }
    pub fn already_move(&self) -> bool {
        return !self.have_not_move_yet;
    }
}

pub enum CheckMate {
    No,
    Check,
    Checkmate,
}

struct Restore {
    from: CellPos,
    to: CellPos,
}

pub struct Board {
    pieces: HashMap<Position, Piece>,
    restore: Option<Restore>,
}

struct RawMove {
    from: Position,
    to: Position,
}

impl<'a> Board {
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

        let mut pieces = HashMap::new();

        for position in Position::into_enum_iter() {
            let position_col_row = (position.column(), position.row());

            match position_col_row {
                (_, Row::R2) => pieces.insert(
                    position,
                    Piece::new(Figure::Pawn, Color::Red, Line::Row(Row::R2)),
                ),
                (Column::b, _) => pieces.insert(
                    position,
                    Piece::new(Figure::Pawn, Color::Blue, Line::Column(Column::b)),
                ),
                (_, Row::R13) => pieces.insert(
                    position,
                    Piece::new(Figure::Pawn, Color::Yellow, Line::Row(Row::R13)),
                ),
                (Column::m, _) => pieces.insert(
                    position,
                    Piece::new(Figure::Pawn, Color::Green, Line::Column(Column::m)),
                ),
                (col, Row::R1) => {
                    let figure = figure_seq.get((col.get_index() - 3) as usize).unwrap();
                    pieces.insert(
                        position,
                        Piece::new(*figure, Color::Red, Line::Row(Row::R1)),
                    )
                }
                (Column::a, row) => {
                    let figure = figure_seq.get((row.get_index() - 3) as usize).unwrap();
                    pieces.insert(
                        position,
                        Piece::new(*figure, Color::Blue, Line::Column(Column::a)),
                    )
                }
                (col, Row::R14) => {
                    let figure = figure_seq_reversed
                        .get((col.get_index() - 3) as usize)
                        .unwrap();
                    pieces.insert(
                        position,
                        Piece::new(*figure, Color::Yellow, Line::Row(Row::R14)),
                    )
                }
                (Column::n, row) => {
                    let figure = figure_seq_reversed
                        .get((row.get_index() - 3) as usize)
                        .unwrap();
                    pieces.insert(
                        position,
                        Piece::new(*figure, Color::Green, Line::Column(Column::n)),
                    )
                }
                _ => None,
            };
        }
        return Board {
            pieces,
            restore: None,
        };
    }

    pub fn piece(&self, pos: Position) -> Option<&Piece> {
        self.pieces.get(&pos)
    }

    pub fn attackers_on_position(&self, target_pos: Position) -> Option<Vec<PiecePos>> {
        let mut attackers = Vec::new();

        let row_idx = target_pos.row().get_index();
        let col_idx = target_pos.column().get_index();

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
                if let Some(attacker_piece) = self.piece(attacker_pos) {
                    if attacker_piece.figure == Figure::Knight {
                        attackers.push(PiecePos {
                            position: attacker_pos,
                            piece: attacker_piece,
                        });
                    }
                }
            }
        }

        let diagonals = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
        for shift in &diagonals {
            let mut distance = 0;
            while let Ok(attacker_pos) = Position::try_from((col_idx + shift.0, row_idx + shift.1))
            {
                if let Some(attacker_piece) = self.piece(attacker_pos) {
                    match attacker_piece.figure {
                        Figure::Rook | Figure::Knight => break,
                        Figure::Queen | Figure::Bishop => {
                            attackers.push(PiecePos {
                                position: attacker_pos,
                                piece: attacker_piece,
                            });
                            break;
                        }
                        Figure::Pawn => {
                            if distance == 0 {
                                match &attacker_piece.home_line {
                                    Line::Column(attacker_starting_col) => {
                                        if attacker_starting_col.get_index() == 1 {
                                            if attacker_pos.column().get_index()
                                                < target_pos.column().get_index()
                                            {
                                                attackers.push(PiecePos {
                                                    position: attacker_pos,
                                                    piece: attacker_piece,
                                                });
                                            }
                                        } else {
                                            if attacker_pos.column().get_index()
                                                > target_pos.column().get_index()
                                            {
                                                attackers.push(PiecePos {
                                                    position: attacker_pos,
                                                    piece: attacker_piece,
                                                });
                                            }
                                        }
                                    }
                                    Line::Row(attacker_starting_row) => {
                                        if attacker_starting_row.get_index() == 1 {
                                            if attacker_pos.row().get_index()
                                                < target_pos.row().get_index()
                                            {
                                                attackers.push(PiecePos {
                                                    position: attacker_pos,
                                                    piece: attacker_piece,
                                                });
                                            }
                                        } else {
                                            if attacker_pos.row().get_index()
                                                > target_pos.row().get_index()
                                            {
                                                attackers.push(PiecePos {
                                                    position: attacker_pos,
                                                    piece: attacker_piece,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            break;
                        }
                        Figure::King => {
                            if distance == 0 {
                                attackers.push(PiecePos {
                                    position: attacker_pos,
                                    piece: attacker_piece,
                                });
                            }
                            break;
                        }
                    }
                }
                distance += 1;
            }
        }

        let vertizontals = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        for shift in &vertizontals {
            let mut distance = 0;
            while let Ok(attacker_pos) = Position::try_from((col_idx + shift.0, row_idx + shift.1))
            {
                if let Some(attacker_piece) = self.piece(attacker_pos) {
                    match attacker_piece.figure {
                        Figure::Pawn | Figure::Knight | Figure::Bishop => break,
                        Figure::Queen | Figure::Rook => {
                            attackers.push(PiecePos {
                                position: attacker_pos,
                                piece: attacker_piece,
                            });
                            break;
                        }
                        Figure::King => {
                            if distance == 0 {
                                attackers.push(PiecePos {
                                    position: attacker_pos,
                                    piece: attacker_piece,
                                });
                            }
                            break;
                        }
                    }
                }
                distance += 1;
            }
        }

        if attackers.len() > 0 {
            return Some(attackers);
        } else {
            return None;
        }
    }

    pub fn find_king(&self, color: Color) -> Option<PiecePos> {
        for (position, piece) in &self.pieces {
            if piece.figure.is(Figure::King) && piece.color == color {
                return Some(PiecePos {
                    position: *position,
                    piece: &piece,
                });
            }
        }
        None
    }

    pub fn piece_move(&mut self, from: Position, to: Position) -> Option<Piece> {
        if let Some(mut piece) = self.pieces.remove(&from) {
            piece.have_not_move_yet = false;
            return self.pieces.insert(to, piece);
        }
        None
    }

    pub fn restorable_piece_move(&mut self, from: Position, to: Position) -> Option<Piece> {
        self.restore = Some(Restore {
            from: CellPos {
                cell: match self.pieces.get(&from) {
                    Some(piece) => Some(piece.clone()),
                    None => None,
                },
                position: from,
            },
            to: CellPos {
                cell: match self.pieces.get(&to) {
                    Some(piece) => Some(piece.clone()),
                    None => None,
                },
                position: to,
            },
        });

        self.piece_move(from, to)
    }

    pub fn restore_move(&mut self) -> bool {
        let restore = match self.restore.as_ref() {
            Some(restore) => restore,
            None => return false
        };

        let from = &restore.from;
        match &from.cell {
            Some(piece) => {
                self.pieces.insert(from.position, piece.clone());
            }
            None => {
                self.pieces.remove(&from.position);
            }
        }

        let to = &restore.to;
        match &to.cell {
            Some(piece) => {
                self.pieces.insert(to.position, piece.clone());
            }
            None => {
                self.pieces.remove(&to.position);
            }
        }

        self.restore = None;
        true
    }

    fn moves(&self, piece: Position) -> Vec<RawMove> {
        //match piece.piece.figure {
        //Figure::Pawn => self.
        //}
        vec![]
    }

    pub fn is_checkmate(&mut self, player_color: Color) -> CheckMate {
        let king_pos = match self.find_king(player_color) {
            Some(k) => k.position(),
            None => return CheckMate::No,
        };

        if self.attackers_on_position(king_pos).is_none() {
            return CheckMate::No;
        }

        let our_pieces_pos = self
            .pieces
            .iter()
            .filter(|(pos, piece)| piece.color == player_color)
            .map(|(pos, piece)| *pos)
            .collect::<Vec<_>>();

        for piece_pos in our_pieces_pos {
            for mv in self.moves(piece_pos) {
                self.restorable_piece_move(mv.from, mv.to);
                if self.attackers_on_position(king_pos).is_none() {
                    self.restore_move();
                    return CheckMate::Check;
                }
                self.restore_move();
            }
        }

        CheckMate::Checkmate
    }
}

pub struct PiecePos<'a> {
    piece: &'a Piece,
    position: Position,
}

pub struct CellPos {
    cell: Option<Piece>,
    position: Position,
}

impl<'a> PiecePos<'a> {
    pub fn piece_pos(&self) -> (&'a Piece, Position) {
        (self.piece, self.position)
    }
    pub fn position(&self) -> Position {
        self.position
    }
    pub fn piece(&self) -> &Piece {
        self.piece
    }
}
