use alloy::sol;

sol! {

    #[derive(Debug, PartialEq)]
    struct CallData {
        uint256 flag; // 0x1 delegate call, 0x0 call.
        address to;
        uint256 value;
        bytes data; // calldata
        bytes hint;
        bytes extra; // for future support: signatures etc.
    }
    #[derive(Debug, PartialEq)]
    struct TransactionResult {
        bool success; // Call status.
        bytes data; // Return/Revert data.
        bytes hint;
    }

    #[derive(Debug, PartialEq)]
    interface IAccount {
        function execTransaction(CallData calldata callData) external returns (TransactionResult memory result);
    }
}
