# Implementation Plan: LAN Screen Sharing

## Overview

Triển khai hệ thống screen sharing qua LAN sử dụng VP9 codec, tương tự cách RustDesk hoạt động. Implementation sẽ thay thế JPEG encoding hiện tại bằng VP9 để đạt hiệu suất nén tốt hơn và hỗ trợ keyframes.

## Tasks

- [x] 1. Setup VP9 dependencies và project structure
  - Thêm `vpx-encode`, `vpx-decode` vào Cargo.toml
  - Tạo module structure: `encoder.rs`, `decoder.rs`
  - _Requirements: 2.1, 2.2_

- [x] 2. Implement YUV conversion utilities
  - [x] 2.1 Implement BGRA to YUV420 conversion
    - Tạo `yuv.rs` module
    - Implement `bgra_to_yuv420()` function
    - _Requirements: 1.4_
  - [ ]* 2.2 Write property test for BGRA↔YUV round-trip
    - **Property 1: Video Pipeline Round-Trip**
    - **Validates: Requirements 1.4, 2.1**

- [x] 3. Implement VP9 Encoder
  - [x] 3.1 Create Vp9Encoder struct with configuration
    - Support bitrate, keyframe interval settings
    - _Requirements: 2.1, 2.2, 2.3_
  - [x] 3.2 Implement encode() method
    - Convert YUV frame to VP9 packets
    - Track keyframe intervals
    - _Requirements: 2.1, 2.3_
  - [ ]* 3.3 Write property test for keyframe interval
    - **Property 3: Keyframe Interval**
    - **Validates: Requirements 2.3**

- [x] 4. Implement VP9 Decoder
  - [x] 4.1 Create Vp9Decoder struct
    - Initialize vpx decoder
    - _Requirements: 5.3_
  - [x] 4.2 Implement decode() method
    - Decode VP9 to YUV frame
    - _Requirements: 5.3_
  - [x] 4.3 Implement YUV420 to RGBA conversion
    - For frontend display
    - _Requirements: 5.3_

- [x] 5. Checkpoint - Verify encoder/decoder works
  - Ensure encode→decode round-trip produces valid output
  - Run property tests

- [x] 6. Update Capture module for VP9 pipeline
  - [x] 6.1 Modify capture to output YUV frames
    - Replace JPEG encoding with YUV conversion
    - Send to encoder channel
    - _Requirements: 1.1, 1.4_
  - [ ]* 6.2 Write property test for frame rate consistency
    - **Property 2: Frame Rate Consistency**
    - **Validates: Requirements 1.3**

- [x] 7. Implement Frame Packet Protocol
  - [x] 7.1 Define StreamPacket struct and serialization
    - Frame ID, flags, payload
    - _Requirements: 4.4_
  - [x] 7.2 Implement chunking for large frames
    - Split frames > 64KB into chunks
    - _Requirements: 4.4_
  - [x] 7.3 Implement chunk reassembly in FrameBuffer
    - Reassemble chunks by frame_id
    - Handle out-of-order delivery
    - _Requirements: 5.2, 5.5_
  - [ ]* 7.4 Write property test for chunking round-trip
    - **Property 5: Frame Chunking Round-Trip**
    - **Validates: Requirements 4.4, 5.2**

- [x] 8. Update TCP Streaming Server
  - [x] 8.1 Modify server to send VP9 packets
    - Store last keyframe for new connections
    - _Requirements: 4.1, 4.2_
  - [x] 8.2 Implement keyframe-first for new clients
    - Send cached keyframe on connect
    - _Requirements: 4.2_
  - [x] 8.3 Implement multi-client broadcast
    - Send to all connected clients
    - Handle client disconnection gracefully
    - _Requirements: 4.3, 4.5_
  - [ ]* 8.4 Write property test for multi-client broadcast
    - **Property 6: Multi-Client Broadcast**
    - **Validates: Requirements 4.3**

- [x] 9. Checkpoint - Verify streaming works
  - Test single client connection
  - Test multi-client scenario
  - Verify keyframe-first behavior

- [x] 10. Update Client Receiver
  - [x] 10.1 Implement TCP receiver with frame buffer
    - Connect to server, receive packets
    - _Requirements: 5.1, 5.2_
  - [x] 10.2 Integrate VP9 decoder
    - Decode received packets
    - Convert to RGBA for display
    - _Requirements: 5.3_
  - [x] 10.3 Emit decoded frames to frontend
    - Use Tauri events
    - _Requirements: 5.4_
  - [ ]* 10.4 Write property test for frame ordering
    - **Property 8: Frame Ordering**
    - **Validates: Requirements 5.5**

- [x] 11. Update Discovery Service
  - [x] 11.1 Update discovery message format
    - Include server name
    - _Requirements: 3.1, 3.3_
  - [ ]* 11.2 Write property test for message parsing
    - **Property 4: Discovery Message Parsing**
    - **Validates: Requirements 3.3**

- [x] 12. Update Frontend Components
  - [x] 12.1 Update TeacherView with quality presets
    - Add Low/Medium/High quality options
    - _Requirements: 7.1, 7.3_
  - [x] 12.2 Update StudentView to display VP9 frames
    - Receive RGBA data from backend
    - Draw to canvas
    - _Requirements: 5.4_
  - [x] 12.3 Add connection statistics display
    - Show FPS, bitrate, client count
    - _Requirements: 6.3, 7.4_

- [x] 13. Final Checkpoint - End-to-end testing
  - Full pipeline test: Teacher → Student
  - Multi-student scenario
  - Quality preset switching
  - Ensure all tests pass

## Notes

- Tasks marked with `*` are optional property-based tests
- VP9 encoding requires libvpx to be installed on the system
- For Windows: vcpkg install libvpx
- For macOS: brew install libvpx
- For Linux: apt install libvpx-dev
