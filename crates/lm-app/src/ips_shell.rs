use crate::{editor_shell::read_bounded_utf8, file_persistence, read_bounded_bytes, spec_text};
use lm_rom::{MAX_IPS_IMAGE_LEN, MAX_IPS_PATCH_LEN, apply_ips, create_ips};
use std::path::{Path, PathBuf};

struct IpsApplySpec {
    source: PathBuf,
    patch: PathBuf,
    output: PathBuf,
}

struct IpsCreateSpec {
    before: PathBuf,
    after: PathBuf,
    output: PathBuf,
}

pub(crate) fn apply(spec_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let spec = parse_apply_spec(spec_path)?;
    require_distinct(&spec.output, &[&spec.source, &spec.patch])?;
    let source = read_bounded_bytes(&spec.source, MAX_IPS_IMAGE_LEN, "IPS source image")?;
    let patch = read_bounded_bytes(&spec.patch, MAX_IPS_PATCH_LEN, "IPS patch")?;
    let output = apply_ips(&source, &patch)?;
    file_persistence::write_new(&spec.output, &output)?;
    println!("IPS patch applied: {}", spec.output.display());
    Ok(())
}

pub(crate) fn create(spec_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let spec = parse_create_spec(spec_path)?;
    require_distinct(&spec.output, &[&spec.before, &spec.after])?;
    let before = read_bounded_bytes(&spec.before, MAX_IPS_IMAGE_LEN, "IPS before image")?;
    let after = read_bounded_bytes(&spec.after, MAX_IPS_IMAGE_LEN, "IPS after image")?;
    let patch = create_ips(&before, &after)?;
    file_persistence::write_new(&spec.output, &patch)?;
    println!("IPS patch created: {}", spec.output.display());
    Ok(())
}

fn parse_apply_spec(spec_path: &Path) -> Result<IpsApplySpec, Box<dyn std::error::Error>> {
    let mut fields = fields(spec_path, "LMIPSA01")?;
    let base = spec_path.parent().unwrap_or_else(|| Path::new("."));
    let spec = IpsApplySpec {
        source: spec_text::take_path(&mut fields, "source", base)?,
        patch: spec_text::take_path(&mut fields, "patch", base)?,
        output: spec_text::take_path(&mut fields, "output", base)?,
    };
    spec_text::reject_unknown(&fields)?;
    Ok(spec)
}

fn parse_create_spec(spec_path: &Path) -> Result<IpsCreateSpec, Box<dyn std::error::Error>> {
    let mut fields = fields(spec_path, "LMIPSC01")?;
    let base = spec_path.parent().unwrap_or_else(|| Path::new("."));
    let spec = IpsCreateSpec {
        before: spec_text::take_path(&mut fields, "before", base)?,
        after: spec_text::take_path(&mut fields, "after", base)?,
        output: spec_text::take_path(&mut fields, "output", base)?,
    };
    spec_text::reject_unknown(&fields)?;
    Ok(spec)
}

fn fields(path: &Path, magic: &str) -> Result<spec_text::Fields, Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, spec_text::MAX_SPEC_BYTES, "IPS specification")?;
    Ok(spec_text::parse_fields(&text, magic)?)
}

fn require_distinct(output: &Path, inputs: &[&Path]) -> Result<(), &'static str> {
    if inputs.contains(&output) {
        Err("IPS output must differ from every input")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn unicode_relative_specs_create_and_apply_without_replacement() {
        let directory = std::env::temp_dir().join(format!(
            "lm-app-ips-日本語-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("before image.smc"), b"abcdef").unwrap();
        fs::write(directory.join("after image.smc"), b"abZZZZef-more").unwrap();
        let create_spec = directory.join("create.txt");
        fs::write(
            &create_spec,
            "LMIPSC01\nbefore before image.smc\nafter after image.smc\noutput change.ips\n",
        )
        .unwrap();
        create(&create_spec).unwrap();
        let apply_spec = directory.join("apply.txt");
        fs::write(
            &apply_spec,
            "LMIPSA01\nsource before image.smc\npatch change.ips\noutput result image.smc\n",
        )
        .unwrap();
        apply(&apply_spec).unwrap();
        assert_eq!(
            fs::read(directory.join("result image.smc")).unwrap(),
            fs::read(directory.join("after image.smc")).unwrap()
        );
        assert!(apply(&apply_spec).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
