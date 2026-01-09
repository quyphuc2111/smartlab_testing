# Design Document: LAN Screen Sharing

## Overview

Hệ thống screen sharing qua LAN được thiết kế theo kiến trúc tương tự RustDesk, sử dụng VP9 codec để nén video hiệu quả và TCP để truyền tải đáng tin cậy. Hệ thống gồm 2 roles: Teacher (broadcaster) và Student (viewer), hoạt động hoàn toàn offline trong mạng nội bộ.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         TEACHER (Server)                         │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐  │
│  │   Display    │───▶│   Capturer   │───▶│   VP9 Encoder    │  │
│  │   (scrap)    │    │  (BGRA→YUV)  │    │   (vpx-encode)   │  │
│  └──────────────┘    └──────────────┘    └────────┬─────────┘  │
│                                                    │            │
│  ┌──────────────┐                        ┌────────▼─────────┐  │
│  │  Discovery   │◀──UDP Broadcast────────│  TCP Streaming   │  │
│  │   Service    │                        │     Server       │  │
│  └──────────────┘                        └────────┬─────────┘  │
└───────────────────────────────────────────────────┼─────────────┘
                                                    │ TCP
                    ┌───────────────────────────────┼───────────────┐
                    │                               │               │
┌───────────────────▼───┐  ┌───────────────────────▼───┐  ┌───────▼───────┐
│      STUDENT 1        │  │      STUDENT 2            │  │   STUDENT N   │
├───────────────────────┤  ├───────────────────────────┤  ├───────────────┤
│ ┌─────────────────┐   │  │ ┌─────────────────┐       │  │               │
│ │  TCP Receiver   │   │  │ │  TCP Receiver   │       │  │     ...       │
│ └────────┬────────┘   │  │ └────────┬────────┘       │  │               │
│ ┌────────▼────────┐   │  │ ┌────────▼────────┐       │  │               │
│ │  Frame Buffer   │   │  │ │  Frame Buffer   │       │  │               │
│ └────────┬────────┘   │  │ └────────┬────────┘       │  │               │
│ ┌────────▼────────┐   │  │ ┌────────▼────────┐       │  │               │
│ │  VP9 Decoder    │   │  │ │  VP9 Decoder    │       │  │               │
│ └────────┬────────┘   │  │ └────────┬────────┘       │  │               │
│ ┌────────▼────────┐   │  │ ┌────────▼────────┐       │  │               │
│ │  Canvas Render  │   │  │ │  Canvas Render  │       │  │               │
│ └─────────────────┘   │  │ └─────────────────┘       │  │               │
└───────────────────────┘  └───────────────────────────┘  └───────────────┘
```

## Components and Interfaces

### 1. Video Capture Module (`capture.rs`)

```rust
pub struct CaptureConfig {
    pub display_index: usize,
    pub fps: u32,
}

pub struct CaptureState {
    pub is_capturing: Arc<AtomicBool>,
}

// Captures screen and converts BGRA to YUV420
pub fn start_capture(
    config: CaptureConfig,
    frame_tx: Sender<YuvFrame>,
    is_capturing: Arc<AtomicBool>,
) -> Result<(), String>;

pub fn get_displays() -> Result<Vec<DisplayInfo>, String>;
```

### 2. VP9 Encoder Module (`encoder.rs`)

```rust
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,      // kbps
    pub keyframe_interval: u32,
}

pub struct Vp9Encoder {
    encoder: vpx_encode::Encoder,
    frame_count: u64,
    config: EncoderConfig,
}

impl Vp9Encoder {
    pub fn new(config: EncoderConfig) -> Result<Self, String>;
    pub fn encode(&mut self, yuv_frame: &YuvFrame) -> Result<Vec<EncodedPacket>, String>;
}

pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub pts: u64,
}
```

### 3. Streaming Server Module (`server.rs`)

```rust
pub struct StreamingServer {
    clients: Arc<Mutex<Vec<TcpStream>>>,
    last_keyframe: Arc<Mutex<Option<Vec<u8>>>>,
}

impl StreamingServer {
    pub async fn start(port: u16) -> Result<Self, String>;
    pub async fn broadcast(&self, packet: &EncodedPacket);
    pub fn client_count(&self) -> usize;
}

// Frame packet format:
// [4 bytes: frame_id] [1 byte: flags] [4 bytes: total_size] [data...]
// flags: bit 0 = is_keyframe
```

### 4. Discovery Service (`discovery.rs`)

```rust
// UDP broadcast message format:
// "SCREEN_SHARE:<ip>:<port>:<name>"

pub async fn start_discovery_broadcaster(
    port: u16,
    name: &str,
    stop_rx: broadcast::Receiver<()>,
);

pub async fn start_discovery_listener(
    app: AppHandle,
) -> Result<(), String>;
```

### 5. Client Receiver Module (`client.rs`)

```rust
pub struct VideoReceiver {
    decoder: Vp9Decoder,
    frame_buffer: FrameBuffer,
}

impl VideoReceiver {
    pub async fn connect(ip: &str, port: u16) -> Result<Self, String>;
    pub async fn receive_frame(&mut self) -> Result<RgbaFrame, String>;
}

pub struct Vp9Decoder {
    decoder: vpx_decode::Decoder,
}

impl Vp9Decoder {
    pub fn decode(&mut self, packet: &[u8]) -> Result<YuvFrame, String>;
    pub fn yuv_to_rgba(yuv: &YuvFrame) -> Vec<u8>;
}
```

## Data Models

### YuvFrame
```rust
pub struct YuvFrame {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,    // Y plane
    pub u: Vec<u8>,    // U plane (Cb)
    pub v: Vec<u8>,    // V plane (Cr)
}
```

### DisplayInfo
```rust
pub struct DisplayInfo {
    pub index: usize,
    pub width: usize,
    pub height: usize,
    pub name: String,
}
```

### ServerInfo
```rust
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
    pub name: String,
}
```

### StreamPacket (Wire Format)
```
┌────────────────────────────────────────────────────────┐
│  Frame ID (4 bytes, big-endian)                        │
├────────────────────────────────────────────────────────┤
│  Flags (1 byte)                                        │
│    bit 0: is_keyframe                                  │
│    bit 1-7: reserved                                   │
├────────────────────────────────────────────────────────┤
│  Payload Size (4 bytes, big-endian)                    │
├────────────────────────────────────────────────────────┤
│  Payload (VP9 encoded data)                            │
└────────────────────────────────────────────────────────┘
```



## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system—essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: Video Pipeline Round-Trip

*For any* valid BGRA frame captured from display, converting to YUV420, encoding with VP9, decoding, and converting back to RGBA should produce visually equivalent output (within lossy compression tolerance).

**Validates: Requirements 1.4, 2.1**

### Property 2: Frame Rate Consistency

*For any* configured FPS value between 1-60, the capture service should produce frames at approximately that rate (±10% tolerance over 1 second window).

**Validates: Requirements 1.3**

### Property 3: Keyframe Interval

*For any* configured keyframe interval N, the encoder should produce a keyframe at least every N frames.

**Validates: Requirements 2.3**

### Property 4: Discovery Message Parsing

*For any* valid discovery broadcast message, parsing should extract correct IP, port, and name fields.

**Validates: Requirements 3.3**

### Property 5: Frame Chunking Round-Trip

*For any* encoded frame (regardless of size), chunking and reassembly should produce identical data.

**Validates: Requirements 4.4, 5.2**

### Property 6: Multi-Client Broadcast

*For any* number of connected clients (1-N), all clients should receive the same frame data when a frame is broadcast.

**Validates: Requirements 4.3**

### Property 7: Keyframe-First Connection

*For any* newly connected client, the first frame received should be a keyframe (to enable immediate decoding).

**Validates: Requirements 4.2**

### Property 8: Frame Ordering

*For any* sequence of frames received (potentially out of order), the frame buffer should output frames in correct order based on frame_id.

**Validates: Requirements 5.5**

### Property 9: Bitrate Configuration

*For any* quality preset or manual bitrate setting, the encoder should accept and apply the configuration without error.

**Validates: Requirements 2.2, 7.2**

## Error Handling

### Capture Errors
- Display not found → Return error, suggest display selection
- Permission denied → Return error with platform-specific guidance
- Frame capture timeout → Skip frame, continue capturing

### Encoding Errors
- Invalid frame dimensions → Return error
- Encoder initialization failed → Return error with codec info
- Encoding failed → Log error, skip frame

### Network Errors
- Port already in use → Return error, suggest different port
- Client connection failed → Log, continue with other clients
- Send failed → Remove client from list, continue streaming

### Decoding Errors
- Invalid VP9 data → Skip frame, wait for next keyframe
- Decoder initialization failed → Return error
- Frame corruption → Request keyframe (future enhancement)

## Testing Strategy

### Unit Tests
- BGRA to YUV420 conversion correctness
- YUV420 to RGBA conversion correctness
- Discovery message parsing
- Frame packet serialization/deserialization
- Chunk splitting and reassembly

### Property-Based Tests
Using `proptest` crate with minimum 100 iterations per property:

1. **Video Pipeline Round-Trip** (Property 1)
   - Generate random BGRA frames
   - Verify encode→decode produces valid output

2. **Frame Chunking Round-Trip** (Property 5)
   - Generate random byte arrays of various sizes
   - Verify chunk→reassemble produces identical data

3. **Discovery Message Parsing** (Property 4)
   - Generate random valid discovery messages
   - Verify parsing extracts correct fields

4. **Frame Ordering** (Property 8)
   - Generate random frame sequences
   - Shuffle and verify correct reordering

### Integration Tests
- Full pipeline: capture → encode → stream → receive → decode → display
- Multi-client streaming scenario
- Discovery and connection flow
- Start/stop lifecycle
