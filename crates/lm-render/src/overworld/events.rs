use super::{OverworldRenderError, validate_layer};
use lm_overworld::{EventReveal, EventTileChange, OverworldLayer};

/// Applies event tile changes through `completed_event`, in event-table order.
///
/// A change is applied only when the current tile still matches its recorded `before` value. This
/// makes overlapping/replayed event records deterministic and prevents a stale event from
/// replacing an unrelated edit.
///
/// # Errors
///
/// Returns [`OverworldRenderError`] if the layer shape or an applicable event coordinate is invalid.
pub fn apply_event_changes(
    layer: &OverworldLayer,
    events: &[EventTileChange],
    completed_event: u8,
) -> Result<OverworldLayer, OverworldRenderError> {
    validate_layer(1, layer)?;
    let mut result = layer.clone();
    for change in events
        .iter()
        .filter(|change| change.event.0 <= completed_event)
    {
        let x = usize::from(change.x);
        let y = usize::from(change.y);
        let Some(index) = y
            .checked_mul(result.width)
            .and_then(|row| row.checked_add(x))
        else {
            return Err(OverworldRenderError::CoordinateOverflow);
        };
        let Some(tile) = result
            .tiles
            .get_mut(index)
            .filter(|_| x < result.width && y < result.height)
        else {
            return Err(OverworldRenderError::EventCoordinateOutOfRange {
                event: change.event.0,
                x: change.x,
                y: change.y,
            });
        };
        if *tile == change.before {
            *tile = change.after;
        }
    }
    Ok(result)
}

/// Applies the first `completed_reveals` source/destination substitutions in table order.
///
/// Every occurrence of the source Map16 tile is replaced. Later entries therefore observe the
/// result of earlier entries, matching the ordered reveal-table representation.
#[must_use]
pub fn apply_event_reveals(
    layer: &OverworldLayer,
    reveals: &[EventReveal],
    completed_reveals: usize,
) -> OverworldLayer {
    let mut result = layer.clone();
    for reveal in reveals.iter().take(completed_reveals) {
        for tile in &mut result.tiles {
            if *tile == reveal.source_tile {
                *tile = reveal.destination_tile;
            }
        }
    }
    result
}
