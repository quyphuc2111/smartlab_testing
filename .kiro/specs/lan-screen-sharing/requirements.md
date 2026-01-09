# Requirements Document

## Introduction

Hệ thống chia sẻ màn hình qua mạng LAN cho phép Teacher broadcast màn hình đến nhiều Students trong cùng mạng nội bộ. Hệ thống hoạt động hoàn toàn offline, không cần server trung gian, sử dụng kỹ thuật tương tự RustDesk với VP9 video codec để đạt hiệu suất cao và độ trễ thấp.

## Glossary

- **Teacher**: Máy tính phát sóng màn hình (server/broadcaster)
- **Student**: Máy tính nhận và hiển thị màn hình (client/viewer)
- **Video_Service**: Module xử lý capture và encode video
- **Discovery_Service**: Module phát hiện server trong mạng LAN
- **Streaming_Service**: Module truyền tải video frames qua mạng
- **VP9_Encoder**: Bộ mã hóa video VP9 cho nén hiệu quả
- **Frame_Buffer**: Bộ đệm lưu trữ và reassemble video frames

## Requirements

### Requirement 1: Screen Capture

**User Story:** As a Teacher, I want to capture my screen content, so that I can share it with Students.

#### Acceptance Criteria

1. WHEN the Teacher starts sharing, THE Video_Service SHALL capture frames from the selected display
2. WHEN multiple displays are available, THE Video_Service SHALL allow selection of specific display
3. WHILE capturing, THE Video_Service SHALL maintain configurable frame rate (1-60 FPS)
4. THE Video_Service SHALL convert captured BGRA frames to YUV420 format for encoding

### Requirement 2: Video Encoding

**User Story:** As a Teacher, I want my screen to be efficiently encoded, so that it can be transmitted with low bandwidth.

#### Acceptance Criteria

1. THE VP9_Encoder SHALL encode YUV420 frames to VP9 format
2. WHEN encoding, THE VP9_Encoder SHALL support configurable bitrate (500kbps - 8Mbps)
3. WHEN encoding, THE VP9_Encoder SHALL produce keyframes at configurable intervals
4. THE VP9_Encoder SHALL optimize for low latency real-time encoding

### Requirement 3: LAN Discovery

**User Story:** As a Student, I want to automatically discover Teachers on my network, so that I can easily connect without manual configuration.

#### Acceptance Criteria

1. WHEN the Teacher starts sharing, THE Discovery_Service SHALL broadcast presence via UDP on port 34254
2. WHEN the Student starts, THE Discovery_Service SHALL listen for Teacher broadcasts
3. WHEN a Teacher is discovered, THE Discovery_Service SHALL emit server information (IP, port, name)
4. THE Discovery_Service SHALL continue broadcasting every 1 second while sharing is active

### Requirement 4: Video Streaming

**User Story:** As a Teacher, I want to stream my screen to multiple Students simultaneously, so that the entire class can view my content.

#### Acceptance Criteria

1. WHEN streaming starts, THE Streaming_Service SHALL accept TCP connections from Students
2. WHEN a Student connects, THE Streaming_Service SHALL send the current keyframe first
3. WHILE streaming, THE Streaming_Service SHALL broadcast encoded frames to all connected Students
4. WHEN a frame exceeds MTU size, THE Streaming_Service SHALL split it into chunks with sequence headers
5. IF a Student disconnects, THEN THE Streaming_Service SHALL continue streaming to remaining Students

### Requirement 5: Video Receiving

**User Story:** As a Student, I want to receive and display the Teacher's screen, so that I can follow along with the lesson.

#### Acceptance Criteria

1. WHEN connecting to Teacher, THE Student SHALL establish TCP connection to the streaming port
2. WHEN receiving chunks, THE Frame_Buffer SHALL reassemble complete frames from chunks
3. WHEN a complete frame is received, THE Student SHALL decode VP9 to raw pixels
4. WHEN decoded, THE Student SHALL display the frame on canvas
5. IF frames are received out of order, THEN THE Frame_Buffer SHALL handle reordering

### Requirement 6: Connection Management

**User Story:** As a Teacher, I want to manage streaming sessions, so that I can control when sharing starts and stops.

#### Acceptance Criteria

1. WHEN the Teacher clicks "Start Sharing", THE System SHALL start capture, encoding, and streaming
2. WHEN the Teacher clicks "Stop Sharing", THE System SHALL stop all services and disconnect Students
3. WHILE sharing, THE System SHALL display connection count and streaming statistics
4. IF an error occurs during streaming, THEN THE System SHALL log the error and attempt recovery

### Requirement 7: Quality Control

**User Story:** As a Teacher, I want to adjust streaming quality, so that I can balance between quality and network bandwidth.

#### Acceptance Criteria

1. THE System SHALL provide quality presets (Low: 500kbps, Medium: 2Mbps, High: 5Mbps)
2. WHEN quality is changed, THE VP9_Encoder SHALL adjust bitrate accordingly
3. THE System SHALL allow manual FPS configuration (5, 10, 15, 30, 60)
4. WHILE streaming, THE System SHALL display current bitrate and frame rate
