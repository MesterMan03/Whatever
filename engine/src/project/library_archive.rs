use anyhow::{Context, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
};

const MAX_FILES: usize = 10000;
const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;

fn portable(path: &str) -> anyhow::Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() < 1024
            && path.split('/').count() <= 64
            && !path.contains('\\'),
        "invalid library path '{path}'"
    );
    for part in path.split('/') {
        ensure!(
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || ":<>\"|?*".contains(c)),
            "nonportable library path '{path}'"
        );
        let name = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !["CON", "PRN", "AUX", "NUL"].contains(&name.as_str())
                && !(name.len() == 4
                    && (name.starts_with("COM") || name.starts_with("LPT"))
                    && name.as_bytes()[3].is_ascii_digit()),
            "reserved library filename"
        );
        ensure!(
            ![".git", ".whatever", "node_modules"].contains(&part),
            "managed directory cannot be bundled"
        );
    }
    Ok(())
}

pub(super) fn validate_files(files: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<()> {
    ensure!(files.len() <= MAX_FILES, "library exceeds file limit");
    let names = files
        .keys()
        .map(|p| p.to_lowercase())
        .collect::<BTreeSet<_>>();
    ensure!(
        names.len() == files.len(),
        "case-insensitive library path collision"
    );
    let mut total = 0;
    for (path, bytes) in files {
        portable(path)?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE,
            "library file exceeds size limit"
        );
        total += bytes.len() as u64;
        ensure!(total <= MAX_TOTAL, "library exceeds unpacked size limit");
        let mut prefix = String::new();
        for part in path
            .split('/')
            .take(path.split('/').count().saturating_sub(1))
        {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            ensure!(
                !names.contains(&prefix.to_lowercase()),
                "library path is both a file and directory"
            );
        }
    }
    Ok(())
}

pub(super) fn directory_files(root: &Path) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &Path,
        dir: &Path,
        files: &mut BTreeMap<String, Vec<u8>>,
        total: &mut u64,
    ) -> anyhow::Result<()> {
        for item in std::fs::read_dir(dir)? {
            let item = item?;
            let kind = item.file_type()?;
            ensure!(
                !kind.is_symlink(),
                "libraries cannot include symbolic links"
            );
            let path = item.path();
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            portable(&relative)?;
            if kind.is_dir() {
                walk(root, &path, files, total)?;
            } else {
                ensure!(
                    kind.is_file() && files.len() < MAX_FILES,
                    "invalid library file or file limit exceeded"
                );
                let size = item.metadata()?.len();
                ensure!(
                    size <= MAX_FILE && *total + size <= MAX_TOTAL,
                    "library size limit exceeded"
                );
                let mut bytes = Vec::new();
                std::fs::File::open(path)?
                    .take(MAX_FILE + 1)
                    .read_to_end(&mut bytes)?;
                *total += bytes.len() as u64;
                files.insert(relative, bytes);
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files, &mut 0)?;
    validate_files(&files)?;
    Ok(files)
}

pub(super) fn read_archive(path: &Path) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    ensure!(
        archive.len() <= MAX_FILES,
        "library exceeds archive entry limit"
    );
    let mut files = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut total = 0;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_owned();
        portable(name.trim_end_matches('/'))?;
        ensure!(
            names.insert(name.trim_end_matches('/').to_lowercase()),
            "duplicate archive path"
        );
        ensure!(
            file.unix_mode().is_none_or(|mode| mode & 0o170000 == 0
                || mode & 0o170000 == 0o100000
                || mode & 0o170000 == 0o040000),
            "archive links and special files are forbidden"
        );
        if file.is_dir() {
            continue;
        }
        ensure!(
            file.size() <= MAX_FILE && total + file.size() <= MAX_TOTAL,
            "library unpacked size limit exceeded"
        );
        let mut bytes = Vec::new();
        file.by_ref()
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .context("read library file and verify CRC")?;
        ensure!(
            bytes.len() as u64 == file.size(),
            "incorrect archive file size"
        );
        total += bytes.len() as u64;
        files.insert(name, bytes);
    }
    validate_files(&files)?;
    Ok(files)
}

pub(super) fn write_archive(files: &BTreeMap<String, Vec<u8>>, path: &Path) -> anyhow::Result<()> {
    validate_files(files)?;
    let output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut zip = zip::ZipWriter::new(output);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, bytes) in files {
        zip.start_file(name, options)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?.sync_all()?;
    Ok(())
}
