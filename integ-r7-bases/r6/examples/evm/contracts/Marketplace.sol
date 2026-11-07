// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IGameItem {
    function ownerOf(uint256) external view returns (address);
    function getApproved(uint256) external view returns (address);
    function isApprovedForAll(address, address) external view returns (bool);
    function transferFrom(address, address, uint256) external;
}

/// @title Marketplace — minimal fixed-price NFT marketplace (list / buy / cancel),
///        paying the seller in native MSK.
/// @notice UNSAFE_MINIMAL_EXAMPLE — teaching code, NOT production. It is
///         deliberately dependency-free. Before any real deployment add: an
///         ERC-2981 fee/royalty split, pausability + access control, listing
///         expiry/nonces, pull-payments (this uses push payment), and an audited
///         reentrancy guard (the inline `nonReentrant` below is a minimal
///         backstop, not a substitute for OpenZeppelin's ReentrancyGuard).
///         evmVersion = "shanghai". Owner authorization is secp256k1, NOT
///         post-quantum.
contract Marketplace {
    struct Listing {
        address seller;
        uint256 price; // wei (MSK has 18 decimals on the EVM lane)
    }

    IGameItem public immutable nft;
    mapping(uint256 => Listing) public listings;

    /// Minimal self-contained reentrancy guard (audit L-02). A production
    /// contract should use OpenZeppelin's ReentrancyGuard.
    uint256 private _entered;

    modifier nonReentrant() {
        require(_entered == 0, "Marketplace: reentrancy");
        _entered = 1;
        _;
        _entered = 0;
    }

    event Listed(uint256 indexed tokenId, address indexed seller, uint256 price);
    event Cancelled(uint256 indexed tokenId, address indexed seller);
    event Bought(uint256 indexed tokenId, address indexed buyer, address indexed seller, uint256 price);

    constructor(address nftAddr) {
        nft = IGameItem(nftAddr);
    }

    /// @notice List `tokenId` for `price` wei. The marketplace must be approved.
    function list(uint256 tokenId, uint256 price) external {
        require(nft.ownerOf(tokenId) == msg.sender, "Marketplace: not owner");
        require(
            nft.getApproved(tokenId) == address(this) || nft.isApprovedForAll(msg.sender, address(this)),
            "Marketplace: not approved"
        );
        require(price > 0, "Marketplace: price = 0");
        listings[tokenId] = Listing(msg.sender, price);
        emit Listed(tokenId, msg.sender, price);
    }

    function cancel(uint256 tokenId) external {
        require(listings[tokenId].seller == msg.sender, "Marketplace: not seller");
        delete listings[tokenId];
        emit Cancelled(tokenId, msg.sender);
    }

    /// @notice Buy `tokenId`, paying exactly the listed price in MSK.
    function buy(uint256 tokenId) external payable nonReentrant {
        Listing memory l = listings[tokenId];
        require(l.price > 0, "Marketplace: not listed");
        require(msg.value == l.price, "Marketplace: wrong price");
        delete listings[tokenId]; // effects before interactions
        nft.transferFrom(l.seller, msg.sender, tokenId);
        (bool ok, ) = payable(l.seller).call{value: msg.value}("");
        require(ok, "Marketplace: payout failed");
        emit Bought(tokenId, msg.sender, l.seller, l.price);
    }
}
