//! Tools for ordering and iterating moves in a search environment.
use super::score::Score;
use super::search::Search;

use core::mov::Move;
use core::movelist::{ArrayVec, BasicMoveList, MoveList, MAX_MOVES};
use core::position::Position;

use num::FromPrimitive;
use num_derive::FromPrimitive;

use std::mem::MaybeUninit;
use std::slice::Iter as SliceIter;

pub type ScoredMove = (Move, Score);

#[derive(Copy, Clone, Debug)]
struct Entry {
    sm: ScoredMove,
    yielded: bool,
}

/// An `ArrayVec` containing `ScoredMoves`.
#[derive(Debug)]
pub struct ScoredMoveList(ArrayVec<Entry, MAX_MOVES>);

/// An iterator over a `ScoredMoveList` which allows the `Move`s to be inspected and scores mutated.
pub struct Scorer<'a> {
    iter: <&'a mut [Entry] as IntoIterator>::IntoIter,
}

impl<'a> Iterator for Scorer<'a> {
    type Item = &'a mut ScoredMove;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|entry| &mut entry.sm)
    }
}

impl<'a> From<&'a mut [Entry]> for Scorer<'a> {
    fn from(val: &'a mut [Entry]) -> Self {
        Self {
            iter: val.into_iter(),
        }
    }
}

impl MoveList for ScoredMoveList {
    fn empty() -> Self {
        ScoredMoveList(ArrayVec::new())
    }

    fn push(&mut self, mv: Move) {
        let entry = Entry {
            sm: (mv, Score::zero()),
            yielded: false,
        };

        self.0.push_val(entry);
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

/// A selection sort over a mutable `Entry` slice.
///
/// In move ordering, selection sort is expected to work best, because pre-sorting the entire list
/// is wasted effort in the many cases where we get an early cutoff. Additionally, for a small list
/// of elements O(n^2) algorithms can outperform if they have a low constant factor relative to
/// O(n*log n) algorithms with more constant overhead.
#[derive(Debug)]
struct SelectionSort<'a> {
    segment: &'a mut [Entry],
}

impl<'a> Iterator for SelectionSort<'a> {
    type Item = &'a Move;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Each time we get a call to next, we want to iterate through the entire list and find the
        // largest value of `Score` among the entries which have not been yielded.
        // Then we want to set the `yielded` flag on that entry to `true`, and return a reference
        // to the move. When we get through the whole list without seeing a `yielded = false`, we
        // return `None`.
        let mut max = Score::INF_N;
        let mut max_entry: MaybeUninit<&mut Entry> = MaybeUninit::uninit();
        let mut found_one: bool = false;

        for entry in &mut *self.segment {
            if !entry.yielded && entry.sm.1 > max {
                max = entry.sm.1;
                max_entry.write(entry);
                found_one = true;
            }
        }

        if found_one {
            // SAFETY: we can assume this is initialized because `found_one` was true, which means
            // we wrote the entry into this.
            //
            // We can further transmute the lifetime to `'a` because we know we aren't modifying
            // the `Move` (only its `yielded` flag).
            unsafe {
                let max_entry = max_entry.assume_init();
                max_entry.yielded = true;
                Some(std::mem::transmute(max_entry))
            }
        } else {
            None
        }
    }
}

impl<'a> From<&'a mut [Entry]> for SelectionSort<'a> {
    fn from(segment: &'a mut [Entry]) -> Self {
        Self { segment }
    }
}

pub struct OrderedMoves {
    buf: ScoredMoveList,
    /// The index of the start of the current segment. A new segment is created each time the
    /// `Phase` increments. So in practice, we'll have a segment for the `HashMove`, a segment for
    /// promotions, a segment for captures, a segment for killer moves, and a segment for quiet
    /// moves.
    segment_start: usize,
    phase: Phase,
}

#[derive(Copy, Clone, Debug, FromPrimitive)]
#[repr(u8)]
pub enum Phase {
    /// Before the first phase has been loaded.
    Pre,
    /// The move currently stored in the HashTable for this position, if any.
    HashTable,
    /// Promotions to a queen, if any.
    QueenPromotions,
    /// Captures which have static exchange evaluation (SEE) > 0; i.e. expected to win material.
    GoodCaptures,
    /// Captures which have SEE = 0; i.e. expected to be neutral material.
    EqualCaptures,
    /// Quiet moves appearing in the killer tables. Such a move caused a cutoff at the same ply in
    /// another variation, and is therefore considered likely to have a similarly positive effect
    /// in this position too.
    Killers,
    /// All other quiet (i.e. non-capturing or promoting) moves. These are further sorted according
    /// to the history heuristic, which scores moves based on how many times have they have caused
    /// cutoffs elsewhere in the tree.
    Quiet,
    /// Captures which have SEE < 0; i.e. expected to lose material.
    BadCaptures,
    /// Promotions to anything other than a queen. In almost every instance, promoting to something
    /// other than a queen is pointless.
    Underpromotions,
}

impl Phase {
    pub fn inc(&mut self) -> bool {
        match FromPrimitive::from_u8(*self as u8 + 1) {
            Some(p) => {
                *self = p;
                true
            }
            None => false,
        }
    }
}

pub trait Loader {
    /// Load the hash move(s) into the passed `MoveList`.
    fn load_hash(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Load promotions into the passed `MoveList`.
    fn load_promotions(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Load captures into the passed `MoveList`.
    fn load_captures(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Provides an iterator over the capture moves, allowing the `Loader` to provide scores for
    /// each move.
    fn score_captures(&mut self, _scorer: Scorer) {}

    /// Load killers into the passed `MoveList`.
    fn load_killers(&mut self, _movelist: &mut ScoredMoveList) {}

    /// Load quiet moves into the passed `MoveList`.
    fn load_quiets(&mut self, _movelist: &mut ScoredMoveList) {}
}

impl OrderedMoves {
    pub fn new() -> Self {
        Self {
            buf: ScoredMoveList::empty(),
            segment_start: 0,
            phase: Phase::Pre,
        }
    }

    pub fn next_phase(&self) -> Phase {
        self.phase
    }

    pub fn load_next_phase<L: Loader>(&mut self, mut loader: L) -> bool {
        let res = self.phase.inc();
        if res {
            use Phase::*;
            match self.phase {
                Pre => {
                    unreachable!("since we have incremented, this can never happen");
                }
                HashTable => {
                    // No need to clear the buf here, because it is guaranteed to already be empty.
                    loader.load_hash(&mut self.buf);
                }
                QueenPromotions => {
                    self.buf.clear();
                }
                GoodCaptures => {
                    self.buf.clear();
                    loader.load_captures(&mut self.buf);
                    loader.score_captures(self.current_segment().into());
                }
                EqualCaptures => {
                    self.buf.clear();
                }
                Killers => {
                    self.buf.clear();
                }
                Quiet => {
                    self.buf.clear();
                }
                BadCaptures => {
                    self.buf.clear();
                }
                Underpromotions => {
                    self.buf.clear();
                }
            }
        }

        res
    }

    fn current_segment(&mut self) -> &mut [Entry] {
        // SAFETY: we know that the bounds passed are valid for this buffer.
        unsafe {
            self.buf
                .0
                .get_slice_mut_unchecked(self.segment_start..self.buf.len())
        }
    }
}

enum IterInner<'a> {
    Empty(std::iter::Empty<&'a Move>),
    Hash(SelectionSort<'a>),
    // QueenPromotions(QueenPromotionsIter),
    // GoodCaptures(GoodCapturesIter),
    // EqualCaptures(EqualCapturesIter),
    // Killers(KillersIter),
    // Quiet(QuietIter),
    // BadCaptures(BadCapturesIter),
    // Underpromotions(UnderpromotionsIter),
}

pub struct Iter<'a>(IterInner<'a>);

impl<'a> Iterator for IterInner<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        use IterInner::*;
        match self {
            Empty(i) => i.next(),
            Hash(i) => i.next(),
            //QueenPromotions(i) => i.next(),
            //GoodCaptures(i) => i.next(),
            //EqualCaptures(i) => i.next(),
            //Killers(i) => i.next(),
            //Quiet(i) => i.next(),
            //BadCaptures(i) => i.next(),
            //Underpromotions(i) => i.next(),
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Move;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self.0.next() {
            Some(m) => Some(m),
            None => {
                // cleanup the `OrderedMoveList` (self.om)
                None
            }
        }
    }
}

impl<'a> IntoIterator for &'a mut OrderedMoves {
    type Item = &'a Move;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        use Phase::*;
        let iter = match self.phase {
            Pre => IterInner::Empty(Default::default()),
            HashTable => IterInner::Hash(SelectionSort::from(self.current_segment())),
            QueenPromotions => IterInner::Empty(Default::default()),
            GoodCaptures => IterInner::Empty(Default::default()),
            EqualCaptures => IterInner::Empty(Default::default()),
            Killers => IterInner::Empty(Default::default()),
            Quiet => IterInner::Empty(Default::default()),
            BadCaptures => IterInner::Empty(Default::default()),
            Underpromotions => IterInner::Empty(Default::default()),
        };

        Iter(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perft::TESTS;

    struct Perft {
        pos: Position,
        count: usize,
    }

    impl Perft {
        pub fn perft(pos: Position, depth: usize) -> usize {
            let mut p = Perft { pos, count: 0 };

            p.perft_recurse(depth);
            p.count
        }

        fn perft_recurse(&mut self, depth: usize) {
            if depth == 1 {
                self.count += self.pos.generate_moves::<BasicMoveList>().len();
            } else {
                let mut moves = OrderedMoves::new();
                // TODO
                // while moves.next_phase(&mut self.pos) {
                //     for mov in &mut moves {
                //         self.pos.make_move(&mov);
                //         self.perft_recurse(depth - 1);
                //         self.pos.unmake_move();
                //     }
                // }
            }
        }
    }

    #[test]
    fn perft() {
        core::init::init_globals();

        for (p, d, r) in TESTS {
            let pos = Position::from_fen(p).unwrap();
            assert_eq!(Perft::perft(pos, d), r);
        }
    }
}
