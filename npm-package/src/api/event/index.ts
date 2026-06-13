/**
 * Event API module for Tauri Remote UI
 *
 * This module handles listening to events from the Tauri application via WebSocket
 */
import {
    EventCallback,
    EventName,
    Options,
    listen as TauriListen,
    UnlistenFn,
} from '@tauri-apps/api/event';
import { hasTauriRuntime, initWebSocket, listenEvent, wsReady } from '../../socket';
export type { UnlistenFn } from '@tauri-apps/api/event';
export { latencyMs } from '../../socket';

/**
 * Listen to events from the Tauri application.
 * Falls back to a WebSocket transport if the Tauri Event API is not available.
 *
 * @param event - The event name to listen for
 * @param handler - Callback to handle the event
 * @param options - Options for the event listener
 */
export async function listen<T>(
    event: EventName,
    handler: EventCallback<T>,
    options?: Options,
): Promise<UnlistenFn> {
    if (hasTauriRuntime()) {
        return await TauriListen(event, handler, options);
    }
    initWebSocket();
    if (wsReady) {
        await wsReady;
    }
    const messageHandler = (e: Event) => {
        handler((e as MessageEvent).data);
    };
    listenEvent.addEventListener(event, messageHandler);
    return () => {
        listenEvent.removeEventListener(event, messageHandler);
    };
}

