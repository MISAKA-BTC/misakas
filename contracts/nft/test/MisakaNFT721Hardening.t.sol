// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {StdInvariant} from "forge-std/StdInvariant.sol";
import {MisakaNFT721Immutable} from "../src/MisakaNFT721Immutable.sol";
import {IMisakaCollection} from "../src/interfaces/IMisakaCollection.sol";

import {IERC721Receiver} from "@openzeppelin/contracts/token/ERC721/IERC721Receiver.sol";
import {IERC721Errors} from "@openzeppelin/contracts/interfaces/draft-IERC6093.sol";
import {IAccessControl} from "@openzeppelin/contracts/access/IAccessControl.sol";

// --- receiver mocks (L-01) ---

contract GoodReceiver is IERC721Receiver {
    function onERC721Received(address, address, uint256, bytes calldata) external pure returns (bytes4) {
        return IERC721Receiver.onERC721Received.selector;
    }
}

contract RevertingReceiver is IERC721Receiver {
    function onERC721Received(address, address, uint256, bytes calldata) external pure returns (bytes4) {
        revert("nope");
    }
}

contract WrongMagicReceiver is IERC721Receiver {
    function onERC721Received(address, address, uint256, bytes calldata) external pure returns (bytes4) {
        return 0xdeadbeef;
    }
}

/// Re-enters safeMint inside the callback. The mint role is held by the test
/// contract, NOT this mock, so the re-entrant call must revert on AccessControl
/// — proving onlyRole is the gate even under reentrancy.
contract ReentrantReceiver is IERC721Receiver {
    MisakaNFT721Immutable public nft;

    function set(MisakaNFT721Immutable n) external {
        nft = n;
    }

    function onERC721Received(address, address, uint256, bytes calldata) external returns (bytes4) {
        nft.safeMint(address(this)); // expected to revert (this mock lacks MINTER_ROLE)
        return IERC721Receiver.onERC721Received.selector;
    }
}

contract MisakaNFT721HardeningTest is Test {
    MisakaNFT721Immutable internal nft;

    address internal admin = address(0xA11CE);
    address internal minter = address(0x814E5);
    address internal alice = address(0xA1);

    string internal constant NAME = "Misaka Hardening";
    string internal constant SYMBOL = "MHD";
    uint256 internal constant MAX_SUPPLY = 5;
    string internal constant BASE_URI = "ipfs://bafyTESTcid/";
    bytes32 internal constant MANIFEST = keccak256("manifest");
    string internal constant COLLECTION_URI = "ipfs://bafyManifest/collection.json";

    function _deploy() internal returns (MisakaNFT721Immutable) {
        return new MisakaNFT721Immutable(
            NAME, SYMBOL, MAX_SUPPLY, BASE_URI, MANIFEST, COLLECTION_URI, admin, minter, admin, 500
        );
    }

    function setUp() public {
        nft = _deploy();
    }

    // --- M-05: strict constructor guards ---
    function test_constructor_rejects_empty_baseURI() public {
        vm.expectRevert(MisakaNFT721Immutable.EmptyBaseURI.selector);
        new MisakaNFT721Immutable(NAME, SYMBOL, MAX_SUPPLY, "", MANIFEST, COLLECTION_URI, admin, minter, admin, 500);
    }

    function test_constructor_rejects_empty_collectionURI() public {
        vm.expectRevert(MisakaNFT721Immutable.EmptyCollectionURI.selector);
        new MisakaNFT721Immutable(NAME, SYMBOL, MAX_SUPPLY, BASE_URI, MANIFEST, "", admin, minter, admin, 500);
    }

    function test_constructor_rejects_zero_manifestHash() public {
        vm.expectRevert(MisakaNFT721Immutable.ZeroManifestHash.selector);
        new MisakaNFT721Immutable(NAME, SYMBOL, MAX_SUPPLY, BASE_URI, bytes32(0), COLLECTION_URI, admin, minter, admin, 500);
    }

    function test_manifestHash_reachable_via_interface() public view {
        assertEq(IMisakaCollection(address(nft)).manifestHash(), MANIFEST);
    }

    // --- M-04: irreversible mint seal ---
    function test_finishMinting_seals_minting_forever() public {
        bytes32 mr = nft.MINTER_ROLE(); // read before any prank (Foundry next-call semantics)
        vm.prank(minter);
        nft.safeMint(alice);
        assertFalse(nft.mintingFinished());

        vm.prank(admin);
        nft.finishMinting();
        assertTrue(nft.mintingFinished());

        // Even with the role, no further mint.
        vm.prank(minter);
        vm.expectRevert(MisakaNFT721Immutable.MintingIsFinished.selector);
        nft.safeMint(alice);

        // Re-granting MINTER_ROLE does NOT reopen minting.
        vm.prank(admin);
        nft.grantRole(mr, alice);
        vm.prank(alice);
        vm.expectRevert(MisakaNFT721Immutable.MintingIsFinished.selector);
        nft.safeMint(alice);
    }

    function test_finishMinting_only_admin() public {
        bytes32 adminRole = nft.DEFAULT_ADMIN_ROLE();
        vm.expectRevert(abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, alice, adminRole));
        vm.prank(alice);
        nft.finishMinting();
    }

    function test_finishMinting_not_twice() public {
        vm.startPrank(admin);
        nft.finishMinting();
        vm.expectRevert(MisakaNFT721Immutable.MintingAlreadyFinished.selector);
        nft.finishMinting();
        vm.stopPrank();
    }

    // --- L-01: AccessControl lifecycle ---
    function test_role_grant_revoke_renounce() public {
        bytes32 mr = nft.MINTER_ROLE();
        vm.prank(admin);
        nft.grantRole(mr, alice);
        assertTrue(nft.hasRole(mr, alice));
        vm.prank(alice);
        nft.safeMint(alice); // alice can now mint
        assertEq(nft.ownerOf(1), alice);

        vm.prank(admin);
        nft.revokeRole(mr, alice);
        assertFalse(nft.hasRole(mr, alice));
        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, alice, mr));
        nft.safeMint(alice);

        // original minter can renounce its own role
        vm.prank(minter);
        nft.renounceRole(mr, minter);
        assertFalse(nft.hasRole(mr, minter));
    }

    // --- L-01: ERC721Receiver behavior ---
    function test_safeMint_to_good_receiver() public {
        GoodReceiver r = new GoodReceiver();
        vm.prank(minter);
        nft.safeMint(address(r));
        assertEq(nft.ownerOf(1), address(r));
    }

    function test_safeMint_to_reverting_receiver_reverts() public {
        RevertingReceiver r = new RevertingReceiver();
        vm.prank(minter);
        vm.expectRevert(bytes("nope"));
        nft.safeMint(address(r));
    }

    function test_safeMint_to_wrong_magic_receiver_reverts() public {
        WrongMagicReceiver r = new WrongMagicReceiver();
        vm.prank(minter);
        vm.expectRevert(abi.encodeWithSelector(IERC721Errors.ERC721InvalidReceiver.selector, address(r)));
        nft.safeMint(address(r));
    }

    function test_reentrant_receiver_cannot_mint_without_role() public {
        ReentrantReceiver r = new ReentrantReceiver();
        r.set(nft);
        // The re-entrant safeMint reverts on AccessControl, bubbling up and
        // failing the outer mint — so no token is created.
        bytes32 mr = nft.MINTER_ROLE(); // read before prank (Foundry next-call semantics)
        vm.prank(minter);
        vm.expectRevert(abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, address(r), mr));
        nft.safeMint(address(r));
        assertEq(nft.totalMinted(), 0);
    }

    // --- L-01: fuzz + invariant ---
    function testFuzz_mint_never_exceeds_cap(uint8 n) public {
        uint256 want = bound(n, 0, MAX_SUPPLY);
        vm.startPrank(minter);
        for (uint256 i = 0; i < want; i++) {
            nft.safeMint(alice);
        }
        vm.stopPrank();
        assertEq(nft.totalMinted(), want);
        assertLe(nft.totalMinted(), nft.maxSupply());
    }

    function test_token_ids_unique_and_sequential() public {
        vm.startPrank(minter);
        for (uint256 i = 1; i <= MAX_SUPPLY; i++) {
            assertEq(nft.safeMint(alice), i);
        }
        vm.stopPrank();
        // every id 1..MAX_SUPPLY exists exactly once and is owned
        for (uint256 i = 1; i <= MAX_SUPPLY; i++) {
            assertEq(nft.ownerOf(i), alice);
        }
    }
}

/// Stateful invariant: across an arbitrary sequence of mints, totalMinted never
/// exceeds maxSupply (the handler only calls safeMint as the minter).
contract MisakaNFT721InvariantTest is StdInvariant, Test {
    MisakaNFT721Immutable internal nft;
    MintHandler internal handler;
    uint256 internal constant CAP = 10;

    function setUp() public {
        nft = new MisakaNFT721Immutable(
            "Inv", "INV", CAP, "ipfs://b/", keccak256("m"), "ipfs://m/c.json",
            address(this), address(this), address(this), 0
        );
        handler = new MintHandler(nft);
        nft.grantRole(nft.MINTER_ROLE(), address(handler));
        targetContract(address(handler));
    }

    function invariant_totalMinted_within_cap() public view {
        assertLe(nft.totalMinted(), nft.maxSupply());
    }
}

contract MintHandler is Test {
    MisakaNFT721Immutable internal nft;

    constructor(MisakaNFT721Immutable n) {
        nft = n;
    }

    function mint(address to) external {
        // bound recipient to a plain EOA-like address that is non-zero
        if (to == address(0)) to = address(0xBEEF);
        try nft.safeMint(to) {} catch {}
    }
}
