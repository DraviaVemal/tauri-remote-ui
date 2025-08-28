# Tauri Remote UI

**Tauri Remote UI** is a plugin that allows you to expose your Tauri application's UI to any web browser, enabling seamless remote interaction and end-to-end testing. The plugin bridges your native app and commercial browsers, letting you use standard web automation tools for testing and debugging—without modifying your app's logic.

## Features

- **Remote UI Exposure:** Interact with your Tauri app from any browser.
- **Seamless E2E Testing:** Use existing web automation/testing tools.
- **Automatic Transport Switching:** IPC for WebView, WebSocket for browsers—handled transparently.
- **Customizable Security:** Control and secure remote access as needed.
- **Zero App Changes:** No additional changes required to your app after plugin setup.
- **Future Compatibility:** When [CEF-RS](https://github.com/cef-rs/cef) becomes available, the same E2E tests (e.g., written with Playwright or similar tools that use the Chromium debug port) will work seamlessly in debug mode, ensuring long-term support for modern testing workflows.

## Operation Flow

- **WebView:** Uses IPC for communication between frontend and backend.
- **Commercial Browser:** Uses WebSocket (WS) for remote frontend-backend communication.
- **Automatic Switching:** The Rust backend plugin and npm frontend wrapper handle transport selection automatically.
- **Security:** The exposure of the web app can be secured and customized by the end user.

## Usage

1. **Install the Rust plugin** in your Tauri project.
2. **Install the NPM plugin** in your frontend.
3. **Access the UI remotely** via the provided web interface when activated.

## Development

- Build Rust: `cargo build`
- Build JS: `pnpm build` (from the root)
- Example app: See `examples/tauri-app/`

## License

MIT
