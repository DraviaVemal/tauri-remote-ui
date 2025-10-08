/**
 * Tauri Remote UI - API
 * 
 * This TypeScript file serves as the main entry point for the tauri-remote-ui package.
 * It provides WebSocket initialization and shared WebSocket state for communicating
 * with a Tauri application.
 */

export let ws: WebSocket | null = null;
export let wsReady: Promise<void> | null = null;
export const filterCollection: {
    [msg_id: string]: (response: any) => any
} = {};
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
        console.info("Remote RPC Attempting...");
        const wsUrl = getWsUrl();
        try {
            ws = new WebSocket(wsUrl);
            wsReady = new Promise((resolve, reject) => {
                ws!.onopen = () => {
                    console.info("Remote Connected.");
                    resolve();
                };
                ws!.onclose = () => {
                    ws = null;
                    wsReady = null;
                    console.info("Remote Dis-Connected.");
                    window.location.replace(getUrl());
                };
                ws!.onerror = (e) => {
                    reject(e);
                };
                ws!.onmessage = ({ data }) => {
                    let json_data = JSON.parse(data);
                    json_data.id && filterCollection[json_data.id] && filterCollection[json_data.id](JSON.parse(json_data.payload))
                };
            });
        } catch (e) {
            setTimeout(() => {
                initWebSocket();
            }, 5000);
        }
    }
}
