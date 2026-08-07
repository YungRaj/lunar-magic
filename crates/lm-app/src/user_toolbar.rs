//! Lunar Magic-compatible `usertoolbar.txt` parser.
//!
//! Lunar Magic reads this UTF-8 file beside its executable at startup. It describes a second
//! toolbar, optional bitmap strips, internal commands, external tools, and shortcut overrides.

const MAX_BYTES: usize = 1024 * 1024;
const MAX_LINE_BYTES: usize = 4096;
const MAX_BUTTONS: usize = 512;
const MAX_IMAGES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserToolbarImageMode {
    Add,
    NewBase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserToolbarImage {
    pub mode: UserToolbarImageMode,
    pub path: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UserToolbarImageBase {
    #[default]
    Global,
    /// Base at the first image contributed by this entry in [`UserToolbar::images`].
    Image(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserToolbarGlobalOption {
    DisplayErrors(Option<u16>),
    HideToolbar,
    PreviousImageBase,
    GlobalImageBase,
    SetImageSize(u16),
    ForceImages { all: bool, first_index: Option<u16> },
    Flag(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserToolbarTarget {
    Spacer,
    Internal(String),
    External(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserToolbarButton {
    pub target: UserToolbarTarget,
    pub icon: Option<i32>,
    pub image_base: UserToolbarImageBase,
    pub tooltip: String,
    pub options: Vec<String>,
    pub shortcut: Vec<String>,
    pub working_directory: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UserToolbar {
    pub buttons: Vec<UserToolbarButton>,
    pub images: Vec<UserToolbarImage>,
    pub global_options: Vec<UserToolbarGlobalOption>,
    /// Explicit square icon size retained only when set before the first image, as in LM.
    pub image_size: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserToolbarError {
    TooLarge(usize),
    LineTooLong { line: usize, bytes: usize },
    TooManyButtons,
    TooManyImages,
    UnterminatedQuote(usize),
    ContentOutsideDefinition { line: usize, value: String },
    EmptyDefinition(usize),
    TooManyDefinitionLines(usize),
    InvalidNumber { line: usize, value: String },
    MissingArgument { line: usize, directive: String },
}

impl std::fmt::Display for UserToolbarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "user toolbar parse error: {self:?}")
    }
}

impl std::error::Error for UserToolbarError {}

impl UserToolbar {
    pub const MAX_FILE_BYTES: usize = MAX_BYTES;

    /// Parses one complete UTF-8 `usertoolbar.txt` using Lunar Magic's five-line button format.
    /// A new `***START***` implicitly finishes the preceding definition, as in the original.
    ///
    /// # Errors
    ///
    /// Returns a typed error for excessive input, malformed quoting or structure, invalid numeric
    /// directives, or unsupported content outside a button definition.
    pub fn parse(text: &str) -> Result<Self, UserToolbarError> {
        if text.len() > MAX_BYTES {
            return Err(UserToolbarError::TooLarge(text.len()));
        }
        let mut result = Self::default();
        let mut definition: Option<(usize, Vec<String>)> = None;
        let mut image_base = UserToolbarImageBase::Global;
        let mut previous_image_base = image_base;
        for (offset, raw) in text.lines().enumerate() {
            let line_number = offset + 1;
            if raw.len() > MAX_LINE_BYTES {
                return Err(UserToolbarError::LineTooLong {
                    line: line_number,
                    bytes: raw.len(),
                });
            }
            let line = strip_comment(raw, line_number)?.trim();
            if line.is_empty() {
                continue;
            }
            if line == "***START***" {
                finish_definition(&mut result, definition.take(), image_base)?;
                definition = Some((line_number, Vec::new()));
                continue;
            }
            if line == "***END***" {
                finish_definition(&mut result, definition.take(), image_base)?;
                continue;
            }
            if let Some((start, fields)) = &mut definition {
                if fields.len() == 5 {
                    return Err(UserToolbarError::TooManyDefinitionLines(*start));
                }
                fields.push(line.to_owned());
            } else {
                parse_global(
                    &mut result,
                    line,
                    line_number,
                    &mut image_base,
                    &mut previous_image_base,
                )?;
            }
        }
        finish_definition(&mut result, definition, image_base)?;
        Ok(result)
    }

    #[must_use]
    pub fn toolbar_visible(&self) -> bool {
        !self
            .global_options
            .contains(&UserToolbarGlobalOption::HideToolbar)
    }
}

fn finish_definition(
    toolbar: &mut UserToolbar,
    definition: Option<(usize, Vec<String>)>,
    image_base: UserToolbarImageBase,
) -> Result<(), UserToolbarError> {
    let Some((start, mut fields)) = definition else {
        return Ok(());
    };
    if fields.is_empty() {
        return Err(UserToolbarError::EmptyDefinition(start));
    }
    if toolbar.buttons.len() == MAX_BUTTONS {
        return Err(UserToolbarError::TooManyButtons);
    }
    while fields.len() < 5 {
        fields.push("LM_DEFAULT".into());
    }
    let target = match fields[0].as_str() {
        "LM_SPACER" => UserToolbarTarget::Spacer,
        value if value.starts_with("LM_") => UserToolbarTarget::Internal(value.into()),
        value => UserToolbarTarget::External(value.into()),
    };
    let (icon, tooltip) = if fields[1] == "LM_DEFAULT" {
        (None, String::new())
    } else {
        let (number, tooltip) = fields[1].split_once(',').unwrap_or((&fields[1], ""));
        let icon = number
            .trim()
            .parse::<i32>()
            .map_err(|_| UserToolbarError::InvalidNumber {
                line: start + 1,
                value: number.trim().into(),
            })?;
        (Some(icon), tooltip.replace("\\n", "\n"))
    };
    toolbar.buttons.push(UserToolbarButton {
        target,
        icon,
        image_base,
        tooltip,
        options: comma_fields(&fields[2]),
        shortcut: comma_fields(&fields[3]),
        working_directory: (fields[4] != "LM_DEFAULT").then(|| unquote(&fields[4]).into()),
    });
    Ok(())
}

fn comma_fields(value: &str) -> Vec<String> {
    if value == "LM_DEFAULT" {
        return Vec::new();
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(unquote)
        .map(str::to_owned)
        .collect()
}

fn parse_global(
    toolbar: &mut UserToolbar,
    line: &str,
    line_number: usize,
    image_base: &mut UserToolbarImageBase,
    previous_image_base: &mut UserToolbarImageBase,
) -> Result<(), UserToolbarError> {
    let (name, argument) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(a, b)| (a, b.trim()));
    match name {
        "LM_ADDIMAGE" | "LM_NEWIMAGE" => {
            if argument.is_empty() {
                return Err(UserToolbarError::MissingArgument {
                    line: line_number,
                    directive: name.into(),
                });
            }
            if toolbar.images.len() == MAX_IMAGES {
                return Err(UserToolbarError::TooManyImages);
            }
            let mode = if name == "LM_ADDIMAGE" {
                UserToolbarImageMode::Add
            } else {
                UserToolbarImageMode::NewBase
            };
            if mode == UserToolbarImageMode::NewBase {
                *previous_image_base = *image_base;
                *image_base = UserToolbarImageBase::Image(toolbar.images.len());
            }
            toolbar.images.push(UserToolbarImage {
                mode,
                path: unquote(argument).into(),
            });
        }
        "LM_DISPLAY_ERRORS" => toolbar
            .global_options
            .push(UserToolbarGlobalOption::DisplayErrors(parse_optional_u16(
                argument,
                line_number,
            )?)),
        "LM_NO_TOOLBAR" => toolbar
            .global_options
            .push(UserToolbarGlobalOption::HideToolbar),
        "LM_IMAGEBASE_PREVIOUS" => {
            std::mem::swap(image_base, previous_image_base);
            toolbar
                .global_options
                .push(UserToolbarGlobalOption::PreviousImageBase);
        }
        "LM_IMAGEBASE_GLOBAL" => {
            *previous_image_base = *image_base;
            *image_base = UserToolbarImageBase::Global;
            toolbar
                .global_options
                .push(UserToolbarGlobalOption::GlobalImageBase);
        }
        "LM_SETIMAGE_SIZE" => {
            let size = parse_required_u16(name, argument, line_number)?;
            if toolbar.images.is_empty() {
                toolbar.image_size = Some(size);
            }
            toolbar
                .global_options
                .push(UserToolbarGlobalOption::SetImageSize(size));
        }
        "LM_USEIMAGE_FORCE" | "LM_USEIMAGE_FORCE_ALL" => {
            toolbar
                .global_options
                .push(UserToolbarGlobalOption::ForceImages {
                    all: name.ends_with("_ALL"),
                    first_index: parse_optional_u16(argument, line_number)?,
                });
        }
        value if value.starts_with("LM_") && argument.is_empty() => toolbar
            .global_options
            .push(UserToolbarGlobalOption::Flag(value.into())),
        _ => {
            return Err(UserToolbarError::ContentOutsideDefinition {
                line: line_number,
                value: line.into(),
            });
        }
    }
    Ok(())
}

fn parse_required_u16(name: &str, value: &str, line: usize) -> Result<u16, UserToolbarError> {
    if value.is_empty() {
        return Err(UserToolbarError::MissingArgument {
            line,
            directive: name.into(),
        });
    }
    value.parse().map_err(|_| UserToolbarError::InvalidNumber {
        line,
        value: value.into(),
    })
}

fn parse_optional_u16(value: &str, line: usize) -> Result<Option<u16>, UserToolbarError> {
    if value.is_empty() {
        Ok(None)
    } else {
        value
            .parse()
            .map(Some)
            .map_err(|_| UserToolbarError::InvalidNumber {
                line,
                value: value.into(),
            })
    }
}

fn strip_comment(value: &str, line: usize) -> Result<&str, UserToolbarError> {
    let mut double = false;
    let mut single = false;
    for (index, character) in value.char_indices() {
        match character {
            '"' if !single => double = !double,
            '\'' if !double
                && (single
                    || value[index + character.len_utf8()..]
                        .chars()
                        .nth(1)
                        .is_some_and(|closing| closing == '\'')) =>
            {
                single = !single;
            }
            ';' if !double && !single => return Ok(&value[..index]),
            _ => {}
        }
    }
    if double || single {
        Err(UserToolbarError::UnterminatedQuote(line))
    } else {
        Ok(value)
    }
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_original_help_examples_and_implicit_end() {
        let parsed = UserToolbar::parse(
            r#"
            ; official-style example
            LM_ADDIMAGE "our bitmap.bmp"
            LM_SETIMAGE_SIZE 24
            ***START***
            LM_SPACER
            ***START***
            LM_VIEW_ADD_SPRITE
            2,Add a sprite\nClick to open
            LM_DEFAULT
            's', VK_CONTROL
            ***START***
            "notepad.exe" "readme.txt" ; semicolon outside quotes
            0,Read the readme
            LM_CLOSE_ON_CLOSE
            'n',VK_CONTROL
            "%4"
            ***END***
        "#,
        )
        .unwrap();
        assert_eq!(parsed.images[0].path, "our bitmap.bmp");
        assert_eq!(
            parsed.global_options,
            vec![UserToolbarGlobalOption::SetImageSize(24)]
        );
        assert_eq!(parsed.buttons.len(), 3);
        assert_eq!(parsed.buttons[0].target, UserToolbarTarget::Spacer);
        assert_eq!(parsed.buttons[1].tooltip, "Add a sprite\nClick to open");
        assert_eq!(parsed.buttons[1].shortcut, ["'s'", "VK_CONTROL"]);
        assert_eq!(parsed.buttons[2].working_directory.as_deref(), Some("%4"));
    }

    #[test]
    fn hide_toolbar_keeps_shortcut_buttons_loaded() {
        let parsed = UserToolbar::parse("LM_NO_TOOLBAR\n***START***\nLM_FILE_SAVE\nLM_DEFAULT\nLM_DEFAULT\n's',VK_CONTROL\n***END***").unwrap();
        assert!(!parsed.toolbar_visible());
        assert_eq!(parsed.buttons[0].shortcut, ["'s'", "VK_CONTROL"]);
    }

    #[test]
    fn comments_respect_quoted_semicolons_and_bad_input_is_atomic() {
        let parsed =
            UserToolbar::parse("***START***\n\"tool.exe\" \"a;b\"; comment\n***END***").unwrap();
        assert_eq!(
            parsed.buttons[0].target,
            UserToolbarTarget::External("\"tool.exe\" \"a;b\"".into())
        );
        assert!(matches!(
            UserToolbar::parse("***START***\nLM_FILE_SAVE\n0,ok\nA\nB\nC\nD"),
            Err(UserToolbarError::TooManyDefinitionLines(1))
        ));
        assert!(matches!(
            UserToolbar::parse("LM_SETIMAGE_SIZE nope"),
            Err(UserToolbarError::InvalidNumber { .. })
        ));
        let apostrophe = UserToolbar::parse(
            "***START***\nLM_FILE_SAVE_BUTTON\n0,Lunar Magic's save; comment\n***END***",
        )
        .unwrap();
        assert_eq!(apostrophe.buttons[0].tooltip, "Lunar Magic's save");
        let semicolon_key = UserToolbar::parse(
            "***START***\nLM_FILE_SAVE_BUTTON\nLM_DEFAULT\nLM_DEFAULT\n';',VK_CONTROL; comment\n***END***",
        )
        .unwrap();
        assert_eq!(semicolon_key.buttons[0].shortcut, ["';'", "VK_CONTROL"]);
    }

    #[test]
    fn parses_lunar_magic_363_oracle_fixture() {
        let parsed = UserToolbar::parse(include_str!(
            "../../../docs/oracle-work/lm363/user-toolbar/usertoolbar.txt"
        ))
        .unwrap();
        assert!(!parsed.toolbar_visible());
        assert_eq!(parsed.buttons.len(), 3);
        assert_eq!(
            parsed.buttons[1].target,
            UserToolbarTarget::Internal("LM_VIEW_OVERWORLD".into())
        );
        assert_eq!(
            parsed.buttons[1].shortcut,
            ["'o'", "VK_CONTROL", "VK_SHIFT"]
        );
        assert_eq!(
            parsed.buttons[2].tooltip,
            "External oracle button\nSecond tooltip line"
        );
        assert_eq!(parsed.buttons[2].working_directory.as_deref(), Some("%4"));
    }

    #[test]
    fn image_base_directives_are_retained_at_each_button_definition() {
        let parsed = UserToolbar::parse(
            "LM_ADDIMAGE \"global.bmp\"\n***START***\nLM_VIEW_16x16\n1,global\n***END***\nLM_NEWIMAGE \"new.bmp\"\n***START***\nLM_VIEW_16x16\n1,new\n***END***\nLM_IMAGEBASE_PREVIOUS\n***START***\nLM_VIEW_16x16\n1,previous\n***END***\nLM_IMAGEBASE_GLOBAL\n***START***\nLM_VIEW_16x16\n1,global again\n***END***",
        )
        .unwrap();
        assert_eq!(
            parsed
                .buttons
                .iter()
                .map(|button| button.image_base)
                .collect::<Vec<_>>(),
            [
                UserToolbarImageBase::Global,
                UserToolbarImageBase::Image(1),
                UserToolbarImageBase::Global,
                UserToolbarImageBase::Global,
            ]
        );
    }

    #[test]
    fn image_size_only_takes_effect_before_the_first_image() {
        let before = UserToolbar::parse("LM_SETIMAGE_SIZE 24\nLM_ADDIMAGE \"a.bmp\"").unwrap();
        assert_eq!(before.image_size, Some(24));
        let after = UserToolbar::parse("LM_ADDIMAGE \"a.bmp\"\nLM_SETIMAGE_SIZE 24").unwrap();
        assert_eq!(after.image_size, None);
        assert!(after
            .global_options
            .contains(&UserToolbarGlobalOption::SetImageSize(24)));
    }
}
