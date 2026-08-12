#![forbid(unsafe_code)]

use std::{
    env,
    path::Path,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("Lunar Magic Rust launcher failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, String> {
    let launcher = env::current_exe().map_err(|error| error.to_string())?;
    let root = launcher
        .parent()
        .ok_or("launcher has no install directory")?;
    launch_from(root, env::args_os().skip(1))
}

fn launch_from(
    root: &Path,
    arguments: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<u8, String> {
    let executable = lm_update::resolve_current(root).map_err(|error| error.to_string())?;
    let status = Command::new(executable)
        .args(arguments)
        .status()
        .map_err(|error| error.to_string())?;
    let code = status
        .code()
        .ok_or("selected application terminated without an exit code")?;
    u8::try_from(code)
        .map_err(|_| format!("selected application returned unsupported exit code {code}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    #[cfg(unix)]
    fn version(root: &Path, name: &str, marker: &str) -> std::path::PathBuf {
        let directory = root.join(name);
        fs::create_dir(&directory).unwrap();
        let executable = directory.join("lm-native");
        fs::write(
            &executable,
            format!("#!/bin/sh\nprintf '%s' \"$1\" > \"$2\"\nexit {marker}\n"),
        )
        .unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        directory
    }

    #[test]
    #[cfg(unix)]
    fn process_switch_rollback_and_arguments_are_exact() {
        let root = tempfile::tempdir().unwrap();
        let first = version(root.path(), "version-1", "3");
        let second = version(root.path(), "version-2", "7");
        let output = root.path().join("argument.txt");
        lm_update::activate_version(root.path(), &first).unwrap();
        assert_eq!(
            launch_from(
                root.path(),
                ["first value".into(), output.clone().into_os_string()]
            )
            .unwrap(),
            3
        );
        assert_eq!(fs::read_to_string(&output).unwrap(), "first value");
        lm_update::activate_version(root.path(), &second).unwrap();
        assert_eq!(
            launch_from(
                root.path(),
                ["second value".into(), output.clone().into_os_string()]
            )
            .unwrap(),
            7
        );
        lm_update::rollback_current(root.path()).unwrap();
        assert_eq!(
            launch_from(
                root.path(),
                ["rolled back".into(), output.clone().into_os_string()]
            )
            .unwrap(),
            3
        );
        assert_eq!(fs::read_to_string(output).unwrap(), "rolled back");
    }

    #[test]
    #[cfg(unix)]
    fn process_launch_rejects_tampered_selected_executable() {
        let root = tempfile::tempdir().unwrap();
        let selected = version(root.path(), "version-1", "0");
        lm_update::activate_version(root.path(), &selected).unwrap();
        fs::write(selected.join("lm-native"), b"tampered").unwrap();
        assert!(launch_from(root.path(), std::iter::empty()).is_err());
    }
}
