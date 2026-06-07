// MISAKA Wallet — background service worker.
//
// Deliberately tiny: it holds NO key material. Its only job is the auto-lock timer.
// The decrypted seed (while unlocked) lives in chrome.storage.session, which is
// in-memory, extension-context-only (never readable by a web page), and is dropped
// automatically when the browser closes. This alarm drops it on idle timeout too.

const ALARM = "misaka-autolock";

async function lockNow() {
  try {
    await chrome.storage.session.remove(["unlocked"]);
  } catch {}
}

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (!msg || msg.target !== "bg") return false;
  if (msg.cmd === "arm-lock") {
    const minutes = Math.max(1, Math.min(60, Number(msg.minutes) || 5));
    chrome.alarms.create(ALARM, { delayInMinutes: minutes });
    sendResponse({ ok: true });
    return true;
  }
  if (msg.cmd === "lock-now") {
    chrome.alarms.clear(ALARM);
    lockNow().then(() => sendResponse({ ok: true }));
    return true;
  }
  return false;
});

chrome.alarms.onAlarm.addListener((a) => {
  if (a.name === ALARM) lockNow();
});

// Belt-and-suspenders: clear any unlocked seed when the worker (re)starts.
chrome.runtime.onStartup?.addListener(lockNow);
chrome.runtime.onInstalled?.addListener(lockNow);
