// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// @title MISAKA PQ-Rooted EVM Smart Account (PREA design v1.1 §13/§14/§15, P0-2)
/// @notice An EVM account whose UNRESTRICTED authority is a post-quantum ML-DSA-87
///         key (NOT secp256k1), with a RESTRICTED secp256k1 "session" key for
///         frequent low-risk operations. Root authorization is verified on-chain by
///         the MISAKA F003 `MLDSA87_VERIFY` precompile (`0x…F003`, version 0x02);
///         session authorization is a normal secp256k1 signature gated by a grant.
///
///         Implemented (P0-2): the ML-DSA root path (`executeRoot`), root-authorized
///         session grant/revoke, the restricted session path (`executeSession`), and
///         ERC-1271 (root-only). Deferred (P1): the offline Vault Owner, operational-
///         root rotation/freeze/recovery, Merkle target allowlists, full ERC-721/1155
///         amount policy + Permit2, ERC-1271 session-purpose recompute, the
///         deterministic Factory and the relayed EntryPoint
///         (design v1.1 §7/§12/§13.6/§14.5-6/§15.2/§16).
///
///         ⚠️ F003 is consensus-FENCED INERT (activation = u64::MAX) on every MISAKA
///         network today, so a call to `0x…F003` returns empty data and `executeRoot`
///         REVERTS until F003 is governance-activated (`executeSession` does not touch
///         F003 and works whenever a grant exists). The contract + tests exist now so
///         the consumer is ready.
contract MisakaPqSmartAccount {
    // --- F003 (ML-DSA-87 verify precompile) ---
    address internal constant F003 = address(0x0000000000000000000000000000000000F003);
    uint8 internal constant F003_VERSION_PREA_ROOT = 0x02;
    bytes internal constant OP_DOMAIN = "MISAKA_PQ_EXECUTE_ROOT_V1";
    /// Vault-Owner op preimage domain (distinct from OP_DOMAIN so a vault signature can
    /// never be replayed as an executeRoot op, and vice versa).
    bytes internal constant VAULT_DOMAIN = "MISAKA_PQ_VAULT_ADMIN_V1";
    uint8 internal constant VAULT_OP_ROTATE = 0;
    uint8 internal constant VAULT_OP_FREEZE = 1;
    uint8 internal constant VAULT_OP_UNFREEZE = 2;

    // --- secp256k1 (session) constants ---
    /// EIP-2 low-`s` bound (secp256k1n/2); reject the malleable high-`s` half.
    uint256 internal constant SECP256K1N_HALF = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0;
    /// Session domain tag for the op hash an off-chain session key signs.
    bytes32 internal constant SESSION_OP_DOMAIN = keccak256("MISAKA_PQ_EXECUTE_SESSION_V1");
    /// ERC-1271 magic value for a valid signature.
    bytes4 internal constant ERC1271_MAGIC = 0x1626ba7e;
    /// Selectors a session may NEVER call (approval-as-delegation drains every value
    /// cap by handing withdrawal rights to an external spender): ERC-20/721 `approve`,
    /// ERC-721/1155 `setApprovalForAll`. DELEGATECALL is structurally impossible (the
    /// account only ever `CALL`s). Permit/Permit2 are a documented P1 follow-up.
    bytes4 internal constant SEL_APPROVE = 0x095ea7b3; // approve(address,uint256)
    bytes4 internal constant SEL_SET_APPROVAL_FOR_ALL = 0xa22cb465; // setApprovalForAll(address,bool)
    // ERC-20 transfer selectors whose amount IS decoded + capped when a token cap is set.
    bytes4 internal constant SEL_TRANSFER = 0xa9059cbb; // transfer(address,uint256)
    bytes4 internal constant SEL_TRANSFER_FROM = 0x23b872dd; // transferFrom(address,address,uint256)

    // --- root identity ---
    /// Vault Owner: the IMMUTABLE, offline COLD recovery anchor (64-byte ML-DSA-87
    /// address payload). Authorizes operational-root ROTATION, FREEZE and UNFREEZE
    /// via `vaultExecute` — NOT day-to-day ops. Set once at deploy.
    bytes32 public immutable vaultOwnerPayloadHi;
    bytes32 public immutable vaultOwnerPayloadLo;
    /// Account version (bound into every preimage / session op hash).
    uint64 public immutable accountVersion;

    // --- mutable ---
    /// Operational Root: the day-to-day high-authority ML-DSA-87 key (`executeRoot` +
    /// session grant/revoke). F003 binds each call's public key to this 64-byte
    /// payload. ROTATABLE by the Vault Owner (a rotation bumps `rootEpoch`).
    bytes32 public operationalRootPayloadHi;
    bytes32 public operationalRootPayloadLo;
    /// Strictly-increasing root operation counter (replay + reentrancy guard).
    uint64 public rootNonce;
    /// Strictly-increasing Vault-Owner operation counter (rotate/freeze/unfreeze).
    uint64 public vaultNonce;
    /// Bumped by a Vault-Owner root rotation; sessions bind to their grant epoch, so a
    /// rotation invalidates ALL outstanding sessions at once.
    uint64 public rootEpoch;
    /// Emergency stop (Vault-Owner only). Blocks BOTH `executeRoot` and
    /// `executeSession`; only the Vault Owner can `vaultExecute(UNFREEZE)`.
    bool public frozen;

    struct SessionGrant {
        bool active;
        uint64 validUntilBlock;
        uint64 maxCalls;
        uint64 callsUsed;
        uint128 maxNativeTotal;
        uint128 nativeUsed;
        uint64 rootEpoch;
    }

    struct Allow {
        bool allowed;
        /// Per-call ERC-20 amount cap for `transfer`/`transferFrom` (0 = no token-amount
        /// semantics — the (target,selector) allowlist + native cap are the gate; a
        /// non-zero cap on a non-transfer selector is rejected as unverifiable).
        uint256 maxAmount;
    }

    /// session key (secp256k1 address) → grant.
    mapping(address => SessionGrant) public sessions;
    /// session key → grant generation. Bumped on every (re-)grant so a re-grant of the
    /// SAME key starts a fresh allowlist generation — re-granting with a narrower
    /// allowlist can never leave stale (broader) entries live (mappings aren't
    /// enumerable to clear, so the lookup is generation-scoped instead).
    mapping(address => uint64) public sessionGrantGen;
    /// session key → grantGen → keccak256(target ‖ selector) → allowance.
    mapping(address => mapping(uint64 => mapping(bytes32 => Allow))) public allows;

    event RootExecuted(uint64 indexed nonce, address indexed target, uint256 value, bool success);
    event SessionGranted(address indexed sessionKey, uint64 validUntilBlock, uint64 maxCalls, uint128 maxNativeTotal);
    event SessionRevoked(address indexed sessionKey);
    event SessionExecuted(address indexed sessionKey, uint64 callIndex, address indexed target, uint256 value);
    event OperationalRootRotated(uint64 indexed newRootEpoch);
    event FrozenSet(bool frozen);

    constructor(
        bytes32 vaultOwnerPayloadHi_,
        bytes32 vaultOwnerPayloadLo_,
        bytes32 operationalRootPayloadHi_,
        bytes32 operationalRootPayloadLo_,
        uint64 accountVersion_
    ) {
        vaultOwnerPayloadHi = vaultOwnerPayloadHi_;
        vaultOwnerPayloadLo = vaultOwnerPayloadLo_;
        operationalRootPayloadHi = operationalRootPayloadHi_;
        operationalRootPayloadLo = operationalRootPayloadLo_;
        accountVersion = accountVersion_;
    }

    receive() external payable {}

    // ------------------------------------------------------------------ root path

    /// The exact bytes the ML-DSA root signature commits to (via F003's internal
    /// keyed-BLAKE2b-512). Fixed widths so an off-chain signer reproduces it
    /// byte-for-byte: domain ‖ chainId(32) ‖ account(20) ‖ version(8) ‖ nonce(8) ‖
    /// validAfter(8) ‖ validUntil(8) ‖ target(20) ‖ value(32) ‖ callData.
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

    /// Execute one root operation authorized by an ML-DSA-87 signature (F003 v0x02).
    /// Self-admin ops (grantSession/revokeSession) are performed by passing
    /// `target = address(this)` and the corresponding calldata — the ML-DSA root
    /// signature then authorizes exactly that self-call.
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
        require(!frozen, "PQ: account frozen");
        require(nonce == rootNonce, "PQ: bad nonce");
        require(block.number >= validAfterBlock && block.number <= validUntilBlock, "PQ: outside validity window");
        require(publicKey.length == 2592 && signature.length == 4627, "PQ: bad key/sig length");

        bytes memory preimage = _opPreimage(target, value, callData, validAfterBlock, validUntilBlock, nonce);
        bytes memory input =
            abi.encodePacked(F003_VERSION_PREA_ROOT, operationalRootPayloadHi, operationalRootPayloadLo, publicKey, signature, preimage);

        (bool verified, bytes memory ret) = F003.staticcall(input);
        require(verified && ret.length == 32 && uint8(ret[31]) == 1, "PQ: ml-dsa root auth failed");

        rootNonce = nonce + 1; // effects before interaction (replay + reentrancy guard)

        (bool success, bytes memory result) = target.call{value: value}(callData);
        require(success, "PQ: target call reverted");
        emit RootExecuted(nonce, target, value, success);
        return result;
    }

    // ---------------------------------------------------------------- vault owner

    /// The bytes the Vault-Owner ML-DSA signature commits to (via F003's keyed
    /// BLAKE2b). Distinct VAULT_DOMAIN ⇒ a vault signature can never be replayed as
    /// an executeRoot op. Fixed widths so an off-chain signer reproduces it exactly.
    function _vaultPreimage(uint8 opType, bytes32 newRootHi, bytes32 newRootLo, uint64 vNonce)
        internal
        view
        returns (bytes memory)
    {
        return abi.encodePacked(
            VAULT_DOMAIN, uint256(block.chainid), address(this), accountVersion, vNonce, opType, newRootHi, newRootLo
        );
    }

    /// A Vault-Owner (cold recovery anchor) operation, authorized by an ML-DSA-87
    /// signature verified via F003 v0x02 against `vaultOwnerPayload`:
    /// - `VAULT_OP_ROTATE`  : set the Operational Root to (newRootHi,newRootLo) and bump
    ///   `rootEpoch` — instantly invalidating EVERY outstanding session (compromised
    ///   operational-root recovery).
    /// - `VAULT_OP_FREEZE`  : emergency stop — blocks executeRoot + executeSession.
    /// - `VAULT_OP_UNFREEZE`: lift the freeze.
    /// NOT gated by `frozen` (the Vault Owner must be able to rotate/unfreeze a frozen
    /// account). `newRootHi/Lo` are ignored for freeze/unfreeze.
    function vaultExecute(
        uint8 opType,
        bytes32 newRootHi,
        bytes32 newRootLo,
        uint64 vNonce,
        bytes calldata publicKey,
        bytes calldata signature
    ) external {
        require(vNonce == vaultNonce, "PQ: bad vault nonce");
        require(publicKey.length == 2592 && signature.length == 4627, "PQ: bad key/sig length");

        bytes memory preimage = _vaultPreimage(opType, newRootHi, newRootLo, vNonce);
        bytes memory input =
            abi.encodePacked(F003_VERSION_PREA_ROOT, vaultOwnerPayloadHi, vaultOwnerPayloadLo, publicKey, signature, preimage);
        (bool verified, bytes memory ret) = F003.staticcall(input);
        require(verified && ret.length == 32 && uint8(ret[31]) == 1, "PQ: ml-dsa vault auth failed");

        vaultNonce = vNonce + 1; // effects before any state change

        if (opType == VAULT_OP_ROTATE) {
            require(newRootHi != bytes32(0) || newRootLo != bytes32(0), "PQ: zero operational root");
            operationalRootPayloadHi = newRootHi;
            operationalRootPayloadLo = newRootLo;
            rootEpoch += 1; // invalidates ALL sessions (they bind their grant epoch)
            emit OperationalRootRotated(rootEpoch);
        } else if (opType == VAULT_OP_FREEZE) {
            frozen = true;
            emit FrozenSet(true);
        } else if (opType == VAULT_OP_UNFREEZE) {
            frozen = false;
            emit FrozenSet(false);
        } else {
            revert("PQ: unknown vault op");
        }
    }

    // -------------------------------------------------------------- session admin
    // Only callable by the account itself (i.e. via executeRoot's authorized
    // self-call), so the ML-DSA root is the sole grantor/revoker of sessions.

    function grantSession(
        address sessionKey,
        uint64 validUntilBlock,
        uint64 maxCalls,
        uint128 maxNativeTotal,
        bytes32[] calldata targetSelectorKeys,
        uint256[] calldata maxAmounts
    ) external {
        require(msg.sender == address(this), "PQ: only root (via executeRoot)");
        require(sessionKey != address(0), "PQ: zero session key");
        require(targetSelectorKeys.length == maxAmounts.length, "PQ: policy length mismatch");

        // New grant generation: orphans any prior allowlist entries for this key, so a
        // re-grant with a narrower allowlist can never inherit stale (broader) entries.
        uint64 gen = sessionGrantGen[sessionKey] + 1;
        sessionGrantGen[sessionKey] = gen;

        SessionGrant storage g = sessions[sessionKey];
        g.active = true;
        g.validUntilBlock = validUntilBlock;
        g.maxCalls = maxCalls;
        g.callsUsed = 0;
        g.maxNativeTotal = maxNativeTotal;
        g.nativeUsed = 0;
        g.rootEpoch = rootEpoch;
        for (uint256 i; i < targetSelectorKeys.length; i++) {
            allows[sessionKey][gen][targetSelectorKeys[i]] = Allow({allowed: true, maxAmount: maxAmounts[i]});
        }
        emit SessionGranted(sessionKey, validUntilBlock, maxCalls, maxNativeTotal);
    }

    function revokeSession(address sessionKey) external {
        require(msg.sender == address(this), "PQ: only root (via executeRoot)");
        sessions[sessionKey].active = false;
        emit SessionRevoked(sessionKey);
    }

    /// The allowlist key for a (target, selector) pair.
    function allowKey(address target, bytes4 selector) public pure returns (bytes32) {
        return keccak256(abi.encodePacked(target, selector));
    }

    // ----------------------------------------------------------------- session path

    /// The op hash a session key signs (domain-bound to this chain + account; the
    /// session "nonce" is the grant's monotonic call index). The signer (session key)
    /// is RECOVERED from the signature over this hash — it is intentionally NOT a
    /// field here.
    function _sessionOpHash(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 callIndex
    ) internal view returns (bytes32) {
        return keccak256(
            abi.encode(
                SESSION_OP_DOMAIN,
                block.chainid,
                address(this),
                accountVersion,
                target,
                value,
                keccak256(callData),
                callIndex
            )
        );
    }

    /// Execute one session operation. `ecdsaSig` is a 65-byte secp256k1 signature by
    /// the granted session key over `_sessionOpHash(...)`. Enforces: grant active +
    /// epoch + expiry, monotonic call index, max-calls, native value cap, the
    /// (target,selector) allowlist, the forbidden-selector blocklist, and (for
    /// `transfer`/`transferFrom`) the ERC-20 amount cap. CALL only — never delegatecall.
    function executeSession(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 callIndex,
        bytes calldata ecdsaSig
    ) external returns (bytes memory) {
        // A session must NEVER call the account itself: grantSession/revokeSession
        // gate only on `msg.sender == address(this)`, so a self-call would let a
        // session allowlisted for (address(this), grantSession) escalate to granting
        // itself unlimited sessions. Fail closed regardless of the allowlist.
        require(!frozen, "PQ: account frozen");
        require(target != address(this), "PQ: session cannot target self");
        require(callData.length >= 4, "PQ: calldata too short");
        bytes4 sel = bytes4(callData[:4]);
        require(sel != SEL_APPROVE && sel != SEL_SET_APPROVAL_FOR_ALL, "PQ: forbidden selector");

        address sessionKey = _recover(_sessionOpHash(target, value, callData, callIndex), ecdsaSig);
        require(sessionKey != address(0), "PQ: bad session signature");

        SessionGrant storage g = sessions[sessionKey];
        require(g.active && g.rootEpoch == rootEpoch, "PQ: session inactive");
        require(block.number <= g.validUntilBlock, "PQ: session expired");
        require(callIndex == g.callsUsed, "PQ: bad session call index");
        require(g.callsUsed < g.maxCalls, "PQ: session call cap");
        require(uint256(value) + uint256(g.nativeUsed) <= uint256(g.maxNativeTotal), "PQ: session native cap");

        Allow storage a = allows[sessionKey][sessionGrantGen[sessionKey]][allowKey(target, sel)];
        require(a.allowed, "PQ: target/selector not allowed");
        if (a.maxAmount != 0) {
            require(_erc20Amount(sel, callData) <= a.maxAmount, "PQ: token amount cap");
        }

        g.callsUsed += 1;
        g.nativeUsed += uint128(value);

        (bool success, bytes memory result) = target.call{value: value}(callData);
        require(success, "PQ: session call reverted");
        emit SessionExecuted(sessionKey, callIndex, target, value);
        return result;
    }

    /// Decode the capped ERC-20 amount for `transfer`/`transferFrom`. A non-zero
    /// amount cap on any other selector is unverifiable → reject (fail-closed).
    function _erc20Amount(bytes4 sel, bytes calldata callData) internal pure returns (uint256) {
        if (sel == SEL_TRANSFER) {
            require(callData.length >= 4 + 64, "PQ: bad transfer calldata");
            return uint256(bytes32(callData[36:68]));
        }
        if (sel == SEL_TRANSFER_FROM) {
            require(callData.length >= 4 + 96, "PQ: bad transferFrom calldata");
            // selector(4) ‖ from(32) ‖ to(32) ‖ amount(32) ⇒ amount at [68:100].
            return uint256(bytes32(callData[68:100]));
        }
        revert("PQ: amount cap unsupported for selector");
    }

    // --------------------------------------------------------------------- ERC-1271

    /// ERC-1271: ONLY an ML-DSA root signature (verified via F003 v0x02 over the
    /// 1271 `hash` wrapped in a session/root op preimage) is a generally-valid
    /// account signature. Session (secp256k1) signatures are NOT accepted here by
    /// default — accepting them unconditionally would let an off-chain order/permit
    /// hash be passed off under a benign purpose (the purpose-confusion vector); a
    /// purpose-recomputing session 1271 path is a P1 follow-up (design §15.2).
    /// `signature` layout: `pubKey(2592) ‖ sig(4627)`. The signed op preimage wraps
    /// `hash` with the ERC-1271 domain so a 1271 attestation can never be replayed as
    /// an `executeRoot` operation (distinct domain tag).
    function isValidSignature(bytes32 hash, bytes calldata signature) external view returns (bytes4) {
        if (signature.length != 2592 + 4627) {
            return 0xffffffff;
        }
        bytes calldata publicKey = signature[:2592];
        bytes calldata sig = signature[2592:];
        bytes memory preimage = abi.encodePacked("MISAKA_PQ_ERC1271_V1", uint256(block.chainid), address(this), hash);
        bytes memory input =
            abi.encodePacked(F003_VERSION_PREA_ROOT, operationalRootPayloadHi, operationalRootPayloadLo, publicKey, sig, preimage);
        (bool verified, bytes memory ret) = F003.staticcall(input);
        if (verified && ret.length == 32 && uint8(ret[31]) == 1) {
            return ERC1271_MAGIC;
        }
        return 0xffffffff;
    }

    // ------------------------------------------------------------------- secp256k1

    /// Recover a secp256k1 signer from a 65-byte `r ‖ s ‖ v` signature, rejecting the
    /// malleable high-`s` half (EIP-2) and `v ∉ {27,28}`. Returns address(0) on a bad
    /// signature (so the grant lookup then fails "session inactive").
    function _recover(bytes32 hash, bytes calldata ecdsaSig) internal pure returns (address) {
        if (ecdsaSig.length != 65) {
            return address(0);
        }
        bytes32 r = bytes32(ecdsaSig[0:32]);
        bytes32 s = bytes32(ecdsaSig[32:64]);
        uint8 v = uint8(ecdsaSig[64]);
        if (uint256(s) > SECP256K1N_HALF || (v != 27 && v != 28)) {
            return address(0);
        }
        return ecrecover(hash, v, r, s);
    }
}
