//! Non-blocking platform persistence for immutable application save snapshots.

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PersistenceTarget {
    Replace(PathBuf),
    Create(PathBuf),
    CreateRemoving { create: PathBuf, remove: PathBuf },
    ReplacePair { first: PathBuf, second: PathBuf },
    CreatePair { first: PathBuf, second: PathBuf },
}

impl PersistenceTarget {
    fn description(&self) -> String {
        match self {
            Self::Replace(path) | Self::Create(path) => path.display().to_string(),
            Self::CreateRemoving { create, remove } => {
                format!("{} and removal of {}", create.display(), remove.display())
            }
            Self::ReplacePair { first, second } | Self::CreatePair { first, second } => {
                format!("{} and {}", first.display(), second.display())
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct PersistenceCompletion {
    pub(crate) request_id: u64,
    pub(crate) target: PersistenceTarget,
    pub(crate) result: Result<(), String>,
}

struct RunningPersistence {
    request_id: u64,
    target: PersistenceTarget,
    result: Receiver<PersistenceCompletion>,
}

enum PersistencePayload {
    Single(Vec<u8>),
    Pair(Vec<u8>, Vec<u8>),
}

#[derive(Default)]
pub(crate) struct PersistenceWorker {
    running: Option<RunningPersistence>,
}

impl PersistenceWorker {
    pub(crate) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(crate) fn start(
        &mut self,
        request_id: u64,
        target: PersistenceTarget,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        if matches!(
            &target,
            PersistenceTarget::ReplacePair { .. } | PersistenceTarget::CreatePair { .. }
        ) {
            return Err("paired persistence requires two explicit payloads".into());
        }
        self.start_payload(request_id, target, PersistencePayload::Single(bytes))
    }

    pub(crate) fn start_pair(
        &mut self,
        request_id: u64,
        first: PathBuf,
        first_bytes: Vec<u8>,
        second: PathBuf,
        second_bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.start_payload(
            request_id,
            PersistenceTarget::ReplacePair { first, second },
            PersistencePayload::Pair(first_bytes, second_bytes),
        )
    }

    pub(crate) fn start_create_removing(
        &mut self,
        request_id: u64,
        create: PathBuf,
        bytes: Vec<u8>,
        remove: PathBuf,
    ) -> Result<(), String> {
        self.start(
            request_id,
            PersistenceTarget::CreateRemoving { create, remove },
            bytes,
        )
    }

    pub(crate) fn start_create_pair(
        &mut self,
        request_id: u64,
        first: PathBuf,
        first_bytes: Vec<u8>,
        second: PathBuf,
        second_bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.start_payload(
            request_id,
            PersistenceTarget::CreatePair { first, second },
            PersistencePayload::Pair(first_bytes, second_bytes),
        )
    }

    fn start_payload(
        &mut self,
        request_id: u64,
        target: PersistenceTarget,
        payload: PersistencePayload,
    ) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a native persistence worker is already running".into());
        }
        let running_target = target.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("lm-save-{request_id}"))
            .spawn(move || {
                let write_result = match (&target, payload) {
                    (PersistenceTarget::Replace(path), PersistencePayload::Single(bytes)) => {
                        lm_app::file_persistence::replace_existing(path, &bytes)
                    }
                    (PersistenceTarget::Create(path), PersistencePayload::Single(bytes)) => {
                        lm_app::file_persistence::write_new(path, &bytes)
                    }
                    (
                        PersistenceTarget::CreateRemoving { create, remove },
                        PersistencePayload::Single(bytes),
                    ) => lm_app::file_persistence::write_new_removing_existing(
                        create, remove, &bytes,
                    ),
                    (
                        PersistenceTarget::ReplacePair { first, second },
                        PersistencePayload::Pair(first_bytes, second_bytes),
                    ) => lm_app::file_persistence::replace_existing_pair(
                        (first, &first_bytes),
                        (second, &second_bytes),
                    ),
                    (
                        PersistenceTarget::CreatePair { first, second },
                        PersistencePayload::Pair(first_bytes, second_bytes),
                    ) => lm_app::file_persistence::write_new_group(&[
                        (first.as_path(), first_bytes.as_slice()),
                        (second.as_path(), second_bytes.as_slice()),
                    ]),
                    _ => unreachable!("persistence target and payload are constructed together"),
                }
                .map_err(|error| error.to_string());
                let _send_result = sender.send(PersistenceCompletion {
                    request_id,
                    target,
                    result: write_result,
                });
            })
            .map_err(|error| format!("could not create ROM-persistence worker: {error}"))?;
        self.running = Some(RunningPersistence {
            request_id,
            target: running_target,
            result,
        });
        Ok(())
    }

    pub(crate) fn show(&mut self, context: &egui::Context) -> Option<PersistenceCompletion> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            egui::Window::new("Saving")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Writing {}", running.target.description()));
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<PersistenceCompletion> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(completion) => {
                self.running = None;
                Some(completion)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                let request_id = running.request_id;
                let target = running.target.clone();
                self.running = None;
                Some(PersistenceCompletion {
                    request_id,
                    target,
                    result: Err("ROM-persistence worker stopped without reporting a result".into()),
                })
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn wait_for_test(&mut self) -> PersistenceCompletion {
        let running = self.running.take().expect("persistence worker is running");
        running
            .result
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("persistence worker reports completion")
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
            "lm-native-worker-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn wait(worker: &mut PersistenceWorker) -> PersistenceCompletion {
        worker.wait_for_test()
    }

    #[test]
    fn create_new_worker_writes_exact_snapshot_and_rejects_collision() {
        let path = path("create");
        let mut worker = PersistenceWorker::default();
        worker
            .start(7, PersistenceTarget::Create(path.clone()), vec![1, 2, 3])
            .unwrap();
        let completion = wait(&mut worker);
        assert_eq!(completion.request_id, 7);
        completion.result.unwrap();
        assert_eq!(fs::read(&path).unwrap(), [1, 2, 3]);

        worker
            .start(8, PersistenceTarget::Create(path.clone()), vec![9])
            .unwrap();
        assert!(wait(&mut worker).result.is_err());
        assert_eq!(fs::read(&path).unwrap(), [1, 2, 3]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn replacement_worker_atomically_publishes_exact_snapshot() {
        let path = path("replace");
        fs::write(&path, [4, 5]).unwrap();
        let mut worker = PersistenceWorker::default();
        worker
            .start(9, PersistenceTarget::Replace(path.clone()), vec![6, 7, 8])
            .unwrap();
        wait(&mut worker).result.unwrap();
        assert_eq!(fs::read(&path).unwrap(), [6, 7, 8]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn paired_worker_preserves_arbitrary_binary_payloads() {
        let first = path("pair-first");
        let second = path("pair-second");
        fs::write(&first, [1]).unwrap();
        fs::write(&second, [2]).unwrap();
        let mut worker = PersistenceWorker::default();
        worker
            .start_pair(
                10,
                first.clone(),
                vec![0, 3, 0],
                second.clone(),
                vec![4, 0, 5],
            )
            .unwrap();
        wait(&mut worker).result.unwrap();
        assert_eq!(fs::read(&first).unwrap(), [0, 3, 0]);
        assert_eq!(fs::read(&second).unwrap(), [4, 0, 5]);
        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
    }

    #[test]
    fn create_pair_is_all_or_nothing_and_never_replaces() {
        let first = path("create-pair-first");
        let second = path("create-pair-second");
        let mut worker = PersistenceWorker::default();
        worker
            .start_create_pair(11, first.clone(), vec![1, 2], second.clone(), vec![3, 4])
            .unwrap();
        wait(&mut worker).result.unwrap();
        assert_eq!(fs::read(&first).unwrap(), [1, 2]);
        assert_eq!(fs::read(&second).unwrap(), [3, 4]);

        fs::remove_file(&first).unwrap();
        worker
            .start_create_pair(12, first.clone(), vec![5], second.clone(), vec![6])
            .unwrap();
        assert!(wait(&mut worker).result.is_err());
        assert!(!first.exists());
        assert_eq!(fs::read(&second).unwrap(), [3, 4]);
        fs::remove_file(second).unwrap();
    }

    #[test]
    fn create_removing_deletes_only_a_stale_regular_sibling() {
        let created = path("create-removing-new");
        let obsolete = path("create-removing-obsolete");
        fs::write(&obsolete, [9, 8]).unwrap();
        let mut worker = PersistenceWorker::default();
        worker
            .start_create_removing(13, created.clone(), vec![1, 2, 3], obsolete.clone())
            .unwrap();
        wait(&mut worker).result.unwrap();
        assert_eq!(fs::read(&created).unwrap(), [1, 2, 3]);
        assert!(!obsolete.exists());

        fs::write(&obsolete, [7, 6]).unwrap();
        worker
            .start_create_removing(14, created.clone(), vec![4], obsolete.clone())
            .unwrap();
        assert!(wait(&mut worker).result.is_err());
        assert_eq!(fs::read(&created).unwrap(), [1, 2, 3]);
        assert_eq!(fs::read(&obsolete).unwrap(), [7, 6]);
        fs::remove_file(created).unwrap();
        fs::remove_file(obsolete).unwrap();
    }

    #[test]
    fn overlapping_worker_is_rejected_without_replacing_running_request() {
        let (sender, receiver) = mpsc::channel();
        let mut worker = PersistenceWorker {
            running: Some(RunningPersistence {
                request_id: 10,
                target: PersistenceTarget::Create(PathBuf::from("first.smc")),
                result: receiver,
            }),
        };
        assert!(
            worker
                .start(
                    11,
                    PersistenceTarget::Create(PathBuf::from("second.smc")),
                    Vec::new(),
                )
                .is_err()
        );
        assert_eq!(worker.running.as_ref().unwrap().request_id, 10);
        drop(sender);
    }

    #[test]
    fn single_payload_api_rejects_a_paired_target_before_spawning() {
        let mut worker = PersistenceWorker::default();
        assert!(
            worker
                .start(
                    12,
                    PersistenceTarget::ReplacePair {
                        first: PathBuf::from("first.mw0"),
                        second: PathBuf::from("second.mw0t"),
                    },
                    vec![1, 2],
                )
                .is_err()
        );
        assert!(!worker.is_running());
        assert!(
            worker
                .start(
                    13,
                    PersistenceTarget::CreatePair {
                        first: PathBuf::from("first.tpl"),
                        second: PathBuf::from("first.palmask"),
                    },
                    vec![1, 2],
                )
                .is_err()
        );
    }
}
