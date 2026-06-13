/**
 * Tauri Remote UI - WebSocket bridge
 *
 * Establishes (and re-uses) a WebSocket connection back to the Tauri host
 * application, providing the transport for the `invoke` and `listen` shims
 * exported from `./api/core` and `./api/event`.
 */

/**
 * Status discriminator on the response payload sent back from the Rust side.
 *
 * Mirrors the `RpcStatus` enum in `src/models.rs` — keep both in sync.
 */
export const RpcStatus = {
    Success: 'success',
    Error: 'error',
} as const;
export type RpcStatus = typeof RpcStatus[keyof typeof RpcStatus];

/** Shape of the response payload returned for a single RPC call. */
export interface RpcResponse<T = unknown> {
    status: RpcStatus;
    payload: T;
}

/** Callback type stored per outstanding RPC request id. */
export type RpcResponseHandler = (response: RpcResponse) => void;

/** Subset of `window` properties this module touches, typed to avoid `any`. */
interface TauriGlobals {
    __TAURI_INTERNALS__?: { invoke?: unknown };
    __TAURI__?: { invoke?: unknown };
}

/** Returns true if the page is running inside a Tauri webview. */
export function hasTauriRuntime(): boolean {
    const w = window as unknown as TauriGlobals;
    return Boolean(
        (w.__TAURI_INTERNALS__ && w.__TAURI_INTERNALS__.invoke) ||
        (w.__TAURI__ && w.__TAURI__.invoke)
    );
}

export let ws: WebSocket | null = null;
export const listenEvent: EventTarget = new EventTarget();
export let wsReady: Promise<void> | null = null;
export const filterCollection: Record<number, RpcResponseHandler> = {};
export let latencyMs: number = 0;

/** Build the WebSocket URL for the RPC connection. */
function getWsUrl(): string {
    const loc = window.location;
    const proto = loc.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${proto}//${loc.host}/remote_ui_ws`;
}

/** Build the disconnect-redirect URL the page navigates to on close. */
function getDisconnectUrl(): string {
    const loc = window.location;
    return `${loc.protocol}//${loc.host}/remote_ui_disconnect`;
}

/**
 * Initialize the WebSocket connection on first use. A no-op when running
 * inside Tauri (native IPC is preferred) or when the socket is already open.
 */
export function initWebSocket(): void {
    if (hasTauriRuntime()) {
        return;
    }
    if (ws) {
        return;
    }
    console.info('Tauri-Remote-UI : Remote RPC Attempting...');
    const wsUrl = getWsUrl();
    try {
        let lastPingTimestamp = Date.now();
        let pingPongTimer: ReturnType<typeof setInterval> | undefined;
        const socket = new WebSocket(wsUrl);
        ws = socket;
        wsReady = new Promise<void>((resolve, reject) => {
            socket.onopen = () => {
                console.info('Tauri-Remote-UI : Remote Connected.');
                lastPingTimestamp = Date.now();
                socket.send('ping');
                pingPongTimer = setInterval(() => {
                    lastPingTimestamp = Date.now();
                    socket.send('ping');
                }, 10000);
                resolve();
            };
            socket.onmessage = ({ data }) => {
                if (data === 'pong') {
                    latencyMs = Date.now() - lastPingTimestamp;
                    if (latencyMs > 200) {
                        console.warn(
                            `Tauri-Remote-UI : High latency detected - ${latencyMs}ms`
                        );
                    }
                    return;
                }
                let jsonData: { id?: number; event?: string; payload?: string };
                try {
                    jsonData = JSON.parse(data);
                } catch (err) {
                    console.warn('Tauri-Remote-UI : Failed to parse message', err);
                    return;
                }
                if (typeof jsonData.id === 'number' && filterCollection[jsonData.id]) {
                    try {
                        const parsed: RpcResponse = JSON.parse(jsonData.payload ?? 'null');
                        filterCollection[jsonData.id](parsed);
                    } catch (err) {
                        console.warn('Tauri-Remote-UI : Failed to parse RPC payload', err);
                    }
                } else if (jsonData.event) {
                    listenEvent.dispatchEvent(
                        new MessageEvent(jsonData.event, { data: jsonData })
                    );
                }
            };
            socket.onclose = () => {
                ws = null;
                wsReady = null;
                if (pingPongTimer !== undefined) {
                    clearInterval(pingPongTimer);
                }
                console.info('Tauri-Remote-UI : Remote Disconnected.');
                window.location.href = getDisconnectUrl();
            };
            socket.onerror = (e) => {
                reject(e);
            };
        });
    } catch (e) {
        console.error(e);
    }
}

