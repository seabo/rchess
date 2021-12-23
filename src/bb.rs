use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq)]
// TODO: internal u64 field should not be pub
pub struct Bitboard(u64);

pub const FIRST_RANK: Bitboard = Bitboard(0x00000000000000FF);
pub const SECOND_RANK: Bitboard = Bitboard(0x000000000000FF00);
pub const THIRD_RANK: Bitboard = Bitboard(0x0000000000FF0000);
pub const FOURTH_RANK: Bitboard = Bitboard(0x00000000FF000000);
pub const FIFTH_RANK: Bitboard = Bitboard(0x000000FF00000000);
pub const SIXTH_RANK: Bitboard = Bitboard(0x0000FF0000000000);
pub const SEVENTH_RANK: Bitboard = Bitboard(0x00FF000000000000);
pub const EIGHTH_RANK: Bitboard = Bitboard(0xFF00000000000000);

pub const WHITE_LEFT_PAWN_CAPTURE_MASK: Bitboard = Bitboard(0xFEFEFEFEFEFE0000);
pub const WHITE_RIGHT_PAWN_CAPTURE_MASK: Bitboard = Bitboard(0xEFEFEFEFEFEF0000);

impl Bitboard {
    pub fn new(bb: u64) -> Self {
        Bitboard(bb)
    }

    pub fn from_sq_idx(sq: u8) -> Self {
        Bitboard(1 << sq)
    }

    #[inline(always)]
    pub fn popcnt(&self) -> u32 {
        self.0.count_ones()
    }

    #[inline(always)]
    pub fn bsf(&self) -> u32 {
        self.0.trailing_zeros()
    }

    #[inline(always)]
    pub fn toggle_lsb(&mut self) {
        *self &= *self - (1 as u64)
    }
}

impl std::ops::Add for Bitboard {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left + right),
        }
    }
}

impl std::ops::AddAssign for Bitboard {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other
    }
}

impl std::ops::Sub for Bitboard {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left - right),
        }
    }
}

impl std::ops::SubAssign for Bitboard {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other
    }
}

impl std::ops::Add<u64> for Bitboard {
    type Output = Self;

    fn add(self, other: u64) -> Self::Output {
        match self {
            Bitboard(left) => Bitboard(left + other),
        }
    }
}

impl std::ops::AddAssign<u64> for Bitboard {
    fn add_assign(&mut self, other: u64) {
        *self = *self + other
    }
}

impl std::ops::Sub<u64> for Bitboard {
    type Output = Self;

    fn sub(self, other: u64) -> Self::Output {
        match self {
            Bitboard(left) => Bitboard(left - other),
        }
    }
}

impl std::ops::SubAssign<u64> for Bitboard {
    fn sub_assign(&mut self, other: u64) {
        *self = *self - other
    }
}

impl std::ops::BitAnd for Bitboard {
    type Output = Self;

    fn bitand(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left & right),
        }
    }
}

impl std::ops::BitAndAssign for Bitboard {
    fn bitand_assign(&mut self, other: Self) {
        *self = *self & other;
    }
}

impl std::ops::BitOr for Bitboard {
    type Output = Self;

    fn bitor(self, other: Self) -> Self::Output {
        match (self, other) {
            (Bitboard(left), Bitboard(right)) => Bitboard(left | right),
        }
    }
}

impl std::ops::BitOrAssign for Bitboard {
    fn bitor_assign(&mut self, other: Self) {
        *self = *self | other;
    }
}

impl std::ops::Not for Bitboard {
    type Output = Self;

    fn not(self) -> Bitboard {
        match self {
            Bitboard(bb) => Bitboard(!bb),
        }
    }
}

impl std::ops::Shl<usize> for Bitboard {
    type Output = Self;

    fn shl(self, shift: usize) -> Bitboard {
        match self {
            Bitboard(bb) => Bitboard(bb << shift),
        }
    }
}

impl std::ops::Shr<usize> for Bitboard {
    type Output = Self;

    fn shr(self, shift: usize) -> Self::Output {
        match self {
            Bitboard(bb) => Bitboard(bb >> shift),
        }
    }
}

// impl std::iter::IntoIterator for Bitboard {
//     type Item = u32;
//     type IntoIter = Bitboard;

//     fn into_iter(&self) -> Self::IntoIter {
//         self.clone()
//     }
// }

impl std::iter::Iterator for Bitboard {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        match self.bsf() {
            64 => None,
            x => {
                self.toggle_lsb();
                Some(x)
            }
        }
    }
}

impl fmt::Display for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut squares: [[u8; 8]; 8] = [[0; 8]; 8];

        for i in 0..64 {
            let rank = i / 8;
            let file = i % 8;
            let x: Bitboard = Bitboard(1 << i);
            if x & *self != Bitboard(0) {
                squares[rank][file] = 1;
            }
        }

        writeln!(f, "")?;
        writeln!(f, "   ┌────────────────────────┐")?;
        for (i, row) in squares.iter().rev().enumerate() {
            write!(f, " {} │", 8 - i)?;
            for square in row {
                write!(f, " {} ", square)?;
            }
            write!(f, "│\n")?;
        }
        writeln!(f, "   └────────────────────────┘")?;
        writeln!(f, "     a  b  c  d  e  f  g  h ")
    }
}
