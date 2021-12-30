mod board;
mod castling;
mod fen;
mod piece;
mod square;
mod state;

use crate::bb::Bitboard;
use crate::masks::{CASTLING_PATH, CASTLING_ROOK_START, FILE_BB, RANK_BB};
use crate::mov::{Move, SpecialMove, UndoableMove};
use crate::movegen::{bishop_moves, rook_moves, MoveGen};
use crate::precalc::boards::{aligned, between_bb, king_moves, knight_moves, pawn_attacks_from};

pub use board::Board;
pub use castling::{CastleType, CastlingRights};
pub use piece::{Piece, PieceType, PROMO_PIECES};
pub use square::Square;
pub use state::State;

use std::fmt;
use std::ops::Not;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Player {
    White = 0,
    Black = 1,
}

impl Player {
    /// Returns the other player.
    pub fn other_player(&self) -> Self {
        match self {
            Player::White => Player::Black,
            Player::Black => Player::White,
        }
    }

    /// Returns the relative square from a given square.
    #[inline(always)]
    pub fn relative_square(self, sq: Square) -> Square {
        assert!(sq.is_okay());
        sq ^ Square((self) as u8 * 56)
    }

    /// Returns the offset for a single move pawn push.
    #[inline(always)]
    pub fn pawn_push(self) -> i8 {
        match self {
            Player::White => 8,
            Player::Black => -8,
        }
    }

    /// Returns the actual algebraic notation board rank for
    /// a given rank as seen from the `Player`s perspective.
    #[inline(always)]
    pub fn relative_rank(&self, rank: u8) -> u8 {
        debug_assert!(rank >= 0 && rank <= 7);
        match self {
            Player::White => rank,
            Player::Black => 7 - rank,
        }
    }
}

impl Not for Player {
    type Output = Self;
    fn not(self) -> Self::Output {
        self.other_player()
    }
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Player::White => write!(f, "White"),
            Player::Black => write!(f, "Black"),
        }
    }
}

// TODO: turn off pub for all the `Position` fields and provide getters
#[derive(Clone, Eq, PartialEq)]
pub struct Position {
    // Array of pieces
    pub(crate) board: Board,

    // Bitboards for each piece type
    // TODO: should we switch to a scheme where the bitboards give all of each piece type
    // (i.e. white pawns and black pawns are all on one bitboard), and then we have a
    // white_pieces bb and black_pieces bb maintained separately? To get white_pawns, you would
    // do (pawns & white_pieces)
    // TODO: rename `no_piece` to `no_pieces` for consistency
    pub(crate) no_piece: Bitboard,
    pub(crate) white_pawns: Bitboard,
    pub(crate) white_knights: Bitboard,
    pub(crate) white_bishops: Bitboard,
    pub(crate) white_rooks: Bitboard,
    pub(crate) white_queens: Bitboard,
    pub(crate) white_king: Bitboard,
    pub(crate) black_pawns: Bitboard,
    pub(crate) black_knights: Bitboard,
    pub(crate) black_bishops: Bitboard,
    pub(crate) black_rooks: Bitboard,
    pub(crate) black_queens: Bitboard,
    pub(crate) black_king: Bitboard,
    // Bitboards for each color
    pub(crate) white_pieces: Bitboard,
    pub(crate) black_pieces: Bitboard,

    // Piece counts
    pub(crate) white_piece_count: u8,
    pub(crate) black_piece_count: u8,

    // "Invisible" state
    turn: Player,
    pub(crate) castling_rights: CastlingRights,
    pub(crate) ep_square: Option<Square>,
    // TODO: use a 'half-move' counter to track the game move number,
    // and make the 50-move rule counter a separate thing. That way the
    // logic for the current concept called `move_number` will be more elegant
    // (i.e. not checking for a white move before incrementing, and not dealing)
    // with 0.5 move increments.
    pub(crate) half_move_clock: u32,
    pub(crate) move_number: u32,

    // `State` struct stores other useful information for fast access
    // TODO: Pleco wraps this in an Arc for quick copying of states without
    // copying memory. Do we need that?
    pub(crate) state: Option<State>,

    /// History stores a `Vec` of `UndoableMove`s, allowing the `Position` to
    /// be rolled back with `unmake_move()`.
    pub(crate) history: Vec<UndoableMove>,
}

impl Position {
    /// Sets the `State` struct for the current position. Should only be called
    /// when initialising a new `Position`.
    pub fn set_state(&mut self) {
        self.state = Some(State::from_position(&self));
    }

    pub fn history(&self) -> &Vec<UndoableMove> {
        &self.history
    }

    /// Make a move on the Board and update the `Position`.
    ///
    /// # Panics
    ///
    /// The supplied `Move` must be legal in the current position, otherwise a
    /// panic will occur. Legal moves can be generated with `MoveGen::generate_all()`
    pub fn make_move(&mut self, mov: Move) {
        // In debug mode, check the move isn't somehow null
        debug_assert_ne!(mov.orig(), mov.dest());

        // Add an undoable move to the position history
        let undoable_move = mov.to_undoable(&self);
        self.history.push(undoable_move);

        // Reset the en passant square
        self.ep_square = None;

        let us = self.turn();
        let them = !us;
        let from = mov.orig();
        let to = mov.dest();
        let moving_piece = self.piece_at_sq(from);
        let captured_piece = if mov.is_en_passant() {
            Piece::make(them, PieceType::Pawn)
        } else {
            self.piece_at_sq(to)
        };

        // Sanity check
        debug_assert_eq!(moving_piece.player(), us);

        // Increment clocks
        self.half_move_clock += 1;
        if us == Player::Black {
            // Black is moving, so the full-move counter will increment
            self.move_number += 1;
        }

        // Castling rights
        let new_castling_rights = self.castling_rights.update(from);
        self.castling_rights = new_castling_rights;

        // Castling move
        if mov.is_castle() {
            // Sanity checks
            debug_assert_eq!(moving_piece.type_of(), PieceType::King);
            debug_assert_eq!(captured_piece.type_of(), PieceType::None);

            let mut r_orig = Square(0);
            let mut r_dest = Square(0);
            self.apply_castling(us, from, to, &mut r_orig, &mut r_dest);
        } else if captured_piece != Piece::None {
            let mut cap_sq = to;
            if captured_piece.type_of() == PieceType::Pawn {
                if mov.is_en_passant() {
                    debug_assert_eq!(to, self.ep_square.unwrap());
                    match us {
                        Player::White => cap_sq -= Square(8),
                        Player::Black => cap_sq += Square(8),
                    };

                    debug_assert_eq!(moving_piece.type_of(), PieceType::Pawn);
                    debug_assert_eq!(us.relative_rank(5), to.rank()); // `to` square is on "6th" rank from player's perspective
                    debug_assert_eq!(self.piece_at_sq(to), Piece::None);
                    debug_assert_eq!(
                        self.piece_at_sq(cap_sq).player_piece(),
                        (them, PieceType::Pawn)
                    );
                }
            }

            // Update the `Bitboard`s and `Piece` array
            self.remove_piece_c(captured_piece, cap_sq);

            // Reset the 50-move clock
            self.half_move_clock = 0;
        }

        if !mov.is_castle() {
            self.move_piece_c(moving_piece, from, to);
        }

        // Extra book-keeping for pawn moves
        if moving_piece.type_of() == PieceType::Pawn {
            if to.0 ^ from.0 == 16 {
                // Double push
                let poss_ep: u8 = (to.0 as i8 - us.pawn_push()) as u8;

                // Set en passant square if the moved pawn can be captured
                if (Bitboard(pawn_attacks_from(Square(poss_ep), us))
                    & self.piece_bb(them, PieceType::Pawn))
                .is_not_empty()
                {
                    self.ep_square = Some(Square(poss_ep));
                }
            } else if let Some(promo_piece_type) = mov.promo_piece_type() {
                let us_promo = Piece::make(us, promo_piece_type);
                self.remove_piece_c(moving_piece, to);
                self.put_piece_c(us_promo, to);
            }

            self.half_move_clock = 0;
        }

        // Update "invisible" state
        self.turn = them;
        self.state = Some(State::from_position(&self));
    }

    /// Unmake the most recent move, returning the `Position` to the previous state.
    pub fn unmake_move(&mut self) -> Option<UndoableMove> {
        if let Some(undoable_move) = self.history.pop() {
            self.turn = !self.turn();
            let us = self.turn();
            let orig = undoable_move.orig;
            let dest = undoable_move.dest;
            let mut piece_on = self.piece_at_sq(dest);

            // Sanity check (only in debug mode) that the move makes sense.
            debug_assert!(self.piece_at_sq(orig) == Piece::None || undoable_move.is_castle());

            if undoable_move.is_promo() {
                debug_assert_eq!(piece_on.type_of(), undoable_move.promo_piece_type.unwrap());

                self.remove_piece_c(piece_on, dest);
                self.put_piece_c(Piece::make(us, PieceType::Pawn), dest);
                piece_on = Piece::make(us, PieceType::Pawn);
            }

            if undoable_move.is_castle() {
                self.undo_castling(us, orig, dest);
            } else {
                self.move_piece_c(piece_on, dest, orig);
                let captured_piece = undoable_move.captured;
                if !captured_piece.is_none() {
                    let mut cap_sq = dest;
                    if undoable_move.is_en_passant() {
                        match us {
                            Player::White => cap_sq -= Square(8),
                            Player::Black => cap_sq += Square(8),
                        };
                    }
                    self.put_piece_c(Piece::make(!us, captured_piece), cap_sq);
                }
            }
            self.half_move_clock = undoable_move.prev_half_move_clock;
            self.ep_square = undoable_move.prev_ep_square;
            self.castling_rights = undoable_move.prev_castling_rights;
            self.state = Some(undoable_move.state);

            if us == Player::Black {
                // unmaking a Black move, so decrement the whole move counter
                self.move_number -= 1;
            }

            Some(undoable_move)
        } else {
            None
        }
    }

    /// Helper function to apply a castling move for a given player.
    ///
    /// Takes in the player to castle, the original king square and the original rook square.
    /// The k_dst and r_dst squares are pointers to values, modifying them to have the correct king and
    /// rook destination squares.
    ///
    /// # Safety
    ///
    /// Assumes that k_orig and r_orig are legal squares, and the player can legally castle.
    fn apply_castling(
        &mut self,
        player: Player,
        k_orig: Square,      // Starting square of the King
        k_dest: Square,      // King destination square
        r_orig: &mut Square, // Origin square of the Rook. Passed in as `Square(0)` and modified by the function
        r_dest: &mut Square, // Destination square of Rook. Passed in as `Square(0)` and modified by the function
    ) {
        if k_orig < k_dest {
            // Kingside castling
            *r_orig = player.relative_square(Square::H1);
            *r_dest = player.relative_square(Square::F1);
        } else {
            // Queenside castling
            *r_orig = player.relative_square(Square::A1);
            *r_dest = player.relative_square(Square::D1);
        }
        self.move_piece_c(Piece::make(player, PieceType::King), k_orig, k_dest);
        self.move_piece_c(Piece::make(player, PieceType::Rook), *r_orig, *r_dest);
    }

    /// Helper function to undo a castling move for a given player.
    ///
    /// # Safety
    ///
    /// Undefined behaviour will result if calling this function when not unmaking an actual
    /// castling move.
    fn undo_castling(&mut self, player: Player, k_orig: Square, k_dest: Square) {
        let r_orig: Square;
        let r_dest: Square;
        if k_orig < k_dest {
            // Kingside castling
            r_orig = player.relative_square(Square::H1);
            r_dest = player.relative_square(Square::F1);
        } else {
            // Queenside castling
            r_orig = player.relative_square(Square::A1);
            r_dest = player.relative_square(Square::D1);
        }

        debug_assert_eq!(
            self.piece_at_sq(r_dest),
            Piece::make(player, PieceType::Rook)
        );
        debug_assert_eq!(
            self.piece_at_sq(k_dest),
            Piece::make(player, PieceType::King)
        );

        self.move_piece_c(Piece::make(player, PieceType::King), k_dest, k_orig);
        self.move_piece_c(Piece::make(player, PieceType::Rook), r_dest, r_orig);
    }

    /// Makes the given uci move on the board if it's legal.
    ///
    /// Returns `true` if the move was legal and successfully applied on the board,
    /// otherwise `false`.
    pub fn make_uci_move(&mut self, uci: &str) -> Option<Move> {
        let moves = MoveGen::generate_legal(&self);

        for mov in moves {
            let uci_mov = mov.to_uci_string();
            if uci == uci_mov {
                self.make_move(mov);
                return Some(mov);
            }
        }

        return None;
    }

    /// Moves a piece on the board for a given player from square `from`
    /// to square `to`. Updates all relevant `Bitboard` and the `Piece` array.
    ///
    /// # Panics
    ///
    /// Panics in debug mode if the two and from square are equal
    fn move_piece_c(&mut self, piece: Piece, from: Square, to: Square) {
        debug_assert_ne!(from, to);
        let comb_bb: Bitboard = from.to_bb() | to.to_bb();
        let (player, piece_ty) = piece.player_piece();
        self.no_piece ^= comb_bb;

        match piece {
            Piece::None => {}
            Piece::WhitePawn => {
                self.white_pawns ^= comb_bb;
            }
            Piece::WhiteKnight => {
                self.white_knights ^= comb_bb;
            }
            Piece::WhiteBishop => {
                self.white_bishops ^= comb_bb;
            }
            Piece::WhiteRook => {
                self.white_rooks ^= comb_bb;
            }
            Piece::WhiteQueen => {
                self.white_queens ^= comb_bb;
            }
            Piece::WhiteKing => {
                self.white_king ^= comb_bb;
            }
            Piece::BlackPawn => {
                self.black_pawns ^= comb_bb;
            }
            Piece::BlackKnight => {
                self.black_knights ^= comb_bb;
            }
            Piece::BlackBishop => {
                self.black_bishops ^= comb_bb;
            }
            Piece::BlackRook => {
                self.black_rooks ^= comb_bb;
            }
            Piece::BlackQueen => {
                self.black_queens ^= comb_bb;
            }
            Piece::BlackKing => {
                self.black_king ^= comb_bb;
            }
        }

        match player {
            Player::White => self.white_pieces ^= comb_bb,
            Player::Black => self.black_pieces ^= comb_bb,
        }

        self.board.remove(from);
        self.board.place(to, player, piece_ty);
    }

    /// Removes a `Piece` from the board for a given player.
    ///
    /// # Panics
    ///
    /// In debug mode, panics if there is not a `piece` at the given square.
    fn remove_piece_c(&mut self, piece: Piece, square: Square) {
        debug_assert_eq!(self.piece_at_sq(square), piece);
        let (player, piece_ty) = piece.player_piece();
        let bb = square.to_bb();
        self.no_piece ^= bb;

        // TODO: factor this out into a function. The same thing is being done in `move_piece_c`
        match piece {
            Piece::None => {}
            Piece::WhitePawn => {
                self.white_pawns ^= bb;
            }
            Piece::WhiteKnight => {
                self.white_knights ^= bb;
            }
            Piece::WhiteBishop => {
                self.white_bishops ^= bb;
            }
            Piece::WhiteRook => {
                self.white_rooks ^= bb;
            }
            Piece::WhiteQueen => {
                self.white_queens ^= bb;
            }
            Piece::WhiteKing => {
                self.white_king ^= bb;
            }
            Piece::BlackPawn => {
                self.black_pawns ^= bb;
            }
            Piece::BlackKnight => {
                self.black_knights ^= bb;
            }
            Piece::BlackBishop => {
                self.black_bishops ^= bb;
            }
            Piece::BlackRook => {
                self.black_rooks ^= bb;
            }
            Piece::BlackQueen => {
                self.black_queens ^= bb;
            }
            Piece::BlackKing => {
                self.black_king ^= bb;
            }
        }

        match player {
            Player::White => {
                self.white_pieces ^= bb;
                self.white_piece_count -= 1;
            }

            Player::Black => {
                self.black_pieces ^= bb;
                self.black_piece_count -= 1;
            }
        }

        self.board.remove(square);
    }

    /// Places a `Piece` on the board at a given `Square`.
    ///
    /// # Safety
    ///
    /// In debug mode, panics if there is already a piece at that `Square`.
    fn put_piece_c(&mut self, piece: Piece, square: Square) {
        debug_assert_eq!(self.piece_at_sq(square), Piece::None);

        let bb = square.to_bb();
        let (player, piece_ty) = piece.player_piece();
        self.no_piece ^= bb;

        // TODO: factor this out into a function. The same thing is being done in `move_piece_c`
        match piece {
            Piece::None => {}
            Piece::WhitePawn => {
                self.white_pawns ^= bb;
            }
            Piece::WhiteKnight => {
                self.white_knights ^= bb;
            }
            Piece::WhiteBishop => {
                self.white_bishops ^= bb;
            }
            Piece::WhiteRook => {
                self.white_rooks ^= bb;
            }
            Piece::WhiteQueen => {
                self.white_queens ^= bb;
            }
            Piece::WhiteKing => {
                self.white_king ^= bb;
            }
            Piece::BlackPawn => {
                self.black_pawns ^= bb;
            }
            Piece::BlackKnight => {
                self.black_knights ^= bb;
            }
            Piece::BlackBishop => {
                self.black_bishops ^= bb;
            }
            Piece::BlackRook => {
                self.black_rooks ^= bb;
            }
            Piece::BlackQueen => {
                self.black_queens ^= bb;
            }
            Piece::BlackKing => {
                self.black_king ^= bb;
            }
        }

        match player {
            Player::White => {
                self.white_pieces ^= bb;
                self.white_piece_count += 1;
            }

            Player::Black => {
                self.black_pieces ^= bb;
                self.black_piece_count += 1;
            }
        }
        self.board.place(square, player, piece_ty);
    }

    // CHECKING
    /// Returns if current side to move is in check.
    #[inline(always)]
    pub fn in_check(&self) -> bool {
        // TODO: do something better with the unwrap
        self.state.as_ref().unwrap().checkers.is_not_empty()
    }

    /// Returns a `Bitboard` of possible attacks to a square with a given occupancy.
    /// Includes pieces from both players.
    // TODO: is there any need to pass `occupied` here? Isn't it already available on `self`?
    pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
        (Bitboard(pawn_attacks_from(sq, Player::Black))
            & self.piece_bb(Player::White, PieceType::Pawn))
            | (Bitboard(pawn_attacks_from(sq, Player::White)))
                & self.piece_bb(Player::Black, PieceType::Pawn)
            | (knight_moves(sq) & self.piece_bb_both_players(PieceType::Knight))
            | (rook_moves(occupied, sq)
                & (self.white_rooks | self.black_rooks | self.white_queens | self.black_queens))
            | (bishop_moves(occupied, sq)
                & (self.white_bishops | self.black_bishops | self.white_queens | self.black_queens))
            | (king_moves(sq) & (self.white_king | self.black_king))
    }

    #[inline]
    pub fn turn(&self) -> Player {
        self.turn
    }

    #[inline]
    pub fn occupied(&self) -> Bitboard {
        !self.no_piece
    }

    #[inline]
    pub fn get_occupied_player(&self, player: Player) -> Bitboard {
        match player {
            Player::White => self.white_pieces,
            Player::Black => self.black_pieces,
        }
    }

    #[inline]
    pub fn occupied_white(&self) -> Bitboard {
        self.white_pieces
    }

    #[inline]
    pub fn occupied_black(&self) -> Bitboard {
        self.black_pieces
    }

    /// Outputs the blockers and pinners of a given square in a tuple `(blockers, pinners)`.
    pub fn slider_blockers(&self, sliders: Bitboard, sq: Square) -> (Bitboard, Bitboard) {
        let mut blockers = Bitboard(0);
        let mut pinners = Bitboard(0);
        let occupied = self.occupied();

        let attackers = sliders
            & ((rook_moves(Bitboard(0), sq)
                & (self.piece_two_bb_both_players(PieceType::Rook, PieceType::Queen)))
                | (bishop_moves(Bitboard(0), sq)
                    & (self.piece_two_bb_both_players(PieceType::Bishop, PieceType::Queen))));

        let player_at = self.board.piece_at_sq(sq).player();
        let other_occ = self.get_occupied_player(player_at);
        for attacker_sq in attackers {
            let bb = Bitboard(between_bb(sq, attacker_sq)) & occupied;
            if bb.is_not_empty() && !bb.more_than_one() {
                blockers |= bb;
                if (bb & other_occ).is_not_empty() {
                    pinners |= attacker_sq.to_bb();
                }
            }
        }

        (blockers, pinners)
    }

    #[inline]
    pub fn piece_bb(&self, player: Player, piece_type: PieceType) -> Bitboard {
        match player {
            Player::White => match piece_type {
                PieceType::None => Bitboard::ALL,
                PieceType::Pawn => self.white_pawns,
                PieceType::Knight => self.white_knights,
                PieceType::Bishop => self.white_bishops,
                PieceType::Rook => self.white_rooks,
                PieceType::Queen => self.white_queens,
                PieceType::King => self.white_king,
            },
            Player::Black => match piece_type {
                PieceType::None => Bitboard::ALL,
                PieceType::Pawn => self.black_pawns,
                PieceType::Knight => self.black_knights,
                PieceType::Bishop => self.black_bishops,
                PieceType::Rook => self.black_rooks,
                PieceType::Queen => self.black_queens,
                PieceType::King => self.black_king,
            },
        }
    }
    /// Returns the Bitboard of the Queens and Rooks for a given player.
    #[inline(always)]
    pub fn sliding_piece_bb(&self, player: Player) -> Bitboard {
        self.piece_two_bb(PieceType::Queen, PieceType::Rook, player)
    }
    /// Returns the Bitboard of the Queens and Bishops for a given player.
    #[inline(always)]
    pub fn diagonal_piece_bb(&self, player: Player) -> Bitboard {
        self.piece_two_bb(PieceType::Queen, PieceType::Bishop, player)
    }

    /// Returns the combined BitBoard of both players for a given piece.
    #[inline(always)]
    pub fn piece_bb_both_players(&self, piece: PieceType) -> Bitboard {
        match piece {
            PieceType::None => Bitboard(0),
            PieceType::Pawn => self.white_pawns | self.black_pawns,
            PieceType::Knight => self.white_knights | self.black_knights,
            PieceType::Bishop => self.white_bishops | self.black_bishops,
            PieceType::Rook => self.white_rooks | self.black_rooks,
            PieceType::Queen => self.white_queens | self.black_queens,
            PieceType::King => self.white_king | self.black_king,
        }
    }

    #[inline]
    pub fn piece_two_bb(
        &self,
        piece_type_1: PieceType,
        piece_type_2: PieceType,
        player: Player,
    ) -> Bitboard {
        self.piece_bb(player, piece_type_1) | self.piece_bb(player, piece_type_2)
    }

    #[inline]
    pub fn piece_two_bb_both_players(
        &self,
        piece_type_1: PieceType,
        piece_type_2: PieceType,
    ) -> Bitboard {
        self.piece_bb_both_players(piece_type_1) | self.piece_bb_both_players(piece_type_2)
    }
    /// Returns the `Piece` at the given `Square`
    #[inline]
    pub fn piece_at_sq(&self, sq: Square) -> Piece {
        self.board.piece_at_sq(sq)
    }

    /// Return the en passant square for the current position (usually `None` except
    /// after a double pawn push.
    #[inline]
    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// Returns the checkers `Bitboard` for the current position.
    #[inline]
    pub fn checkers(&self) -> Bitboard {
        // TODO: deal with the unwrap somehow
        self.state.as_ref().unwrap().checkers
    }

    /// Check if the castle path is impeded for the current player. Does not assume
    /// the current player has the ability to castle, whether by having castling-rights
    /// or having the rook and king be on the correct squares. Also does not check legality
    /// (i.e. ensuring none of the king squares are in check).
    #[inline]
    pub fn castle_impeded(&self, castle_type: CastleType) -> bool {
        let path = Bitboard(CASTLING_PATH[self.turn as usize][castle_type as usize]);
        (path & self.occupied()).is_not_empty()
    }

    /// Check if the given player can castle to the given side.
    #[inline]
    pub fn can_castle(&self, player: Player, side: CastleType) -> bool {
        if player == Player::White {
            if side == CastleType::Kingside {
                self.castling_rights.white_kingside
            } else {
                self.castling_rights.white_queenside
            }
        } else {
            if side == CastleType::Kingside {
                self.castling_rights.black_kingside
            } else {
                self.castling_rights.black_queenside
            }
        }
    }

    #[inline]
    pub fn castling_rook_square(&self, side: CastleType) -> Square {
        Square(CASTLING_ROOK_START[self.turn() as usize][side as usize])
    }

    /// Returns the king square for the given player.
    #[inline]
    pub fn king_sq(&self, player: Player) -> Square {
        self.piece_bb(player, PieceType::King).to_square()
    }

    /// Returns the pinned pieces of the given player.
    ///
    /// Pinned is defined as pinned to the same players king
    #[inline(always)]
    pub fn pinned_pieces(&self, player: Player) -> Bitboard {
        self.state
            .as_ref()
            .expect("tried to check state when it was not set")
            .blockers[player as usize]
            & self.get_occupied_player(player)
    }

    // MOVE TESTING
    /// Tests if a given pseudo-legal move is legal. Used for checking the legality
    /// of moves that are generated as pseudo-legal in movegen. Pseudo-legal moves
    /// can create a discovered check, or the moving side can move into check. The case
    /// of castling through check is already dealt with in movegen.
    pub fn legal_move(&self, mov: Move) -> bool {
        if mov.is_none() || mov.is_null() {
            return false;
        }

        let us = self.turn();
        let them = !us;
        let orig = mov.orig();
        let orig_bb = orig.to_bb();
        let dest = mov.dest();

        // En passant
        if mov.move_type() == SpecialMove::EnPassant {
            let ksq = self.king_sq(us);
            let dest_bb = dest.to_bb();
            let captured_sq = Square((dest.0 as i8).wrapping_sub(us.pawn_push()) as u8);
            // Work out the occupancy bb resulting from the en passant move
            let occupied = (self.occupied() ^ orig_bb ^ captured_sq.to_bb()) | dest_bb;

            return (rook_moves(occupied, ksq) & self.sliding_piece_bb(them)).is_empty()
                && (bishop_moves(occupied, ksq) & self.diagonal_piece_bb(them)).is_empty();
        }

        let piece = self.piece_at_sq(orig);
        if piece == Piece::None {
            return false;
        }

        // If moving the king, check if the destination square is not being attacked
        // Note: castling moves are already checked in movegen.
        if piece.type_of() == PieceType::King {
            return mov.move_type() == SpecialMove::Castling
                || (self.attackers_to(dest, self.occupied()) & self.get_occupied_player(them))
                    .is_empty();
        }

        // Ensure we are not moving a pinned piece, or if we are, it is remaining staying
        // pinned but moving along the current rank, file, diagonal between the pinner and the king
        (self.pinned_pieces(us) & orig_bb).is_empty() || aligned(orig, dest, self.king_sq(us))
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "")?;
        writeln!(f, "BITBOARDS\n=========\n")?;
        writeln!(f, "No Pieces:\n {}", self.no_piece)?;
        writeln!(f, "White Pawns:\n {}", self.white_pawns)?;
        writeln!(f, "White Knights:\n {}", self.white_knights)?;
        writeln!(f, "White Bishops:\n {}", self.white_bishops)?;
        writeln!(f, "White Rooks:\n {}", self.white_rooks)?;
        writeln!(f, "White Queens:\n {}", self.white_queens)?;
        writeln!(f, "White King:\n {}", self.white_king)?;
        writeln!(f, "Black Pawns:\n {}", self.black_pawns)?;
        writeln!(f, "Black Knights:\n {}", self.black_knights)?;
        writeln!(f, "Black Bishops:\n {}", self.black_bishops)?;
        writeln!(f, "Black Rooks:\n {}", self.black_rooks)?;
        writeln!(f, "Black Queens:\n {}", self.black_queens)?;
        writeln!(f, "Black King:\n {}", self.black_king)?;
        writeln!(f, "White Pieces:\n {}", self.white_pieces)?;
        writeln!(f, "Black Pieces:\n {}", self.black_pieces)?;

        writeln!(f, "BOARD ARRAY\n===========\n")?;
        writeln!(f, "{}", self.board)?;

        writeln!(f, "PIECE COUNTS\n============\n")?;
        writeln!(f, "White: {}", self.white_piece_count)?;
        writeln!(f, "Black: {}", self.black_piece_count)?;
        writeln!(f)?;

        writeln!(f, "INVISIBLE STATE\n===============\n")?;
        writeln!(f, "Turn: {}", self.turn())?;
        writeln!(f, "Castling Rights: {}", self.castling_rights)?;
        writeln!(
            f,
            "En Passant Square: {}",
            match self.ep_square {
                Some(sq) => sq.to_string(),
                None => "none".to_string(),
            }
        )?;
        writeln!(f, "Half move clock: {}", self.half_move_clock)?;
        writeln!(f, "Move number: {}", self.move_number)?;
        writeln!(f)?;
        writeln!(f, "STATE\n=====\n")?;

        if let Some(state) = &self.state {
            writeln!(f, "{}", state)?;
        } else {
            writeln!(f, "None")?;
        }
        writeln!(f)?;
        writeln!(f, "HISTORY\n=======")?;
        for mov in &self.history {
            write!(f, "{} ", mov)?;
        }
        writeln!(f)
    }
}

/// For whatever rank the bit (inner value of a `Square`) is, returns the
/// corresponding rank as a u64.
#[inline(always)]
pub fn rank_bb(s: u8) -> u64 {
    RANK_BB[rank_idx_of_sq(s) as usize]
}

/// For whatever rank the bit (inner value of a `Square`) is, returns the
/// corresponding `Rank` index.
#[inline(always)]
pub fn rank_idx_of_sq(s: u8) -> u8 {
    (s >> 3) as u8
}

/// For whatever file the bit (inner value of a `Square`) is, returns the
/// corresponding file as a u64.
#[inline(always)]
pub fn file_bb(s: u8) -> u64 {
    FILE_BB[file_of_sq(s) as usize]
}

/// For whatever file the bit (inner value of a `Square`) is, returns the
/// corresponding file.
// TODO: make this return a dedicated `File` enum
#[inline(always)]
pub fn file_of_sq(s: u8) -> u8 {
    s & 0b0000_0111
}

/// Given a square (u8) that is valid, returns the bitboard representation
/// of that square.
///
/// # Safety
///
/// If the input is greater than 63, an empty u64 will be returned.
#[inline]
pub fn u8_to_u64(s: u8) -> u64 {
    debug_assert!(s < 64);
    (1 as u64).wrapping_shl(s as u32)
}
