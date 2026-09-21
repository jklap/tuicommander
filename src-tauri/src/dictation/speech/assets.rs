//! What a speech installation is made of, and how it gets onto the disk.
//!
//! Nothing here is fetched from a name. Every byte this module downloads is
//! named by a URL pinned to an immutable revision and checked against a sha256
//! written in this file, so an upstream that is compromised — or merely
//! re-tagged — produces a refused install rather than a different model. That
//! is the whole reason the catalogue is Rust and not a JSON file beside the
//! binary: a manifest the installer could rewrite is not an allowlist.
//!
//! ```text
//! <config dir>/models/speech/
//!   .staging/<asset id>/          while installing; the engine never looks here
//!   onnxruntime/libonnxruntime.dylib
//!   italian/
//!     bundle.json  tokenizer.model  *.onnx
//!     voices/giovanni.safetensors
//! ```
//!
//! # Two upstreams, for one reason
//!
//! The ONNX graphs come from `KevinAHM/pocket-tts-onnx` on Hugging Face, which
//! is public and serves a sha256 for every file. The voices come from
//! `kyutai/pocket-tts`, which is **gated**: without an accepted licence and a
//! token, not even the file metadata is readable, so an app cannot download
//! them for the user. CC-BY-4.0 allows redistribution with attribution, so the
//! voices — 4.6 MB each, against 125 MB for a language — are re-published on a
//! TUICommander release and fetched from there. The attribution for both is in
//! `THIRD_PARTY_NOTICES.md`.
//!
//! # Why installing is two steps
//!
//! [`stage`] downloads and verifies into `.staging`, holding no lock; [`promote`]
//! renames the staged directory into place. Splitting them is what lets a
//! 125 MB download run while the engine is still speaking, and confines the
//! part that must exclude synthesis to a rename. The caller owns that lock —
//! see [`super::library::SpeechLibrary`].

use std::io::Read;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::SpeechCancel;
use super::pocket::bundles_dir;

/// Where the onnxruntime library is installed, under the speech models
/// directory. Shared with `pocket::resolve_runtime_library`, which looks for it
/// there — the two must never disagree about the name.
pub const RUNTIME_SUBDIR: &str = "onnxruntime";

/// Where a language keeps its voices, relative to the language directory.
/// Shared with `pocket::voice_path` for the same reason.
pub const VOICES_SUBDIR: &str = "voices";

/// Half-installed assets live here, beside the finished ones but out of the
/// way: the engine resolves a language by name, and a name starting with a dot
/// is not a language.
const STAGING_SUBDIR: &str = ".staging";

/// One file that has to arrive, byte for byte.
pub struct Fetch {
    /// Where it lands, relative to the asset's install directory. A relative
    /// path with no `..`, checked by a test rather than at runtime: this is
    /// catalogue data, so a bad entry is a bug in this file, not input.
    pub name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
}

/// How an asset's bytes are carried.
pub enum Payload {
    /// One request per file, each laid down under its own name. Per-file
    /// hashes, which is what Hugging Face publishes.
    Files(&'static [Fetch]),
    /// One upstream archive holding a whole SDK, of which we want exactly one
    /// shared library. The archive's hash is pinned; the member's is not,
    /// because Microsoft does not publish it.
    Library(Fetch),
}

/// What an asset is, which decides where it installs and what it offers.
pub enum Kind {
    /// A language: the graphs, the tokenizer and the voices that go with them.
    ///
    /// Voices ride along with the language rather than being separate assets.
    /// They are 4% of the download, they are useless without the language, and
    /// the engine already expects them inside the language directory.
    Language {
        language: &'static str,
        /// Which voices this language ships, for a caller that has to offer a
        /// choice. Every name here must have a matching entry under
        /// `voices/` in the payload; a test proves it.
        voices: &'static [&'static str],
    },
    /// The onnxruntime shared library for the platform this binary runs on.
    Runtime,
}

pub struct Asset {
    pub id: &'static str,
    pub display_name: &'static str,
    pub kind: Kind,
    pub payload: Payload,
}

/// A file that must be present for an asset to count as installed.
pub struct Installed {
    pub name: &'static str,
    /// `None` when the size cannot be known ahead of time — a member extracted
    /// from an upstream archive, where only the archive's size is pinned.
    pub size_bytes: Option<u64>,
}

/// Whether an asset can be used right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing of it is on disk.
    Absent,
    /// Every file is present and the right size.
    Ready,
    /// Some of it is there. Reported rather than treated as absent so the
    /// difference between "never downloaded" and "a download died" reaches the
    /// user, who can act on them differently.
    Incomplete { missing: Vec<String> },
}

impl Asset {
    /// Where this asset's files live once installed.
    pub fn install_dir(&self) -> PathBuf {
        match self.kind {
            Kind::Language { language, .. } => bundles_dir().join(language),
            Kind::Runtime => bundles_dir().join(RUNTIME_SUBDIR),
        }
    }

    /// Total bytes this asset costs to download. For an archive that is the
    /// archive, not what comes out of it — it is what the progress bar counts.
    pub fn download_bytes(&self) -> u64 {
        match &self.payload {
            Payload::Files(files) => files.iter().map(|f| f.size_bytes).sum(),
            Payload::Library(fetch) => fetch.size_bytes,
        }
    }

    /// What has to be on disk afterwards. Not the same as what is downloaded:
    /// an archive contributes one extracted member under a different name.
    pub fn installed_files(&self) -> Vec<Installed> {
        match &self.payload {
            Payload::Files(files) => files
                .iter()
                .map(|f| Installed {
                    name: f.name,
                    size_bytes: Some(f.size_bytes),
                })
                .collect(),
            Payload::Library(_) => vec![Installed {
                name: library_name(),
                size_bytes: None,
            }],
        }
    }

    /// The voices this asset offers, empty for anything that is not a language.
    pub fn voices(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Language { voices, .. } => voices,
            Kind::Runtime => &[],
        }
    }

    pub fn language(&self) -> Option<&'static str> {
        match self.kind {
            Kind::Language { language, .. } => Some(language),
            Kind::Runtime => None,
        }
    }
}

/// Is this asset usable, and if not, what is missing?
///
/// Sizes rather than hashes. The hash is checked once, while the bytes are
/// arriving, and re-reading 125 MB to answer a status query would make opening
/// a settings panel cost a disk sweep. A file truncated after install still
/// shows the wrong size, which is the failure this has to catch.
pub fn status(asset: &Asset) -> Status {
    let dir = asset.install_dir();
    let mut missing = Vec::new();
    let mut present = 0usize;
    let expected = asset.installed_files();
    for file in &expected {
        let path = dir.join(file.name);
        let ok = match (path.metadata(), file.size_bytes) {
            (Ok(meta), Some(size)) => meta.len() == size,
            (Ok(meta), None) => meta.len() > 0,
            (Err(_), _) => false,
        };
        if ok {
            present += 1;
        } else {
            missing.push(file.name.to_string());
        }
    }
    if missing.is_empty() {
        Status::Ready
    } else if present == 0 {
        Status::Absent
    } else {
        Status::Incomplete { missing }
    }
}

/// Remove an installed asset. Absent is success — the caller asked for it to
/// be gone, and it is.
///
/// This does not stop a loaded engine from using the files it already mapped.
/// Serialising against synthesis is [`super::library::SpeechLibrary`]'s job,
/// for the same reason [`promote`] does not do it either.
pub fn remove(asset: &Asset) -> Result<(), InstallError> {
    let dir = asset.install_dir();
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| InstallError::Disk(format!("{}: {e}", dir.display())))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The caller asked for it to stop. Not a failure.
    Cancelled,
    Network(String),
    Disk(String),
    /// What arrived is not what the catalogue pinned. Separate from
    /// [`InstallError::Network`] because a retry cannot fix it and the message
    /// a user needs is a different one.
    Corrupt {
        file: String,
        expected: String,
        got: String,
    },
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "download cancelled"),
            Self::Network(reason) => write!(f, "download failed: {reason}"),
            Self::Disk(reason) => write!(f, "could not write the download: {reason}"),
            Self::Corrupt {
                file,
                expected,
                got,
            } => write!(
                f,
                "{file} is not the file we pinned (expected {expected}, got {got})"
            ),
        }
    }
}

impl std::error::Error for InstallError {}

fn staging_dir(id: &str) -> PathBuf {
    bundles_dir().join(STAGING_SUBDIR).join(id)
}

/// Download and verify an asset into a staging directory, without touching the
/// installed one.
///
/// Returns the staging path for [`promote`]. On any error — including
/// cancellation — the staging directory is removed before returning, so a
/// failed attempt leaves nothing behind for the next one to trip over.
///
/// `on_progress` is called with bytes fetched so far and the asset's total,
/// often enough for a bar and not once per byte: it fires per chunk of the
/// response stream.
pub async fn stage(
    asset: &Asset,
    cancel: &SpeechCancel,
    on_progress: impl Fn(u64, u64),
) -> Result<PathBuf, InstallError> {
    let staging = staging_dir(asset.id);
    discard(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|e| InstallError::Disk(format!("{}: {e}", staging.display())))?;

    let result = fill(asset, &staging, cancel, on_progress).await;
    if result.is_err() {
        discard(&staging);
    }
    result.map(|()| staging)
}

/// Throw away a staging directory. Failure is ignored on purpose: this runs on
/// the error path, where reporting that the cleanup also failed would replace
/// the real reason with a worse one.
pub fn discard(staging: &Path) {
    let _ = std::fs::remove_dir_all(staging);
}

/// Move a staged directory into place, replacing whatever is installed.
///
/// The caller must already have unloaded any engine reading the old files, and
/// must keep it unloaded until this returns — see
/// [`super::library::SpeechLibrary::install`].
///
/// The old directory is removed before the new one is renamed in, which leaves
/// a window where neither exists. That is the right way round: a crash inside
/// it leaves the asset *absent*, which [`status`] reports honestly and the user
/// can fix by downloading again. The other order can leave a directory that
/// half belongs to each version and passes every existence check.
pub fn promote(asset: &Asset, staging: &Path) -> Result<PathBuf, InstallError> {
    let dir = asset.install_dir();
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| InstallError::Disk(format!("{}: {e}", parent.display())))?;
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| InstallError::Disk(format!("replacing {}: {e}", dir.display())))?;
    }
    std::fs::rename(staging, &dir)
        .map_err(|e| InstallError::Disk(format!("installing {}: {e}", dir.display())))?;
    Ok(dir)
}

async fn fill(
    asset: &Asset,
    staging: &Path,
    cancel: &SpeechCancel,
    on_progress: impl Fn(u64, u64),
) -> Result<(), InstallError> {
    let total = asset.download_bytes();
    let mut done = 0u64;
    match &asset.payload {
        Payload::Files(files) => {
            for file in *files {
                let dest = staging.join(file.name);
                download(file, &dest, cancel, done, total, &on_progress).await?;
                done += file.size_bytes;
            }
        }
        Payload::Library(fetch) => {
            // The archive lands inside staging so a cancelled install has one
            // directory to delete, not a directory and a stray temp file. It
            // keeps its upstream file name because that is where the format
            // comes from: `.zip` on Windows, `.tgz` everywhere else.
            let archive = staging.join(fetch.name);
            download(fetch, &archive, cancel, done, total, &on_progress).await?;
            let member = staging.join(library_name());
            extract_library(archive, member).await?;
        }
    }
    Ok(())
}

async fn download(
    file: &Fetch,
    dest: &Path,
    cancel: &SpeechCancel,
    already: u64,
    total: u64,
    on_progress: &impl Fn(u64, u64),
) -> Result<(), InstallError> {
    if cancel.is_cancelled() {
        return Err(InstallError::Cancelled);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| InstallError::Disk(format!("{}: {e}", parent.display())))?;
    }

    let response = reqwest::Client::new()
        .get(file.url)
        .send()
        .await
        .map_err(|e| InstallError::Network(format!("{}: {e}", file.url)))?;
    if !response.status().is_success() {
        return Err(InstallError::Network(format!(
            "{} returned {}",
            file.url,
            response.status()
        )));
    }

    let mut handle = tokio::fs::File::create(dest)
        .await
        .map_err(|e| InstallError::Disk(format!("{}: {e}", dest.display())))?;
    // Hashed as it streams rather than by re-reading afterwards: the file is up
    // to 125 MB and the bytes are already in hand.
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(InstallError::Cancelled);
        }
        let chunk = chunk.map_err(|e| InstallError::Network(format!("{}: {e}", file.url)))?;
        hasher.update(&chunk);
        handle
            .write_all(&chunk)
            .await
            .map_err(|e| InstallError::Disk(format!("{}: {e}", dest.display())))?;
        written += chunk.len() as u64;
        on_progress(already + written, total);
    }
    handle
        .flush()
        .await
        .map_err(|e| InstallError::Disk(format!("{}: {e}", dest.display())))?;

    let got = hex(&hasher.finalize());
    if got != file.sha256 {
        return Err(InstallError::Corrupt {
            file: file.name.to_string(),
            expected: file.sha256.to_string(),
            got,
        });
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Pull the one shared library out of an upstream SDK archive.
///
/// Blocking work — a 74 MB gunzip — so it runs off the async runtime rather
/// than holding a worker for the duration.
async fn extract_library(archive: PathBuf, dest: PathBuf) -> Result<(), InstallError> {
    tokio::task::spawn_blocking(move || {
        let result = if archive.extension().is_some_and(|e| e == "zip") {
            extract_from_zip(&archive, &dest)
        } else {
            extract_from_tar_gz(&archive, &dest)
        };
        // The archive is several times the size of what we kept; nothing reads
        // it again either way.
        let _ = std::fs::remove_file(&archive);
        result
    })
    .await
    .map_err(|e| InstallError::Disk(format!("extracting the runtime library: {e}")))?
}

/// The library file name this platform's loader looks for. Must agree with
/// `pocket::library_name`, which is what resolves it at load time.
pub const fn library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else {
        "libonnxruntime.so"
    }
}

/// Does this archive member look like the runtime library?
///
/// Verified against all three real 1.23.0 archives. Two rules, and both are
/// load-bearing:
///
/// The trailing dot in the stem keeps `libonnxruntime_providers_shared.so`
/// out — it starts with `libonnxruntime` but not with `libonnxruntime.`.
///
/// The parent directory must be `lib` **itself**, not merely contain one
/// somewhere up the path: `lib/pkgconfig/libonnxruntime.pc` clears every other
/// test here and is a text file. Asking only whether some component is `lib`
/// let it through, which is what the pkgconfig test caught.
///
/// macOS ships both `libonnxruntime.dylib` and `libonnxruntime.1.23.0.dylib`
/// as real files and Linux ships only the versioned one, which is why an exact
/// name is preferred by [`pick_member`] rather than required here.
fn is_library_member(path: &str) -> bool {
    let parts: Vec<&str> = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    let [.., parent, name] = parts.as_slice() else {
        return false;
    };
    if *parent != "lib" {
        return false;
    }
    let stem = if cfg!(target_os = "windows") {
        "onnxruntime"
    } else {
        "libonnxruntime"
    };
    name.starts_with(&format!("{stem}."))
}

/// Choose between several plausible members, or refuse.
///
/// Refusing matters more than choosing: guessing wrong here installs a file
/// that loads and then fails somewhere deep inside onnxruntime.
fn pick_member(candidates: Vec<String>) -> Result<String, InstallError> {
    if let Some(exact) = candidates
        .iter()
        .find(|name| name.rsplit('/').next() == Some(library_name()))
    {
        return Ok(exact.clone());
    }
    match candidates.len() {
        0 => Err(InstallError::Corrupt {
            file: library_name().to_string(),
            expected: "a shared library in lib/".into(),
            got: "no such member".into(),
        }),
        1 => Ok(candidates[0].clone()),
        _ => Err(InstallError::Corrupt {
            file: library_name().to_string(),
            expected: "one shared library in lib/".into(),
            got: candidates.join(", "),
        }),
    }
}

fn extract_from_tar_gz(archive: &Path, dest: &Path) -> Result<(), InstallError> {
    let candidates = {
        let mut tar = open_tar(archive)?;
        let entries = tar
            .entries()
            .map_err(|e| InstallError::Disk(format!("reading the archive: {e}")))?;
        let mut found = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|e| InstallError::Disk(format!("reading the archive: {e}")))?;
            // Symlinks are how Linux ships the unversioned name; following one
            // would write a dangling link instead of a library.
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry
                .path()
                .map_err(|e| InstallError::Disk(format!("reading the archive: {e}")))?
                .to_string_lossy()
                .into_owned();
            if is_library_member(&path) {
                found.push(path);
            }
        }
        found
    };
    let wanted = pick_member(candidates)?;

    // A second pass rather than keeping the first reader: `tar::Entries`
    // borrows the archive, and a gzip stream cannot seek back.
    let mut tar = open_tar(archive)?;
    let entries = tar
        .entries()
        .map_err(|e| InstallError::Disk(format!("reading the archive: {e}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|e| InstallError::Disk(format!("reading the archive: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| InstallError::Disk(format!("reading the archive: {e}")))?
            .to_string_lossy()
            .into_owned();
        if path == wanted {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| InstallError::Disk(format!("reading {wanted}: {e}")))?;
            return write_library(dest, &bytes);
        }
    }
    Err(InstallError::Disk(format!("{wanted} vanished mid-archive")))
}

type TarGz = tar::Archive<flate2::read::GzDecoder<std::fs::File>>;

fn open_tar(archive: &Path) -> Result<TarGz, InstallError> {
    let handle = std::fs::File::open(archive)
        .map_err(|e| InstallError::Disk(format!("{}: {e}", archive.display())))?;
    Ok(tar::Archive::new(flate2::read::GzDecoder::new(handle)))
}

fn extract_from_zip(archive: &Path, dest: &Path) -> Result<(), InstallError> {
    let handle = std::fs::File::open(archive)
        .map_err(|e| InstallError::Disk(format!("{}: {e}", archive.display())))?;
    let mut zip = zip::ZipArchive::new(handle)
        .map_err(|e| InstallError::Disk(format!("reading the archive: {e}")))?;
    let candidates: Vec<String> = (0..zip.len())
        .filter_map(|index| {
            let entry = zip.by_index(index).ok()?;
            let name = entry.name().to_string();
            (entry.is_file() && is_library_member(&name)).then_some(name)
        })
        .collect();
    let wanted = pick_member(candidates)?;

    let mut entry = zip
        .by_name(&wanted)
        .map_err(|e| InstallError::Disk(format!("reading {wanted}: {e}")))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| InstallError::Disk(format!("reading {wanted}: {e}")))?;
    write_library(dest, &bytes)
}

fn write_library(dest: &Path, bytes: &[u8]) -> Result<(), InstallError> {
    std::fs::write(dest, bytes)
        .map_err(|e| InstallError::Disk(format!("{}: {e}", dest.display())))?;
    // The archive marks it executable and the loader on unix needs that back;
    // `fs::write` creates with 0644.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| InstallError::Disk(format!("{}: {e}", dest.display())))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The catalogue
// ---------------------------------------------------------------------------

/// The Hugging Face revision every language is pinned to. A commit, not a
/// branch: `main` moves, and a model that changes under a hash we already
/// checked is exactly what the hash is there to catch.
const POCKET_ONNX_REVISION: &str = "58a6d00cf13d239b6748cb0769f35c580a8f606c";

/// Where the voices are re-published, for the gating reason in the module
/// documentation. A tag, which on GitHub is as immutable as we make it — the
/// sha256 below is what actually holds it still.
const VOICES_TAG: &str = "speech-voices-v1";

macro_rules! pocket_onnx_url {
    ($language:literal, $file:literal) => {
        concat!(
            "https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/",
            "58a6d00cf13d239b6748cb0769f35c580a8f606c",
            "/onnx/",
            $language,
            "/",
            $file
        )
    };
}

macro_rules! voice_url {
    ($file:literal) => {
        concat!(
            "https://github.com/sstraus/tuicommander/releases/download/",
            "speech-voices-v1",
            "/",
            $file
        )
    };
}

/// Italian, quantised.
///
/// `text_conditioner` has no int8 export upstream, which is why it is the one
/// full-precision graph here; the engine's `quantised()` picks the `_int8`
/// name for the other three when it is present, so what the catalogue
/// downloads is what the engine will open.
static ITALIAN_FILES: &[Fetch] = &[
    Fetch {
        name: "bundle.json",
        url: pocket_onnx_url!("italian", "bundle.json"),
        sha256: "c779c25fd836c9b85a3fc570474774777176757bd6bec0b5bffbbe599644a9f9",
        size_bytes: 24_365,
    },
    Fetch {
        name: "tokenizer.model",
        url: pocket_onnx_url!("italian", "tokenizer.model"),
        sha256: "6583b974a11b90e14d8a4c8e9c43f06c3861b9ede6e5023a4c27ab5a3a7d4c39",
        size_bytes: 60_078,
    },
    Fetch {
        name: "text_conditioner.onnx",
        url: pocket_onnx_url!("italian", "text_conditioner.onnx"),
        sha256: "692369f5ac340006fa44252155da77fe6c8a60a859848297777e0caea534068e",
        size_bytes: 16_388_344,
    },
    Fetch {
        name: "flow_lm_main_int8.onnx",
        url: pocket_onnx_url!("italian", "flow_lm_main_int8.onnx"),
        sha256: "f43ce4d823471095a7bd6d9dcfcceb46145ea96b0f2b85b7d668f15816965055",
        size_bytes: 76_341_079,
    },
    Fetch {
        name: "flow_lm_flow_int8.onnx",
        url: pocket_onnx_url!("italian", "flow_lm_flow_int8.onnx"),
        sha256: "21b2bec2f9ae4323fc545a0c7ffb274bdfa925a699fd304ed03aba53e4ca9129",
        size_bytes: 9_962_530,
    },
    Fetch {
        name: "mimi_decoder_int8.onnx",
        url: pocket_onnx_url!("italian", "mimi_decoder_int8.onnx"),
        sha256: "f120bc5cddca9514c511f128786f5d9e6e6893b067faae5e30f5b2bd5643aa03",
        size_bytes: 22_684_077,
    },
    Fetch {
        name: "voices/giovanni.safetensors",
        url: voice_url!("italian-giovanni.safetensors"),
        sha256: "e2b47d9d63ad18cfb2e98591b714a0464456a739b1b4316b6427fcf9d5ea671f",
        size_bytes: 4_621_552,
    },
];

/// onnxruntime, pinned at 1.23.0.
///
/// Not the newest release on purpose: 1.24 onwards dropped the macOS Intel and
/// universal2 builds, and TUICommander still ships for Intel Macs. `ort`
/// 2.0.0-rc.13 needs API version 17 or later and 1.23 provides 23, so nothing
/// is given up by staying here. Digests are Microsoft's own, as published on
/// the release, and were re-checked against the downloaded archives.
static ONNXRUNTIME: Asset = Asset {
    id: "onnxruntime",
    display_name: "ONNX Runtime",
    kind: Kind::Runtime,
    payload: Payload::Library(runtime_fetch()),
};

const fn runtime_fetch() -> Fetch {
    if cfg!(target_os = "macos") {
        Fetch {
            name: "onnxruntime-osx-universal2-1.23.0.tgz",
            url: concat!(
                "https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/",
                "onnxruntime-osx-universal2-1.23.0.tgz"
            ),
            sha256: "5e4365fb4a05aef353f6232b9a1848f37e608c421c9227e9224572205c0cfc08",
            size_bytes: 42_631_433,
        }
    } else if cfg!(target_os = "windows") {
        Fetch {
            name: "onnxruntime-win-x64-1.23.0.zip",
            url: concat!(
                "https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/",
                "onnxruntime-win-x64-1.23.0.zip"
            ),
            sha256: "72c23470310ec79a7d42d27fe9d257e6c98540c73fa5a1db1f67f538c6c16f2f",
            size_bytes: 78_078_377,
        }
    } else if cfg!(target_arch = "aarch64") {
        Fetch {
            name: "onnxruntime-linux-aarch64-1.23.0.tgz",
            url: concat!(
                "https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/",
                "onnxruntime-linux-aarch64-1.23.0.tgz"
            ),
            sha256: "0b9f47d140411d938e47915824d8daaa424df95a88b5f1fc843172a75168f7a0",
            size_bytes: 7_216_713,
        }
    } else {
        Fetch {
            name: "onnxruntime-linux-x64-1.23.0.tgz",
            url: concat!(
                "https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/",
                "onnxruntime-linux-x64-1.23.0.tgz"
            ),
            sha256: "b6deea7f2e22c10c043019f294a0ea4d2a6c0ae52a009c34847640db75ec5580",
            size_bytes: 8_257_032,
        }
    }
}

static ITALIAN: Asset = Asset {
    id: "italian",
    display_name: "Italian",
    kind: Kind::Language {
        language: "italian",
        voices: &["giovanni"],
    },
    payload: Payload::Files(ITALIAN_FILES),
};

/// Everything a user may install, and nothing else. A download request names
/// an entry here; an id that is not in this list is refused rather than
/// resolved, which is what makes this an allowlist instead of a hint.
pub static CATALOGUE: &[&Asset] = &[&ONNXRUNTIME, &ITALIAN];

/// Look an asset up by id.
pub fn find(id: &str) -> Option<&'static Asset> {
    CATALOGUE.iter().copied().find(|asset| asset.id == id)
}

/// The language assets, for a caller offering a choice of voice.
pub fn languages() -> impl Iterator<Item = &'static Asset> {
    CATALOGUE.iter().copied().filter(|a| a.language().is_some())
}

/// The runtime library asset. Every language needs it, so it is worth naming
/// rather than finding by id at each call site.
pub fn runtime() -> &'static Asset {
    &ONNXRUNTIME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_id_is_unique() {
        // `find` returns the first match, so a duplicate id would make one of
        // the two assets unreachable and undeletable rather than noisy.
        let mut ids: Vec<&str> = CATALOGUE.iter().map(|a| a.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate asset id in the catalogue");
    }

    #[test]
    fn every_pinned_url_is_https_and_carries_an_immutable_revision() {
        // A `main` or a `latest` here would silently un-pin the asset: the
        // sha256 would then reject the new file rather than the manifest
        // selecting the old one, and the user would see corruption.
        for asset in CATALOGUE {
            let urls: Vec<&str> = match &asset.payload {
                Payload::Files(files) => files.iter().map(|f| f.url).collect(),
                Payload::Library(fetch) => vec![fetch.url],
            };
            for url in urls {
                assert!(url.starts_with("https://"), "{url} is not https");
                assert!(
                    url.contains(POCKET_ONNX_REVISION)
                        || url.contains(VOICES_TAG)
                        || url.contains("/releases/download/v1.23.0/"),
                    "{url} is not pinned to a revision this file declares"
                );
            }
        }
    }

    #[test]
    fn every_pinned_hash_is_a_sha256_and_every_size_is_real() {
        for asset in CATALOGUE {
            let fetches: Vec<&Fetch> = match &asset.payload {
                Payload::Files(files) => files.iter().collect(),
                Payload::Library(fetch) => vec![fetch],
            };
            for fetch in fetches {
                assert_eq!(fetch.sha256.len(), 64, "{} is not a sha256", fetch.name);
                assert!(
                    fetch
                        .sha256
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                    "{} is not lowercase hex",
                    fetch.name
                );
                assert!(fetch.size_bytes > 0, "{} has no size", fetch.name);
            }
        }
    }

    #[test]
    fn no_asset_can_write_outside_its_own_directory() {
        // The names are catalogue data rather than input, so this is a guard on
        // us: a `../` here would let an install overwrite another language, or
        // the whisper models beside it.
        for asset in CATALOGUE {
            if let Payload::Files(files) = &asset.payload {
                for file in *files {
                    let path = Path::new(file.name);
                    assert!(path.is_relative(), "{} is absolute", file.name);
                    assert!(
                        path.components()
                            .all(|c| matches!(c, std::path::Component::Normal(_))),
                        "{} escapes the install directory",
                        file.name
                    );
                }
            }
        }
    }

    #[test]
    fn every_voice_a_language_offers_is_a_file_it_downloads() {
        // The two are declared separately — one for the UI, one for the
        // downloader — so nothing but this stops them drifting apart and
        // offering a voice that never arrives.
        for asset in languages() {
            let Payload::Files(files) = &asset.payload else {
                panic!("{} is a language with no files", asset.id);
            };
            for voice in asset.voices() {
                let wanted = format!("{VOICES_SUBDIR}/{voice}.safetensors");
                assert!(
                    files.iter().any(|f| f.name == wanted),
                    "{} offers the voice {voice} but never downloads {wanted}",
                    asset.id
                );
            }
        }
    }

    #[test]
    fn a_language_downloads_everything_the_engine_opens() {
        // `Engine::open` reads these four graphs, `Engine::prepare` the
        // manifest and the tokenizer. A language missing one of them installs
        // cleanly and then fails on the first spoken word.
        for asset in languages() {
            let Payload::Files(files) = &asset.payload else {
                continue;
            };
            let names: Vec<&str> = files.iter().map(|f| f.name).collect();
            assert!(names.contains(&"bundle.json"), "{}", asset.id);
            assert!(names.contains(&"tokenizer.model"), "{}", asset.id);
            assert!(names.contains(&"text_conditioner.onnx"), "{}", asset.id);
            for stem in ["flow_lm_main", "flow_lm_flow", "mimi_decoder"] {
                assert!(
                    names
                        .iter()
                        .any(|n| *n == format!("{stem}.onnx") || *n == format!("{stem}_int8.onnx")),
                    "{} downloads no {stem} graph",
                    asset.id
                );
            }
        }
    }

    #[test]
    fn the_runtime_installs_under_the_name_the_loader_looks_for() {
        // `pocket::resolve_runtime_library` joins `onnxruntime` and the
        // platform library name. Installing under any other name produces a
        // download that succeeds and a runtime that is still "not found".
        let installed = runtime().installed_files();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].name, library_name());
        assert!(runtime().install_dir().ends_with(RUNTIME_SUBDIR));
    }

    #[test]
    fn the_real_archive_layouts_resolve_to_exactly_one_library() {
        // Paths taken from `tar tvzf` / `unzip -l` on the three 1.23.0
        // archives. The providers library and the cmake files are the traps.
        let macos = vec![
            "./onnxruntime-osx-universal2-1.23.0/lib/libonnxruntime.1.23.0.dylib",
            "./onnxruntime-osx-universal2-1.23.0/lib/libonnxruntime.dylib",
            "./onnxruntime-osx-universal2-1.23.0/lib/pkgconfig/libonnxruntime.pc",
        ];
        let linux = vec![
            "onnxruntime-linux-x64-1.23.0/lib/libonnxruntime.so.1.23.0",
            "onnxruntime-linux-x64-1.23.0/lib/libonnxruntime_providers_shared.so",
            "onnxruntime-linux-x64-1.23.0/lib/pkgconfig/libonnxruntime.pc",
        ];
        let windows = vec![
            "onnxruntime-win-x64-1.23.0/lib/onnxruntime.dll",
            "onnxruntime-win-x64-1.23.0/lib/onnxruntime_providers_shared.dll",
        ];
        let listing = if cfg!(target_os = "macos") {
            macos
        } else if cfg!(target_os = "windows") {
            windows
        } else {
            linux
        };
        let candidates: Vec<String> = listing
            .into_iter()
            .filter(|p| is_library_member(p))
            .map(str::to_string)
            .collect();
        let picked = pick_member(candidates).expect("one library");
        assert!(
            picked.ends_with(&format!("/{}", library_name()))
                || picked.contains("libonnxruntime.so.1.23.0"),
            "picked {picked}"
        );
    }

    #[test]
    fn a_pkgconfig_file_is_not_mistaken_for_the_library() {
        // `libonnxruntime.pc` starts with `libonnxruntime.` and sits under a
        // path containing `lib`. Only "its parent is `lib`" excludes it — the
        // first version of this rule asked whether *any* component was `lib`
        // and would have installed a 330-byte text file as the runtime.
        assert!(!is_library_member("x/lib/pkgconfig/libonnxruntime.pc"));
        assert!(!is_library_member(
            "x/lib/cmake/onnxruntime/onnxruntime.dll"
        ));
    }

    #[test]
    fn the_providers_library_is_not_mistaken_for_the_runtime() {
        // It ships beside the real one in all three archives, and loading it
        // gives a library that resolves and then has no API in it.
        assert!(!is_library_member(
            "x/lib/libonnxruntime_providers_shared.so"
        ));
        assert!(!is_library_member("x/lib/onnxruntime_providers_shared.dll"));
    }

    #[test]
    fn two_libraries_and_no_exact_name_is_refused_rather_than_guessed() {
        let error = pick_member(vec![
            "a/lib/libonnxruntime.so.1.23.0".into(),
            "a/lib/libonnxruntime.so.1.24.0".into(),
        ])
        .unwrap_err();
        assert!(matches!(error, InstallError::Corrupt { .. }), "{error:?}");
    }

    #[test]
    fn an_archive_with_no_library_is_refused() {
        let error = pick_member(Vec::new()).unwrap_err();
        assert!(matches!(error, InstallError::Corrupt { .. }), "{error:?}");
    }

    #[test]
    fn an_unknown_id_resolves_to_nothing() {
        // The allowlist property: an id the catalogue does not carry must not
        // become a path or a URL anywhere downstream.
        assert!(find("italian").is_some());
        assert!(find("../whisper").is_none());
        assert!(find("french").is_none());
    }

    /// Point the config directory at a temporary one and lay down an asset's
    /// files at their declared sizes.
    ///
    /// Sparse — `set_len` rather than writing 125 MB of zeros. [`status`] reads
    /// metadata, so the files it sees are indistinguishable from real ones and
    /// the test costs no disk.
    fn installed(asset: &Asset) -> (tempfile::TempDir, impl Drop) {
        let root = tempfile::tempdir().unwrap();
        let guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        let dir = asset.install_dir();
        for file in asset.installed_files() {
            let path = dir.join(file.name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let handle = std::fs::File::create(&path).unwrap();
            handle.set_len(file.size_bytes.unwrap_or(1)).unwrap();
        }
        (root, guard)
    }

    #[test]
    fn a_language_with_nothing_on_disk_is_absent() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        assert_eq!(status(&ITALIAN), Status::Absent);
    }

    #[test]
    fn a_language_with_every_file_at_its_declared_size_is_ready() {
        let (_root, _guard) = installed(&ITALIAN);
        assert_eq!(status(&ITALIAN), Status::Ready);
    }

    #[test]
    fn a_truncated_file_leaves_the_language_incomplete_rather_than_ready() {
        // The criterion this exists for: an install that died partway must
        // never report ready. The hash catches it while downloading; the size
        // is what catches it afterwards, which is the case a hash cannot see
        // without re-reading 125 MB on every status query.
        let (_root, _guard) = installed(&ITALIAN);
        let truncated = ITALIAN.install_dir().join("flow_lm_main_int8.onnx");
        std::fs::File::create(&truncated)
            .unwrap()
            .set_len(17)
            .unwrap();

        let Status::Incomplete { missing } = status(&ITALIAN) else {
            panic!("a truncated graph still reported ready");
        };
        assert_eq!(missing, vec!["flow_lm_main_int8.onnx".to_string()]);
    }

    #[test]
    fn a_language_missing_only_its_voice_is_incomplete_not_absent() {
        // Absent offers a download; incomplete says what is missing. A user
        // whose voice file failed should not be told the whole 125 MB is gone.
        let (_root, _guard) = installed(&ITALIAN);
        std::fs::remove_file(
            ITALIAN
                .install_dir()
                .join(VOICES_SUBDIR)
                .join("giovanni.safetensors"),
        )
        .unwrap();
        assert!(matches!(status(&ITALIAN), Status::Incomplete { .. }));
    }

    #[test]
    fn promoting_replaces_the_old_directory_whole_rather_than_merging_into_it() {
        // A merge would leave a file from the previous version beside the new
        // ones — the graphs would load and disagree about the tokenizer.
        let (_root, _guard) = installed(&ITALIAN);
        let stale = ITALIAN.install_dir().join("flow_lm_main.onnx");
        std::fs::write(&stale, b"a graph from the version before").unwrap();

        let staging = staging_dir(ITALIAN.id);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("bundle.json"), b"{}").unwrap();

        let installed_dir = promote(&ITALIAN, &staging).unwrap();
        assert_eq!(installed_dir, ITALIAN.install_dir());
        assert!(!stale.exists(), "the old graph survived the replacement");
        assert!(installed_dir.join("bundle.json").exists());
        assert!(!staging.exists(), "the staging directory was left behind");
    }

    #[test]
    fn removing_an_asset_that_was_never_installed_is_success() {
        // Delete is idempotent on purpose: the caller asked for it to be gone
        // and it is, so a second click must not raise an error.
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        assert_eq!(remove(&ITALIAN), Ok(()));
    }

    #[test]
    fn removing_an_installed_language_leaves_nothing_behind() {
        let (_root, _guard) = installed(&ITALIAN);
        assert_eq!(status(&ITALIAN), Status::Ready);
        remove(&ITALIAN).unwrap();
        assert_eq!(status(&ITALIAN), Status::Absent);
    }

    #[test]
    fn staging_cannot_be_mistaken_for_a_language() {
        // `PocketSpeech::for_language` joins the speech directory with a name.
        // A staging directory named like a language would be found by it while
        // still half-written, which is why the name starts with a dot and the
        // id never appears at the top level.
        assert!(STAGING_SUBDIR.starts_with('.'));
        let staging = staging_dir("italian");
        assert_ne!(staging, ITALIAN.install_dir());
        assert!(staging.starts_with(bundles_dir().join(STAGING_SUBDIR)));
    }

    // -----------------------------------------------------------------
    // The download path, against a real server on loopback
    // -----------------------------------------------------------------
    //
    // A local axum server rather than a mocked client: the thing worth
    // proving is what `stage` does with bytes arriving over HTTP — where it
    // writes them, when it hashes them, and what it leaves behind when it
    // refuses them. A fake `reqwest` would assert against the fake.

    const BODY: &[u8] = b"a bundle, for the purposes of argument";
    /// sha256 of `BODY`.
    const BODY_SHA: &str = "1d93e7f8caaeaa212f2364b1dc5a9e85ad27cc33cb85bbafc361d7ea08eb0fb1";

    /// Serve `BODY` at one path, and hand back the URL. The server stops when
    /// the returned handle is dropped at the end of the test.
    async fn serve() -> (String, tokio::task::JoinHandle<()>) {
        let app = axum::Router::new().route("/f", axum::routing::get(|| async { BODY }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{address}/f"), server)
    }

    /// An asset pointing at the local server. Leaked because the catalogue is
    /// `&'static` by design — a test port cannot be known at compile time, and
    /// a leak that lives as long as the test process is the honest way to say
    /// so rather than loosening the lifetime everywhere else.
    fn test_asset(url: String, sha256: &'static str) -> Asset {
        Asset {
            id: "test-language",
            display_name: "Test",
            kind: Kind::Language {
                language: "test-language",
                voices: &[],
            },
            payload: Payload::Files(Box::leak(Box::new([Fetch {
                name: "bundle.json",
                url: Box::leak(url.into_boxed_str()),
                sha256,
                size_bytes: BODY.len() as u64,
            }]))),
        }
    }

    #[tokio::test]
    async fn a_verified_download_is_staged_first_and_installed_only_on_promote() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        let (url, _server) = serve().await;
        let asset = test_asset(url, BODY_SHA);

        let staging = stage(&asset, &SpeechCancel::new(), |_, _| {})
            .await
            .unwrap();
        assert_eq!(std::fs::read(staging.join("bundle.json")).unwrap(), BODY);
        // The whole point of the split: nothing is live yet.
        assert_eq!(status(&asset), Status::Absent);

        promote(&asset, &staging).unwrap();
        assert_eq!(status(&asset), Status::Ready);
    }

    #[tokio::test]
    async fn a_file_whose_hash_is_wrong_never_reaches_the_install_directory() {
        // The criterion in one test: a corrupt download must not appear ready,
        // and must not leave a staging directory for the next attempt to find
        // half-full and skip.
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        let (url, _server) = serve().await;
        // A hash the body cannot possibly have, which is what a substituted
        // file upstream would look like from here.
        let asset = test_asset(
            url,
            "0000000000000000000000000000000000000000000000000000000000000000",
        );

        let error = stage(&asset, &SpeechCancel::new(), |_, _| {})
            .await
            .unwrap_err();
        let InstallError::Corrupt { file, got, .. } = error else {
            panic!("a wrong hash was not reported as corruption: {error:?}");
        };
        assert_eq!(file, "bundle.json");
        assert_eq!(got, BODY_SHA, "the hash reported is not the one computed");
        assert_eq!(status(&asset), Status::Absent);
        assert!(
            !staging_dir(asset.id).exists(),
            "staging survived a refusal"
        );
    }

    #[tokio::test]
    async fn a_download_cancelled_before_it_starts_leaves_nothing_behind() {
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        let (url, _server) = serve().await;
        let asset = test_asset(url, BODY_SHA);

        let cancel = SpeechCancel::new();
        cancel.cancel();
        assert_eq!(
            stage(&asset, &cancel, |_, _| {}).await.unwrap_err(),
            InstallError::Cancelled
        );
        assert_eq!(status(&asset), Status::Absent);
        assert!(!staging_dir(asset.id).exists());
    }

    #[tokio::test]
    async fn a_url_that_answers_404_is_a_network_error_not_a_corrupt_file() {
        // The two need different messages: one says try again, the other says
        // this download will never work. A 404 body hashed and compared would
        // have reported corruption.
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        let (url, _server) = serve().await;
        let asset = test_asset(format!("{url}-does-not-exist"), BODY_SHA);

        let error = stage(&asset, &SpeechCancel::new(), |_, _| {})
            .await
            .unwrap_err();
        assert!(matches!(error, InstallError::Network(_)), "{error:?}");
    }

    #[tokio::test]
    async fn progress_reaches_the_declared_total() {
        // A bar fed a total it never reaches sits at 99% forever. The total is
        // the catalogue's, so this also checks the declared size is the real
        // one for at least one asset shape.
        let root = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(root.path().to_path_buf());
        let (url, _server) = serve().await;
        let asset = test_asset(url, BODY_SHA);

        let seen = std::sync::Mutex::new(Vec::new());
        stage(&asset, &SpeechCancel::new(), |done, total| {
            seen.lock().unwrap().push((done, total));
        })
        .await
        .unwrap();

        let seen = seen.into_inner().unwrap();
        assert!(!seen.is_empty(), "no progress was reported at all");
        let (done, total) = *seen.last().unwrap();
        assert_eq!(done, total);
        assert_eq!(total, asset.download_bytes());
    }

    #[test]
    fn hex_is_lowercase_and_zero_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }

    #[test]
    fn the_progress_total_is_what_gets_downloaded_not_what_gets_installed() {
        // For a language the two agree; for the runtime the archive is 42 MB
        // and the library inside it is 74 MB. A bar fed the installed size
        // would stop at 57%.
        assert_eq!(
            ITALIAN.download_bytes(),
            ITALIAN_FILES.iter().map(|f| f.size_bytes).sum::<u64>()
        );
        let Payload::Library(fetch) = &runtime().payload else {
            panic!("the runtime is not an archive");
        };
        assert_eq!(runtime().download_bytes(), fetch.size_bytes);
    }
}
