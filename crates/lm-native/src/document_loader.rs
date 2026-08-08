//! Non-blocking bounded reads for native document-open workflows.

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Debug)]
pub(crate) struct BoundedRead {
    pub(crate) path: PathBuf,
    pub(crate) maximum: u64,
    pub(crate) description: String,
    optional: bool,
    prefix: bool,
}

impl BoundedRead {
    pub(crate) fn new(path: PathBuf, maximum: u64, description: impl Into<String>) -> Self {
        Self {
            path,
            maximum,
            description: description.into(),
            optional: false,
            prefix: false,
        }
    }

    /// Creates a bounded sibling read that is omitted only when the path does not exist.
    pub(crate) fn optional(path: PathBuf, maximum: u64, description: impl Into<String>) -> Self {
        Self {
            path,
            maximum,
            description: description.into(),
            optional: true,
            prefix: false,
        }
    }

    /// Reads at most `maximum` bytes and ignores a trailing suffix, matching fixed-size C reads.
    pub(crate) fn prefix(path: PathBuf, maximum: u64, description: impl Into<String>) -> Self {
        Self {
            path,
            maximum,
            description: description.into(),
            optional: false,
            prefix: true,
        }
    }

    /// Prefix-reads a sibling when present and omits only a missing path.
    pub(crate) fn optional_prefix(
        path: PathBuf,
        maximum: u64,
        description: impl Into<String>,
    ) -> Self {
        Self {
            path,
            maximum,
            description: description.into(),
            optional: true,
            prefix: true,
        }
    }
}

#[derive(Debug)]
pub(crate) struct LoadedDocument {
    pub(crate) files: Vec<(PathBuf, Vec<u8>)>,
}

impl LoadedDocument {
    /// Converts a worker result into the exact request-shaped file group expected by a workflow.
    pub(crate) fn into_exact<const N: usize>(
        self,
        description: &str,
    ) -> Result<[(PathBuf, Vec<u8>); N], String> {
        let actual = self.files.len();
        self.files.try_into().map_err(|_| {
            format!(
                "{description} loader returned an invalid file group: expected {N}, got {actual}"
            )
        })
    }
}

struct RunningLoad {
    descriptions: String,
    result: Receiver<Result<LoadedDocument, String>>,
}

#[derive(Default)]
pub(crate) struct DocumentLoader {
    running: Option<RunningLoad>,
}

impl DocumentLoader {
    pub(crate) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(crate) fn start(&mut self, requests: Vec<BoundedRead>) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a document load is already running".into());
        }
        if requests.is_empty() {
            return Err("a document load requires at least one file".into());
        }
        let descriptions = requests
            .iter()
            .map(|request| request.description.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-document-load".into())
            .spawn(move || {
                let loaded = requests
                    .into_iter()
                    .map(|request| {
                        let read = if request.prefix {
                            crate::dialogs::read_regular_prefix(
                                &request.path,
                                request.maximum,
                                &request.description,
                            )
                        } else {
                            crate::dialogs::read_regular_bounded(
                                &request.path,
                                request.maximum,
                                &request.description,
                            )
                        };
                        match read {
                            Ok(bytes) => Ok(Some((request.path, bytes))),
                            Err(error)
                                if request.optional
                                    && error.kind() == std::io::ErrorKind::NotFound =>
                            {
                                Ok(None)
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(|files| LoadedDocument {
                        files: files.into_iter().flatten().collect(),
                    });
                let _send_result = sender.send(loaded);
            })
            .map_err(|error| format!("could not create document-loader worker: {error}"))?;
        self.running = Some(RunningLoad {
            descriptions,
            result,
        });
        Ok(())
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
    ) -> Option<Result<LoadedDocument, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            egui::Window::new("Opening")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Reading {}", running.descriptions));
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<Result<LoadedDocument, String>> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(result) => {
                self.running = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                Some(Err(
                    "document-loader worker stopped without reporting a result".into(),
                ))
            }
        }
    }

    #[cfg(test)]
    fn wait_for_test(&mut self) -> Result<LoadedDocument, String> {
        let running = self.running.take().expect("document loader is running");
        running
            .result
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("document loader reports completion")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-document-loader-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn loads_multiple_bounded_files_in_request_order() {
        let first = path("first");
        let second = path("second");
        fs::write(&first, [0, 1]).unwrap();
        fs::write(&second, [2, 3, 4]).unwrap();
        let mut loader = DocumentLoader::default();
        loader
            .start(vec![
                BoundedRead::new(first.clone(), 2, "first fixture"),
                BoundedRead::new(second.clone(), 3, "second fixture"),
            ])
            .unwrap();
        let group = loader.wait_for_test().unwrap();
        assert_eq!(group.files[0], (first.clone(), vec![0, 1]));
        assert_eq!(group.files[1], (second.clone(), vec![2, 3, 4]));
        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
    }

    #[test]
    fn exact_groups_preserve_request_order_and_report_shape_mismatch() {
        let first = (PathBuf::from("first"), vec![1]);
        let second = (PathBuf::from("second"), vec![2]);
        assert_eq!(
            LoadedDocument {
                files: vec![first.clone(), second.clone()],
            }
            .into_exact::<2>("fixture")
            .unwrap(),
            [first, second]
        );
        assert_eq!(
            LoadedDocument { files: Vec::new() }
                .into_exact::<1>("fixture")
                .unwrap_err(),
            "fixture loader returned an invalid file group: expected 1, got 0"
        );
    }

    #[test]
    fn bound_failure_does_not_return_a_partial_group() {
        let first = path("bounded-first");
        let second = path("bounded-second");
        fs::write(&first, [1]).unwrap();
        fs::write(&second, [2, 3]).unwrap();
        let mut loader = DocumentLoader::default();
        loader
            .start(vec![
                BoundedRead::new(first.clone(), 1, "first fixture"),
                BoundedRead::new(second.clone(), 1, "second fixture"),
            ])
            .unwrap();
        assert!(loader.wait_for_test().is_err());
        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
    }

    #[test]
    fn optional_read_omits_only_a_missing_sibling() {
        let required = path("optional-required");
        let missing = path("optional-missing");
        fs::write(&required, [1, 2]).unwrap();
        let mut loader = DocumentLoader::default();
        loader
            .start(vec![
                BoundedRead::new(required.clone(), 2, "required fixture"),
                BoundedRead::optional(missing, 1, "optional fixture"),
            ])
            .unwrap();
        assert_eq!(
            loader.wait_for_test().unwrap().files,
            vec![(required.clone(), vec![1, 2])]
        );

        let invalid = path("optional-directory");
        fs::create_dir(&invalid).unwrap();
        loader
            .start(vec![
                BoundedRead::new(required.clone(), 2, "required fixture"),
                BoundedRead::optional(invalid.clone(), 1, "optional fixture"),
            ])
            .unwrap();
        assert!(loader.wait_for_test().is_err());
        fs::remove_dir(invalid).unwrap();
        fs::remove_file(required).unwrap();
    }

    #[test]
    fn prefix_reads_accept_short_and_trailing_files_and_optional_missing_siblings() {
        let short = path("prefix-short");
        let trailing = path("prefix-trailing");
        let missing = path("prefix-missing");
        fs::write(&short, [1, 2]).unwrap();
        fs::write(&trailing, [3, 4, 5, 6]).unwrap();
        let mut loader = DocumentLoader::default();
        loader
            .start(vec![
                BoundedRead::prefix(short.clone(), 3, "short prefix"),
                BoundedRead::prefix(trailing.clone(), 3, "trailing prefix"),
                BoundedRead::optional_prefix(missing, 3, "missing prefix"),
            ])
            .unwrap();
        assert_eq!(
            loader.wait_for_test().unwrap().files,
            vec![
                (short.clone(), vec![1, 2]),
                (trailing.clone(), vec![3, 4, 5])
            ]
        );
        fs::remove_file(short).unwrap();
        fs::remove_file(trailing).unwrap();
    }

    #[test]
    fn empty_and_overlapping_requests_are_rejected() {
        let mut loader = DocumentLoader::default();
        assert!(loader.start(Vec::new()).is_err());
        let path = path("overlap");
        fs::write(&path, [1]).unwrap();
        loader
            .start(vec![BoundedRead::new(path.clone(), 1, "fixture")])
            .unwrap();
        assert!(
            loader
                .start(vec![BoundedRead::new(path.clone(), 1, "fixture")])
                .is_err()
        );
        loader.wait_for_test().unwrap();
        fs::remove_file(path).unwrap();
    }
}
