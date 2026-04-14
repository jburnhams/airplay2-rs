with open('examples/play_mp3.rs', 'r') as f:
    content = f.read()

content = content.replace(
    '    let player = AirPlayPlayer::with_config(config);',
    '    let mut player = AirPlayPlayer::with_config(config);'
)

with open('examples/play_mp3.rs', 'w') as f:
    f.write(content)
