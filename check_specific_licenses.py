import json
import subprocess

def main():
    output = subprocess.check_output(["cargo", "metadata", "--format-version", "1"])
    metadata = json.loads(output)

    for pkg in metadata["packages"]:
        if pkg['name'] in ["fdk-aac", "alac-encoder", "integration_tests"]:
            print(f"{pkg['name']} {pkg['version']}: {pkg.get('license', 'N/A')}")

if __name__ == "__main__":
    main()
