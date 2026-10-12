// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {MisakaPqSmartAccount} from "./MisakaPqSmartAccount.sol";

/// @title MISAKA PQ EntryPoint — relayed operation execution (PREA design v1.1 §16, P0-2)
/// @notice A permissionless relay. The PQ account's `executeRoot` / `executeSession`
///         are already self-validating (they verify the ML-DSA / secp256k1 signature
///         internally) and callable by anyone, so the relayer is purely a gas-paying
///         carrier with NO authority: it cannot move funds or authorize anything; it
///         only forwards a pre-signed op (and, for a not-yet-deployed account,
///         deploys it first via the Factory — the ERC-4337 `initCode` pattern).
///
///         The op SIGNATURE binds the account, chain, nonce and calldata (see the
///         account), so a malicious relayer can neither alter nor replay an op, and
///         submitting to the wrong account simply fails the account's own checks.
///
///         FEE REIMBURSEMENT (design §16.3): each op commits to a signed
///         `maxRelayerFee`; the account pays `min(measured cost, maxRelayerFee)` to
///         `tx.origin` (the relayer EOA) as the last step of execution — capped, signed,
///         and trusting no relayer-supplied value. The EntryPoint forwards the op
///         verbatim (the fee is account-side); it relays all three self-validating
///         entrypoints (executeRoot / executeSession / executeSessionWithProof).
contract MisakaPqEntryPoint {
    struct UserOp {
        /// The target PQ account (its deterministic Factory address).
        address account;
        /// Empty if `account` is already deployed; otherwise `factory(20) ‖
        /// factory_calldata` (a `createAccount(...)` call) to deploy it first.
        bytes initCode;
        /// The self-validating call to run on the account (an ABI-encoded
        /// `executeRoot(...)` or `executeSession(...)`).
        bytes callData;
    }

    event OpHandled(address indexed account, bool deployed);

    /// Relay a batch of pre-signed ops. Each op is independent; a failing op reverts
    /// the whole batch (callers that want best-effort submit one op per call). The
    /// relayer (`msg.sender`) gains no authority — the account validates every op.
    function handleOps(UserOp[] calldata ops) external {
        uint256 n = ops.length;
        for (uint256 i; i < n; i++) {
            UserOp calldata op = ops[i];
            bool deployed = false;

            if (op.account.code.length == 0) {
                require(op.initCode.length >= 20, "EP: account not deployed, no initCode");
                address factory = address(bytes20(op.initCode[:20]));
                require(factory.code.length > 0, "EP: factory has no code");
                (bool dok,) = factory.call(op.initCode[20:]);
                require(dok, "EP: account deploy failed");
                require(op.account.code.length > 0, "EP: account not deployed at expected address");
                deployed = true;
            }

            // Restrict the relay to the account's two SELF-VALIDATING entrypoints. The
            // EntryPoint holds no authority, so forwarding arbitrary calldata is not an
            // exploit — but pinning the selector makes the relayer's purpose explicit and
            // stops it being used as a generic call-forwarder (e.g. to a non-account
            // target or a non-execute account method). grantSession/revokeSession are
            // unreachable here anyway (they require msg.sender==the account itself).
            require(op.callData.length >= 4, "EP: calldata too short");
            bytes4 sel = bytes4(op.callData[:4]);
            require(
                sel == MisakaPqSmartAccount.executeRoot.selector || sel == MisakaPqSmartAccount.executeSession.selector
                    || sel == MisakaPqSmartAccount.executeSessionWithProof.selector,
                "EP: only execute ops"
            );

            // Forward the pre-signed op. The account self-validates; the relayer holds
            // no authority. Value is never forwarded by the EntryPoint (an op funds
            // itself from the account's own balance).
            (bool ok,) = op.account.call(op.callData);
            require(ok, "EP: op reverted");
            emit OpHandled(op.account, deployed);
        }
    }
}
