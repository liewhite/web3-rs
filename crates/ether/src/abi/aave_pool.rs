use alloy::sol;


sol! {
    #[derive(Debug, PartialEq)]
    interface IPool {
        function supply(address asset, uint256 amount, address onBehalfOf, uint16 referralCode) external;
    }

}
