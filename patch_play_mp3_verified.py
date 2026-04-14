with open('examples/play_mp3_verified.rs', 'r') as f:
    content = f.read()

content = content.replace(
    '    #[allow(unused_mut)]\n    let mut player = AirPlayPlayer::new();',
    '    let player = AirPlayPlayer::new();'
)

with open('examples/play_mp3_verified.rs', 'w') as f:
    f.write(content)
