// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import {ERC721Royalty} from "@openzeppelin/contracts/token/ERC721/extensions/ERC721Royalty.sol";
import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {Strings} from "@openzeppelin/contracts/utils/Strings.sol";

import {IMisakaCollection} from "./interfaces/IMisakaCollection.sol";

/// @title MisakaNFT721Immutable
/// @notice The recommended default collectible template for MISAKA: a fully
///         IMMUTABLE ERC-721. Fixed `maxSupply` and `baseURI`, monotonic token
///         IDs from 1 (never reused), `safeMint` gated by MINTER_ROLE, NO
///         metadata setter, NO proxy, NO pause, NO admin rescue/burn of user
///         tokens. Royalty via ERC-2981 (informational; marketplaces enforce).
///
/// @dev SECURITY: settlement is the MISAKA BlockDAG, but owner authorization on
///      the EVM lane is secp256k1/ECDSA — this is NOT post-quantum account
///      security. Do not market tokens minted here as "post-quantum".
contract MisakaNFT721Immutable is ERC721, ERC721Royalty, AccessControl, IMisakaCollection {
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");

    uint256 private immutable _maxSupplyValue;
    bytes32 private immutable _manifestHashValue;
    uint256 private _totalMintedValue;
    string private _baseTokenURI;
    string private _collectionURIValue;

    error MaxSupplyReached();
    error MintToZero();
    error ZeroAdmin();
    error ZeroMaxSupply();

    constructor(
        string memory name_,
        string memory symbol_,
        uint256 maxSupply_,
        string memory baseURI_,
        bytes32 manifestHash_,
        string memory collectionURI_,
        address admin_,
        address minter_,
        address royaltyReceiver_,
        uint96 royaltyBps_
    ) ERC721(name_, symbol_) {
        if (maxSupply_ == 0) revert ZeroMaxSupply(); // 0 is NOT "unlimited"
        if (admin_ == address(0)) revert ZeroAdmin();
        _maxSupplyValue = maxSupply_;
        _baseTokenURI = baseURI_;
        _manifestHashValue = manifestHash_;
        _collectionURIValue = collectionURI_;
        _grantRole(DEFAULT_ADMIN_ROLE, admin_);
        if (minter_ != address(0)) _grantRole(MINTER_ROLE, minter_);
        if (royaltyReceiver_ != address(0) && royaltyBps_ > 0) {
            _setDefaultRoyalty(royaltyReceiver_, royaltyBps_);
        }
        emit CollectionManifest(manifestHash_, collectionURI_);
        emit MetadataFrozen(manifestHash_); // immutable: frozen from birth
    }

    /// Mint the next token to `to`. Token IDs are 1-based, monotonic, never
    /// reused; capped at `maxSupply`. Emits the standard ERC-721 Transfer.
    function safeMint(address to) external onlyRole(MINTER_ROLE) returns (uint256 tokenId) {
        if (to == address(0)) revert MintToZero();
        if (_totalMintedValue >= _maxSupplyValue) revert MaxSupplyReached();
        unchecked {
            tokenId = ++_totalMintedValue;
        }
        _safeMint(to, tokenId);
    }

    // --- IMisakaCollection ---
    function maxSupply() external view returns (uint256) {
        return _maxSupplyValue;
    }

    function totalMinted() external view returns (uint256) {
        return _totalMintedValue;
    }

    /// Always true: this profile's metadata is immutable by construction.
    function metadataFrozen() external pure returns (bool) {
        return true;
    }

    function collectionManifestURI() external view returns (string memory) {
        return _collectionURIValue;
    }

    function manifestHash() external view returns (bytes32) {
        return _manifestHashValue;
    }

    // --- metadata ---
    function _baseURI() internal view override returns (string memory) {
        return _baseTokenURI;
    }

    /// `<baseURI><tokenId>.json` (content-addressed JSON per token).
    function tokenURI(uint256 tokenId) public view override returns (string memory) {
        _requireOwned(tokenId);
        string memory base = _baseURI();
        return bytes(base).length == 0 ? "" : string.concat(base, Strings.toString(tokenId), ".json");
    }

    // --- ERC-165 ---
    function supportsInterface(bytes4 interfaceId)
        public
        view
        override(ERC721, ERC721Royalty, AccessControl)
        returns (bool)
    {
        return interfaceId == type(IMisakaCollection).interfaceId || super.supportsInterface(interfaceId);
    }
}
