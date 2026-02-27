import json
import subprocess

def main():
    output = subprocess.check_output(["cargo", "metadata", "--format-version", "1"])
    metadata = json.loads(output)

    for pkg in metadata["packages"]:
        lic = pkg.get("license", "")
        if lic and "LGPL" in lic:
            print(f"{pkg['name']} {pkg['version']}: {lic}")

if __name__ == "__main__":
    main()
