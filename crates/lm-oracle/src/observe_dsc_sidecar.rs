use crate::{Observation, sha256_hex};
use lm_level::{DscDirective, DscDisplayContext, DscMaterialization, DscResolvedTable, DscSidecar};

#[must_use]
pub fn observe_dsc_sidecar(sidecar: &DscSidecar) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "dsc/source-length", &sidecar.source().len());
    put(
        &mut result,
        "dsc/source-sha256",
        &sha256_hex(sidecar.source()),
    );
    put(&mut result, "dsc/entry-count", &sidecar.entries().len());
    for (index, entry) in sidecar.entries().iter().enumerate() {
        let root = format!("dsc/entries/{index:04}");
        put(
            &mut result,
            &format!("{root}/key"),
            &format!("{:04x}", entry.key),
        );
        put(
            &mut result,
            &format!("{root}/flags"),
            &format!("{:08x}", entry.flags),
        );
        match &entry.directive {
            DscDirective::Description(description) => {
                put(&mut result, &format!("{root}/kind"), &"description");
                put(&mut result, &format!("{root}/text"), &description.text);
                observe_optional(&mut result, &root, "background", description.background);
                observe_optional(&mut result, &root, "detail", description.detail);
                observe_optional(&mut result, &root, "foreground", description.foreground);
                observe_optional(&mut result, &root, "mode", description.mode);
            }
            DscDirective::DisplayMapping(value) => {
                put(&mut result, &format!("{root}/kind"), &"display-mapping");
                put(
                    &mut result,
                    &format!("{root}/value"),
                    &format!("{value:04x}"),
                );
            }
            DscDirective::AlternateMapping(value) => {
                put(&mut result, &format!("{root}/kind"), &"alternate-mapping");
                put(
                    &mut result,
                    &format!("{root}/value"),
                    &format!("{value:04x}"),
                );
            }
        }
    }
    result
}

/// Records display substitutions for a caller-selected tile domain and feature state.
#[must_use]
pub fn observe_dsc_display(
    table: &DscResolvedTable,
    sources: impl IntoIterator<Item = u16>,
    context: DscDisplayContext,
) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "dsc-display/first-feature",
        &u8::from(context.first_feature_enabled),
    );
    put(
        &mut result,
        "dsc-display/first-suppressed",
        &u8::from(context.first_feature_suppressed),
    );
    put(
        &mut result,
        "dsc-display/second-feature",
        &u8::from(context.second_feature_enabled),
    );
    for source in sources {
        let resolved = table.resolve_display(source, context);
        let root = format!("dsc-display/tiles/{source:04x}");
        put(
            &mut result,
            &format!("{root}/target"),
            &format!("{:04x}", resolved.tile_id),
        );
        put(
            &mut result,
            &format!("{root}/blended"),
            &u8::from(resolved.blended),
        );
    }
    result
}

#[must_use]
pub fn observe_dsc_materialization(materialized: &DscMaterialization) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "dsc-materialization/cell-count",
        &materialized.mappings.len(),
    );
    for (index, (mapping, flags)) in materialized
        .mappings
        .iter()
        .zip(&materialized.flags)
        .enumerate()
    {
        if *mapping == 0 && *flags == 0 {
            continue;
        }
        let root = format!("dsc-materialization/cells/{index:04x}");
        put(
            &mut result,
            &format!("{root}/mapping"),
            &format!("{mapping:04x}"),
        );
        put(
            &mut result,
            &format!("{root}/flags"),
            &format!("{flags:02x}"),
        );
    }
    result
}

fn observe_optional(result: &mut Observation, root: &str, name: &str, value: Option<u32>) {
    if let Some(value) = value {
        put(result, &format!("{root}/{name}"), &format!("{value:06x}"));
    }
}

fn put(result: &mut Observation, path: &str, value: &impl ToString) {
    result
        .insert(path, value.to_string())
        .expect("DSC observation paths are unique");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observes_order_flags_styles_and_mapping_kind() {
        let sidecar = DscSidecar::decode(b"10\t0\ttext\\b112233\n11\t10\t1234\n").unwrap();
        let observed = observe_dsc_sidecar(&sidecar);
        assert_eq!(observed.get("dsc/entry-count"), Some("2"));
        assert_eq!(observed.get("dsc/entries/0000/text"), Some("text"));
        assert_eq!(observed.get("dsc/entries/0000/background"), Some("112233"));
        assert_eq!(
            observed.get("dsc/entries/0001/kind"),
            Some("alternate-mapping")
        );
    }

    #[test]
    fn observes_contextual_display_resolution() {
        let sidecar = DscSidecar::decode(b"10\t4\t1234\n").unwrap();
        let table = DscResolvedTable::from_sidecar(
            &sidecar,
            lm_level::DscDescriptionStyle {
                background: 0,
                detail: 0,
                foreground: 0,
                mode: 0,
            },
        );
        let observed = observe_dsc_display(
            &table,
            [0x10],
            DscDisplayContext {
                first_feature_enabled: true,
                ..DscDisplayContext::default()
            },
        );
        assert_eq!(observed.get("dsc-display/tiles/0010/target"), Some("1234"));
        assert_eq!(observed.get("dsc-display/tiles/0010/blended"), Some("1"));
    }

    #[test]
    fn observes_sparse_materialized_cells() {
        let observed = observe_dsc_materialization(&DscMaterialization {
            mappings: vec![0, 0x4104],
            flags: vec![0, 0x20],
        });
        assert_eq!(observed.get("dsc-materialization/cell-count"), Some("2"));
        assert_eq!(
            observed.get("dsc-materialization/cells/0001/mapping"),
            Some("4104")
        );
        assert_eq!(
            observed.get("dsc-materialization/cells/0001/flags"),
            Some("20")
        );
    }
}
