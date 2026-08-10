mod about_dialog;
mod animation_modes;
mod animation_rate;
mod appearance_editor;
mod appearance_editor_form;
mod application;
mod built_in_runtime_installer;
mod catalog_graphics_compatibility;
mod configuration_loader;
mod copier_header_dialog;
mod custom_object_editor;
mod custom_object_editor_form;
mod custom_sprite_editor;
mod custom_sprite_editor_form;
mod current_level_palette_transfer;
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
mod external_tool_config_editor;
mod external_tool_launcher;
mod external_tools;
mod frontend_ui;
#[path = "rom_graphics_editor/graphics_batch.rs"]
mod graphics_batch;
mod graphics_editor;
mod graphics_insertion_dialog;
mod graphics_migration_dialog;
mod graphics_painter;
mod help_dialog;
mod ips_compat;
mod ips_create_dialog;
mod ips_patch_dialog;
mod layer3_editor;
mod layer3_editor_form;
mod level_access_restriction_dialog;
mod level_deletion_dialog;
mod level_editor;
mod level_editor_advanced;
mod level_editor_auxiliary;
mod level_editor_forms;
mod level_editor_panels;
mod level_editor_render;
mod level_graphics_export;
mod level_outline;
mod level_usage_dialog;
mod live_audio;
mod live_emulator;
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
mod open_level_number_dialog;
mod osc_sidecar_editor;
mod osc_sidecar_editor_form;
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
mod pristine_full_level_render;
mod profile_loader;
mod rats_reclamation_dialog;
mod recovery_store;
mod restore_point_dialog;
mod revision_patch_installer;
mod rom_allocation;
mod rom_boss_sequence_editor;
mod rom_event_editors;
mod rom_exanimation_editor;
mod rom_expanded_settings_editor;
mod rom_expansion_dialog;
mod rom_graphics_editor;
mod rom_legacy_graphics_bypass_editor;
mod rom_level_assets_editor;
mod rom_load;
mod rom_loader;
mod rom_lunar_magic_metadata_editor;
mod rom_map16_editor;
mod rom_mwl_batch_export_dialog;
mod rom_mwl_batch_import_dialog;
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
mod rom_shared_palette_editor;
mod rom_tilemap_editor;
mod rom_title_recording_editor;
mod shortcut_editor;
mod ssc_sidecar_editor;
mod ssc_sidecar_editor_form;
mod startup;
#[cfg(test)]
mod test_support;
mod toolbar_editor;
mod toolbar_graphics_transfer;
mod user_toolbar_images;
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
    let window_size = visual_smoke_window_size().unwrap_or([1100.0, 720.0]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Lunar Magic Rust")
            .with_inner_size(window_size)
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Lunar Magic Rust",
        options,
        Box::new(move |context| {
            context.egui_ctx.set_zoom_factor(1.0);
            let mut app = app;
            app.enable_crash_recovery();
            app.load_persistent_preferences(context.storage);
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}

#[cfg(feature = "visual-smoke")]
fn visual_smoke_window_size() -> Option<[f32; 2]> {
    let width = std::env::var("LM_NATIVE_SCREENSHOT_WIDTH")
        .ok()?
        .parse::<f32>()
        .ok()?;
    let height = std::env::var("LM_NATIVE_SCREENSHOT_HEIGHT")
        .ok()?
        .parse::<f32>()
        .ok()?;
    (width >= 720.0 && height >= 480.0 && width.is_finite() && height.is_finite())
        .then_some([width, height])
}

#[cfg(not(feature = "visual-smoke"))]
const fn visual_smoke_window_size() -> Option<[f32; 2]> {
    None
}
