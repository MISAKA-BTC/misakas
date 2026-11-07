//! Link matchers for the terminal. Link matchers are used to match links in the terminal and perform
//! actions on them. Specifically we use them to open links in the browser and copy addresses, transaction
//! ids, and block hashes to the clipboard.

use crate::{imports::*, notifier::Notification};
use application_runtime::{is_nw, is_wasm};
use workflow_core::task::dispatch;
use workflow_dom::{clipboard, link};
use workflow_wasm::jserror::*;

pub fn register_link_matchers(cli: &Arc<KaspaCli>) -> Result<()> {
    if !is_wasm() {
        return Ok(());
    }

    // http links (open)
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"http[s]?:\/\/\S+", "i"),
        Arc::new(Box::new(move |_modifiers, url| {
            if is_nw() {
                nw_sys::shell::open_external(url);
            } else {
                link::open(url);
            }
        })),
    )?;

    // addresses (open,copy). kaspa-pq URIs use the `misaka*` prefix family
    // (see docs/adr/0001-network-isolation.md). There is no public kaspa-pq
    // block explorer yet, so the ctrl/meta-click action only copies the URI
    // to the clipboard and writes an informational line. The regex still
    // matches the kaspa-pq prefix family, plus the legacy `kaspa[test]:`
    // pattern for backwards-compat clipboard handling, but no external
    // lookup is performed.
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"(misaka|misakatest|misakasim|misakadev|kaspa|kaspatest):\S+", "i"),
        Arc::new(Box::new(move |modifiers, uri| {
            if modifiers.ctrl || modifiers.meta {
                if uri.starts_with("kaspa:") || uri.starts_with("kaspatest:") {
                    cli_.term().writeln("mainline Kaspa addresses are not valid on kaspa-pq (see docs/adr/0001-network-isolation.md)");
                } else {
                    cli_.term().writeln("kaspa-pq addresses cannot currently be looked up via a block explorer");
                }
            } else {
                write_to_clipboard(&cli_, uri);
            }
        })),
    )?;

    // blocks (open,copy) https://explorer.kaspa.org/blocks/
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"(block|pool):?\s+[0-9a-fA-F]{128}", "i"),
        Arc::new(Box::new(move |modifiers, text| {
            let re = Regex::new(r"(?i)^(block|pool):?\s+").unwrap();
            let uri = re.replace(text, "");

            if modifiers.ctrl || modifiers.meta {
                nw_sys::shell::open_external(&format!("https://explorer.kaspa.org/blocks/{uri}"));
            } else {
                write_to_clipboard(&cli_, uri.to_string().as_str());
            }
        })),
    )?;

    // transactions
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"(transaction|tx|txid)(\s+|\s*:\s*)[0-9a-fA-F]{128}", "i"),
        Arc::new(Box::new(move |modifiers, text| {
            let re = Regex::new(r"(?i)^(transaction|tx|txid)\s*:?\s*").unwrap();
            let uri = re.replace(text, "");

            if modifiers.ctrl || modifiers.meta {
                nw_sys::shell::open_external(&format!("https://explorer.kaspa.org/txs/{uri}"));
            } else {
                write_to_clipboard(&cli_, uri.to_string().as_str());
            }
        })),
    )?;

    // 64 byte hex encoded sequences (copy) — kaspa-pq hashes are Hash64 (128 hex chars)
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"[0-9a-fA-F]{128}", "i"),
        Arc::new(Box::new(move |_modifiers, text| {
            let re = Regex::new(r"(?i)^(transaction|tx|txid)\s*:?\s*").unwrap();
            let text = re.replace(text, "");
            write_to_clipboard(&cli_, text.to_string().as_str());
        })),
    )?;

    Ok(())
}

fn write_to_clipboard(cli: &Arc<KaspaCli>, text: &str) {
    if is_nw() {
        let clipboard = nw_sys::clipboard::get();
        clipboard.set(text);
        cli.notifier().notify(Notification::Clipboard);
    } else {
        let cli = cli.clone();
        let text = text.to_owned();
        dispatch(async move {
            if let Err(err) = clipboard::write_text(&text).await {
                log_error!("{:?}", JsErrorData::from(err));
            } else {
                cli.notifier().notify(Notification::Clipboard);
            }
        });
    }
}
