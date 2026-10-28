// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title GameToken — a minimal ERC-20 in-game fungible token for MISAKA BCG examples.
/// @notice Self-contained (no external imports) so it deploys with a plain
///         `forge create` against the MISAKA eth-rpc adapter. For production use
///         OpenZeppelin's audited `ERC20` + `AccessControl`/`Ownable` instead.
///         Compile with evmVersion = "shanghai".
contract GameToken {
    string public name = "MISAKA Game Token";
    string public symbol = "MGT";
    uint8 public constant decimals = 18;
    uint256 public totalSupply;
    address public owner;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    modifier onlyOwner() {
        require(msg.sender == owner, "GameToken: not owner");
        _;
    }

    constructor() {
        owner = msg.sender;
    }

    /// @notice Mint new tokens (owner only — your game backend / faucet).
    function mint(address to, uint256 amount) external onlyOwner {
        totalSupply += amount;
        balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 allowed = allowance[from][msg.sender];
        require(allowed >= amount, "GameToken: insufficient allowance");
        if (allowed != type(uint256).max) {
            allowance[from][msg.sender] = allowed - amount;
        }
        _transfer(from, to, amount);
        return true;
    }

    function _transfer(address from, address to, uint256 amount) internal {
        require(to != address(0), "GameToken: to zero");
        require(balanceOf[from] >= amount, "GameToken: insufficient balance");
        unchecked {
            balanceOf[from] -= amount;
            balanceOf[to] += amount;
        }
        emit Transfer(from, to, amount);
    }
}
