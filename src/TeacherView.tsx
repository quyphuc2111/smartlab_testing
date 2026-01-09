import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { frontendLog } from "./LogPanel";

interface DisplayInfo {
    index: number;
    width: number;
    height: number;
    name: string;
}

// Quality presets matching Requirements 7.1
type QualityPreset = "low" | "medium" | "high";

const QUALITY_PRESETS: Record<QualityPreset, { label: string; bitrate: number; quality: number }> = {
    low: { label: "Low (500 kbps)", bitrate: 500, quality: 20 },
    medium: { label: "Medium (2 Mbps)", bitrate: 2000, quality: 50 },
    high: { label: "High (5 Mbps)", bitrate: 5000, quality: 80 },
};

// FPS options matching Requirements 7.3
const FPS_OPTIONS = [5, 10, 15, 30, 60];

export default function TeacherView() {
    const [isSharing, setIsSharing] = useState(false);
    const [port, setPort] = useState(8080);
    const [fps, setFps] = useState(15);
    const [qualityPreset, setQualityPreset] = useState<QualityPreset>("medium");
    const [serverName, setServerName] = useState("");
    const [displays, setDisplays] = useState<DisplayInfo[]>([]);
    const [selectedDisplay, setSelectedDisplay] = useState(0);
    const [clientCount, setClientCount] = useState(0);
    const [stats, setStats] = useState({ fps: 0, bitrate: 0 });

    useEffect(() => {
        // Load available displays
        invoke<DisplayInfo[]>("get_displays")
            .then(setDisplays)
            .catch((e) => frontendLog(`Error getting displays: ${e}`));
    }, []);

    // Listen for client count updates and streaming stats
    useEffect(() => {
        const unlistenClientCount = listen<number>("client-count", (event) => {
            setClientCount(event.payload);
        });

        return () => {
            unlistenClientCount.then(f => f());
        };
    }, []);

    const startSharing = async () => {
        try {
            frontendLog(`Starting Server on port ${port}...`);
            // Pass server name if provided, otherwise let backend use hostname
            const name = serverName.trim() || null;
            await invoke("start_server_cmd", { port, name });

            const quality = QUALITY_PRESETS[qualityPreset].quality;
            frontendLog(`Starting capture on display ${selectedDisplay} with ${qualityPreset} quality (${QUALITY_PRESETS[qualityPreset].bitrate} kbps)...`);
            await invoke("start_capture", {
                displayIndex: selectedDisplay,
                fps,
                quality,
            });

            setIsSharing(true);
            setStats({ fps, bitrate: QUALITY_PRESETS[qualityPreset].bitrate });
            frontendLog("Sharing started.");
        } catch (err) {
            console.error("Error starting share:", err);
            frontendLog(`Error: ${err}`);
            alert("Failed to start sharing: " + err);
        }
    };

    const stopSharing = async () => {
        frontendLog("Stopping share...");
        await invoke("stop_capture");
        await invoke("stop_server_cmd");
        setIsSharing(false);
        setClientCount(0);
        setStats({ fps: 0, bitrate: 0 });
    };

    return (
        <div className="card">
            <h2>Teacher Mode</h2>

            {!isSharing ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', alignItems: 'center' }}>
                    <div>
                        <label>Server Name: </label>
                        <input
                            type="text"
                            value={serverName}
                            onChange={(e) => setServerName(e.target.value)}
                            placeholder="e.g., Math Class Room 101"
                            style={{ padding: '5px', width: '200px' }}
                        />
                    </div>
                    <div>
                        <label>Display: </label>
                        <select 
                            value={selectedDisplay} 
                            onChange={(e) => setSelectedDisplay(Number(e.target.value))}
                            style={{ padding: '5px' }}
                        >
                            {displays.map((d) => (
                                <option key={d.index} value={d.index}>
                                    {d.name} ({d.width}x{d.height})
                                </option>
                            ))}
                        </select>
                    </div>
                    <div>
                        <label>Port: </label>
                        <input
                            type="number"
                            value={port}
                            onChange={(e) => setPort(Number(e.target.value))}
                            style={{ padding: '5px', width: '80px' }}
                        />
                    </div>
                    <div>
                        <label>Quality: </label>
                        <select
                            value={qualityPreset}
                            onChange={(e) => setQualityPreset(e.target.value as QualityPreset)}
                            style={{ padding: '5px' }}
                        >
                            {(Object.keys(QUALITY_PRESETS) as QualityPreset[]).map((preset) => (
                                <option key={preset} value={preset}>
                                    {QUALITY_PRESETS[preset].label}
                                </option>
                            ))}
                        </select>
                    </div>
                    <div>
                        <label>FPS: </label>
                        <select
                            value={fps}
                            onChange={(e) => setFps(Number(e.target.value))}
                            style={{ padding: '5px' }}
                        >
                            {FPS_OPTIONS.map((f) => (
                                <option key={f} value={f}>
                                    {f} FPS
                                </option>
                            ))}
                        </select>
                    </div>
                    <button onClick={startSharing}>Start Sharing</button>
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
                            <span style={{ color: 'green', fontWeight: 'bold' }}>● Streaming on Port {port}</span>
                            <span style={{ color: '#4CAF50' }}>
                                {clientCount} {clientCount === 1 ? 'client' : 'clients'} connected
                            </span>
                        </div>
                        <div style={{ display: 'flex', gap: '20px', fontSize: '14px', color: '#888' }}>
                            <span>Quality: {QUALITY_PRESETS[qualityPreset].label}</span>
                            <span>FPS: {stats.fps}</span>
                            <span>Bitrate: {stats.bitrate} kbps</span>
                        </div>
                    </div>
                    <button onClick={stopSharing} style={{ backgroundColor: '#cc3333' }}>Stop Sharing</button>
                </div>
            )}
        </div>
    );
}
