use std::io;

pub const FWK_MAGIC: &[u8] = &[0x32, 0xAC];
pub const FRAMEWORK_VID: u16 = 0x32AC;
pub const LED_MATRIX_PID: u16 = 0x0020;
pub const B1_LCD_PID: u16 = 0x0021;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    Brightness = 0x00,
    Pattern = 0x01,
    Bootloader = 0x02,
    Sleeping = 0x03,
    Animate = 0x04,
    Panic = 0x05,
    DisplayBwImage = 0x06,
    SendCol = 0x07,
    CommitCols = 0x08,
    StartGame = 0x10,
    GameControl = 0x11,
    GameStatus = 0x12,
    SetColor = 0x13,
    DisplayOn = 0x14,
    InvertScreen = 0x15,
    SetPixelColumn = 0x16,
    FlushFramebuffer = 0x17,
    ClearRam = 0x18,
    ScreenSaver = 0x19,
    Fps = 0x1A,
    PowerMode = 0x1B,
    AnimationPeriod = 0x1C,
    PwmFreq = 0x1E,
    DebugMode = 0x1F,
    Version = 0x20,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GameControlArg {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
    Exit = 4,
    SecondLeft = 5,
    SecondRight = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[repr(u8)]
pub enum GameId {
    Snake = 0,
    Pong = 1,
    Tetris = 2,
    GameOfLife = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[repr(u8)]
pub enum GameOfLifePattern {
    CurrentMatrix = 0,
    Pattern1 = 1,
    Blinker = 2,
    Toad = 3,
    Beacon = 4,
    Glider = 5,
    BeaconToadBlinker = 6,
}

/// Each command must fit into one 64-byte USB CDC packet.
pub fn encode_command(command: Command, args: &[u8]) -> io::Result<Vec<u8>> {
    if args.len() > 61 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Command exceeds 64 bytes",
        ));
    }
    let mut bytes = Vec::with_capacity(3 + args.len());
    bytes.extend_from_slice(FWK_MAGIC);
    bytes.push(command as u8);
    bytes.extend_from_slice(args);
    Ok(bytes)
}

pub fn decode_version(response: &[u8; 32]) -> String {
    let mut version = format!("{}.{}.{}", response[0], response[1] >> 4, response[1] & 15);
    if response[2] == 1 {
        version.push_str(" (Pre-Release)");
    }
    version
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_match_firmware_wire_format() {
        assert_eq!(
            encode_command(
                Command::StartGame,
                &[GameId::GameOfLife as u8, GameOfLifePattern::Glider as u8]
            )
            .unwrap(),
            [0x32, 0xac, 0x10, 3, 5]
        );
        assert_eq!(
            encode_command(Command::GameControl, &[GameControlArg::Exit as u8]).unwrap(),
            [0x32, 0xac, 0x11, 4]
        );
        assert_eq!(
            encode_command(Command::SendCol, &[0; 61]).unwrap().len(),
            64
        );
        assert_eq!(
            encode_command(Command::SendCol, &[0; 62])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
