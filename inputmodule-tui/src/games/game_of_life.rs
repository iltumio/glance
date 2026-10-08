/// Game of Life runs on the device firmware. This module only handles
/// the TUI display showing pattern selection and relaying commands.
///
/// The actual simulation lives in the firmware (fl16-inputmodules).
/// We send StartGame(GameOfLife, pattern) and the device runs it.
use crate::serial::protocol::GameOfLifePattern;

pub const PATTERNS: &[(GameOfLifePattern, &str)] = &[
    (GameOfLifePattern::CurrentMatrix, "Current Matrix"),
    (GameOfLifePattern::Pattern1, "Pattern 1"),
    (GameOfLifePattern::Blinker, "Blinker"),
    (GameOfLifePattern::Toad, "Toad"),
    (GameOfLifePattern::Beacon, "Beacon"),
    (GameOfLifePattern::Glider, "Glider"),
    (
        GameOfLifePattern::BeaconToadBlinker,
        "Beacon + Toad + Blinker",
    ),
];
