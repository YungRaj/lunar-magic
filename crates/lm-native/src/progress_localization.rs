use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog};

const OPENING_TITLE_ID: &str = "lm-progress-opening-title";
const READING_FORMAT_ID: &str = "lm-progress-reading-format";
const SAVING_TITLE_ID: &str = "lm-progress-saving-title";
const WRITING_FORMAT_ID: &str = "lm-progress-writing-format";

pub(crate) fn install(context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
    for (id, key) in [
        (OPENING_TITLE_ID, ExtendedUiTextKey::ProgressOpeningTitle),
        (READING_FORMAT_ID, ExtendedUiTextKey::ProgressReadingFormat),
        (SAVING_TITLE_ID, ExtendedUiTextKey::ProgressSavingTitle),
        (WRITING_FORMAT_ID, ExtendedUiTextKey::ProgressWritingFormat),
    ] {
        let value = catalog.map_or_else(
            || key.english().to_owned(),
            |catalog| catalog.extended_text(key).to_owned(),
        );
        context.data_mut(|data| data.insert_temp(egui::Id::new(id), value));
    }
}

pub(crate) fn opening_title(context: &egui::Context) -> String {
    value(
        context,
        OPENING_TITLE_ID,
        ExtendedUiTextKey::ProgressOpeningTitle,
    )
}

pub(crate) fn reading(context: &egui::Context, description: &str) -> String {
    value(
        context,
        READING_FORMAT_ID,
        ExtendedUiTextKey::ProgressReadingFormat,
    )
    .replace("{description}", description)
}

pub(crate) fn saving_title(context: &egui::Context) -> String {
    value(
        context,
        SAVING_TITLE_ID,
        ExtendedUiTextKey::ProgressSavingTitle,
    )
}

pub(crate) fn writing(context: &egui::Context, target: &str) -> String {
    value(
        context,
        WRITING_FORMAT_ID,
        ExtendedUiTextKey::ProgressWritingFormat,
    )
    .replace("{target}", target)
}

fn value(context: &egui::Context, id: &str, key: ExtendedUiTextKey) -> String {
    context
        .data(|data| data.get_temp::<String>(egui::Id::new(id)))
        .unwrap_or_else(|| key.english().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_text_defaults_to_english_and_replaces_dynamic_data() {
        let context = egui::Context::default();
        assert_eq!(opening_title(&context), "Opening");
        assert_eq!(reading(&context, "palette file"), "Reading palette file");
        assert_eq!(saving_title(&context), "Saving");
        assert_eq!(writing(&context, "output.mwl"), "Writing output.mwl");
    }

    #[test]
    fn progress_text_uses_installed_extension_translations() {
        let context = egui::Context::default();
        let catalog = LocalizationCatalog::new(
            "zz-test",
            lm_app::UiTextKey::ALL.map(|key| (key, key.english().to_owned())),
        )
        .unwrap()
        .with_extended_ui_texts([
            (ExtendedUiTextKey::ProgressOpeningTitle, "Abrir".into()),
            (
                ExtendedUiTextKey::ProgressReadingFormat,
                "Leer {description}".into(),
            ),
            (ExtendedUiTextKey::ProgressSavingTitle, "Guardar".into()),
            (
                ExtendedUiTextKey::ProgressWritingFormat,
                "Escribir {target}".into(),
            ),
        ])
        .unwrap();
        install(&context, Some(&catalog));
        assert_eq!(opening_title(&context), "Abrir");
        assert_eq!(reading(&context, "paleta"), "Leer paleta");
        assert_eq!(saving_title(&context), "Guardar");
        assert_eq!(writing(&context, "salida.mwl"), "Escribir salida.mwl");
    }

    #[test]
    fn shared_workers_have_no_fixed_english_progress_captions() {
        let loader = include_str!("document_loader.rs");
        let persistence = include_str!("persistence_worker.rs");
        for (source, literals) in [
            (loader, ["Window::new(\"Opening\")", "format!(\"Reading {}"]),
            (
                persistence,
                ["Window::new(\"Saving\")", "format!(\"Writing {}"],
            ),
        ] {
            for literal in literals {
                assert!(
                    !source.contains(literal),
                    "fixed progress caption: {literal}"
                );
            }
        }
    }
}
