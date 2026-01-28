use crate::protocol::dacp::commands::DacpCommand;

#[test]
fn test_command_from_path() {
    assert_eq!(
        DacpCommand::from_path("/ctrl-int/1/play"),
        Some(DacpCommand::Play)
    );
    assert_eq!(
        DacpCommand::from_path("/ctrl-int/1/playpause"),
        Some(DacpCommand::PlayPause)
    );
    assert_eq!(
        DacpCommand::from_path("/ctrl-int/1/nextitem"),
        Some(DacpCommand::NextItem)
    );
    assert_eq!(DacpCommand::from_path("/invalid"), None);
    assert_eq!(DacpCommand::from_path("/ctrl-int/1/unknown"), None);
}

#[test]
fn test_command_path_roundtrip() {
    let commands = [
        DacpCommand::Play,
        DacpCommand::Pause,
        DacpCommand::PlayPause,
        DacpCommand::PlayResume,
        // DacpCommand::PlayResume2 is skipped here because it maps to same path as PlayResume
        DacpCommand::Stop,
        DacpCommand::NextItem,
        DacpCommand::PrevItem,
        DacpCommand::BeginFastForward,
        DacpCommand::BeginRewind,
        DacpCommand::VolumeUp,
        DacpCommand::VolumeDown,
        DacpCommand::MuteToggle,
        DacpCommand::ShuffleSongs,
    ];

    for cmd in commands {
        let path = cmd.path();
        let parsed = DacpCommand::from_path(path);
        assert_eq!(parsed, Some(cmd), "Failed to roundtrip {:?}", cmd);
    }
}

#[test]
fn test_playresume2_path() {
    // Special case for PlayResume2 which maps to same path as PlayResume
    let cmd = DacpCommand::PlayResume2;
    let path = cmd.path();
    let parsed = DacpCommand::from_path(path);
    assert_eq!(parsed, Some(DacpCommand::PlayResume));
}

#[test]
fn test_invalid_paths() {
    // Wrong prefix
    assert_eq!(DacpCommand::from_path("/ctrl-int/2/play"), None);
    assert_eq!(DacpCommand::from_path("/api/1/play"), None);

    // Malformed
    assert_eq!(DacpCommand::from_path("play"), None);
    assert_eq!(DacpCommand::from_path(""), None);

    // Unknown command
    assert_eq!(DacpCommand::from_path("/ctrl-int/1/jump"), None);
    assert_eq!(DacpCommand::from_path("/ctrl-int/1/"), None);
}

#[test]
fn test_descriptions() {
    // Just verify that all commands have non-empty descriptions
    let commands = [
        DacpCommand::Play,
        DacpCommand::Pause,
        DacpCommand::PlayPause,
        DacpCommand::PlayResume,
        DacpCommand::PlayResume2,
        DacpCommand::Stop,
        DacpCommand::NextItem,
        DacpCommand::PrevItem,
        DacpCommand::BeginFastForward,
        DacpCommand::BeginRewind,
        DacpCommand::VolumeUp,
        DacpCommand::VolumeDown,
        DacpCommand::MuteToggle,
        DacpCommand::ShuffleSongs,
    ];

    for cmd in commands {
        assert!(
            !cmd.description().is_empty(),
            "Description for {:?} is empty",
            cmd
        );
    }
}
