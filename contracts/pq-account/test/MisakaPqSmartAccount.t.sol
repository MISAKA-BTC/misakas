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
}
