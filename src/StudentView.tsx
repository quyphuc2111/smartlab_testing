import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { frontendLog } from "./LogPanel";

interface ServerInfo {
    ip: string;
    port: number;
    name: string;
}

interface DecodedFrameData {
    width: number;
    height: number;
    data: number[];
    frame_id: number;
    is_keyframe: boolean;
}

export default function StudentView() {
    const [servers, setServers] = useState<ServerInfo[]>([]);
    const [receiving, setReceiving] = useState(false);
    const [selectedServer, setSelectedServer] = useState<ServerInfo | null>(null);
    const [stats, setStats] = useState({ fps: 0, frameCount: 0, resolution: '' });
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const frameCountRef = useRef(0);
    const lastFpsUpdateRef = useRef(Date.now());
    const fpsFrameCountRef = useRef(0);

    useEffect(() => {
        frontendLog("Starting Discovery...");
        invoke("start_discovery").catch(console.error);

        const unlisten = listen<ServerInfo>("server-found", (event) => {
            frontendLog(`Discovered Server: ${event.payload.name} at ${event.payload.ip}:${event.payload.port}`);
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

    const startReceiving = async (server: ServerInfo) => {
        try {
            frontendLog(`Connecting to ${server.ip}:${server.port}...`);
            setSelectedServer(server);
            await invoke("start_tcp_video_receiver", { ip: server.ip, port: server.port });
            setReceiving(true);
            frontendLog("Connected! Waiting for frames...");
        } catch (e) {
            frontendLog(`Error: ${e}`);
            setSelectedServer(null);
        }
    };

    const stopReceiving = async () => {
        await invoke("stop_tcp_video_receiver");
        setReceiving(false);
        setSelectedServer(null);
        setServers([]);
        frameCountRef.current = 0;
        fpsFrameCountRef.current = 0;
        setStats({ fps: 0, frameCount: 0, resolution: '' });
        invoke("start_discovery").catch(console.error);
    };

    // Listen for decoded VP9 frames (RGBA data)
    useEffect(() => {
        if (!receiving) return;

        const unlisten = listen<DecodedFrameData>("decoded-frame", (event) => {
            const frame = event.payload;
            frameCountRef.current++;
            fpsFrameCountRef.current++;
            
            // Update FPS every second
            const now = Date.now();
            if (now - lastFpsUpdateRef.current >= 1000) {
                const fps = fpsFrameCountRef.current;
                fpsFrameCountRef.current = 0;
                lastFpsUpdateRef.current = now;
                setStats(prev => ({ 
                    ...prev, 
                    fps, 
                    frameCount: frameCountRef.current 
                }));
            }
            
            // Draw RGBA data to canvas
            const canvas = canvasRef.current;
            if (canvas) {
                // Resize canvas if needed
                if (canvas.width !== frame.width || canvas.height !== frame.height) {
                    canvas.width = frame.width;
                    canvas.height = frame.height;
                    setStats(prev => ({ 
                        ...prev, 
                        resolution: `${frame.width}x${frame.height}` 
                    }));
                    frontendLog(`Canvas resized to ${frame.width}x${frame.height}`);
                }
                
                const ctx = canvas.getContext('2d');
                if (ctx) {
                    // Create ImageData from RGBA array
                    const imageData = ctx.createImageData(frame.width, frame.height);
                    const rgbaData = new Uint8ClampedArray(frame.data);
                    imageData.data.set(rgbaData);
                    ctx.putImageData(imageData, 0, 0);
                }
            }
            
            if (frameCountRef.current % 60 === 1) {
                frontendLog(`Frame #${frame.frame_id} (${frame.width}x${frame.height}, keyframe: ${frame.is_keyframe})`);
            }
        });

        // Listen for receiver stopped event
        const unlistenStopped = listen("receiver-stopped", () => {
            frontendLog("Receiver stopped");
            setReceiving(false);
            setSelectedServer(null);
        });

        return () => {
            unlisten.then(f => f());
            unlistenStopped.then(f => f());
        };
    }, [receiving]);

    return (
        <div className="card">
            <h2>Student Mode</h2>
            {!receiving ? (
                <div>
                    <h3>Available Servers:</h3>
                    {servers.length === 0 && <p>Scanning for teachers...</p>}
                    {servers.map((s) => (
                        <div key={`${s.ip}:${s.port}`} className="server-item" style={{ 
                            margin: '5px 0',
                            padding: '10px',
                            border: '1px solid #444',
                            borderRadius: '4px',
                            display: 'flex',
                            justifyContent: 'space-between',
                            alignItems: 'center'
                        }}>
                            <div>
                                <div style={{ fontWeight: 'bold' }}>{s.name}</div>
                                <div style={{ fontSize: '12px', color: '#888' }}>{s.ip}:{s.port}</div>
                            </div>
                            <button 
                                onClick={() => startReceiving(s)}
                                style={{ marginLeft: '10px' }}
                            >
                                Connect
                            </button>
                        </div>
                    ))}
                    
                    <p style={{ marginTop: '10px', fontSize: '12px', color: '#888' }}>
                        Click "Connect" to start receiving video from a teacher.
                    </p>
                </div>
            ) : (
                <div>
                    <div style={{ 
                        display: 'flex', 
                        flexDirection: 'column',
                        gap: '8px',
                        marginBottom: '15px',
                        padding: '10px',
                        backgroundColor: '#1a1a1a',
                        borderRadius: '8px'
                    }}>
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                            <span style={{ color: 'green', fontWeight: 'bold' }}>
                                ● Connected to: {selectedServer?.name}
                            </span>
                            <button 
                                onClick={stopReceiving} 
                                style={{ backgroundColor: '#cc3333', padding: '5px 15px' }}
                            >
                                Disconnect
                            </button>
                        </div>
                        <div style={{ fontSize: '12px', color: '#888' }}>
                            {selectedServer?.ip}:{selectedServer?.port}
                        </div>
                        <div style={{ display: 'flex', gap: '20px', fontSize: '14px', color: '#888' }}>
                            <span style={{ color: '#4CAF50' }}>{stats.fps} FPS</span>
                            {stats.resolution && <span>Resolution: {stats.resolution}</span>}
                            <span>Frames: {stats.frameCount}</span>
                        </div>
                    </div>
                    <canvas
                        ref={canvasRef}
                        style={{ 
                            width: '100%', 
                            border: '1px solid #666', 
                            backgroundColor: '#000',
                            maxHeight: '70vh',
                            objectFit: 'contain'
                        }}
                    />
                </div>
            )}
        </div>
    );
}
