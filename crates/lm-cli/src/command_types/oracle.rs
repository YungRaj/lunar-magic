use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OracleOwnership {
    None,
    ChangedRats,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleCaptureCommand {
    pub case_id: String,
    pub lunar_magic_version: String,
    pub operation: String,
    pub before: PathBuf,
    pub after: PathBuf,
    pub decoded_before: PathBuf,
    pub decoded_after: PathBuf,
    pub ownership: OracleOwnership,
    pub output: PathBuf,
    pub arguments: Vec<(String, String)>,
}
