use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct LogEntry<E> {
    pub index: u64,
    pub term: u64,
    pub data: E,
}

pub trait Log: Send + 'static {
    type Event;

    fn at(&self, index: u64) -> Option<&LogEntry<Self::Event>>;
    fn last_term(&self) -> u64;
    fn last_index(&self) -> u64;
    fn try_append(
        &mut self,
        prev_term: u64,
        prev_index: u64,
        entries: Vec<LogEntry<Self::Event>>,
    ) -> Result<u64, u64>;
    fn push(&mut self, term: u64, event: Self::Event);
    fn slice(&self, from_index: u64, to_index: u64) -> &[LogEntry<Self::Event>];
    fn slice_to_end(&self, from_index: u64) -> &[LogEntry<Self::Event>];
}

pub struct VecLog<E>(Vec<LogEntry<E>>);

impl<E: Send + 'static> Log for VecLog<E> {
    type Event = E;

    fn at(&self, index: u64) -> Option<&LogEntry<E>> {
        let vec_index = self.to_vec_index(index)?;

        self.0.get(vec_index)
    }

    fn last_term(&self) -> u64 {
        self.0.last().map(|entry| entry.term).unwrap_or(0)
    }

    fn last_index(&self) -> u64 {
        self.0.last().map(|entry| entry.index).unwrap_or(0)
    }

    fn try_append(
        &mut self,
        prev_term: u64,
        prev_index: u64,
        mut entries: Vec<LogEntry<E>>,
    ) -> Result<u64, u64> {
        if prev_index == 0 && prev_term == 0 {
            self.0.append(&mut entries);
            return Ok(self.last_index());
        }
        if let Some(entry) = self.at(prev_index) {
            if entry.term == prev_term {
                self.clear_from_index(prev_index + 1);
                self.0.append(&mut entries);
                Ok(self.last_index())
            } else {
                self.clear_from_index(prev_index);
                Err(self.last_index())
            }
        } else {
            Err(self.last_index())
        }
    }

    fn push(&mut self, term: u64, event: E) {
        self.0.push(LogEntry {
            index: self.last_index() + 1,
            term,
            data: event,
        });
    }

    fn slice_to_end(&self, from_index: u64) -> &[LogEntry<E>] {
        if let Some(vec_index) = self.to_vec_index(from_index) {
            &self.0[vec_index..]
        } else {
            &[]
        }
    }

    // slice range [from_index, to_index)
    fn slice(&self, from_index: u64, to_index: u64) -> &[LogEntry<E>] {
        match (self.to_vec_index(from_index), self.to_vec_index(to_index)) {
            (Some(from), Some(to)) => &self.0[from..to],
            _ => &[],
        }
    }
}

impl<E> VecLog<E> {
    pub fn new() -> Self {
        VecLog(Vec::new())
    }

    fn clear_from_index(&mut self, index: u64) {
        if let Some(vec_index) = self.to_vec_index(index) {
            self.0.truncate(vec_index);
        }
    }

    // convert log's index to Vec's index
    // None if log's index not found
    #[inline]
    fn to_vec_index(&self, index: u64) -> Option<usize> {
        let last_index = self.0.last()?.index as usize;
        let last_vec_index = self.0.len() - 1;
        let vec_index: isize = last_vec_index as isize - (last_index as isize - index as isize);
        if vec_index >= 0 {
            Some(vec_index as usize)
        } else {
            None
        }
    }
}

mod tests {
    #[test]
    fn test_append_entries_success() {
        use super::{Log, LogEntry};
        let mut log = super::VecLog::new();
        log.push(1, 1);
        log.push(1, 2);
        log.push(1, 3);

        let result = log.try_append(
            1,
            3,
            vec![LogEntry {
                index: 4,
                term: 1,
                data: 4,
            }],
        );
        assert_eq!(Ok(4), result);
        assert_eq!(4, log.last_index());
        assert_eq!(1, log.last_term());
        let result = log.try_append(
            1,
            3,
            vec![LogEntry {
                index: 4,
                term: 1,
                data: 4,
            }],
        );
        assert_eq!(Ok(4), result);
        assert_eq!(4, log.last_index());
        assert_eq!(1, log.last_term());
        let result = log.try_append(
            1,
            3,
            vec![
                LogEntry {
                    index: 4,
                    term: 1,
                    data: 4,
                },
                LogEntry {
                    index: 5,
                    term: 2,
                    data: 5,
                },
            ],
        );
        assert_eq!(Ok(5), result);
        assert_eq!(5, log.last_index());
        assert_eq!(2, log.last_term());
    }

    #[test]
    fn test_append_entries_failure() {
        use super::{Log, LogEntry};
        let mut log = super::VecLog::new();
        log.push(1, 1);
        log.push(1, 2);
        log.push(1, 3);

        let result = log.try_append(
            1,
            4,
            vec![LogEntry {
                index: 5,
                term: 2,
                data: 5,
            }],
        );
        assert_eq!(Err(3), result);
        // Conflict index 3
        // Current
        // [1, 1, 1]
        // Incoming
        // [1, 1, 2, 2]
        let result = log.try_append(
            2,
            3,
            vec![LogEntry {
                index: 4,
                term: 2,
                data: 4,
            }],
        );
        assert_eq!(Err(2), result);
    }
}
