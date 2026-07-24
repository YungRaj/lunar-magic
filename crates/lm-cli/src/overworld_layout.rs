use lm_project::{
    CompleteOverworldRomLayout, CompleteOverworldShape, EndpointRomLayout, EventRevealRomLayout,
    ExAnimationRomLayout, LevelPointerTable, MessageRomLayout, OverworldLayersRomLayout,
    PaletteRomLayout, SpriteRomLayout,
};
use lm_rom::Mapper;
use std::collections::BTreeMap;
use std::fmt;

const OVERWORLD_SLOTS: usize = 0x200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldLayoutDescriptor {
    pub layer1_table: usize,
    pub layer2_table: usize,
    pub event_source_table: usize,
    pub event_destination_table: usize,
    pub endpoint_table: usize,
    pub message_table: usize,
    pub sprite_table: usize,
    pub palette_table: usize,
    pub animation_table: usize,
    pub width: usize,
    pub height: usize,
    pub event_reveals: usize,
    pub endpoints: usize,
    pub messages: usize,
    pub sprites: usize,
    pub sprite_record_len: usize,
    pub palette_colors: usize,
    pub animation_max_records: usize,
    pub animation_max_encoded: usize,
}

impl OverworldLayoutDescriptor {
    pub const MAX_FILE_LEN: usize = 64 * 1024;
    pub const KEYS: [&'static str; 19] = [
        "layer1_table",
        "layer2_table",
        "event_source_table",
        "event_destination_table",
        "endpoint_table",
        "message_table",
        "sprite_table",
        "palette_table",
        "animation_table",
        "width",
        "height",
        "event_reveals",
        "endpoints",
        "messages",
        "sprites",
        "sprite_record_len",
        "palette_colors",
        "animation_max_records",
        "animation_max_encoded",
    ];

    /// Parses an exact key/value descriptor. Blank lines and `#` comments are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldLayoutError`] for malformed, unknown, duplicate, missing, or invalid
    /// values.
    pub fn parse(text: &str) -> Result<Self, OverworldLayoutError> {
        if text.len() > Self::MAX_FILE_LEN {
            return Err(OverworldLayoutError::TooLarge(text.len()));
        }
        let mut values = BTreeMap::new();
        for (index, raw) in text.lines().enumerate() {
            let line_number = index + 1;
            let line = raw.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or(OverworldLayoutError::MalformedLine(line_number))?;
            let key = key.trim();
            if !Self::KEYS.contains(&key) {
                return Err(OverworldLayoutError::UnknownKey {
                    line: line_number,
                    key: key.into(),
                });
            }
            let value =
                parse_number(value.trim()).map_err(|_| OverworldLayoutError::InvalidNumber {
                    line: line_number,
                    value: value.trim().into(),
                })?;
            if values.insert(key.to_owned(), value).is_some() {
                return Err(OverworldLayoutError::DuplicateKey(key.into()));
            }
        }
        let mut get = |key: &'static str| {
            values
                .remove(key)
                .ok_or(OverworldLayoutError::MissingKey(key))
        };
        Ok(Self {
            layer1_table: get("layer1_table")?,
            layer2_table: get("layer2_table")?,
            event_source_table: get("event_source_table")?,
            event_destination_table: get("event_destination_table")?,
            endpoint_table: get("endpoint_table")?,
            message_table: get("message_table")?,
            sprite_table: get("sprite_table")?,
            palette_table: get("palette_table")?,
            animation_table: get("animation_table")?,
            width: get("width")?,
            height: get("height")?,
            event_reveals: get("event_reveals")?,
            endpoints: get("endpoints")?,
            messages: get("messages")?,
            sprites: get("sprites")?,
            sprite_record_len: get("sprite_record_len")?,
            palette_colors: get("palette_colors")?,
            animation_max_records: get("animation_max_records")?,
            animation_max_encoded: get("animation_max_encoded")?,
        })
    }

    #[must_use]
    pub const fn shape(self) -> CompleteOverworldShape {
        CompleteOverworldShape {
            width: self.width,
            height: self.height,
            event_reveals: self.event_reveals,
            endpoints: self.endpoints,
            messages: self.messages,
            sprites: self.sprites,
            sprite_record_len: self.sprite_record_len,
            palette_colors: self.palette_colors,
        }
    }

    #[must_use]
    pub fn rom_layout(self, mapper: Mapper) -> CompleteOverworldRomLayout {
        let table = |offset| LevelPointerTable {
            offset,
            entries: OVERWORLD_SLOTS,
            stride: 3,
        };
        CompleteOverworldRomLayout {
            layers: OverworldLayersRomLayout {
                mapper,
                layer1: table(self.layer1_table),
                layer2: table(self.layer2_table),
                width: self.width,
                height: self.height,
            },
            event_reveals: EventRevealRomLayout {
                mapper,
                sources: table(self.event_source_table),
                destinations: table(self.event_destination_table),
                entries_per_slot: self.event_reveals,
            },
            endpoints: EndpointRomLayout {
                mapper,
                pointers: table(self.endpoint_table),
                endpoints_per_slot: self.endpoints,
            },
            messages: MessageRomLayout {
                mapper,
                pointers: table(self.message_table),
                messages_per_slot: self.messages,
            },
            sprites: SpriteRomLayout {
                mapper,
                pointers: table(self.sprite_table),
                sprites_per_slot: self.sprites,
                record_len: self.sprite_record_len,
            },
            palette: PaletteRomLayout {
                mapper,
                pointers: table(self.palette_table),
                colors_per_palette: self.palette_colors,
            },
            animation: ExAnimationRomLayout {
                mapper,
                pointers: table(self.animation_table),
                maximum_records: self.animation_max_records,
                maximum_encoded_len: self.animation_max_encoded,
            },
        }
    }

    #[must_use]
    pub const fn pointer_tables(self) -> [usize; 9] {
        [
            self.layer1_table,
            self.layer2_table,
            self.event_source_table,
            self.event_destination_table,
            self.endpoint_table,
            self.message_table,
            self.sprite_table,
            self.palette_table,
            self.animation_table,
        ]
    }
}

fn parse_number(value: &str) -> Result<usize, std::num::ParseIntError> {
    if let Some(hex) = value.strip_prefix("0x") {
        usize::from_str_radix(hex, 16)
    } else {
        value.parse()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldLayoutError {
    TooLarge(usize),
    MalformedLine(usize),
    UnknownKey { line: usize, key: String },
    DuplicateKey(String),
    InvalidNumber { line: usize, value: String },
    MissingKey(&'static str),
}

impl fmt::Display for OverworldLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid overworld layout descriptor: {self:?}")
    }
}

impl std::error::Error for OverworldLayoutError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    fn descriptor() -> String {
        SelfValues::VALUES
            .iter()
            .fold(String::new(), |mut output, (key, value)| {
                writeln!(output, "{key}={value}").unwrap();
                output
            })
    }

    struct SelfValues;
    impl SelfValues {
        const VALUES: [(&'static str, &'static str); 19] = [
            ("layer1_table", "0x100"),
            ("layer2_table", "0x700"),
            ("event_source_table", "0xd00"),
            ("event_destination_table", "0x1300"),
            ("endpoint_table", "0x1900"),
            ("message_table", "0x1f00"),
            ("sprite_table", "0x2500"),
            ("palette_table", "0x2b00"),
            ("animation_table", "0x3100"),
            ("width", "32"),
            ("height", "32"),
            ("event_reveals", "16"),
            ("endpoints", "8"),
            ("messages", "8"),
            ("sprites", "8"),
            ("sprite_record_len", "9"),
            ("palette_colors", "256"),
            ("animation_max_records", "32"),
            ("animation_max_encoded", "0x8000"),
        ];
    }

    #[test]
    fn exact_descriptor_builds_every_domain_layout() {
        let parsed = OverworldLayoutDescriptor::parse(&descriptor()).unwrap();
        assert_eq!(parsed.width, 32);
        assert_eq!(parsed.animation_max_encoded, 0x8000);
        let layout = parsed.rom_layout(Mapper::ExLoRom);
        assert_eq!(layout.layers.layer1.offset, 0x100);
        assert_eq!(layout.animation.pointers.entries, OVERWORLD_SLOTS);
        assert_eq!(layout.animation.mapper, Mapper::ExLoRom);
    }

    #[test]
    fn oversized_descriptor_is_rejected_before_parsing_fields() {
        assert_eq!(
            OverworldLayoutDescriptor::parse(
                &"#".repeat(OverworldLayoutDescriptor::MAX_FILE_LEN + 1)
            ),
            Err(OverworldLayoutError::TooLarge(
                OverworldLayoutDescriptor::MAX_FILE_LEN + 1
            ))
        );
    }

    #[test]
    fn unknown_duplicate_missing_and_bad_values_are_rejected() {
        assert!(matches!(
            OverworldLayoutDescriptor::parse("surprise=1"),
            Err(OverworldLayoutError::UnknownKey { .. })
        ));
        let duplicate = format!("{}layer1_table=2\n", descriptor());
        assert!(matches!(
            OverworldLayoutDescriptor::parse(&duplicate),
            Err(OverworldLayoutError::DuplicateKey(_))
        ));
        assert!(matches!(
            OverworldLayoutDescriptor::parse("layer1_table=1"),
            Err(OverworldLayoutError::MissingKey(_))
        ));
        let bad = descriptor().replace("width=32", "width=wide");
        assert!(matches!(
            OverworldLayoutDescriptor::parse(&bad),
            Err(OverworldLayoutError::InvalidNumber { .. })
        ));
    }
}
