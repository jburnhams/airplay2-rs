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
        DacpCommand::NextItem,
        DacpCommand::VolumeUp,
    ];

    for cmd in commands {
        let path = cmd.path();
        let parsed = DacpCommand::from_path(path);
        assert_eq!(parsed, Some(cmd));
    }
}
