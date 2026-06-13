/**
 * Core API module for Tauri Remote UI
 *
 * This module handles sending messages to the Tauri application via WebSocket
 */
import { InvokeArgs, InvokeOptions, invoke as TauriInvoke } from '@tauri-apps/api/core';
import {
    filterCollection,
    hasTauriRuntime,
    initWebSocket,
    RpcStatus,
    ws,
    wsReady,
} from '../../socket';

/** Monotonic request id, encapsulated in module scope. */
let nextRequestId = 0;

/**
 * Invoke a command on the Tauri application.
 * Falls back to a WebSocket transport if Tauri IPC is not available.
 *
 * @param cmd - The command name to invoke
 * @param args - Arguments to pass to the command
 * @param options - Options for the command
 */
export async function invoke<T>(
    cmd: string,
    args?: InvokeArgs,
    options?: InvokeOptions,
): Promise<T> {
    if (hasTauriRuntime()) {
        return await TauriInvoke<T>(cmd, args, options);
    }
    initWebSocket();
    if (wsReady) {
        await wsReady;
    }
    if (!ws || ws.readyState !== WebSocket.OPEN) {
        throw new Error('No WebSocket or Tauri IPC available to invoke');
    }
    const requestId = ++nextRequestId;
    return await new Promise<T>((resolve, reject) => {
        const message = { id: requestId, cmd, args, options };
        const timeoutHandle = setTimeout(() => {
            delete filterCollection[requestId];
            reject(new Error(`Invoke timeout. cmd: ${cmd}`));
        }, 30000);
        filterCollection[requestId] = ({ status, payload }) => {
            clearTimeout(timeoutHandle);
            delete filterCollection[requestId];
            if (status === RpcStatus.Success) {
                resolve(payload as T);
            } else {
                reject(payload);
            }
        };
        ws!.send(JSON.stringify(message));
    });
}

