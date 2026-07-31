use lm_app::ToolInvocation;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

pub(crate) fn execute(invocation: &ToolInvocation) -> Result<(), String> {
    let launch = process_launch(invocation);
    let mut command = ProcessCommand::new(&launch.executable);
    command.args(&launch.arguments);
    if let Some(directory) = &invocation.working_directory {
        command.current_dir(directory);
    }
    let status = command.status().map_err(|error| {
        format!(
            "could not start external tool {:?}: {error}",
            invocation.tool_id
        )
    })?;
    if !status.success() {
        return Err(format!(
            "external tool {:?} exited unsuccessfully ({})",
            invocation.tool_id,
            exit_description(status.code())
        ));
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct ProcessLaunch {
    executable: PathBuf,
    arguments: Vec<OsString>,
}

fn process_launch(invocation: &ToolInvocation) -> ProcessLaunch {
    #[cfg(target_os = "macos")]
    if is_macos_application_bundle(&invocation.executable) {
        let mut arguments = vec![
            OsString::from("-W"),
            OsString::from("-n"),
            OsString::from("-a"),
            invocation.executable.as_os_str().to_owned(),
        ];
        if !invocation.arguments.is_empty() {
            arguments.push(OsString::from("--args"));
            arguments.extend(invocation.arguments.iter().map(OsString::from));
        }
        return ProcessLaunch {
            executable: PathBuf::from("/usr/bin/open"),
            arguments,
        };
    }
    ProcessLaunch {
        executable: invocation.executable.clone(),
        arguments: invocation.arguments.iter().map(OsString::from).collect(),
    }
}

#[cfg(target_os = "macos")]
fn is_macos_application_bundle(path: &std::path::Path) -> bool {
    path.is_dir()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

fn exit_description(code: Option<i32>) -> String {
    code.map_or_else(|| "terminated by signal".into(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::fs;

    fn process(executable: &str, arguments: &[&str]) -> ToolInvocation {
        ToolInvocation {
            tool_id: "test-process".into(),
            executable: executable.into(),
            arguments: arguments.iter().map(|value| (*value).into()).collect(),
            working_directory: None,
        }
    }

    #[test]
    fn exit_descriptions_distinguish_codes_and_signals() {
        assert_eq!(exit_description(Some(7)), "7");
        assert_eq!(exit_description(None), "terminated by signal");
    }

    #[test]
    fn ordinary_executables_preserve_every_argument_boundary() {
        let invocation = process("/tmp/Editor With Spaces", &["", "two words", "{literal}"]);
        assert_eq!(
            process_launch(&invocation),
            ProcessLaunch {
                executable: PathBuf::from("/tmp/Editor With Spaces"),
                arguments: vec!["".into(), "two words".into(), "{literal}".into()],
            }
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn application_bundles_use_waiting_new_instance_open_without_a_shell() {
        let root = std::env::temp_dir().join(format!(
            "lm-external-tool-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("bundle")
        ));
        let bundle = root.join("Editor With Spaces.APP");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&bundle).unwrap();
        let invocation = ToolInvocation {
            tool_id: "bundle".into(),
            executable: bundle.clone(),
            arguments: vec![
                "".into(),
                "file with spaces.bin".into(),
                "--flag=value".into(),
            ],
            working_directory: Some(root.clone()),
        };
        assert_eq!(
            process_launch(&invocation),
            ProcessLaunch {
                executable: PathBuf::from("/usr/bin/open"),
                arguments: vec![
                    "-W".into(),
                    "-n".into(),
                    "-a".into(),
                    bundle.as_os_str().to_owned(),
                    "--args".into(),
                    "".into(),
                    "file with spaces.bin".into(),
                    "--flag=value".into(),
                ],
            }
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn direct_process_execution_reports_success_and_nonzero_exit() {
        #[cfg(unix)]
        let (success, success_arguments, failure, failure_arguments) =
            ("/usr/bin/true", &[][..], "/usr/bin/false", &[][..]);
        #[cfg(windows)]
        let (success, success_arguments, failure, failure_arguments) = (
            "cmd.exe",
            &["/C", "exit", "0"][..],
            "cmd.exe",
            &["/C", "exit", "7"][..],
        );

        execute(&process(success, success_arguments)).unwrap();
        let error = execute(&process(failure, failure_arguments)).unwrap_err();
        assert!(error.contains("test-process"));
        assert!(error.contains("exited unsuccessfully"));
    }
}
