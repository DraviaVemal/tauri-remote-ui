/**
 * Event API module for Tauri Remote UI
 * 
 * This module handles listening to events from the Tauri application via WebSocket
 */
import { EventCallback, EventName, Options, listen as TauriListen, UnlistenFn } from '@tauri-apps/api/event';
import { initWebSocket, ws, wsReady } from '../../socket';

export type { UnlistenFn } from '@tauri-apps/api/event';

/**
 * Listen to events from the Tauri application
 * Falls back to WebSocket if Tauri Event API is not available
 * 
 * @param event - The event name to listen for
 * @param handler - Callback to handle the event
 * @param options - Options for the event listener
 */
export async function listen<T>(event: EventName, handler: EventCallback<T>, options?: Options): Promise<UnlistenFn> {
    if (((window as any).__TAURI_INTERNALS__ && (window as any).__TAURI_INTERNALS__.invoke) ||
        (window as any).__TAURI__ && (window as any).__TAURI__.invoke) {
        return await TauriListen(event, handler, options);
    } else {
        initWebSocket();
        // If WebSocket is connecting, wait for it
        if (wsReady) {
            await wsReady;
        }
        if (ws && ws.readyState === WebSocket.OPEN) {
            // Handle WebSocket messages for events
            const messageHandler = (wsEvent: MessageEvent) => {
                try {
                    const data = JSON.parse(wsEvent.data);
                    if (data.event === event) {
                        handler(data);
                    }
                } catch (err) {
                    console.error(err);
                    throw new Error('Error handling WebSocket event');
                }
            };

            ws.addEventListener('message', messageHandler);

            // Return an unlisten function
            return () => {
                ws?.removeEventListener('message', messageHandler);
            };
        } else {
            throw new Error("No WebSocket or Tauri IPC available to invoke");
        }
    }
}
