// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// @title MISAKA PQ-Rooted EVM Smart Account — root path (PREA design v1.1 §13, P0-2)
/// @notice An EVM account whose UNRESTRICTED authority is a post-quantum ML-DSA-87
///         key (NOT secp256k1). A root operation is authorized by an ML-DSA-87
///         signature verified on-chain by the MISAKA F003 `MLDSA87_VERIFY`
///         precompile (`0x…F003`, version 0x02). The account stores only the
///         64-byte ADDRESS PAYLOAD of its root key; the full public key is supplied
///         per call and F003 binds it to that payload before verifying.
///
///         This MVP implements ONLY the root path (`executeRoot`). Operational-root
///         rotation, the offline Vault Owner, freeze/recovery, the restricted
///         secp256k1 session path, ERC-1271, the Factory and the EntryPoint are
///         deferred to later P0-2 / P1 slices (design v1.1 §7/§12/§13.5/§14/§15/§16).
///
///         IMPORTANT: F003 is consensus-FENCED INERT (activation = u64::MAX) on every
///         MISAKA network today. While inert a call to `0x…F003` returns empty data,
///         so `executeRoot` REVERTS ("ml-dsa root auth unavailable"). This account is
///         only operable once F003 is activated by governance. The contract + its
///         tests exist now so the consumer is ready; the live e2e (real F003 + a real
///         ML-DSA signature) runs against an F003-activated test harness.
///
///         SECURITY NOTE: ownership/authority here is post-quantum (ML-DSA-87). This
///         account intentionally does NOT accept any secp256k1/ECDSA transaction as
///         an authority — on a PQ-active network the consensus rule (PREA I-6) also
///         skips a direct ECDSA tx whose sender is a registered PQ account.
contract MisakaPqSmartAccount {
    /// The MISAKA F003 ML-DSA-87 verify precompile.
    address internal constant F003 = address(0x0000000000000000000000000000000000F003);
    /// F003 input version tag for a PREA key-bound root authorization (option B:
    /// F003 hashes the op preimage itself).
    uint8 internal constant F003_VERSION_PREA_ROOT = 0x02;
    /// Domain tag prepended to the canonical op preimage. The off-chain ML-DSA
    /// signer MUST construct the identical preimage (see `_opPreimage`).
    bytes internal constant OP_DOMAIN = "MISAKA_PQ_EXECUTE_ROOT_V1";

    /// The root key's 64-byte ML-DSA-87 address payload (keyed-BLAKE2b-512 of the
    /// public key under the MISAKA address context), split into two words. F003
    /// checks the per-call public key hashes to exactly this.
    bytes32 public immutable rootPayloadHi;
    bytes32 public immutable rootPayloadLo;
    /// Account version (bound into every op preimage so a signature for one
    /// account version can never authorize another).
    uint64 public immutable accountVersion;

    /// Strictly-increasing root operation counter (replay + intra-call reentrancy
    /// guard: a reentrant call would need a signature over the NEXT nonce).
    uint64 public rootNonce;

    event RootExecuted(uint64 indexed nonce, address indexed target, uint256 value, bool success);

    constructor(bytes32 rootPayloadHi_, bytes32 rootPayloadLo_, uint64 accountVersion_) {
        rootPayloadHi = rootPayloadHi_;
        rootPayloadLo = rootPayloadLo_;
        accountVersion = accountVersion_;
    }

    receive() external payable {}

    /// The exact bytes the ML-DSA root signature commits to (via F003's internal
    /// keyed-BLAKE2b-512). Packed with FIXED widths so an off-chain signer can
    /// reproduce it byte-for-byte: domain ‖ chainId(32) ‖ account(20) ‖
    /// version(8) ‖ nonce(8) ‖ validAfter(8) ‖ validUntil(8) ‖ target(20) ‖
    /// value(32) ‖ callData. chainId + account bind the op to THIS chain and
    /// account (anti cross-chain / cross-account replay).
    function _opPreimage(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 validAfterBlock,
        uint64 validUntilBlock,
        uint64 nonce
    ) internal view returns (bytes memory) {
        return abi.encodePacked(
            OP_DOMAIN,
            uint256(block.chainid),
            address(this),
            accountVersion,
            nonce,
            validAfterBlock,
            validUntilBlock,
            target,
            value,
            callData
        );
    }

    /// Execute one root operation authorized by an ML-DSA-87 signature.
    /// @param publicKey the 2592-byte ML-DSA-87 public key (F003 binds it to the
    ///        stored root payload — a wrong key fails the binding, not just the sig).
    /// @param signature the 4627-byte ML-DSA-87 signature over the op preimage's
    ///        keyed-BLAKE2b-512 digest under the PREA root context.
    function executeRoot(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 validAfterBlock,
        uint64 validUntilBlock,
        uint64 nonce,
        bytes calldata publicKey,
        bytes calldata signature
    ) external returns (bytes memory) {
        require(nonce == rootNonce, "PQ: bad nonce");
        require(block.number >= validAfterBlock && block.number <= validUntilBlock, "PQ: outside validity window");
        // Fail-closed already (a wrong length shifts the F003 offsets and F003 returns
        // false), but check explicitly for a clear error + tight input.
        require(publicKey.length == 2592 && signature.length == 4627, "PQ: bad key/sig length");

        bytes memory preimage = _opPreimage(target, value, callData, validAfterBlock, validUntilBlock, nonce);

        // F003 v0x02 input: version ‖ expected_payload64 ‖ pubkey ‖ sig ‖ preimage.
        bytes memory input =
            abi.encodePacked(F003_VERSION_PREA_ROOT, rootPayloadHi, rootPayloadLo, publicKey, signature, preimage);

        (bool verified, bytes memory ret) = F003.staticcall(input);
        // Inert F003 ⇒ empty return ⇒ this require fails ("…unavailable" semantics).
        require(verified && ret.length == 32 && uint8(ret[31]) == 1, "PQ: ml-dsa root auth failed");

        // Effects before interaction: bump the nonce FIRST so a reentrant call from
        // the target cannot replay this op (it would need a signature over nonce+1).
        rootNonce = nonce + 1;

        (bool success, bytes memory result) = target.call{value: value}(callData);
        require(success, "PQ: target call reverted");
        emit RootExecuted(nonce, target, value, success);
        return result;
    }
}
