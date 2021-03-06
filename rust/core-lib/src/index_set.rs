// Copyright 2017 The xi-editor Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A data structure for manipulating sets of indices (typically used for
//! representing valid lines).

// Note: this data structure has nontrivial overlap with Subset in the rope
// crate. Maybe we don't need both.

use std::cmp::{min, max};
use xi_rope::delta::{Delta, Transformer};
use xi_rope::rope::RopeInfo;

pub struct IndexSet {
    ranges: Vec<(usize, usize)>,
}

pub fn remove_n_at<T: Clone>(v: &mut Vec<T>, index: usize, n: usize) {
    if n == 1 {
        v.remove(index);
    } else if n > 1 {
        let new_len = v.len() - n;
        for i in index..new_len {
            v[i] = v[i + n].clone();
        }
        v.truncate(new_len);
    }
}

impl IndexSet {
    /// Create a new, empty set.
    pub fn new() -> IndexSet {
        IndexSet {
            ranges: Vec::new(),
        }
    }

    /// Clear the set.
    pub fn clear(&mut self) {
        self.ranges.clear();
    }

    /// Add the range start..end to the set.
    pub fn union_one_range(&mut self, start: usize, end: usize) {
        for i in 0..self.ranges.len() {
            let (istart, iend) = self.ranges[i];
            if start > iend {
                continue;
            } else if end < istart {
                self.ranges.insert(i, (start, end));
                return;
            } else {
                self.ranges[i].0 = min(start, istart);
                let mut j = i;
                while j + 1 < self.ranges.len() && end >= self.ranges[j + 1].0 {
                    j += 1;
                }
                self.ranges[i].1 = max(end, self.ranges[j].1);
                remove_n_at(&mut self.ranges, i + 1, j - i);
                return;
            }
        }
        self.ranges.push((start, end));
    }

    /// Deletes the given range from the set.
    pub fn delete_range(&mut self, start: usize, end: usize) {
        let mut ix = match self.ranges.binary_search_by(|r| r.1.cmp(&start)) {
            Ok(ix) => ix,
            Err(ix) => ix,
        };

        let mut del_from = None;
        let mut del_len = 0;
        while ix < self.ranges.len() {
            if self.ranges[ix].0 >= end {
                break;
            }

            if self.ranges[ix].0 < start {
                if self.ranges[ix].1 > end {
                    let range = (end, self.ranges[ix].1);
                    self.ranges.insert(ix+1, range);
                }
                self.ranges[ix].1 = start;
            } else if self.ranges[ix].1 > end {
                self.ranges[ix].0 = end;
            } else {
                if del_from.is_none() {
                    del_from = Some(ix);
                }
                del_len += 1;
            }

            ix += 1;
        }

        if let Some(del_from) = del_from {
            remove_n_at(&mut self.ranges, del_from, del_len);
        }
    }

    /// Return an iterator that yields start..end minus the coverage in this set.
    pub fn minus_one_range(&self, start: usize, end: usize) -> MinusIter {
        let mut ranges = &self.ranges[..];
        while !ranges.is_empty() && start >= ranges[0].1 {
            ranges = &ranges[1..];
        }
        MinusIter {
            ranges,
            start,
            end,
        }
    }

    /// Computes a new set based on applying a delta to the old set. Collapsed regions are removed
    /// and contiguous regions are combined.
    pub fn apply_delta(&self, delta: &Delta<RopeInfo>) -> IndexSet {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let mut transformer = Transformer::new(delta);
        for &(start, end) in &self.ranges {
            let new_range = (
                transformer.transform(start, false),
                transformer.transform(end, false)
            );
            if new_range.0 == new_range.1 {
                continue; // remove collapsed regions
            }
            if !ranges.is_empty() {
                let ix = ranges.len() - 1;
                if ranges[ix].1 == new_range.0 {
                    ranges[ix] = (ranges[ix].0, new_range.1);
                    continue;
                }
            }
            ranges.push(new_range);
        }
        IndexSet { ranges }
    }

    #[cfg(test)]
    fn get_ranges(&self) -> &[(usize, usize)] {
        &self.ranges
    }
}

/// The iterator generated by `minus_one_range`.
pub struct MinusIter<'a> {
    ranges: &'a [(usize, usize)],
    start: usize,
    end: usize,
}

impl<'a> Iterator for MinusIter<'a> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        while self.start < self.end {
            if self.ranges.is_empty() || self.end <= self.ranges[0].0 {
                let result = (self.start, self.end);
                self.start = self.end;
                return Some(result);
            }
            let result = (self.start, self.ranges[0].0);
            self.start = self.ranges[0].1;
            self.ranges = &self.ranges[1..];
            if result.1 > result.0 {
                return Some(result);
            }
        }
        None
    }
}

impl<'a> DoubleEndedIterator for MinusIter<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        while self.start < self.end {
            if self.ranges.is_empty() || self.ranges[self.ranges.len() - 1].1 <= self.start {
                let result = (self.start, self.end);
                self.start = self.end;
                return Some(result);
            }
            let last_ix = self.ranges.len() - 1;
            let result = (self.ranges[last_ix].1, self.end);
            self.end = self.ranges[last_ix].0;
            self.ranges = &self.ranges[..last_ix];
            if result.1 > result.0 {
                return Some(result);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::IndexSet;

    #[test]
    fn empty_behavior() {
        let e = IndexSet::new();
        assert_eq!(e.minus_one_range(0, 0).collect::<Vec<_>>(), vec![]);
        assert_eq!(e.minus_one_range(3, 5).collect::<Vec<_>>(), vec![(3, 5)]);
    }

    #[test]
    fn single_range_behavior() {
        let mut e = IndexSet::new();
        e.union_one_range(3, 5);
        assert_eq!(e.minus_one_range(0, 0).collect::<Vec<_>>(), vec![]);
        assert_eq!(e.minus_one_range(3, 5).collect::<Vec<_>>(), vec![]);
        assert_eq!(e.minus_one_range(0, 3).collect::<Vec<_>>(), vec![(0, 3)]);
        assert_eq!(e.minus_one_range(0, 4).collect::<Vec<_>>(), vec![(0, 3)]);
        assert_eq!(e.minus_one_range(4, 10).collect::<Vec<_>>(), vec![(5, 10)]);
        assert_eq!(e.minus_one_range(5, 10).collect::<Vec<_>>(), vec![(5, 10)]);
        assert_eq!(e.minus_one_range(0, 10).collect::<Vec<_>>(), vec![(0, 3), (5, 10)]);
    }

    #[test]
    fn two_range_minus() {
        let mut e = IndexSet::new();
        e.union_one_range(3, 5);
        e.union_one_range(7, 9);
        assert_eq!(e.minus_one_range(0, 0).collect::<Vec<_>>(), vec![]);
        assert_eq!(e.minus_one_range(3, 5).collect::<Vec<_>>(), vec![]);
        assert_eq!(e.minus_one_range(0, 3).collect::<Vec<_>>(), vec![(0, 3)]);
        assert_eq!(e.minus_one_range(0, 4).collect::<Vec<_>>(), vec![(0, 3)]);
        assert_eq!(e.minus_one_range(4, 10).collect::<Vec<_>>(), vec![(5, 7), (9, 10)]);
        assert_eq!(e.minus_one_range(5, 10).collect::<Vec<_>>(), vec![(5, 7), (9, 10)]);
        assert_eq!(e.minus_one_range(8, 10).collect::<Vec<_>>(), vec![(9, 10)]);
        assert_eq!(e.minus_one_range(0, 10).collect::<Vec<_>>(), vec![(0, 3), (5, 7), (9, 10)]);
    }

    #[test]
    fn minus_one_range_double_ended_iter() {
        let mut e = IndexSet::new();
        e.union_one_range(3, 5);
        e.union_one_range(7, 9);
        e.union_one_range(12, 15);

        let mut iter = e.minus_one_range(4, 13);
        assert_eq!(iter.next(), Some((5, 7)));
        assert_eq!(iter.next(), Some((9, 12)));
        assert_eq!(iter.next(), None);

        let mut iter = e.minus_one_range(4, 13);
        assert_eq!(iter.next_back(), Some((9, 12)));
        assert_eq!(iter.next_back(), Some((5, 7)));
        assert_eq!(iter.next_back(), None);

        let mut iter = e.minus_one_range(4, 13);
        assert_eq!(iter.next_back(), Some((9, 12)));
        assert_eq!(iter.next(), Some((5, 7)));
        assert_eq!(iter.next_back(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn unions() {
        let mut e = IndexSet::new();
        e.union_one_range(3, 5);
        assert_eq!(e.get_ranges(), &[(3, 5)]);
        e.union_one_range(7, 9);
        assert_eq!(e.get_ranges(), &[(3, 5), (7, 9)]);
        e.union_one_range(1, 2);
        assert_eq!(e.get_ranges(), &[(1, 2), (3, 5), (7, 9)]);
        e.union_one_range(2, 3);
        assert_eq!(e.get_ranges(), &[(1, 5), (7, 9)]);
        e.union_one_range(4, 6);
        assert_eq!(e.get_ranges(), &[(1, 6), (7, 9)]);
        assert_eq!(e.minus_one_range(0, 10).collect::<Vec<_>>(), vec![(0, 1), (6, 7), (9, 10)]);

        e.clear();
        assert_eq!(e.get_ranges(), &[]);
        e.union_one_range(3, 4);
        assert_eq!(e.get_ranges(), &[(3, 4)]);
        e.union_one_range(5, 6);
        assert_eq!(e.get_ranges(), &[(3, 4), (5, 6)]);
        e.union_one_range(7, 8);
        assert_eq!(e.get_ranges(), &[(3, 4), (5, 6), (7, 8)]);
        e.union_one_range(9, 10);
        assert_eq!(e.get_ranges(), &[(3, 4), (5, 6), (7, 8), (9, 10)]);
        e.union_one_range(11, 12);
        assert_eq!(e.get_ranges(), &[(3, 4), (5, 6), (7, 8), (9, 10), (11, 12)]);
        e.union_one_range(2, 10);
        assert_eq!(e.get_ranges(), &[(2, 10), (11, 12)]);
    }

    #[test]
    fn delete_range() {
        let mut e = IndexSet::new();
        e.union_one_range(1, 2);
        e.union_one_range(4, 6);
        e.union_one_range(6, 7);
        e.union_one_range(8, 8);
        e.union_one_range(10, 12);
        e.union_one_range(13, 14);
        e.delete_range(5, 11);
        assert_eq!(e.get_ranges(), &[(1, 2), (4, 5), (11, 12), (13, 14)]);

        let mut e = IndexSet::new();
        e.union_one_range(1, 2);
        e.union_one_range(4, 6);
        e.delete_range(2, 4);
        assert_eq!(e.get_ranges(), &[(1, 2), (4, 6)]);


        let mut e = IndexSet::new();
        e.union_one_range(0, 10);
        e.delete_range(4, 6);
        assert_eq!(e.get_ranges(), &[(0, 4), (6, 10)]);
    }

    #[test]
    fn apply_delta() {
        use xi_rope::delta::Delta;
        use xi_rope::interval::Interval;
        use xi_rope::rope::Rope;

        let mut e = IndexSet::new();
        e.union_one_range(1, 3);
        e.union_one_range(5, 9);

        let d = Delta::simple_edit(Interval::new_closed_open(2, 2), Rope::from("..."), 10);
        let s = e.apply_delta(&d);
        assert_eq!(s.get_ranges(), &[(1, 6), (8, 12)]);

        let d = Delta::simple_edit(Interval::new_closed_open(0, 3), Rope::from(""), 10);
        let s = e.apply_delta(&d);
        assert_eq!(s.get_ranges(), &[(2, 6)]);

        let d = Delta::simple_edit(Interval::new_closed_open(2, 6), Rope::from(""), 10);
        let s = e.apply_delta(&d);
        assert_eq!(s.get_ranges(), &[(1, 5)]);
    }
}
