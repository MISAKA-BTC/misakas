// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Test} from "forge-std/Test.sol";
import {MisakaPqSmartAccount} from "../src/MisakaPqSmartAccount.sol";
import {MisakaPqAccountFactory} from "../src/MisakaPqAccountFactory.sol";
import {MisakaPqEntryPoint} from "../src/MisakaPqEntryPoint.sol";

contract MockF003True {
    fallback(bytes calldata) external returns (bytes memory) {
        return abi.encode(true);
    }
}

contract CallTarget {
    uint256 public lastValue;

    function ping(uint256 x) external payable returns (uint256) {
        lastValue = msg.value;
        return x + 1;
    }
}

contract FactoryAndEntryPointTest is Test {
    address internal constant F003 = address(0x0000000000000000000000000000000000F003);

    MisakaPqAccountFactory internal factory;
    MisakaPqEntryPoint internal entryPoint;
    CallTarget internal target;

    bytes32 internal constant VAULT_HI = bytes32(uint256(0xCCCC));
    bytes32 internal constant VAULT_LO = bytes32(uint256(0xDDDD));
    bytes32 internal constant RP_HI = bytes32(uint256(0xAAAA)); // operational root
    bytes32 internal constant RP_LO = bytes32(uint256(0xBBBB));
    uint64 internal constant VERSION = 1;

    bytes internal pubkey = new bytes(2592);
    bytes internal sig = new bytes(4627);

    function setUp() public {
        factory = new MisakaPqAccountFactory();
        entryPoint = new MisakaPqEntryPoint();
        target = new CallTarget();
        vm.etch(F003, address(new MockF003True()).code);
    }

    function _cfg(uint256 index) internal pure returns (MisakaPqAccountFactory.AccountConfig memory) {
        return MisakaPqAccountFactory.AccountConfig({
            vaultOwnerPayloadHi: VAULT_HI,
            vaultOwnerPayloadLo: VAULT_LO,
            operationalRootPayloadHi: RP_HI,
            operationalRootPayloadLo: RP_LO,
            accountVersion: VERSION,
            accountIndex: index
        });
    }

    // --- Factory ---

    function test_getAddress_matches_deployment() public {
        address predicted = factory.getAddress(_cfg(0));
        address deployed = factory.createAccount(_cfg(0));
        assertEq(deployed, predicted, "deployed at predicted CREATE2 address");
        assertTrue(predicted.code.length > 0, "code present");
        // the account carries the root identity it was salted with.
        MisakaPqSmartAccount a = MisakaPqSmartAccount(payable(deployed));
        assertEq(a.operationalRootPayloadHi(), RP_HI);
        assertEq(a.operationalRootPayloadLo(), RP_LO);
        assertEq(a.accountVersion(), VERSION);
    }

    function test_createAccount_is_idempotent() public {
        address a1 = factory.createAccount(_cfg(0));
        address a2 = factory.createAccount(_cfg(0));
        assertEq(a1, a2, "second create returns the same address (no revert)");
    }

    function test_distinct_index_distinct_address() public view {
        address a0 = factory.getAddress(_cfg(0));
        address a1 = factory.getAddress(_cfg(1));
        assertTrue(a0 != a1, "account_index changes the address");
    }

    // --- EntryPoint ---

    function _executeRootCall() internal view returns (bytes memory) {
        return abi.encodeWithSelector(
            MisakaPqSmartAccount.executeRoot.selector,
            address(target),
            uint256(0),
            abi.encodeWithSelector(CallTarget.ping.selector, uint256(1)),
            uint64(0),
            type(uint64).max,
            uint64(0),
            pubkey,
            sig,
            uint256(0)
        );
    }

    function test_entrypoint_deploy_then_execute() public {
        address acct = factory.getAddress(_cfg(7));
        assertEq(acct.code.length, 0, "not yet deployed");

        bytes memory initCode = abi.encodePacked(
            address(factory), abi.encodeWithSelector(factory.createAccount.selector, _cfg(7))
        );
        MisakaPqEntryPoint.UserOp[] memory ops = new MisakaPqEntryPoint.UserOp[](1);
        ops[0] = MisakaPqEntryPoint.UserOp({account: acct, initCode: initCode, callData: _executeRootCall()});

        // A random relayer submits; it has no authority — the account self-validates.
        vm.prank(address(0xBEEF));
        entryPoint.handleOps(ops);

        assertTrue(acct.code.length > 0, "EntryPoint deployed the account via initCode");
        assertEq(MisakaPqSmartAccount(payable(acct)).rootNonce(), 1, "root op executed through the EntryPoint");
    }

    function test_entrypoint_forward_to_deployed() public {
        address acct = factory.createAccount(_cfg(0)); // pre-deployed
        MisakaPqEntryPoint.UserOp[] memory ops = new MisakaPqEntryPoint.UserOp[](1);
        ops[0] = MisakaPqEntryPoint.UserOp({account: acct, initCode: "", callData: _executeRootCall()});
        entryPoint.handleOps(ops);
        assertEq(MisakaPqSmartAccount(payable(acct)).rootNonce(), 1, "forwarded to a deployed account");
    }

    function test_entrypoint_undeployed_no_initcode_reverts() public {
        address acct = factory.getAddress(_cfg(9)); // not deployed
        MisakaPqEntryPoint.UserOp[] memory ops = new MisakaPqEntryPoint.UserOp[](1);
        ops[0] = MisakaPqEntryPoint.UserOp({account: acct, initCode: "", callData: _executeRootCall()});
        vm.expectRevert("EP: account not deployed, no initCode");
        entryPoint.handleOps(ops);
    }

    function test_entrypoint_rejects_non_execute_selector() public {
        address acct = factory.createAccount(_cfg(0));
        // A non-execute selector (e.g. grantSession, or anything else) is refused by the
        // EntryPoint — it relays only the two self-validating ops.
        bytes memory cd =
            abi.encodeWithSelector(MisakaPqSmartAccount.grantSession.selector, address(0xBEEF), uint64(0), uint64(0), uint128(0), new bytes32[](0), new uint256[](0));
        MisakaPqEntryPoint.UserOp[] memory ops = new MisakaPqEntryPoint.UserOp[](1);
        ops[0] = MisakaPqEntryPoint.UserOp({account: acct, initCode: "", callData: cd});
        vm.expectRevert("EP: only execute ops");
        entryPoint.handleOps(ops);
    }
}
