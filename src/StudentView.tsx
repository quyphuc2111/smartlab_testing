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
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const wsRef = useRef<WebSocket | null>(null);

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

    const connectToServer = (ip: string, port: number) => {
        if (connected) return;

        frontendLog(`Connecting to ws://${ip}:${port}...`);
        const ws = new WebSocket(`ws://${ip}:${port}`);
        ws.binaryType = "arraybuffer";
        wsRef.current = ws;

        ws.onopen = () => {
            frontendLog("WebSocket Connected.");
            setConnected(true);
        };

        ws.onmessage = async (event) => {
            const data = new Uint8Array(event.data);
            if (data.length > 0) {
                // Decode JPEG and draw to canvas
                const blob = new Blob([data], { type: 'image/jpeg' });
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
            }
        };

        ws.onclose = () => {
            frontendLog("WebSocket Disconnected.");
            setConnected(false);
        };

        ws.onerror = (e) => {
            frontendLog(`WebSocket Error: ${e}`);
        };
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
                        <div key={`${s.ip}:${s.port}`} className="server-item" style={{ margin: '5px 0' }}>
                            <span>{s.ip}:{s.port}</span>
                            <button onClick={() => connectToServer(s.ip, s.port)} style={{ marginLeft: '10px' }}>
                                Connect
                            </button>
                        </div>
                    ))}
                    <div style={{ marginTop: '20px' }}>
                        <p>Manual Connect:</p>
                        <input id="manual-ip" placeholder="IP" style={{ marginRight: '5px' }} />
                        <input id="manual-port" placeholder="Port" type="number" defaultValue={8080} style={{ marginRight: '5px', width: '80px' }} />
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
                    <canvas
                        ref={canvasRef}
                        style={{ width: '100%', border: '1px solid #666', backgroundColor: '#000' }}
                    />
                </div>
            )}
        </div>
    );
}
