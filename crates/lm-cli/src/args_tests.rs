use super::*;
use lm_rom::Mapper;

#[path = "args_tests_portable.rs"]
mod portable;
#[path = "args_tests_rats.rs"]
mod rats;
#[path = "args_tests_transfers.rs"]
mod transfers;
#[path = "args_tests_utility.rs"]
mod utility;

#[test]
fn parses_complete_map16_native_commands() {
    let export = ["smw-map16-complete-export", "input.smc", "output.map16"].map(OsString::from);
    assert_eq!(
        parse_from(&export).unwrap(),
        Command::SmwMap16CompleteExport {
            rom: PathBuf::from("input.smc"),
            template: None,
            output: PathBuf::from("output.map16"),
        }
    );

    let template = [
        "smw-map16-complete-export-template",
        "input.smc",
        "template.map16",
        "output.map16",
    ]
    .map(OsString::from);
    assert_eq!(
        parse_from(&template).unwrap(),
        Command::SmwMap16CompleteExport {
            rom: PathBuf::from("input.smc"),
            template: Some(PathBuf::from("template.map16")),
            output: PathBuf::from("output.map16"),
        }
    );

    let import = [
        "smw-map16-complete-import",
        "input.smc",
        "input.map16",
        "output.smc",
    ]
    .map(OsString::from);
    assert_eq!(
        parse_from(&import).unwrap(),
        Command::SmwMap16CompleteImport {
            input_rom: PathBuf::from("input.smc"),
            map16: PathBuf::from("input.map16"),
            output_rom: PathBuf::from("output.smc"),
        }
    );
}
