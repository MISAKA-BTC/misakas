//! Local storage implementation for the wallet SDK.
//! This module provides a local storage implementation
//! that functions uniformly in native and JS environments.
//! In native and NodeJS environments, this subsystem
//! will use the native file system IO. In the browser
//! environment, if called from the web page context
//! this will use `localStorage` and if invoked in the
//! chromium extension context it will use the
//! `chrome.storage.local` API. The implementation
//! is backed by the [`workflow_store`](https://docs.rs/workflow-store/)
//! crate.

pub mod cache;
pub mod collection;
pub mod interface;
pub mod payload;
pub mod storage;
pub mod streams;
pub mod transaction;
pub mod wallet;

pub use collection::Collection;
pub use payload::Payload;
pub use storage::Storage;
pub use wallet::WalletStorage;

use crate::error::Error;
use crate::result::Result;
use std::sync::OnceLock;
use wasm_bindgen::prelude::*;
use workflow_store::fs::create_dir_all_sync;

// Audit (2026-06-27) M-2: these were `static mut Option<String>` with `unsafe`
// getters/setters — unsound (data race / UB / dangling `&'static str`), especially
// once `wasm_bindgen` lets JS drive the setters. They are set-once-before-use globals,
// so a `OnceLock` models them exactly: the getters keep returning `&'static str`
// (a reference into the static `OnceLock` is itself `'static`), so no caller changes,
// and the setters become safe + race-free with a clear "already initialized" error.
static DEFAULT_STORAGE_FOLDER: OnceLock<String> = OnceLock::new();
static DEFAULT_WALLET_FILE: OnceLock<String> = OnceLock::new();
static DEFAULT_SETTINGS_FILE: OnceLock<String> = OnceLock::new();

pub fn default_storage_folder() -> &'static str {
    DEFAULT_STORAGE_FOLDER.get_or_init(|| "~/.kaspa".to_string()).as_str()
}

pub fn default_wallet_file() -> &'static str {
    DEFAULT_WALLET_FILE.get_or_init(|| "kaspa".to_string()).as_str()
}

pub fn default_settings_file() -> &'static str {
    DEFAULT_SETTINGS_FILE.get_or_init(|| "kaspa".to_string()).as_str()
}

/// Set a custom storage folder for the wallet SDK
/// subsystem.  Encrypted wallet files and transaction
/// data will be stored in this folder. If not set
/// the storage folder will default to `~/.kaspa`
/// (note that the folder is hidden).
///
/// This must be called before using any other wallet
/// SDK functions: the default is set-once, so calling
/// this after the folder has already been resolved
/// (or set) returns an error rather than silently
/// changing it underneath an initialized wallet.
///
/// NOTE: This function will create a folder if it
/// doesn't exist. This function will have no effect
/// if invoked in the browser environment.
pub fn set_default_storage_folder(folder: String) -> Result<()> {
    create_dir_all_sync(&folder).map_err(|err| Error::custom(format!("Failed to create storage folder: {err}")))?;
    DEFAULT_STORAGE_FOLDER
        .set(folder)
        .map_err(|_| Error::custom("the default storage folder is already initialized; it must be set before any wallet operation"))
}

/// Set a custom storage folder for the wallet SDK
/// subsystem.  Encrypted wallet files and transaction
/// data will be stored in this folder. If not set
/// the storage folder will default to `~/.kaspa`
/// (note that the folder is hidden).
///
/// This must be called before using any other wallet
/// SDK functions.
///
/// NOTE: This function will create a folder if it
/// doesn't exist. This function will have no effect
/// if invoked in the browser environment.
///
/// @param {String} folder - the path to the storage folder
///
/// @category Wallet API
#[wasm_bindgen(js_name = setDefaultStorageFolder, skip_jsdoc)]
pub fn js_set_default_storage_folder(folder: String) -> Result<()> {
    set_default_storage_folder(folder)
}

/// Set the name of the default wallet file name
/// or the `localStorage` key.  If `Wallet::open`
/// is called without a wallet file name, this name
/// will be used.  Please note that this name
/// will be suffixed with `.wallet` suffix.
///
/// This function should be called before using any
/// other wallet SDK functions; the default is set-once
/// and returns an error if it has already been resolved.
pub fn set_default_wallet_file(folder: String) -> Result<()> {
    DEFAULT_WALLET_FILE
        .set(folder)
        .map_err(|_| Error::custom("the default wallet file is already initialized; it must be set before any wallet operation"))
}

/// Set the name of the default wallet file name
/// or the `localStorage` key.  If `Wallet::open`
/// is called without a wallet file name, this name
/// will be used.  Please note that this name
/// will be suffixed with `.wallet` suffix.
///
/// This function should be called before using any
/// other wallet SDK functions.
///
/// @param {String} folder - the name to the wallet file or key.
///
/// @category Wallet API
#[wasm_bindgen(js_name = setDefaultWalletFile)]
pub fn js_set_default_wallet_file(folder: String) -> Result<()> {
    set_default_wallet_file(folder)
}
