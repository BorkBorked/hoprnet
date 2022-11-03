# HOPR Ethereum Package

Draft readme, for rust migration

## Installation

1. `rustup`
2. `foundryup`
3. `brew install lcov` (to install lcov for viewing coverage report)

## Contracts

```
cd contracts
```

### Test

```
make sc-test
```

### Run Coverage

```
make sc-coverage
```

### Deployment
#### Local

```
anvil
make anvil-deploy-erc1820
FOUNDRY_PROFILE=development ENVIRONMENT_NAME=localhost forge script script/HoprToken.s.sol:DeployHoprTokenScript --broadcast
```

### Note

1. Three solc versions are needed

- 0.4: Permittable token, implementation of xHOPR. The permittable token implementation is extracted from the deployed xHOPR token. The only alternative done on the contract is to keep `pragma solidity` with the least version
- 0.6: Deployed Hoprtoken
- 0.8: More recent contracts

2. Dependencies are vendored directly into the repo. Some of them are locked to a specific version

```
forge install foundry-rs/forge-std \
openzeppelin-contracts=OpenZeppelin/openzeppelin-contracts@v4.4.2 \
openzeppelin-contracts-v3-0-1=OpenZeppelin/openzeppelin-contracts@v3.0.1 \
--no-git --no-commit
```

3. If `forge coverage` is not found in as a command, update `foundryup` to the [latest nightly release](https://github.com/foundry-rs/foundry/releases) may solve the issue.

4. `forge coverage` may run into `Error: Function has no kind` when compiler has multiple versions. Opened an issue https://github.com/foundry-rs/foundry/issues/3519

<!-- 5. To move faster on the migration of toolchain and to avoid constantly running into foundry's error during the compilation `Error: Discovered incompatible solidity versions in following`, the compiler version of the following contracts have be moved to `pragma solidity >=0.6.0 <0.9.0;`
- src/HoprToken.sol (^0.6.0)
- src/ERC777/ERC777Snapshot.sol (^0.6.0)
- src/openzeppelin-contracts/ERC777.sol (>=0.6.0 <0.8.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/access/AccessControl.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/GSN/Context.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/introspection/IERC1820Registry.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/math/SafeMath.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/token/ERC20/IERC20.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/token/ERC777/IERC777.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/token/ERC777/IERC777Recipient.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/token/ERC777/IERC777Sender.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/utils/EnumerableSet.sol (^0.6.0)
- lib/openzeppelin-contracts-v3-0-1/contracts/utils/Address.sol (^0.6.2) -->

6. Remove "PermittableToken.sol" from source code as it prevents coverage engine from working. Possibly because its required compiler version is 0.4.x This contract is only used when testing "HoprWrapper" contract. TODO: use a different approach to test "HoprWrapper"
7. Moved `src/mock` to `test/mock` folder, and adapt the relative path used in "HoprWhitehat.sol"
8. To move faster on the rest of toolchain upgrade, only tests for "HoprToken" contract is fully migrated. Tests for "HoprChannels" is halfway through. TODO: complete tests for the following contracts:

```
|____stake
| |____HoprStake.t.sol
| |____HoprStake2.t.sol
| |____HoprStakeSeason3.t.sol
| |____HoprStakeSeason4.t.sol
| |____HoprStakeSeason5.t.sol
| |____HoprStakeBase.t.sol
| |____HoprBoost.t.sol
| |____HoprWhitehat.t.sol
|____proxy
| |____HoprStakingProxyForNetworkRegistry.t.sol
| |____HoprDummyProxyForNetworkRegistry.t.sol
|____ERC777
| |____ERC777Snapshot.t.sol
|____HoprChannels.t.sol (the other half)
|____HoprDistributor.t.sol
|____HoprForwarder.t.sol
|____HoprWrapper.t.sol
|____HoprNetworkRegistry.t.sol
```

5. Temporarily skipped deployment scripts for 
 - HoprDistributor
 - HoprWrapper

6. writeJson is next inline https://github.com/foundry-rs/foundry/pull/3595, to save deployed addressed used in function `writeEnvironment()` in `contracts/script/utils/EnvironmentConfig.s.sol`