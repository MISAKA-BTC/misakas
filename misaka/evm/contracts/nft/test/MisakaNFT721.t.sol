// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {MisakaNFT721Immutable} from "../src/MisakaNFT721Immutable.sol";
import {IMisakaCollection} from "../src/interfaces/IMisakaCollection.sol";

import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {IERC721} from "@openzeppelin/contracts/token/ERC721/IERC721.sol";
import {IERC721Metadata} from "@openzeppelin/contracts/token/ERC721/extensions/IERC721Metadata.sol";
import {IERC2981} from "@openzeppelin/contracts/interfaces/IERC2981.sol";
import {IAccessControl} from "@openzeppelin/contracts/access/IAccessControl.sol";
import {IERC721Errors} from "@openzeppelin/contracts/interfaces/draft-IERC6093.sol";
import {Strings} from "@openzeppelin/contracts/utils/Strings.sol";

contract MisakaNFT721Test is Test {
    MisakaNFT721Immutable internal nft;

    address internal admin = address(0xA11CE);
    address internal minter = address(0x814E5);
    address internal royaltyReceiver = address(0x0ABE);
    address internal alice = address(0xA1);
    address internal bob = address(0xB0B);

    string internal constant NAME = "Misaka Test Collection";
    string internal constant SYMBOL = "MTC";
    uint256 internal constant MAX_SUPPLY = 3;
    string internal constant BASE_URI = "ipfs://bafyTESTcid/";
    bytes32 internal constant MANIFEST = keccak256("manifest-bytes");
    string internal constant COLLECTION_URI = "ipfs://bafyManifest/collection.json";
    uint96 internal constant ROYALTY_BPS = 500; // 5%

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event MetadataFrozen(bytes32 indexed manifestHash);
    event CollectionManifest(bytes32 indexed manifestHash, string uri);

    function setUp() public {
        nft = new MisakaNFT721Immutable(
            NAME, SYMBOL, MAX_SUPPLY, BASE_URI, MANIFEST, COLLECTION_URI,
            admin, minter, royaltyReceiver, ROYALTY_BPS
        );
    }

    // --- construction / immutable surface ---
    function test_constructor_sets_immutable_state() public view {
        assertEq(nft.name(), NAME);
        assertEq(nft.symbol(), SYMBOL);
        assertEq(nft.maxSupply(), MAX_SUPPLY);
        assertEq(nft.totalMinted(), 0);
        assertTrue(nft.metadataFrozen());
        assertEq(nft.collectionManifestURI(), COLLECTION_URI);
        assertEq(nft.manifestHash(), MANIFEST);
        assertTrue(nft.hasRole(nft.DEFAULT_ADMIN_ROLE(), admin));
        assertTrue(nft.hasRole(nft.MINTER_ROLE(), minter));
    }

    function test_constructor_emits_manifest_and_frozen() public {
        vm.expectEmit(true, false, false, true);
        emit CollectionManifest(MANIFEST, COLLECTION_URI);
        vm.expectEmit(true, false, false, false);
        emit MetadataFrozen(MANIFEST);
        new MisakaNFT721Immutable(
            NAME, SYMBOL, MAX_SUPPLY, BASE_URI, MANIFEST, COLLECTION_URI,
            admin, minter, royaltyReceiver, ROYALTY_BPS
        );
    }

    function test_constructor_rejects_zero_maxSupply() public {
        vm.expectRevert(MisakaNFT721Immutable.ZeroMaxSupply.selector);
        new MisakaNFT721Immutable(
            NAME, SYMBOL, 0, BASE_URI, MANIFEST, COLLECTION_URI,
            admin, minter, royaltyReceiver, ROYALTY_BPS
        );
    }

    function test_constructor_rejects_zero_admin() public {
        vm.expectRevert(MisakaNFT721Immutable.ZeroAdmin.selector);
        new MisakaNFT721Immutable(
            NAME, SYMBOL, MAX_SUPPLY, BASE_URI, MANIFEST, COLLECTION_URI,
            address(0), minter, royaltyReceiver, ROYALTY_BPS
        );
    }

    // --- ERC-165 ---
    function test_supports_interfaces() public view {
        assertTrue(nft.supportsInterface(type(IERC165).interfaceId));
        assertTrue(nft.supportsInterface(type(IERC721).interfaceId));
        assertTrue(nft.supportsInterface(type(IERC721Metadata).interfaceId));
        assertTrue(nft.supportsInterface(type(IERC2981).interfaceId));
        assertTrue(nft.supportsInterface(type(IAccessControl).interfaceId));
        assertTrue(nft.supportsInterface(type(IMisakaCollection).interfaceId));
        assertFalse(nft.supportsInterface(0xffffffff));
    }

    // --- mint authorization ---
    function test_safeMint_only_minter() public {
        // Read the role BEFORE prank — otherwise vm.prank is consumed by this
        // staticcall instead of by safeMint (Foundry next-call semantics).
        bytes32 minterRole = nft.MINTER_ROLE();
        vm.expectRevert(
            abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, alice, minterRole)
        );
        vm.prank(alice);
        nft.safeMint(alice);
    }

    function test_safeMint_assigns_owner_and_emits_transfer() public {
        vm.expectEmit(true, true, true, false);
        emit Transfer(address(0), alice, 1);
        vm.prank(minter);
        uint256 id = nft.safeMint(alice);
        assertEq(id, 1);
        assertEq(nft.ownerOf(1), alice);
        assertEq(nft.balanceOf(alice), 1);
        assertEq(nft.totalMinted(), 1);
    }

    function test_safeMint_rejects_zero_recipient() public {
        vm.prank(minter);
        vm.expectRevert(MisakaNFT721Immutable.MintToZero.selector);
        nft.safeMint(address(0));
    }

    // --- supply cap & monotonic IDs ---
    function test_token_ids_are_monotonic_from_one() public {
        vm.startPrank(minter);
        assertEq(nft.safeMint(alice), 1);
        assertEq(nft.safeMint(bob), 2);
        assertEq(nft.safeMint(alice), 3);
        vm.stopPrank();
        assertEq(nft.totalMinted(), 3);
    }

    function test_safeMint_reverts_past_max_supply() public {
        vm.startPrank(minter);
        nft.safeMint(alice);
        nft.safeMint(alice);
        nft.safeMint(alice);
        vm.expectRevert(MisakaNFT721Immutable.MaxSupplyReached.selector);
        nft.safeMint(alice);
        vm.stopPrank();
    }

    function test_ids_not_reused_after_burn_is_impossible() public {
        // There is no burn; once minted, totalMinted only advances. Mint all,
        // confirm the counter is at the cap and stays there.
        vm.startPrank(minter);
        nft.safeMint(alice);
        nft.safeMint(alice);
        nft.safeMint(alice);
        vm.stopPrank();
        assertEq(nft.totalMinted(), MAX_SUPPLY);
    }

    // --- metadata ---
    function test_tokenURI_appends_json() public {
        vm.prank(minter);
        nft.safeMint(alice);
        assertEq(nft.tokenURI(1), string.concat(BASE_URI, "1.json"));
    }

    function test_tokenURI_reverts_for_unminted() public {
        vm.expectRevert(abi.encodeWithSelector(IERC721Errors.ERC721NonexistentToken.selector, 99));
        nft.tokenURI(99);
    }

    // --- royalty (ERC-2981) ---
    function test_royalty_info() public view {
        (address recv, uint256 amount) = nft.royaltyInfo(1, 10_000);
        assertEq(recv, royaltyReceiver);
        assertEq(amount, (10_000 * ROYALTY_BPS) / 10_000);
    }

    // --- transfer ---
    function test_transfer_moves_ownership() public {
        vm.prank(minter);
        nft.safeMint(alice);
        vm.prank(alice);
        nft.transferFrom(alice, bob, 1);
        assertEq(nft.ownerOf(1), bob);
    }
}
