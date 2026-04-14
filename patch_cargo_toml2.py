with open('Cargo.toml', 'r') as f:
    content = f.read()

content = content.replace('rand = "0.9.0"', 'rand = "0.8.5"')

with open('Cargo.toml', 'w') as f:
    f.write(content)
