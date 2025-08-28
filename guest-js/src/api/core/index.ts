/**
 * Core API module for Tauri Remote UI
 * 
 * This module handles sending messages to the Tauri application via WebSocket
 */
import { InvokeArgs, InvokeOptions, invoke as TauriInvoke } from '@tauri-apps/api/core'
import { ws, wsReady, initWebSocket, filterCollection } from '../../socket';

/**
 * Invoke a command on the Tauri application
 * Falls back to WebSocket if Tauri IPC is not available
 * 
 * @param cmd - The command name to invoke
 * @param args - Arguments to pass to the command
 * @param options - Options for the command
 */
let msg_id = 0;
export async function invoke<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
    initWebSocket();
    // Tauri IPC
    try {
        if (((window as any).__TAURI_INTERNALS__ && (window as any).__TAURI_INTERNALS__.invoke) ||
            (window as any).__TAURI__ && (window as any).__TAURI__.invoke) {
            return await TauriInvoke(cmd, args, options);
        } else {
            throw new Error("Failed to Find Tauri handle")
        }
    } catch (e) {
        // If WebSocket is connecting, wait for it
        if (wsReady) {
            await wsReady;
        }
        if (ws && ws.readyState === WebSocket.OPEN) {
            return new Promise((resolve, reject) => {
                const msg = {
                    id: ++msg_id,
                    cmd, args, options
                };
                let clear = setTimeout(() => {
                    delete filterCollection[msg_id];
                    reject(`Invoke Timeout. cmd : ${cmd}`);
                }, 30000);
                filterCollection[msg_id] = ({ status, payload }) => {
                    clearTimeout(clear);
                    if (status = "success") {
                        resolve(payload);
                    } else {
                        reject(payload);
                    }
                };
                ws!.send(JSON.stringify(msg));
            });
        } else {
            throw new Error('No WebSocket or Tauri IPC available to invoke');
        }
    }
}
