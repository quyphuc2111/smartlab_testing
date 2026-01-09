import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { frontendLog } from "./LogPanel";

export default function TeacherView() {
    const [isSharing, setIsSharing] = useState(false);
    const [port, setPort] = useState(8080);
    const [codec, setCodec] = useState("video/webm; codecs=vp9");
    const mediaRecorderRef = useRef<MediaRecorder | null>(null);
    const streamRef = useRef<MediaStream | null>(null);

    // Cleanup on unmount
    useEffect(() => {
        return () => {
            if (isSharing) {
                stopSharing();
            }
        };
    }, [isSharing]);

    const startSharing = async () => {
        try {
            frontendLog(`Starting Server on port ${port}...`);
            // 1. Start Server
            await invoke("start_server_cmd", { port });

            frontendLog("Selecting Screen...");
            // 2. Capture Screen
            const stream = await navigator.mediaDevices.getDisplayMedia({
                video: { cursor: "always" },
                audio: false // Audio adds complexity to MSE, skipping for now
            } as any);

            streamRef.current = stream;

            // 3. Init Recorder
            const recorder = new MediaRecorder(stream, { mimeType: codec });
            mediaRecorderRef.current = recorder;

            let isFirstChunk = true;
            recorder.ondataavailable = async (e) => {
                if (e.data.size > 0) {
                    const buffer = await e.data.arrayBuffer();
                    const bytes = new Uint8Array(buffer);

                    if (isFirstChunk) {
                        frontendLog(`Sending Video Header (${bytes.length} bytes)`);
                        await invoke("send_video_header", { header: Array.from(bytes) });
                        isFirstChunk = false;
                    }
                    if (Math.random() < 0.05) frontendLog(`Sending Chunk (${bytes.length} bytes)`);
                    await invoke("send_video_chunk", { chunk: Array.from(bytes) });
                }
            };

            // 4. Start recording with 100ms slices (low latency)
            recorder.start(100);
            setIsSharing(true);
            frontendLog("Sharing started.");

            // Handle stream stop (user clicks "Stop sharing" in browser UI)
            stream.getVideoTracks()[0].onended = () => {
                stopSharing();
            };

        } catch (err) {
            console.error("Error starting share:", err);
            alert("Failed to start sharing: " + err);
        }
    };

    const stopSharing = async () => {
        frontendLog("Stopping share...");
        if (mediaRecorderRef.current) {
            mediaRecorderRef.current.stop();
        }
        if (streamRef.current) {
            streamRef.current.getTracks().forEach(t => t.stop());
        }
        await invoke("stop_server_cmd");
        setIsSharing(false);
    };

    return (
        <div className="card">
            <h2>Teacher Mode</h2>

            {!isSharing ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', alignItems: 'center' }}>
                    <div>
                        <label>Port: </label>
                        <input
                            type="number"
                            value={port}
                            onChange={(e) => setPort(Number(e.target.value))}
                            style={{ padding: '5px' }}
                        />
                    </div>
                    <div>
                        <label>Codec: </label>
                        <select value={codec} onChange={(e) => setCodec(e.target.value)} style={{ padding: '5px' }}>
                            <option value="video/webm; codecs=vp9">VP9 (WebM)</option>
                            <option value="video/webm; codecs=vp8">VP8 (WebM)</option>
                            <option value="video/webm; codecs=h264">H.264 (WebM)</option>
                        </select>
                    </div>
                    <button onClick={startSharing}>Start Sharing</button>
                </div>
            ) : (
                <div>
                    <p style={{ color: 'green' }}>Creating Stream on Port {port}...</p>
                    <button onClick={stopSharing} style={{ backgroundColor: '#cc3333' }}>Stop Sharing</button>
                </div>
            )}
        </div>
    );
}
