use super::{Observation, put, sha256_hex};
use lm_overworld::{OverworldMessage, OverworldSprite};

#[must_use]
pub fn observe_overworld_messages(messages: &[OverworldMessage]) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "overworld/messages/count", messages.len());
    for (index, message) in messages.iter().enumerate() {
        put(
            &mut result,
            &format!("overworld/messages/{index:04x}/sha256"),
            sha256_hex(message.encoded()),
        );
    }
    result
}

#[must_use]
pub fn observe_overworld_sprites(sprites: &[OverworldSprite]) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "overworld/sprites/count", sprites.len());
    for (index, sprite) in sprites.iter().enumerate() {
        let base = format!("overworld/sprites/{index:04x}");
        put(&mut result, &format!("{base}/id"), sprite.id);
        put(&mut result, &format!("{base}/x"), sprite.x);
        put(&mut result, &format!("{base}/y"), sprite.y);
        put(
            &mut result,
            &format!("{base}/submap"),
            sprite.submap.encoded(),
        );
        put(
            &mut result,
            &format!("{base}/extra-sha256"),
            sha256_hex(&sprite.extra),
        );
    }
    result
}
