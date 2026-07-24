mod animation_modes;
mod appearance_editor;
mod appearance_editor_form;
mod application;
mod built_in_runtime_installer;
mod configuration_loader;
mod copier_header_dialog;
mod custom_object_editor;
mod custom_object_editor_form;
mod custom_sprite_editor;
mod custom_sprite_editor_form;
mod dialogs;
mod document_loader;
mod document_persistence;
mod dsc_sidecar_editor;
mod dsc_sidecar_editor_form;
mod editor_view;
mod effects;
mod exanimation_editor;
mod exanimation_form;
mod expanded_settings_editor;
mod expanded_settings_editor_form;
mod external_tool_launcher;
mod external_tools;
mod frontend_ui;
mod graphics_editor;
mod graphics_migration_dialog;
mod graphics_painter;
mod ips_patch_dialog;
mod layer3_editor;
mod layer3_editor_form;
mod level_editor;
mod level_editor_advanced;
mod level_editor_auxiliary;
mod level_editor_forms;
mod level_editor_panels;
mod level_editor_render;
mod map16_editor;
mod map16_editor_render;
mod map16_set_editor;
mod map16_subtile_form;
mod metadata_editor;
mod metadata_editor_forms;
mod mwl_editor;
mod mwl_editor_form;
mod native_clipboard;
mod native_level_assets_editor;
mod native_level_assets_panels;
mod native_level_document_editor;
mod native_level_document_form;
mod native_map16_sidecar_editor;
mod native_map16_sidecar_form;
mod native_render;
mod overworld_appearance_editor;
mod overworld_appearance_editor_forms;
mod overworld_editor;
mod overworld_editor_animation;
mod overworld_editor_forms;
mod overworld_editor_palette;
mod overworld_editor_records;
mod overworld_editor_render;
mod palette_editor;
mod path_editor;
mod path_editor_forms;
mod persistence_worker;
mod profile_loader;
mod rats_reclamation_dialog;
mod revision_patch_installer;
mod rom_allocation;
mod rom_boss_sequence_editor;
mod rom_event_editors;
mod rom_exanimation_editor;
mod rom_expanded_settings_editor;
mod rom_expansion_dialog;
mod rom_graphics_editor;
mod rom_level_assets_editor;
mod rom_load;
mod rom_loader;
mod rom_lunar_magic_metadata_editor;
mod rom_map16_editor;
mod rom_navigation_link_editors;
mod rom_overworld_editor;
mod rom_overworld_event_number_editor;
mod rom_overworld_level_name_editor;
mod rom_overworld_message_editor;
mod rom_overworld_player_start_editor;
mod rom_overworld_settings_editor;
mod rom_overworld_special_event_editor;
mod rom_ownership;
mod rom_palette_editor;
mod rom_secondary_exit_editor;
mod rom_tilemap_editor;
mod rom_title_recording_editor;
mod startup;
mod vanilla_graphics_editor;
mod vanilla_level_editor;
mod vanilla_map16_preview;

use application::NativeApplication;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let startup_options = lm_app::startup_args::StartupOptions::parse(std::env::args_os().skip(1))?;
    if startup_options.help {
        println!("{}", startup::HELP);
        return Ok(());
    }
    let app = NativeApplication::from_startup(startup::initialize(startup_options));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Lunar Magic Rust")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Lunar Magic Rust",
        options,
        Box::new(move |context| {
            context.egui_ctx.set_zoom_factor(1.0);
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}
