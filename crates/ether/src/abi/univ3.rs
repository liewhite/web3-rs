use alloy::sol;

sol! {
    #[derive(Debug, PartialEq)]
    interface IUniswapV3PoolActions {
        /// @notice Sets the initial price for the pool
        /// @dev Price is represented as a sqrt(amountToken1/amountToken0) Q64.96 value
        /// @param sqrtPriceX96 the initial sqrt price of the pool as a Q64.96
        function initialize(uint160 sqrtPriceX96) external;

        /// @notice Adds liquidity for the given recipient/tickLower/tickUpper position
        /// @dev The caller of this method receives a callback in the form of IUniswapV3MintCallback#uniswapV3MintCallback
        /// in which they must pay any token0 or token1 owed for the liquidity. The amount of token0/token1 due depends
        /// on tickLower, tickUpper, the amount of liquidity, and the current price.
        /// @param recipient The address for which the liquidity will be created
        /// @param tickLower The lower tick of the position in which to add liquidity
        /// @param tickUpper The upper tick of the position in which to add liquidity
        /// @param amount The amount of liquidity to mint
        /// @param data Any data that should be passed through to the callback
        /// @return amount0 The amount of token0 that was paid to mint the given amount of liquidity. Matches the value in the callback
        /// @return amount1 The amount of token1 that was paid to mint the given amount of liquidity. Matches the value in the callback
        function mint(
            address recipient,
            int24 tickLower,
            int24 tickUpper,
            uint128 amount,
            bytes calldata data
        ) external returns (uint256 amount0, uint256 amount1);

        /// @notice Collects tokens owed to a position
        /// @dev Does not recompute fees earned, which must be done either via mint or burn of any amount of liquidity.
        /// Collect must be called by the position owner. To withdraw only token0 or only token1, amount0Requested or
        /// amount1Requested may be set to zero. To withdraw all tokens owed, caller may pass any value greater than the
        /// actual tokens owed, e.g. type(uint128).max. Tokens owed may be from accumulated swap fees or burned liquidity.
        /// @param recipient The address which should receive the fees collected
        /// @param tickLower The lower tick of the position for which to collect fees
        /// @param tickUpper The upper tick of the position for which to collect fees
        /// @param amount0Requested How much token0 should be withdrawn from the fees owed
        /// @param amount1Requested How much token1 should be withdrawn from the fees owed
        /// @return amount0 The amount of fees collected in token0
        /// @return amount1 The amount of fees collected in token1
        function collect(
            address recipient,
            int24 tickLower,
            int24 tickUpper,
            uint128 amount0Requested,
            uint128 amount1Requested
        ) external returns (uint128 amount0, uint128 amount1);

        /// @notice Burn liquidity from the sender and account tokens owed for the liquidity to the position
        /// @dev Can be used to trigger a recalculation of fees owed to a position by calling with an amount of 0
        /// @dev Fees must be collected separately via a call to #collect
        /// @param tickLower The lower tick of the position for which to burn liquidity
        /// @param tickUpper The upper tick of the position for which to burn liquidity
        /// @param amount How much liquidity to burn
        /// @return amount0 The amount of token0 sent to the recipient
        /// @return amount1 The amount of token1 sent to the recipient
        function burn(
            int24 tickLower,
            int24 tickUpper,
            uint128 amount
        ) external returns (uint256 amount0, uint256 amount1);

        /// @notice Swap token0 for token1, or token1 for token0
        /// @dev The caller of this method receives a callback in the form of IUniswapV3SwapCallback#uniswapV3SwapCallback
        /// @param recipient The address to receive the output of the swap
        /// @param zeroForOne The direction of the swap, true for token0 to token1, false for token1 to token0
        /// @param amountSpecified The amount of the swap, which implicitly configures the swap as exact input (positive), or exact output (negative)
        /// @param sqrtPriceLimitX96 The Q64.96 sqrt price limit. If zero for one, the price cannot be less than this
        /// value after the swap. If one for zero, the price cannot be greater than this value after the swap
        /// @param data Any data to be passed through to the callback
        /// @return amount0 The delta of the balance of token0 of the pool, exact when negative, minimum when positive
        /// @return amount1 The delta of the balance of token1 of the pool, exact when negative, minimum when positive
        function swap(
            address recipient,
            bool zeroForOne,
            int256 amountSpecified,
            uint160 sqrtPriceLimitX96,
            bytes calldata data
        ) external returns (int256 amount0, int256 amount1);

        /// @notice Receive token0 and/or token1 and pay it back, plus a fee, in the callback
        /// @dev The caller of this method receives a callback in the form of IUniswapV3FlashCallback#uniswapV3FlashCallback
        /// @dev Can be used to donate underlying tokens pro-rata to currently in-range liquidity providers by calling
        /// with 0 amount{0,1} and sending the donation amount(s) from the callback
        /// @param recipient The address which will receive the token0 and token1 amounts
        /// @param amount0 The amount of token0 to send
        /// @param amount1 The amount of token1 to send
        /// @param data Any data to be passed through to the callback
        function flash(
            address recipient,
            uint256 amount0,
            uint256 amount1,
            bytes calldata data
        ) external;

        /// @notice Increase the maximum number of price and liquidity observations that this pool will store
        /// @dev This method is no-op if the pool already has an observationCardinalityNext greater than or equal to
        /// the input observationCardinalityNext.
        /// @param observationCardinalityNext The desired minimum number of observations for the pool to store
        function increaseObservationCardinalityNext(uint16 observationCardinalityNext) external;
    }
}

sol! {
    interface IUniswapV3SwapCallback {
        /// @notice Called to `msg.sender` after executing a swap via IUniswapV3Pool#swap.
        /// @dev In the implementation you must pay the pool tokens owed for the swap.
        /// The caller of this method must be checked to be a UniswapV3Pool deployed by the canonical UniswapV3Factory.
        /// amount0Delta and amount1Delta can both be 0 if no tokens were swapped.
        /// @param amount0Delta The amount of token0 that was sent (negative) or must be received (positive) by the pool by
        /// the end of the swap. If positive, the callback must send that amount of token0 to the pool.
        /// @param amount1Delta The amount of token1 that was sent (negative) or must be received (positive) by the pool by
        /// the end of the swap. If positive, the callback must send that amount of token1 to the pool.
        /// @param data Any data passed through by the caller via the IUniswapV3PoolActions#swap call
        function uniswapV3SwapCallback(
            int256 amount0Delta,
            int256 amount1Delta,
            bytes calldata data
        ) external;
    }
}

sol! {
    #[derive(Debug, PartialEq)]
    interface IUniswapV3PoolEvents {
        /// @notice Emitted exactly once by a pool when #initialize is first called on the pool
        /// @dev Mint/Burn/Swap cannot be emitted by the pool before Initialize
        /// @param sqrtPriceX96 The initial sqrt price of the pool, as a Q64.96
        /// @param tick The initial tick of the pool, i.e. log base 1.0001 of the starting price of the pool
        event Initialize(uint160 sqrtPriceX96, int24 tick);
    
        /// @notice Emitted when liquidity is minted for a given position
        /// @param sender The address that minted the liquidity
        /// @param owner The owner of the position and recipient of any minted liquidity
        /// @param tickLower The lower tick of the position
        /// @param tickUpper The upper tick of the position
        /// @param amount The amount of liquidity minted to the position range
        /// @param amount0 How much token0 was required for the minted liquidity
        /// @param amount1 How much token1 was required for the minted liquidity
        event Mint(
            address sender,
            address indexed owner,
            int24 indexed tickLower,
            int24 indexed tickUpper,
            uint128 amount,
            uint256 amount0,
            uint256 amount1
        );
    
        /// @notice Emitted when fees are collected by the owner of a position
        /// @dev Collect events may be emitted with zero amount0 and amount1 when the caller chooses not to collect fees
        /// @param owner The owner of the position for which fees are collected
        /// @param tickLower The lower tick of the position
        /// @param tickUpper The upper tick of the position
        /// @param amount0 The amount of token0 fees collected
        /// @param amount1 The amount of token1 fees collected
        event Collect(
            address indexed owner,
            address recipient,
            int24 indexed tickLower,
            int24 indexed tickUpper,
            uint128 amount0,
            uint128 amount1
        );
    
        /// @notice Emitted when a position's liquidity is removed
        /// @dev Does not withdraw any fees earned by the liquidity position, which must be withdrawn via #collect
        /// @param owner The owner of the position for which liquidity is removed
        /// @param tickLower The lower tick of the position
        /// @param tickUpper The upper tick of the position
        /// @param amount The amount of liquidity to remove
        /// @param amount0 The amount of token0 withdrawn
        /// @param amount1 The amount of token1 withdrawn
        event Burn(
            address indexed owner,
            int24 indexed tickLower,
            int24 indexed tickUpper,
            uint128 amount,
            uint256 amount0,
            uint256 amount1
        );
    
        /// @notice Emitted by the pool for any swaps between token0 and token1
        /// @param sender The address that initiated the swap call, and that received the callback
        /// @param recipient The address that received the output of the swap
        /// @param amount0 The delta of the token0 balance of the pool
        /// @param amount1 The delta of the token1 balance of the pool
        /// @param sqrtPriceX96 The sqrt(price) of the pool after the swap, as a Q64.96
        /// @param liquidity The liquidity of the pool after the swap
        /// @param tick The log base 1.0001 of price of the pool after the swap
        event Swap(
            address indexed sender,
            address indexed recipient,
            int256 amount0,
            int256 amount1,
            uint160 sqrtPriceX96,
            uint128 liquidity,
            int24 tick
        );
    
        /// @notice Emitted by the pool for any flashes of token0/token1
        /// @param sender The address that initiated the swap call, and that received the callback
        /// @param recipient The address that received the tokens from flash
        /// @param amount0 The amount of token0 that was flashed
        /// @param amount1 The amount of token1 that was flashed
        /// @param paid0 The amount of token0 paid for the flash, which can exceed the amount0 plus the fee
        /// @param paid1 The amount of token1 paid for the flash, which can exceed the amount1 plus the fee
        event Flash(
            address indexed sender,
            address indexed recipient,
            uint256 amount0,
            uint256 amount1,
            uint256 paid0,
            uint256 paid1
        );
    
        /// @notice Emitted by the pool for increases to the number of observations that can be stored
        /// @dev observationCardinalityNext is not the observation cardinality until an observation is written at the index
        /// just before a mint/swap/burn.
        /// @param observationCardinalityNextOld The previous value of the next observation cardinality
        /// @param observationCardinalityNextNew The updated value of the next observation cardinality
        event IncreaseObservationCardinalityNext(
            uint16 observationCardinalityNextOld,
            uint16 observationCardinalityNextNew
        );
    
        /// @notice Emitted when the protocol fee is changed by the pool
        /// @param feeProtocol0Old The previous value of the token0 protocol fee
        /// @param feeProtocol1Old The previous value of the token1 protocol fee
        /// @param feeProtocol0New The updated value of the token0 protocol fee
        /// @param feeProtocol1New The updated value of the token1 protocol fee
        event SetFeeProtocol(uint8 feeProtocol0Old, uint8 feeProtocol1Old, uint8 feeProtocol0New, uint8 feeProtocol1New);
    
        /// @notice Emitted when the collected protocol fees are withdrawn by the factory owner
        /// @param sender The address that collects the protocol fees
        /// @param recipient The address that receives the collected protocol fees
        /// @param amount0 The amount of token0 protocol fees that is withdrawn
        /// @param amount0 The amount of token1 protocol fees that is withdrawn
        event CollectProtocol(address indexed sender, address indexed recipient, uint128 amount0, uint128 amount1);
    }
}

sol!{
    #[derive(Debug, PartialEq)]
    interface IUniswapV3Factory {
        /// @notice Emitted when the owner of the factory is changed
        /// @param oldOwner The owner before the owner was changed
        /// @param newOwner The owner after the owner was changed
        event OwnerChanged(address indexed oldOwner, address indexed newOwner);
    
        /// @notice Emitted when a pool is created
        /// @param token0 The first token of the pool by address sort order
        /// @param token1 The second token of the pool by address sort order
        /// @param fee The fee collected upon every swap in the pool, denominated in hundredths of a bip
        /// @param tickSpacing The minimum number of ticks between initialized ticks
        /// @param pool The address of the created pool
        event PoolCreated(
            address indexed token0,
            address indexed token1,
            uint24 indexed fee,
            int24 tickSpacing,
            address pool
        );
    
        /// @notice Emitted when a new fee amount is enabled for pool creation via the factory
        /// @param fee The enabled fee, denominated in hundredths of a bip
        /// @param tickSpacing The minimum number of ticks between initialized ticks for pools created with the given fee
        event FeeAmountEnabled(uint24 indexed fee, int24 indexed tickSpacing);
    
        /// @notice Returns the current owner of the factory
        /// @dev Can be changed by the current owner via setOwner
        /// @return The address of the factory owner
        function owner() external view returns (address);
    
        /// @notice Returns the tick spacing for a given fee amount, if enabled, or 0 if not enabled
        /// @dev A fee amount can never be removed, so this value should be hard coded or cached in the calling context
        /// @param fee The enabled fee, denominated in hundredths of a bip. Returns 0 in case of unenabled fee
        /// @return The tick spacing
        function feeAmountTickSpacing(uint24 fee) external view returns (int24);
    
        /// @notice Returns the pool address for a given pair of tokens and a fee, or address 0 if it does not exist
        /// @dev tokenA and tokenB may be passed in either token0/token1 or token1/token0 order
        /// @param tokenA The contract address of either token0 or token1
        /// @param tokenB The contract address of the other token
        /// @param fee The fee collected upon every swap in the pool, denominated in hundredths of a bip
        /// @return pool The pool address
        function getPool(
            address tokenA,
            address tokenB,
            uint24 fee
        ) external view returns (address pool);
    
        /// @notice Creates a pool for the given two tokens and fee
        /// @param tokenA One of the two tokens in the desired pool
        /// @param tokenB The other of the two tokens in the desired pool
        /// @param fee The desired fee for the pool
        /// @dev tokenA and tokenB may be passed in either order: token0/token1 or token1/token0. tickSpacing is retrieved
        /// from the fee. The call will revert if the pool already exists, the fee is invalid, or the token arguments
        /// are invalid.
        /// @return pool The address of the newly created pool
        function createPool(
            address tokenA,
            address tokenB,
            uint24 fee
        ) external returns (address pool);
    
        /// @notice Updates the owner of the factory
        /// @dev Must be called by the current owner
        /// @param _owner The new owner of the factory
        function setOwner(address _owner) external;
    
        /// @notice Enables a fee amount with the given tickSpacing
        /// @dev Fee amounts may never be removed once enabled
        /// @param fee The fee amount to enable, denominated in hundredths of a bip (i.e. 1e-6)
        /// @param tickSpacing The spacing between ticks to be enforced for all pools created with the given fee amount
        function enableFeeAmount(uint24 fee, int24 tickSpacing) external;
    }
}

sol!{
    #[derive(Debug, PartialEq)]
    interface IUniswapV3PoolState {
        /// @notice The 0th storage slot in the pool stores many values, and is exposed as a single method to save gas
        /// when accessed externally.
        /// @return sqrtPriceX96 The current price of the pool as a sqrt(token1/token0) Q64.96 value
        /// tick The current tick of the pool, i.e. according to the last tick transition that was run.
        /// This value may not always be equal to SqrtTickMath.getTickAtSqrtRatio(sqrtPriceX96) if the price is on a tick
        /// boundary.
        /// observationIndex The index of the last oracle observation that was written,
        /// observationCardinality The current maximum number of observations stored in the pool,
        /// observationCardinalityNext The next maximum number of observations, to be updated when the observation.
        /// feeProtocol The protocol fee for both tokens of the pool.
        /// Encoded as two 4 bit values, where the protocol fee of token1 is shifted 4 bits and the protocol fee of token0
        /// is the lower 4 bits. Used as the denominator of a fraction of the swap fee, e.g. 4 means 1/4th of the swap fee.
        /// unlocked Whether the pool is currently locked to reentrancy
        function slot0()
            external
            view
            returns (
                uint160 sqrtPriceX96,
                int24 tick,
                uint16 observationIndex,
                uint16 observationCardinality,
                uint16 observationCardinalityNext,
                uint8 feeProtocol,
                bool unlocked
            );
    
        /// @notice The fee growth as a Q128.128 fees of token0 collected per unit of liquidity for the entire life of the pool
        /// @dev This value can overflow the uint256
        function feeGrowthGlobal0X128() external view returns (uint256);
    
        /// @notice The fee growth as a Q128.128 fees of token1 collected per unit of liquidity for the entire life of the pool
        /// @dev This value can overflow the uint256
        function feeGrowthGlobal1X128() external view returns (uint256);
    
        /// @notice The amounts of token0 and token1 that are owed to the protocol
        /// @dev Protocol fees will never exceed uint128 max in either token
        function protocolFees() external view returns (uint128 token0, uint128 token1);
    
        /// @notice The currently in range liquidity available to the pool
        /// @dev This value has no relationship to the total liquidity across all ticks
        function liquidity() external view returns (uint128);
    
        /// @notice Look up information about a specific tick in the pool
        /// @param tick The tick to look up
        /// @return liquidityGross the total amount of position liquidity that uses the pool either as tick lower or
        /// tick upper,
        /// liquidityNet how much liquidity changes when the pool price crosses the tick,
        /// feeGrowthOutside0X128 the fee growth on the other side of the tick from the current tick in token0,
        /// feeGrowthOutside1X128 the fee growth on the other side of the tick from the current tick in token1,
        /// tickCumulativeOutside the cumulative tick value on the other side of the tick from the current tick
        /// secondsPerLiquidityOutsideX128 the seconds spent per liquidity on the other side of the tick from the current tick,
        /// secondsOutside the seconds spent on the other side of the tick from the current tick,
        /// initialized Set to true if the tick is initialized, i.e. liquidityGross is greater than 0, otherwise equal to false.
        /// Outside values can only be used if the tick is initialized, i.e. if liquidityGross is greater than 0.
        /// In addition, these values are only relative and must be used only in comparison to previous snapshots for
        /// a specific position.
        function ticks(int24 tick)
            external
            view
            returns (
                uint128 liquidityGross,
                int128 liquidityNet,
                uint256 feeGrowthOutside0X128,
                uint256 feeGrowthOutside1X128,
                int56 tickCumulativeOutside,
                uint160 secondsPerLiquidityOutsideX128,
                uint32 secondsOutside,
                bool initialized
            );
    
        /// @notice Returns 256 packed tick initialized boolean values. See TickBitmap for more information
        function tickBitmap(int16 wordPosition) external view returns (uint256);
    
        /// @notice Returns the information about a position by the position's key
        /// @param key The position's key is a hash of a preimage composed by the owner, tickLower and tickUpper
        /// @return _liquidity The amount of liquidity in the position,
        /// Returns feeGrowthInside0LastX128 fee growth of token0 inside the tick range as of the last mint/burn/poke,
        /// Returns feeGrowthInside1LastX128 fee growth of token1 inside the tick range as of the last mint/burn/poke,
        /// Returns tokensOwed0 the computed amount of token0 owed to the position as of the last mint/burn/poke,
        /// Returns tokensOwed1 the computed amount of token1 owed to the position as of the last mint/burn/poke
        function positions(bytes32 key)
            external
            view
            returns (
                uint128 _liquidity,
                uint256 feeGrowthInside0LastX128,
                uint256 feeGrowthInside1LastX128,
                uint128 tokensOwed0,
                uint128 tokensOwed1
            );
    
        /// @notice Returns data about a specific observation index
        /// @param index The element of the observations array to fetch
        /// @dev You most likely want to use #observe() instead of this method to get an observation as of some amount of time
        /// ago, rather than at a specific index in the array.
        /// @return blockTimestamp The timestamp of the observation,
        /// Returns tickCumulative the tick multiplied by seconds elapsed for the life of the pool as of the observation timestamp,
        /// Returns secondsPerLiquidityCumulativeX128 the seconds per in range liquidity for the life of the pool as of the observation timestamp,
        /// Returns initialized whether the observation has been initialized and the values are safe to use
        function observations(uint256 index)
            external
            view
            returns (
                uint32 blockTimestamp,
                int56 tickCumulative,
                uint160 secondsPerLiquidityCumulativeX128,
                bool initialized
            );
    }
}