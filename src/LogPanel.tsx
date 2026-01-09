import { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

interface LogMessage {
    timestamp: string;
    message: string;
}

export default function LogPanel() {
    const [logs, setLogs] = useState<LogMessage[]>([]);
    const endRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const unlisten = listen<string>("log-message", (event) => {
            addLog(event.payload);
        });

        // Add a global function for frontend to log easily
        (window as any).logToPanel = (msg: string) => {
            addLog(msg);
        };

        return () => {
            unlisten.then(f => f());
            delete (window as any).logToPanel;
        };
    }, []);

    const addLog = (msg: string) => {
        const timestamp = new Date().toLocaleTimeString();
        setLogs(prev => [...prev.slice(-49), { timestamp, message: msg }]); // Keep last 50
    };

    useEffect(() => {
        endRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [logs]);

    return (
        <div className="log-panel" style={{
            position: 'fixed',
            bottom: 0,
            left: 0,
            right: 0,
            height: '150px',
            background: '#222',
            borderTop: '1px solid #444',
            overflowY: 'auto',
            padding: '10px',
            fontFamily: 'monospace',
            fontSize: '0.85em',
            textAlign: 'left',
            zIndex: 1000
        }}>
            {logs.map((l, i) => (
                <div key={i} style={{ borderBottom: '1px solid #333', padding: '2px 0' }}>
                    <span style={{ color: '#888', marginRight: '8px' }}>[{l.timestamp}]</span>
                    <span style={{ color: '#ddd' }}>{l.message}</span>
                </div>
            ))}
            <div ref={endRef} />
        </div>
    );
}

// Helper for other components
export const frontendLog = (msg: string) => {
    if ((window as any).logToPanel) {
        (window as any).logToPanel(msg);
    } else {
        console.log("[LogPanel Not Ready]", msg);
    }
};
