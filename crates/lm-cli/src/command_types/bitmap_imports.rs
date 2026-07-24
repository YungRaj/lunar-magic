use std::path::PathBuf;

macro_rules! map16_import_command {
    ($name:ident, $input:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            pub $input: PathBuf,
            pub palette: PathBuf,
            pub palette_access: PathBuf,
            pub graphics: PathBuf,
            pub occupancy: PathBuf,
            pub palette_row: u8,
            pub acts_like: u16,
            pub source_page: u16,
            pub palette_output: PathBuf,
            pub graphics_output: PathBuf,
            pub occupancy_output: PathBuf,
            pub page_output: PathBuf,
        }
    };
}

map16_import_command!(RgbMap16ImportCommand, rgb);
map16_import_command!(RgbaMap16ImportCommand, rgba);
map16_import_command!(PngMap16ImportCommand, png);
