// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Test} from "forge-std/Test.sol";
import {MisakaPqSmartAccount} from "../src/MisakaPqSmartAccount.sol";

// EIP-1967 implementation slot: keccak256("eip1967.proxy.implementation") - 1.
bytes32 constant EIP1967_IMPL_SLOT = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

/// A trivial implementation a proxy could point at — exposes the generic `poke(uint256)`
/// selector the session would call. Its presence is irrelevant to the deny (the account
/// never inspects the impl); it only makes the scenario concrete.
contract Impl {
    function poke(uint256) external payable returns (uint256) {
        return 1;
    }
}

/// A plain (non-proxy) contract — neither `proxiableUUID()` nor `implementation()`. Used to
/// prove the H-6 default-deny is the gate: an UNapproved generic call reverts, an APPROVED one
/// (NON_PROXY) succeeds, and a revoke / root rotation re-denies it.
contract PlainTarget {
    uint256 public last;

    function poke(uint256 x) external payable returns (uint256) {
        last = x;
        return x;
    }
}

/// An OZ TransparentUpgradeableProxy-style mock: the `implementation()` getter is ADMIN-ONLY,
/// so a call from the smart account (not the admin) REVERTS — the proxy is UNDETECTABLE by the
/// old H-2 heuristic (`_isUpgradeableProxy` would have classified it as a non-proxy and let the
/// call through). H-6 default-deny rejects it because the root never approved it.
contract UndetectableTransparentProxyMock {
    address internal immutable admin;
    address internal impl;

    constructor(address impl_) {
        admin = msg.sender;
        impl = impl_;
    }

    /// Admin-only getter — reverts for any other caller (the account), so the proxy looks like
    /// a plain contract to the old detection probe.
    function implementation() external view returns (address) {
        require(msg.sender == admin, "admin only");
        return impl;
    }

    function poke(uint256) external payable returns (uint256) {
        return 7;
    }
}

/// An EIP-1167 minimal clone exposes NO getters at all (its runtime is just the 45-byte
/// delegatecall stub). We model it with a contract that has the generic selector but no proxy
/// interface, so the old heuristic finds nothing — H-6 still denies it (not root-approved).
contract MinimalCloneMock {
    function poke(uint256) external payable returns (uint256) {
        return 11;
    }
    // Deliberately NO proxiableUUID()/implementation() — mirrors a 1167 clone's bare interface.
}

/// Beacon implementations with DISTINCT bytecode, so swapping the beacon's impl changes the
/// impl `codehash` (the thing an H-6 BEACON approval pins).
contract BeaconImplA {
    function poke(uint256) external payable returns (uint256) {
        return 100;
    }
}

contract BeaconImplB {
    // Different body => different bytecode => different codehash than BeaconImplA.
    uint256 public marker;

    function poke(uint256 x) external payable returns (uint256) {
        marker = x + 1;
        return 200;
    }
}

/// A beacon-backed proxy the session calls. It exposes `implementation()` (the OZ Beacon
/// signal) returning the CURRENT impl address, and forwards generic calls to it. Swapping the
/// impl (`setImpl`) does NOT change the proxy's own bytecode — exactly the H-2 hole a proxy-
/// bytecode `codeHashPin` cannot close. H-6 closes it: a BEACON approval pins the impl codehash,
/// which the account re-reads via `implementation()` on every call.
contract BeaconProxyMock {
    address public impl;

    constructor(address impl_) {
        impl = impl_;
    }

    function implementation() external view returns (address) {
        return impl;
    }

    function setImpl(address impl_) external {
        impl = impl_;
    }

    fallback() external payable {
        address i = impl;
        (bool ok, bytes memory ret) = i.delegatecall(msg.data);
        require(ok, "beacon proxy: impl reverted");
        assembly {
            return(add(ret, 0x20), mload(ret))
        }
    }

    receive() external payable {}
}

/// H-6: the heuristic proxy detect-and-deny (H-2) MISSED OZ Transparent/Beacon/EIP-1167/registry
/// proxies, which fell through and EXECUTED while a `codeHashPin` froze only the (stable) proxy
/// bytecode, not the delegated implementation. H-6 replaces it with DEFAULT-DENY + a root-approved
/// allowlist (`approveTarget`/`revokeTarget`, namespaced by `rootEpoch`) for generic contract
/// calls; BEACON approvals additionally pin the beacon impl codehash.
contract ProxySwapTest is Test {
    MisakaPqSmartAccount internal account;

    uint256 internal constant SK = 0xA11CE;
    uint64 internal constant VERSION = 1;
    uint8 internal constant TS_NATIVE = 0;

    // Proxy classes (mirror MisakaPqSmartAccount.PROXY_CLASS_*).
    uint8 internal constant PC_NON_PROXY = 0;
    uint8 internal constant PC_UUPS = 1;
    uint8 internal constant PC_TRANSPARENT = 2;
    uint8 internal constant PC_BEACON = 3;
    uint8 internal constant PC_OTHER = 4;

    function setUp() public {
        account = new MisakaPqSmartAccount(
            bytes32(uint256(0x7777)), bytes32(uint256(0x8888)), bytes32(uint256(0x1111)), bytes32(uint256(0x2222)), VERSION
        );
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

    function _entryPinned(bytes32 key, uint8 std, uint256 maxPerCall, uint256 maxTotal, uint256 id, bytes32 codeHashPin)
        internal
        pure
        returns (MisakaPqSmartAccount.PolicyEntry memory)
    {
        return MisakaPqSmartAccount.PolicyEntry({
            targetSelectorKey: key,
            standard: std,
            maxPerCall: maxPerCall,
            maxTotal: maxTotal,
            erc1155TokenId: id,
            codeHashPin: codeHashPin
        });
    }

    /// Grant the session a GENERIC (NATIVE) allow for (target, sel). The session policy's own
    /// codeHashPin is now irrelevant for a generic contract call (H-6 uses the root allowlist),
    /// so pin it to the target codehash to prove the OLD path is no longer the gate.
    function _grantGenericPinned(address target, bytes4 sel, bytes32 pin) internal {
        MisakaPqSmartAccount.PolicyEntry[] memory e = new MisakaPqSmartAccount.PolicyEntry[](1);
        e[0] = _entryPinned(account.allowKey(target, sel), TS_NATIVE, 0, 0, 0, pin);
        _grantV2(e, 1 ether, 5);
    }

    /// Root-approve a generic-call target (an `executeRoot` self-call, mocked via prank).
    function _approveTarget(
        address target,
        bytes32 codeHashPin,
        uint8 proxyClass,
        bytes32 implOrBeaconHash,
        bool requireImplPin
    ) internal {
        vm.prank(address(account));
        account.approveTarget(target, codeHashPin, proxyClass, implOrBeaconHash, requireImplPin);
    }

    function _exec(address target, bytes memory cd, uint64 idx) internal returns (bytes memory) {
        return account.executeSession(target, 0, cd, idx, _sessionSig(target, 0, cd, idx), 0);
    }

    // ----------------------------------------------------------------------- tests

    /// (1) A generic call to an unapproved-but-policy-pinned PlainTarget reverts not-root-approved.
    /// Pinning in the SESSION policy (the old H-2 gate) is no longer sufficient.
    function test_generic_call_unapproved_reverts() public {
        PlainTarget tgt = new PlainTarget();
        bytes4 sel = PlainTarget.poke.selector;
        _grantGenericPinned(address(tgt), sel, address(tgt).codehash);

        bytes memory cd = abi.encodeWithSelector(sel, uint256(7));
        vm.expectRevert("PQ: generic call target not root-approved");
        _exec(address(tgt), cd, 0);
    }

    /// (2) An UNDETECTABLE transparent proxy (admin-only `implementation()` that reverts for the
    /// account) is denied as not-approved — the H-2 heuristic would have missed it and executed.
    function test_undetectable_transparent_proxy_denied() public {
        Impl impl = new Impl();
        UndetectableTransparentProxyMock proxy = new UndetectableTransparentProxyMock(address(impl));
        bytes4 sel = UndetectableTransparentProxyMock.poke.selector;
        _grantGenericPinned(address(proxy), sel, address(proxy).codehash);

        bytes memory cd = abi.encodeWithSelector(sel, uint256(0));
        vm.expectRevert("PQ: generic call target not root-approved");
        _exec(address(proxy), cd, 0);
    }

    /// (3) An EIP-1167 minimal clone (no getters at all) is denied — nothing for the old probe to
    /// detect, but H-6 default-deny still rejects it.
    function test_minimal_clone_denied() public {
        MinimalCloneMock clone = new MinimalCloneMock();
        bytes4 sel = MinimalCloneMock.poke.selector;
        _grantGenericPinned(address(clone), sel, address(clone).codehash);

        bytes memory cd = abi.encodeWithSelector(sel, uint256(0));
        vm.expectRevert("PQ: generic call target not root-approved");
        _exec(address(clone), cd, 0);
    }

    /// (4) A BEACON approved with the impl codehash pinned: the first call succeeds; after the
    /// beacon swaps to a DIFFERENT impl (different codehash), the next call reverts on impl-hash
    /// mismatch. The proxy's own codehash is stable throughout.
    function test_beacon_impl_swap_reverts_on_impl_hash_mismatch() public {
        BeaconImplA implA = new BeaconImplA();
        BeaconImplB implB = new BeaconImplB();
        // sanity: distinct impl bytecode so the swap is observable.
        assertTrue(address(implA).codehash != address(implB).codehash, "impls must differ");

        BeaconProxyMock proxy = new BeaconProxyMock(address(implA));
        bytes4 sel = BeaconImplA.poke.selector;
        _grantGenericPinned(address(proxy), sel, address(proxy).codehash);

        bytes32 proxyHash = address(proxy).codehash;
        _approveTarget(address(proxy), proxyHash, PC_BEACON, address(implA).codehash, true);

        bytes memory cd = abi.encodeWithSelector(sel, uint256(0));
        // First call: impl is implA, matches the pinned impl hash.
        _exec(address(proxy), cd, 0);

        // Swap the beacon impl; proxy bytecode unchanged, but implementation() now returns implB.
        proxy.setImpl(address(implB));
        assertEq(address(proxy).codehash, proxyHash, "beacon proxy codehash stable across impl swap");

        vm.expectRevert("PQ: beacon impl-hash mismatch");
        _exec(address(proxy), cd, 1);
    }

    /// (5a) approveTarget then revokeTarget => the generic call reverts again.
    function test_approve_then_revoke_reverts() public {
        PlainTarget tgt = new PlainTarget();
        bytes4 sel = PlainTarget.poke.selector;
        _grantGenericPinned(address(tgt), sel, address(tgt).codehash);
        _approveTarget(address(tgt), address(tgt).codehash, PC_NON_PROXY, bytes32(0), false);

        bytes memory cd = abi.encodeWithSelector(sel, uint256(7));
        _exec(address(tgt), cd, 0); // approved => succeeds
        assertEq(tgt.last(), 7, "approved generic call executes");

        vm.prank(address(account));
        account.revokeTarget(address(tgt));

        bytes memory cd2 = abi.encodeWithSelector(sel, uint256(9));
        vm.expectRevert("PQ: generic call target not root-approved");
        _exec(address(tgt), cd2, 1);
    }

    /// (5b) A root rotation (rootEpoch bump) invalidates an approval — the approval is namespaced
    /// by rootEpoch, so a call after rotation default-denies even without an explicit revoke.
    /// (The grant also binds rootEpoch, so the session itself is inactive — the structural check
    /// is "session inactive" first; this proves the approval did not carry into the new epoch.)
    function test_root_rotation_invalidates_approval() public {
        PlainTarget tgt = new PlainTarget();
        bytes32 hash = address(tgt).codehash;
        _approveTarget(address(tgt), hash, PC_NON_PROXY, bytes32(0), false);

        uint64 epochBefore = account.rootEpoch();
        (bool approvedBefore,,,,) = account.approvedTargets(epochBefore, address(tgt));
        assertTrue(approvedBefore, "approved under current epoch");

        // Rotate the operational root via the Vault Owner is gated on F003 (inert on test nets),
        // so assert the namespacing directly: a different epoch has no approval.
        (bool approvedNextEpoch,,,,) = account.approvedTargets(epochBefore + 1, address(tgt));
        assertTrue(!approvedNextEpoch, "approval does not carry into the next root epoch");
    }

    /// (6) UPDATED: the existing plain-contract generic call now requires an APPROVE first, then
    /// succeeds (the session-policy pin alone is no longer sufficient).
    function test_plain_contract_generic_pinned_call_still_allowed() public {
        PlainTarget tgt = new PlainTarget();
        bytes4 sel = PlainTarget.poke.selector;
        _grantGenericPinned(address(tgt), sel, address(tgt).codehash);
        _approveTarget(address(tgt), address(tgt).codehash, PC_NON_PROXY, bytes32(0), false);

        bytes memory cd = abi.encodeWithSelector(sel, uint256(7));
        _exec(address(tgt), cd, 0);
        assertEq(tgt.last(), 7, "approved generic pinned call to a non-proxy executes");
    }

    /// (7) approveTarget / revokeTarget called by a non-root (anyone but the account itself)
    /// reverts — they are root-only (executeRoot self-call), mirroring grant/revokeSession.
    function test_approve_revoke_non_root_reverts() public {
        PlainTarget tgt = new PlainTarget();
        bytes32 hash = address(tgt).codehash;

        vm.expectRevert("PQ: only root (via executeRoot)");
        account.approveTarget(address(tgt), hash, PC_NON_PROXY, bytes32(0), false);

        vm.expectRevert("PQ: only root (via executeRoot)");
        account.revokeTarget(address(tgt));

        // Even with a non-account prank it must revert.
        vm.prank(address(0xBEEF));
        vm.expectRevert("PQ: only root (via executeRoot)");
        account.approveTarget(address(tgt), hash, PC_NON_PROXY, bytes32(0), false);
    }

    /// Config guard: approveTarget rejects requireImplPin for non-BEACON classes (the impl is not
    /// on-chain verifiable for UUPS/TRANSPARENT/OTHER), and a zero codehash pin.
    function test_approve_target_config_guards() public {
        PlainTarget tgt = new PlainTarget();
        bytes32 hash = address(tgt).codehash;

        vm.prank(address(account));
        vm.expectRevert("PQ: code-hash pin required");
        account.approveTarget(address(tgt), bytes32(0), PC_NON_PROXY, bytes32(0), false);

        vm.prank(address(account));
        vm.expectRevert("PQ: impl pin only supported for BEACON");
        account.approveTarget(address(tgt), hash, PC_UUPS, bytes32(uint256(1)), true);

        vm.prank(address(account));
        vm.expectRevert("PQ: beacon impl hash required");
        account.approveTarget(address(tgt), hash, PC_BEACON, bytes32(0), true);
    }

    /// A generic call whose target's codehash no longer matches the approved pin reverts on
    /// code-hash mismatch (the approval pin is enforced independently of the session policy pin).
    function test_approved_target_codehash_mismatch_reverts() public {
        PlainTarget tgt = new PlainTarget();
        bytes4 sel = PlainTarget.poke.selector;
        _grantGenericPinned(address(tgt), sel, address(tgt).codehash);
        // Approve with a WRONG codehash pin.
        _approveTarget(address(tgt), keccak256("not the real codehash"), PC_NON_PROXY, bytes32(0), false);

        bytes memory cd = abi.encodeWithSelector(sel, uint256(7));
        vm.expectRevert("PQ: code-hash mismatch");
        _exec(address(tgt), cd, 0);
    }
}
