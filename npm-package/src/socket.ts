/**
 * Tauri Remote UI - API
 * 
 * This TypeScript file serves as the main entry point for the tauri-remote-ui package.
 * It provides WebSocket initialization and shared WebSocket state for communicating
 * with a Tauri application.
 */

export let ws: WebSocket | null = null;
export let listenEvent: EventTarget = new EventTarget();
export let wsReady: Promise<void> | null = null;
export const filterCollection: {
    [msg_id: string]: (response: any) => any
} = {};
export let latencyMs: number = 0;

/**
 * Get the WebSocket URL based on the current window location
 */
function getWsUrl(): string {
    const loc = window.location;
    const proto = loc.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${proto}//${loc.host}/remote_ui_ws`;
    return wsUrl;
}

function getUrl(): string {
    const loc = window.location;
    const wsUrl = `${loc.protocol}//${loc.host}/remote_ui_disconnect`;
    return wsUrl;
}

/**
 * Initialize the WebSocket connection
 * This should be called once at the start of your application
 */
export function initWebSocket(): void {
    try {
        // If we're in a Tauri app, don't use WebSocket
        if (((window as any).__TAURI_INTERNALS__ && (window as any).__TAURI_INTERNALS__.invoke) ||
            (window as any).__TAURI__ && (window as any).__TAURI__.invoke) {
            return
        } else {
            throw new Error("Moving to WS backup for Tauri Backend")
        }
    } catch {
        if (ws) return;
        console.info("Tauri-Remote-UI : Remote RPC Attempting...");
        const wsUrl = getWsUrl();
        try {
            let lastPingTimestamp = Date.now();
            let pingPongTimer: NodeJS.Timeout;
            ws = new WebSocket(wsUrl);
            wsReady = new Promise((resolve, reject) => {
                ws!.onopen = () => {
                    console.info("Tauri-Remote-UI : Remote Connected.");
                    lastPingTimestamp = Date.now();
                    ws?.send("ping");
                    pingPongTimer = setInterval(() => {
                        lastPingTimestamp = Date.now();
                        ws?.send("ping");
                    }, 10000);
                    resolve();
                };
                ws!.onmessage = ({ data }) => {
                    if (data === "pong") {
                        latencyMs = Date.now() - lastPingTimestamp;
                        if (latencyMs > 200) {
                            console.warn(`Tauri-Remote-UI : High latency detected - ${latencyMs}ms`);
                        }
                        return;
                    }
                    let jsonData = JSON.parse(data);
                    if (jsonData.id && filterCollection[jsonData.id]) {
                        filterCollection[jsonData.id](JSON.parse(jsonData.payload))
                    } else {
                        listenEvent.dispatchEvent(new MessageEvent(jsonData.event, { data: jsonData }));
                    }
                };
                ws!.onclose = () => {
                    ws = null;
                    wsReady = null;
                    pingPongTimer && clearInterval(pingPongTimer);
                    console.info("Tauri-Remote-UI : Remote DisConnected.");
                    window.location.href = getUrl();
                };
                ws!.onerror = (e) => {
                    reject(e);
                };
            });
        } catch (e) {
            console.error(e);
        }
    }
}
