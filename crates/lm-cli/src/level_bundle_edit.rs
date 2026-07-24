use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_level::{CompleteLevelFile, MAX_AUXILIARY_EDIT_SCRIPT_BYTES, parse_auxiliary_edit_script};
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    script: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == output || script == output {
        return Err("level auxiliary edit output must differ from both inputs".into());
    }
    let mut file =
        CompleteLevelFile::decode(&read_bounded(input, CompleteLevelFile::MAX_FILE_LEN)?)?;
    let script = String::from_utf8(read_bounded(script, MAX_AUXILIARY_EDIT_SCRIPT_BYTES)?)?;
    let edits = parse_auxiliary_edit_script(&script)?;
    file.0.apply_auxiliary_edits(&edits)?;
    let encoded = file.encode()?;
    let reopened = CompleteLevelFile::decode(&encoded)?;
    if reopened != file {
        return Err("edited level bundle failed semantic reopen verification".into());
    }
    write_new_batch(&[(output, encoded.as_slice())])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{EntranceKind, Level, ScreenExit, Subtile};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn parses_every_auxiliary_domain_and_rejects_late_bad_commands() {
        let edits = parse_auxiliary_edit_script("LMAUXED1\nentrance-insert 0 secondary 1 2 3 4 0x500\nscreen-exit-insert 0 0x1234\nsecondary-exit-insert 0 0x105 2 3 4 5 0x20 0x80 7\nmap16-upsert 0x20 1 2 3 4 5\n").unwrap();
        assert_eq!(edits.len(), 4);
        assert!(parse_auxiliary_edit_script("LMAUXED1\nentrance-remove 0\nbad\n").is_err());
        assert!(parse_auxiliary_edit_script("wrong\n").is_err());
    }

    #[test]
    fn edits_all_domains_and_publishes_a_semantically_reopenable_bundle() {
        let directory = std::env::temp_dir().join(format!(
            "lm-level-aux-edit-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let input = directory.join("input.lmlevel");
        let script = directory.join("edits.txt");
        let output = directory.join("output.lmlevel");
        let level = Level {
            unknown_extensions: vec![vec![0, 0xff, 7]],
            ..Level::default()
        };
        fs::write(&input, CompleteLevelFile(level).encode().unwrap()).unwrap();
        fs::write(&script, "LMAUXED1\nentrance-insert 0 main 1 2 3 4 5\nentrance-insert 1 midway 6 7 8 9 10\nentrance-move 1 0\nentrance-replace 1 secondary 11 12 13 14 15\nentrance-remove 0\nscreen-exit-insert 0 0x1111\nscreen-exit-insert 1 0x2222\nscreen-exit-move 1 0\nscreen-exit-replace 1 0x3333\nscreen-exit-remove 0\nsecondary-exit-insert 0 0x105 2 3 4 5 6 0x80 7\nsecondary-exit-insert 1 0x106 8 9 10 11 12 0x90 13\nsecondary-exit-move 1 0\nsecondary-exit-replace 1 0x107 14 15 1 2 3 0xa0 4\nsecondary-exit-remove 0\nmap16-upsert 0x20 1 2 3 4 5\nmap16-upsert 0x20 6 7 8 9 10\nmap16-upsert 0x21 11 12 13 14 15\nmap16-remove 0x21\n").unwrap();
        execute(&input, &script, &output).unwrap();
        let edited = CompleteLevelFile::decode(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(edited.0.entrances.len(), 1);
        assert_eq!(edited.0.entrances[0].kind, EntranceKind::Secondary);
        assert_eq!(edited.0.entrances[0].x, 11);
        assert_eq!(edited.0.screen_exits, [ScreenExit { encoded: 0x3333 }]);
        assert_eq!(edited.0.secondary_exits[0].destination_level, 0x107);
        assert_eq!(edited.0.map16_overrides[0].0, 0x20);
        assert_eq!(edited.0.map16_overrides[0].1.bottom_right, Subtile(9));
        assert_eq!(edited.0.unknown_extensions, [vec![0, 0xff, 7]]);

        let invalid_script = directory.join("invalid.txt");
        let invalid_output = directory.join("invalid.lmlevel");
        fs::write(
            &invalid_script,
            "LMAUXED1\nentrance-insert 0 main 1 2 3 4 5\nentrance-remove 99\n",
        )
        .unwrap();
        assert!(execute(&input, &invalid_script, &invalid_output).is_err());
        assert!(!invalid_output.exists());
        for path in [input, script, output] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_file(invalid_script).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
