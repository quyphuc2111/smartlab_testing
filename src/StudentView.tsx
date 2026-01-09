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
    const [receiving, setReceiving] = useState(false);
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const frameCountRef = useRef(0);

    useEffect(() => {
        frontendLog("Starting Discovery...");
        invoke("start_discovery").catch(console.error);

        const unlisten = listen<ServerInfo>("server-found", (event) => {
            frontendLog(`Discovered Server at ${event.payload.ip}:${event.payload.port}`);
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

    const startReceiving = async () => {
        try {
            frontendLog("Starting video receiver...");
            await invoke("start_video_receiver");
            setReceiving(true);
            frontendLog("Video receiver started. Waiting for frames...");
        } catch (e) {
            frontendLog(`Error: ${e}`);
        }
    };

    const stopReceiving = async () => {
        await invoke("stop_video_receiver");
        setReceiving(false);
        setServers([]);
        frameCountRef.current = 0;
        invoke("start_discovery").catch(console.error);
    };

    // Listen for video frames
    useEffect(() => {
        if (!receiving) return;

        const unlisten = listen<number[]>("video-frame", (event) => {
            frameCountRef.current++;
            const frameData = new Uint8Array(event.payload);
            
            // Decode JPEG and draw to canvas
            const blob = new Blob([frameData], { type: 'image/jpeg' });
            const url = URL.createObjectURL(blob);
            const img = new Image();
            img.onload = () => {
                const canvas = canvasRef.current;
                if (canvas) {
                    canvas.width = img.width;
                    canvas.height = img.height;
                    const ctx = canvas.getContext('2d');
                    if (ctx) {
                        ctx.drawImage(img, 0, 0);
                    }
                }
                URL.revokeObjectURL(url);
            };
            img.src = url;
            
            if (frameCountRef.current % 30 === 1) {
                frontendLog(`Received frame #${frameCountRef.current}`);
            }
        });

        return () => {
            unlisten.then(f => f());
        };
    }, [receiving]);

    return (
        <div className="card">
            <h2>Student Mode</h2>
            {!receiving ? (
                <div>
                    <h3>Available Servers:</h3>
                    {servers.length === 0 && <p>Scanning...</p>}
                    {servers.map((s) => (
                        <div key={`${s.ip}:${s.port}`} className="server-item" style={{ margin: '5px 0' }}>
                            <span>{s.ip}:{s.port}</span>
                        </div>
                    ))}
                    
                    {servers.length > 0 && (
                        <button onClick={startReceiving} style={{ marginTop: '15px' }}>
                            Start Receiving
                        </button>
                    )}
                    
                    <p style={{ marginTop: '10px', fontSize: '12px', color: '#888' }}>
                        Video is broadcast via UDP multicast - just click "Start Receiving" when a server is found.
                    </p>
                </div>
            ) : (
                <div>
                    <button onClick={stopReceiving} style={{ marginBottom: '10px', backgroundColor: '#cc3333' }}>
                        Stop Receiving
                    </button>
                    <canvas
                        ref={canvasRef}
                        style={{ width: '100%', border: '1px solid #666', backgroundColor: '#000' }}
                    />
                </div>
            )}
        </div>
    );
}
