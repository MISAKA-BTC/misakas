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
        address indexed account, bytes32 rootPayloadHi, bytes32 rootPayloadLo, uint64 accountVersion, uint256 accountIndex
    );

    /// Deploy (or return the existing) PQ account for the given root identity.
    function createAccount(bytes32 rootPayloadHi, bytes32 rootPayloadLo, uint64 accountVersion, uint256 accountIndex)
        external
        returns (address account)
    {
        bytes32 salt = _salt(rootPayloadHi, rootPayloadLo, accountVersion, accountIndex);
        account = _computeAddress(salt, rootPayloadHi, rootPayloadLo, accountVersion);
        if (account.code.length > 0) {
            return account; // already deployed — idempotent
        }
        MisakaPqSmartAccount deployed =
            new MisakaPqSmartAccount{salt: salt}(rootPayloadHi, rootPayloadLo, accountVersion);
        require(address(deployed) == account, "Factory: address mismatch");
        emit AccountCreated(account, rootPayloadHi, rootPayloadLo, accountVersion, accountIndex);
    }

    /// The deterministic address an account WOULD have (deployed or not).
    function getAddress(bytes32 rootPayloadHi, bytes32 rootPayloadLo, uint64 accountVersion, uint256 accountIndex)
        external
        view
        returns (address)
    {
        return _computeAddress(
            _salt(rootPayloadHi, rootPayloadLo, accountVersion, accountIndex), rootPayloadHi, rootPayloadLo, accountVersion
        );
    }

    function _salt(bytes32 rootPayloadHi, bytes32 rootPayloadLo, uint64 accountVersion, uint256 accountIndex)
        internal
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(SALT_DOMAIN, rootPayloadHi, rootPayloadLo, accountVersion, accountIndex));
    }

    function _computeAddress(bytes32 salt, bytes32 rootPayloadHi, bytes32 rootPayloadLo, uint64 accountVersion)
        internal
        view
        returns (address)
    {
        bytes32 initCodeHash = keccak256(
            abi.encodePacked(type(MisakaPqSmartAccount).creationCode, abi.encode(rootPayloadHi, rootPayloadLo, accountVersion))
        );
        return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, initCodeHash)))));
    }
}
