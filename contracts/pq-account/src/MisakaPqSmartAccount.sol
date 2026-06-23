// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// @title MISAKA PQ-Rooted EVM Smart Account (PREA design v1.1 §13/§14/§15, P0-2 + P1)
/// @notice An EVM account whose UNRESTRICTED authority is a post-quantum ML-DSA-87
///         key (NOT secp256k1), with a RESTRICTED secp256k1 "session" key for
///         frequent low-risk operations. Root authorization is verified on-chain by
///         the MISAKA F003 `MLDSA87_VERIFY` precompile (`0x…F003`, version 0x02);
///         session authorization is a normal secp256k1 signature gated by a grant.
///
///         Implemented: the ML-DSA root path (`executeRoot`), the offline Vault Owner
///         (`vaultExecute`: operational-root ROTATION / FREEZE / UNFREEZE), root-
///         authorized session grant/revoke, the restricted session path
///         (`executeSession`), and ERC-1271. P1 session policy (design §14/§15):
///         - a Merkle-committed (target, selector, policy) allowlist for large
///           allowlists without O(N) grant-time SSTOREs (`executeSessionWithProof`);
///         - a standard-aware non-native amount policy (ERC-20 amount caps, ERC-721
///           transfer-count caps + single-tokenId pin, ERC-1155 per-id amount caps),
///           with an EXPLICIT token-standard discriminator (never inferred from the
///           shared 0x23b872dd transferFrom selector);
///         - Permit2 deny-by-default for sessions (the canonical Permit2 address is a
///           denied target, plus its authority-granting selectors);
///         - ERC-1271 session signatures gated by a purpose RECOMPUTE: a session may
///           attest only KNOWN typed schemas whose hash the account recomputes and
///           matches, so a session cannot pass off a Permit/order digest as a benign
///           purpose (design §15.2). Raw hashes / unknown schemas / `Custom` are
///           default-rejected.
///         Deferred (documented below): a capped Permit2 path, ERC-721 multi-tokenId
///         Merkle sub-allowlists on the explicit path, full router/DEX sub-call decode,
///         and additional ERC-1271 schemas (NftListing/Permit).
///
///         ⚠️ F003 is consensus-FENCED INERT (activation = u64::MAX) on every MISAKA
///         network today, so a call to `0x…F003` returns empty data and `executeRoot`
///         REVERTS until F003 is governance-activated. The session paths (including the
///         ERC-1271 session-envelope path) are pure secp256k1/keccak and do NOT touch
///         F003, so they work whenever a grant exists. The contract + tests exist now
///         so the consumer is ready.
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
    /// ERC-1271 magic value for a valid signature, and its negation.
    bytes4 internal constant ERC1271_MAGIC = 0x1626ba7e;
    bytes4 internal constant ERC1271_INVALID = 0xffffffff;
    /// First byte of an ERC-1271 SESSION envelope ('S'); distinguishes it from the
    /// fixed-length ML-DSA root signature (length 2592+4627) without ambiguity.
    bytes1 internal constant ERC1271_ENVELOPE_TAG = 0x53;
    /// Fixed gas added to the account-measured execution gas when reimbursing a relayer
    /// (§16.3), approximating the unmeasured intrinsic + calldata + EntryPoint-dispatch
    /// + the reimbursement transfer itself. The signed `maxRelayerFee` is the HARD cap,
    /// so this approximation can never cause an overpayment beyond what the op authorized.
    uint256 internal constant FEE_OVERHEAD_GAS = 40_000;

    // Selectors a session may NEVER call (approval-as-delegation drains every value cap
    // by handing withdrawal rights to an external spender): ERC-20/721 `approve`,
    // ERC-721/1155 `setApprovalForAll`, and the common non-standard allowance-GROWING
    // selectors `increaseAllowance`/`increaseApproval`. DELEGATECALL is structurally
    // impossible (the account only ever `CALL`s).
    bytes4 internal constant SEL_APPROVE = 0x095ea7b3; // approve(address,uint256)
    bytes4 internal constant SEL_SET_APPROVAL_FOR_ALL = 0xa22cb465; // setApprovalForAll(address,bool)
    bytes4 internal constant SEL_INCREASE_ALLOWANCE = 0x39509351; // increaseAllowance(address,uint256)
    bytes4 internal constant SEL_INCREASE_APPROVAL = 0xd73dd623; // increaseApproval(address,uint256)
    // ERC-20 transfer selectors whose amount IS decoded + capped when an ERC-20 policy
    // sets a cap. NOTE: 0x23b872dd is SHARED by ERC-20 transferFrom and ERC-721
    // transferFrom — the policy's token-standard discriminator (NOT the selector)
    // decides whether the 3rd word is an amount or a tokenId.
    bytes4 internal constant SEL_TRANSFER = 0xa9059cbb; // transfer(address,uint256)
    bytes4 internal constant SEL_TRANSFER_FROM = 0x23b872dd; // transferFrom(address,address,uint256)
    // ERC-721 safe transfers (each moves exactly one tokenId).
    bytes4 internal constant SEL_SAFE_TRANSFER_FROM = 0x42842e0e; // safeTransferFrom(address,address,uint256)
    bytes4 internal constant SEL_SAFE_TRANSFER_FROM_DATA = 0xb88d4fde; // safeTransferFrom(address,address,uint256,bytes)
    // ERC-1155 transfers.
    bytes4 internal constant SEL_ERC1155_SAFE_TRANSFER = 0xf242432a; // safeTransferFrom(address,address,uint256,uint256,bytes)
    bytes4 internal constant SEL_ERC1155_SAFE_BATCH = 0x2eb2c2d6; // safeBatchTransferFrom(...)

    // --- Permit2 deny-by-default (design §14.2) ---
    /// The canonical Uniswap Permit2 contract. A session may NEVER call it (all
    /// approval/transfer authority routed through Permit2 is uncapped from this
    /// account's perspective). The off-chain Permit2 SIGNATURE vector is handled by the
    /// ERC-1271 session-purpose recompute (a `Permit` purpose is not a known schema →
    /// rejected).
    address internal constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;
    // Permit2 authority-GRANTING selectors (denied even on a Permit2 fork/clone at a
    // different address — defence-in-depth on top of the canonical-address deny above,
    // which is the primary guard). Covers AllowanceTransfer (approve / transferFrom /
    // batch transferFrom / lockdown) and SignatureTransfer (permit single+batch /
    // permitTransferFrom single+batch).
    bytes4 internal constant SEL_P2_APPROVE = 0x87517c45; // approve(address,address,uint160,uint48)
    bytes4 internal constant SEL_P2_TRANSFER_FROM = 0x36c78516; // transferFrom(address,address,uint160,address)
    bytes4 internal constant SEL_P2_TRANSFER_FROM_BATCH = 0x0d58b1db; // transferFrom((address,address,uint160,address)[])
    bytes4 internal constant SEL_P2_PERMIT_SINGLE = 0x2b67b570; // permit(address,PermitSingle,bytes)
    bytes4 internal constant SEL_P2_PERMIT_BATCH = 0x2a2d80d1; // permit(address,PermitBatch,bytes)
    bytes4 internal constant SEL_P2_PERMIT_TRANSFER_FROM = 0x30f28b7a; // permitTransferFrom(...)
    bytes4 internal constant SEL_P2_PERMIT_TRANSFER_FROM_BATCH = 0xa0f0f0d5; // permitTransferFrom(... batch)
    bytes4 internal constant SEL_P2_LOCKDOWN = 0xcc53287f; // lockdown((address,address)[])

    // --- Merkle / ERC-1271 schema domains ---
    /// Leaf domain tag (double-hashed leaves are domain-separated from internal nodes →
    /// second-preimage-safe; commutative sorted-pair internal hashing needs no index bits).
    bytes32 internal constant LEAF_DOMAIN = keccak256("MISAKA_PQ_SESSION_POLICY_LEAF_V1");
    /// EIP-712 type hashes for the KNOWN ERC-1271 session schemas the account recomputes.
    /// FROZEN: an off-chain signer/wallet reproduces these byte-for-byte.
    bytes32 internal constant LOGIN_TYPEHASH =
        keccak256("MisakaLogin(address account,address sessionKey,uint64 grantId,bytes32 statement,uint256 deadline)");
    bytes32 internal constant ORDER_TYPEHASH = keccak256(
        "MisakaOrder(address account,address sessionKey,uint64 grantId,address collection,uint256 tokenId,uint256 amount,uint256 deadline)"
    );
    /// The ONLY ERC-1271 purposes a grant may opt into (the schemas the account can
    /// recompute today: Login + Order). Deny-by-default: granting any other bit
    /// (NftListing/Permit reserved, or Custom) reverts — so a future schema impl can
    /// never silently activate a reserved bit on an already-granted session.
    uint32 internal constant ALLOWED_PURPOSE_MASK =
        (uint32(1) << uint8(SignaturePurpose.Login)) | (uint32(1) << uint8(SignaturePurpose.Order));

    /// Non-native asset standard a session policy entry governs. EXPLICIT discriminator:
    /// it is NEVER inferred from the selector (0x23b872dd is shared by ERC-20 and
    /// ERC-721 transferFrom).
    enum TokenStandard {
        NATIVE, // 0 — no token decode (pure native-value or non-asset call)
        ERC20, // 1 — amount cap on transfer/transferFrom
        ERC721, // 2 — transfer-count cap; optional single-tokenId pin (proof path)
        ERC1155 // 3 — per-(pinned id) amount cap on safeTransferFrom
    }

    /// ERC-1271 session signature purposes. Only KNOWN schemas (Login, Order) are
    /// recomputable; NftListing/Permit are reserved (default-rejected) and `Custom` is
    /// never grantable.
    enum SignaturePurpose {
        Login, // 0
        NftListing, // 1 (reserved — not a known schema yet → rejected)
        Permit, // 2 (reserved — rejected; Permit2 off-chain vector closed by default)
        Order, // 3
        Custom // 4 (never grantable, never valid)
    }

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

    /// A per-(target,selector) session policy entry. `standard` is the EXPLICIT token
    /// discriminator. `maxPerCall`: ERC-20/1155 per-call amount cap (0 = no amount
    /// semantics). `maxTotal`: cumulative cap (ERC-20/1155 = summed amount; ERC-721 =
    /// transfer count) across the grant generation (0 = uncapped beyond `maxCalls`).
    /// `erc1155TokenId`: the pinned token id an ERC-1155 entry may move.
    struct Allow {
        bool allowed;
        TokenStandard standard;
        uint256 maxPerCall;
        uint256 maxTotal;
        uint256 erc1155TokenId;
    }

    /// One entry of an explicit `grantSessionV2` policy. `targetSelectorKey` =
    /// `allowKey(target, selector)`.
    struct PolicyEntry {
        bytes32 targetSelectorKey;
        uint8 standard;
        uint256 maxPerCall;
        uint256 maxTotal;
        uint256 erc1155TokenId;
    }

    /// A Merkle leaf for the proof path (calldata-only; the tree lives off-chain and
    /// only its root is on-chain). Commits the FULL per-pair policy so the proof
    /// authorizes the policy, not just the (target,selector) pair. `codeHashPin`:
    /// 0 = skip, else require `target.codehash == codeHashPin` (§14.5 proxy-swap
    /// defence). `extraCap`: for ERC-721/1155, the pinned tokenId + 1 (0 = any id for
    /// ERC-721; REQUIRED, non-zero, for ERC-1155); MUST be 0 for NATIVE/ERC-20.
    struct PolicyLeaf {
        address target;
        bytes4 selector;
        uint8 tokenStandard;
        bytes32 codeHashPin;
        uint256 maxPerCall;
        uint256 maxTotal;
        uint256 extraCap;
    }

    /// The unified in-memory policy view fed to `_checkAndConsumeTokenPolicy`,
    /// populated from a storage `Allow` (explicit path) OR a verified `PolicyLeaf`
    /// (proof path). `tokenId`/`hasTokenIdPin` carry an ERC-721/1155 id restriction.
    /// `policyKey` keys the cumulative `tokenUsed` counter (namespaced per path).
    struct ResolvedPolicy {
        TokenStandard standard;
        uint256 maxPerCall;
        uint256 maxTotal;
        uint256 tokenId;
        bool hasTokenIdPin;
        bytes32 policyKey;
    }

    /// An ERC-1271 SESSION envelope (calldata-decoded from `signature[1:]`). The session
    /// declares the purpose + typed payload; the account RECOMPUTES the schema hash and
    /// requires it to equal the `hash` argument (design §15.2). `secpSig` is a 65-byte
    /// session signature over that same `hash`.
    struct Erc1271Envelope {
        uint8 purpose;
        address sessionKey;
        bytes32 domainSeparator;
        address collection;
        uint256 tokenId;
        uint256 amount;
        uint256 deadline;
        uint64 grantId;
        bytes32 statement;
        // Session signature over `hash` (the recomputed schema digest), split so it can
        // be recovered from a memory-decoded envelope (bytes-memory slicing is unsupported).
        bytes32 sigR;
        bytes32 sigS;
        uint8 sigV;
    }

    /// session key (secp256k1 address) → grant.
    mapping(address => SessionGrant) public sessions;
    /// session key → grant generation. Bumped on every (re-)grant so a re-grant of the
    /// SAME key starts a fresh allowlist generation — re-granting with a narrower
    /// allowlist can never leave stale (broader) entries live (mappings aren't
    /// enumerable to clear, so all gen-scoped lookups are generation-scoped instead).
    mapping(address => uint64) public sessionGrantGen;
    /// session key → strictly-monotonic call nonce. The session op hash binds the
    /// `callIndex`, and this counter is the value `callIndex` must equal. Crucially it
    /// is NOT reset by `_newGrant` (unlike the per-grant `callsUsed` budget), so after a
    /// same-key RE-GRANT a stale signature for an already-consumed index can never be
    /// replayed under the new generation (the new generation continues from the same
    /// nonce). `callsUsed`/`maxCalls` remain the per-generation spend budget.
    mapping(address => uint64) public sessionNonce;
    /// session key → grantGen → keccak256(target ‖ selector) → explicit allowance.
    mapping(address => mapping(uint64 => mapping(bytes32 => Allow))) public allows;
    /// session key → grantGen → committed Merkle root of `PolicyLeaf`s (proof path).
    /// 0 = no proof-path policy for this generation (fail-closed).
    mapping(address => mapping(uint64 => bytes32)) public sessionPolicyRoot;
    /// session key → grantGen → policyKey → cumulative ERC-20/1155 amount OR ERC-721
    /// transfer COUNT. Gen-scoped, so a re-grant orphans it to 0. NOTE: the cumulative
    /// budget is per-(target,selector) [explicit] / per-(PROOF,target,selector) [proof],
    /// NOT per-asset — granting a session BOTH `transfer` and `transferFrom` for the
    /// same ERC-20 gives it the SUM of the two caps. Grant narrowly.
    mapping(address => mapping(uint64 => mapping(bytes32 => uint256))) public tokenUsed;
    /// session key → grantGen → ERC-1271 purpose bitmask (bit i = SignaturePurpose(i)
    /// allowed). Default 0 = deny-all session ERC-1271.
    mapping(address => mapping(uint64 => uint32)) public sessionPurposeMask;

    event RootExecuted(uint64 indexed nonce, address indexed target, uint256 value, bool success);
    event SessionGranted(address indexed sessionKey, uint64 validUntilBlock, uint64 maxCalls, uint128 maxNativeTotal);
    event SessionRevoked(address indexed sessionKey);
    event SessionExecuted(address indexed sessionKey, uint64 callIndex, address indexed target, uint256 value);
    event SessionPurposesSet(address indexed sessionKey, uint64 indexed grantGen, uint32 purposeMask);
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
    /// validAfter(8) ‖ validUntil(8) ‖ maxRelayerFee(32) ‖ target(20) ‖ value(32) ‖
    /// callData. `maxRelayerFee` is signed so a relayer can never claim a fee the user
    /// did not authorize for THIS op.
    function _opPreimage(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 validAfterBlock,
        uint64 validUntilBlock,
        uint64 nonce,
        uint256 maxRelayerFee
    ) internal view returns (bytes memory) {
        return abi.encodePacked(
            OP_DOMAIN,
            uint256(block.chainid),
            address(this),
            accountVersion,
            nonce,
            validAfterBlock,
            validUntilBlock,
            maxRelayerFee,
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
        bytes calldata signature,
        uint256 maxRelayerFee
    ) external returns (bytes memory) {
        uint256 gasStart = gasleft();
        require(!frozen, "PQ: account frozen");
        require(nonce == rootNonce, "PQ: bad nonce");
        require(block.number >= validAfterBlock && block.number <= validUntilBlock, "PQ: outside validity window");
        require(publicKey.length == 2592 && signature.length == 4627, "PQ: bad key/sig length");

        bytes memory preimage = _opPreimage(target, value, callData, validAfterBlock, validUntilBlock, nonce, maxRelayerFee);
        bytes memory input =
            abi.encodePacked(F003_VERSION_PREA_ROOT, operationalRootPayloadHi, operationalRootPayloadLo, publicKey, signature, preimage);

        (bool verified, bytes memory ret) = F003.staticcall(input);
        require(verified && ret.length == 32 && uint8(ret[31]) == 1, "PQ: ml-dsa root auth failed");

        rootNonce = nonce + 1; // effects before interaction (replay + reentrancy guard)

        (bool success, bytes memory result) = target.call{value: value}(callData);
        require(success, "PQ: target call reverted");
        emit RootExecuted(nonce, target, value, success);
        _reimburseRelayer(gasStart, maxRelayerFee);
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

    /// Legacy explicit grant: every entry is treated as an ERC-20 policy (its
    /// `maxAmounts[i]` is the per-call amount cap; 0 = no amount semantics) — exactly
    /// the pre-P1 behaviour. For ERC-721/1155 caps or a Merkle policy root use
    /// `grantSessionV2` / `grantSessionWithRoot`.
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

        uint64 gen = _newGrant(sessionKey, validUntilBlock, maxCalls, maxNativeTotal);
        for (uint256 i; i < targetSelectorKeys.length; i++) {
            allows[sessionKey][gen][targetSelectorKeys[i]] = Allow({
                allowed: true,
                standard: TokenStandard.ERC20,
                maxPerCall: maxAmounts[i],
                maxTotal: 0,
                erc1155TokenId: 0
            });
        }
        emit SessionGranted(sessionKey, validUntilBlock, maxCalls, maxNativeTotal);
    }

    /// Explicit grant with per-entry token-standard policy (design §14.6). Each
    /// `PolicyEntry` carries the EXPLICIT standard + caps. ERC-721 entries are
    /// transfer-COUNT capped (`maxTotal`), so `maxPerCall` must be ≤ 1.
    function grantSessionV2(
        address sessionKey,
        uint64 validUntilBlock,
        uint64 maxCalls,
        uint128 maxNativeTotal,
        PolicyEntry[] calldata entries
    ) external {
        require(msg.sender == address(this), "PQ: only root (via executeRoot)");
        require(sessionKey != address(0), "PQ: zero session key");

        uint64 gen = _newGrant(sessionKey, validUntilBlock, maxCalls, maxNativeTotal);
        for (uint256 i; i < entries.length; i++) {
            PolicyEntry calldata e = entries[i];
            require(e.standard <= uint8(TokenStandard.ERC1155), "PQ: bad standard");
            TokenStandard std = TokenStandard(e.standard);
            if (std == TokenStandard.ERC721) {
                require(e.maxPerCall <= 1, "PQ: erc721 per-call must be <=1");
            }
            if (std == TokenStandard.NATIVE) {
                require(e.maxPerCall == 0 && e.maxTotal == 0 && e.erc1155TokenId == 0, "PQ: native entry must be zero");
            }
            allows[sessionKey][gen][e.targetSelectorKey] = Allow({
                allowed: true,
                standard: std,
                maxPerCall: e.maxPerCall,
                maxTotal: e.maxTotal,
                erc1155TokenId: e.erc1155TokenId
            });
        }
        emit SessionGranted(sessionKey, validUntilBlock, maxCalls, maxNativeTotal);
    }

    /// Grant whose policy is a Merkle root of `PolicyLeaf`s, consumed by
    /// `executeSessionWithProof` (design §14, large allowlists with no O(N) grant-time
    /// SSTOREs). A grant generation may carry EITHER explicit `allows` (V2) OR a policy
    /// root (this) OR both — they are independent lookups.
    function grantSessionWithRoot(
        address sessionKey,
        uint64 validUntilBlock,
        uint64 maxCalls,
        uint128 maxNativeTotal,
        bytes32 policyMerkleRoot
    ) external {
        require(msg.sender == address(this), "PQ: only root (via executeRoot)");
        require(sessionKey != address(0), "PQ: zero session key");
        require(policyMerkleRoot != bytes32(0), "PQ: zero policy root");

        uint64 gen = _newGrant(sessionKey, validUntilBlock, maxCalls, maxNativeTotal);
        sessionPolicyRoot[sessionKey][gen] = policyMerkleRoot;
        emit SessionGranted(sessionKey, validUntilBlock, maxCalls, maxNativeTotal);
    }

    /// Opt a session generation into ERC-1271 attestation purposes (design §15). Must
    /// be called for the CURRENT generation (after the matching grant); a subsequent
    /// re-grant bumps the generation and zeroes the mask. `Custom` can never be allowed.
    function grantSessionPurposes(address sessionKey, uint32 purposeMask) external {
        require(msg.sender == address(this), "PQ: only root (via executeRoot)");
        require(sessions[sessionKey].active, "PQ: no active grant");
        // Deny-by-default: only the recomputable schemas (Login/Order) may be granted.
        // Reserved (NftListing/Permit) and Custom bits revert — closes the latent
        // footgun of a reserved bit silently activating if a schema is added later.
        require(purposeMask & ~ALLOWED_PURPOSE_MASK == 0, "PQ: purpose not allowed");
        uint64 gen = sessionGrantGen[sessionKey];
        sessionPurposeMask[sessionKey][gen] = purposeMask;
        emit SessionPurposesSet(sessionKey, gen, purposeMask);
    }

    /// Open a fresh grant generation for `sessionKey` and set the shared `SessionGrant`
    /// fields. Bumping the generation orphans the prior generation's allows / policy
    /// root / purpose mask / cumulative counters together.
    function _newGrant(address sessionKey, uint64 validUntilBlock, uint64 maxCalls, uint128 maxNativeTotal)
        internal
        returns (uint64 gen)
    {
        gen = sessionGrantGen[sessionKey] + 1;
        sessionGrantGen[sessionKey] = gen;
        SessionGrant storage g = sessions[sessionKey];
        g.active = true;
        g.validUntilBlock = validUntilBlock;
        g.maxCalls = maxCalls;
        g.callsUsed = 0;
        g.maxNativeTotal = maxNativeTotal;
        g.nativeUsed = 0;
        g.rootEpoch = rootEpoch;
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
        uint64 callIndex,
        uint256 maxRelayerFee
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
                callIndex,
                maxRelayerFee
            )
        );
    }

    /// Shared pre-authorization gate (BOTH session entrypoints). Runs BEFORE recover /
    /// allowlist / proof so the structural denies (frozen, self-target, approval
    /// selectors, Permit2) are path-independent and cannot be bypassed by any policy.
    function _frontGuards(address target, bytes calldata callData) internal view returns (bytes4 sel) {
        // A session must NEVER call the account itself: grantSession/revokeSession gate
        // only on `msg.sender == address(this)`, so a self-call would let a session
        // allowlisted for (address(this), grantSession) escalate to granting itself
        // unlimited sessions. Fail closed regardless of any allowlist/proof.
        require(!frozen, "PQ: account frozen");
        require(target != address(this), "PQ: session cannot target self");
        require(callData.length >= 4, "PQ: calldata too short");
        sel = bytes4(callData[:4]);
        require(!_isApprovalSelector(sel), "PQ: forbidden selector");
        require(target != PERMIT2, "PQ: permit2 target denied");
        require(!_isPermit2Selector(sel), "PQ: permit2 selector denied");
    }

    /// Allowance-as-delegation selectors a session may never call (approve grants an
    /// external spender unbounded pull rights, bypassing every value cap).
    function _isApprovalSelector(bytes4 sel) internal pure returns (bool) {
        return sel == SEL_APPROVE || sel == SEL_SET_APPROVAL_FOR_ALL || sel == SEL_INCREASE_ALLOWANCE
            || sel == SEL_INCREASE_APPROVAL;
    }

    /// Permit2 authority-granting selectors (clone/fork defence; the canonical address
    /// is denied wholesale in `_frontGuards`).
    function _isPermit2Selector(bytes4 sel) internal pure returns (bool) {
        return sel == SEL_P2_APPROVE || sel == SEL_P2_TRANSFER_FROM || sel == SEL_P2_TRANSFER_FROM_BATCH
            || sel == SEL_P2_PERMIT_SINGLE || sel == SEL_P2_PERMIT_BATCH || sel == SEL_P2_PERMIT_TRANSFER_FROM
            || sel == SEL_P2_PERMIT_TRANSFER_FROM_BATCH || sel == SEL_P2_LOCKDOWN;
    }

    function _recoverSessionKey(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 callIndex,
        uint256 maxRelayerFee,
        bytes calldata ecdsaSig
    ) internal view returns (address sk) {
        sk = _recover(_sessionOpHash(target, value, callData, callIndex, maxRelayerFee), ecdsaSig);
        require(sk != address(0), "PQ: bad session signature");
    }

    /// Execute one session operation against the EXPLICIT (target,selector) allowlist.
    /// `ecdsaSig` is a 65-byte secp256k1 signature by the granted session key over
    /// `_sessionOpHash(...)`. CALL only — never delegatecall.
    function executeSession(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 callIndex,
        bytes calldata ecdsaSig,
        uint256 maxRelayerFee
    ) external returns (bytes memory) {
        uint256 gasStart = gasleft();
        bytes4 sel = _frontGuards(target, callData);
        address sk = _recoverSessionKey(target, value, callData, callIndex, maxRelayerFee, ecdsaSig);
        // Grant active/epoch FIRST (so an ungranted/revoked/rotated key reports
        // "session inactive", not "target/selector not allowed").
        require(sessions[sk].active && sessions[sk].rootEpoch == rootEpoch, "PQ: session inactive");

        bytes32 pk = allowKey(target, sel);
        Allow storage a = allows[sk][sessionGrantGen[sk]][pk];
        require(a.allowed, "PQ: target/selector not allowed");
        bytes memory result = _runSession(target, value, callData, callIndex, sk, _resolveFromAllow(a, pk));
        _reimburseRelayer(gasStart, maxRelayerFee);
        return result;
    }

    /// Execute one session operation authorized by a Merkle proof against the grant's
    /// committed `sessionPolicyRoot` (design §14). The `leaf` + `proof` are UNSIGNED
    /// (supplied by the submitter) but must hash into the root the ML-DSA root
    /// committed at grant time, so a relayer can never forge a broader policy.
    function executeSessionWithProof(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 callIndex,
        bytes calldata ecdsaSig,
        PolicyLeaf calldata leaf,
        bytes32[] calldata proof,
        uint256 maxRelayerFee
    ) external returns (bytes memory) {
        uint256 gasStart = gasleft();
        bytes4 sel = _frontGuards(target, callData);
        require(leaf.target == target && leaf.selector == sel, "PQ: leaf/call mismatch");
        address sk = _recoverSessionKey(target, value, callData, callIndex, maxRelayerFee, ecdsaSig);
        require(sessions[sk].active && sessions[sk].rootEpoch == rootEpoch, "PQ: session inactive");

        bytes32 root = sessionPolicyRoot[sk][sessionGrantGen[sk]];
        require(root != bytes32(0), "PQ: no policy root");
        require(_verifyMerkle(proof, root, _leafHash(leaf)), "PQ: bad merkle proof");
        // §14.5: pin the target's own bytecode if requested (defends against a
        // non-delegating implementation swap and against calling an EOA/empty account).
        if (leaf.codeHashPin != bytes32(0)) {
            require(target.codehash == leaf.codeHashPin, "PQ: code-hash mismatch");
        }
        bytes memory result =
            _runSession(target, value, callData, callIndex, sk, _resolveFromLeaf(leaf, _proofPolicyKey(target, sel)));
        _reimburseRelayer(gasStart, maxRelayerFee);
        return result;
    }

    /// Shared post-authorization core (BOTH session entrypoints). The caller has
    /// already validated front guards, recovered `sessionKey`, validated the grant is
    /// active + on-epoch, and AUTHORIZED the (target,selector,policy) tuple (explicit
    /// allow OR Merkle proof). This enforces the remaining grant gates + token policy,
    /// then performs effects-before-interaction.
    function _runSession(
        address target,
        uint256 value,
        bytes calldata callData,
        uint64 callIndex,
        address sessionKey,
        ResolvedPolicy memory pol
    ) internal returns (bytes memory) {
        SessionGrant storage g = sessions[sessionKey];
        require(block.number <= g.validUntilBlock, "PQ: session expired");
        // Replay nonce is the per-key MONOTONIC `sessionNonce` (survives re-grants), not
        // the per-grant `callsUsed` (which resets) — so a stale signature for an
        // already-consumed index cannot be replayed under a new generation.
        require(callIndex == sessionNonce[sessionKey], "PQ: bad session call index");
        require(g.callsUsed < g.maxCalls, "PQ: session call cap");
        require(uint256(value) + uint256(g.nativeUsed) <= uint256(g.maxNativeTotal), "PQ: session native cap");

        _checkAndConsumeTokenPolicy(sessionKey, sessionGrantGen[sessionKey], pol, bytes4(callData[:4]), callData);

        sessionNonce[sessionKey] += 1; // monotonic replay guard (effects before interaction)
        g.callsUsed += 1; // per-generation spend budget
        g.nativeUsed += uint128(value);

        (bool success, bytes memory result) = target.call{value: value}(callData);
        require(success, "PQ: session call reverted");
        emit SessionExecuted(sessionKey, callIndex, target, value);
        return result;
    }

    /// Standard-aware amount/count decode + per-call & cumulative caps. Branches ONLY on
    /// `pol.standard` (NEVER on the selector — 0x23b872dd is shared by ERC-20/721).
    function _checkAndConsumeTokenPolicy(
        address sessionKey,
        uint64 gen,
        ResolvedPolicy memory pol,
        bytes4 sel,
        bytes calldata callData
    ) internal {
        if (pol.standard == TokenStandard.NATIVE) {
            return; // no token semantics
        }
        if (pol.standard == TokenStandard.ERC20) {
            // Decode only when a cap is set (matches the pre-P1 behaviour: an ERC-20
            // entry with no cap authorizes the (target,selector) without inspecting
            // calldata, so a non-transfer selector is fine).
            if (pol.maxPerCall != 0 || pol.maxTotal != 0) {
                uint256 amt = _decodeErc20Amount(sel, callData);
                if (pol.maxPerCall != 0) {
                    require(amt <= pol.maxPerCall, "PQ: token amount cap");
                }
                if (pol.maxTotal != 0) {
                    uint256 used = tokenUsed[sessionKey][gen][pol.policyKey] + amt;
                    require(used <= pol.maxTotal, "PQ: token total cap");
                    tokenUsed[sessionKey][gen][pol.policyKey] = used;
                }
            }
            return;
        }
        if (pol.standard == TokenStandard.ERC721) {
            // Decode only when an id pin or count cap is set (an unrestricted ERC-721
            // entry just authorizes the (target,selector), bounded by maxCalls).
            if (pol.hasTokenIdPin || pol.maxTotal != 0) {
                uint256 tokenId = _decodeErc721TokenId(sel, callData);
                if (pol.hasTokenIdPin) {
                    require(tokenId == pol.tokenId, "PQ: tokenId not allowed");
                }
                if (pol.maxTotal != 0) {
                    uint256 used = tokenUsed[sessionKey][gen][pol.policyKey] + 1;
                    require(used <= pol.maxTotal, "PQ: nft transfer count cap");
                    tokenUsed[sessionKey][gen][pol.policyKey] = used;
                }
            }
            return;
        }
        // ERC1155
        if (sel == SEL_ERC1155_SAFE_BATCH) {
            revert("PQ: 1155 batch not supported");
        }
        require(sel == SEL_ERC1155_SAFE_TRANSFER, "PQ: amount cap unsupported for selector");
        (uint256 id, uint256 amount) = _decodeErc1155Single(callData);
        require(id == pol.tokenId, "PQ: 1155 id mismatch");
        if (pol.maxPerCall != 0) {
            require(amount <= pol.maxPerCall, "PQ: token amount cap");
        }
        if (pol.maxTotal != 0) {
            uint256 used = tokenUsed[sessionKey][gen][pol.policyKey] + amount;
            require(used <= pol.maxTotal, "PQ: token total cap");
            tokenUsed[sessionKey][gen][pol.policyKey] = used;
        }
    }

    function _resolveFromAllow(Allow storage a, bytes32 key) internal view returns (ResolvedPolicy memory pol) {
        pol.standard = a.standard;
        pol.maxPerCall = a.maxPerCall;
        pol.maxTotal = a.maxTotal;
        pol.policyKey = key;
        if (a.standard == TokenStandard.ERC1155) {
            pol.tokenId = a.erc1155TokenId;
            pol.hasTokenIdPin = true; // ERC-1155 always pins a single id on the explicit path
        }
        // ERC-721 explicit path: no single-id pin (count cap only) — use the proof path
        // (leaf.extraCap) or multiple leaves to restrict tokenIds.
    }

    function _resolveFromLeaf(PolicyLeaf calldata leaf, bytes32 key) internal pure returns (ResolvedPolicy memory pol) {
        require(leaf.tokenStandard <= uint8(TokenStandard.ERC1155), "PQ: bad standard");
        pol.standard = TokenStandard(leaf.tokenStandard);
        pol.maxPerCall = leaf.maxPerCall;
        pol.maxTotal = leaf.maxTotal;
        pol.policyKey = key;
        if (pol.standard == TokenStandard.ERC721) {
            // Parity with grantSessionV2: an ERC-721 transfer moves exactly one token,
            // so maxPerCall is meaningless above 1 (the count cap is `maxTotal`).
            require(leaf.maxPerCall <= 1, "PQ: erc721 per-call must be <=1");
            if (leaf.extraCap != 0) {
                pol.tokenId = leaf.extraCap - 1; // extraCap = tokenId + 1 (0 = any)
                pol.hasTokenIdPin = true;
            }
        } else if (pol.standard == TokenStandard.ERC1155) {
            require(leaf.extraCap != 0, "PQ: 1155 id required");
            pol.tokenId = leaf.extraCap - 1;
            pol.hasTokenIdPin = true;
        } else {
            // NATIVE / ERC20 carry no tokenId pin.
            require(leaf.extraCap == 0, "PQ: extraCap must be 0");
        }
    }

    /// Proof-path cumulative counters are namespaced apart from the explicit path so a
    /// session holding BOTH an explicit and a proof policy for the same (target,selector)
    /// never shares a counter.
    function _proofPolicyKey(address target, bytes4 sel) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked("PROOF", allowKey(target, sel)));
    }

    /// Decode the capped ERC-20 amount for `transfer`/`transferFrom`. A non-zero cap on
    /// any other selector is unverifiable → reject (fail-closed).
    function _decodeErc20Amount(bytes4 sel, bytes calldata callData) internal pure returns (uint256) {
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

    /// Decode the moved tokenId for any of the three ERC-721 transfer selectors. The
    /// tokenId sits at the 3rd argument word for all of them (the trailing `data` in
    /// 0xb88d4fde sits past this fixed head and is not read).
    function _decodeErc721TokenId(bytes4 sel, bytes calldata callData) internal pure returns (uint256) {
        if (sel == SEL_TRANSFER_FROM || sel == SEL_SAFE_TRANSFER_FROM || sel == SEL_SAFE_TRANSFER_FROM_DATA) {
            require(callData.length >= 4 + 96, "PQ: bad erc721 calldata");
            return uint256(bytes32(callData[68:100]));
        }
        revert("PQ: amount cap unsupported for selector");
    }

    /// Decode (id, amount) from ERC-1155 `safeTransferFrom(from,to,id,amount,data)`.
    /// head = from(32) ‖ to(32) ‖ id(32) ‖ amount(32) ‖ data_offset(32) = 160 bytes.
    function _decodeErc1155Single(bytes calldata callData) internal pure returns (uint256 id, uint256 amount) {
        require(callData.length >= 4 + 160, "PQ: bad erc1155 calldata");
        id = uint256(bytes32(callData[68:100]));
        amount = uint256(bytes32(callData[100:132]));
    }

    /// OpenZeppelin-style commutative sorted-pair Merkle verification (no index bits).
    function _verifyMerkle(bytes32[] calldata proof, bytes32 root, bytes32 leaf) internal pure returns (bool) {
        bytes32 computed = leaf;
        for (uint256 i; i < proof.length; i++) {
            bytes32 p = proof[i];
            computed = computed <= p ? keccak256(abi.encodePacked(computed, p)) : keccak256(abi.encodePacked(p, computed));
        }
        return computed == root;
    }

    /// OZ StandardMerkleTree leaf hashing: double-keccak with a domain tag in the inner
    /// hash (leaf-vs-node second-preimage separation; abi.encode 32-byte slots avoid
    /// packed ambiguity).
    function _leafHash(PolicyLeaf calldata leaf) internal pure returns (bytes32) {
        return keccak256(
            bytes.concat(
                keccak256(
                    abi.encode(
                        LEAF_DOMAIN,
                        leaf.target,
                        leaf.selector,
                        leaf.tokenStandard,
                        leaf.codeHashPin,
                        leaf.maxPerCall,
                        leaf.maxTotal,
                        leaf.extraCap
                    )
                )
            )
        );
    }

    // --------------------------------------------------------------------- ERC-1271

    /// ERC-1271. Two valid shapes:
    /// (1) ROOT: `signature` = `pubKey(2592) ‖ sig(4627)` (length 7219) — an ML-DSA root
    ///     signature verified via F003 v0x02 over the 1271 `hash` wrapped in the
    ///     ERC-1271 domain. (INERT today: F003 returns empty ⇒ 0xffffffff.)
    /// (2) SESSION: `signature[0] == 'S'` then an abi-encoded `Erc1271Envelope`. Valid
    ///     ONLY when the account RECOMPUTES the declared KNOWN schema's hash and it
    ///     equals `hash`, the grant opted into that purpose, the session amount caps are
    ///     respected, and the recovered signer is the granted session key (design §15.2).
    /// Everything else (raw hashes, unknown schema, `Custom`, bad length) ⇒ 0xffffffff.
    function isValidSignature(bytes32 hash, bytes calldata signature) external view returns (bytes4) {
        if (signature.length == 2592 + 4627) {
            bytes calldata publicKey = signature[:2592];
            bytes calldata sig = signature[2592:];
            bytes memory preimage = abi.encodePacked("MISAKA_PQ_ERC1271_V1", uint256(block.chainid), address(this), hash);
            bytes memory input = abi.encodePacked(
                F003_VERSION_PREA_ROOT, operationalRootPayloadHi, operationalRootPayloadLo, publicKey, sig, preimage
            );
            (bool verified, bytes memory ret) = F003.staticcall(input);
            if (verified && ret.length == 32 && uint8(ret[31]) == 1) {
                return ERC1271_MAGIC;
            }
            return ERC1271_INVALID;
        }
        if (signature.length >= 1 && signature[0] == ERC1271_ENVELOPE_TAG) {
            return _validateSessionEnvelope(hash, signature[1:]);
        }
        return ERC1271_INVALID;
    }

    /// Validate an ERC-1271 SESSION envelope. FAIL-CLOSED: every failure path returns
    /// 0xffffffff (a malformed abi-decode reverts, which a spec-compliant verifier also
    /// treats as invalid).
    function _validateSessionEnvelope(bytes32 hash, bytes calldata envBytes) internal view returns (bytes4) {
        if (frozen) {
            return ERC1271_INVALID;
        }
        Erc1271Envelope memory e = abi.decode(envBytes, (Erc1271Envelope));
        // Reject Custom (4) and any unknown purpose value up-front.
        if (e.purpose >= uint8(SignaturePurpose.Custom)) {
            return ERC1271_INVALID;
        }
        uint64 gen = sessionGrantGen[e.sessionKey];
        SessionGrant storage g = sessions[e.sessionKey];
        if (!g.active || g.rootEpoch != rootEpoch) {
            return ERC1271_INVALID;
        }
        if (block.number > g.validUntilBlock) {
            return ERC1271_INVALID;
        }
        if (e.deadline != 0 && block.number > e.deadline) {
            return ERC1271_INVALID;
        }
        if (e.grantId != gen) {
            return ERC1271_INVALID;
        }
        if ((sessionPurposeMask[e.sessionKey][gen] & (uint32(1) << e.purpose)) == 0) {
            return ERC1271_INVALID;
        }
        (bool known, bytes32 expected) = _hashKnownSchema(e);
        if (!known || expected != hash) {
            return ERC1271_INVALID;
        }
        if (!_envelopeWithinCaps(e)) {
            return ERC1271_INVALID;
        }
        address signer = _recoverRSV(hash, e.sigR, e.sigS, e.sigV);
        if (signer == address(0) || signer != e.sessionKey) {
            return ERC1271_INVALID;
        }
        return ERC1271_MAGIC;
    }

    /// Recompute the EIP-712 digest for a KNOWN schema from the declared typed fields.
    /// Returns (false, 0) for reserved/unknown purposes (NftListing, Permit) so they are
    /// default-rejected. Binds account + sessionKey + grantId into the struct hash.
    function _hashKnownSchema(Erc1271Envelope memory e) internal view returns (bool known, bytes32 digest) {
        if (e.purpose == uint8(SignaturePurpose.Login)) {
            bytes32 structHash =
                keccak256(abi.encode(LOGIN_TYPEHASH, address(this), e.sessionKey, e.grantId, e.statement, e.deadline));
            return (true, keccak256(abi.encodePacked(hex"1901", e.domainSeparator, structHash)));
        }
        if (e.purpose == uint8(SignaturePurpose.Order)) {
            bytes32 structHash = keccak256(
                abi.encode(
                    ORDER_TYPEHASH, address(this), e.sessionKey, e.grantId, e.collection, e.tokenId, e.amount, e.deadline
                )
            );
            return (true, keccak256(abi.encodePacked(hex"1901", e.domainSeparator, structHash)));
        }
        return (false, bytes32(0));
    }

    /// A session ERC-1271 attestation may not authorize more than the session could
    /// spend on-chain (design §15.3). Login carries no asset. An Order for `collection`
    /// requires the session to ALSO be authorized to move that collection on-chain via
    /// an explicit ERC-20-typed `transferFrom` allow that carries a CAP, and the order
    /// `amount` must be within BOTH that allow's per-call cap AND its remaining
    /// cumulative budget (the same ceilings the on-chain spend at
    /// `_checkAndConsumeTokenPolicy` enforces).
    ///
    /// LIMITATION (documented, by design): ERC-1271 is a `view`, so it cannot consume
    /// the cumulative `tokenUsed` counter. It therefore bounds each INDIVIDUAL order to
    /// the currently-remaining budget, NOT the SUM of many distinct attestations — a
    /// session can sign multiple orders each within budget. The consuming marketplace
    /// MUST track order fills on its side (standard practice). This is acceptable: a
    /// single attestation can never exceed the session's spend authority, and the
    /// stateful on-chain path remains the hard cap on actual asset movement.
    function _envelopeWithinCaps(Erc1271Envelope memory e) internal view returns (bool) {
        if (e.purpose == uint8(SignaturePurpose.Login)) {
            return true;
        }
        // Order
        bytes32 key = allowKey(e.collection, SEL_TRANSFER_FROM);
        Allow storage a = allows[e.sessionKey][sessionGrantGen[e.sessionKey]][key];
        // Must be authorized to move this collection on-chain, as a priced (ERC-20)
        // asset, under at least one explicit amount cap (an uncapped allow must NOT
        // yield an unbounded off-chain order).
        if (!a.allowed || a.standard != TokenStandard.ERC20 || (a.maxPerCall == 0 && a.maxTotal == 0)) {
            return false;
        }
        if (a.maxPerCall != 0 && e.amount > a.maxPerCall) {
            return false;
        }
        if (a.maxTotal != 0) {
            // `tokenUsed <= maxTotal` always holds (enforced before every write), so
            // `maxTotal - used` cannot underflow.
            uint256 used = tokenUsed[e.sessionKey][sessionGrantGen[e.sessionKey]][key];
            if (e.amount > a.maxTotal - used) {
                return false;
            }
        }
        return true;
    }

    // --------------------------------------------------------------- relayer fee

    /// Reimburse the relayer (design §16.3) up to the SIGNED `maxRelayerFee` — never
    /// more. Pays `min(measuredCost, maxRelayerFee)` to `tx.origin` (the EOA that
    /// submitted the tx, i.e. the relayer when routed via the EntryPoint, or the user
    /// on a direct call). `measuredCost = (gasUsed + FEE_OVERHEAD_GAS) * tx.gasprice`,
    /// where `gasUsed` is this op's account-side gas. The signed cap is the hard bound,
    /// so the approximation never overpays beyond what the op authorized; no relayer-
    /// supplied value is trusted.
    ///
    /// LIMITATION (documented): `tx.origin` is the originating EOA, so a relayer routed
    /// through its OWN contract (a bundler contract) is reimbursed at its operator EOA,
    /// not the bundler contract. Acceptable for the EOA-relayer MVP; a relayer-address
    /// parameter or ERC-4337 prefund accounting is the path to contract-relayer support.
    function _reimburseRelayer(uint256 gasStart, uint256 maxRelayerFee) internal {
        if (maxRelayerFee == 0) {
            return;
        }
        uint256 cost = (gasStart - gasleft() + FEE_OVERHEAD_GAS) * tx.gasprice;
        uint256 fee = cost < maxRelayerFee ? cost : maxRelayerFee;
        // Best-effort: cap at the remaining balance so an op that legitimately spends
        // the account down to < fee (its own signed value-forward) is NOT bricked at the
        // fee step. Never exceeds the signed cap; underpays only when funds ran out.
        uint256 bal = address(this).balance;
        if (fee > bal) {
            fee = bal;
        }
        if (fee == 0) {
            return;
        }
        // tx.origin is always an EOA (no code) → it can always receive value, so this
        // transfer cannot be made to revert by a malicious recipient, and an EOA cannot
        // reenter. Effects (nonce/counters) are already committed above; `fee <= balance`.
        (bool ok,) = tx.origin.call{value: fee}("");
        require(ok, "PQ: relayer reimbursement failed");
    }

    // ------------------------------------------------------------------- secp256k1

    /// Recover a secp256k1 signer from a 65-byte calldata `r ‖ s ‖ v` signature.
    /// Returns address(0) on a bad signature (so the grant lookup then fails
    /// "session inactive").
    function _recover(bytes32 hash, bytes calldata ecdsaSig) internal pure returns (address) {
        if (ecdsaSig.length != 65) {
            return address(0);
        }
        return _recoverRSV(hash, bytes32(ecdsaSig[0:32]), bytes32(ecdsaSig[32:64]), uint8(ecdsaSig[64]));
    }

    /// Core secp256k1 recovery, rejecting the malleable high-`s` half (EIP-2) and
    /// `v ∉ {27,28}`. Memory-friendly (used by the ERC-1271 envelope path, where the
    /// signature comes from an abi-decoded struct).
    function _recoverRSV(bytes32 hash, bytes32 r, bytes32 s, uint8 v) internal pure returns (address) {
        if (uint256(s) > SECP256K1N_HALF || (v != 27 && v != 28)) {
            return address(0);
        }
        return ecrecover(hash, v, r, s);
    }
}
