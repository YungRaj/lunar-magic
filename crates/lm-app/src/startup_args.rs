use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StartupOptions {
    pub rom: Option<PathBuf>,
    pub ui_config: Option<PathBuf>,
    pub tools_config: Option<PathBuf>,
    pub recent_state: Option<PathBuf>,
    pub revision_profile: Option<PathBuf>,
    pub command_script: Option<PathBuf>,
    pub level: Option<u16>,
    pub allow_in_place_rom_write: bool,
    pub help: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartupArgumentError {
    MissingValue(&'static str),
    Duplicate(&'static str),
    UnknownOption(OsString),
    InvalidLevel(OsString),
    UnexpectedPositional(PathBuf),
}

impl fmt::Display for StartupArgumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid startup arguments: {self:?}")
    }
}

impl std::error::Error for StartupArgumentError {}

impl StartupOptions {
    /// Parses the shared OS-string-safe startup grammar used by application frontends.
    ///
    /// # Errors
    ///
    /// Returns [`StartupArgumentError`] for missing or duplicate values, unknown options, or more
    /// than one positional ROM path.
    pub fn parse(
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Result<Self, StartupArgumentError> {
        let mut options = Self::default();
        let mut arguments = arguments.into_iter();
        let mut positional_only = false;
        while let Some(argument) = arguments.next() {
            if !positional_only && argument == "--" {
                positional_only = true;
                continue;
            }
            if !positional_only && (argument == "--help" || argument == "-h") {
                options.help = true;
                continue;
            }
            if !positional_only && argument == "--allow-in-place-rom-write" {
                if options.allow_in_place_rom_write {
                    return Err(StartupArgumentError::Duplicate(
                        "--allow-in-place-rom-write",
                    ));
                }
                options.allow_in_place_rom_write = true;
                continue;
            }
            if !positional_only && argument == "--level" {
                if options.level.is_some() {
                    return Err(StartupArgumentError::Duplicate("--level"));
                }
                let value = arguments
                    .next()
                    .ok_or(StartupArgumentError::MissingValue("--level"))?;
                let text = value
                    .to_str()
                    .ok_or_else(|| StartupArgumentError::InvalidLevel(value.clone()))?;
                let digits = text
                    .strip_prefix("0x")
                    .or_else(|| text.strip_prefix("0X"))
                    .or_else(|| text.strip_prefix('$'))
                    .unwrap_or(text);
                options.level = Some(
                    u16::from_str_radix(digits, 16)
                        .map_err(|_| StartupArgumentError::InvalidLevel(value))?,
                );
                continue;
            }
            let destination = if !positional_only && argument == "--rom" {
                (&mut options.rom, "--rom")
            } else if !positional_only && argument == "--ui-config" {
                (&mut options.ui_config, "--ui-config")
            } else if !positional_only && argument == "--tools-config" {
                (&mut options.tools_config, "--tools-config")
            } else if !positional_only && argument == "--recent-state" {
                (&mut options.recent_state, "--recent-state")
            } else if !positional_only && argument == "--profile" {
                (&mut options.revision_profile, "--profile")
            } else if !positional_only && argument == "--script" {
                (&mut options.command_script, "--script")
            } else {
                if !positional_only && argument.to_string_lossy().starts_with('-') {
                    return Err(StartupArgumentError::UnknownOption(argument));
                }
                if options.rom.is_some() {
                    return Err(StartupArgumentError::UnexpectedPositional(argument.into()));
                }
                options.rom = Some(argument.into());
                continue;
            };
            if destination.0.is_some() {
                return Err(StartupArgumentError::Duplicate(destination.1));
            }
            let value = arguments
                .next()
                .ok_or(StartupArgumentError::MissingValue(destination.1))?;
            *destination.0 = Some(value.into());
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<StartupOptions, StartupArgumentError> {
        StartupOptions::parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn accepts_legacy_and_explicit_rom_paths() {
        assert_eq!(
            parse(&["My Hack 日本語.smc"]).unwrap().rom,
            Some("My Hack 日本語.smc".into())
        );
        assert_eq!(
            parse(&["--rom", "My Hack.smc"]).unwrap().rom,
            Some("My Hack.smc".into())
        );
        assert_eq!(
            parse(&["--", "-special.smc"]).unwrap().rom,
            Some("-special.smc".into())
        );
    }

    #[test]
    fn parses_independent_frontend_configurations_in_any_order() {
        let options = parse(&[
            "--tools-config",
            "My Tools.lmtools",
            "--ui-config",
            "日本語 UI.lmuicfg",
            "--recent-state",
            "Recent Files.lmrecent",
            "--profile",
            "SMW US.lmrev",
            "--script",
            "Build Hack.lmscript",
            "--allow-in-place-rom-write",
            "--level",
            "102",
            "game.smc",
        ])
        .unwrap();
        assert_eq!(options.rom, Some("game.smc".into()));
        assert_eq!(options.ui_config, Some("日本語 UI.lmuicfg".into()));
        assert_eq!(options.tools_config, Some("My Tools.lmtools".into()));
        assert_eq!(options.recent_state, Some("Recent Files.lmrecent".into()));
        assert_eq!(options.revision_profile, Some("SMW US.lmrev".into()));
        assert_eq!(options.command_script, Some("Build Hack.lmscript".into()));
        assert_eq!(options.level, Some(0x102));
        assert!(options.allow_in_place_rom_write);
    }

    #[test]
    fn rejects_missing_duplicate_unknown_and_extra_arguments() {
        assert_eq!(
            parse(&["--rom"]),
            Err(StartupArgumentError::MissingValue("--rom"))
        );
        assert_eq!(
            parse(&["--allow-in-place-rom-write", "--allow-in-place-rom-write"]),
            Err(StartupArgumentError::Duplicate(
                "--allow-in-place-rom-write"
            ))
        );
        assert_eq!(
            parse(&["--rom", "one.smc", "--rom", "two.smc"]),
            Err(StartupArgumentError::Duplicate("--rom"))
        );
        assert!(matches!(
            parse(&["--wat"]),
            Err(StartupArgumentError::UnknownOption(_))
        ));
        assert!(matches!(
            parse(&["one.smc", "two.smc"]),
            Err(StartupArgumentError::UnexpectedPositional(_))
        ));
        assert_eq!(
            parse(&["--profile"]),
            Err(StartupArgumentError::MissingValue("--profile"))
        );
        assert_eq!(
            parse(&["--profile", "one", "--profile", "two"]),
            Err(StartupArgumentError::Duplicate("--profile"))
        );
        assert_eq!(
            parse(&["--level"]),
            Err(StartupArgumentError::MissingValue("--level"))
        );
        assert_eq!(
            parse(&["--level", "102", "--level", "105"]),
            Err(StartupArgumentError::Duplicate("--level"))
        );
        assert!(matches!(
            parse(&["--level", "not-a-level"]),
            Err(StartupArgumentError::InvalidLevel(_))
        ));
    }

    #[test]
    fn help_can_be_combined_with_configuration_for_frontend_owned_policy() {
        let options = parse(&["--help", "--ui-config", "ui.lmuicfg"]).unwrap();
        assert!(options.help);
        assert_eq!(options.ui_config, Some("ui.lmuicfg".into()));
    }
}
