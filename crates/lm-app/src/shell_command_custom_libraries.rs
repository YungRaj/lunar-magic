use super::{ShellCommand, ShellCommandError, no_argument, path_argument};

pub(super) fn parse_custom_library_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let result = match command {
        "custom-open" => path_argument(argument, "custom-open").map(ShellCommand::CustomObjectOpen),
        "custom-edit" => path_argument(argument, "custom-edit").map(ShellCommand::CustomObjectEdit),
        "custom-undo" => no_argument(argument, "custom-undo", ShellCommand::CustomObjectUndo),
        "custom-redo" => no_argument(argument, "custom-redo", ShellCommand::CustomObjectRedo),
        "custom-status" => no_argument(argument, "custom-status", ShellCommand::CustomObjectStatus),
        "custom-save" => no_argument(argument, "custom-save", ShellCommand::CustomObjectSave),
        "custom-close" => no_argument(argument, "custom-close", ShellCommand::CustomObjectClose),
        "custom-discard" => no_argument(
            argument,
            "custom-discard",
            ShellCommand::CustomObjectDiscard,
        ),
        "custom-sprite-open" => {
            path_argument(argument, "custom-sprite-open").map(ShellCommand::CustomSpriteOpen)
        }
        "custom-sprite-edit" => {
            path_argument(argument, "custom-sprite-edit").map(ShellCommand::CustomSpriteEdit)
        }
        "custom-sprite-undo" => no_argument(
            argument,
            "custom-sprite-undo",
            ShellCommand::CustomSpriteUndo,
        ),
        "custom-sprite-redo" => no_argument(
            argument,
            "custom-sprite-redo",
            ShellCommand::CustomSpriteRedo,
        ),
        "custom-sprite-status" => no_argument(
            argument,
            "custom-sprite-status",
            ShellCommand::CustomSpriteStatus,
        ),
        "custom-sprite-save" => no_argument(
            argument,
            "custom-sprite-save",
            ShellCommand::CustomSpriteSave,
        ),
        "custom-sprite-close" => no_argument(
            argument,
            "custom-sprite-close",
            ShellCommand::CustomSpriteClose,
        ),
        "custom-sprite-discard" => no_argument(
            argument,
            "custom-sprite-discard",
            ShellCommand::CustomSpriteDiscard,
        ),
        "dsc-open" => path_argument(argument, "dsc-open").map(ShellCommand::DscSidecarOpen),
        "dsc-replace" => {
            path_argument(argument, "dsc-replace").map(ShellCommand::DscSidecarReplace)
        }
        "dsc-undo" => no_argument(argument, "dsc-undo", ShellCommand::DscSidecarUndo),
        "dsc-redo" => no_argument(argument, "dsc-redo", ShellCommand::DscSidecarRedo),
        "dsc-status" => no_argument(argument, "dsc-status", ShellCommand::DscSidecarStatus),
        "dsc-save" => no_argument(argument, "dsc-save", ShellCommand::DscSidecarSave),
        "dsc-close" => no_argument(argument, "dsc-close", ShellCommand::DscSidecarClose),
        "dsc-discard" => no_argument(argument, "dsc-discard", ShellCommand::DscSidecarDiscard),
        "native-sidecar-open" => {
            path_argument(argument, "native-sidecar-open").map(ShellCommand::NativeMap16SidecarOpen)
        }
        "native-sidecar-edit" => {
            path_argument(argument, "native-sidecar-edit").map(ShellCommand::NativeMap16SidecarEdit)
        }
        "native-sidecar-undo" => no_argument(
            argument,
            "native-sidecar-undo",
            ShellCommand::NativeMap16SidecarUndo,
        ),
        "native-sidecar-redo" => no_argument(
            argument,
            "native-sidecar-redo",
            ShellCommand::NativeMap16SidecarRedo,
        ),
        "native-sidecar-status" => no_argument(
            argument,
            "native-sidecar-status",
            ShellCommand::NativeMap16SidecarStatus,
        ),
        "native-sidecar-save" => no_argument(
            argument,
            "native-sidecar-save",
            ShellCommand::NativeMap16SidecarSave,
        ),
        "native-sidecar-close" => no_argument(
            argument,
            "native-sidecar-close",
            ShellCommand::NativeMap16SidecarClose,
        ),
        "native-sidecar-discard" => no_argument(
            argument,
            "native-sidecar-discard",
            ShellCommand::NativeMap16SidecarDiscard,
        ),
        _ => return None,
    };
    Some(result)
}
