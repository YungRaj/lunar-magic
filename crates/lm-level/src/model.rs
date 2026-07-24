use crate::{
    Entrance, Layer3Data, LevelHeader, Map16Tile, ObjectStream, ScreenExit, SecondaryExit,
    SpriteStream,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LayerData {
    pub objects: ObjectStream,
    pub raw_tilemap: Vec<u16>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Level {
    pub number: u16,
    pub header: LevelHeader,
    pub layer1: LayerData,
    pub layer2: LayerData,
    pub layer3: Option<Layer3Data>,
    pub sprites: SpriteStream,
    pub entrances: Vec<Entrance>,
    pub screen_exits: Vec<ScreenExit>,
    pub secondary_exits: Vec<SecondaryExit>,
    pub map16_overrides: Vec<(u32, Map16Tile)>,
    pub unknown_extensions: Vec<Vec<u8>>,
}
