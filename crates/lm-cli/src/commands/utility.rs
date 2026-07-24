use crate::args::{Command, Direction};
use crate::oracle_input::{MAX_ROM_BYTES, read_bounded};
use lm_oracle::compare_bytes;
use lm_rom::{pc_to_snes, snes_to_pc};
use std::io::{self, Write};

pub(super) fn execute(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::Address {
            mapper,
            direction,
            value,
        } => match direction {
            Direction::SnesToPc => println!("{:#x}", snes_to_pc(*mapper, *value)?),
            Direction::PcToSnes => {
                println!("{:#08x}", pc_to_snes(*mapper, usize::try_from(*value)?)?);
            }
        },
        Command::Diff { left, right } => {
            let differences = compare_bytes(
                &read_bounded(left, MAX_ROM_BYTES)?,
                &read_bounded(right, MAX_ROM_BYTES)?,
            );
            let stdout = io::stdout();
            if let Err(error) = write_diff_report(stdout.lock(), &differences) {
                if error.kind() != io::ErrorKind::BrokenPipe {
                    return Err(error.into());
                }
            }
        }
        Command::OracleVerify {
            manifest,
            before,
            after,
            observations,
        } => crate::oracle::verify(manifest, before, after, observations.as_ref())?,
        Command::OracleCapture(command) => crate::oracle_capture::capture(command)?,
        Command::OracleVerifySuite { root } => crate::oracle_suite::verify(root)?,
        Command::OracleCoverage { root, requirements } => {
            crate::oracle_suite::audit_coverage(root, requirements)?;
        }
        Command::OracleReleaseGate { root, requirements } => {
            crate::oracle_suite::release_gate(root, requirements)?;
        }
        Command::Checksum {
            input,
            output,
            field_offset,
        } => crate::rom_commands::checksum(input, output, *field_offset)?,
        Command::ChecksumAuto { input, output } => {
            crate::rom_commands::checksum_auto(input, output)?;
        }
        Command::RomExpand {
            input,
            output,
            mapper,
            target_logical_len,
            fill,
        } => crate::rom_commands::expand(input, output, *mapper, *target_logical_len, *fill)?,
        Command::CopierHeaderAdd {
            input,
            output,
            fill,
        } => crate::rom_commands::convert_copier_header(
            input,
            output,
            lm_rom::CopierHeader::Present,
            *fill,
        )?,
        Command::CopierHeaderRemove { input, output } => {
            crate::rom_commands::convert_copier_header(
                input,
                output,
                lm_rom::CopierHeader::Absent,
                0,
            )?;
        }
        Command::Patch {
            input,
            output,
            offset,
            bytes,
        } => crate::rom_commands::patch(input, output, *offset, bytes)?,
        Command::IpsApply {
            source,
            patch,
            output,
        } => crate::rom_commands::ips_apply(source, patch, output)?,
        Command::IpsCreate {
            before,
            after,
            output,
        } => crate::rom_commands::ips_create(before, after, output)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn write_diff_report(
    mut output: impl Write,
    differences: &[lm_oracle::ByteDifference],
) -> io::Result<()> {
    for difference in differences {
        writeln!(
            output,
            "{:#x}..{:#x}",
            difference.range.start, difference.range.end
        )?;
    }
    writeln!(output, "changed-ranges: {}", differences.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn diff_report_is_stable_and_exposes_broken_pipes_as_io() {
        let differences = compare_bytes(&[0, 1, 2, 3], &[0, 4, 5, 3]);
        let mut output = Vec::new();
        write_diff_report(&mut output, &differences).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "0x1..0x3\nchanged-ranges: 1\n"
        );
        assert_eq!(
            write_diff_report(BrokenWriter, &differences)
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
