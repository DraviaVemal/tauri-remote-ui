import React, { useEffect, useState } from 'react';
import { invoke } from "tauri-remote-ui/api/core";
import { listen, latencyMs } from "tauri-remote-ui/api/event";
import './styles/App.css';

const App: React.FC = () => {

    let [counter, setCounter] = useState(0);
    const [latency, setLatency] = useState(latencyMs);

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
    return (
        <div className="App">
            <header className="App-header">
                <h1>tauri-remote-ui</h1>
                <h2>Tauri Sample App to Test Remote UI Plugin</h2>
                <h3>{counter}</h3>
                <div className='counter'>
                    <button onClick={async () => {
                        try {
                            let test = await invoke("increment");
                            console.log(test)
                        } catch (e) {
                            console.log("Timeout", e);
                        }
                    }}>Increment</button>
                    <button onClick={async () => {
                        try {
                            let test = await invoke("decrement");
                            console.log(test)
                        } catch (e) {
                            console.log("Timeout", e);
                        }
                    }}>Decrement</button>
                </div>
                <div className='control'>
                    <button onClick={async () => {
                        try {
                            let test = await invoke("enable_server");
                            console.log(test)
                        } catch (e) {
                            console.log("Timeout", e);
                        }
                    }}>Start Remote UI</button>
                    <button onClick={async () => {
                        let test = await invoke("disable_server");
                        console.log(test)
                    }}>Stop Remote UI</button>
                    <button onClick={async () => {
                        let test = await invoke("exit_app");
                        console.log(test)
                    }}>Exit</button>
                </div>
            </header>
            <h4>Latency: {latency} ms</h4>
        </div >
    );
};

export default App;