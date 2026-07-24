use crate::{complete_level_render_spec::CompleteLevelDscSpec, read_bounded_bytes};
use lm_level::{DscDescriptionStyle, DscResolvedTable, DscSidecar, MAX_DSC_SOURCE_LEN};

pub(crate) fn load(
    spec: &CompleteLevelDscSpec,
) -> Result<DscResolvedTable, Box<dyn std::error::Error>> {
    let bytes = read_bounded_bytes(&spec.path, MAX_DSC_SOURCE_LEN, "DSC sidecar")?;
    let source = DscSidecar::decode(&bytes)?;
    Ok(DscResolvedTable::from_sidecar(
        &source,
        DscDescriptionStyle {
            background: 0,
            detail: 0,
            foreground: 0,
            mode: 0,
        },
    ))
}
