use super::Player;
use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Piece {
    None,
    WhitePawn,
    WhiteKnight,
    WhiteBishop,
    WhiteRook,
    WhiteQueen,
    WhiteKing,
    BlackPawn,
    BlackKnight,
    BlackBishop,
    BlackRook,
    BlackQueen,
    BlackKing,
}

impl Piece {
    /// Returns the type of the given piece.
    pub fn type_of(&self) -> PieceType {
        match *self {
            Piece::None => PieceType::None,
            Piece::WhitePawn => PieceType::Pawn,
            Piece::WhiteKnight => PieceType::Knight,
            Piece::WhiteBishop => PieceType::Bishop,
            Piece::WhiteRook => PieceType::Rook,
            Piece::WhiteQueen => PieceType::Queen,
            Piece::WhiteKing => PieceType::King,
            Piece::BlackPawn => PieceType::Pawn,
            Piece::BlackKnight => PieceType::Knight,
            Piece::BlackBishop => PieceType::Bishop,
            Piece::BlackRook => PieceType::Rook,
            Piece::BlackQueen => PieceType::Queen,
            Piece::BlackKing => PieceType::King,
        }
    }

    /// Returns the player of the given piece.
    ///
    /// # Panics
    ///
    /// Panics if the given `Piece` is `Piece::None`. This function
    /// should only be used when the `Piece` is guaranteed to not be
    /// `Piece::None`.
    pub fn player(&self) -> Player {
        match *self {
            Piece::None => panic!(),
            Piece::WhitePawn => Player::White,
            Piece::WhiteKnight => Player::White,
            Piece::WhiteBishop => Player::White,
            Piece::WhiteRook => Player::White,
            Piece::WhiteQueen => Player::White,
            Piece::WhiteKing => Player::White,
            Piece::BlackPawn => Player::Black,
            Piece::BlackKnight => Player::Black,
            Piece::BlackBishop => Player::Black,
            Piece::BlackRook => Player::Black,
            Piece::BlackQueen => Player::Black,
            Piece::BlackKing => Player::Black,
        }
    }

    /// Return a `Piece` from a `Player` and a `PieceType`.
    pub fn make(player: Player, piece_type: PieceType) -> Self {
        match player {
            Player::White => match piece_type {
                PieceType::None => Piece::None,
                PieceType::Pawn => Piece::WhitePawn,
                PieceType::Knight => Piece::WhiteKnight,
                PieceType::Bishop => Piece::WhiteBishop,
                PieceType::Rook => Piece::WhiteRook,
                PieceType::Queen => Piece::WhiteQueen,
                PieceType::King => Piece::WhiteKing,
            },
            Player::Black => match piece_type {
                PieceType::None => Piece::None,
                PieceType::Pawn => Piece::BlackPawn,
                PieceType::Knight => Piece::BlackKnight,
                PieceType::Bishop => Piece::BlackBishop,
                PieceType::Rook => Piece::BlackRook,
                PieceType::Queen => Piece::BlackQueen,
                PieceType::King => Piece::BlackKing,
            },
        }
    }

    /// Returns a tuple containing the `Player` and `PieceType` of the `Piece`.
    pub fn player_piece(&self) -> (Player, PieceType) {
        (self.player(), self.type_of())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PieceType {
    None,
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

pub const PROMO_PIECES: [PieceType; 4] = [
    PieceType::Knight,
    PieceType::Bishop,
    PieceType::Rook,
    PieceType::Queen,
];

impl PieceType {
    pub fn is_none(&self) -> bool {
        *self == PieceType::None
    }

    fn long_name(&self) -> &str {
        match self {
            PieceType::None => "none",
            PieceType::Pawn => "pawn",
            PieceType::Knight => "knight",
            PieceType::Bishop => "bishop",
            PieceType::Rook => "rook",
            PieceType::Queen => "queen",
            PieceType::King => "king",
        }
    }

    fn short_name(&self) -> &str {
        match self {
            PieceType::None => "-",
            PieceType::Pawn => "p",
            PieceType::Knight => "n",
            PieceType::Bishop => "b",
            PieceType::Rook => "r",
            PieceType::Queen => "q",
            PieceType::King => "k",
        }
    }
}

impl fmt::Display for PieceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(width) = f.width() {
            if width == 1 {
                write!(f, "{}", self.short_name())
            } else {
                write!(f, "{}", self.long_name())
            }
        } else {
            write!(f, "{}", self.long_name())
        }
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Piece::None => write!(f, "."),
            Piece::WhitePawn => write!(f, "P"),
            Piece::WhiteKnight => write!(f, "N"),
            Piece::WhiteBishop => write!(f, "B"),
            Piece::WhiteRook => write!(f, "R"),
            Piece::WhiteQueen => write!(f, "Q"),
            Piece::WhiteKing => write!(f, "K"),
            Piece::BlackPawn => write!(f, "p"),
            Piece::BlackKnight => write!(f, "n"),
            Piece::BlackBishop => write!(f, "b"),
            Piece::BlackRook => write!(f, "r"),
            Piece::BlackQueen => write!(f, "q"),
            Piece::BlackKing => write!(f, "k"),
        }
    }
}
