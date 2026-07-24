use crate::{Observation, ObservationError};
use lm_profile::{GroupedMap16RemapRecord, Map16RemapRange};

/// Observes decoded Map16 remap groups independently of pointers and RATS placement.
///
/// # Errors
///
/// Returns an observation error if a generated semantic path collides.
pub fn observe_map16_remaps(
    range_groups: &[Vec<Map16RemapRange>],
    record_groups: &[Vec<GroupedMap16RemapRecord>],
) -> Result<Observation, ObservationError> {
    let mut result = Observation::new();
    result.insert(
        "map16/remap/range-group-count",
        range_groups.len().to_string(),
    )?;
    for (group, records) in range_groups.iter().enumerate() {
        result.insert(
            format!("map16/remap/range-groups/{group:02x}/count"),
            records.len().to_string(),
        )?;
        for (index, record) in records.iter().enumerate() {
            result.insert(
                format!("map16/remap/range-groups/{group:02x}/{index:04x}"),
                format!("{:04x}:{:04x}", record.source_tile, record.destination_tile),
            )?;
        }
    }
    result.insert(
        "map16/remap/record-group-count",
        record_groups.len().to_string(),
    )?;
    for (group, records) in record_groups.iter().enumerate() {
        result.insert(
            format!("map16/remap/record-groups/{group:02x}/count"),
            records.len().to_string(),
        )?;
        for (index, record) in records.iter().enumerate() {
            result.insert(
                format!("map16/remap/record-groups/{group:02x}/{index:04x}"),
                format!(
                    "{:02x}:{:04x}:{:04x}",
                    record.flags, record.source_tile, record.destination_tile
                ),
            )?;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_independently_addressable() {
        let observation = observe_map16_remaps(
            &[vec![Map16RemapRange {
                source_tile: 0x123,
                destination_tile: 0x456,
            }]],
            &[vec![GroupedMap16RemapRecord {
                flags: 1,
                source_tile: 0x789,
                destination_tile: 0xabc,
            }]],
        )
        .unwrap();
        assert_eq!(
            observation.get("map16/remap/range-groups/00/0000"),
            Some("0123:0456")
        );
        assert_eq!(
            observation.get("map16/remap/record-groups/00/0000"),
            Some("01:0789:0abc")
        );
    }
}
