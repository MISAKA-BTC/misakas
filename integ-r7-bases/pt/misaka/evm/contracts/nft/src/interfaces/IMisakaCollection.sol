// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMisakaCollection
/// @notice The MISAKA collection surface beyond the ERC-721 standard: the
///         fixed supply, the mint progress, the metadata-mutability flag, and
///         the content-addressed collection manifest. Indexers/Explorer read
///         these to render a collection without trusting off-chain claims.
interface IMisakaCollection {
    /// Emitted once when the collection's metadata becomes permanently frozen
    /// (immutable collections emit this at construction).
    event MetadataFrozen(bytes32 indexed manifestHash);

    /// Emitted at construction: binds the on-chain collection to its
    /// content-addressed manifest (keccak256 of the canonical manifest bytes).
    event CollectionManifest(bytes32 indexed manifestHash, string uri);

    function maxSupply() external view returns (uint256);
    function totalMinted() external view returns (uint256);
    function metadataFrozen() external view returns (bool);
    function collectionManifestURI() external view returns (string memory);

    /// keccak256 of the canonical collection manifest bytes — the content hash
    /// indexers are told to trust. Declared here so it is reachable through the
    /// documented IMisakaCollection ABI, not just the concrete contract.
    function manifestHash() external view returns (bytes32);
}
