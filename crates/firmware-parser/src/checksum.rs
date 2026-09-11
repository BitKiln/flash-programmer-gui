use crc32fast::Hasher as Crc32Hasher;
use md5::{Digest, Md5};
use sha2::Sha256;

use crate::metadata::{ChecksumSummary, MemorySegment};

/// Computes CRC32, MD5, and SHA-256 for a contiguous byte slice.
pub fn compute_checksums(data: &[u8]) -> (u32, ChecksumSummary) {
    let mut crc_hasher = Crc32Hasher::new();
    crc_hasher.update(data);
    let crc = crc_hasher.finalize();

    let mut md5_hasher = Md5::new();
    md5_hasher.update(data);
    let md5_result = md5_hasher.finalize();
    let md5_str = format!("{:x}", md5_result);

    let mut sha_hasher = Sha256::new();
    sha_hasher.update(data);
    let sha_result = sha_hasher.finalize();
    let sha256_str = format!("{:x}", sha_result);

    let summary = ChecksumSummary {
        crc32: format!("0x{:08X}", crc),
        md5: md5_str,
        sha256: sha256_str,
    };

    (crc, summary)
}

/// Computes the canonical concatenated checksums across multiple memory segments.
///
/// Segments are processed in sequential order, feeding each segment's payload
/// directly into the hashers without intermediate allocation.
pub fn compute_canonical_checksums(segments: &[MemorySegment]) -> (u32, ChecksumSummary) {
    let mut crc_hasher = Crc32Hasher::new();
    let mut md5_hasher = Md5::new();
    let mut sha_hasher = Sha256::new();

    for segment in segments {
        crc_hasher.update(&segment.data);
        md5_hasher.update(&segment.data);
        sha_hasher.update(&segment.data);
    }

    let crc = crc_hasher.finalize();
    let md5_result = md5_hasher.finalize();
    let sha_result = sha_hasher.finalize();

    let summary = ChecksumSummary {
        crc32: format!("0x{:08X}", crc),
        md5: format!("{:x}", md5_result),
        sha256: format!("{:x}", sha_result),
    };

    (crc, summary)
}

/// Computes checksums over the entire address span of the segments, filling any
/// memory gaps with `pad_byte` (typically `0xFF` representing erased flash).
pub fn compute_padded_checksums(
    segments: &[MemorySegment],
    pad_byte: u8,
) -> Option<ChecksumSummary> {
    if segments.is_empty() {
        return None;
    }

    let mut crc_hasher = Crc32Hasher::new();
    let mut md5_hasher = Md5::new();
    let mut sha_hasher = Sha256::new();

    let pad_chunk = [pad_byte; 1024];

    let mut current_addr_64 = segments[0].start_address as u64;

    for segment in segments {
        let seg_start_64 = segment.start_address as u64;
        if seg_start_64 > current_addr_64 {
            let mut gap_remaining = (seg_start_64 - current_addr_64) as usize;
            while gap_remaining > 0 {
                let to_write = std::cmp::min(gap_remaining, pad_chunk.len());
                crc_hasher.update(&pad_chunk[..to_write]);
                md5_hasher.update(&pad_chunk[..to_write]);
                sha_hasher.update(&pad_chunk[..to_write]);
                gap_remaining -= to_write;
            }
        }

        crc_hasher.update(&segment.data);
        md5_hasher.update(&segment.data);
        sha_hasher.update(&segment.data);

        current_addr_64 = segment.end_address_u64();
    }

    let crc = crc_hasher.finalize();
    let md5_result = md5_hasher.finalize();
    let sha_result = sha_hasher.finalize();

    Some(ChecksumSummary {
        crc32: format!("0x{:08X}", crc),
        md5: format!("{:x}", md5_result),
        sha256: format!("{:x}", sha_result),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_slice_checksums() {
        let (crc, summary) = compute_checksums(&[]);
        assert_eq!(crc, 0);
        assert_eq!(summary.crc32, "0x00000000");
        assert_eq!(summary.md5, "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            summary.sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_padded_checksums_empty_segments_returns_none() {
        assert!(compute_padded_checksums(&[], 0xFF).is_none());
    }

    #[test]
    fn test_padded_checksums_single_segment_matches_direct() {
        let seg = MemorySegment::new(0x08000000, vec![1, 2, 3, 4]);
        let padded = compute_padded_checksums(std::slice::from_ref(&seg), 0xFF).unwrap();
        let (_crc, direct) = compute_checksums(&seg.data);
        assert_eq!(padded.crc32, direct.crc32);
        assert_eq!(padded.md5, direct.md5);
        assert_eq!(padded.sha256, direct.sha256);
    }
}
