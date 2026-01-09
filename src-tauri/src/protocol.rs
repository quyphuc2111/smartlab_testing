//! Frame Packet Protocol Module
//!
//! Defines the wire format for streaming video frames over TCP.
//! Handles serialization, chunking, and reassembly of video frames.
//!
//! Wire Format (StreamPacket):
//! ┌────────────────────────────────────────────────────────┐
//! │  Frame ID (4 bytes, big-endian)                        │
//! ├────────────────────────────────────────────────────────┤
//! │  Flags (1 byte)                                        │
//! │    bit 0: is_keyframe                                  │
//! │    bit 1: is_chunk (part of larger frame)              │
//! │    bit 2-7: reserved                                   │
//! ├────────────────────────────────────────────────────────┤
//! │  Chunk Index (2 bytes, big-endian) - if is_chunk       │
//! ├────────────────────────────────────────────────────────┤
//! │  Total Chunks (2 bytes, big-endian) - if is_chunk      │
//! ├────────────────────────────────────────────────────────┤
//! │  Payload Size (4 bytes, big-endian)                    │
//! ├────────────────────────────────────────────────────────┤
//! │  Payload (VP9 encoded data)                            │
//! └────────────────────────────────────────────────────────┘

use std::collections::HashMap;

/// Maximum chunk size (64KB - header overhead)
pub const MAX_CHUNK_SIZE: usize = 64 * 1024 - 16;

/// Header size for non-chunked packets
pub const HEADER_SIZE: usize = 9; // 4 (frame_id) + 1 (flags) + 4 (payload_size)

/// Header size for chunked packets
pub const CHUNKED_HEADER_SIZE: usize = 13; // 4 (frame_id) + 1 (flags) + 2 (chunk_idx) + 2 (total_chunks) + 4 (payload_size)

/// Flag bit for keyframe
pub const FLAG_KEYFRAME: u8 = 0x01;

/// Flag bit for chunked packet
pub const FLAG_CHUNKED: u8 = 0x02;

/// Stream packet for video frame transmission
#[derive(Debug, Clone, PartialEq)]
pub struct StreamPacket {
    /// Unique frame identifier (monotonically increasing)
    pub frame_id: u32,
    /// Whether this frame is a keyframe
    pub is_keyframe: bool,
    /// Chunk index (0 if not chunked)
    pub chunk_index: u16,
    /// Total number of chunks (1 if not chunked)
    pub total_chunks: u16,
    /// VP9 encoded payload data
    pub payload: Vec<u8>,
}

impl StreamPacket {
    /// Create a new non-chunked stream packet
    pub fn new(frame_id: u32, is_keyframe: bool, payload: Vec<u8>) -> Self {
        Self {
            frame_id,
            is_keyframe,
            chunk_index: 0,
            total_chunks: 1,
            payload,
        }
    }

    /// Create a new chunked stream packet
    pub fn new_chunk(
        frame_id: u32,
        is_keyframe: bool,
        chunk_index: u16,
        total_chunks: u16,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            frame_id,
            is_keyframe,
            chunk_index,
            total_chunks,
            payload,
        }
    }

    /// Check if this packet is part of a chunked frame
    pub fn is_chunked(&self) -> bool {
        self.total_chunks > 1
    }

    /// Serialize the packet to bytes for transmission
    pub fn serialize(&self) -> Vec<u8> {
        let is_chunked = self.is_chunked();
        let header_size = if is_chunked { CHUNKED_HEADER_SIZE } else { HEADER_SIZE };
        let mut buf = Vec::with_capacity(header_size + self.payload.len());

        // Frame ID (4 bytes, big-endian)
        buf.extend_from_slice(&self.frame_id.to_be_bytes());

        // Flags (1 byte)
        let mut flags: u8 = 0;
        if self.is_keyframe {
            flags |= FLAG_KEYFRAME;
        }
        if is_chunked {
            flags |= FLAG_CHUNKED;
        }
        buf.push(flags);

        // Chunk info (only if chunked)
        if is_chunked {
            buf.extend_from_slice(&self.chunk_index.to_be_bytes());
            buf.extend_from_slice(&self.total_chunks.to_be_bytes());
        }

        // Payload size (4 bytes, big-endian)
        buf.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());

        // Payload
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Deserialize a packet from bytes
    ///
    /// Returns the packet and the number of bytes consumed
    pub fn deserialize(data: &[u8]) -> Result<(Self, usize), ProtocolError> {
        if data.len() < HEADER_SIZE {
            return Err(ProtocolError::InsufficientData {
                expected: HEADER_SIZE,
                actual: data.len(),
            });
        }

        // Frame ID
        let frame_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);

        // Flags
        let flags = data[4];
        let is_keyframe = (flags & FLAG_KEYFRAME) != 0;
        let is_chunked = (flags & FLAG_CHUNKED) != 0;

        let (chunk_index, total_chunks, payload_size_offset) = if is_chunked {
            if data.len() < CHUNKED_HEADER_SIZE {
                return Err(ProtocolError::InsufficientData {
                    expected: CHUNKED_HEADER_SIZE,
                    actual: data.len(),
                });
            }
            let chunk_index = u16::from_be_bytes([data[5], data[6]]);
            let total_chunks = u16::from_be_bytes([data[7], data[8]]);
            (chunk_index, total_chunks, 9)
        } else {
            (0, 1, 5)
        };

        // Payload size
        let payload_size = u32::from_be_bytes([
            data[payload_size_offset],
            data[payload_size_offset + 1],
            data[payload_size_offset + 2],
            data[payload_size_offset + 3],
        ]) as usize;

        let header_size = payload_size_offset + 4;
        let total_size = header_size + payload_size;

        if data.len() < total_size {
            return Err(ProtocolError::InsufficientData {
                expected: total_size,
                actual: data.len(),
            });
        }

        // Payload
        let payload = data[header_size..total_size].to_vec();

        Ok((
            Self {
                frame_id,
                is_keyframe,
                chunk_index,
                total_chunks,
                payload,
            },
            total_size,
        ))
    }
}


/// Protocol error types
#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolError {
    /// Not enough data to parse packet
    InsufficientData { expected: usize, actual: usize },
    /// Invalid packet format
    InvalidFormat(String),
    /// Chunk reassembly error
    ReassemblyError(String),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::InsufficientData { expected, actual } => {
                write!(f, "Insufficient data: expected {} bytes, got {}", expected, actual)
            }
            ProtocolError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            ProtocolError::ReassemblyError(msg) => write!(f, "Reassembly error: {}", msg),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Split a large frame into chunks for transmission
///
/// # Arguments
/// * `frame_id` - Unique frame identifier
/// * `is_keyframe` - Whether this is a keyframe
/// * `data` - The frame data to chunk
///
/// # Returns
/// Vector of StreamPackets (single packet if data fits, multiple chunks otherwise)
pub fn chunk_frame(frame_id: u32, is_keyframe: bool, data: &[u8]) -> Vec<StreamPacket> {
    if data.len() <= MAX_CHUNK_SIZE {
        // No chunking needed
        return vec![StreamPacket::new(frame_id, is_keyframe, data.to_vec())];
    }

    // Split into chunks
    let chunks: Vec<&[u8]> = data.chunks(MAX_CHUNK_SIZE).collect();
    let total_chunks = chunks.len() as u16;

    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            StreamPacket::new_chunk(
                frame_id,
                is_keyframe,
                i as u16,
                total_chunks,
                chunk.to_vec(),
            )
        })
        .collect()
}

/// Frame buffer for reassembling chunked frames
#[derive(Debug)]
pub struct FrameBuffer {
    /// Pending frames being reassembled (frame_id -> chunks)
    pending: HashMap<u32, PendingFrame>,
    /// Last completed frame ID (for ordering)
    last_complete_frame_id: u32,
    /// Maximum number of pending frames to keep
    max_pending: usize,
}

#[derive(Debug)]
struct PendingFrame {
    /// Whether this frame is a keyframe
    is_keyframe: bool,
    /// Total chunks expected
    total_chunks: u16,
    /// Received chunks (chunk_index -> payload)
    chunks: HashMap<u16, Vec<u8>>,
}

/// Reassembled frame output
#[derive(Debug, Clone)]
pub struct ReassembledFrame {
    /// Frame ID
    pub frame_id: u32,
    /// Whether this is a keyframe
    pub is_keyframe: bool,
    /// Complete frame data
    pub data: Vec<u8>,
}

impl Default for FrameBuffer {
    fn default() -> Self {
        Self::new(10)
    }
}

impl FrameBuffer {
    /// Create a new frame buffer
    ///
    /// # Arguments
    /// * `max_pending` - Maximum number of incomplete frames to buffer
    pub fn new(max_pending: usize) -> Self {
        Self {
            pending: HashMap::new(),
            last_complete_frame_id: 0,
            max_pending,
        }
    }

    /// Process an incoming packet
    ///
    /// # Returns
    /// - `Ok(Some(frame))` if a complete frame is ready
    /// - `Ok(None)` if more chunks are needed
    /// - `Err` if there's a protocol error
    pub fn process_packet(&mut self, packet: StreamPacket) -> Result<Option<ReassembledFrame>, ProtocolError> {
        // Skip old frames
        if packet.frame_id <= self.last_complete_frame_id {
            return Ok(None);
        }

        // Non-chunked packet - return immediately
        if !packet.is_chunked() {
            self.last_complete_frame_id = packet.frame_id;
            self.cleanup_old_frames(packet.frame_id);
            return Ok(Some(ReassembledFrame {
                frame_id: packet.frame_id,
                is_keyframe: packet.is_keyframe,
                data: packet.payload,
            }));
        }

        // Chunked packet - add to pending
        let pending = self.pending.entry(packet.frame_id).or_insert_with(|| PendingFrame {
            is_keyframe: packet.is_keyframe,
            total_chunks: packet.total_chunks,
            chunks: HashMap::new(),
        });

        // Validate chunk info
        if pending.total_chunks != packet.total_chunks {
            return Err(ProtocolError::ReassemblyError(format!(
                "Chunk count mismatch for frame {}: expected {}, got {}",
                packet.frame_id, pending.total_chunks, packet.total_chunks
            )));
        }

        // Store chunk
        pending.chunks.insert(packet.chunk_index, packet.payload);

        // Check if frame is complete
        if pending.chunks.len() == pending.total_chunks as usize {
            // Reassemble frame
            let mut data = Vec::new();
            for i in 0..pending.total_chunks {
                match pending.chunks.get(&i) {
                    Some(chunk) => data.extend_from_slice(chunk),
                    None => {
                        return Err(ProtocolError::ReassemblyError(format!(
                            "Missing chunk {} for frame {}",
                            i, packet.frame_id
                        )));
                    }
                }
            }

            let is_keyframe = pending.is_keyframe;
            self.pending.remove(&packet.frame_id);
            self.last_complete_frame_id = packet.frame_id;
            self.cleanup_old_frames(packet.frame_id);

            return Ok(Some(ReassembledFrame {
                frame_id: packet.frame_id,
                is_keyframe,
                data,
            }));
        }

        // Enforce max pending limit
        if self.pending.len() > self.max_pending {
            self.cleanup_old_frames(packet.frame_id);
        }

        Ok(None)
    }

    /// Clean up old incomplete frames
    fn cleanup_old_frames(&mut self, current_frame_id: u32) {
        let threshold = current_frame_id.saturating_sub(self.max_pending as u32);
        self.pending.retain(|&id, _| id > threshold);
    }

    /// Get the last completed frame ID
    pub fn last_complete_frame_id(&self) -> u32 {
        self.last_complete_frame_id
    }

    /// Get the number of pending incomplete frames
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Reset the frame buffer state
    pub fn reset(&mut self) {
        self.pending.clear();
        self.last_complete_frame_id = 0;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_packet_new() {
        let packet = StreamPacket::new(1, true, vec![1, 2, 3, 4]);
        assert_eq!(packet.frame_id, 1);
        assert!(packet.is_keyframe);
        assert_eq!(packet.chunk_index, 0);
        assert_eq!(packet.total_chunks, 1);
        assert_eq!(packet.payload, vec![1, 2, 3, 4]);
        assert!(!packet.is_chunked());
    }

    #[test]
    fn test_stream_packet_new_chunk() {
        let packet = StreamPacket::new_chunk(5, false, 2, 4, vec![5, 6, 7]);
        assert_eq!(packet.frame_id, 5);
        assert!(!packet.is_keyframe);
        assert_eq!(packet.chunk_index, 2);
        assert_eq!(packet.total_chunks, 4);
        assert_eq!(packet.payload, vec![5, 6, 7]);
        assert!(packet.is_chunked());
    }

    #[test]
    fn test_serialize_deserialize_non_chunked() {
        let original = StreamPacket::new(42, true, vec![10, 20, 30, 40, 50]);
        let serialized = original.serialize();
        let (deserialized, consumed) = StreamPacket::deserialize(&serialized).unwrap();

        assert_eq!(consumed, serialized.len());
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_serialize_deserialize_chunked() {
        let original = StreamPacket::new_chunk(100, false, 3, 10, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let serialized = original.serialize();
        let (deserialized, consumed) = StreamPacket::deserialize(&serialized).unwrap();

        assert_eq!(consumed, serialized.len());
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_serialize_deserialize_empty_payload() {
        let original = StreamPacket::new(1, false, vec![]);
        let serialized = original.serialize();
        let (deserialized, consumed) = StreamPacket::deserialize(&serialized).unwrap();

        assert_eq!(consumed, serialized.len());
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_deserialize_insufficient_data() {
        let data = vec![0, 1, 2]; // Too short
        let result = StreamPacket::deserialize(&data);
        assert!(matches!(result, Err(ProtocolError::InsufficientData { .. })));
    }

    #[test]
    fn test_chunk_frame_small() {
        let data = vec![1, 2, 3, 4, 5];
        let packets = chunk_frame(1, true, &data);

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].frame_id, 1);
        assert!(packets[0].is_keyframe);
        assert!(!packets[0].is_chunked());
        assert_eq!(packets[0].payload, data);
    }

    #[test]
    fn test_chunk_frame_large() {
        // Create data larger than MAX_CHUNK_SIZE
        let data: Vec<u8> = (0..MAX_CHUNK_SIZE * 3 + 100).map(|i| (i % 256) as u8).collect();
        let packets = chunk_frame(5, false, &data);

        assert_eq!(packets.len(), 4); // Should be 4 chunks

        for (i, packet) in packets.iter().enumerate() {
            assert_eq!(packet.frame_id, 5);
            assert!(!packet.is_keyframe);
            assert_eq!(packet.chunk_index, i as u16);
            assert_eq!(packet.total_chunks, 4);
            assert!(packet.is_chunked());
        }

        // Verify total data matches
        let reassembled: Vec<u8> = packets.iter().flat_map(|p| p.payload.clone()).collect();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_frame_buffer_non_chunked() {
        let mut buffer = FrameBuffer::new(10);

        let packet = StreamPacket::new(1, true, vec![1, 2, 3]);
        let result = buffer.process_packet(packet).unwrap();

        assert!(result.is_some());
        let frame = result.unwrap();
        assert_eq!(frame.frame_id, 1);
        assert!(frame.is_keyframe);
        assert_eq!(frame.data, vec![1, 2, 3]);
    }

    #[test]
    fn test_frame_buffer_chunked() {
        let mut buffer = FrameBuffer::new(10);

        // Send 3 chunks out of order
        let chunk2 = StreamPacket::new_chunk(1, true, 2, 3, vec![7, 8, 9]);
        let chunk0 = StreamPacket::new_chunk(1, true, 0, 3, vec![1, 2, 3]);
        let chunk1 = StreamPacket::new_chunk(1, true, 1, 3, vec![4, 5, 6]);

        // First two chunks shouldn't complete the frame
        assert!(buffer.process_packet(chunk2).unwrap().is_none());
        assert!(buffer.process_packet(chunk0).unwrap().is_none());

        // Third chunk should complete the frame
        let result = buffer.process_packet(chunk1).unwrap();
        assert!(result.is_some());

        let frame = result.unwrap();
        assert_eq!(frame.frame_id, 1);
        assert!(frame.is_keyframe);
        assert_eq!(frame.data, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn test_frame_buffer_skip_old_frames() {
        let mut buffer = FrameBuffer::new(10);

        // Process frame 5
        let packet5 = StreamPacket::new(5, false, vec![5]);
        buffer.process_packet(packet5).unwrap();

        // Try to process older frame 3 - should be skipped
        let packet3 = StreamPacket::new(3, false, vec![3]);
        let result = buffer.process_packet(packet3).unwrap();
        assert!(result.is_none());

        assert_eq!(buffer.last_complete_frame_id(), 5);
    }

    #[test]
    fn test_frame_buffer_cleanup() {
        let mut buffer = FrameBuffer::new(3);

        // Add incomplete frames
        for i in 1..=5 {
            let chunk = StreamPacket::new_chunk(i, false, 0, 2, vec![i as u8]);
            buffer.process_packet(chunk).unwrap();
        }

        // Should have cleaned up old frames
        assert!(buffer.pending_count() <= 3);
    }

    #[test]
    fn test_frame_buffer_reset() {
        let mut buffer = FrameBuffer::new(10);

        let packet = StreamPacket::new(10, false, vec![1]);
        buffer.process_packet(packet).unwrap();

        assert_eq!(buffer.last_complete_frame_id(), 10);

        buffer.reset();

        assert_eq!(buffer.last_complete_frame_id(), 0);
        assert_eq!(buffer.pending_count(), 0);
    }

    #[test]
    fn test_chunk_and_reassemble_roundtrip() {
        let original_data: Vec<u8> = (0..200_000).map(|i| (i % 256) as u8).collect();
        let packets = chunk_frame(42, true, &original_data);

        let mut buffer = FrameBuffer::new(10);
        let mut result = None;

        for packet in packets {
            if let Some(frame) = buffer.process_packet(packet).unwrap() {
                result = Some(frame);
            }
        }

        assert!(result.is_some());
        let frame = result.unwrap();
        assert_eq!(frame.frame_id, 42);
        assert!(frame.is_keyframe);
        assert_eq!(frame.data, original_data);
    }
}
