//! Which photo the folder scan analyses next.
//!
//! A scan works through a folder in the order it was given: outward from
//! the photo that was on screen when it began. That is the right order for
//! as long as the photographer stays there. Jump to the far end of a large
//! folder and the photo now being looked at was the last the scan would
//! reach — a face panel saying "Analyzing…" for minutes.
//!
//! So the scan is also told, as it goes, which photos are being looked at
//! now, and takes those before carrying on where it was. Nothing is
//! analysed twice and nothing is skipped; only the order changes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The photos still to analyse, handed out one at a time to whichever
/// worker asks.
pub struct Queue {
    todo: Vec<usize>,
    /// Where each photo is in `todo`.
    place: HashMap<usize, usize>,
    taken: Vec<AtomicBool>,
    next: AtomicUsize,
}

impl Queue {
    /// `todo` is the photos to analyse, by their number in the folder, in
    /// the order to work through them when nothing else is asked for.
    /// A photo given more than once is analysed once, where it first appears.
    pub fn new(todo: Vec<usize>) -> Self {
        let mut seen = std::collections::HashSet::new();
        let todo: Vec<usize> = todo.into_iter().filter(|i| seen.insert(*i)).collect();
        let place = todo.iter().enumerate().map(|(at, &index)| (index, at)).collect();
        let taken = todo.iter().map(|_| AtomicBool::new(false)).collect();
        Self { todo, place, taken, next: AtomicUsize::new(0) }
    }

    pub fn len(&self) -> usize {
        self.todo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.todo.is_empty()
    }

    /// The next photo to analyse: the first of `front` not yet handed out,
    /// or failing that the next in the scan's own order. `None` when every
    /// photo has been.
    pub fn take(&self, front: &[usize]) -> Option<usize> {
        for index in front {
            if let Some(&at) = self.place.get(index) {
                if !self.taken[at].swap(true, Ordering::SeqCst) {
                    return Some(*index);
                }
            }
        }
        loop {
            let at = self.next.fetch_add(1, Ordering::SeqCst);
            let index = *self.todo.get(at)?;
            if !self.taken[at].swap(true, Ordering::SeqCst) {
                return Some(index);
            }
        }
    }

    /// Whether every photo has been handed out.
    pub fn exhausted(&self) -> bool {
        self.taken.iter().all(|t| t.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_alone_it_works_through_in_the_order_given() {
        let queue = Queue::new(vec![5, 4, 6, 3, 7]);
        let got: Vec<usize> = std::iter::from_fn(|| queue.take(&[])).collect();
        assert_eq!(got, [5, 4, 6, 3, 7]);
        assert!(queue.exhausted());
    }

    #[test]
    fn the_photos_being_looked_at_come_first_and_then_it_carries_on_where_it_was() {
        let queue = Queue::new((0..10).collect());
        assert_eq!(queue.take(&[]), Some(0));
        // Jumped to the far end: that photo, then the ones beside it.
        assert_eq!(queue.take(&[9, 8]), Some(9));
        assert_eq!(queue.take(&[9, 8]), Some(8));
        // Those are done; back to where it was.
        assert_eq!(queue.take(&[9, 8]), Some(1));
        let rest: Vec<usize> = std::iter::from_fn(|| queue.take(&[])).collect();
        assert_eq!(rest, [2, 3, 4, 5, 6, 7], "nothing twice, nothing missed");
        assert!(queue.exhausted());
    }

    #[test]
    fn a_photo_already_analysed_or_not_in_the_folder_is_passed_over() {
        // 2 was remembered from last time and is not in the list at all.
        let queue = Queue::new(vec![0, 1, 3]);
        assert_eq!(queue.take(&[2, 99, 3]), Some(3));
        assert_eq!(queue.take(&[2, 99, 3]), Some(0));
    }

    #[test]
    fn workers_asking_at_once_are_never_given_the_same_photo() {
        let queue = Queue::new((0..2000).collect());
        let front: Vec<usize> = (1500..1600).collect();
        let got: Vec<Vec<usize>> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8).map(|_| scope.spawn(|| std::iter::from_fn(|| queue.take(&front)).collect::<Vec<_>>())).collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        let mut all: Vec<usize> = got.into_iter().flatten().collect();
        all.sort_unstable();
        assert_eq!(all, (0..2000).collect::<Vec<_>>());
    }

    #[test]
    fn a_photo_given_twice_is_analysed_once() {
        let q = Queue::new(vec![3, 1, 3, 2, 1]);
        let mut given = Vec::new();
        while let Some(i) = q.take(&[]) {
            given.push(i);
        }
        assert_eq!(given, [3, 1, 2]);
    }

    #[test]
    fn an_empty_folder_has_nothing_to_give() {
        let queue = Queue::new(vec![]);
        assert!(queue.is_empty());
        assert_eq!(queue.take(&[3]), None);
        assert!(queue.exhausted());
    }
}
