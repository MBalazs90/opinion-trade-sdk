/// ABI fragments for opinion.trade smart contracts on BNB Chain.
///
/// Uses alloy's `sol!` macro to generate type-safe contract bindings.
use alloy::sol;

sol! {
    /// ERC20 approve (for USDT collateral).
    #[sol(rpc)]
    contract IERC20 {
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
    }

    /// ERC1155 setApprovalForAll (for ConditionalTokens outcome tokens).
    #[sol(rpc)]
    contract IERC1155 {
        function setApprovalForAll(address operator, bool approved) external;
        function isApprovedForAll(address account, address operator) external view returns (bool);
    }

    /// Gnosis ConditionalTokens contract.
    #[sol(rpc)]
    contract IConditionalTokens {
        function splitPosition(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata partition,
            uint256 amount
        ) external;

        function mergePositions(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata partition,
            uint256 amount
        ) external;

        function redeemPositions(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata indexSets
        ) external;
    }
}
