import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { frontendLog } from "./LogPanel";

interface ServerInfo {
    ip: string;
    port: number;
}

export default function StudentView() {
    const [servers, setServers] = useState<ServerInfo[]>([]);
    const [connected, setConnected] = useState(false);
    const videoRef = useRef<HTMLVideoElement>(null);
    const mediaSourceRef = useRef<MediaSource | null>(null);
    const sourceBufferRef = useRef<SourceBuffer | null>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const queueRef = useRef<Uint8Array[]>([]);

    useEffect(() => {
        // Start Discovery
        frontendLog("Starting Discovery...");
        invoke("start_discovery").catch(console.error);

        // Listen for events
        const unlisten = listen<ServerInfo>("server-found", (event) => {
            setServers((prev) => {
                const exists = prev.find(s => s.ip === event.payload.ip && s.port === event.payload.port);
                if (exists) return prev;
                return [...prev, event.payload];
            });
        });

        return () => {
            unlisten.then(f => f());
        };
    }, []);

    const connectToServer = (ip: string, port: number) => {
        if (connected) return;

        frontendLog(`Connecting to ws://${ip}:${port}...`);
        const ws = new WebSocket(`ws://${ip}:${port}`);
        ws.binaryType = "arraybuffer";
        wsRef.current = ws;

        const mediaSource = new MediaSource();
        mediaSourceRef.current = mediaSource;
        if (videoRef.current) {
            videoRef.current.src = URL.createObjectURL(mediaSource);
        }

        mediaSource.addEventListener("sourceopen", () => {
            try {
                // Hardcoded codec as in TeacherView. Ideally should negotiate.
                const sb = mediaSource.addSourceBuffer("video/webm; codecs=vp9");
                sourceBufferRef.current = sb;
                sb.mode = "sequence";

                sb.addEventListener("updateend", () => {
                    processQueue();
                });

                sb.addEventListener("error", (e) => {
                    frontendLog("SourceBuffer Error: " + JSON.stringify(e));
                });
                frontendLog("MediaSource opened. SourceBuffer created.");
            } catch (e) {
                console.error("Error creating SourceBuffer:", e);
                frontendLog("Error creating SourceBuffer: " + String(e));
                alert("Error: Codec not supported or MediaSource issue.");
            }
        });

        mediaSource.addEventListener("sourceclose", () => {
            frontendLog("MediaSource closed.");
        });

        ws.onopen = () => {
            console.log("Connected to server");
            frontendLog("WebSocket Connected.");
            setConnected(true);
        };

        ws.onmessage = async (event) => {
            const data = new Uint8Array(event.data);
            if (data.length > 0) {
                if (Math.random() < 0.05) frontendLog(`Received Chunk (${data.length} bytes)`);
                queueRef.current.push(data);
                processQueue();
            }
        };

        ws.onclose = () => {
            setConnected(false);
        };
    };

    const processQueue = () => {
        const sb = sourceBufferRef.current;
        if (sb && !sb.updating && queueRef.current.length > 0) {
            try {
                const data = queueRef.current.shift();
                if (data) sb.appendBuffer(data);
            } catch (e) {
                console.error("Append error:", e);
            }
        }
    };

    const disconnect = () => {
        if (wsRef.current) wsRef.current.close();
        setConnected(false);
        setServers([]);
        invoke("start_discovery").catch(console.error);
    };

    return (
        <div className="card">
            <h2>Student Mode</h2>
            {!connected ? (
                <div>
                    <h3>Available Servers:</h3>
                    {servers.length === 0 && <p>Scanning...</p>}
                    {servers.map((s) => (
                        <div key={`${s.ip}:${s.port}`} className="server-item">
                            <span>{s.ip}:{s.port}</span>
                            <button onClick={() => connectToServer(s.ip, s.port)}>Connect</button>
                        </div>
                    ))}
                    <div style={{ marginTop: '20px' }}>
                        <p>Manual Connect:</p>
                        <input id="manual-ip" placeholder="IP" style={{ marginRight: '5px' }} />
                        <input id="manual-port" placeholder="Port" type="number" defaultValue={8080} style={{ marginRight: '5px' }} />
                        <button onClick={() => {
                            const ip = (document.getElementById('manual-ip') as HTMLInputElement).value;
                            const port = (document.getElementById('manual-port') as HTMLInputElement).value;
                            if (ip && port) connectToServer(ip, Number(port));
                        }}>Connect</button>
                    </div>
                </div>
            ) : (
                <div>
                    <button onClick={disconnect} style={{ marginBottom: '10px' }}>Disconnect</button>
                    <video
                        ref={videoRef}
                        autoPlay
                        controls
                        style={{ width: '100%', border: '1px solid #666' }}
                    />
                </div>
            )}
        </div>
    );
}
