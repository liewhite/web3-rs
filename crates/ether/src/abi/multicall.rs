use alloy::sol;

sol! {
    #[derive(Debug, PartialEq)]
    interface Multicall {
        function aggregate(address token_a,address token_b,address[] pools) external returns (uint256[]);
    }
}
