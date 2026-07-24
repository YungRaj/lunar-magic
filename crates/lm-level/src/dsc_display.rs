use crate::DscResolvedTable;

/// Editor feature state used by Lunar Magic's conditional custom-display selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DscDisplayContext {
    /// Enables mappings carrying source flag bit 2.
    pub first_feature_enabled: bool,
    /// Native state that suppresses the first feature even when enabled.
    pub first_feature_suppressed: bool,
    /// Enables mappings carrying source flag bit 1.
    pub second_feature_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DscDisplayResolution {
    pub tile_id: u16,
    /// Lunar Magic selects its averaged/blended pixel path for this display.
    pub blended: bool,
}

impl DscResolvedTable {
    /// Resolves the direct render-time mapping used by `RenderMap16TileToPixelBuffer`.
    ///
    /// This covers both its built-in conditional tile substitutions and `.dsc` display mappings.
    /// Alternate mappings belong to level-cell materialization and are intentionally separate.
    #[must_use]
    pub fn resolve_display(&self, source: u16, context: DscDisplayContext) -> DscDisplayResolution {
        let mut tile_id = source & 0x7fff;
        let mut blended = false;
        match tile_id {
            0x21 | 0x22 if context.second_feature_enabled => {
                tile_id = 0x114;
                blended = true;
            }
            0x23 if context.second_feature_enabled => {
                tile_id = 0x113;
                blended = true;
            }
            0x24 if context.second_feature_enabled => {
                tile_id = 0x115;
                blended = true;
            }
            0x27..=0x2a if context.first_feature_enabled && !context.first_feature_suppressed => {
                blended = true;
            }
            _ => {}
        }

        if let Some(entry) = self.get(tile_id) {
            if entry.native_flags & 1 != 0 {
                blended = true;
            }
            let first_mapping = entry.native_flags & 2 != 0
                && context.first_feature_enabled
                && !context.first_feature_suppressed;
            let second_mapping = entry.native_flags & 4 != 0 && context.second_feature_enabled;
            if (first_mapping || second_mapping)
                && let Some(mapping) = entry.display_mapping
            {
                tile_id = mapping;
                blended = true;
            }
        }
        DscDisplayResolution { tile_id, blended }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DscDescriptionStyle, DscSidecar};

    const DEFAULTS: DscDescriptionStyle = DscDescriptionStyle {
        background: 0,
        detail: 0,
        foreground: 0,
        mode: 0,
    };

    #[test]
    fn conditional_dsc_mappings_follow_native_feature_flags() {
        let source = DscSidecar::decode(b"10\t2\t1234\n11\t4\t2345\n").unwrap();
        let table = DscResolvedTable::from_sidecar(&source, DEFAULTS);
        assert_eq!(
            table
                .resolve_display(0x10, DscDisplayContext::default())
                .tile_id,
            0x10
        );
        assert_eq!(
            table
                .resolve_display(
                    0x10,
                    DscDisplayContext {
                        second_feature_enabled: true,
                        ..DscDisplayContext::default()
                    }
                )
                .tile_id,
            0x1234
        );
        assert_eq!(
            table
                .resolve_display(
                    0x11,
                    DscDisplayContext {
                        first_feature_enabled: true,
                        ..DscDisplayContext::default()
                    }
                )
                .tile_id,
            0x2345
        );
        assert_eq!(
            table
                .resolve_display(
                    0x11,
                    DscDisplayContext {
                        first_feature_enabled: true,
                        first_feature_suppressed: true,
                        second_feature_enabled: false,
                    }
                )
                .tile_id,
            0x11
        );
    }

    #[test]
    fn built_in_second_feature_substitutions_are_recovered() {
        let table = DscResolvedTable::from_sidecar(&DscSidecar::decode(b"").unwrap(), DEFAULTS);
        let context = DscDisplayContext {
            second_feature_enabled: true,
            ..DscDisplayContext::default()
        };
        assert_eq!(table.resolve_display(0x21, context).tile_id, 0x114);
        assert_eq!(table.resolve_display(0x23, context).tile_id, 0x113);
        assert_eq!(table.resolve_display(0x24, context).tile_id, 0x115);
        assert!(table.resolve_display(0x21, context).blended);
    }
}
