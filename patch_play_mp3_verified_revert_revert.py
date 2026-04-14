with open('examples/play_mp3_verified.rs', 'r') as f:
    content = f.read()

content = content.replace(
    '    let mut player = AirPlayPlayer::new(); // Note: must be mut for earlier rustc or we ignore',
    '    #[allow(unused_mut, reason = "Player is required to be mutable for some features or future usage")]\n    let mut player = AirPlayPlayer::new();'
)

with open('examples/play_mp3_verified.rs', 'w') as f:
    f.write(content)
