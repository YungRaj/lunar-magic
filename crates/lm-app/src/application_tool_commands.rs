use crate::{read_bounded_bytes, shell_command, tool_shell};
use lm_app::{AppState, Command, ToolConfig};

pub(crate) fn execute_tool_command(
    app: &mut AppState,
    command: shell_command::ToolCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        shell_command::ToolCommand::Install(path) => {
            let config = ToolConfig::decode(&read_bounded_bytes(
                &path,
                ToolConfig::MAX_ENCODED_LEN,
                "external-tool configuration",
            )?)?;
            app.set_external_tools(config.tools)?;
            show_tool_status(app);
        }
        shell_command::ToolCommand::Status => show_tool_status(app),
        shell_command::ToolCommand::Run(id) => {
            let effects = app.dispatch(Command::RunExternalTool(id))?;
            tool_shell::print_invocations(&effects);
        }
        shell_command::ToolCommand::Event(event) => {
            let effects = app.external_tool_event(tool_shell::parse_event(&event)?)?;
            tool_shell::print_invocations(&effects);
        }
        shell_command::ToolCommand::Execute(id) => {
            let effects = app.dispatch(Command::RunExternalTool(id))?;
            tool_shell::execute_invocations(&effects)?;
        }
        shell_command::ToolCommand::ExecuteEvent(event) => {
            let effects = app.external_tool_event(tool_shell::parse_event(&event)?)?;
            tool_shell::execute_invocations(&effects)?;
        }
    }
    Ok(())
}

pub(crate) fn show_tool_status(app: &AppState) {
    if app.external_tools().is_empty() {
        println!("no external tools configured");
    } else {
        for tool in app.external_tools() {
            println!(
                "tool {:?}: {} ({})",
                tool.id,
                tool.name,
                tool.executable.display()
            );
        }
    }
}
