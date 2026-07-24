use super::{SecondaryExit, SecondaryExitEncodingError};

pub(super) fn validate_secondary_exit(
    exit: &SecondaryExit,
    entry: usize,
) -> Result<(), SecondaryExitEncodingError> {
    if exit.destination_level > 0x01ff {
        return Err(SecondaryExitEncodingError::DestinationLevelOutOfRange {
            entry,
            value: exit.destination_level,
        });
    }
    for (value, error) in [
        (
            exit.screen,
            SecondaryExitEncodingError::ScreenOutOfRange {
                entry,
                value: exit.screen,
            },
        ),
        (
            exit.x,
            SecondaryExitEncodingError::XOutOfRange {
                entry,
                value: exit.x,
            },
        ),
        (
            exit.y,
            SecondaryExitEncodingError::YOutOfRange {
                entry,
                value: exit.y,
            },
        ),
    ] {
        if value > 0x0f {
            return Err(error);
        }
    }
    if exit.destination_flags & 8 != 0 {
        return Err(SecondaryExitEncodingError::DestinationFlagsUseLevelBit {
            entry,
            value: exit.destination_flags,
        });
    }
    if exit.x_and_overworld_flags & 0x0f != 0 {
        return Err(SecondaryExitEncodingError::XFlagsUsePositionBits {
            entry,
            value: exit.x_and_overworld_flags,
        });
    }
    Ok(())
}
