use crate::exanimation_io::exanimation_save_request;
use crate::overworld_endpoint_io::endpoint_save_request;
use crate::overworld_event_io::event_save_requests;
use crate::overworld_io::layer_save_requests;
use crate::overworld_message_io::message_save_request;
use crate::overworld_sprite_io::sprite_save_request;
use crate::palette_io::palette_save_request;
use crate::{
    EndpointIoError, EndpointRomLayout, EndpointSaveOptions, EventRevealIoError,
    EventRevealRomLayout, EventRevealSaveOptions, ExAnimationIoError, ExAnimationRomLayout,
    ExAnimationSaveOptions, MessageIoError, MessageRomLayout, MessageSaveOptions, OverworldIoError,
    OverworldLayers, OverworldLayersRomLayout, OverworldSaveOptions, PaletteIoError,
    PaletteRomLayout, PaletteSaveOptions, PayloadReclamation, PayloadSaveError, PayloadSaveResult,
    Project, SpriteIoError, SpriteRomLayout, SpriteSaveOptions,
};
use lm_graphics::{CompactExAnimation, Palette};
use lm_overworld::{EventRevealTable, OverworldEndpoint, OverworldMessage, OverworldSprite};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteOverworldRomLayout {
    pub layers: OverworldLayersRomLayout,
    pub event_reveals: EventRevealRomLayout,
    pub endpoints: EndpointRomLayout,
    pub messages: MessageRomLayout,
    pub sprites: SpriteRomLayout,
    pub palette: PaletteRomLayout,
    pub animation: ExAnimationRomLayout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteOverworldData {
    pub layers: OverworldLayers,
    pub event_reveals: EventRevealTable,
    pub endpoints: Vec<OverworldEndpoint>,
    pub messages: Vec<OverworldMessage>,
    pub sprites: Vec<OverworldSprite>,
    pub palette: Palette,
    pub animation: CompactExAnimation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteOverworldSaveOptions {
    pub layers: OverworldSaveOptions,
    pub event_reveals: EventRevealSaveOptions,
    pub endpoints: EndpointSaveOptions,
    pub messages: MessageSaveOptions,
    pub sprites: SpriteSaveOptions,
    pub palette: PaletteSaveOptions,
    pub animation: ExAnimationSaveOptions,
}

impl CompleteOverworldSaveOptions {
    /// Creates a conservative save configuration using one protected allocation policy for all
    /// nine payloads and no unverified previous-block reclamation hints.
    #[must_use]
    pub fn uniform_allocation(allocation: AllocationPolicy) -> Self {
        Self {
            layers: OverworldSaveOptions {
                layer1_allocation: allocation.clone(),
                layer2_allocation: allocation.clone(),
                previous_layer1: None,
                previous_layer2: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            event_reveals: EventRevealSaveOptions {
                source_allocation: allocation.clone(),
                destination_allocation: allocation.clone(),
                previous_sources: None,
                previous_destinations: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            endpoints: EndpointSaveOptions {
                allocation: allocation.clone(),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            messages: MessageSaveOptions {
                allocation: allocation.clone(),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            sprites: SpriteSaveOptions {
                allocation: allocation.clone(),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            palette: PaletteSaveOptions {
                allocation: allocation.clone(),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            animation: ExAnimationSaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedCompleteOverworld {
    pub layer1: PayloadSaveResult,
    pub layer2: PayloadSaveResult,
    pub event_sources: PayloadSaveResult,
    pub event_destinations: PayloadSaveResult,
    pub endpoints: PayloadSaveResult,
    pub messages: PayloadSaveResult,
    pub sprites: PayloadSaveResult,
    pub palette: PayloadSaveResult,
    pub animation: PayloadSaveResult,
}

#[derive(Clone, Copy)]
enum CompleteSaveCommit<'a> {
    WithoutChecksum,
    WithChecksum(usize),
    WithReclamation(PayloadReclamation<'a>),
}

#[derive(Debug)]
pub enum CompleteOverworldIoError {
    MapperMismatch {
        expected: Mapper,
        actual: Mapper,
        domain: &'static str,
    },
    Layers(OverworldIoError),
    Events(EventRevealIoError),
    Endpoints(EndpointIoError),
    Messages(MessageIoError),
    Sprites(SpriteIoError),
    Palette(PaletteIoError),
    Animation(ExAnimationIoError),
    Save(PayloadSaveError),
    InternalResultCount(usize),
}

impl fmt::Display for CompleteOverworldIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "complete overworld I/O failed: {self:?}")
    }
}

impl std::error::Error for CompleteOverworldIoError {}

impl From<PayloadSaveError> for CompleteOverworldIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Loads every modeled pointer-backed overworld domain through one revision layout.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteOverworldIoError`] for mapper disagreement or any domain load failure.
    pub fn load_complete_overworld(
        &self,
        slot: usize,
        layout: CompleteOverworldRomLayout,
        double_size_modes: &[bool],
    ) -> Result<CompleteOverworldData, CompleteOverworldIoError> {
        validate_mappers(&layout)?;
        Ok(CompleteOverworldData {
            layers: self
                .load_overworld_layers(slot, layout.layers)
                .map_err(CompleteOverworldIoError::Layers)?,
            event_reveals: self
                .load_event_reveals(slot, layout.event_reveals)
                .map_err(CompleteOverworldIoError::Events)?,
            endpoints: self
                .load_overworld_endpoints(slot, layout.endpoints)
                .map_err(CompleteOverworldIoError::Endpoints)?,
            messages: self
                .load_overworld_messages(slot, layout.messages)
                .map_err(CompleteOverworldIoError::Messages)?,
            sprites: self
                .load_overworld_sprites(slot, layout.sprites)
                .map_err(CompleteOverworldIoError::Sprites)?,
            palette: self
                .load_palette(slot, layout.palette)
                .map_err(CompleteOverworldIoError::Palette)?,
            animation: self
                .load_exanimation(slot, layout.animation, double_size_modes)
                .map_err(CompleteOverworldIoError::Animation)?,
        })
    }

    /// Validates and stages every modeled overworld payload, then commits all nine pointers and
    /// allocations as one undoable edit batch.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteOverworldIoError`] if any domain, mapper, allocation, or pointer fails.
    /// The ROM and history remain unchanged unless all nine payloads succeed.
    pub fn save_complete_overworld(
        &mut self,
        slot: usize,
        data: &CompleteOverworldData,
        layout: CompleteOverworldRomLayout,
        options: &CompleteOverworldSaveOptions,
        double_size_modes: &[bool],
    ) -> Result<SavedCompleteOverworld, CompleteOverworldIoError> {
        self.save_complete_overworld_group(
            slot,
            data,
            &layout,
            options,
            double_size_modes,
            CompleteSaveCommit::WithoutChecksum,
        )
    }

    /// Saves all nine modeled overworld payloads and the SNES checksum as one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteOverworldIoError`] when any domain, allocation, mapper, or checksum fails.
    pub fn save_complete_overworld_with_checksum(
        &mut self,
        slot: usize,
        data: &CompleteOverworldData,
        layout: CompleteOverworldRomLayout,
        options: &CompleteOverworldSaveOptions,
        double_size_modes: &[bool],
        checksum_field: usize,
    ) -> Result<SavedCompleteOverworld, CompleteOverworldIoError> {
        self.save_complete_overworld_group(
            slot,
            data,
            &layout,
            options,
            double_size_modes,
            CompleteSaveCommit::WithChecksum(checksum_field),
        )
    }

    /// Saves all nine payloads, reclaims exactly owned displaced blocks, and repairs checksum in
    /// one undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`CompleteOverworldIoError`] for domain validation, non-exact ownership,
    /// allocation, mapping, reclamation overlap, or checksum failure without partial mutation.
    pub fn save_complete_overworld_with_checksum_and_reclamation(
        &mut self,
        slot: usize,
        data: &CompleteOverworldData,
        layout: CompleteOverworldRomLayout,
        options: &CompleteOverworldSaveOptions,
        double_size_modes: &[bool],
        reclamation: PayloadReclamation<'_>,
    ) -> Result<SavedCompleteOverworld, CompleteOverworldIoError> {
        self.save_complete_overworld_group(
            slot,
            data,
            &layout,
            options,
            double_size_modes,
            CompleteSaveCommit::WithReclamation(reclamation),
        )
    }

    fn save_complete_overworld_group(
        &mut self,
        slot: usize,
        data: &CompleteOverworldData,
        layout: &CompleteOverworldRomLayout,
        options: &CompleteOverworldSaveOptions,
        double_size_modes: &[bool],
        commit: CompleteSaveCommit<'_>,
    ) -> Result<SavedCompleteOverworld, CompleteOverworldIoError> {
        validate_mappers(layout)?;
        let mut requests = Vec::with_capacity(9);
        requests.extend(
            layer_save_requests(slot, &data.layers, layout.layers, &options.layers)
                .map_err(CompleteOverworldIoError::Layers)?,
        );
        requests.extend(
            event_save_requests(
                slot,
                &data.event_reveals,
                layout.event_reveals,
                &options.event_reveals,
            )
            .map_err(CompleteOverworldIoError::Events)?,
        );
        requests.push(
            endpoint_save_request(slot, &data.endpoints, layout.endpoints, &options.endpoints)
                .map_err(CompleteOverworldIoError::Endpoints)?,
        );
        requests.push(
            message_save_request(slot, &data.messages, layout.messages, &options.messages)
                .map_err(CompleteOverworldIoError::Messages)?,
        );
        requests.push(
            sprite_save_request(slot, &data.sprites, layout.sprites, &options.sprites)
                .map_err(CompleteOverworldIoError::Sprites)?,
        );
        requests.push(
            palette_save_request(slot, &data.palette, layout.palette, &options.palette)
                .map_err(CompleteOverworldIoError::Palette)?,
        );
        requests.push(
            exanimation_save_request(
                slot,
                &data.animation,
                layout.animation,
                double_size_modes,
                &options.animation,
            )
            .map_err(CompleteOverworldIoError::Animation)?,
        );
        let description = format!("save complete overworld {slot:02x}");
        let results = match commit {
            CompleteSaveCommit::WithReclamation(reclamation) => self
                .save_tagged_payloads_with_checksum_and_reclamation(
                    description,
                    &requests,
                    reclamation.checksum_field,
                    reclamation.manifest,
                )?,
            CompleteSaveCommit::WithChecksum(field) => {
                self.save_tagged_payloads_with_checksum(description, &requests, field)?
            }
            CompleteSaveCommit::WithoutChecksum => {
                self.save_tagged_payloads(description, &requests)?
            }
        };
        let count = results.len();
        let [
            layer1,
            layer2,
            event_sources,
            event_destinations,
            endpoints,
            messages,
            sprites,
            palette,
            animation,
        ] = <[PayloadSaveResult; 9]>::try_from(results)
            .map_err(|_| CompleteOverworldIoError::InternalResultCount(count))?;
        Ok(SavedCompleteOverworld {
            layer1,
            layer2,
            event_sources,
            event_destinations,
            endpoints,
            messages,
            sprites,
            palette,
            animation,
        })
    }
}

fn validate_mappers(
    layout: &CompleteOverworldRomLayout,
) -> Result<Mapper, CompleteOverworldIoError> {
    let expected = layout.layers.mapper;
    for (domain, actual) in [
        ("event reveals", layout.event_reveals.mapper),
        ("endpoints", layout.endpoints.mapper),
        ("messages", layout.messages.mapper),
        ("sprites", layout.sprites.mapper),
        ("palette", layout.palette.mapper),
        ("animation", layout.animation.mapper),
    ] {
        if actual != expected {
            return Err(CompleteOverworldIoError::MapperMismatch {
                expected,
                actual,
                domain,
            });
        }
    }
    Ok(expected)
}

#[cfg(test)]
#[path = "overworld_full_io_tests.rs"]
mod tests;
