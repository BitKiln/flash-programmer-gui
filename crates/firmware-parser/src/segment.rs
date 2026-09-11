use crate::checksum::compute_checksums;
use crate::error::ParseError;
use crate::metadata::{MemoryGap, MemorySegment, SegmentMetadata, ValidationWarning};

/// A raw chunk of memory parsed from an individual record before consolidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawChunk {
    pub address: u32,
    pub data: Vec<u8>,
    pub line: usize,
}

/// Result of chunk consolidation: segments, memory gaps, and non-fatal validation warnings.
pub type ConsolidatedResult = (Vec<MemorySegment>, Vec<MemoryGap>, Vec<ValidationWarning>);

/// Consolidates raw chunks into sorted, contiguous memory segments, detecting gaps
/// and verifying that overlapping records do not contain conflicting data.
pub fn consolidate_chunks(mut chunks: Vec<RawChunk>) -> Result<ConsolidatedResult, ParseError> {
    if chunks.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }

    // Sort chunks in ascending order by start address, breaking ties by line number
    chunks.sort_by(|a, b| a.address.cmp(&b.address).then_with(|| a.line.cmp(&b.line)));

    let mut segments = Vec::new();
    let mut gaps = Vec::new();
    let mut warnings = Vec::new();

    let mut current_segment = MemorySegment::new(chunks[0].address, chunks[0].data.clone());

    for chunk in chunks.into_iter().skip(1) {
        let current_end_64 = current_segment.end_address_u64();
        let chunk_addr_64 = chunk.address as u64;

        if chunk_addr_64 == current_end_64 {
            // Contiguous slice: append directly
            let new_end_64 = current_end_64 + (chunk.data.len() as u64);
            if new_end_64 > 0x1_0000_0000 {
                return Err(ParseError::AddressOverflow {
                    line: chunk.line,
                    address: new_end_64,
                });
            }
            current_segment.data.extend_from_slice(&chunk.data);
        } else if chunk_addr_64 > current_end_64 {
            // Gap detected: record gap and begin new segment
            let gap_size = (chunk_addr_64 - current_end_64) as u32;
            gaps.push(MemoryGap {
                start_address: current_end_64 as u32,
                end_address: chunk.address,
                size: gap_size,
            });
            segments.push(current_segment);
            current_segment = MemorySegment::new(chunk.address, chunk.data);
        } else {
            // Overlapping slice: chunk_addr_64 < current_end_64
            let overlap_offset = (chunk_addr_64 - (current_segment.start_address as u64)) as usize;
            let overlap_len = std::cmp::min(
                current_segment.data.len() - overlap_offset,
                chunk.data.len(),
            );

            for i in 0..overlap_len {
                let existing = current_segment.data[overlap_offset + i];
                let incoming = chunk.data[i];
                if existing != incoming {
                    return Err(ParseError::ConflictingDataOverlap {
                        line: chunk.line,
                        address: (chunk_addr_64 + i as u64) as u32,
                        existing,
                        incoming,
                    });
                }
            }

            warnings.push(ValidationWarning::RedundantOverlap {
                address: chunk.address,
                line: Some(chunk.line),
            });

            if chunk.data.len() > overlap_len {
                let extra_len = (chunk.data.len() - overlap_len) as u64;
                let new_end_64 = current_end_64 + extra_len;
                if new_end_64 > 0x1_0000_0000 {
                    return Err(ParseError::AddressOverflow {
                        line: chunk.line,
                        address: new_end_64,
                    });
                }
                current_segment
                    .data
                    .extend_from_slice(&chunk.data[overlap_len..]);
            }
        }
    }

    segments.push(current_segment);

    Ok((segments, gaps, warnings))
}

/// Generates segment metadata with per-segment checksums.
pub fn build_segments_metadata(segments: &[MemorySegment]) -> Vec<SegmentMetadata> {
    segments
        .iter()
        .enumerate()
        .map(|(index, seg)| {
            let (_crc, checksums) = compute_checksums(&seg.data);
            let end_address = if seg.end_address_u64() >= 0x1_0000_0000 {
                0xFFFF_FFFF
            } else {
                seg.end_address_u64() as u32
            };
            SegmentMetadata {
                index,
                start_address: seg.start_address,
                end_address,
                size_bytes: seg.data.len(),
                checksums,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contiguous_consolidation() {
        let chunks = vec![
            RawChunk {
                address: 0x08000000,
                data: vec![1, 2, 3, 4],
                line: 1,
            },
            RawChunk {
                address: 0x08000004,
                data: vec![5, 6, 7, 8],
                line: 2,
            },
        ];

        let (segments, gaps, warnings) = consolidate_chunks(chunks).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start_address, 0x08000000);
        assert_eq!(segments[0].end_address(), 0x08000008);
        assert_eq!(segments[0].data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(gaps.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_gap_detection() {
        let chunks = vec![
            RawChunk {
                address: 0x08000000,
                data: vec![0xAA; 16],
                line: 1,
            },
            RawChunk {
                address: 0x08040000,
                data: vec![0xBB; 16],
                line: 2,
            },
        ];

        let (segments, gaps, warnings) = consolidate_chunks(chunks).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].start_address, 0x08000010);
        assert_eq!(gaps[0].end_address, 0x08040000);
        assert_eq!(gaps[0].size, 0x08040000 - 0x08000010);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_conflicting_overlap() {
        let chunks = vec![
            RawChunk {
                address: 0x08000000,
                data: vec![0xAA, 0xBB],
                line: 1,
            },
            RawChunk {
                address: 0x08000001,
                data: vec![0xCC, 0xDD],
                line: 2,
            },
        ];

        let err = consolidate_chunks(chunks).unwrap_err();
        assert_eq!(
            err,
            ParseError::ConflictingDataOverlap {
                line: 2,
                address: 0x08000001,
                existing: 0xBB,
                incoming: 0xCC,
            }
        );
    }

    #[test]
    fn test_redundant_overlap_warning() {
        let chunks = vec![
            RawChunk {
                address: 0x08000000,
                data: vec![0xAA, 0xBB],
                line: 1,
            },
            RawChunk {
                address: 0x08000000,
                data: vec![0xAA, 0xBB, 0xCC],
                line: 2,
            },
        ];

        let (segments, gaps, warnings) = consolidate_chunks(chunks).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].data, vec![0xAA, 0xBB, 0xCC]);
        assert_eq!(warnings.len(), 1);
        assert!(matches!(
            warnings[0],
            ValidationWarning::RedundantOverlap {
                address: 0x08000000,
                line: Some(2)
            }
        ));
        assert!(gaps.is_empty());
    }
}
