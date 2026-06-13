/**
 * Source-of-truth for the npm package's version, used in the WebSocket
 * handshake to detect frontend/backend version skew.
 *
 * **Keep this in sync with `package.json` `version` and the Rust crate's
 * `Cargo.toml` `version`.** The release flow (`DEVOPS_BUILD=1`) rewrites this
 * value from the latest git tag.
 */
export const PACKAGE_VERSION = '1.0.1';
