use crate::{
    LevelLoadError, LevelPointerTable, PayloadLoadError, PayloadReadPolicy, PayloadSaveError,
    PayloadSaveRequest, PayloadSaveResult, Project,
};
use lm_overworld::{OverworldLayer, OverworldLayerEncodingError};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldLayersRomLayout {
    pub mapper: Mapper,
    pub layer1: LevelPointerTable,
    pub layer2: LevelPointerTable,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldLayers {
    pub layer1: OverworldLayer,
    pub layer2: OverworldLayer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldSaveOptions {
    pub layer1_allocation: AllocationPolicy,
    pub layer2_allocation: AllocationPolicy,
    pub previous_layer1: Option<RatsBlock>,
    pub previous_layer2: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedOverworldLayers {
    pub layer1: PayloadSaveResult,
    pub layer2: PayloadSaveResult,
}

#[derive(Debug)]
pub enum OverworldIoError {
    Layout(LevelLoadError),
    SizeOverflow,
    LayerShape {
        layer: u8,
        width: usize,
        height: usize,
        tiles: usize,
    },
    Load(PayloadLoadError),
    Decode {
        layer: u8,
        bytes: Vec<u8>,
    },
    Save(PayloadSaveError),
    Encode(OverworldLayerEncodingError),
}

impl fmt::Display for OverworldIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld I/O failed: {self:?}")
    }
}

impl std::error::Error for OverworldIoError {}

impl From<LevelLoadError> for OverworldIoError {
    fn from(value: LevelLoadError) -> Self {
        Self::Layout(value)
    }
}

impl From<PayloadLoadError> for OverworldIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<PayloadSaveError> for OverworldIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl From<OverworldLayerEncodingError> for OverworldIoError {
    fn from(value: OverworldLayerEncodingError) -> Self {
        Self::Encode(value)
    }
}

impl Project {
    /// Loads both tile layers of one overworld slot from fixed or tagged payloads.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldIoError`] for invalid dimensions, tables, pointers, or layer bytes.
    pub fn load_overworld_layers(
        &self,
        slot: usize,
        layout: OverworldLayersRomLayout,
    ) -> Result<OverworldLayers, OverworldIoError> {
        let encoded_len = layer_encoded_len(layout)?;
        let layer1 = self.load_payload(
            layout.layer1.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: encoded_len },
        )?;
        let layer2 = self.load_payload(
            layout.layer2.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: encoded_len },
        )?;
        Ok(OverworldLayers {
            layer1: OverworldLayer::decode_le(layout.width, layout.height, &layer1.bytes)
                .map_err(|bytes| OverworldIoError::Decode { layer: 1, bytes })?,
            layer2: OverworldLayer::decode_le(layout.width, layout.height, &layer2.bytes)
                .map_err(|bytes| OverworldIoError::Decode { layer: 2, bytes })?,
        })
    }

    /// Saves both overworld tile layers as one atomic and undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldIoError`] for shape mismatches, layouts, allocation, or mapping failures.
    pub fn save_overworld_layers(
        &mut self,
        slot: usize,
        layers: &OverworldLayers,
        layout: OverworldLayersRomLayout,
        options: &OverworldSaveOptions,
    ) -> Result<SavedOverworldLayers, OverworldIoError> {
        let requests = layer_save_requests(slot, layers, layout, options)?;
        let mut results = self.save_tagged_payloads(
            format!("save complete overworld slot {slot:02x}"),
            &requests,
        )?;
        Ok(SavedOverworldLayers {
            layer1: results.remove(0),
            layer2: results.remove(0),
        })
    }
}

pub(crate) fn layer_save_requests(
    slot: usize,
    layers: &OverworldLayers,
    layout: OverworldLayersRomLayout,
    options: &OverworldSaveOptions,
) -> Result<[PayloadSaveRequest; 2], OverworldIoError> {
    validate_layer(1, &layers.layer1, layout)?;
    validate_layer(2, &layers.layer2, layout)?;
    let maximum_payload_len = layer_encoded_len(layout)?;
    Ok([
        PayloadSaveRequest {
            description: format!("save overworld slot {slot:02x} layer 1"),
            payload: layers.layer1.encode_le()?,
            pointer: layout.layer1.pointer_offset(slot)?.into(),
            mapper: layout.mapper,
            allocation_policy: options.layer1_allocation.clone(),
            previous_block: options.previous_layer1.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len,
            erase_fill: options.erase_fill,
        },
        PayloadSaveRequest {
            description: format!("save overworld slot {slot:02x} layer 2"),
            payload: layers.layer2.encode_le()?,
            pointer: layout.layer2.pointer_offset(slot)?.into(),
            mapper: layout.mapper,
            allocation_policy: options.layer2_allocation.clone(),
            previous_block: options.previous_layer2.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len,
            erase_fill: options.erase_fill,
        },
    ])
}

fn layer_encoded_len(layout: OverworldLayersRomLayout) -> Result<usize, OverworldIoError> {
    layout
        .width
        .checked_mul(layout.height)
        .and_then(|tiles| tiles.checked_mul(2))
        .ok_or(OverworldIoError::SizeOverflow)
}

fn validate_layer(
    number: u8,
    layer: &OverworldLayer,
    layout: OverworldLayersRomLayout,
) -> Result<(), OverworldIoError> {
    if layer.width != layout.width
        || layer.height != layout.height
        || layout.width.checked_mul(layout.height) != Some(layer.tiles.len())
    {
        Err(OverworldIoError::LayerShape {
            layer: number,
            width: layer.width,
            height: layer.height,
            tiles: layer.tiles.len(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> OverworldLayersRomLayout {
        OverworldLayersRomLayout {
            mapper: Mapper::LoRom,
            layer1: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            layer2: LevelPointerTable {
                offset: 0x30,
                entries: 1,
                stride: 3,
            },
            width: 4,
            height: 2,
        }
    }

    fn policy() -> AllocationPolicy {
        AllocationPolicy {
            search: 0x100..0x8000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x20..0x33)],
        }
    }

    fn layers() -> OverworldLayers {
        OverworldLayers {
            layer1: OverworldLayer::new(4, 2, (0_u16..8).collect()).unwrap(),
            layer2: OverworldLayer::new(4, 2, (10_u16..18).collect()).unwrap(),
        }
    }

    fn options() -> OverworldSaveOptions {
        OverworldSaveOptions {
            layer1_allocation: policy(),
            layer2_allocation: policy(),
            previous_layer1: None,
            previous_layer2: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn both_layers_save_load_and_undo_as_one_edit() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_overworld_layers(0, &layers(), layout(), &options())
            .unwrap();
        assert_eq!(
            project.load_overworld_layers(0, layout()).unwrap(),
            layers()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }

    #[test]
    fn invalid_second_layer_shape_prevents_any_mutation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut invalid = layers();
        invalid.layer2.width = 3;
        assert!(matches!(
            project.save_overworld_layers(0, &invalid, layout(), &options()),
            Err(OverworldIoError::LayerShape { layer: 2, .. })
        ));
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn pristine_fixed_layers_load() {
        let expected = layers();
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x30..0x33].copy_from_slice(&[0x20, 0x81, 0x80]);
        bytes[0x100..0x110].copy_from_slice(&expected.layer1.encode_le().unwrap());
        bytes[0x120..0x130].copy_from_slice(&expected.layer2.encode_le().unwrap());
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert_eq!(
            project.load_overworld_layers(0, layout()).unwrap(),
            expected
        );
    }
}
