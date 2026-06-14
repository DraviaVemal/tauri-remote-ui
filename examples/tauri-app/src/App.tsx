import React, { useEffect, useState } from 'react';
import { invoke } from "tauri-remote-ui/api/core";
import { listen, latencyMs } from "tauri-remote-ui/api/event";
import './styles/App.css';

const App: React.FC = () => {

    let [counter, setCounter] = useState(0);
    const [latency, setLatency] = useState(latencyMs);

    // True when running inside the Tauri desktop webview, false when loaded
    // through the Remote UI server (browser / mobile companion).
    const isDesktop = typeof window !== "undefined" &&
        Boolean((window as any).__TAURI_INTERNALS__ || (window as any).__TAURI__);

    useEffect(() => {
        const interval = setInterval(() => {
            setLatency(latencyMs);
        }, 500); // update every 500ms
        return () => clearInterval(interval);
    }, []);

    useEffect(() => {
        lstn();
    }, []);

    const lstn = async () => {
        try {
            await listen("counter", (event) => {
                setCounter((event.payload as any).result);
            })
        } catch (err) {
            console.log("Main App : ", err)
        }
    }
    const latencyClass =
        latency < 50 ? "" : latency < 200 ? "warn" : "bad";

    const call = async (cmd: string) => {
        try {
            const res = await invoke(cmd);
            console.log(res);
        } catch (e) {
            console.log("Timeout", e);
        }
    };

    return (
        <div className="App">
            <header className="App-header">
                <div className="brand">
                    <div className="brand-mark">T</div>
                    <div className="brand-meta">
                        <h1>tauri-remote-ui</h1>
                        <h2>Sample app to test the Remote UI plugin</h2>
                    </div>
                </div>

                <div className="divider" />

                <div className="result-card">
                    <span className="label">Counter</span>
                    <span className="value">{counter}</span>
                </div>

                <div className="section">
                    <p className="section-title">Counter actions</p>
                    <div className="btn-row">
                        <button className="btn-primary" onClick={() => call("increment")}>
                            Increment
                        </button>
                        <button className="btn-warning" onClick={() => call("decrement")}>
                            Decrement
                        </button>
                    </div>
                </div>

                <div className="section">
                    <p className="section-title">Remote UI server</p>
                    <div className="btn-row">
                        {isDesktop ? (
                            <button className="btn-success" onClick={() => call("enable_server")}>
                                Start
                            </button>
                        ) : (
                            <button className="btn-warning" onClick={() => call("disable_server")}>
                                Stop
                            </button>
                        )}
                        <button className="btn-danger" onClick={() => call("exit_app")}>
                            Exit App
                        </button>
                    </div>
                </div>

                <div className="footer">
                    <span>Live connection</span>
                    <span className="latency">
                        <span className={`dot ${latencyClass}`} />
                        {latency} ms
                    </span>
                </div>
            </header>
        </div>
    );
};

export default App;