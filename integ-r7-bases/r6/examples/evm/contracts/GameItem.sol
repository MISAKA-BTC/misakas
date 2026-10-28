// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title GameItem — a minimal ERC-721 in-game item (character / skin / NFT).
/// @notice Self-contained for `forge create`; production should use OpenZeppelin
///         `ERC721`(+`ERC721URIStorage`/`ERC2981` royalties). evmVersion = "shanghai".
contract GameItem {
    string public name = "MISAKA Game Item";
    string public symbol = "MGI";
    address public owner;
    uint256 public nextId;

    mapping(uint256 => address) public ownerOf;
    mapping(address => uint256) public balanceOf;
    mapping(uint256 => address) public getApproved;
    mapping(address => mapping(address => bool)) public isApprovedForAll;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);
    event ApprovalForAll(address indexed owner, address indexed operator, bool approved);

    constructor() {
        owner = msg.sender;
    }

    /// @notice Mint a new item to `to` (owner only). Returns the new token id.
    function mint(address to) external returns (uint256 id) {
        require(msg.sender == owner, "GameItem: not owner");
        require(to != address(0), "GameItem: to zero");
        id = ++nextId;
        ownerOf[id] = to;
        balanceOf[to] += 1;
        emit Transfer(address(0), to, id);
    }

    function approve(address to, uint256 id) external {
        address holder = ownerOf[id];
        require(msg.sender == holder || isApprovedForAll[holder][msg.sender], "GameItem: not authorized");
        getApproved[id] = to;
        emit Approval(holder, to, id);
    }

    function setApprovalForAll(address operator, bool approved) external {
        isApprovedForAll[msg.sender][operator] = approved;
        emit ApprovalForAll(msg.sender, operator, approved);
    }

    function transferFrom(address from, address to, uint256 id) public {
        require(ownerOf[id] == from, "GameItem: wrong from");
        require(to != address(0), "GameItem: to zero");
        require(
            msg.sender == from || getApproved[id] == msg.sender || isApprovedForAll[from][msg.sender],
            "GameItem: not authorized"
        );
        ownerOf[id] = to;
        unchecked {
            balanceOf[from] -= 1;
            balanceOf[to] += 1;
        }
        delete getApproved[id];
        emit Transfer(from, to, id);
    }
}
