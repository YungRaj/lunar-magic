use lm_level::{
    CustomObjectEntry, CustomObjectLibrary, CustomObjectLibraryError, DescriptionFormat,
};

/// One application-level mutation of a paired custom-object library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomObjectLibraryEdit {
    Insert {
        index: usize,
        entry: CustomObjectEntry,
    },
    Replace {
        index: usize,
        entry: CustomObjectEntry,
    },
    Remove {
        index: usize,
    },
    Move {
        from: usize,
        to: usize,
    },
    SetDescriptionFormat(DescriptionFormat),
}

pub(super) fn apply_edit(
    library: &mut CustomObjectLibrary,
    edit: &CustomObjectLibraryEdit,
) -> Result<(), CustomObjectLibraryError> {
    match edit {
        CustomObjectLibraryEdit::Insert { index, entry } => library.insert(*index, entry.clone()),
        CustomObjectLibraryEdit::Replace { index, entry } => {
            library.replace(*index, entry.clone()).map(drop)
        }
        CustomObjectLibraryEdit::Remove { index } => library.remove(*index).map(drop),
        CustomObjectLibraryEdit::Move { from, to } => library.move_entry(*from, *to),
        CustomObjectLibraryEdit::SetDescriptionFormat(format) => {
            library.set_description_format(*format)
        }
    }
}
