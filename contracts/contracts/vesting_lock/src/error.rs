#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Error {
    // CKB syscall errors
    IndexOutOfBound = 1,
    ItemMissing = 2,
    LengthNotEnough = 3,
    InvalidData = 4,

    // Script-specific errors
    InvalidArgs = 10,
    InvalidWitness = 11,
    InvalidTransaction = 12,
    InvalidTransactionStructure = 13,
    TotalAmountChanged = 14,
    InvalidBeneficiaryClaimedDelta = 15,
    InvalidCreatorClaimedDelta = 16,
    InvalidStateChange = 17,

    // Vesting logic errors
    InvalidAmount = 20,
    InsufficientVested = 21,
    AlreadyTerminated = 22,
    InvalidEpoch = 23,
    StaleHeader = 24,
    Unauthorized = 25,
    BlockNumberDecrease = 26,
    BlockNumberMismatch = 27,

    // Encoding errors
    InvalidCellData = 30, // Deprecated - use specific errors below
    LoadCellDataFailed = 31,
    WrongDataLength = 32,
    NoMatchingInputCell = 33,
    NoMatchingOutputCell = 34,
    NoHeaderDependencies = 35,

    // Transaction structure errors
    MultipleInputsNotAllowed = 36,
    CreatorOperationMissingOutput = 37,
    AnonymousUpdateMissingOutput = 38,
    InputDataWrongLength = 39,
    OutputDataWrongLength = 40,
    CreatorFullTerminationHasOutput = 41,
    BeneficiaryFullClaimHasOutput = 42,
    BeneficiaryPartialClaimMissingOutput = 43,
    NothingToTerminate = 44,
}

impl From<ckb_std::error::SysError> for Error {
    fn from(err: ckb_std::error::SysError) -> Self {
        use ckb_std::error::SysError;
        match err {
            SysError::IndexOutOfBound => Error::IndexOutOfBound,
            SysError::ItemMissing => Error::ItemMissing,
            SysError::LengthNotEnough(_) => Error::LengthNotEnough,
            SysError::Encoding => Error::InvalidData,
            SysError::Unknown(_) => Error::InvalidData,
            _ => Error::InvalidData,
        }
    }
}
