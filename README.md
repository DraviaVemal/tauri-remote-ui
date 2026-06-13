# tauri-remote-ui  (AGPL-3.0)

**Tauri Remote UI** is a plugin that allows you to expose your Tauri application's UI to any web browser.

## Badges

![Crates.io Version](https://img.shields.io/crates/v/tauri-remote-ui?style=flat&label=crates.io%20%3A%20tauri-remote-ui) ![NPM Version](https://img.shields.io/npm/v/tauri-remote-ui?label=npm%20%3A%20tauri-remote-ui)

## 📜 License

- Open Source: AGPL-3.0 (see LICENSE)
- Commercial: Available via sponsorship (see LICENSE)

## ✨ Features

- Seamless enable/diable integration
- Network level access control setting
- Network latency tracking

## ⚠️ Security Warning — Read Before Exposing the Server

> **This plugin currently ships with NO authentication, NO authorization, and NO transport encryption.**
> Anyone who can reach the bound port can drive your application's UI and invoke any Tauri command your app exposes — there is no login, no API key, no TLS, and no rate limit.

The only access control today is a coarse **network-scope filter** (`OriginType`) applied to the peer's IP at TCP-accept time:

| Scope | Bind address | Who can connect |
| --- | --- | --- |
| `Localhost` *(default, recommended)* | `127.0.0.1` | This machine only. |
| `Subnet` | `0.0.0.0` | Any host on **any** of this machine's local IPv4/IPv6 subnets, plus loopback. |
| `Any` | `0.0.0.0` | **Anyone routable to this machine.** No filter applied. |

A built-in token / SSL / SSO story is on the roadmap (see *Planned Features*), but until it lands, **the only safe public-network deployment is one where you have added authentication yourself in front of this plugin** (mutual-TLS reverse proxy, WireGuard, Tailscale, Cloudflare Access, etc.).

To audit who is being allowed through at runtime, initialize a logger in your host app and run with `RUST_LOG=tauri_remote_ui=debug` — every accept/reject decision is logged with the peer IP and the active scope.

## Supports

|Environment|Support|
|-|-|
|Windows|✅|
|Mac|✅|
|Linux|✅|
|Android|❌|
|iOS|❌|

## Use Case

- Enabling seamless remote interaction for development debugging—without modifying your app's logic.
- Enabling end-to-end testing of tauri application using standard web automation tools like playwright.
  - Note : Based on tauri target OS the webkit will change windows will be 100% match test case as both are chromium rest of OS around maximum 10% UI difference are expected from actual application
- Enable remote access feature for local close ciruit hardware related application

## Planned Features

- Multiple Window of Tauri app support in Remote UI logic
- SSO Ingration Option
- Custom Starting window name options
- Dynamic Port mapping
- SSL Certificate ingration
- Authendication system for remote access (User_id,Password)

## [Documents](https://docs.draviavemal.com)

Refer document central for detailed information [docs](https://docs.draviavemal.com)
