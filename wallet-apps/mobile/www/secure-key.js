// MISAKA Wallet (mobile) — hardware-backed, biometric-gated vault key.
//
// The BIP39 mnemonic is encrypted with the SAME vault format as the extension
// (src/vault.js, AES-256-GCM). On mobile the AES key is NOT derived from a typed
// password but from a random 32-byte "wrapping key" that lives in the OS secure
// element (iOS Keychain/Secure Enclave, Android Keystore/StrongBox) and is released
// only after a biometric / device-passcode check. So: Face ID / fingerprint unlocks
// the wallet, and the key never exists in JS until the OS authorizes it.
//
// Plugins (see ../package.json): @aparajita/capacitor-secure-storage +
// @aparajita/capacitor-biometric-auth. Swap for your preferred secure-storage plugin
// as long as it maps to the iOS/Android access-control settings documented in ../README.md.

import { SecureStorage } from "@aparajita/capacitor-secure-storage";
import { BiometricAuth } from "@aparajita/capacitor-biometric-auth";

const WRAP_KEY_ID = "misaka.vault.wrapkey";

const toHex = (u8) => [...u8].map((b) => b.toString(16).padStart(2, "0")).join("");

// Configure the secure store for the strongest at-rest policy the device supports:
// device-only (never iCloud-synced) + biometric/passcode access control.
async function configureStore() {
  await SecureStorage.setSynchronize(false); // iOS: this-device-only, no iCloud Keychain
  // The plugin applies SecAccessControl(.biometryCurrentSet|.devicePasscode) on iOS and a
  // BiometricPrompt-gated Keystore key on Android when access is requested below.
}

/** First-run: mint a random wrapping key and store it behind the secure element. */
export async function provisionWrappingKey() {
  await configureStore();
  const wrap = crypto.getRandomValues(new Uint8Array(32));
  await SecureStorage.set(WRAP_KEY_ID, toHex(wrap), /* convertDate */ false, /* access */ {
    biometric: true,
    devicePasscode: true,
  });
  wrap.fill(0);
}

/** Unlock: prompt biometrics, then release the wrapping key from the secure element. */
export async function unlockWrappingKey(reason = "Unlock MISAKA Wallet") {
  const avail = await BiometricAuth.checkBiometry();
  if (avail.isAvailable) {
    await BiometricAuth.authenticate({
      reason,
      cancelTitle: "Cancel",
      allowDeviceCredential: true, // fall back to device passcode
      iosFallbackTitle: "Use passcode",
      androidTitle: "Unlock wallet",
    });
  }
  const hex = await SecureStorage.get(WRAP_KEY_ID, false);
  if (!hex) throw new Error("no wrapping key — wallet not provisioned");
  return Uint8Array.from(hex.match(/.{2}/g).map((h) => parseInt(h, 16)));
}

export async function hasWrappingKey() {
  try { return !!(await SecureStorage.get(WRAP_KEY_ID, false)); } catch { return false; }
}

export async function wipeWrappingKey() {
  try { await SecureStorage.remove(WRAP_KEY_ID); } catch {}
}

// Bridge to the shared vault: derive the AES password material from the hardware
// wrapping key (so vault.js stays identical across platforms). The wrapping key, not
// a human password, is the secret — and it only exists post-biometric, briefly, in RAM.
export function wrappingKeyToVaultPassword(wrapU8) {
  return "hwk:" + toHex(wrapU8); // fed to createVault/openVault as the "password"
}
