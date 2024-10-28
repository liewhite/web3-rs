use alloy::sol;

sol! {
    #[derive(Debug, PartialEq)]
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
        function approve(address spender, uint256 amount) external returns (bool);
        function balanceOf(address addr) external returns (uint256);
        function decimals() external returns (uint256);
        function symbol() external returns (string);
    }
}
