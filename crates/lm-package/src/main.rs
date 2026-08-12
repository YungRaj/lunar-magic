#![forbid(unsafe_code)]

use flate2::{Compression, GzBuilder, write::GzEncoder};
use std::{
    env, fmt,
    fmt::Write as _,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;
const TAR_BLOCK: usize = 512;

fn main() {
    if let Err(error) = run(env::args_os().skip(1)) {
        eprintln!("portable packaging failed: {error}");
        std::process::exit(1);
    }
}

fn run(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> Result<(), PackageError> {
    let options = Options::parse(arguments)?;
    let result = package(&options)?;
    println!("{}", result.archive.display());
    println!("{}", result.checksum.display());
    println!("{}", result.update_manifest.display());
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Options {
    bin_dir: PathBuf,
    output_dir: PathBuf,
    project_root: PathBuf,
    target: String,
    version: String,
}

impl Options {
    fn parse(
        arguments: impl IntoIterator<Item = std::ffi::OsString>,
    ) -> Result<Self, PackageError> {
        let mut bin_dir = None;
        let mut output_dir = None;
        let mut project_root = None;
        let mut target = None;
        let mut version = None;
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            let argument = argument
                .into_string()
                .map_err(|_| PackageError::Arguments("option names must be UTF-8".into()))?;
            let value = match argument.as_str() {
                "--bin-dir" | "--output-dir" | "--project-root" | "--target" | "--version" => {
                    arguments.next().ok_or_else(|| {
                        PackageError::Arguments(format!("{argument} requires a value"))
                    })?
                }
                "--help" | "-h" => {
                    return Err(PackageError::Arguments(
                        "usage: lm-package --bin-dir DIR --output-dir DIR --target TRIPLE [--version VERSION] [--project-root DIR]".into(),
                    ));
                }
                _ => {
                    return Err(PackageError::Arguments(format!(
                        "unknown packaging option {argument}"
                    )));
                }
            };
            match argument.as_str() {
                "--bin-dir" => set_once(&mut bin_dir, PathBuf::from(value), &argument)?,
                "--output-dir" => set_once(&mut output_dir, PathBuf::from(value), &argument)?,
                "--project-root" => set_once(&mut project_root, PathBuf::from(value), &argument)?,
                "--target" => set_once(
                    &mut target,
                    value
                        .into_string()
                        .map_err(|_| PackageError::Arguments("target must be UTF-8".into()))?,
                    &argument,
                )?,
                "--version" => set_once(
                    &mut version,
                    value
                        .into_string()
                        .map_err(|_| PackageError::Arguments("version must be UTF-8".into()))?,
                    &argument,
                )?,
                _ => unreachable!(),
            }
        }
        let current = env::current_dir().map_err(PackageError::Io)?;
        let options = Self {
            bin_dir: bin_dir
                .ok_or_else(|| PackageError::Arguments("--bin-dir is required".into()))?,
            output_dir: output_dir
                .ok_or_else(|| PackageError::Arguments("--output-dir is required".into()))?,
            project_root: project_root.unwrap_or(current),
            target: target.ok_or_else(|| PackageError::Arguments("--target is required".into()))?,
            version: version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
        };
        validate_component("target", &options.target)?;
        validate_component("version", &options.version)?;
        Ok(options)
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<(), PackageError> {
    if slot.replace(value).is_some() {
        return Err(PackageError::Arguments(format!(
            "{name} was supplied twice"
        )));
    }
    Ok(())
}

fn validate_component(label: &str, value: &str) -> Result<(), PackageError> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(PackageError::Arguments(format!(
            "{label} must contain only ASCII letters, digits, '.', '_', or '-'"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackageResult {
    archive: PathBuf,
    checksum: PathBuf,
    update_manifest: PathBuf,
}

fn package(options: &Options) -> Result<PackageResult, PackageError> {
    let executable_suffix = if options.target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let bundle_name = format!("lunar-magic-rust-{}-{}", options.version, options.target);
    let mut files = vec![
        input_file(
            &options
                .bin_dir
                .join(format!("lm-launcher{executable_suffix}")),
            format!("lm-launcher{executable_suffix}"),
            0o755,
        )?,
        input_file(
            &options
                .bin_dir
                .join(format!("lm-native{executable_suffix}")),
            format!("lm-native{executable_suffix}"),
            0o755,
        )?,
        input_file(
            &options.bin_dir.join(format!("lm-cli{executable_suffix}")),
            format!("lm-cli{executable_suffix}"),
            0o755,
        )?,
        input_file(
            &options
                .bin_dir
                .join(format!("lm-libretro{executable_suffix}")),
            format!("lm-libretro{executable_suffix}"),
            0o755,
        )?,
        input_file(
            &options.project_root.join("README.md"),
            "README.md".into(),
            0o644,
        )?,
        input_file(
            &options.project_root.join("LICENSE-MIT"),
            "LICENSE-MIT".into(),
            0o644,
        )?,
        input_file(
            &options.project_root.join("LICENSE-APACHE"),
            "LICENSE-APACHE".into(),
            0o644,
        )?,
    ];
    files.sort_by(|left, right| left.name.cmp(&right.name));
    let manifest = release_manifest(&options.version, &options.target, &files);
    files.push(InputFile {
        name: "RELEASE-MANIFEST.txt".into(),
        bytes: manifest.into_bytes(),
        mode: 0o644,
    });
    files.sort_by(|left, right| left.name.cmp(&right.name));

    let tar = encode_tar(&bundle_name, &files)?;
    let mut encoder: GzEncoder<Vec<u8>> = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::best());
    encoder.write_all(&tar).map_err(PackageError::Io)?;
    let archive_bytes = encoder.finish().map_err(PackageError::Io)?;
    let archive_name = format!("{bundle_name}.tar.gz");
    let checksum_name = format!("{archive_name}.sha256");
    let checksum_bytes = format!("{}  {archive_name}\n", hex_sha256(&archive_bytes)).into_bytes();
    let update_name = format!("{archive_name}.update");
    let update_bytes = format!(
        "LMUPDATE1\nversion {}\ntarget {}\narchive {archive_name}\nlength {}\nsha256 {}\n",
        options.version,
        options.target,
        archive_bytes.len(),
        hex_sha256(&archive_bytes)
    )
    .into_bytes();

    fs::create_dir_all(&options.output_dir).map_err(PackageError::Io)?;
    let archive = options.output_dir.join(&archive_name);
    let checksum = options.output_dir.join(&checksum_name);
    let update_manifest = options.output_dir.join(&update_name);
    create_new(&archive, &archive_bytes)?;
    if let Err(error) = create_new(&checksum, &checksum_bytes) {
        let _cleanup = fs::remove_file(&archive);
        return Err(error);
    }
    if let Err(error) = create_new(&update_manifest, &update_bytes) {
        let _archive_cleanup = fs::remove_file(&archive);
        let _checksum_cleanup = fs::remove_file(&checksum);
        return Err(error);
    }
    Ok(PackageResult {
        archive,
        checksum,
        update_manifest,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InputFile {
    name: String,
    bytes: Vec<u8>,
    mode: u32,
}

fn input_file(path: &Path, name: String, mode: u32) -> Result<InputFile, PackageError> {
    let metadata = fs::metadata(path).map_err(|source| PackageError::Input {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES {
        return Err(PackageError::InvalidInput(path.to_owned()));
    }
    let bytes = fs::read(path).map_err(|source| PackageError::Input {
        path: path.to_owned(),
        source,
    })?;
    Ok(InputFile { name, bytes, mode })
}

fn release_manifest(version: &str, target: &str, files: &[InputFile]) -> String {
    let mut manifest = format!("LMRELEASE1\nversion {version}\ntarget {target}\n");
    for file in files {
        writeln!(
            manifest,
            "file {} {} {}",
            file.name,
            file.bytes.len(),
            hex_sha256(&file.bytes)
        )
        .expect("writing to a String cannot fail");
    }
    manifest
}

fn encode_tar(prefix: &str, files: &[InputFile]) -> Result<Vec<u8>, PackageError> {
    let mut output = Vec::new();
    for file in files {
        let path = format!("{prefix}/{}", file.name);
        if path.len() > 100 || !path.is_ascii() {
            return Err(PackageError::ArchivePath(path));
        }
        let mut header = [0_u8; TAR_BLOCK];
        header[..path.len()].copy_from_slice(path.as_bytes());
        write_octal(&mut header[100..108], u64::from(file.mode))?;
        write_octal(&mut header[108..116], 0)?;
        write_octal(&mut header[116..124], 0)?;
        write_octal(
            &mut header[124..136],
            u64::try_from(file.bytes.len()).map_err(|_| PackageError::TarValue(u64::MAX))?,
        )?;
        write_octal(&mut header[136..148], 0)?;
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum = header.iter().map(|byte| u64::from(*byte)).sum();
        write_checksum(&mut header[148..156], checksum)?;
        output.extend_from_slice(&header);
        output.extend_from_slice(&file.bytes);
        let padding = (TAR_BLOCK - file.bytes.len() % TAR_BLOCK) % TAR_BLOCK;
        output.resize(output.len() + padding, 0);
    }
    output.resize(output.len() + TAR_BLOCK * 2, 0);
    Ok(output)
}

fn write_octal(field: &mut [u8], value: u64) -> Result<(), PackageError> {
    let digits = format!("{value:o}");
    if digits.len() + 1 > field.len() {
        return Err(PackageError::TarValue(value));
    }
    field.fill(b'0');
    let start = field.len() - digits.len() - 1;
    field[start..start + digits.len()].copy_from_slice(digits.as_bytes());
    field[field.len() - 1] = 0;
    Ok(())
}

fn write_checksum(field: &mut [u8], value: u64) -> Result<(), PackageError> {
    let digits = format!("{value:06o}");
    if digits.len() != 6 || field.len() != 8 {
        return Err(PackageError::TarValue(value));
    }
    field[..6].copy_from_slice(digits.as_bytes());
    field[6] = 0;
    field[7] = b' ';
    Ok(())
}

fn create_new(path: &Path, bytes: &[u8]) -> Result<(), PackageError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| PackageError::Output {
            path: path.to_owned(),
            source,
        })?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    if let Err(source) = result {
        drop(file);
        let _cleanup = fs::remove_file(path);
        return Err(PackageError::Output {
            path: path.to_owned(),
            source,
        });
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut state = [
        0x6a09_e667_u32,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let bit_len = u64::try_from(bytes.len())
        .expect("SHA-256 input length fits the supported 64-bit targets")
        .wrapping_mul(8);
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in padded.chunks_exact(64) {
        sha256_compress(&mut state, chunk);
    }
    let mut output = String::with_capacity(64);
    for word in state {
        write!(output, "{word:08x}").expect("writing to a String cannot fail");
    }
    output
}

#[allow(clippy::many_single_char_names, clippy::unreadable_literal)]
fn sha256_compress(state: &mut [u32; 8], chunk: &[u8]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut schedule = [0_u32; 64];
    for (index, word) in schedule[..16].iter_mut().enumerate() {
        let at = index * 4;
        *word = u32::from_be_bytes(chunk[at..at + 4].try_into().expect("four-byte word"));
    }
    for index in 16..64 {
        let s0 = schedule[index - 15].rotate_right(7)
            ^ schedule[index - 15].rotate_right(18)
            ^ (schedule[index - 15] >> 3);
        let s1 = schedule[index - 2].rotate_right(17)
            ^ schedule[index - 2].rotate_right(19)
            ^ (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for index in 0..64 {
        let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(sum1)
            .wrapping_add(choice)
            .wrapping_add(K[index])
            .wrapping_add(schedule[index]);
        let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = sum0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (target, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *target = target.wrapping_add(value);
    }
}

#[derive(Debug)]
enum PackageError {
    Arguments(String),
    Io(io::Error),
    Input { path: PathBuf, source: io::Error },
    InvalidInput(PathBuf),
    Output { path: PathBuf, source: io::Error },
    ArchivePath(String),
    TarValue(u64),
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arguments(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Input { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::InvalidInput(path) => write!(
                formatter,
                "{} is not a bounded regular file",
                path.display()
            ),
            Self::Output { path, source } => {
                write!(formatter, "cannot create {}: {source}", path.display())
            }
            Self::ArchivePath(path) => write!(formatter, "archive path is not portable: {path}"),
            Self::TarValue(value) => write!(formatter, "value {value} does not fit the tar header"),
        }
    }
}

impl std::error::Error for PackageError {}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Read;

    #[test]
    fn sha256_matches_published_vectors() {
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn portable_bundle_is_deterministic_complete_and_create_new() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let bin = root.path().join("bin");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&bin).unwrap();
        for (path, bytes) in [
            (project.join("README.md"), b"readme".as_slice()),
            (project.join("LICENSE-MIT"), b"mit"),
            (project.join("LICENSE-APACHE"), b"apache"),
            (bin.join("lm-launcher"), b"launcher"),
            (bin.join("lm-native"), b"native"),
            (bin.join("lm-cli"), b"cli"),
            (bin.join("lm-libretro"), b"libretro"),
        ] {
            fs::write(path, bytes).unwrap();
        }
        let options = |output: PathBuf| Options {
            bin_dir: bin.clone(),
            output_dir: output,
            project_root: project.clone(),
            target: "x86_64-test-none".into(),
            version: "1.2.3".into(),
        };
        let first = package(&options(root.path().join("first"))).unwrap();
        let second = package(&options(root.path().join("second"))).unwrap();
        assert_eq!(
            fs::read(&first.archive).unwrap(),
            fs::read(&second.archive).unwrap()
        );
        assert_eq!(
            fs::read(&first.checksum).unwrap(),
            fs::read(&second.checksum).unwrap()
        );
        assert_eq!(
            fs::read(&first.update_manifest).unwrap(),
            fs::read(&second.update_manifest).unwrap()
        );
        let update =
            lm_update::UpdateManifest::decode(&fs::read(&first.update_manifest).unwrap()).unwrap();
        update
            .verify_archive(
                "1.2.2".parse().unwrap(),
                "x86_64-test-none",
                &fs::read(&first.archive).unwrap(),
            )
            .unwrap();
        assert!(matches!(
            package(&options(root.path().join("first"))),
            Err(PackageError::Output { .. })
        ));
        let collision_dir = root.path().join("checksum-collision");
        fs::create_dir(&collision_dir).unwrap();
        let archive_name = "lunar-magic-rust-1.2.3-x86_64-test-none.tar.gz";
        let checksum_path = collision_dir.join(format!("{archive_name}.sha256"));
        fs::write(&checksum_path, b"retain existing checksum").unwrap();
        assert!(matches!(
            package(&options(collision_dir.clone())),
            Err(PackageError::Output { .. })
        ));
        assert!(!collision_dir.join(archive_name).exists());
        assert_eq!(
            fs::read(checksum_path).unwrap(),
            b"retain existing checksum"
        );
        let update_collision_dir = root.path().join("update-collision");
        fs::create_dir(&update_collision_dir).unwrap();
        let update_path = update_collision_dir.join(format!("{archive_name}.update"));
        fs::write(&update_path, b"retain existing update manifest").unwrap();
        assert!(matches!(
            package(&options(update_collision_dir.clone())),
            Err(PackageError::Output { .. })
        ));
        assert!(!update_collision_dir.join(archive_name).exists());
        assert!(
            !update_collision_dir
                .join(format!("{archive_name}.sha256"))
                .exists()
        );
        assert_eq!(
            fs::read(update_path).unwrap(),
            b"retain existing update manifest"
        );

        let mut tar = Vec::new();
        GzDecoder::new(fs::File::open(first.archive).unwrap())
            .read_to_end(&mut tar)
            .unwrap();
        let entries = tar_entries(&tar);
        assert_eq!(
            entries
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "lunar-magic-rust-1.2.3-x86_64-test-none/LICENSE-APACHE",
                "lunar-magic-rust-1.2.3-x86_64-test-none/LICENSE-MIT",
                "lunar-magic-rust-1.2.3-x86_64-test-none/README.md",
                "lunar-magic-rust-1.2.3-x86_64-test-none/RELEASE-MANIFEST.txt",
                "lunar-magic-rust-1.2.3-x86_64-test-none/lm-cli",
                "lunar-magic-rust-1.2.3-x86_64-test-none/lm-launcher",
                "lunar-magic-rust-1.2.3-x86_64-test-none/lm-libretro",
                "lunar-magic-rust-1.2.3-x86_64-test-none/lm-native",
            ]
        );
        let manifest = std::str::from_utf8(&entries[3].1).unwrap();
        assert!(manifest.starts_with("LMRELEASE1\nversion 1.2.3\ntarget x86_64-test-none\n"));
        assert!(manifest.contains(&format!("file lm-native 6 {}", hex_sha256(b"native"))));
        assert!(manifest.contains(&format!("file lm-libretro 8 {}", hex_sha256(b"libretro"))));
    }

    #[test]
    fn arguments_reject_unsafe_names_duplicates_and_missing_values() {
        for arguments in [
            vec!["--target", "../escape"],
            vec!["--target", "ok", "--target", "again"],
            vec!["--version", "1", "--version", "2"],
            vec!["--bin-dir"],
            vec!["--unknown", "x"],
        ] {
            assert!(Options::parse(arguments.into_iter().map(Into::into)).is_err());
        }
    }

    fn tar_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut entries = Vec::new();
        let mut offset = 0;
        while bytes
            .get(offset..offset + TAR_BLOCK)
            .is_some_and(|header| header.iter().any(|b| *b != 0))
        {
            let header = &bytes[offset..offset + TAR_BLOCK];
            let name_end = header[..100]
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(100);
            let name = std::str::from_utf8(&header[..name_end]).unwrap().to_owned();
            let size_text = std::str::from_utf8(&header[124..136])
                .unwrap()
                .trim_end_matches('\0')
                .trim_start_matches('0');
            let size = if size_text.is_empty() {
                0
            } else {
                usize::from_str_radix(size_text, 8).unwrap()
            };
            offset += TAR_BLOCK;
            entries.push((name, bytes[offset..offset + size].to_vec()));
            offset += size.div_ceil(TAR_BLOCK) * TAR_BLOCK;
        }
        entries
    }
}
