use rusqlite::params;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::schema::initialize;
use super::{
    indexed_file_from_row, open_default_connection, DbError, DuplicateGroup, DuplicateScanResult,
    IndexedFile,
};

// Max Hamming distance (out of 256 dHash bits) for two photos to count as look-alikes.
// The 16x16 dHash is 4x finer than the old 8x8, so a proportionally tighter threshold
// (~9.4%) keeps re-saved/resized near-dups together while no longer chaining unrelated
// text-heavy screenshots into giant false clusters.
const SIMILAR_HAMMING_THRESHOLD_256: u32 = 24;
// Photos flatter than this 8-bit luma range carry no usable gradient signal (solid colour
// thumbnails, blank scans); they cluster trivially, so they're excluded from the finder.
const PERCEPTUAL_DETAIL_MIN_RANGE: u8 = 10;
// Two photos only count as look-alikes if their (quantised log2) aspect ratios are within
// this many units — separates portrait screenshots from landscape photos without hard cuts.
const ASPECT_TOLERANCE: u8 = 8;
// Bytes of the stored perceptual signature: [aspect_q, 32-byte dHash].
pub(super) const PERCEPTUAL_SIGNATURE_LEN: usize = 33;
// Extensions the `image` crate can decode directly (so we hash the original, not a thumbnail).
pub(super) const IMAGE_DECODABLE_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff"];

/// Compute a 256-bit difference hash (dHash) plus an aspect byte for an image, returned as a
/// 33-byte signature: `[aspect_q, 32-byte dHash]`. Returns `None` if the file can't be decoded
/// (HEIC originals — callers pass the JPEG thumbnail for those) or if the image is too flat to
/// carry a usable gradient signal (solid-colour thumbnails, blank scans).
pub(super) fn perceptual_hash_256_of(path: &Path) -> Option<[u8; PERCEPTUAL_SIGNATURE_LEN]> {
    let image = image::open(path).ok()?;
    let (width, height) = (image.width(), image.height());
    // 17x16 grayscale → compare each pixel to its right neighbour → 16x16 = 256 bits.
    let small = image
        .resize_exact(17, 16, image::imageops::FilterType::Triangle)
        .to_luma8();

    // Detail guard: reject near-flat images whose luma range is below the floor.
    let (mut min, mut max) = (255u8, 0u8);
    for pixel in small.pixels() {
        min = min.min(pixel[0]);
        max = max.max(pixel[0]);
    }
    if max.saturating_sub(min) < PERCEPTUAL_DETAIL_MIN_RANGE {
        return None;
    }

    let mut signature = [0u8; PERCEPTUAL_SIGNATURE_LEN];
    signature[0] = aspect_quantized(width, height);
    let mut bit = 0usize;
    for y in 0..16u32 {
        for x in 0..16u32 {
            if small.get_pixel(x, y)[0] < small.get_pixel(x + 1, y)[0] {
                signature[1 + bit / 8] |= 1u8 << (bit % 8);
            }
            bit += 1;
        }
    }
    Some(signature)
}

/// Quantise an image's aspect ratio onto a 0..=254 scale via log2(width/height), so the finder
/// can keep look-alikes of similar shape together while separating portrait screenshots from
/// landscape photos. 127 ≈ square; lower = taller, higher = wider.
fn aspect_quantized(width: u32, height: u32) -> u8 {
    if width == 0 || height == 0 {
        return 127;
    }
    let ratio = (width as f32 / height as f32).log2().clamp(-2.0, 2.0);
    (((ratio + 2.0) / 4.0) * 254.0).round() as u8
}

/// Whether two perceptual signatures count as look-alikes: similar aspect ratio AND a 256-bit
/// dHash Hamming distance within the threshold. Signatures shorter than expected never match.
fn signatures_are_similar(a: &[u8], b: &[u8]) -> bool {
    if a.len() != PERCEPTUAL_SIGNATURE_LEN || b.len() != PERCEPTUAL_SIGNATURE_LEN {
        return false;
    }
    if a[0].abs_diff(b[0]) > ASPECT_TOLERANCE {
        return false;
    }
    let distance: u32 = a[1..]
        .iter()
        .zip(b[1..].iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum();
    distance <= SIMILAR_HAMMING_THRESHOLD_256
}

/// Pick a path the `image` crate can decode: the JPEG thumbnail when present (HEIC,
/// RAW, …), otherwise the original if its extension is directly decodable.
fn decodable_image_path(file: &IndexedFile) -> Option<PathBuf> {
    if let Some(thumb) = &file.thumbnail_path {
        return Some(PathBuf::from(thumb));
    }
    let ext = file.extension.as_deref()?.to_ascii_lowercase();
    if IMAGE_DECODABLE_EXTENSIONS.contains(&ext.as_str()) {
        Some(PathBuf::from(&file.absolute_path))
    } else {
        None
    }
}

/// Union-find root with path halving.
fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

pub fn find_default_similar_photos<F: FnMut(usize, usize)>(
    mut progress: F,
) -> Result<DuplicateScanResult, DbError> {
    let connection = open_default_connection()?;
    find_similar_photos(&connection, &mut progress)
}

/// Group photos that look alike (not just byte-identical) by perceptual signature.
///
/// Reads the 256-bit signature cached in `files.perceptual_hash_v2` (populated at index time);
/// for libraries indexed before the v9 schema it decodes the photo once and back-fills the
/// column so later scans are instant. Clusters by aspect-aware Hamming distance with a
/// union-find. Groups come back largest-file-first so the UI keeps the highest-quality copy by
/// default. Reuses `DuplicateScanResult` for shape.
pub fn find_similar_photos(
    connection: &Connection,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<DuplicateScanResult, DbError> {
    initialize(connection)?;

    // Perceptual signature BLOB (column 9) carried alongside each photo.
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<(IndexedFile, Option<Vec<u8>>)> {
        Ok((indexed_file_from_row(row)?, row.get(9)?))
    };
    let mut statement = connection.prepare(
        "SELECT id, absolute_path, relative_path, category, source, extension, size_bytes, modified_unix, thumbnail_path, perceptual_hash_v2
         FROM files WHERE category = 'photo'",
    )?;
    let rows = statement.query_map([], map_row)?;
    let mut photos: Vec<(IndexedFile, Option<Vec<u8>>)> = Vec::new();
    for row in rows {
        photos.push(row?);
    }

    // Reuse the cached signature; lazily back-fill any photo missing one (pre-v9 libraries).
    // Skip photos with no decodable source or too flat to carry a signal (detail guard).
    let total = photos.len();
    let mut hashed: Vec<(IndexedFile, Vec<u8>)> = Vec::new();
    for (done, (file, stored)) in photos.into_iter().enumerate() {
        progress(done + 1, total);
        let signature = match stored {
            Some(bytes) if bytes.len() == PERCEPTUAL_SIGNATURE_LEN => bytes,
            _ => {
                let Some(path) = decodable_image_path(&file) else {
                    continue;
                };
                let Some(signature) = perceptual_hash_256_of(&path) else {
                    continue;
                };
                let _ = connection.execute(
                    "UPDATE files SET perceptual_hash_v2 = ?1 WHERE id = ?2",
                    params![&signature[..], file.id],
                );
                signature.to_vec()
            }
        };
        hashed.push((file, signature));
    }

    // Cluster by aspect-aware Hamming distance with a union-find over the hashed photos.
    let n = hashed.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if signatures_are_similar(&hashed[i].1, &hashed[j].1) {
                let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    let mut clusters: HashMap<usize, Vec<IndexedFile>> = HashMap::new();
    for (index, (file, _)) in hashed.into_iter().enumerate() {
        let root = uf_find(&mut parent, index);
        clusters.entry(root).or_default().push(file);
    }

    let mut groups: Vec<DuplicateGroup> = clusters
        .into_values()
        .filter(|files| files.len() >= 2)
        .map(|mut files| {
            // Largest first → the UI keeps the highest-quality copy (files[0]).
            files.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes).then(a.id.cmp(&b.id)));
            let largest = files[0].size_bytes;
            let total_bytes: u64 = files.iter().map(|file| file.size_bytes).sum();
            DuplicateGroup {
                hash: format!("sim-{}", files[0].id),
                size_bytes: largest,
                reclaimable_bytes: total_bytes - largest,
                files,
            }
        })
        .collect();
    groups.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));

    let total_groups = groups.len();
    let reclaimable_bytes = groups.iter().map(|group| group.reclaimable_bytes).sum();
    Ok(DuplicateScanResult {
        groups,
        total_groups,
        reclaimable_bytes,
        scanned_candidates: total,
    })
}
