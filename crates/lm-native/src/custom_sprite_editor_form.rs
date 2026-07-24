use crate::level_editor_forms;
use lm_level::CustomSpriteEntry;

#[derive(Clone, Debug, Default)]
pub(crate) struct CustomSpriteForm {
    pub(crate) sprite_records: String,
    pub(crate) description: String,
    pub(crate) insert_index: usize,
    pub(crate) move_to: usize,
}

impl CustomSpriteForm {
    pub(crate) fn load(entry: &CustomSpriteEntry, index: usize) -> Self {
        Self {
            sprite_records: entry
                .sprites
                .iter()
                .map(|sprite| level_editor_forms::format_bytes(&sprite.encoded))
                .collect::<Vec<_>>()
                .join("\n"),
            description: entry.description.clone(),
            insert_index: index,
            move_to: index,
        }
    }

    pub(crate) fn entry(&self) -> Result<CustomSpriteEntry, String> {
        let sprites = self
            .sprite_records
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(level_editor_forms::parse_sprite)
            .collect::<Result<Vec<_>, _>>()?;
        CustomSpriteEntry::new(sprites, self.description.clone()).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::SpriteRecord;

    #[test]
    fn form_round_trips_multi_record_placement_and_unicode() {
        let value = CustomSpriteEntry::new(
            vec![
                SpriteRecord {
                    encoded: vec![1, 2, 3],
                },
                SpriteRecord {
                    encoded: vec![0, 4, 5, 6],
                },
            ],
            "Boss ★".into(),
        )
        .unwrap();
        assert_eq!(CustomSpriteForm::load(&value, 0).entry().unwrap(), value);
    }

    #[test]
    fn form_rejects_empty_placement_and_internal_boundary() {
        assert!(CustomSpriteForm::default().entry().is_err());
        let form = CustomSpriteForm {
            sprite_records: "01 02 03\n01 04 05".into(),
            description: "bad boundary".into(),
            ..CustomSpriteForm::default()
        };
        assert!(form.entry().is_err());
    }
}
