use crate::ClipboardKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSelection {
    pub kind: ClipboardKind,
    indices: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionError {
    Empty,
    DuplicateIndex(usize),
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid editor selection: {self:?}")
    }
}

impl std::error::Error for SelectionError {}

impl EditorSelection {
    /// Constructs a nonempty, canonical selection in ascending index order.
    ///
    /// # Errors
    ///
    /// Returns [`SelectionError`] for an empty selection or duplicate indexes.
    pub fn new(kind: ClipboardKind, mut indices: Vec<usize>) -> Result<Self, SelectionError> {
        if indices.is_empty() {
            return Err(SelectionError::Empty);
        }
        indices.sort_unstable();
        if let Some(index) = indices
            .windows(2)
            .find_map(|pair| (pair[0] == pair[1]).then_some(pair[0]))
        {
            return Err(SelectionError::DuplicateIndex(index));
        }
        Ok(Self { kind, indices })
    }

    #[must_use]
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selections_are_sorted_unique_and_nonempty() {
        let selection = EditorSelection::new(ClipboardKind::Map16Tiles, vec![9, 2, 5]).unwrap();
        assert_eq!(selection.indices(), [2, 5, 9]);
        assert_eq!(
            EditorSelection::new(ClipboardKind::Map16Tiles, vec![]),
            Err(SelectionError::Empty)
        );
        assert_eq!(
            EditorSelection::new(ClipboardKind::Map16Tiles, vec![2, 2]),
            Err(SelectionError::DuplicateIndex(2))
        );
    }
}
