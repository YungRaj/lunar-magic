use super::super::{ShellCommand, ShellCommandError, path_argument};

pub(in super::super) fn parse_ips_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    match command {
        "ips-apply" => Some(path_argument(argument, "ips-apply").map(ShellCommand::IpsApply)),
        "ips-create" => Some(path_argument(argument, "ips-create").map(ShellCommand::IpsCreate)),
        _ => None,
    }
}

pub(in super::super) fn parse_copier_header_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    match command {
        "copier-header-add" => {
            Some(path_argument(argument, "copier-header-add").map(ShellCommand::CopierHeaderAdd))
        }
        "copier-header-remove" => Some(
            path_argument(argument, "copier-header-remove").map(ShellCommand::CopierHeaderRemove),
        ),
        _ => None,
    }
}

pub(in super::super) fn parse_standalone_render_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    match command {
        "map16-render-file" => {
            Some(path_argument(argument, "map16-render-file").map(ShellCommand::RenderMap16))
        }
        "overworld-render-file" => Some(
            path_argument(argument, "overworld-render-file").map(ShellCommand::RenderOverworld),
        ),
        _ => None,
    }
}
