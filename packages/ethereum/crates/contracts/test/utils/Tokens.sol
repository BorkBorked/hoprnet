// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.6.0 <0.9.0;

import "../../src/HoprToken.sol";
import "./Deploy.sol";
import "./Accounts.sol";
import "forge-std/Test.sol";

contract HoprTokenFixture is Test, ERC1820RegistryFixture, AccountsFixture {
    HoprToken public hoprToken;
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant DEFAULT_ADMIN_ROLE = 0x00;

    function setUp() public virtual override {
        super.setUp();
        hoprToken = new HoprToken();
        vm.prank(address(this));
        // give deployer account minter role.
        hoprToken.grantRole(MINTER_ROLE, address(this));
        // mint 10 hopr tokens for accountA and accountB
        hoprToken.mint(accountA.accountAddr, 10 ether, hex'00', hex'00');
        hoprToken.mint(accountB.accountAddr, 10 ether, hex'00', hex'00');
    }
}
