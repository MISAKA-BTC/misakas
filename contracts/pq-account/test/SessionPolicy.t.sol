// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Test} from "forge-std/Test.sol";
import {MisakaPqSmartAccount} from "../src/MisakaPqSmartAccount.sol";

/// ERC-721-ish + ERC-1155-ish recorder. `transferFrom(address,address,uint256)` shares
/// selector 0x23b872dd with ERC-20 — used to prove the standard discriminator decides
/// whether the 3rd word is a tokenId or an amount.
contract NftTarget {
    uint256 public last721Id;
    uint256 public last1155Id;
    uint256 public last1155Amount;

    function transferFrom(address, address, uint256 tokenId) external {
        last721Id = tokenId;
    }

    function safeTransferFrom(address, address, uint256 tokenId) external {
        last721Id = tokenId;
    }

    function safeTransferFrom(address, address, uint256 tokenId, bytes calldata) external {
        last721Id = tokenId;
    }

    function safeTransferFrom(address, address, uint256 id, uint256 amount, bytes calldata) external {
        last1155Id = id;
        last1155Amount = amount;
    }

    function safeBatchTransferFrom(address, address, uint256[] calldata, uint256[] calldata, bytes calldata) external {}
}

/// Minimal ERC-20-like target.
contract Erc20Target {
    mapping(address => uint256) public sent;

    function transfer(address to, uint256 amount) external returns (bool) {
        sent[to] += amount;
        return true;
    }

    function transferFrom(address, address to, uint256 amount) external returns (bool) {
        sent[to] += amount;
        return true;
    }
}

/// Tests for the P1 session policy surface: Merkle proof allowlist, ERC-721/1155 amount
/// policy, Permit2 deny-by-default, and ERC-1271 session-purpose recompute.
contract SessionPolicyTest is Test {
    MisakaPqSmartAccount internal account;
    NftTarget internal nft;
    Erc20Target internal erc20;

    uint256 internal constant SK = 0xA11CE;
    uint64 internal constant VERSION = 1;

    bytes4 internal constant SEL_TRANSFER = 0xa9059cbb;
    bytes4 internal constant SEL_TRANSFER_FROM = 0x23b872dd;
    bytes4 internal constant SEL_SAFE_721 = 0x42842e0e;
    bytes4 internal constant SEL_SAFE_721_DATA = 0xb88d4fde;
    bytes4 internal constant SEL_1155 = 0xf242432a;
    bytes4 internal constant SEL_1155_BATCH = 0x2eb2c2d6;
    bytes4 internal constant SEL_P2_APPROVE = 0x87517c45;
    bytes1 internal constant TAG = 0x53;
    address internal constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;

    bytes32 internal constant LEAF_DOMAIN = keccak256("MISAKA_PQ_SESSION_POLICY_LEAF_V1");
    bytes32 internal constant LOGIN_TYPEHASH =
        keccak256("MisakaLogin(address account,address sessionKey,uint64 grantId,bytes32 statement,uint256 deadline)");
    bytes32 internal constant ORDER_TYPEHASH = keccak256(
        "MisakaOrder(address account,address sessionKey,uint64 grantId,address collection,uint256 tokenId,uint256 amount,uint256 deadline)"
    );

    // TokenStandard enum values.
    uint8 internal constant TS_NATIVE = 0;
    uint8 internal constant TS_ERC20 = 1;
    uint8 internal constant TS_ERC721 = 2;
    uint8 internal constant TS_ERC1155 = 3;

    function setUp() public {
        account = new MisakaPqSmartAccount(
            bytes32(uint256(0x7777)), bytes32(uint256(0x8888)), bytes32(uint256(0x1111)), bytes32(uint256(0x2222)), VERSION
        );
        nft = new NftTarget();
        erc20 = new Erc20Target();
        vm.deal(address(account), 100 ether);
    }

    // --------------------------------------------------------------------- helpers

    function _sk() internal pure returns (address) {
        return vm.addr(SK);
    }

    function _sessionSig(address tgt, uint256 value, bytes memory callData, uint64 callIndex)
        internal
        view
        returns (bytes memory)
    {
        bytes32 domain = keccak256("MISAKA_PQ_EXECUTE_SESSION_V1");
        bytes32 opHash = keccak256(
            abi.encode(domain, block.chainid, address(account), VERSION, tgt, value, keccak256(callData), callIndex, uint256(0))
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(SK, opHash);
        return abi.encodePacked(r, s, v);
    }

    function _grantV2(MisakaPqSmartAccount.PolicyEntry[] memory entries, uint128 maxNative, uint64 maxCalls) internal {
        vm.prank(address(account));
        account.grantSessionV2(_sk(), type(uint64).max, maxCalls, maxNative, entries);
    }

    function _grantRoot(bytes32 root, uint128 maxNative, uint64 maxCalls) internal {
        vm.prank(address(account));
        account.grantSessionWithRoot(_sk(), type(uint64).max, maxCalls, maxNative, root);
    }

    function _entry(bytes32 key, uint8 std, uint256 maxPerCall, uint256 maxTotal, uint256 id)
        internal
        pure
        returns (MisakaPqSmartAccount.PolicyEntry memory)
    {
        return MisakaPqSmartAccount.PolicyEntry({
            targetSelectorKey: key,
            standard: std,
            maxPerCall: maxPerCall,
            maxTotal: maxTotal,
            erc1155TokenId: id
        });
    }

    function _leaf(address tgt, bytes4 sel, uint8 std, bytes32 codeHashPin, uint256 maxPerCall, uint256 maxTotal, uint256 extraCap)
        internal
        pure
        returns (MisakaPqSmartAccount.PolicyLeaf memory)
    {
        return MisakaPqSmartAccount.PolicyLeaf({
            target: tgt,
            selector: sel,
            tokenStandard: std,
            codeHashPin: codeHashPin,
            maxPerCall: maxPerCall,
            maxTotal: maxTotal,
            extraCap: extraCap
        });
    }

    function _leafHash(MisakaPqSmartAccount.PolicyLeaf memory l) internal pure returns (bytes32) {
        return keccak256(
            bytes.concat(
                keccak256(
                    abi.encode(
                        LEAF_DOMAIN, l.target, l.selector, l.tokenStandard, l.codeHashPin, l.maxPerCall, l.maxTotal, l.extraCap
                    )
                )
            )
        );
    }

    function _pair(bytes32 a, bytes32 b) internal pure returns (bytes32) {
        return a <= b ? keccak256(abi.encodePacked(a, b)) : keccak256(abi.encodePacked(b, a));
    }

    // ============================================================ proof path (Spec1)

    function test_proof_single_leaf_happy() public {
        MisakaPqSmartAccount.PolicyLeaf memory l = _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, bytes32(0), 0, 0, 0);
        _grantRoot(_leafHash(l), 5 ether, 5);
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(123));
        bytes32[] memory proof = new bytes32[](0);
        account.executeSessionWithProof(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), l, proof, 0);
        assertEq(nft.last721Id(), 123, "proof-authorized 721 transfer executed");
    }

    function test_proof_two_leaf_happy_and_bad_proof() public {
        MisakaPqSmartAccount.PolicyLeaf memory lNft = _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, bytes32(0), 0, 0, 0);
        MisakaPqSmartAccount.PolicyLeaf memory lErc = _leaf(address(erc20), SEL_TRANSFER, TS_ERC20, bytes32(0), 100, 0, 0);
        bytes32 hN = _leafHash(lNft);
        bytes32 hE = _leafHash(lErc);
        _grantRoot(_pair(hN, hE), 5 ether, 5);

        // valid proof for the NFT leaf = [sibling hE]
        bytes32[] memory proofN = new bytes32[](1);
        proofN[0] = hE;
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(7));
        account.executeSessionWithProof(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), lNft, proofN, 0);
        assertEq(nft.last721Id(), 7);

        // wrong sibling -> bad proof
        bytes32[] memory bad = new bytes32[](1);
        bad[0] = keccak256("nope");
        vm.expectRevert("PQ: bad merkle proof");
        account.executeSessionWithProof(address(nft), 0, cd, 1, _sessionSig(address(nft), 0, cd, 1), lNft, bad, 0);
    }

    function test_proof_leaf_call_mismatch_reverts() public {
        MisakaPqSmartAccount.PolicyLeaf memory l = _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, bytes32(0), 0, 0, 0);
        _grantRoot(_leafHash(l), 5 ether, 5);
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(1));
        bytes32[] memory proof = new bytes32[](0);
        // call a DIFFERENT target than the leaf names
        vm.expectRevert("PQ: leaf/call mismatch");
        account.executeSessionWithProof(address(erc20), 0, cd, 0, _sessionSig(address(erc20), 0, cd, 0), l, proof, 0);
    }

    function test_proof_no_policy_root_reverts() public {
        // legacy grant -> no policy root
        bytes32[] memory keys = new bytes32[](0);
        uint256[] memory amts = new uint256[](0);
        vm.prank(address(account));
        account.grantSession(_sk(), type(uint64).max, 5, 5 ether, keys, amts);
        MisakaPqSmartAccount.PolicyLeaf memory l = _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, bytes32(0), 0, 0, 0);
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(1));
        bytes32[] memory proof = new bytes32[](0);
        vm.expectRevert("PQ: no policy root");
        account.executeSessionWithProof(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), l, proof, 0);
    }

    function test_proof_codehash_pin_ok_and_mismatch() public {
        bytes32 pin = address(nft).codehash;
        MisakaPqSmartAccount.PolicyLeaf memory l = _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, pin, 0, 0, 0);
        _grantRoot(_leafHash(l), 5 ether, 5);
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(9));
        bytes32[] memory proof = new bytes32[](0);
        account.executeSessionWithProof(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), l, proof, 0);
        assertEq(nft.last721Id(), 9);

        // a leaf pinning the WRONG codehash for the same target -> mismatch
        MisakaPqSmartAccount.PolicyLeaf memory lBad =
            _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, keccak256("wrong"), 0, 0, 0);
        _grantRoot(_leafHash(lBad), 5 ether, 5); // re-grant new gen
        vm.expectRevert("PQ: code-hash mismatch");
        account.executeSessionWithProof(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), lBad, proof, 0);
    }

    function test_proof_forbidden_and_permit2_denied_before_proof() public {
        _grantRoot(keccak256("anyroot"), 5 ether, 5);
        bytes32[] memory proof = new bytes32[](0);
        MisakaPqSmartAccount.PolicyLeaf memory l = _leaf(address(nft), SEL_P2_APPROVE, TS_ERC20, bytes32(0), 0, 0, 0);
        // Permit2 selector is denied in the front guards, before any proof work.
        bytes memory cd = abi.encodeWithSelector(SEL_P2_APPROVE, address(0), address(0), uint160(0), uint48(0));
        vm.expectRevert("PQ: permit2 selector denied");
        account.executeSessionWithProof(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), l, proof, 0);
    }

    // =============================================== token-standard collision (Spec1+2)

    function test_proof_erc721_vs_erc20_collision_same_selector() public {
        // Two leaves, SAME selector 0x23b872dd, DIFFERENT standard.
        uint256 hugeId = 2 ** 200; // as an ERC-20 amount this dwarfs any cap
        MisakaPqSmartAccount.PolicyLeaf memory lNft =
            _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, bytes32(0), 0, 0, hugeId + 1); // pin tokenId=hugeId
        MisakaPqSmartAccount.PolicyLeaf memory lErc =
            _leaf(address(erc20), SEL_TRANSFER_FROM, TS_ERC20, bytes32(0), 100, 0, 0); // amount cap 100
        bytes32 hN = _leafHash(lNft);
        bytes32 hE = _leafHash(lErc);
        _grantRoot(_pair(hN, hE), 0, 5);

        bytes32[] memory proofN = new bytes32[](1);
        proofN[0] = hE;
        bytes32[] memory proofE = new bytes32[](1);
        proofE[0] = hN;

        // ERC-721: the 3rd word is a tokenId (hugeId) — pinned, NOT amount-capped -> ok.
        bytes memory cdN = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), hugeId);
        account.executeSessionWithProof(address(nft), 0, cdN, 0, _sessionSig(address(nft), 0, cdN, 0), lNft, proofN, 0);
        assertEq(nft.last721Id(), hugeId, "721 tokenId not treated as an amount");

        // ERC-20: the same 3rd-word position IS an amount -> exceeding the cap reverts.
        bytes memory cdE = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(200));
        vm.expectRevert("PQ: token amount cap");
        account.executeSessionWithProof(address(erc20), 0, cdE, 1, _sessionSig(address(erc20), 0, cdE, 1), lErc, proofE, 0);
    }

    function test_proof_erc721_tokenid_pin_rejects_wrong_id() public {
        MisakaPqSmartAccount.PolicyLeaf memory l =
            _leaf(address(nft), SEL_TRANSFER_FROM, TS_ERC721, bytes32(0), 0, 0, 42 + 1); // pin tokenId 42
        _grantRoot(_leafHash(l), 0, 5);
        bytes32[] memory proof = new bytes32[](0);
        bytes memory cdBad = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(43));
        vm.expectRevert("PQ: tokenId not allowed");
        account.executeSessionWithProof(address(nft), 0, cdBad, 0, _sessionSig(address(nft), 0, cdBad, 0), l, proof, 0);
        bytes memory cdOk = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(42));
        account.executeSessionWithProof(address(nft), 0, cdOk, 0, _sessionSig(address(nft), 0, cdOk, 0), l, proof, 0);
        assertEq(nft.last721Id(), 42);
    }

    function test_proof_safe721_with_data_decodes_tokenid() public {
        MisakaPqSmartAccount.PolicyLeaf memory l =
            _leaf(address(nft), SEL_SAFE_721_DATA, TS_ERC721, bytes32(0), 0, 0, 77 + 1);
        _grantRoot(_leafHash(l), 0, 5);
        bytes32[] memory proof = new bytes32[](0);
        bytes memory cd =
            abi.encodeWithSelector(SEL_SAFE_721_DATA, address(account), address(0xBEEF), uint256(77), bytes("xx"));
        account.executeSessionWithProof(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), l, proof, 0);
        assertEq(nft.last721Id(), 77, "tokenId decoded past the data offset word");
    }

    // ====================================================== explicit 721/1155 (Spec2)

    function test_explicit_erc721_count_cap() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_TRANSFER_FROM), TS_ERC721, 1, 2, 0); // count cap 2
        _grantV2(e, 0, 5);
        for (uint256 i; i < 2; i++) {
            bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(i));
            account.executeSession(address(nft), 0, cd, uint64(i), _sessionSig(address(nft), 0, cd, uint64(i)), 0);
        }
        bytes memory cd3 = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(99));
        vm.expectRevert("PQ: nft transfer count cap");
        account.executeSession(address(nft), 0, cd3, 2, _sessionSig(address(nft), 0, cd3, 2), 0);
    }

    function test_explicit_erc1155_amount_and_id() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        // pin id=5, per-call<=10, total<=15
        e[0] = _entry(account.allowKey(address(nft), SEL_1155), TS_ERC1155, 10, 15, 5);
        _grantV2(e, 0, 5);

        // wrong id -> mismatch
        bytes memory cdWrongId = abi.encodeWithSelector(SEL_1155, address(account), address(0xBEEF), uint256(6), uint256(1), bytes(""));
        vm.expectRevert("PQ: 1155 id mismatch");
        account.executeSession(address(nft), 0, cdWrongId, 0, _sessionSig(address(nft), 0, cdWrongId, 0), 0);

        // over per-call -> cap
        bytes memory cdOver = abi.encodeWithSelector(SEL_1155, address(account), address(0xBEEF), uint256(5), uint256(11), bytes(""));
        vm.expectRevert("PQ: token amount cap");
        account.executeSession(address(nft), 0, cdOver, 0, _sessionSig(address(nft), 0, cdOver, 0), 0);

        // ok 10, then 6 would exceed cumulative 15 -> total cap
        bytes memory cdOk = abi.encodeWithSelector(SEL_1155, address(account), address(0xBEEF), uint256(5), uint256(10), bytes(""));
        account.executeSession(address(nft), 0, cdOk, 0, _sessionSig(address(nft), 0, cdOk, 0), 0);
        assertEq(nft.last1155Amount(), 10);
        bytes memory cd6 = abi.encodeWithSelector(SEL_1155, address(account), address(0xBEEF), uint256(5), uint256(6), bytes(""));
        vm.expectRevert("PQ: token total cap");
        account.executeSession(address(nft), 0, cd6, 1, _sessionSig(address(nft), 0, cd6, 1), 0);
    }

    function test_explicit_erc1155_batch_rejected() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_1155_BATCH), TS_ERC1155, 10, 0, 5);
        _grantV2(e, 0, 5);
        uint256[] memory ids = new uint256[](1);
        uint256[] memory amts = new uint256[](1);
        ids[0] = 5;
        amts[0] = 1;
        bytes memory cd = abi.encodeWithSelector(SEL_1155_BATCH, address(account), address(0xBEEF), ids, amts, bytes(""));
        vm.expectRevert("PQ: 1155 batch not supported");
        account.executeSession(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), 0);
    }

    function test_explicit_erc20_cumulative_total_cap() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(erc20), SEL_TRANSFER), TS_ERC20, 0, 100, 0); // total cap 100
        _grantV2(e, 0, 5);
        bytes memory cd60 = abi.encodeWithSelector(SEL_TRANSFER, address(0xBEEF), uint256(60));
        account.executeSession(address(erc20), 0, cd60, 0, _sessionSig(address(erc20), 0, cd60, 0), 0);
        bytes memory cd60b = abi.encodeWithSelector(SEL_TRANSFER, address(0xBEEF), uint256(60));
        vm.expectRevert("PQ: token total cap");
        account.executeSession(address(erc20), 0, cd60b, 1, _sessionSig(address(erc20), 0, cd60b, 1), 0);
    }

    function test_grantV2_erc721_per_call_gt1_reverts() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_TRANSFER_FROM), TS_ERC721, 2, 0, 0);
        vm.prank(address(account));
        vm.expectRevert("PQ: erc721 per-call must be <=1");
        account.grantSessionV2(_sk(), type(uint64).max, 5, 0, e);
    }

    function test_grantV2_only_root() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](0);
        vm.expectRevert("PQ: only root (via executeRoot)");
        account.grantSessionV2(_sk(), type(uint64).max, 5, 0, e);
    }

    // ============================================================= Permit2 (Spec3)

    function test_permit2_target_denied() public {
        // Even arbitrary calldata to the canonical Permit2 address is denied.
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(PERMIT2, bytes4(0xdeadbeef)), TS_NATIVE, 0, 0, 0);
        _grantV2(e, 1 ether, 5);
        bytes memory cd = abi.encodeWithSelector(bytes4(0xdeadbeef));
        vm.expectRevert("PQ: permit2 target denied");
        account.executeSession(PERMIT2, 0, cd, 0, _sessionSig(PERMIT2, 0, cd, 0), 0);
    }

    function test_permit2_selector_denied_on_clone() public {
        // A Permit2 fork at a different address is still denied by the selector list.
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(erc20), SEL_P2_APPROVE), TS_NATIVE, 0, 0, 0);
        _grantV2(e, 0, 5);
        bytes memory cd = abi.encodeWithSelector(SEL_P2_APPROVE, address(0), address(0), uint160(0), uint48(0));
        vm.expectRevert("PQ: permit2 selector denied");
        account.executeSession(address(erc20), 0, cd, 0, _sessionSig(address(erc20), 0, cd, 0), 0);
    }

    function test_permit2_erc20_approve_still_forbidden() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(erc20), 0x095ea7b3), TS_NATIVE, 0, 0, 0);
        _grantV2(e, 0, 5);
        bytes memory cd = abi.encodeWithSelector(bytes4(0x095ea7b3), PERMIT2, type(uint256).max);
        vm.expectRevert("PQ: forbidden selector");
        account.executeSession(address(erc20), 0, cd, 0, _sessionSig(address(erc20), 0, cd, 0), 0);
    }

    // ========================================================== ERC-1271 session (Spec4)

    uint32 internal constant MASK_LOGIN = uint32(1) << 0;
    uint32 internal constant MASK_ORDER = uint32(1) << 3;

    function _grantPurposes(uint32 mask) internal {
        vm.prank(address(account));
        account.grantSessionPurposes(_sk(), mask);
    }

    function _grantSimple(uint64 maxCalls) internal {
        bytes32[] memory keys = new bytes32[](0);
        uint256[] memory amts = new uint256[](0);
        vm.prank(address(account));
        account.grantSession(_sk(), type(uint64).max, maxCalls, 1 ether, keys, amts);
    }

    function _loginDigest(uint64 grantId, bytes32 statement, uint256 deadline, bytes32 domainSep)
        internal
        view
        returns (bytes32)
    {
        bytes32 structHash =
            keccak256(abi.encode(LOGIN_TYPEHASH, address(account), _sk(), grantId, statement, deadline));
        return keccak256(abi.encodePacked(hex"1901", domainSep, structHash));
    }

    function _loginEnvelope(uint64 grantId, bytes32 statement, uint256 deadline, bytes32 domainSep, bytes32 signOver)
        internal
        view
        returns (bytes memory)
    {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(SK, signOver);
        MisakaPqSmartAccount.Erc1271Envelope memory e = MisakaPqSmartAccount.Erc1271Envelope({
            purpose: 0, // Login
            sessionKey: _sk(),
            domainSeparator: domainSep,
            collection: address(0),
            tokenId: 0,
            amount: 0,
            deadline: deadline,
            grantId: grantId,
            statement: statement,
            sigR: r,
            sigS: s,
            sigV: v
        });
        return abi.encodePacked(TAG, abi.encode(e));
    }

    function test_erc1271_session_login_happy() public {
        _grantSimple(5);
        _grantPurposes(MASK_LOGIN);
        bytes32 domainSep = keccak256("dapp-domain");
        bytes32 statement = keccak256("Sign in to MISAKA");
        bytes32 digest = _loginDigest(1, statement, 0, domainSep);
        bytes memory env = _loginEnvelope(1, statement, 0, domainSep, digest);
        assertEq(account.isValidSignature(digest, env), bytes4(0x1626ba7e), "session login attestation valid");
    }

    function test_erc1271_purpose_not_opted_in_rejects() public {
        _grantSimple(5); // no grantSessionPurposes call -> mask 0
        bytes32 domainSep = keccak256("dapp-domain");
        bytes32 statement = keccak256("x");
        bytes32 digest = _loginDigest(1, statement, 0, domainSep);
        bytes memory env = _loginEnvelope(1, statement, 0, domainSep, digest);
        assertEq(account.isValidSignature(digest, env), bytes4(0xffffffff), "purpose not opted in -> invalid");
    }

    function test_erc1271_recompute_mismatch_rejects() public {
        _grantSimple(5);
        _grantPurposes(MASK_LOGIN);
        bytes32 domainSep = keccak256("dapp-domain");
        bytes32 statement = keccak256("x");
        bytes32 digest = _loginDigest(1, statement, 0, domainSep);
        // The session signs the real digest, but the verifier passes a DIFFERENT hash
        // (the "pass off an arbitrary digest under Login" attack) -> recompute mismatch.
        bytes memory env = _loginEnvelope(1, statement, 0, domainSep, digest);
        assertEq(account.isValidSignature(keccak256("attacker-digest"), env), bytes4(0xffffffff), "recompute mismatch");
    }

    function test_erc1271_wrong_signer_rejects() public {
        _grantSimple(5);
        _grantPurposes(MASK_LOGIN);
        bytes32 domainSep = keccak256("dapp-domain");
        bytes32 statement = keccak256("x");
        bytes32 digest = _loginDigest(1, statement, 0, domainSep);
        // sign with a DIFFERENT key but claim _sk() as sessionKey
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(0xBADBAD, digest);
        MisakaPqSmartAccount.Erc1271Envelope memory e = MisakaPqSmartAccount.Erc1271Envelope({
            purpose: 0,
            sessionKey: _sk(),
            domainSeparator: domainSep,
            collection: address(0),
            tokenId: 0,
            amount: 0,
            deadline: 0,
            grantId: 1,
            statement: statement,
            sigR: r,
            sigS: s,
            sigV: v
        });
        bytes memory env = abi.encodePacked(TAG, abi.encode(e));
        assertEq(account.isValidSignature(digest, env), bytes4(0xffffffff), "signer != claimed sessionKey");
    }

    function test_erc1271_stale_grantId_rejects() public {
        _grantSimple(5); // gen 1
        _grantPurposes(MASK_LOGIN);
        bytes32 domainSep = keccak256("dapp-domain");
        bytes32 statement = keccak256("x");
        // claim grantId 0 (stale) — must mismatch current gen 1
        bytes32 digest = _loginDigest(0, statement, 0, domainSep);
        bytes memory env = _loginEnvelope(0, statement, 0, domainSep, digest);
        assertEq(account.isValidSignature(digest, env), bytes4(0xffffffff), "stale grantId rejected");
    }

    function test_erc1271_custom_and_unknown_rejected() public {
        _grantSimple(5);
        _grantPurposes(MASK_LOGIN | MASK_ORDER);
        bytes32 domainSep = keccak256("d");
        // Build an envelope with purpose=4 (Custom) and purpose=1 (NftListing, unknown schema)
        for (uint8 p = 1; p <= 4; p++) {
            if (p == 3) continue; // 3=Order is known; tested elsewhere
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(SK, keccak256("anything"));
            MisakaPqSmartAccount.Erc1271Envelope memory e = MisakaPqSmartAccount.Erc1271Envelope({
                purpose: p,
                sessionKey: _sk(),
                domainSeparator: domainSep,
                collection: address(0),
                tokenId: 0,
                amount: 0,
                deadline: 0,
                grantId: 1,
                statement: bytes32(0),
                sigR: r,
                sigS: s,
                sigV: v
            });
            bytes memory env = abi.encodePacked(TAG, abi.encode(e));
            assertEq(account.isValidSignature(keccak256("anything"), env), bytes4(0xffffffff), "custom/unknown purpose rejected");
        }
    }

    function test_erc1271_raw_short_sig_rejected() public {
        _grantSimple(5);
        _grantPurposes(MASK_LOGIN);
        // a bare 65-byte secp sig (no envelope tag) is not 1271-valid.
        assertEq(account.isValidSignature(keccak256("x"), new bytes(65)), bytes4(0xffffffff));
        assertEq(account.isValidSignature(keccak256("x"), hex"1234"), bytes4(0xffffffff));
    }

    function test_erc1271_order_happy_and_over_cap() public {
        // Order requires an explicit transferFrom allow for the collection + honours its cap.
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_TRANSFER_FROM), TS_ERC20, 1000, 0, 0); // amount ceiling 1000
        _grantV2(e, 0, 5);
        _grantPurposes(MASK_ORDER);

        bytes32 domainSep = keccak256("market");
        uint256 amount = 500;
        bytes32 structHash = keccak256(
            abi.encode(ORDER_TYPEHASH, address(account), _sk(), uint64(1), address(nft), uint256(7), amount, uint256(0))
        );
        bytes32 digest = keccak256(abi.encodePacked(hex"1901", domainSep, structHash));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(SK, digest);
        MisakaPqSmartAccount.Erc1271Envelope memory env = MisakaPqSmartAccount.Erc1271Envelope({
            purpose: 3, // Order
            sessionKey: _sk(),
            domainSeparator: domainSep,
            collection: address(nft),
            tokenId: 7,
            amount: amount,
            deadline: 0,
            grantId: 1,
            statement: bytes32(0),
            sigR: r,
            sigS: s,
            sigV: v
        });
        bytes memory sig = abi.encodePacked(TAG, abi.encode(env));
        assertEq(account.isValidSignature(digest, sig), bytes4(0x1626ba7e), "in-cap order valid");

        // amount over the 1000 ceiling -> invalid (recompute also must match the new amount)
        uint256 big = 2000;
        bytes32 structHash2 = keccak256(
            abi.encode(ORDER_TYPEHASH, address(account), _sk(), uint64(1), address(nft), uint256(7), big, uint256(0))
        );
        bytes32 digest2 = keccak256(abi.encodePacked(hex"1901", domainSep, structHash2));
        (uint8 v2, bytes32 r2, bytes32 s2) = vm.sign(SK, digest2);
        env.amount = big;
        env.sigR = r2;
        env.sigS = s2;
        env.sigV = v2;
        bytes memory sig2 = abi.encodePacked(TAG, abi.encode(env));
        assertEq(account.isValidSignature(digest2, sig2), bytes4(0xffffffff), "over-cap order rejected");
    }

    function test_grantSessionPurposes_rejects_custom_bit() public {
        _grantSimple(5);
        vm.prank(address(account));
        vm.expectRevert("PQ: purpose not allowed");
        account.grantSessionPurposes(_sk(), uint32(1) << 4);
    }

    function test_grantSessionPurposes_rejects_reserved_bits() public {
        // NftListing(1)/Permit(2) are reserved (no recomputable schema) -> deny-by-default.
        _grantSimple(5);
        uint32 permitBit = uint32(1) << 2;
        vm.prank(address(account));
        vm.expectRevert("PQ: purpose not allowed");
        account.grantSessionPurposes(_sk(), MASK_LOGIN | permitBit);
    }

    function test_grantSessionPurposes_only_root() public {
        _grantSimple(5);
        vm.expectRevert("PQ: only root (via executeRoot)");
        account.grantSessionPurposes(_sk(), MASK_LOGIN);
    }

    function _orderEnvelope(
        bytes32 domainSep,
        address collection,
        uint256 tokenId,
        uint256 amount,
        uint64 grantId,
        bytes32 signOver
    ) internal view returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(SK, signOver);
        MisakaPqSmartAccount.Erc1271Envelope memory e = MisakaPqSmartAccount.Erc1271Envelope({
            purpose: 3, // Order
            sessionKey: _sk(),
            domainSeparator: domainSep,
            collection: collection,
            tokenId: tokenId,
            amount: amount,
            deadline: 0,
            grantId: grantId,
            statement: bytes32(0),
            sigR: r,
            sigS: s,
            sigV: v
        });
        return abi.encodePacked(TAG, abi.encode(e));
    }

    function _orderDigest(bytes32 domainSep, address collection, uint256 tokenId, uint256 amount, uint64 grantId)
        internal
        view
        returns (bytes32)
    {
        bytes32 structHash = keccak256(
            abi.encode(ORDER_TYPEHASH, address(account), _sk(), grantId, collection, tokenId, amount, uint256(0))
        );
        return keccak256(abi.encodePacked(hex"1901", domainSep, structHash));
    }

    /// F1 regression: an ERC-20 transferFrom allow with ONLY a cumulative cap
    /// (maxPerCall=0, maxTotal=100) must NOT yield a valid Order attestation for an
    /// amount far exceeding 100 — the prior code short-circuited to valid when
    /// maxPerCall==0 and never consulted maxTotal.
    function test_erc1271_order_cumulative_only_over_budget_rejected() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_TRANSFER_FROM), TS_ERC20, 0, 100, 0); // cumulative-only
        _grantV2(e, 0, 5);
        _grantPurposes(MASK_ORDER);
        bytes32 domainSep = keccak256("market");

        bytes32 dBig = _orderDigest(domainSep, address(nft), 7, 1e30, 1);
        assertEq(
            account.isValidSignature(dBig, _orderEnvelope(domainSep, address(nft), 7, 1e30, 1, dBig)),
            bytes4(0xffffffff),
            "over cumulative budget rejected"
        );

        bytes32 dOk = _orderDigest(domainSep, address(nft), 7, 80, 1);
        assertEq(
            account.isValidSignature(dOk, _orderEnvelope(domainSep, address(nft), 7, 80, 1, dOk)),
            bytes4(0x1626ba7e),
            "within cumulative budget valid"
        );
    }

    /// An Order against a non-ERC20-typed (ERC-721 count-cap) transferFrom allow is
    /// rejected — a count must never be (mis)read as a price ceiling.
    function test_erc1271_order_non_erc20_allow_rejected() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_TRANSFER_FROM), TS_ERC721, 1, 5, 0); // ERC-721 count cap
        _grantV2(e, 0, 5);
        _grantPurposes(MASK_ORDER);
        bytes32 domainSep = keccak256("market");
        bytes32 d = _orderDigest(domainSep, address(nft), 7, 1, 1);
        assertEq(
            account.isValidSignature(d, _orderEnvelope(domainSep, address(nft), 7, 1, 1, d)),
            bytes4(0xffffffff),
            "order against non-ERC20 allow rejected"
        );
    }

    /// An Order with NO explicit transferFrom allow for the collection is rejected.
    function test_erc1271_order_no_allow_rejected() public {
        _grantSimple(5); // legacy grant, no explicit collection allow
        _grantPurposes(MASK_ORDER);
        bytes32 domainSep = keccak256("market");
        bytes32 d = _orderDigest(domainSep, address(nft), 7, 1, 1);
        assertEq(
            account.isValidSignature(d, _orderEnvelope(domainSep, address(nft), 7, 1, 1, d)),
            bytes4(0xffffffff),
            "order with no collection allow rejected"
        );
    }

    function test_session_increase_allowance_forbidden() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(erc20), 0x39509351), TS_NATIVE, 0, 0, 0);
        _grantV2(e, 0, 5);
        bytes memory cd = abi.encodeWithSelector(bytes4(0x39509351), PERMIT2, type(uint256).max);
        vm.expectRevert("PQ: forbidden selector");
        account.executeSession(address(erc20), 0, cd, 0, _sessionSig(address(erc20), 0, cd, 0), 0);
    }

    /// Cross-generation replay guard: the monotonic per-key sessionNonce survives a
    /// same-key re-grant, so a stale signature for an already-consumed call index can
    /// NOT be replayed under the new generation.
    function test_session_regrant_blocks_stale_nonce_replay() public {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_TRANSFER_FROM), TS_ERC721, 1, 0, 0);
        _grantV2(e, 0, 5); // gen 1

        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(1));
        account.executeSession(address(nft), 0, cd, 0, _sessionSig(address(nft), 0, cd, 0), 0); // consume index 0
        assertEq(account.sessionNonce(_sk()), 1, "nonce advanced");
        bytes memory staleSig = _sessionSig(address(nft), 0, cd, 0); // a stale index-0 signature

        _grantV2(e, 0, 5); // gen 2 — callsUsed resets, sessionNonce persists
        assertEq(account.sessionNonce(_sk()), 1, "nonce survives re-grant");

        // Replaying the stale index-0 signature under gen 2 fails (nonce is now 1).
        vm.expectRevert("PQ: bad session call index");
        account.executeSession(address(nft), 0, cd, 0, staleSig, 0);

        // The legitimate next op uses index 1 and succeeds.
        bytes memory cd1 = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(2));
        account.executeSession(address(nft), 0, cd1, 1, _sessionSig(address(nft), 0, cd1, 1), 0);
        assertEq(nft.last721Id(), 2, "next op executed at index 1");
    }

    // ===================================================== relayer fee (§16.3)

    address internal constant RELAYER = address(0xCAFE);

    function _sessionSigFee(address tgt, uint256 value, bytes memory callData, uint64 callIndex, uint256 maxRelayerFee)
        internal
        view
        returns (bytes memory)
    {
        bytes32 domain = keccak256("MISAKA_PQ_EXECUTE_SESSION_V1");
        bytes32 opHash = keccak256(
            abi.encode(domain, block.chainid, address(account), VERSION, tgt, value, keccak256(callData), callIndex, maxRelayerFee)
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(SK, opHash);
        return abi.encodePacked(r, s, v);
    }

    function _grantPingV2() internal {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entry(account.allowKey(address(nft), SEL_TRANSFER_FROM), TS_ERC721, 1, 0, 0);
        _grantV2(e, 0, 5);
    }

    /// The relayer (tx.origin) is reimbursed, and the payment is HARD-capped at the
    /// signed maxRelayerFee when the true gas cost exceeds it.
    function test_session_fee_capped_at_signed_max() public {
        _grantPingV2();
        uint256 maxFee = 1000; // wei
        vm.txGasPrice(1e12); // make the true cost vastly exceed the cap
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(1));
        bytes memory sig = _sessionSigFee(address(nft), 0, cd, 0, maxFee);
        uint256 before = RELAYER.balance;
        vm.prank(RELAYER, RELAYER); // msg.sender == tx.origin == relayer
        account.executeSession(address(nft), 0, cd, 0, sig, maxFee);
        assertEq(RELAYER.balance - before, maxFee, "fee hard-capped at maxRelayerFee");
    }

    /// When the true cost is below the cap, the relayer is paid the (smaller) actual
    /// cost, never the full cap.
    function test_session_fee_pays_actual_below_cap() public {
        _grantPingV2();
        uint256 maxFee = 1 ether; // huge cap
        vm.txGasPrice(1); // gasprice 1 wei → true cost is tiny
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(1));
        bytes memory sig = _sessionSigFee(address(nft), 0, cd, 0, maxFee);
        uint256 before = RELAYER.balance;
        vm.prank(RELAYER, RELAYER);
        account.executeSession(address(nft), 0, cd, 0, sig, maxFee);
        uint256 paid = RELAYER.balance - before;
        assertGt(paid, 0, "relayer reimbursed");
        assertLt(paid, maxFee, "paid the actual cost, below the cap");
    }

    /// maxRelayerFee == 0 (direct/self submission) pays nothing.
    function test_session_fee_zero_no_payment() public {
        _grantPingV2();
        vm.txGasPrice(1e12);
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(1));
        bytes memory sig = _sessionSig(address(nft), 0, cd, 0); // signs maxRelayerFee=0
        uint256 before = RELAYER.balance;
        vm.prank(RELAYER, RELAYER);
        account.executeSession(address(nft), 0, cd, 0, sig, 0);
        assertEq(RELAYER.balance, before, "no fee paid when maxRelayerFee == 0");
    }

    /// A relayer that tampers the maxRelayerFee (submits a value different from the
    /// signed one) is rejected: the recomputed op hash no longer matches the signature,
    /// so a different (unauthorized) key is recovered → no active session.
    function test_session_fee_tamper_rejected() public {
        _grantPingV2();
        bytes memory cd = abi.encodeWithSelector(SEL_TRANSFER_FROM, address(account), address(0xBEEF), uint256(1));
        bytes memory sig = _sessionSigFee(address(nft), 0, cd, 0, 1000); // signed for fee=1000
        vm.prank(RELAYER, RELAYER);
        vm.expectRevert("PQ: session inactive"); // recovered key for fee=5000 != granted key
        account.executeSession(address(nft), 0, cd, 0, sig, 5000);
    }
}
