use crate::Command;
use lm_project::RomMutation;

/// Serializer output ready to cross the revision-checked application mutation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedRomCommit {
    pub expected_revision: u64,
    pub description: String,
    pub mutation: RomMutation,
}

impl PreparedRomCommit {
    #[must_use]
    pub fn into_command(self) -> Command {
        Command::CommitRomMutation {
            expected_revision: self.expected_revision,
            description: self.description,
            mutation: self.mutation,
        }
    }
}
