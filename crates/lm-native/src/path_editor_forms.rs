use crate::level_editor_forms;
#[cfg(test)]
use crate::overworld_editor_forms::SUBMAP_NAMES;
use lm_overworld::{PathDirection, PathEdge, PathNode, Submap};

#[derive(Clone, Debug, Default)]
pub(crate) struct NodeForm {
    pub(crate) id: String,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) submap: usize,
    pub(crate) level: String,
    pub(crate) raw_flags: String,
}

impl NodeForm {
    pub(crate) fn load(value: PathNode) -> Self {
        Self {
            id: format!("{:04X}", value.id),
            x: format!("{:04X}", value.x),
            y: format!("{:04X}", value.y),
            submap: usize::from(value.submap.encoded()),
            level: value
                .level
                .map_or_else(String::new, |level| format!("{level:04X}")),
            raw_flags: format!("{:02X}", value.raw_flags),
        }
    }

    pub(crate) fn parse(&self) -> Result<PathNode, String> {
        Ok(PathNode {
            id: level_editor_forms::parse_hex_u16(&self.id, "path node ID")?,
            x: level_editor_forms::parse_hex_u16(&self.x, "path node X")?,
            y: level_editor_forms::parse_hex_u16(&self.y, "path node Y")?,
            submap: Submap::decode(u8::try_from(self.submap).unwrap_or(u8::MAX))
                .ok_or("invalid path node submap")?,
            level: optional_u16(&self.level, "path node level")?,
            raw_flags: level_editor_forms::parse_hex_u8(&self.raw_flags, "path node flags")?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EdgeForm {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) direction: usize,
    pub(crate) exit: String,
    pub(crate) raw_flags: String,
    pub(crate) one_way: bool,
    pub(crate) reciprocal: bool,
    pub(crate) reverse_exit: String,
    pub(crate) reverse_raw_flags: String,
}

impl EdgeForm {
    pub(crate) fn load(value: PathEdge) -> Self {
        Self {
            from: format!("{:04X}", value.from),
            to: format!("{:04X}", value.to),
            direction: direction_index(value.direction),
            exit: value
                .exit_index
                .map_or_else(String::new, |exit| format!("{exit:02X}")),
            raw_flags: format!("{:02X}", value.raw_flags),
            one_way: value.is_one_way(),
            reciprocal: false,
            reverse_exit: String::new(),
            reverse_raw_flags: "00".into(),
        }
    }

    pub(crate) fn load_with_edges(value: PathEdge, edges: &[PathEdge]) -> Self {
        let mut form = Self::load(value);
        if !value.is_one_way()
            && let Some(reverse) = edges.iter().find(|candidate| {
                candidate.from == value.to
                    && candidate.to == value.from
                    && candidate.direction == value.direction.opposite()
                    && !candidate.is_one_way()
            })
        {
            form.reciprocal = true;
            form.reverse_exit = reverse
                .exit_index
                .map_or_else(String::new, |exit| format!("{exit:02X}"));
            form.reverse_raw_flags = format!("{:02X}", reverse.raw_flags);
        }
        form
    }

    pub(crate) fn parse(&self) -> Result<PathEdge, String> {
        let mut edge = PathEdge {
            from: level_editor_forms::parse_hex_u16(&self.from, "path edge source")?,
            to: level_editor_forms::parse_hex_u16(&self.to, "path edge destination")?,
            direction: directions()[self.direction.min(3)],
            exit_index: optional_u8(&self.exit, "path edge exit")?,
            raw_flags: level_editor_forms::parse_hex_u8(&self.raw_flags, "path edge flags")?,
        };
        edge.set_one_way(self.one_way);
        Ok(edge)
    }

    pub(crate) fn parse_pair(&self) -> Result<Vec<PathEdge>, String> {
        let mut forward = self.parse()?;
        if !self.reciprocal {
            return Ok(vec![forward]);
        }
        forward.set_one_way(false);
        let mut reverse = PathEdge {
            from: forward.to,
            to: forward.from,
            direction: forward.direction.opposite(),
            exit_index: optional_u8(&self.reverse_exit, "reverse path edge exit")?,
            raw_flags: level_editor_forms::parse_hex_u8(
                &self.reverse_raw_flags,
                "reverse path edge flags",
            )?,
        };
        reverse.set_one_way(false);
        Ok(vec![forward, reverse])
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) const DIRECTION_NAMES: [&str; 4] = ["Up", "Right", "Down", "Left"];
#[cfg(test)]
#[allow(dead_code)]
pub(crate) const PATH_SUBMAP_NAMES: [&str; 7] = SUBMAP_NAMES;

const fn directions() -> [PathDirection; 4] {
    [
        PathDirection::Up,
        PathDirection::Right,
        PathDirection::Down,
        PathDirection::Left,
    ]
}

fn direction_index(value: PathDirection) -> usize {
    directions()
        .into_iter()
        .position(|item| item == value)
        .unwrap_or(0)
}

fn optional_u16(text: &str, name: &str) -> Result<Option<u16>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        level_editor_forms::parse_hex_u16(text, name).map(Some)
    }
}

fn optional_u8(text: &str, name: &str) -> Result<Option<u8>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        level_editor_forms::parse_hex_u8(text, name).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_form_preserves_unknown_flags_while_owning_one_way_bit() {
        let edge = PathEdge {
            from: 1,
            to: 2,
            direction: PathDirection::Left,
            exit_index: Some(0xaa),
            raw_flags: 0x80,
        };
        let mut form = EdgeForm::load(edge);
        form.one_way = true;
        let parsed = form.parse().unwrap();
        assert_eq!(parsed.raw_flags, 0x81);
        assert!(parsed.is_one_way());
        assert_eq!(parsed.exit_index, Some(0xaa));
    }

    #[test]
    fn reciprocal_form_loads_and_parses_both_field_complete_directions() {
        let forward = PathEdge {
            from: 1,
            to: 2,
            direction: PathDirection::Right,
            exit_index: Some(0xaa),
            raw_flags: 0x80,
        };
        let reverse = PathEdge {
            from: 2,
            to: 1,
            direction: PathDirection::Left,
            exit_index: Some(0xbb),
            raw_flags: 0xc0,
        };
        let form = EdgeForm::load_with_edges(forward, &[forward, reverse]);
        assert!(form.reciprocal);
        assert_eq!(form.reverse_exit, "BB");
        assert_eq!(form.parse_pair().unwrap(), [forward, reverse]);
    }
}
