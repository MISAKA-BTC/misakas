// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {MisakaPqSmartAccount} from "./MisakaPqSmartAccount.sol";

/// @title MISAKA PQ Account Factory (PREA design v1.1 §8.1, P0-2)
/// @notice Deterministic CREATE2 deployment of `MisakaPqSmartAccount`, so an
///         account's address is a pure function of its root identity + index and
///         can be computed off-chain before deployment (the registration flow and
///         the EntryPoint's initCode path both rely on this). Idempotent: a second
///         `createAccount` with the same args returns the already-deployed address.
///
///         NOTE (P1): the design's full salt also binds `genesis_commitment` and
///         `recovery_spk_hash`; this MVP salts on what the account is actually
///         parameterized by today (root payload ‖ version ‖ index). Extend the salt
///         in lock-step when those immutable fields are added to the account.
contract MisakaPqAccountFactory {
    bytes internal constant SALT_DOMAIN = "MISAKA_PQ_ACCOUNT_V1";

    event AccountCreated(
        address indexed account, bytes32 vaultOwnerPayloadHi, bytes32 operationalRootPayloadHi, uint64 accountVersion, uint256 accountIndex
    );

    /// A PQ account's root identity (the account constructor args) + sub-account index.
    struct AccountConfig {
        bytes32 vaultOwnerPayloadHi;
        bytes32 vaultOwnerPayloadLo;
        bytes32 operationalRootPayloadHi;
        bytes32 operationalRootPayloadLo;
        uint64 accountVersion;
        uint256 accountIndex;
    }

    /// Deploy (or return the existing) PQ account for the given identity.
    function createAccount(AccountConfig calldata cfg) external returns (address account) {
        bytes32 salt = _salt(cfg);
        account = _computeAddress(salt, cfg);
        if (account.code.length > 0) {
            return account; // already deployed — idempotent
        }
        MisakaPqSmartAccount deployed = new MisakaPqSmartAccount{salt: salt}(
            cfg.vaultOwnerPayloadHi, cfg.vaultOwnerPayloadLo, cfg.operationalRootPayloadHi, cfg.operationalRootPayloadLo, cfg.accountVersion
        );
        require(address(deployed) == account, "Factory: address mismatch");
        emit AccountCreated(account, cfg.vaultOwnerPayloadHi, cfg.operationalRootPayloadHi, cfg.accountVersion, cfg.accountIndex);
    }

    /// The deterministic address an account WOULD have (deployed or not).
    function getAddress(AccountConfig calldata cfg) external view returns (address) {
        return _computeAddress(_salt(cfg), cfg);
    }

    function _salt(AccountConfig calldata cfg) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                SALT_DOMAIN,
                cfg.vaultOwnerPayloadHi,
                cfg.vaultOwnerPayloadLo,
                cfg.operationalRootPayloadHi,
                cfg.operationalRootPayloadLo,
                cfg.accountVersion,
                cfg.accountIndex
            )
        );
    }

    function _computeAddress(bytes32 salt, AccountConfig calldata cfg) internal view returns (address) {
        bytes32 initCodeHash = keccak256(
            abi.encodePacked(
                type(MisakaPqSmartAccount).creationCode,
                abi.encode(
                    cfg.vaultOwnerPayloadHi,
                    cfg.vaultOwnerPayloadLo,
                    cfg.operationalRootPayloadHi,
                    cfg.operationalRootPayloadLo,
                    cfg.accountVersion
                )
            )
        );
        return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, initCodeHash)))));
    }
}
