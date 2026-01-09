import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { frontendLog } from "./LogPanel";

interface DisplayInfo {
    index: number;
    width: number;
    height: number;
    name: string;
}

export default function TeacherView() {
    const [isSharing, setIsSharing] = useState(false);
    const [port, setPort] = useState(8080);
    const [fps, setFps] = useState(15);
    const [quality, setQuality] = useState(70);
    const [displays, setDisplays] = useState<DisplayInfo[]>([]);
    const [selectedDisplay, setSelectedDisplay] = useState(0);

    useEffect(() => {
        // Load available displays
        invoke<DisplayInfo[]>("get_displays")
            .then(setDisplays)
            .catch((e) => frontendLog(`Error getting displays: ${e}`));
    }, []);

    const startSharing = async () => {
        try {
            frontendLog(`Starting Server on port ${port}...`);
            await invoke("start_server_cmd", { port });

            frontendLog(`Starting capture on display ${selectedDisplay}...`);
            await invoke("start_capture", {
                displayIndex: selectedDisplay,
                fps,
                quality,
            });

            setIsSharing(true);
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
    };

    return (
        <div className="card">
            <h2>Teacher Mode</h2>

            {!isSharing ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', alignItems: 'center' }}>
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
                        <label>FPS: </label>
                        <input
                            type="number"
                            value={fps}
                            onChange={(e) => setFps(Number(e.target.value))}
                            min={1}
                            max={60}
                            style={{ padding: '5px', width: '60px' }}
                        />
                    </div>
                    <div>
                        <label>Quality: </label>
                        <input
                            type="range"
                            value={quality}
                            onChange={(e) => setQuality(Number(e.target.value))}
                            min={10}
                            max={100}
                            style={{ width: '100px' }}
                        />
                        <span> {quality}%</span>
                    </div>
                    <button onClick={startSharing}>Start Sharing</button>
                </div>
            ) : (
                <div>
                    <p style={{ color: 'green' }}>Streaming on Port {port}...</p>
                    <button onClick={stopSharing} style={{ backgroundColor: '#cc3333' }}>Stop Sharing</button>
                </div>
            )}
        </div>
    );
}
