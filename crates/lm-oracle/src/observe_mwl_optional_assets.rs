use crate::{
    ExAnimationObservationError, Observation, observe_compact_exanimation_with_modes,
    observe_palette,
};
use lm_project::MwlOptionalLevelAssets;

/// Produces a field-addressable snapshot of the semantic palette and `ExAnimation`
/// content in an MWL file.
///
/// MWL section metadata is intentionally omitted because Lunar Magic rewrites its
/// allocator/source-address provenance when it imports and re-exports a level.
///
/// # Errors
///
/// Returns [`ExAnimationObservationError`] when an ordinary record cannot be interpreted by the
/// supplied size-mode table.
pub fn observe_mwl_optional_assets(
    assets: &MwlOptionalLevelAssets,
    double_size_modes: &[bool],
) -> Result<Observation, ExAnimationObservationError> {
    let mut result = Observation::new();
    merge(
        &mut result,
        "mwl/optional-assets",
        &observe_palette(&assets.palette),
    );
    put(
        &mut result,
        "mwl/optional-assets/exanimation/present",
        &assets.exanimation.is_some(),
    );
    if let Some(animation) = &assets.exanimation {
        merge(
            &mut result,
            "mwl/optional-assets",
            &observe_compact_exanimation_with_modes(animation, double_size_modes)?,
        );
    }
    Ok(result)
}

fn merge(result: &mut Observation, prefix: &str, source: &Observation) {
    for (path, value) in source.entries() {
        result
            .insert(format!("{prefix}/{path}"), value)
            .expect("composed MWL optional-asset paths are unique");
    }
}

fn put(result: &mut Observation, path: &str, value: &impl ToString) {
    result
        .insert(path, value.to_string())
        .expect("MWL optional-asset observation paths are unique");
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationRecord, Palette};

    fn assets() -> MwlOptionalLevelAssets {
        MwlOptionalLevelAssets {
            palette_metadata: [1, 2],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            exanimation_metadata: [3, 4],
            exanimation: Some(CompactExAnimation {
                setting: 5,
                header_value: 6,
                trigger_mask: 1 << 3,
                trigger_values: {
                    let mut values = [0; 16];
                    values[3] = 7;
                    values
                },
                records: vec![ExAnimationRecord::inactive()],
            }),
        }
    }

    #[test]
    fn observes_fields_but_not_relocation_metadata() {
        let mut value = assets();
        let before = observe_mwl_optional_assets(&value, &[false; 256]).unwrap();
        value.palette_metadata = [9, 10];
        value.exanimation_metadata = [11, 12];
        assert_eq!(
            before,
            observe_mwl_optional_assets(&value, &[false; 256]).unwrap()
        );
        assert_eq!(
            before.get("mwl/optional-assets/palette/colors/0100/bgr555"),
            Some("256")
        );
        assert_eq!(
            before.get("mwl/optional-assets/exanimation/triggers/03"),
            Some("7")
        );
    }

    #[test]
    fn absence_is_explicit() {
        let mut value = assets();
        value.exanimation = None;
        let observation = observe_mwl_optional_assets(&value, &[]).unwrap();
        assert_eq!(
            observation.get("mwl/optional-assets/exanimation/present"),
            Some("false")
        );
        assert!(
            observation
                .get("mwl/optional-assets/exanimation/record-count")
                .is_none()
        );
    }
}
