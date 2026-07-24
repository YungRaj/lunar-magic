use lm_app::ToolInvocation;
use std::process::Command as ProcessCommand;

pub(crate) fn execute(invocation: &ToolInvocation) -> Result<(), String> {
    let mut command = ProcessCommand::new(&invocation.executable);
    command.args(&invocation.arguments);
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

fn exit_description(code: Option<i32>) -> String {
    code.map_or_else(|| "terminated by signal".into(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
