//! Bounded snapshot history for canonical portable document values.

#[derive(Clone, Debug)]
pub(crate) struct PortableValueHistory<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
}

impl<T> PortableValueHistory<T> {
    pub(crate) const fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    pub(crate) fn record(&mut self, previous: T) {
        self.redo.clear();
        if self.limit == 0 {
            return;
        }
        self.undo.push(previous);
        let excess = self.undo.len().saturating_sub(self.limit);
        if excess != 0 {
            self.undo.drain(..excess);
        }
    }

    pub(crate) fn undo(&mut self, current: &mut T) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(current, previous));
        true
    }

    pub(crate) fn redo(&mut self, current: &mut T) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(current, next));
        true
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_undo_and_invalidates_redo_on_divergence() {
        let mut history = PortableValueHistory::with_limit(2);
        let mut value = 1;
        history.record(value);
        value = 2;
        history.record(value);
        value = 3;
        history.record(value);
        value = 4;
        assert!(history.undo(&mut value));
        assert_eq!(value, 3);
        assert!(history.undo(&mut value));
        assert_eq!(value, 2);
        assert!(!history.undo(&mut value));
        assert!(history.redo(&mut value));
        history.record(value);
        value = 9;
        assert!(!history.can_redo());
        assert_eq!(value, 9);
    }
}
