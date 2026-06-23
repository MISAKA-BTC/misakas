// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Test} from "forge-std/Test.sol";
import {MisakaPqSmartAccount} from "../src/MisakaPqSmartAccount.sol";

/// Mock F003 returning ABI `true` (32 bytes, last byte 0x01). Stateless so it is
/// safe under STATICCALL and works when `vm.etch`'d to 0x…F003.
contract MockF003True {
    fallback(bytes calldata) external returns (bytes memory) {
        return abi.encode(true);
    }
}

/// Mock F003 returning ABI `false`.
contract MockF003False {
    fallback(bytes calldata) external returns (bytes memory) {
        return abi.encode(false);
    }
}

/// A target the account calls; records the last call + can force a revert.
contract CallTarget {
    uint256 public lastValue;
    bool public shouldRevert;

    function ping(uint256 x) external payable returns (uint256) {
        lastValue = msg.value;
        require(!shouldRevert, "target revert");
        return x + 1;
    }

    function setRevert(bool r) external {
        shouldRevert = r;
    }

    receive() external payable {}
}

/// Logic tests for `executeRoot` with F003 MOCKED (real ML-DSA verify is exercised
/// by the Rust end-to-end test against an F003-activated harness — Foundry cannot
/// run the lattice precompile). Covers: happy path + value forward + nonce bump,
/// replay, wrong nonce, validity window, ML-DSA-false, inert-F003, target revert.
contract MisakaPqSmartAccountTest is Test {
    address internal constant F003 = address(0x0000000000000000000000000000000000F003);

    MisakaPqSmartAccount internal account;
    CallTarget internal target;

    bytes32 internal constant VAULT_HI = bytes32(uint256(0x7777));
    bytes32 internal constant VAULT_LO = bytes32(uint256(0x8888));
    bytes32 internal constant RP_HI = bytes32(uint256(0x1111)); // operational root
    bytes32 internal constant RP_LO = bytes32(uint256(0x2222));
    uint64 internal constant VERSION = 1;

    // The mock F003 ignores these; the REAL key/sig binding is the Rust e2e's job.
    bytes internal pubkey = new bytes(2592);
    bytes internal sig = new bytes(4627);

    function setUp() public {
        account = new MisakaPqSmartAccount(VAULT_HI, VAULT_LO, RP_HI, RP_LO, VERSION);
        target = new CallTarget();
        vm.deal(address(account), 100 ether);
    }

    function _etchTrue() internal {
        vm.etch(F003, address(new MockF003True()).code);
    }

    function _etchFalse() internal {
        vm.etch(F003, address(new MockF003False()).code);
    }

    function _exec(uint64 nonce) internal returns (bytes memory) {
        return account.executeRoot(
            address(target),
            1 ether,
            abi.encodeWithSelector(CallTarget.ping.selector, uint256(41)),
            0,
            type(uint64).max,
            nonce,
            pubkey,
            sig,
            0
        );
    }

    function test_executeRoot_happy_path() public {
        _etchTrue();
        assertEq(account.rootNonce(), 0);
        bytes memory ret = _exec(0);
        assertEq(abi.decode(ret, (uint256)), 42, "target returned x+1");
        assertEq(account.rootNonce(), 1, "nonce incremented");
        assertEq(target.lastValue(), 1 ether, "value forwarded");
    }

    function test_replay_same_nonce_reverts() public {
        _etchTrue();
        _exec(0);
        vm.expectRevert("PQ: bad nonce");
        _exec(0); // nonce is now 1; replaying 0 must fail
    }

    function test_wrong_nonce_reverts() public {
        _etchTrue();
        vm.expectRevert("PQ: bad nonce");
        _exec(5);
    }

    function test_outside_window_reverts() public {
        _etchTrue();
        vm.roll(1000);
        vm.expectRevert("PQ: outside validity window");
        account.executeRoot(address(target), 0, "", 0, 10, 0, pubkey, sig, 0);
    }

    function test_ml_dsa_false_reverts() public {
        _etchFalse();
        vm.expectRevert("PQ: ml-dsa root auth failed");
        _exec(0);
    }

    function test_inert_f003_reverts() public {
        // No code at F003 (inert): staticcall returns empty ⇒ the auth require fails.
        vm.expectRevert("PQ: ml-dsa root auth failed");
        _exec(0);
    }

    function test_target_revert_bubbles() public {
        _etchTrue();
        target.setRevert(true);
        vm.expectRevert("PQ: target call reverted");
        _exec(0);
    }

    // ----------------------------------------------------------- session path tests

    uint256 internal constant SK = 0xA11CE; // session private key (test only)
    bytes4 internal constant SEL_PING = CallTarget.ping.selector;
    bytes4 internal constant SEL_TRANSFER = 0xa9059cbb; // transfer(address,uint256)
    bytes4 internal constant SEL_APPROVE = 0x095ea7b3;

    function _grant(address sk, bytes32[] memory keys, uint256[] memory amounts, uint128 maxNative, uint64 maxCalls)
        internal
    {
        vm.prank(address(account)); // simulate executeRoot's authorized self-call
        account.grantSession(sk, type(uint64).max, maxCalls, maxNative, keys, amounts);
    }

    function _sessionSig(uint256 sk, address tgt, uint256 value, bytes memory callData, uint64 callIndex)
        internal
        view
        returns (bytes memory)
    {
        bytes32 domain = keccak256("MISAKA_PQ_EXECUTE_SESSION_V1");
        bytes32 opHash =
            keccak256(abi.encode(domain, block.chainid, address(account), VERSION, tgt, value, keccak256(callData), callIndex, uint256(0)));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(sk, opHash);
        return abi.encodePacked(r, s, v);
    }

    function _grantPing(uint128 maxNative, uint64 maxCalls) internal returns (address) {
        address sk = vm.addr(SK);
        bytes32[] memory keys = new bytes32[](1);
        keys[0] = account.allowKey(address(target), SEL_PING);
        uint256[] memory amts = new uint256[](1);
        amts[0] = 0;
        _grant(sk, keys, amts, maxNative, maxCalls);
        return sk;
    }

    function test_session_happy_path() public {
        address sk = _grantPing(5 ether, 3);
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(7));
        bytes memory s = _sessionSig(SK, address(target), 1 ether, cd, 0);
        bytes memory ret = account.executeSession(address(target), 1 ether, cd, 0, s, 0);
        assertEq(abi.decode(ret, (uint256)), 8);
        assertEq(target.lastValue(), 1 ether, "native value forwarded");
        (,,, uint64 used,,,) = account.sessions(sk);
        assertEq(used, 1, "callsUsed incremented");
    }

    function test_session_forbidden_selector_reverts() public {
        _grantPing(5 ether, 3);
        bytes memory cd = abi.encodeWithSelector(SEL_APPROVE, address(0xBEEF), uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 0);
        vm.expectRevert("PQ: forbidden selector");
        account.executeSession(address(target), 0, cd, 0, s, 0);
    }

    function test_session_unlisted_target_reverts() public {
        address sk = vm.addr(SK);
        bytes32[] memory keys = new bytes32[](0);
        uint256[] memory amts = new uint256[](0);
        _grant(sk, keys, amts, 5 ether, 3); // grant exists but no (target,selector) allowed
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 0);
        vm.expectRevert("PQ: target/selector not allowed");
        account.executeSession(address(target), 0, cd, 0, s, 0);
    }

    function test_session_native_cap_reverts() public {
        _grantPing(1 ether, 3); // maxNativeTotal = 1 ether
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 2 ether, cd, 0);
        vm.expectRevert("PQ: session native cap");
        account.executeSession(address(target), 2 ether, cd, 0, s, 0);
    }

    function test_session_call_cap_reverts() public {
        _grantPing(10 ether, 1); // maxCalls = 1
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        account.executeSession(address(target), 0, cd, 0, _sessionSig(SK, address(target), 0, cd, 0), 0);
        vm.expectRevert("PQ: session call cap");
        account.executeSession(address(target), 0, cd, 1, _sessionSig(SK, address(target), 0, cd, 1), 0);
    }

    function test_session_bad_call_index_reverts() public {
        _grantPing(5 ether, 3);
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 5);
        vm.expectRevert("PQ: bad session call index");
        account.executeSession(address(target), 0, cd, 5, s, 0);
    }

    function test_session_expired_reverts() public {
        address sk = vm.addr(SK);
        bytes32[] memory keys = new bytes32[](1);
        keys[0] = account.allowKey(address(target), SEL_PING);
        uint256[] memory amts = new uint256[](1);
        vm.prank(address(account));
        account.grantSession(sk, 100, 3, 5 ether, keys, amts); // validUntil = block 100
        vm.roll(101);
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 0);
        vm.expectRevert("PQ: session expired");
        account.executeSession(address(target), 0, cd, 0, s, 0);
    }

    function test_session_erc20_amount_cap() public {
        MockToken token = new MockToken();
        address sk = vm.addr(SK);
        bytes32[] memory keys = new bytes32[](1);
        keys[0] = account.allowKey(address(token), SEL_TRANSFER);
        uint256[] memory amts = new uint256[](1);
        amts[0] = 100; // ERC-20 amount cap
        _grant(sk, keys, amts, 0, 5);

        bytes memory cdBad = abi.encodeWithSelector(SEL_TRANSFER, address(0xBEEF), uint256(200));
        vm.expectRevert("PQ: token amount cap");
        account.executeSession(address(token), 0, cdBad, 0, _sessionSig(SK, address(token), 0, cdBad, 0), 0);

        bytes memory cdOk = abi.encodeWithSelector(SEL_TRANSFER, address(0xBEEF), uint256(50));
        account.executeSession(address(token), 0, cdOk, 0, _sessionSig(SK, address(token), 0, cdOk, 0), 0);
        assertEq(token.sent(address(0xBEEF)), 50, "capped transfer executed");
    }

    function test_session_revoke() public {
        address sk = _grantPing(5 ether, 3);
        vm.prank(address(account));
        account.revokeSession(sk);
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 0);
        vm.expectRevert("PQ: session inactive");
        account.executeSession(address(target), 0, cd, 0, s, 0);
    }

    function test_session_ungranted_key_reverts() public {
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(0xBADBAD, address(target), 0, cd, 0); // key with no grant
        vm.expectRevert("PQ: session inactive");
        account.executeSession(address(target), 0, cd, 0, s, 0);
    }

    function test_session_cannot_target_self() public {
        // Even a granted session allowlisted for (address(this), grantSession) must
        // not be able to self-call grantSession (privilege escalation). The self-target
        // guard fires before the allowlist, so any self-target reverts.
        address sk = vm.addr(SK);
        bytes32[] memory keys = new bytes32[](1);
        keys[0] = account.allowKey(address(account), MisakaPqSmartAccount.grantSession.selector);
        uint256[] memory amts = new uint256[](1);
        _grant(sk, keys, amts, 5 ether, 3);
        bytes memory cd = abi.encodeWithSelector(bytes4(0x12345678)); // any 4-byte calldata
        bytes memory s = _sessionSig(SK, address(account), 0, cd, 0);
        vm.expectRevert("PQ: session cannot target self");
        account.executeSession(address(account), 0, cd, 0, s, 0);
    }

    function test_grantSession_only_root() public {
        bytes32[] memory keys = new bytes32[](0);
        uint256[] memory amts = new uint256[](0);
        vm.expectRevert("PQ: only root (via executeRoot)");
        account.grantSession(vm.addr(SK), type(uint64).max, 3, 1 ether, keys, amts);
    }

    function test_erc1271_root_only() public {
        bytes memory s = abi.encodePacked(pubkey, sig); // 2592 + 4627
        _etchTrue();
        assertEq(account.isValidSignature(keccak256("hello"), s), bytes4(0x1626ba7e), "root sig valid via F003");
        _etchFalse();
        assertEq(account.isValidSignature(keccak256("hello"), s), bytes4(0xffffffff), "F003 false -> invalid");
        // wrong length -> invalid (and a 65-byte secp256k1 session sig is never 1271-valid).
        assertEq(account.isValidSignature(keccak256("hello"), hex"1234"), bytes4(0xffffffff), "bad length -> invalid");
    }

    // ------------------------------------------------- vault owner: rotation/freeze

    function _vault(uint8 opType, bytes32 hi, bytes32 lo, uint64 vNonce) internal {
        account.vaultExecute(opType, hi, lo, vNonce, pubkey, sig); // mock F003 ignores key/sig
    }

    function test_vault_freeze_blocks_root_and_session_then_unfreeze() public {
        _etchTrue();
        _grantPing(5 ether, 3); // grant a session BEFORE freeze
        _vault(1, bytes32(0), bytes32(0), 0); // FREEZE
        assertTrue(account.frozen());

        vm.expectRevert("PQ: account frozen");
        _exec(0); // executeRoot blocked

        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        vm.expectRevert("PQ: account frozen");
        account.executeSession(address(target), 0, cd, 0, _sessionSig(SK, address(target), 0, cd, 0), 0); // session blocked

        _vault(2, bytes32(0), bytes32(0), 1); // UNFREEZE
        assertFalse(account.frozen());
        _exec(0); // works again
        assertEq(account.rootNonce(), 1);
    }

    function test_vault_rotate_invalidates_sessions_and_changes_root() public {
        _etchTrue();
        _grantPing(5 ether, 3); // session granted at epoch 0
        _vault(0, bytes32(uint256(0x9999)), bytes32(uint256(0xAAAA)), 0); // ROTATE
        assertEq(account.rootEpoch(), 1, "epoch bumped");
        assertEq(account.operationalRootPayloadHi(), bytes32(uint256(0x9999)), "operational root rotated");
        // the session (granted at epoch 0) is now invalid.
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        vm.expectRevert("PQ: session inactive");
        account.executeSession(address(target), 0, cd, 0, _sessionSig(SK, address(target), 0, cd, 0), 0);
    }

    function test_vault_bad_nonce_reverts() public {
        _etchTrue();
        vm.expectRevert("PQ: bad vault nonce");
        _vault(1, bytes32(0), bytes32(0), 5);
    }

    function test_vault_auth_false_reverts() public {
        _etchFalse();
        vm.expectRevert("PQ: ml-dsa vault auth failed");
        _vault(1, bytes32(0), bytes32(0), 0);
    }

    function test_vault_rotate_zero_root_reverts() public {
        _etchTrue();
        vm.expectRevert("PQ: zero operational root");
        _vault(0, bytes32(0), bytes32(0), 0);
    }

    function test_vault_unknown_op_reverts() public {
        _etchTrue();
        vm.expectRevert("PQ: unknown vault op");
        _vault(9, bytes32(0), bytes32(0), 0);
    }

    function test_vault_can_rotate_while_frozen() public {
        _etchTrue();
        _vault(1, bytes32(0), bytes32(0), 0); // FREEZE
        assertTrue(account.frozen());
        // Anti-lockout: the Vault Owner can still ROTATE (and UNFREEZE) while frozen.
        _vault(0, bytes32(uint256(0x9999)), bytes32(uint256(0xAAAA)), 1); // ROTATE
        assertEq(account.rootEpoch(), 1, "rotated while frozen");
        assertTrue(account.frozen(), "still frozen after rotate");
    }

    function test_session_regrant_narrows_allowlist() public {
        _etchTrue();
        address sk = _grantPing(5 ether, 3); // gen 1: (target, ping) allowed
        // Re-grant the SAME key with an EMPTY allowlist (narrowing).
        bytes32[] memory empty = new bytes32[](0);
        uint256[] memory emptyAmts = new uint256[](0);
        _grant(sk, empty, emptyAmts, 5 ether, 3); // gen 2: nothing allowed
        // The prior (target, ping) allowance MUST NOT survive the re-grant.
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        vm.expectRevert("PQ: target/selector not allowed");
        account.executeSession(address(target), 0, cd, 0, _sessionSig(SK, address(target), 0, cd, 0), 0);
    }

    // ----------------------------------------------- relayer fee reimbursement (§16.3)

    function test_executeRoot_reimburses_relayer_capped() public {
        _etchTrue();
        address relayer = address(0xCAFE);
        uint256 maxFee = 1000; // wei
        vm.txGasPrice(1e12); // true cost exceeds the cap
        bytes memory cd = abi.encodeWithSelector(CallTarget.ping.selector, uint256(1));
        uint256 before = relayer.balance;
        vm.prank(relayer, relayer); // msg.sender == tx.origin == relayer
        account.executeRoot(address(target), 0, cd, 0, type(uint64).max, 0, pubkey, sig, maxFee);
        assertEq(relayer.balance - before, maxFee, "root op reimburses relayer, capped at maxRelayerFee");
        assertEq(account.rootNonce(), 1, "op still executed");
    }

    function test_executeRoot_zero_fee_no_payment() public {
        _etchTrue();
        address relayer = address(0xCAFE);
        vm.txGasPrice(1e12);
        bytes memory cd = abi.encodeWithSelector(CallTarget.ping.selector, uint256(1));
        uint256 before = relayer.balance;
        vm.prank(relayer, relayer);
        account.executeRoot(address(target), 0, cd, 0, type(uint64).max, 0, pubkey, sig, 0);
        assertEq(relayer.balance, before, "no fee when maxRelayerFee == 0");
    }

    /// An op whose own value-forward drains the account below the fee is NOT bricked at
    /// the fee step — reimbursement is best-effort (capped at the remaining balance).
    function test_executeRoot_self_draining_op_not_bricked_by_fee() public {
        _etchTrue();
        address relayer = address(0xCAFE);
        vm.txGasPrice(1e12); // a fee would be due, but the op forwards the whole balance
        bytes memory cd = abi.encodeWithSelector(CallTarget.ping.selector, uint256(1));
        vm.prank(relayer, relayer);
        // forward the full 100 ether to the (payable) target, leaving nothing for the fee
        account.executeRoot(address(target), 100 ether, cd, 0, type(uint64).max, 0, pubkey, sig, 1 ether);
        assertEq(account.rootNonce(), 1, "op executed despite no balance left for the fee");
        assertEq(target.lastValue(), 100 ether, "value forwarded");
    }
}

/// Minimal ERC-20-like target for the session amount-cap test.
contract MockToken {
    mapping(address => uint256) public sent;

    function transfer(address to, uint256 amount) external returns (bool) {
        sent[to] += amount;
        return true;
    }
}
