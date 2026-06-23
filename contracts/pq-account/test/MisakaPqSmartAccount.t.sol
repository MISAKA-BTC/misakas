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

    bytes32 internal constant RP_HI = bytes32(uint256(0x1111));
    bytes32 internal constant RP_LO = bytes32(uint256(0x2222));
    uint64 internal constant VERSION = 1;

    // The mock F003 ignores these; the REAL key/sig binding is the Rust e2e's job.
    bytes internal pubkey = new bytes(2592);
    bytes internal sig = new bytes(4627);

    function setUp() public {
        account = new MisakaPqSmartAccount(RP_HI, RP_LO, VERSION);
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
            sig
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
        account.executeRoot(address(target), 0, "", 0, 10, 0, pubkey, sig);
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
            keccak256(abi.encode(domain, block.chainid, address(account), VERSION, tgt, value, keccak256(callData), callIndex));
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
        bytes memory ret = account.executeSession(address(target), 1 ether, cd, 0, s);
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
        account.executeSession(address(target), 0, cd, 0, s);
    }

    function test_session_unlisted_target_reverts() public {
        address sk = vm.addr(SK);
        bytes32[] memory keys = new bytes32[](0);
        uint256[] memory amts = new uint256[](0);
        _grant(sk, keys, amts, 5 ether, 3); // grant exists but no (target,selector) allowed
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 0);
        vm.expectRevert("PQ: target/selector not allowed");
        account.executeSession(address(target), 0, cd, 0, s);
    }

    function test_session_native_cap_reverts() public {
        _grantPing(1 ether, 3); // maxNativeTotal = 1 ether
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 2 ether, cd, 0);
        vm.expectRevert("PQ: session native cap");
        account.executeSession(address(target), 2 ether, cd, 0, s);
    }

    function test_session_call_cap_reverts() public {
        _grantPing(10 ether, 1); // maxCalls = 1
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        account.executeSession(address(target), 0, cd, 0, _sessionSig(SK, address(target), 0, cd, 0));
        vm.expectRevert("PQ: session call cap");
        account.executeSession(address(target), 0, cd, 1, _sessionSig(SK, address(target), 0, cd, 1));
    }

    function test_session_bad_call_index_reverts() public {
        _grantPing(5 ether, 3);
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 5);
        vm.expectRevert("PQ: bad session call index");
        account.executeSession(address(target), 0, cd, 5, s);
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
        account.executeSession(address(target), 0, cd, 0, s);
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
        account.executeSession(address(token), 0, cdBad, 0, _sessionSig(SK, address(token), 0, cdBad, 0));

        bytes memory cdOk = abi.encodeWithSelector(SEL_TRANSFER, address(0xBEEF), uint256(50));
        account.executeSession(address(token), 0, cdOk, 0, _sessionSig(SK, address(token), 0, cdOk, 0));
        assertEq(token.sent(address(0xBEEF)), 50, "capped transfer executed");
    }

    function test_session_revoke() public {
        address sk = _grantPing(5 ether, 3);
        vm.prank(address(account));
        account.revokeSession(sk);
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(SK, address(target), 0, cd, 0);
        vm.expectRevert("PQ: session inactive");
        account.executeSession(address(target), 0, cd, 0, s);
    }

    function test_session_ungranted_key_reverts() public {
        bytes memory cd = abi.encodeWithSelector(SEL_PING, uint256(1));
        bytes memory s = _sessionSig(0xBADBAD, address(target), 0, cd, 0); // key with no grant
        vm.expectRevert("PQ: session inactive");
        account.executeSession(address(target), 0, cd, 0, s);
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
        account.executeSession(address(account), 0, cd, 0, s);
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
}

/// Minimal ERC-20-like target for the session amount-cap test.
contract MockToken {
    mapping(address => uint256) public sent;

    function transfer(address to, uint256 amount) external returns (bool) {
        sent[to] += amount;
        return true;
    }
}
